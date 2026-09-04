# FreeDF ZIP 일괄 동기화 프로토콜 (제안)

> 상태: 제안 (구현 전 검토용)
> 목적: "쿼리를 여러 번 기다리지 말고, ZIP 하나로 묶어 보내고 서버가 알아서
> 처리하게" — 왕복 수를 1로 줄이는 **일괄(bulk) 계층**.
> 기존 델타 프로토콜(`sync-protocol.md`)은 유지하고, 상황에 따라 선택합니다.

---

## 1. 현재 아키텍처에서 "기다림"의 정체

델타 프로토콜은 이미 획을 write-behind로 증분 전송합니다(1초 플러시 스레드).
따라서 **필기 중**에는 대부분 왕복을 기다리지 않습니다. 남는 대기는:

| 시점 | 현재 비용 |
|---|---|
| 문서 열기 | `document_load` 1왕복 (로더 스레드가 대기) |
| 저장/구조 연산 | flush + 델타 + meta — 왕복 1~3회 |
| 오래 오프라인 후 재연결 | 대기열 전부 순차 재전송, $N_{\text{rt}} = O(\text{대기 획 수})$ |
| 초기 가져오기/백업/마이그레이션 | 문장 단위 스트림 |

즉 "쿼리를 기다리는" 비용은 **왕복 수**에 있습니다. ZIP 일괄은 이것을

$$
N_{\text{rt}} = 1 \quad(\text{업로드}) + 1\ (\text{상태 폴링, 생략 가능})
$$

로 만드는 계층입니다.

---

## 2. 전송 (Transport)

```
POST /api/v2/sync/bulk
  Content-Type: application/zip
  Authorization: Bearer <token>
  Body: <sync-bundle.zip>

→ 202 Accepted
  { "bulk_id": "uuid", "state": "queued", "uploaded_bytes": N }
```

- 서버는 **즉시 202로 응답**하고 처리는 백그라운드(작업 큐)로.
- 클라이언트는 폴링 또는 SSE로 완료를 받습니다:

```
GET  /api/v2/sync/bulk/{bulk_id}
→ 200 { "state": "queued|processing|done|failed",
        "progress": 0.0..1.0,
        "new_rev": 7,                // 성공 시 새 문서 revision
        "error": null | { "code", "message" } }
```

- 실패 시 **같은 ZIP을 그대로 재전송**해도 안전해야 합니다(멱등성, §5).

---

## 3. 번들 레이아웃 (ZIP 내부)

```
sync-bundle.zip
├── manifest.json                 # 봉투: 버전·순서·다이제스트
├── doc-42/
│   ├── meta.json                 # page_count, pages[](용지/북마크)
│   ├── pdf.bin                   # PDF 바이트 (없으면 생략)
│   ├── strokes.jsonl             # 획 델타 (put/update/tombstone)
│   └── ops.jsonl                 # 구조 연산 저널 (순서 보존)
└── doc-77/ ...
```

### 3.1 `manifest.json`

```json
{
  "schema": 1,
  "client_id": "window-3f9a",
  "client_seq": 1234,
  "created_at": "2026-09-04T10:00:00Z",
  "documents": [
    {
      "doc_id": 42,
      "base_rev": 7,
      "strokes": "doc-42/strokes.jsonl",
      "ops": "doc-42/ops.jsonl",
      "pdf": "doc-42/pdf.bin",
      "digests": {
        "doc-42/strokes.jsonl": "sha256:…",
        "doc-42/pdf.bin": "sha256:…"
      }
    }
  ]
}
```

- `base_rev`: 이 번들이 기반으로 하는 서버 revision (§4 낙관적 동시성).
- `digests`: 전송 손상 검증 + 중복 감지.

### 3.2 `strokes.jsonl` — 획 델타

줄마다 하나의 멱등 연산, `op_id`는 재시도 안전용 UUID:

```jsonl
{"op_id":"…","op":"stroke_put","id":9001,"page":3,"points":[[x,y,p,t_ms],…],"widths":[…],"tool":"pen","color":[26,26,28,255]}
{"op_id":"…","op":"stroke_del","id":9002}
```

- 현재 프로토콜의 증분 문장과 **동일한 필드**, 배열이 아니라 파일로 묶은 것.
- 저장 시 전체 획 재전송은 하지 않음 — 변경된 획만 담습니다(델타 유지).

### 3.3 `ops.jsonl` — 구조 연산 저널

현재 SQL 델타 함수의 파라미터를 그대로 직렬화합니다(서버 SQL 재사용):

```jsonl
{"op":"page_insert","at":3,"count":1,"paper":{"style":"Ruled","color":[…]}}
{"op":"page_delete","page":5}
{"op":"page_shift","from":3,"delta":1}
{"op":"page_rotate","page":2,"cw":true}
{"op":"meta","page_count":10,"pages":[…]}
```

**순서가 의미**이므로 파일 내 줄 순서대로 적용합니다.

### 3.4 `meta.json` / `pdf.bin`

- 페이지/문서 메타는 작으므로 전체 덮어쓰기(현재 `document_sync_meta`와 동일).
- PDF는 바이트 그대로. 이미 DB에 동일 해시의 PDF가 있으면 서버가 재사용
  (media 객체 테이블 참조 — `0001_media.sql`의 콘텐츠 주소 저장 활용).

---

## 4. 서버 처리 (원자성 + 낙관적 동시성)

```
1. ZIP 압축 해제 → 임시 디렉터리, 크기/개수 상한 검사
2. manifest의 digests 검증
3. 단일 DB 트랜잭션 시작
   a. 모든 doc_id에 대해 base_rev == 현재 rev 인지 확인
      - 다르면 → 409 Conflict { latest_rev }  (전체 롤백)
   b. ops.jsonl 순서대로 적용 (기존 SQL 델타 함수 재사용)
   c. strokes.jsonl 적용:
      stroke_put  → INSERT … ON CONFLICT (doc_id, stroke_id) DO UPDATE
      stroke_del  → DELETE (없으면 무시)
   d. meta/pdf 갱신, rev += 1
4. 커밋 → 임시 파일 삭제 → 상태 done
```

- **원자성**: ZIP 하나 = 트랜잭션 하나. 중간 실패 시 부분 반영 없음.
- **동시성**: `base_rev` 불일치 시 서버는 병합하지 않고 **409로 거부**.
  클라이언트는 `latest_rev`를 받아 로컬에서 rebase 후 재전송
  (멱등성이 있으므로 재전송은 안전). 창이 여럿이면 마지막 커밋이 이김.

## 5. 멱등성 & 재시도

- 모든 획 연산에 `op_id`(클라이언트 생성 UUID)를 부여하고,
  서버에 짧은 **멱등 저널**(예: `op_id` 유니크 인덱스, 24h 보관)을 둡니다.
- ZIP 재전송 시 이미 반영된 `op_id`는 건너뜀 → 중복 획/중복 구조 연산 없음.
- 다이제스트가 같으면 "이미 반영된 번들"로 즉시 done 응답 가능.

## 6. 언제 ZIP을 쓰고 언제 델타를 쓸까

$$
T_{\text{delta}} = N_{\text{rt}}\cdot\text{RTT} + \frac{B}{\text{bw}},
\qquad
T_{\text{zip}} = \text{RTT} + \frac{B(1-\text{압축률})}{\text{bw}} + t_{\text{proc}}
$$

| 상황 | 선택 |
|---|---|
| 필기 중 실시간 증분 (1초 플러시) | **델타** (이미 왕복 숨김) |
| 페이지 삽입/삭제/회전 1건 | **델타** (1왕복, 0바이트) |
| 장기 오프라인 후 대량 재연결 | **ZIP** (대기열을 파일 하나로) |
| 저장 버튼 — "지금 다 반영하고 싶다" | **ZIP** (flush+meta를 1왕복으로) |
| 가져오기/내보내기/백업/마이그레이션 | **ZIP** (자연스러운 파일 포맷) |

## 7. 보안·운영 고려

- ZIP 크기 상한(예: 256MB), 엔트리 수 상한, **zip-slip 방지**(경로 검증),
  압축 폭탄 대비 압축률 상한.
- `bulk_id`별 상태는 24h 후 정리, 완료 번들은 요약만 남김.
- 기존 REST 델타 API와 병행 제공 — 클라이언트가 회선 품질에 따라 선택.

## 8. 기존 프로토콜과의 관계

```
필기(실시간): 획 → CachingBackend → pending.jsonl → 1초 플러시 → SQL 증분 (유지)
구조 연산 1건: SQL 델타 함수 (유지)
대량/저장/백업: sync-bundle.zip → bulk 엔드포인트 → 1왕복 + 상태 폴링
```

즉 ZIP 계층은 기존 델타를 **대체하지 않고**, "왕복을 묶고 싶은 순간"을 위한
일괄 채널입니다.
