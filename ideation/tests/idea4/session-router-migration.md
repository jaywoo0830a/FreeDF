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

## 5. 적용 후 실기 회귀 — "점이 찍히듯이" (0916-2)

마이그레이션 커밋(31dcc74) 이후 실기에서 **획이 점으로 잘리는** 증상이 보고됐다.
원인은 라우터가 아니라, **게이트를 기하로 바꾼 뒤에도 egui 레벨을 읽고 남아
있던 두 번째 땜질**이었다 — 같은 실수의 다른 얼굴:

```rust
// canvas/input.rs (수정 전)
let primary_down = ctx.input(|i| i.pointer.primary_down());   // egui 시계
...
if !primary_down && self.active_stroke.is_some() { self.finish_stroke(); }  // 가짜 Up
```

- 샘플링 게이트 시절에는 게이트 자체가 egui의 점유 인정
  (`is_pointer_button_down_on`)이었으므로 "게이트 통과 ⇒ egui가 프레스를 안다"
  가 성립했다 → `!primary_down` = "정말 뗐다".
- 게이트가 **순수 기하**가 되면서 그 등식이 깨졌다: 펜 Down 에지는 egui보다 먼저
  도착할 수 있으므로(evdev/OTD가 빠른 시계) 그 프레임의 `primary_down`은 아직
  false다. 보험이 이 **한 프레임 지연을 Up 유실로 오독**해 획을 시작점에서 잘랐고,
  그 뒤의 Drag 는 툴 세션이 닫혀 있어 버려졌다 = 점.

### 로그 증거 (`writing.log`, 28획)

| 부류 | 획 수 | 종료 시 `live_pressure` | 해석 |
|---|---|---|---|
| `점 부족` (n=1) | 14 | **0.07~0.14 (13/14)** | 펜이 **눌린 채** 종료 = 가짜 Up |
| 정상 (n=9~29) | 14 | 0.0 (14/14) | 진짜 펜 뗌 |

경합을 이긴 프레스만 살아남아 정확히 반반 — "간혹 연속적이긴 한데 확실하지 않다"와 일치.

### 수정

1. **보험을 에지 기준으로**: `router.open_session().is_none() && active_stroke.is_some()`
   — egui/시계를 읽지 않는다. Up 유실은 이제 라우터가 소유한다.
2. **라이브 세션 워치독** (`frame_live`, 스펙 확장): 이벤트가 `SESSION_STALE_MS`(500ms)
   끊겼고 **접촉 증거도 없을 때만** 합성 up 을 정상 경로로 전달해 세션을 닫는다.
   두 조건 모두 필요하므로 ① 한 프레임 시계 지연은 창 안에 묻히고 ② 펜을 대고
   멈춰 있는 동안(증거 있음)은 닫히지 않는다. 접촉 증거는 egui의 `primary_down`
   뿐 아니라 **하드웨어 접촉**(`PenState.contact` = tip 스위치 ∨ 필압>0 — 우리
   스트림과 같은 시계라 경합이 없다)을 포함한다. 워치독은 **장부에 새 행을 쓰지
   않는다** — Down 에지는 승인 시점에 이미 정산됐다(계약 ③ 유지), 관측은
   `Report`(로그)다.

### 고정한 회귀 테스트

- `lagging_egui_frame_never_kills_a_live_session` — 증거 없는 프레임만으로는 세션이 끝나지 않는다.
- `quiet_but_evidenced_session_stays_open` — 멈춰 있는 펜은 획의 끝이 아니다.
- `lost_up_edge_is_closed_by_the_live_watchdog` — 진짜 유실은 창 뒤에 합성 up 으로 닫힌다.
- `lagging_evidence_frame_keeps_the_stroke_alive` (파이프라인) — 첫 프레임 증거가
  거짓이어도 `begin → extend… → end` 온전한 획.

### 교훈

게이트를 "다른 시계를 읽지 않는 형태"로 바꿀 때는, **그 게이트가 암묵적으로
보증하던 등식**(여기서는 "세션이 열렸다 ⇒ egui가 프레스를 안다")을 함께 찾아
없애야 한다. 남은 egui 레벨/리액션 사용처는 팬(UI 소유가 정당)과 탭 점
커밋(가드 유지)뿐이다.
