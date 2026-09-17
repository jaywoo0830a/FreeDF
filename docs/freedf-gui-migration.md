# freedf → freedf-gui 점진적 이전 계획

> 상태: **제안 (2026-09-17)** · 결정 전까지 `freedf`는 계속 출하 바이너리.
> 배경: `crates/freedf-gui`는 elm-magic(`view!`)으로 UI를 다시 쓰기 위한 재작성
> 실험 크레이트로, 현재 앱 셸(툴바·사이드바·탭·모달·상태바)까지 동작한다
> (`CHANGELOG.md` [Unreleased] 참고). 본 문서는 이를 정식 라인으로 만드는
> 단계별 계획이다.

## 0. 원칙

1. **freedf는 폐기 시점까지 손대지 않는다** — 이전 기간 내내 출하 가능 상태 유지
   (버그 수정만, 신규 기능 금지). 스위치오버 전까지 두 바이너리가 공존한다.
2. **공유 계층을 먼저 뽑아낸다** — UI가 아니라 서비스/모델 계층을 공용 크레이트로
   올려 "freedf 폐기"의 비용을 UI 코드만 남도록 만든다.
3. **계약은 그대로 유지** — eguidev 계측 id(`docs/eguidev-automation.md`)는 공개
   계약이므로 id를 새 크레이트에서 **동일하게** 재등록한다. smoketest도 대상만
   바꿔 재사용한다.
4. **각 단계의 끝은 항상 초록** — 두 크레이트 모두 `cargo check/test` + 스모크
   통과를 단계 종료 조건으로 삼는다.

## 1. 자산 분류 (crates/freedf ≒ 26.7k lines 기준)

| 분류 | 자산 | 이전 전략 |
|---|---|---|
| **순수 모델/계산** | `freedf-core`(노트·페이지·잉크·히스토리·검색·펜), `freedf-canvas`(잉크 메시), `freedf-sync`(프로토콜) | 이미 분리됨 — 그대로 재사용 |
| **서비스(GUI-프리에 가까움)** | `storage.rs`(295) · `sync_storage.rs`(1055) · `server.rs`(400, 미디어 클라이언트) · `sync_client.rs` · `pdf.rs`(557, pdfium) · `settings.rs`(983) · `recent.rs` · `recording.rs`/`player.rs` · `dictionary.rs` | **Phase 1** — 공용 크레이트로 추출 |
| **플랫폼(Windows 중심)** | `winstyle.rs` · `gamepad.rs` · `key_hook.rs` · Windows Ink 입력 | **Phase 4** — elm-magic 무관, 그대로 이식 |
| **UI(egui 명령형)** | `app/mod.rs`(3860) · `ui/`(3823) · `app/toolbar` · `app/panels` · `app/tabs` · `app/canvas` · `app/actions` · `theme/` | **Phase 2~3** — elm-magic으로 재작성 (포팅 아님) |
| **자동화** | `app/dev.rs` + `dev-automation` feature + smoketest | **Phase 3** — 계약 id 동일하게 재등록 |

## 2. 단계

### Phase 1 — 서비스 계층 추출 (freedf-gui v0.1) — **✅ 완료 (2026-09-17)**

- **완료**: `crates/freedf-services` 신설. `storage`·`sync_storage`·`server`·
  `sync_client`·`pdf`·`settings`·`recent`·`recording`·`player`를 `git mv`로 이동
  (이력 보존). `freedf`는 모듈 셔임(`pub(crate) use freedf_services::X::*;`)으로
  기존 `crate::X::*` 경로를 유지 — 호출부 무변화.
- 이동 중 정리: `pub(crate)` 항목 → `pub`(교차 크레이트 가시성, 20건),
  `pdf`가 `Pdfium` 타입을 재노출(freedf의 pdfium-render 직접 의존 제거),
  `settings::default_canvas_color`의 theme 참조를 리터럴로(서비스 계층은 egui 테마
  미의존 — 값은 NORD0 #2E3440 동일, 양쪽 동시 변경 주석), `server::normalized_base`
  → `pub` (freedf-gui 소비).
- 예외: `theme`(egui 스타일)과 `app/dictionary.rs`(오버레이 UI 포함)는 freedf에
  잔존 — settings의 `MacroKey::from_egui`만 예외적으로 egui::Key를 씀(services가
  egui에 얇게 의존).
- **검증**: `cargo test --workspace` 전부 통과 — freedf 92 + freedf-services 28
  (구 freedf 120을 정확히 분할) + freedf-gui 7(셸 6 + 서비스 스모크 1) + core/canvas/sync.
  freedf-gui는 이제 `freedf-services`를 직접 의존(`services_smoke` 테스트로 연결 확인).

### Phase 2 — 캔버스 (freedf-gui v1) — 최고 리스크 구간

- `<Raw>` 플레이스홀더 자리에 실제 페이지 렌더 + 잉크 오버레이 페인팅 연결:
  `freedf-core`의 프로젝션/변환 + `freedf-canvas` 메시 + pdfium 텍스처.
- 포인터/펜 입력 → `freedf-core` 커맨드 파이프라인 (기존 `app/input` 세션 라우터 재사용).
- **판단**: 캔버스는 painter 영역이라 elm-magic 어휘 밖 — `<Raw>` 사용이 *예외가
  아니라 정식 설계*. `<Raw>` 경계를 한 곳(`canvas.rs` 모듈)으로 몰아 둔다.
- **종료 조건**: freedf-gui에서 PDF 열기 → 확대/이동 → 잉크 스트로크 저장까지
  (freedf-core 저장소로) 동작. smoketest `20_ink_tool_picker` 상응 검증.

### Phase 3 — 위젯 UI 전면 재작성 (freedf-gui v2)

- 라이브러리/아웃라인/북마크/미디어 패널, 3단 툴바, 설정 창, 검색 바, 토스트를
  elm-magic으로 **재작성** (기존 코드 복사가 아니라 `view!` 설계로 다시 씀 —
  이게 이 크레이트의 존재 이유).
- eguidev 계측 재등록: id는 `docs/eguidev-automation.md` 표 그대로
  (`toolbar.*`, `tabs.*`, `canvas.surface`, `toast.*`, `menu.*`). elm-magic 경계를
  넘는 위젯(어댑터가 그린 버튼)은 `Pass.buttons`의 `Response`에 계측을 붙이는
  얇은 래퍼를 만든다.
- **종료 조건**: smoketest 스위트를 freedf-gui 대상으로 녹색화 (기존 스위트를
  `smoketest/gui/`로 복제해 이행).

### Phase 4 — 플랫폼/입력 이식 (v2.5)

- Windows Ink 압력, 게임패드, `winstyle`(네이티브 창), 전역 키 훅, 화상 키보드
  우회 등 — UI 프레임워크와 무관한 코드는 그대로 이식.
- **종료 조건**: Windows 실기에서 잉크 압력/게임패드 동작 확인.

### Phase 5 — 스위치오버 (v3)

- `freedf-gui`를 기본 바이너리로. `freedf` 빈은 한동안 유지하되 deprecated 표기
  (호환용 경로: `--doc <id>` CLI, 설정/DB 마이그레이션 없음 — DB는 이미
  PostgreSQL 단일 진실이므로 상태 이전 불필요).
- `crates/freedf`의 UI 코드 삭제, 서비스 크레이트만 잔존 → 이후 크레이트명 정리.
- docs 갱신(README, UI-COMPONENTS, eguidev-automation의 대상 크레이트 명기).

## 3. 리스크 & 완화

| 리스크 | 영향 | 완화 |
|---|---|---|
| elm-magic 미성숙 — 발견된 버그 3건(`docs/elm-magic-bug-report.md`) + 문서화된 제약(렌더 순서 기반 슬롯, 리스트 아이템 필드 대입 불가 등) | 재작성 중 컴파일/동작 장애 | rev 고정 유지, 워크어라운드를 `freedf-gui` 내 한 곳(`shell.rs` 헤더 주석)에 모으고 업스트림 수정 시 제거 |
| **렌더 순서 기반 슬롯** — 조건부로 등장하는 컴포넌트 순서가 바뀌면 상태 슬롯 섞임 (v0.4 keyed 대기) | 패널 on/off 등 동적 UI에서 상태 오염 | Phase 3까지 상태를 컴포넌트 최상위에 두고, 동적 자식 컴포넌트는 `key` 사용 or 순서 고정. keyed 트리 안정화되면 재평가 |
| eguidev 계약 id 재등록 누락 | 자동화/시각 리뷰 회귀 | 계약 표를 체크리스트화, smoketest를 스위치오버 게이트로 |
| 캔버스 성능/부드러움 재현 (연속 줌 최적화, One Euro 필터 등 freedf의 실측 튜닝) | 사용성 퇴보 | `ZOON-OPT.md`·`docs/OPTIMIZATION.md`의 계수/전략을 그대로 이식, 기존 테스트(freedf-core)가 로직을 보존 |
| Windows 전용 기능 (DWM/Mica, 잉크 압력) | Windows 품질 | Phase 4를 별도 단계로 분리 — Linux에서 먼저 기능 패리티 |
| 두 바이너리 공존 기간의 유지보수 분산 | 리소스 | Phase 1 이후 freedf는 feature freeze + 버그 픽스만 |

## 4. 지금 바로 하는 것

- [ ] `freedf` 리포지토리에 feature freeze 선언 (버그 픽스만)
- [x] Phase 1 완료: `freedf-services` 추출 (2026-09-17 — 위 참고)
- [ ] `docs/elm-magic-bug-report.md` 업스트림 전달 → 수정 리비전 나오면 워크어라운드 제거
- [ ] 패리티 원장: README Features 목록을 체크리스트로 `docs/freedf-gui-parity.md`에 옮기고 Phase마다 갱신
