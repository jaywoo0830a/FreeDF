# 세션 라우터 마이그레이션 — 땜질(PendingDown)을 구조로 교체

> **적용 완료** (2026-09-16). 5단계 전부 코드베이스에 반영됐다:
> #4 어댑터 에지 보존(`input_devices.rs`), `Hub::dropped()` 카운터,
> 코어 `app/input/session_router.rs` (JS 스펙 13건 1:1 이식),
> `app/input/ink_sink.rs` (순수 기하 즉담 — 생산 경로 hold 없음),
> `input.rs` 재배선 (땜질/게이트/`insert(0,…)` 삭제), 장부 → 기존
> STROKE-RECOVER/STROKE-DROP 로그. `cargo test --workspace` 통과.
> 이하 문서는 당시 계획이다.
>
> 대상 브랜치: `fix/stroke-down-edge-defer` · 스펙: `session-router.test.js` (15 tests) ·
> 참고: `stroke-edge.test.js`, 같은 디렉터리 README.md §9

## 0. 목표와 비목표

**목표** — `crates/freedf/src/app/canvas/input.rs` 의 샘플링 게이트
(`response.is_pointer_button_down_on()`)과 `PendingDown` 땜질을 세션 라우터 구조로
교체한다. 행동상 차이는 (1) 0916 유실이 재발하지 않고 (2) 땜질의 **승격 전 Drag
결손**과 **빠른 탭 유실**이 사라진다.

**비목표** — 툴 상태기계/워크스페이스/잉크 파이프라인 변경 없음. 커맨드 어휘 동일.
소스 우선순위·점유 만료(#3)는 별도 과제로 남긴다 (라우터는 그 자리를 비워둔다).

## 1. 배치 — 어디가 바뀌나

```
현재:
  evdev 스레드 ─► PenMonitor ─► pen_adapter ─┐
  egui 이벤트 ─► egui_adapter ────────────────┤ hub.emit (한 포인터 점유 규칙)
                                              ▼
  input.rs handle_canvas_input: hub.take
    ├─ 게이트 = response.is_pointer_button_down_on()   [샘플링 — 경합 원점]
    ├─ PendingDown.retain / promote_if                  [땜질]
    ├─ pointer_events.insert(0, 승격 Down)              [순서 수작업]
    └─ workspace.handle → take_commands

목표:
  (동일 전 경로: hub 까지 — 점유 규칙은 허브 소유 그대로)
                                              ▼
  input.rs: hub.take → router.dispatch(ev, now)     [한 소스의 온전한 스트림만 도착]
              │                          │
              │ frame(now, evidence)     ▼
              │                      sinks.handle(evs) ─► pointer_events push (순서 보존)
              └─ 보류/버퍼/TTL/장부는 전부 라우터 내부
    게이트 · PendingDown · insert(0) · 승격 블록 → **삭제**
```

교차 소스는 허브가 이미 걸러 내므로 라우터는 한 소스의 스트림만 받는다
(스펙 계약 ④ — 'foreign' 방어는 배선 실수를 드러내는 용도).

## 2. 단계

### 2.1 [준비] 어댑터 에지 보존 (#4 — 선행)
- `freedf-core/src/input_devices.rs`: `point == None` 일 때 `prev_contact`를
  갱신하지 않는다 — **위치 없는 Down 은 라우터에 도달하지 않는다**.
- 스펙: `stroke-edge.test.js` 모델 ② (이미 작성됨). 커어 테스트 1–2건.
- 함께: `Hub::dropped()` 카운터 복원 (관측 계약 4.3).

### 2.2 [코어] `SessionRouter` — 순수 상태기계, egui 의존 0
- 새 파일 `crates/freedf/src/app/input/session_router.rs`:
  ```rust
  pub enum Decision { Now, Hold, Refuse }
  pub struct SessionView<'a> { pub id: u64, pub source: PointerSource,
      pub down: &'a PointerEvent, pub drags: &'a [PointerEvent], pub age_ms: u64 }
  pub struct Ctx { pub now_ms: u64, pub evidence: bool }
  pub trait Sink { fn name(&self) -> &'static str;
                   fn admit(&mut self, s: &SessionView, ctx: Ctx) -> Decision;
                   fn handle(&mut self, evs: &[InputEvent]); }
  pub struct SessionRouter { sinks: Vec<Box<dyn Sink>>, ttl_ms: u64,
                             open: Option<Session>, resolutions: Vec<Resolution> }
  impl SessionRouter {
      pub fn dispatch(&mut self, ev: InputEvent, now_ms: u64) -> DispatchReport;
      pub fn frame(&mut self, ctx: Ctx) -> Option<DispatchReport>;
      pub fn ledger(&self) -> &[Resolution];
  }
  ```
- JS 스펙 대응 테스트 — **1:1 매핑표**:

  | JS (`session-router.test.js`) | Rust `#[test]` |
  |---|---|
  | 싱크가 볼 수 있는 것은 (세션, 문맥) 뿐 | `admit_receives_only_session_and_ctx` |
  | 라우터는 시계를 읽지 않는다 | (구조로 강제 — now_ms 인자 외 시계 없음, `clippy::disallowed_methods` 또는 리뷰) |
  | 모든 Down 은 정산 장부를 갖는다 | `every_down_is_settled` |
  | 교차 소스는 끊거나 섞지 않는다 | `foreign_source_never_taints_session` |
  | 증거 문맥은 프레임에서 흘러온다 | `evidence_flows_through_ctx` |
  | 순수 기하 싱크 — hold 없음 | `pure_geometry_sink_never_holds` |
  | Down 은 하나의 싱크로 | `down_reaches_exactly_one_sink` |
  | 거절은 닫힌 거절이다 | `refused_down_never_becomes_session` |
  | hold 승인 시 온전한 세션 재생 | `promotion_replays_buffered_drags` |
  | 빠른 탭 완결 전달 | `fast_tap_survives_late_admission` |
  | TTL 만료 정산 | `expired_hold_settled` |
  | hold 중 refuse → 취소 | `hold_refuse_cancels` |
  | onAbandon: deliver | `abandon_policy_is_data` |
  | 대조군 2건 | (Rust 대응 없음 — 구조 교체 자체가 명세) |

### 2.3 [싱크] `InkSink` — 게이트를 정책으로 강등
- `admit`: `geometry.contains_window(down.point)` (후속 #2 — **샘플링 게이트 삭제**).
  포커스 유예(`focus_grace_until_ms`) 구간이면 `Refuse` — 유예 프레스는
  삼켜져야 하는 프레스다. `Hold` 로 두면 유예 프레스가 유예 해제 뒤 400ms
  늦게 재생되는 지연·오동작이 된다. **hold 는 정말 "나중에 답이 생기는" 경우에만** —
  기하 즉담 구조에서는 평시 경로에 지연이 0프레임이고, hold/TTL 은 안전망이다.
- `handle`: 받은 이벤트를 `pointer_events`(혹은 직접 `workspace.handle`)로 —
  **Down→Drag→Up 순서 보존**이 싱크의 책임.
- `evidence`: "이 프레임에 그 소스의 접촉 증거가 있었는가" (Rust: hub 소비 중
  같은 소스 Drag/Down 관측) — 땜질의 contact_evidence와 동일 값.

### 2.4 [재배선] `input.rs` — 땜질 삭제
1. `hub.take` 소비를 `router.dispatch(ev, now_ms)` 로 교체.
2. 프레임 말미 `router.frame(Ctx { now_ms, evidence })` — 보류 세션 재판정.
3. `PendingDown` 구조체·`PENDING_DOWN_TTL_MS`·승격 블록·`insert(0, …)` 삭제
   (TTL 값은 라우터 생성 인자로 이전).
4. 활성 툴 동기화 / 커맨드 실행 블록은 그대로 — 커맨드는 싱크가 넣은
   pointer_events를 기존 경로로 소비.
5. 탭(점) 커밋: `response.clicked()` 경로는 이번 단계에서 그대로 둔다
   (레벨 기반이지만 라우터 세션과 충돌하지 않음 — `active_stroke.is_none()` 가드).
   놓친 Up 보험(`finish_stroke()`)은 `workspace.handle(Up)` 경유로 교체 (#3 일부).

### 2.5 [관측] 장부 → 로그
- `Resolution` 행을 기존 `STROKE-RECOVER` / `STROKE-DROP` 로그로 변환 (형식 유지 —
  디바이스 로그 비교 가능). `promoted/expired/cancelled/refused` 구분 포함.

## 3. 수용 기준

1. `tmp/0916debug.log` 시나리오(저필압 연속 프레스 7건) — 유실 0, 장부에
   전부 정산. 프로브 테스트 부활 없이 유닛으로 재현 가능.
2. 실기: 가벼운 필압 빠른 탭 ×20 → 20점. 탭 중 팬 프레임 → 점 없음(기존과 동일).
   세션 중 마우스 클릭 → 펜 획 유지(교차 소스 방어).
3. `cargo test` (freedf-core/-canvas/freedf) + ideation vitest 전부 통과.
4. 땜질 코드 흔적 0: `PendingDown`, `PENDING_DOWN_TTL_MS`, `promote_if`,
   `insert(0` 검색 결과 없음.

## 4. 리스크와 완화

| 리스크 | 완화 |
|---|---|
| `response.clicked()` 점 커밋과 라우터 세션의 이중 처리 | 가드 유지(`active_stroke.is_none()`), 점 커밋 흡수는 별도 과제로 문서화 |
| 포커스 유예의 Hold가 예상보다 잦음 | 장부로 hold 빈도 관측 → 잦으면 유예 정책 재검토 (데이터 우선) |
| 어댑터 #4 선행 실패 시 위치 없는 Down 유입 | 라우터 'foreign'/좌표 검증 방어 + `Hub::dropped()` 카운터로 관측 |
| 행동 변화(승격 전 Drag 보존)로 기존 그리기 결과 미세 변화 | 의도된 수정 — 0916 스펙 테스트가 정의하는 정확한 동작 |
