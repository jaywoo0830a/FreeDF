//! 입력 소스(펜/마우스/트랙패드) 판정 — 기기 종류 추정과 소스별 활동 추적.
//!
//! - [`egui_adapter`]: egui 이벤트 → 통합 어휘 번역 (egui 쪽 장치 어댑터).
//!   egui 포인터 이벤트를 통합 어휘로 만드는 것은 이 모듈 하나로 한정한다 —
//!   어댑터 경계 바깥에서는 raw 이벤트가 아니라 허브를 향해야 한다.
//! - [`hooks::InputSources`]: 소스별 활동 추정 (판정 규칙은 이 파일에 모임).

pub(crate) mod egui_adapter;
pub(crate) mod hooks;
pub(crate) mod ink_sink;
pub(crate) mod session_router;

pub(crate) use hooks::InputSources;
pub(crate) use ink_sink::InkSink;
pub(crate) use session_router::SessionRouter;
