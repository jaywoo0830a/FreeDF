# FreeDF Sync v3 — 스냅샷 중심: "클라이언트는 ZIP만, 나머지는 서버가"

> 상태: 구현 완료 — 서버(server/backend), 프로토콜 크레이트(crates/freedf-sync), 앱 저장소(crates/freedf/src/sync_storage.rs)
> (OpenAPI 3.2.0 명세: `docs/openapi/sync-v3.openapi.yaml`)
> 전제: 대역폭 풍부(1Gbps), **서버 자원 풍부**. 최적화 대상은
> **구현 단순성과 왕복 수** — 클라이언트 로직을 최소화하고 복잡한 것은 전부 서버로.
> **최우선 불변식: 필기 중 지연 0. 동기화는 펜 경로에 절대 끼어들지 않는다(§4).**

## 1. 멘탈 모델

```
저장:  임계 초과 → 백그라운드 스레드 → Arc 스냅샷(복사 아님) → ZIP → PUT (비동기)
로딩:  GET 한 번 → 서버가 내부 쿼리 전부 조회 → ZIP 조립 → 다운로드 → 압축해제 → 렌더
싱크:  base_revision 비교 → 충돌 시 서버가 패치 계산 → 클라이언트 병합 → 재전송
```

클라이언트가 알아야 하는 것은 세 가지뿐입니다:
**ZIP 만들기, ZIP 풀기, 패치 적용하기.** 그리고 그 세 가지는 전부
**백그라운드 스레드**에서만 일어납니다.

## 2. 저장 — "때려박기"

- 임계: pen-up + M초 무입력(1순위) / N개 획 / 창 비활성화 / 앱 종료 / 명시 저장.
- 백그라운드 스레드: 문서당 진행 중 업로드 1개, dirty 플래그로 중복 방지,
  실패 시 지수 백오프 재시도. `upload_id`(UUID) 멱등 — 같은 ZIP 재전송 = no-op.
- `PUT /v3/documents/{id}/snapshot` → **202 즉시 응답** (서버 큐에서 처리).
- ZIP에는 PDF 바이트가 아니라 **CAS 다이제스트 참조**만 — PDF는 최초 1회만 전송.

## 3. 로딩 — "서버가 ZIP 만들어줌"

`GET /v3/documents/{id}` → 서버가 strokes/pages/edits/session/meta 내부 조회 → ZIP 스트리밍.
**단일 왕복**. 서버는 조립 ZIP을 **리비전별로 캐시**(이후 요청은 재조립 없음)하고,
304는 조립 **전에** 리비전 한 번으로 판정합니다 — 반복 로딩 비용은 사실상 0.
다운로드·압축해제도 백그라운드 스레드에서 하고, 렌더 준비가 끝난 상태만
UI 스레드에 전달합니다 — 로딩도 필기를 멈추지 않습니다.

## 4. 실시간성 보장 — 필기 중 0 지연 (불변 규칙)

### 4.1 핫 패스 분리

UI 스레드가 문서 상태를 **단독 소유**합니다. 획은 append-only
`Vec<Arc<Stroke>>` — pen 이벤트는 그저 push(O(1))뿐.
**프레임 안에 네트워크·디스크·압축·잠금 대기가 없습니다.**

백그라운드 업로더는 **라이브 데이터를 절대 건드리지 않습니다.**
임계 도달 시 UI 스레드가 하는 일은 `Arc` 복제(원자 카운터 증가, ~ns)와
mpsc 채널 전송 하나뿐. 압축·직렬화·PUT은 전부 업로더 스레드에서.

### 4.2 스냅샷 복제 = 포인터 복제

획과 페이지를 `Arc`로 감싸면 "전체 문서 복사"가 아니라
**참조 카운트만 올리는 O(1) 연산**입니다. 문서가 아무리 커도
스냅샷 생성 비용은 문서 크기와 무관합니다.

### 4.3 코얼레싱 — 업로드 중 더 써지면 최신 것만

업로더에 진행 중인 작업이 있을 때 새 dirty가 생기면, 완료 후
**가장 최신 스냅샷 하나만** 전송합니다. 중간 상태는 폐기.
필기 폭주에도 업로드 큐는 항상 길이 ≤ 1입니다.

### 4.4 임계의 우선순위: "펜을 든 후 M초"

필기 중에는 절대 발사하지 않습니다. 트리거는
**pen-up + M ms 무입력**이 1순위, N획/창 비활성/종료는 보조.
펜이 내려간 동안에는 업로드 시작은 물론 임계 검사도 건너뜁니다.

### 4.5 pull 패치 적용은 포인터 스왑

다중 창 동기화로 받은 패치는 백그라운드 스레드가 **복제된 상태에**
적용해 새 불변 상태를 만들고, UI 스레드는 안전 시점(펜을 든 순간 또는
다음 프레임 시작)에 `Arc` 포인터만 교체합니다. 필기 중에 긴 병합이
화면을 멈추는 일이 없습니다.

### 4.6 CPU 경합 방지

ZIP 압축(deflate)은 저레벨로, 렌더링 코어를 비켜갑니다.
1Gbps 회선에서는 압축률보다 속도가 우선이므로 `flate2`의 fast 레벨로
충분합니다. 업로더 스레드는 `std::thread`로 생성해 OS가 다른 코어에
배치하도록 맡깁니다.

## 5. 싱크의 교묘한 수

### 5.1 낙관적 동시성: base_revision

업로드 ZIP에 `base_revision`을 실어 보냅니다.

- 일치 → 적용, revision+1. 끝.
- 불일치 → **충돌. 이때가 교묘한 부분.**

### 5.2 핵심: "충돌 응답에 서버가 계산한 패치를 실어 나른다"

업로드가 어차피 **전체 스냅샷**이므로, 충돌이 발생한 순간 서버는
**양쪽 상태를 모두** 갖고 있습니다 (방금 받은 클라이언트 상태 + 서버 현재 상태).
따라서 **클라이언트는 diff 로직이 필요 없습니다.** 서버가 diff를 계산해
`conflict.patch`로 돌려줍니다:

- **획**: ID 집합 차이 → `strokes_added[]` / `stroke_ids_removed[]` 합집합 병합.
  동시 필기는 서로 다른 획이므로 **충돌 자체가 원천적으로 없음**.
- **페이지 구조**: 다르면 `pages_changed=true` + 서버 기준 전체 페이지 목록(작음).
  구조 충돌은 드물고, 드물 때만 서버 우위.
- **PDF**: 다이제스트 LWW.

클라이언트는 패치를 병합하고 `base_revision=최신`으로 재전송 → 성공.
보통 1회면 끝납니다. 병합은 §4.5처럼 백그라운드에서 일어나므로
필기 지연과 무관합니다.

### 5.3 다중 창: pull + push 알림

- `GET /changes?since_revision=` — 서버가 **변경 로그**에서 그 이후만 패치로 반환.
- `webhook(documentChanged)` — "바뀌었음"만 알리고, 내용은 클라이언트가 pull.
- 변경 로그(획 add/remove + 페이지 op 소형 로그)는 **서버 내부 구현**이므로
  클라이언트의 단순함은 그대로 유지됩니다.

## 6. API

| 메서드 | 경로 | 역할 |
|---|---|---|
| GET/POST | `/v3/documents` | 문서 목록 / 문서 생성 (PDF는 CAS 참조) |
| PUT | `/v3/documents/{id}/title` | 제목 변경 |
| GET | `/v3/documents/{id}/pdf` | 원본 PDF 다운로드 (ETag/304) |
| PUT | `/v3/documents/{id}/pdf` | 원본 PDF 업로드/교체 (멱등, revision+1) |
| DELETE | `/v3/documents/{id}` | 문서 삭제 |
| PUT | `/v3/documents/{id}/snapshot` | 전체 ZIP 업로드 (202, 멱등) |
| GET | `/v3/uploads/{uploadId}` | 적용/충돌 결과 (conflict면 패치 동봉) |
| GET | `/v3/documents/{id}` | 전체 ZIP 다운로드 (서버 조립) |
| GET | `/v3/documents/{id}/revision` | 현재 revision |
| GET | `/v3/documents/{id}/changes?since_revision=` | 변경분 pull (jsonl/zip) |
| POST | `/v3/objects/query` | CAS 보유 probe |
| PUT | `/v3/objects/{digest}` | CAS 업로드 (멱등, 본문-다이제스트 검증) |
| GET | `/v3/objects/{digest}` | CAS fetch |
| — | `webhooks.documentChanged` | push 알림 |

## 7. 문서 생명주기

1. `POST /v3/documents` — 서버가 문서를 만들고 **비어 있는 상태**로 준비
   (PDF는 CAS 다이제스트로 참조).
2. 클라이언트가 첫 스냅샷을 `PUT /v3/documents/{id}/snapshot`으로 전송
   (이 시점의 `base_revision = 0` — 새 문서는 항상 일치 → 첫 업로드는 충돌 불가).
3. 이후 스냅샷 반복(`base_revision` 낙관적 동시성, §5).
4. `PUT /v3/documents/{id}/title` — 제목 변경(경량, 스냅샷 불필요).
5. `DELETE /v3/documents/{id}` — 문서·첨부 객체 삭제.

## 8. 프로토콜 단일 소스 — freedf-sync 크레이트

모든 v3 타입·직렬화 규칙·클라이언트 HTTP 로직은 **`crates/freedf-sync`** 한 곳에만
존재합니다. 서버(server/backend)와 앱(freedf)이 같은 크레이트를 의존하므로
스키마가 어긋날 수 없습니다.

- 타입: `Snapshot`/`SnapshotMeta`/`Patch`/`ChangeRecord`/`Digest`/`UploadReceipt`/
  `DocumentInfo`/`CreateDocument`/`RenameDocument`/`ObjectInfo`/`ApiError`/`RevisionInfo`.
- 클라이언트: `SyncClient` — 서버 통신 전체(문서·스냅샷·객체·변경분).
- 명세 확장 시 순서: `sync-v3.openapi.yaml` 갱신 → `freedf-sync` 타입 추가 →
  서버/앱이 그 타입 사용. 수제 `json!` 응답은 만들지 않습니다.

## 9. 스냅샷 ZIP 레이아웃

```
snapshot.zip
├── meta.json       # revision/base_revision, page_count, updated_at,
│                   # title, kind, pdf_digest, session(GUI 세션 JSON)
├── strokes.jsonl   # 획 전체 (id, page, points, width, color, tool)
├── pages.json      # 페이지 목록 (paper, rotation)
├── edits.json      # 영속 편집 저널(undo) 배열
└── pdf.digest      # sha256 참조 (바이트 미포함)
```

## 10. 비용

$$
T_{\text{save}} = \text{RTT}_{202} + \frac{B_{\text{strokes}}(1-r)}{\text{bw}} + t_{\text{proc}}
\qquad
T_{\text{load}} = \text{RTT} + \frac{B}{\text{bw}} + t_{\text{query}}
$$

충돌 시 +1~2 왕복(드묾). PDF는 CAS 1회.
**UI 스레드가 부담하는 비용은 스냅샷 `Arc` 복제 O(1)이 전부입니다.**

## 11. 함정과 대응

- **"전체 ZIP"이 커질까 걱정**: ZIP에는 획/메타만. PDF는 CAS 참조라 최초 1회.
  임계값(변경량·시간)을 키우면 업로드 빈도 조절.
- **백그라운드 ZIP 만들기와 필기 공유 상태**: 잠금 금지. §4의 `Arc` 스냅샷으로
  업로더는 UI 데이터를 읽지도 잠그지도 않습니다.
- **pen-down 중 임계 발사 금지**: 펜이 내려간 동안은 임계 검사 자체를 건너뜁니다(§4.4).
- **서버 큐 순서**: 문서별 단일 소비자. revision 증가 순서 = 적용 순서.
- **변경 로그 무한 증가**: 보관 기간 설정, 오래된 클라이언트는 전체 로딩으로 폴백.

## 12. 이전 제안과의 관계

- v3-번들(순수 이벤트 소싱, `/v3/bundles`) 설계에서 **클라이언트 복잡도를 걷어낸** 단순화안.
- 패치 계산/적용은 서버가 담당(§5.2) — 클라이언트 diff 로직 없음.
- 실시간 필기 중에는 임계값을 작게(예: 1초 무입력) 두면 사실상 연속 동기화가 됩니다.
