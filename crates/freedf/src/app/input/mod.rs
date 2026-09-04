//! 입력 소스(펜/마우스/트랙패드) 판정 — 기기 종류 추정과 소스별 활동 추적.
//!
//! 판정 규칙은 [`hooks::InputSources`] 한 곳에 모여 있습니다.

pub(crate) mod hooks;

pub(crate) use hooks::InputSources;
