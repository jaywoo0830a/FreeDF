# 아이디어 #5 — 계약 상향 (C1~C4) 실행 스펙

> idea4 가 **게이트를 땜질에서 구조로** 바꿨다면(세션 라우터), idea5 는 그 위에
> 남아 있던 **같은 실수의 다른 얼굴들**을 계약으로 승격한 변경분이다.
> Rust 이식: `crates/freedf` (커밋 "잔여 땜질 청소" 이후 C1~C4 작업).
> 이 문서는 흐름 파악용 인덱스다 — 엄격함보다 **무엇이 어디로 옮겨졌는지**가 목적.

## 두 관문 (무엇을 땜질로 보는가)

| 관문 | 질문 | 걸리면 |
|---|---|---|
| ① 같은 시계인가 | 그 판단이 다른 시계(egui/시계/모니터)를 다시 읽는가? | 이벤트가 안고 온 데이터로 대체 |
| ② 다른 소유자가 있는가 | 허브/어댑터/라우터/워크스페이스가 이미 아는가? | 소유자에게 묻는다 |

## C1 — 오버레이 탭 판정 → 라우터 싱크

| | |
|---|---|
| 종전 | 오버레이가 egui 원시 이벤트(`frame_tap_pos`)를 다시 읽어 탭 판정 — 잉크와 **다른 시계**. 뒤처리로 삼킴 표식(`wheel_swallow_click`)·적체된 이벤트(유령 점) |
| 이후 | `createWheelSink` 가 라우터 싱크 (우선순위 1, `canvasSinks`). 프레스는 이벤트가 안고 온 좌표로 판정되고 결과는 **의도**로 앱에 전달 |
| 계약 | 히트테스트는 순수 기하(`wheelGeom().hit`) · 싱크는 앱 상태를 모른다(intent 만) · 열려 있으면 프레스 전부 휠 소유 |
| 실행 스펙 | [`wheel-sink.test.js`](./wheel-sink.test.js) |
| Rust | `app/input/wheel_sink.rs`, `app/input/mod.rs`(`CanvasSinks`), `canvas/input.rs`(주입/의도 적용), `canvas/wheel.rs`(기하 위임) |

## C2 — 틸트: 크기 → **벡터**

| | |
|---|---|
| 종전 | 어휘는 크기(float)만 나른다 → 방향이 필요한 쪽(커서 렌더)이 어휘 밖의 별도 벡터를 들고 다님 (이중 상태) |
| 이후 | `Pointer.tilt = [x, y]` (도) — 크기/방위각은 `tiltMagnitude`/`tiltAzimuth` 로 **파생**. 어댑터가 조건화한 벡터를 그대로 싣는다 |
| 계약 | 이벤트가 나르는 틸트 = 소비자가 보는 틸트 (미러 금지). 구 호출(숫자)은 `[n, 0]` 으로 정규화 |
| 실행 스펙 | [`tilt-contract.test.js`](./tilt-contract.test.js) |
| Rust | `input_events.rs`(`tilt: [f32;2]` + `tilt_magnitude`/`tilt_azimuth`/`NO_TILT`), `input_devices.rs`, 앱의 `pen_tilt` 미러 삭제 |

## C3 — 틸트 능력: 스트림 존재 근사 → **능력 질의**

| | |
|---|---|
| 종전 | "틸트를 보고하는 장치인가"를 `pen_monitor.is_some()`(스트림 존재)으로 근사 — 압력만 보고하는 펜도 스트림은 있다 |
| 이후 | `PenCapabilities`(장치 열거가 아는 사실) → 어댑터 `tiltSupported()` → 렌더 분기. 모르면 낙관(구 동작 보존) |
| 실행 스펙 | [`capability.test.js`](./capability.test.js) |
| Rust | `pen_input.rs`(`PenCapabilities`), `input_devices.rs`, `canvas/paint.rs`, HUD 라벨 |

## C4 — 진단 3종 → **단일 판정**

| | |
|---|---|
| 종전 | `stroke end` 판정(ink.rs) · `PENUP-CHANGED`(ink.rs) · `LIVE-FLAT`(paint.rs) 가 흩어져 각자 다른 재료만 봄 → **설정을 모르는 오진**(writing.log: 필압 끔을 "OTD 확인"으로 진단) |
| 이후 | 재료 = 설정/장치 + 측정 + **라우터 장부**(`summary()`). 판정은 `strokeVerdict` 한 함수, 로그도 한 줄 (구 grep 토큰은 접어 넣어 유지) |
| 계약 | 설정 꺼짐/탭은 정상으로 판정 · 평평함의 원인을 장부(`stale`/`expired`)로 구분 |
| 실행 스펙 | [`verdict.test.js`](./verdict.test.js) |
| Rust | `app/canvas/diagnostics.rs`, `session_router.rs`(`LedgerSummary`/`summary()`), `canvas/ink.rs`, `canvas/paint.rs` |

## 흐름 한 눈에

```
장치(evdev/OTD) ── PenEventAdapter ─(틸트 벡터·압력·소스)─┐
egui(마우스/터치) ── egui_adapter ───────────────────────┤
                                                         ▼
                                                      Hub (점유 규칙)
                                                         ▼
                          SessionRouter ── CanvasSinks[w1: WheelSink, w2: InkSink]
                                                         ▼
                                        Workspace(툴) → Command → 앱(문서/렌더)
                                                         ▼
                            diagnostics: 설정 + 측정 + 장부 → 한 판정 (HUD/로그)
```

## 아직 남은 땜질 (다음 후보)

- 허브의 점유 스테일 타임아웃/소스 우선순위 (idea4 README #3의 나머지 반쪽).
- 진단이 쓰는 접촉 통계(`pen_contact`/`live_pressure`)도 장치 축 어휘로 승격할지.
- 휠 자동 닫힘 타이머(4초)는 **UI 소유 정당** — 오버레이 자신의 유휴 정책이라 남긴다.
