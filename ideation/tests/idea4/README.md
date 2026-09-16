# 획 시작 에지 계약 — 참고 문서 (2026-09-15 필기 유실 회귀)

> 이 문서는 P1~P3(아이디어 #4의 Rust 이식) 이후 발생한 "필기 유실/획 연결" 회귀의
> **사고 기록이자 계약 명세**다. 실행 스펙은 같은 디렉터리의
> [`stroke-edge.test.js`](./stroke-edge.test.js) — 이 문서의 표가 곧 그 테스트의 인덱스다.
>
> | | |
> |---|---|
> | 증상 발생 | P1(`aa30b36`) · P2(`594eca3`) · P3(`d27cd08`) 머지 직후 |
> | 원인 확정 | `tmp/0916debug.log` (Debug HUD 진단, 763줄) |
> | 1차 수정 | `fix/stroke-down-edge-defer` 브랜치, 커밋 `a59a5e3` — 앱 경계 에지 보류/승격 |
> | 미해결 | 장치 경계 에지 보존(#4), 세션 진실 단일화(#3), 기하 게이트(#2) — 아래 "남은 위험" |

## 1. 증상

- **필압이 낮으면 필기가 안 된다.** 가볍게 그으면 획이 아예 나오지 않는다.
- **세게 누르면 가끔 이어진다.** 필압을 세게 주면 그려지지만 100%는 아니다.
- 되돌린 "힐 로직"(`be73218`)으로 고치면 이번엔 **펜을 뗀 뒤 점이 연발**했다.

필압은 **원인이 아니라 시간차의 변수**였다 — 아래 3.2 참고.

## 2. 증거 (`tmp/0916debug.log`, Debug HUD 진단)

| 관측 | 값 | 의미 |
|---|---|---|
| 펜 Down/Up 에지 | **7 / 7** | 장치 경계(어댑터)는 에지를 정확히 냈다 |
| 시작된 획 | **4** (`stroke start`) | 7건 중 **3건이 유실** |
| 유실 프레스 | t=18.2250 · 20.8748 · 22.2498 | `pen(d/dr/u)=1/0/0` 인데 `cmds(b/ext/end)=0/0/0` |
| 유실 프레스 이후 | Drag가 **매 프레임**(≈100Hz) 도착, 커맨드 **0건** | 세션이 열리지 않았다 → 프레스 전체 폐기 |
| `hub_drop` | 필기 중 **매 프레임 12~30** | 같은 물리 펜이 `pen`(evdev)과 `mouse`+`pad`(egui) 두 스트림으로 도착 → 반대쪽 통째 drop |
| Down 프레임 | `mouse=0/0/0 pad=0/0/0` (drained) | egui 이벤트는 도착했지만 **모두 drop** → 게이트 판정은 egui 상태에만 의존 |

핵심: **장치 경계는 정상, 앱 경계의 게이트 1회 판정이 에지를 파괴**했다.
Down이 버려진 뒤에는 Drag가 아무리 와도 툴이 idle 이라 `(false, drag) → 무시`,
Up도 `(false, up) → 무시` — 복구 경로가 0이었다.

## 3. 원인 (3층)

### 3.1 주원인 — Down 에지를 **한 프레임**, **타 суб시스템 소유 조건**으로 판정

리팩터 전(`5eebde1`)은 `primary_down && (down_on || dragged())` 를 **매 프레임 폴링**해
게이트가 늦게 참이 되어도 획이 살아났다("늦은 시작"). 커맨드 경로(허브 → 툴 →
`begin-stroke`)로 옮기면서 Down **에지 1회**만 평가하게 됐고, 그 1회가
`response.is_pointer_button_down_on() || response.dragged()` — **egui(윈도우 메시지 루프)가
소유한 상태** — 에 막히면 끝이었다. evdev 폴링 스레드(장치 축)와 egui는 **지연이 다른
두 시계**라, 같은 물리 프레스가 프레임 경계에서 어긋날 수 있다.

### 3.2 왜 "필압"과 상관되어 보였나

가벼운 접촉에서는 팁 임계 부근에서 **BTN_TOUCH(장치 contact)가 OS 가상 press(→egui)보다
먼저** 오는 시간차가 커진다 → 게이트가 거짓인 프레임에 Down 도착 → 유실.
세게 누르면 두 사건이 같은 프레임에 들어올 확률이 올라간다 → "가끔 된다".
즉 **경합(race)** 이고 필압은 그 시간차를 만드는 변수일 뿐, 재현이 100%가 아닌 이유.

### 3.3 부원인 — "펜이 눌려 있다"는 사실이 4곳에 중복 저장

| 저장소 | 갱신 규칙 | 깨질 때 |
|---|---|---|
| `Hub::active_source` | 같은 소스의 Up만 해제 | Up drop → 점유 고착, **반대 스트림 전체 drop** |
| `Workspace::pointer_down` | Down/Up 에지 | 에지 상실 → 영구 true |
| `PointerTool::active` | `(true, Down) → 무시` | true 로 잔류 → **다음 Down 삼킴** |
| `App::active_stroke` | 에지 + 폴링 보험 | 보험(`finish_stroke`)은 잉크만 닫고 툴/워크스페이스에 미통보 |

- 보험이 잉크만 닫으면 → 다음 프레스의 Down 이 삼켜져 **획이 안 나옴**.
- Up 에지가 상실되면 → `active_stroke` 가 열린 채 남아 다음 프레스의 점이 같은 획에
  append → **획이 연속으로 이어짐**.
- 힐 로직(되돌린 패치)이 "세션 밖 Drag → Down 승격"을 한 이유는 "점 연발": Up 프레임에도
  egui 꼬리 hover 이벤트가 `mouse=0/14/1 pad=0/14/1` 로 쏟아져, 승격된 세션이 곧바로
  따라오는 Up 에 닫혀 프레임마다 점이 찍혔다.

### 3.4 장치 경계 — 어댑터도 에지를 소모한다 (후속 #4)

`input_devices.rs` 의 `PenEventAdapter::update` 는 `point == None`(egui `hover_pos()` 미확정)로
이벤트를 못 만드는 순간에도 `prev_contact` 를 갱신한다 → **Down/Up 에지 영구 소실**.
디버그 커밋(`25f45f2`)이 이 계약 테스트를 추가했고 revert(`ef72376`)가 함께 지웠다.

## 4. 계약 명세 — "에지는 파괴되지 않는다"

### 4.1 앱 경계 (캔버스 정책 게이트) — 참고 모델 ① / Rust `PendingDown`

| 규칙 | 내용 |
|---|---|
| 보류 | 게이트가 거짓인 프레임의 Down 은 버리지 않고 `(에지, 시각)` 과 함께 보관 |
| 승격 | **모두 참**: 팬 아님 · TTL 안(250ms) · 게이트가 *지금* 참 · 접촉 증거 있음 |
| 승격 좌표 | **원래 접촉점/필압** (늦은 시작보다 정확 — 첫 점이 실제 접촉점) |
| 승격 순서 | 이번 프레임 이벤트 **맨 앞** — `begin → extend` 순서 보장 |
| 폐기 | 같은 소스의 Up · 팬 프레임 · 포커스 유예 · TTL 만료 (부활 금지) |
| 반계약 | **보류된 에지가 없으면 절대 승격 없음** — 세션 밖 Drag 승격(힐 로직) 금지 |

`접촉 증거` = egui `primary_down` 또는 같은 소스의 Drag(펜/패드 어댑터는 접촉 중에만 Drag 를 만든다).

### 4.2 장치 경계 (펜 스트림 어댑터) — 참고 모델 ② / 후속 #4

| 규칙 | 내용 |
|---|---|
| 에지 보존 | 접촉 Down/Up 순간 위치가 없으면 에지를 **소비하지 않고** 보관 |
| 해소 | 위치가 도착하는 다음 갱신에서 **원래 위상** 그대로 emit (Down 이 Drag 보다 먼저) |
| 능력 협상 | 압력/기울기 기본값 채움은 어댑터 몫 (툴은 fallback 을 모른다 — 기존 계약 유지) |

### 4.3 허브 — drop 은 관측 가능해야 한다

"한 번에 한 포인터" drop 자체는 설계대로다. 단 **관측 불가능하면 유실 원인을 알 수 없다** —
이식은 `emit` 반환값 또는 누적 카운터로 drop 을 반드시 관찰 가능하게 유지한다
(revert 로 사라진 `Hub::dropped()` 는 이 계약의 구현이었다 — 복원 권장).

## 5. 테스트 → 계약 매핑 (`stroke-edge.test.js`)

| 테스트 | 고정하는 계약 | 상태 |
|---|---|---|
| 게이트가 늦게 참이 되어도 프레스 전체가 살아난다 | 4.1 보류/승격 — 회귀 서명의 반전 | ✅ Rust 적용됨 |
| 첫 점은 보류된 Down 의 원래 접촉점 | 4.1 승격 좌표 | ✅ |
| 보류된 에지가 없으면 절대 승격하지 않는다 | 4.1 반계약 (점 연발 방지) | ✅ |
| TTL 을 넘긴 보류는 승격되지 않고 버려진다 | 4.1 폐기 (부활 금지) | ✅ |
| 팬 프레임이 프레스를 소유한다 | 4.1 폐기 (팬 소유) | ✅ |
| 같은 소스의 Up 이 도착하면 보류는 폐기된다 | 4.1 폐기 (프레스 종료) | ✅ |
| 접촉 증거가 없으면 게이트가 참이어도 승격하지 않는다 | 4.1 승격 조건 ④ | ✅ |
| 펜 점유 중 반대 소스 스트림은 drop — 관찰 가능해야 한다 | 4.3 관측성 | ⚠️ JS ✅ / Rust 카운터 미복원 |
| 접촉 Down 순간 위치가 없어도 에지는 소실되지 않는다 | 4.2 에지 보존 | ❌ Rust 미적용 (#4) |
| 위치 없는 Up 에지도 보존된다 | 4.2 에지 보존 | ❌ Rust 미적용 (#4) |
| 어댑터+게이트 합성 스트림도 잘-형성 | 두 경계의 책임 분리 증명 | ❌ #4 후 |

> 참고 모델(`createStrokeGate`, `createPenStreamAdapter`)은 테스트 파일 안의 명세다.
> 계약 객체로 승격하면 `src/idea4/canvas-policy.js` 등으로 옮기고
> `layering.test.js` 의 `ALLOWED` 맵에 의존성을 등록한다.

## 6. Rust 구현 매핑 (커밋 `a59a5e3`)

| 항목 | 위치 |
|---|---|
| 보류 상태기계 `PendingDown` (TTL `PENDING_DOWN_TTL_MS = 250`) | `crates/freedf/src/app/canvas/input.rs` 상단 |
| 보류 (게이트 거짓 프레임) | `input.rs` 허브 소비 클로저 `PointerPhase::Down` 분기 |
| 승격 (게이트 지금 참 + 접촉 증거) | `input.rs` `self.input_hub = hub;` 직후 블록 — `pointer_events.insert(0, d)` |
| 폐기 (Up/팬/포커스 유예) | 같은 클로저 `PointerPhase::Up` 분기 + 승격 블록 전단 |
| 단위 테스트 6건 | `crates/freedf/src/app/canvas/tests.rs` (`deferred_down_*`, `no_retained_edge_*`, `cancel_returns_*`) |
| 진단 로그 | `STROKE-RECOVER`(승격, delay=Nms) / `STROKE-DROP`(팬·만료) — Debug HUD 켤 때만 |

## 7. 실기 검증 절차

1. Debug HUD 를 켜고 펜으로 그린다 (`pen_trace` 활성 → stderr + 실행 디렉터리 `freedf_pendebug.log`).
2. `grep -E 'STROKE-RECOVER|STROKE-DROP' freedf_pendebug.log`
   - `STROKE-RECOVER … delay=Nms` → 늦은 시작 복구 동작 중 (N = 게이트가 거짓이었던 프레임 수)
   - `STROKE-DROP … gate=false` → TTL 안에 게이트가 참이 되지 않음 (게이트 쪽 원인)
   - `STROKE-DROP … gate=true evidence=false` → 접촉 증거 부재 (어댑터 에지 소실 — #4 확정)
3. 수동 확인: ① 가볍게 그어도 획이 나오는가 ② 펜을 뗀 뒤 점이 연발하지 않는가
   ③ 툴바/오버레이를 눌렀을 때 잉크가 생기지 않는가 (게이트 회귀 없음)

## 8. 남은 위험 / 후속 수정

| # | 내용 | 근거 | 상태 |
|---|---|---|---|
| #2 | 게이트를 순수 기하로 — `canvas.contains(point)` + `response.hovered()` 참고용. `response` 가 "필기 가능 여부"를 결정하는 결합 제거 → 보류 창이 사실상 불필요 | 3.1 | **완료** (InkSink 순수 기하 admit) |
| #3 | 세션 진실 단일화 — "놓친 Up 보험"을 `finish_stroke()` 대신 `workspace.handle(Up)` 으로 라우팅, 허브 점유에 스테일 타임아웃 + 소스 우선순위(Pen > Pad > Mouse) | 3.3 | **부분** — 라이브 워치독(`frame_live`, 합성 up)으로 대체 완료. 허브 점유 스테일 타임아웃·소스 우선순위는 미착수 |
| #4 | 어댑터 에지 보존 — `prev_contact` 갱신을 "이벤트를 실제로 냈을 때"로 제한 (pending 에지 보관) | 3.4 | **완료** |
| — | `pressure_collapsed` 문턱(`pressure<=0.01 && last>0.05`, `commands.rs`) 재검토 — 가벼운 획이 4점에서 잘릴 수 있음 | 증상 (1) | **완료 (삭제)** — 꼬리 가드 제거, 꼬리 없는 구조 |
| — | `Hub::dropped()` 카운터 복원 (진단 계약 4.3) | 2, 4.3 | **완료** |

## 8.1 땜질 청소 2차 (0916-3) — 무엇이 사라졌나

`session-router-migration.md` §6 참조. 요약:

| 사라진 것 | 무엇을 메우고 있었나 | 구조적 대체 |
|---|---|---|
| 탭 점 커밋(`response.clicked()` + `commit_dot`) | 탭을 egui 리액션으로 **다시** 판정해 점을 중복 커밋 | 라우터 세션 `[down, up]` → 1점 획 |
| `focus_swallow_next_click`, `wheel_swallow_click` | 위 점 경로의 예외 삼킴 표식 2개 | 세션이 없으면 점도 없다 / 휠이 프레스 소유 |
| 휠 열림 조기 반환의 허브 미소비 | 포인터 이벤트 적체 → 휠 닫힐 때 유령 점 | 휠이 열려 있는 동안 포인터 스트림 즉시 소비 |
| 장치 래치(`InputDevice`, `last_touch_time`, has_touch 추정) | egui Touch 유무로 펜/마우스 추정 + 1초 유예 | 이벤트의 `PointerSource` + `Hub::active_source()` |
| `pressure_source`/`sample_pressure`(모니터 스냅샷·egui Touch force 재샘플링) | 압력을 다른 시계에서 다시 읽음 (펜 압력이 마우스 획에 샘) | `PointerEvent.pressure`(어댑터 능력 협상) + `model_pressure()` 한 곳 |
| `LIFT-CUT`(접촉 해제/필압 붕괴 추정 꼬리 컷) | 펜 떼는 순간 가늘어짐 (소스 무관 가드 — 마우스 4점 절단 잠복 버그) | 어댑터 Up 에지 + 라우터 세션 닫기 (꼬리가 존재하지 않음) |
| 캔버스의 `smooth_tilt`(틸트 EMA/점프 제한) | 장치 노이즈를 앱이 필터링 | `PenEventAdapter`가 장치 상태로 소유(`adapter.tilt()`) |
| 판정 "점 부족 / 필압 일정 → OTD 확인" | 설정(필압 끔)과 탭을 모르는 오진 | 설정/탭 구분 판정 |

## 8.2 아직 남은 땜질 (다음 청소 대상)

> **C1~C4 는 처리됐다** — 실행 스펙은 [`../idea5/`](../idea5/README.md)로 옮겼다
> (C1 휠 싱크 · C2 틸트 벡터 · C3 틸트 능력 · C4 진단 단일화). 아래 표는 이력이다.

| # | 위치 | 무엇을 메우고 있나 | 구조적 대체 (계획) | 상태 |
|---|---|---|---|---|
| C1 | `overlays.rs` 휠 탭 판정(`frame_tap_pos`, egui 리액션) | 오버레이 입력이 잉크와 **다른 시계**로 판정됨 | **WheelSink** — 라우터 싱크로 승격(기하 소유, 우선순위 선행) | **완료** (`idea5/wheel-sink.test.js`) |
| C2 | 이벤트 어휘의 틸트가 **크기(float)** 뿐 | 방위각(방향)이 어휘에 없어 앱이 별도 벡터를 들고 렌더에 씀 | `PointerEvent`에 틸트 벡터 추가 (events.js 스펙 동시 갱신) | **완료** (`idea5/tilt-contract.test.js`) |
| C3 | `paint.rs`의 `pen_monitor.is_some()` 분기 | "틸트를 보고하는 장치인가"를 스트림 존재로 **근사** | 능력 협상: `pen_input`이 `reports_tilt` 능력을 보고 | **완료** (`idea5/capability.test.js`) |
| C4 | 진단 계열(`LIVE-FLAT`, `PENUP-CHANGED`, `pen_verdict`) | 증상 추적용 하드코딩 판정 — 라우터 장부와 분리 | 장부(`Resolution`)+접촉/압력 통계를 합친 단일 판정 | **완료** (`idea5/verdict.test.js`) |
| C5 | `InputSources`의 `#[allow(dead_code)]` 선제 필드/접근자 | 쓰이지 않는 마우스/트랙패드 활동 추적 | 실제 소비자가 생길 때까지 삭제 | **완료** |

## 9. 다음 설계 — 세션 라우터 (땜질에서 구조로)

`stroke-edge.test.js` 의 참고 모델 ①(PendingDown)은 정확하지만, 보류 상태기계가
**앱 경계(`input.rs`)에 손으로 붙여진** 형태다 — "지금 그려도 되는가"를 판정하는
샘플링 게이트(`response.is_pointer_button_down_on()`)가 남아 있어 같은 사고가
다른 형태로 재발할 여지가 있다. `session-router.test.js` 는 이를 구조로 바꾸는
설계 스펙이다.

### 9.1 진단 — 버그는 "두 질문의 융합"에서 나왔다

- **라우팅** ("이 프레스를 어디로 보내는가") — 이벤트가 스스로 안고 있는 데이터만으로
  답할 수 있는 **순수 함수** 질문. 기하 포함 판정이 여기 해당 (후속 #2).
- **준비성** ("지금 그릴 준비가 됐는가") — 시간이 관여하는 질문. 포커스 유예, 접촉 증거.

리팩터는 둘을 하나의 샘플링 boolean 으로 융합했고, 샘플링은 **다른 시계(egui)의 상태**를
읽으므로 Down 에지 1회 판정이 경합에 질 수밖에 없었다. 땜질(PendingDown)은 이 융합을
해체하지 않고 앱에 시간 상태를 덧칠한 것.

### 9.2 이상적인 객체 — `createSessionRouter`

"프레스의 목적지와 완결을 소유하는 객체". 싱크 인터페이스:
`{ name, admit(session, ctx) → 'now'|'hold'|'refuse', handle(events) }`

| 강제 장치 | 내용 | 대응 테스트 |
|---|---|---|
| 판정 재료의 타입화 | 싱크가 보는 것은 `(session, ctx)` 뿐 — `response` 류 암묵 샘플링이 시그니처에 없음 | 싱크가 볼 수 있는 것은 (세션, 명시 문맥) 뿐이다 |
| 시계의 격리 | 라우터는 `Date.now` 등을 부르지 않는다 — 시간은 `dispatch/frame` 인자로만 (정적 검사) | 라우터는 시계를 읽지 않는다 |
| 장부(resolution) | 모든 Down 에지는 `delivered/refused/cancelled/expired/replaced` 로 정산 — 유실이 상태가 아니라 데이터로 관측됨 | 모든 Down 에지는 정산 장부를 갖는다 |
| 온전한 세션 | hold 된 세션은 승인 시 `[down, …버퍼된 drag]` 를 한 번에 재생 — **승인 전 샘플 무손실** (땜질은 이 구간을 유실함) | hold 된 세션은 승인 시 재생한다 |
| 완결 보장 | Up 프레임에야 승인돼도 완결 세션 전달 (빠른 탭 유실 방지) | 빠른 탭 테스트 |
| 점 연발의 구조적 차단 | 승격 원점은 언제나 "라우터가 보유한 세션" — 세션 없는 Drag 는 `unrouted` 로 정산될 뿐 세션을 만들 수 없음 | 거절은 닫힌 거절이다 |
| 정책은 데이터 | TTL·만료 시 전달 여부(`onAbandon`), 싱크 우선순위(배열 순서) — 흩어진 상태가 아니라 생성 인자 | onAbandon / 우선순위 테스트 |

**실패하는 인터페이스 테스트**: 말미의 대조군(`it.fails` 2건)은 현재 설계의 축소 모의
(샘플링 게이트 + 앱 보류)가 이 계약을 만족할 수 없음을 **실행으로 증명**한다 —
승인 전 Drag 유실, 장부 부재. "실패해야 통과"하므로 스위트는 녹색을 유지하면서
구조 교체의 명세로 남는다.

### 9.3 Rust 이식 경로

상세 마이그레이션 계획은 **`session-router-migration.md`** (배치도 · 단계별 Rust
변경 · JS 스펙↔Rust 테스트 1:1 매핑표 · 수용 기준 · 리스크) 로 분리했다. 요지:

1. **선행** — 어댑터 에지 보존 (#4): "위치 없는 Down"을 라우터가 아예 못 받게 한다.
2. **코어** — `SessionRouter` (순수 상태기계, egui 의존 0) + `Sink` 트레이트를
   `app/input/session_router.rs` 에 두고 JS 스펙을 1:1 유닛 테스트로 이식.
3. **싱크** — `InkSink.admit` = 순수 기하 (#2): 샘플링 게이트가 정책으로 강등되고
   hold 는 포커스 유예에서만 발생 — 라우터의 보류 장치는 하중을 지지지 않는
   안전망이 된다.
4. **재배선** — `input.rs`: `hub.take → router.dispatch`, 프레임 말미
   `router.frame`, `PendingDown`·승격 블록·`insert(0,…)` 삭제.
5. **관측** — 장부(Resolution) → 기존 `STROKE-RECOVER`/`STROKE-DROP` 로그 형식 유지.


