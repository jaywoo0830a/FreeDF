//! # freedf-theme — FreeDF 공용 테마
//!
//! 디자인 토큰(Nord 팔레트)과 그 토큰을 `egui::Style`/`Visuals`에 매핑하는
//! 스타일 빌더를 담은 크레이트입니다.
//!
//! 원래 `freedf` 바이너리 안의 모듈(`crates/freedf/src/theme/`)이었지만,
//! elm-magic으로 재작성한 셸(`freedf-gui`)이 **같은 테마**를 써야 해서
//! 크레이트로 추출했습니다 (docs/freedf-gui-migration.md — 배경색 회귀 항목).
//! `freedf`는 [`freedf_theme`]를 재수출해 기존 `crate::theme::…` 경로를
//! 그대로 유지합니다.
//!
//! - [`tokens`] — 원시 토큰: Nord 16색 팔레트, 간격, 타이포그래피
//! - [`nord`] — 의미 토큰(`semantic`) + `egui::Style` 빌더(`nord_style`) /
//!   설치기(`install`)

pub mod nord;
pub mod tokens;

#[cfg(test)]
mod tests {
    use crate::nord::{install, nord_style, semantic};

    /// 배경 회귀 방지 — 창/패널/스테이지 배경은 Nord 팔레트의 **불투명** 색이어야
    /// 한다. egui 기본 다크(근사 검정 `#1B1B1B`)로 되돌아가면 실패한다.
    /// (실측 회귀: freedf-gui 창 배경 `#080808` — docs/freedf-gui-migration.md)
    #[test]
    fn nord_style_uses_opaque_nord_backgrounds() {
        let visuals = nord_style().visuals;
        assert!(visuals.dark_mode, "Nord 테마는 다크 모드 고정");
        assert_eq!(visuals.window_fill.to_array(), [0x2E, 0x34, 0x40, 0xFF]);
        assert_eq!(visuals.panel_fill.to_array(), [0x3B, 0x42, 0x52, 0xFF]);
        assert_eq!(visuals.faint_bg_color.to_array(), [0x4C, 0x56, 0x6A, 0xFF]);
        assert_eq!(visuals.window_fill, semantic::BG_WINDOW);
        assert_eq!(visuals.panel_fill, semantic::BG_PANEL);
        assert_eq!(visuals.faint_bg_color, semantic::BG_MUTED);
    }

    /// 설치기는 라이트/다크 두 테마 모두에 Nord 스타일을 넣고 다크로 고정한다 —
    /// Windows 시스템 테마가 라이트여도 Nord가 나와야 한다(실측: 타이틀바 라이트).
    #[test]
    fn install_applies_nord_to_both_themes() {
        let ctx = egui::Context::default();
        install(&ctx);
        assert_eq!(
            ctx.style_of(egui::Theme::Dark).visuals.panel_fill,
            semantic::BG_PANEL
        );
        assert_eq!(
            ctx.style_of(egui::Theme::Light).visuals.panel_fill,
            semantic::BG_PANEL
        );
    }
}
