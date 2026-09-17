//! Design-token based Nord theme.
//!
//! - [`tokens`] — primitive tokens (Nord palette, spacing, typography)
//! - [`nord`] — semantic tokens + the `egui::Style` builder / installer
//!
//! 구현은 공유 크레이트 [`freedf_theme`]로 추출했습니다 — elm-magic 셸
//! (`freedf-gui`)이 같은 테마를 써야 하기 때문입니다
//! (docs/freedf-gui-migration.md). 여기서는 기존 `crate::theme::…` 경로
//! 호환을 위해 재수출만 합니다.

pub use freedf_theme::{nord, tokens};

