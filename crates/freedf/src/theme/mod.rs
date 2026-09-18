//! Design-token based Nord theme.
//!
//! - [`tokens`] — primitive tokens (Nord palette, spacing, typography)
//! - [`nord`] — semantic tokens + the `egui::Style` builder / installer
//!
//! 구현은 [`freedf_theme`] 크레이트에 있습니다 — 원래 `freedf` 내부 모듈이던
//! 팔레트/스타일 빌더를 elm-magic 셸(`freedf-gui`)과 공유하려고 추출했었습니다.
//! freedf-gui는 이후 elm-magic 0.6 CSS 속성으로 자체 스타일을 정의하며
//! freedf-theme 의존을 제거했으므로, 이 크레이트는 이제 **freedf 전용**입니다
//! (docs/freedf-gui-migration.md). 여기서는 기존 `crate::theme::…` 경로
//! 호환을 위해 재수출만 합니다.

pub use freedf_theme::{nord, tokens};
