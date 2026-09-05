//! # Nord egui theme — semantic tokens + style builder
//!
//! Semantic tokens map the primitive tokens ([`super::tokens`]) to UI roles,
//! then [`nord_style`] assigns them to `egui::Style`/`Visuals` fields.
//!
//! This is a **dark-mode** theme (PDF viewers are for long reading sessions),
//! so the app is locked to dark mode via [`install`].

use eframe::egui;
use egui::{Color32, CornerRadius, FontId, Margin, Stroke, TextStyle, Vec2};

use super::tokens::{colors, spacing, typography};

/// Semantic color tokens — roles derived from the Nord palette.
// The full set is defined for consistency; unused roles stay available.
#[allow(dead_code)]
pub mod semantic {
    use super::{colors, Color32};

    // --- Backgrounds ---------------------------------------------------
    /// Window / canvas surround background.
    pub const BG_WINDOW: Color32 = colors::NORD0;
    /// Panels, toolbars, sidebars, widget surfaces.
    pub const BG_PANEL: Color32 = colors::NORD1;
    /// Hover / inactive widget surfaces.
    pub const BG_SURFACE: Color32 = colors::NORD2;
    /// Muted surfaces, disabled elements, tooltip backgrounds.
    pub const BG_MUTED: Color32 = colors::NORD3;

    // --- Text ----------------------------------------------------------
    /// Faint / weak text (labels, hints).
    pub const TEXT_FAINT: Color32 = colors::NORD4;
    /// Primary text.
    pub const TEXT_PRIMARY: Color32 = colors::NORD5;
    /// Strong / emphasized text.
    pub const TEXT_STRONG: Color32 = colors::NORD6;

    // --- Accents -------------------------------------------------------
    /// Links and interactive emphasis.
    pub const ACCENT_LINK: Color32 = colors::NORD8;
    /// Selection / focus background.
    pub const ACCENT_SELECT: Color32 = colors::NORD9;
    /// Active elements (pressed buttons, toggles).
    pub const ACCENT_ACTIVE: Color32 = colors::NORD10;
    /// Teal accent (secondary emphasis).
    pub const ACCENT_TEAL: Color32 = colors::NORD7;
    /// Error text.
    pub const COLOR_ERROR: Color32 = colors::NORD11;
    /// Warning text.
    pub const COLOR_WARN: Color32 = colors::NORD12;
    /// Highlight accent.
    pub const COLOR_HIGHLIGHT: Color32 = colors::NORD13;
    /// Success text.
    pub const COLOR_SUCCESS: Color32 = colors::NORD14;

    // --- Borders -------------------------------------------------------
    /// Weak border (widget frames).
    pub const BORDER_WEAK: Color32 = colors::NORD3;
    /// Window outline — 약간 더 또렷한 경계 (창↔패널 계층 구분).
    pub const BORDER_WINDOW: Color32 = colors::NORD2;

    // --- Page ----------------------------------------------------------
    /// The PDF page background stays white even in dark mode.
    pub const PAGE_BG: Color32 = colors::PAGE;
    /// Canvas surround behind the page.
    pub const PAGE_SURROUND: Color32 = colors::NORD0;

    // --- Floating overlay (canvas nav bar) -----------------------------
    /// Semi-transparent panel background for the floating nav bar.
    pub fn overlay_bg() -> Color32 {
        Color32::from_rgba_unmultiplied(
            colors::NORD1.r(),
            colors::NORD1.g(),
            colors::NORD1.b(),
            210,
        )
    }
    /// Border for the floating nav bar.
    pub const OVERLAY_BORDER: Color32 = colors::NORD3;
}

/// Builds the complete Nord `egui::Style`.
pub fn nord_style() -> egui::Style {
    let mut style = egui::Style::default();
    style.animation_time = 0.25; // smooth hover / selection transitions

    // --- Typography: 1rem base scale -----------------------------------
    style.text_styles.insert(
        TextStyle::Small,
        FontId::new(typography::SMALL, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Body,
        FontId::new(typography::BODY, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Button,
        FontId::new(typography::BUTTON, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(typography::HEADING, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Monospace,
        FontId::new(typography::MONO, egui::FontFamily::Monospace),
    );

    // --- Spacing: roomy, reading-friendly --------------------------------
    style.spacing.item_spacing = Vec2::new(spacing::ITEM_X, spacing::ITEM_Y);
    style.spacing.window_margin = Margin::same(spacing::WINDOW_MARGIN);
    style.spacing.button_padding = Vec2::new(spacing::BUTTON_PAD.0, spacing::BUTTON_PAD.1);
    style.spacing.interact_size = Vec2::new(0.0, spacing::INTERACT_H);
    style.spacing.icon_width = spacing::ICON;
    style.spacing.icon_width_inner = spacing::ICON_INNER;
    style.spacing.slider_width = spacing::SLIDER_W;
    style.spacing.slider_rail_height = spacing::SLIDER_RAIL_H;

    // --- Scrollbars: 얇은 플로팅 스타일 (툴바 가로 스크롤 포함) ---------
    // 평소에는 2pt 두께의 얇은 바, 호버하면 8pt로 부드럽게 확장됩니다.
    style.spacing.scroll = egui::style::ScrollStyle {
        floating: true,
        bar_width: 8.0,               // 호버 시 최대 두께
        floating_width: 2.0,          // 평소(비호버) 얇은 두께
        floating_allocated_width: 4.0, // 항상 얇은 바가 보이도록 4pt 할당
        handle_min_length: 20.0,
        foreground_color: false,
        dormant_background_opacity: 0.0,
        active_background_opacity: 0.12,
        interact_background_opacity: 0.2,
        dormant_handle_opacity: 0.35,
        active_handle_opacity: 0.7,
        interact_handle_opacity: 1.0,
        ..egui::style::ScrollStyle::floating()
    };

    // --- Visuals: Nord (dark) --------------------------------------------
    style.visuals = nord_visuals();
    style
}

/// Nord `Visuals` (dark mode).
fn nord_visuals() -> egui::Visuals {
    use semantic::*;
    // 창/메뉴만 더 둥글게 (8px) — 위젯(버튼 등)은 기존 4px 그대로.
    let radius = CornerRadius::same(8);
    egui::Visuals {
        dark_mode: true,
        hyperlink_color: ACCENT_LINK,
        faint_bg_color: BG_MUTED,
        extreme_bg_color: BG_WINDOW,
        code_bg_color: BG_PANEL,
        warn_fg_color: COLOR_WARN,
        error_fg_color: COLOR_ERROR,
        window_fill: BG_WINDOW,
        panel_fill: BG_PANEL,
        window_stroke: Stroke::new(1.0, BORDER_WINDOW),
        widgets: egui::style::Widgets {
            noninteractive: widget(BG_PANEL, TEXT_FAINT),
            inactive: widget(BG_SURFACE, TEXT_PRIMARY),
            hovered: widget(BG_MUTED, TEXT_STRONG),
            active: widget(ACCENT_ACTIVE, TEXT_STRONG),
            open: widget(BG_SURFACE, TEXT_PRIMARY),
        },
        selection: egui::style::Selection {
            bg_fill: ACCENT_SELECT,
            stroke: Stroke::new(1.0, ACCENT_ACTIVE),
        },
        // 창 스타일 — 공식 API (Visuals::window_*):
        // - 더 부드러운 그림자 (깊이감)
        // - 최상위 창 제목 강조
        // - 터치/펜용 리사이즈 손잡이 확대
        window_shadow: shadow(110, 6, 18),
        popup_shadow: shadow(90, 4, 14),
        window_corner_radius: radius,
        menu_corner_radius: radius,
        window_highlight_topmost: true,
        resize_corner_size: 16.0,
        ..Default::default()
    }
}

/// Widget visuals for a given background + foreground role.
fn widget(bg: Color32, fg: Color32) -> egui::style::WidgetVisuals {
    egui::style::WidgetVisuals {
        bg_fill: bg,
        weak_bg_fill: bg,
        bg_stroke: Stroke::new(1.0, semantic::BORDER_WEAK),
        fg_stroke: Stroke::new(1.0, fg),
        corner_radius: CornerRadius::same(spacing::CORNER_RADIUS),
        expansion: 0.0,
    }
}

/// Soft drop shadow from black at the given alpha.
fn shadow(alpha: u8, offset_y: i8, blur: u8) -> egui::epaint::Shadow {
    egui::epaint::Shadow {
        color: Color32::from_black_alpha(alpha),
        offset: [0, offset_y],
        blur,
        spread: 0,
    }
}

/// Applies the Nord theme and locks the app to dark mode.
pub fn install(ctx: &egui::Context) {
    // PDF reading is best on dark — lock to the dark theme.
    ctx.set_theme(egui::ThemePreference::Dark);
    let style = nord_style();
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        ctx.style_mut_of(theme, |s| *s = style.clone());
    }
}
