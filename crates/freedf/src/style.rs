//! FreeDF design system.
//!
//! Typography (1rem base), a 4 px spacing grid, rounded corners, and the
//! **Catppuccin** pastel palette — Mocha for the dark theme, Latte for the
//! light theme.
//!
//! This replicates the visuals of `catppuccin-egui` v5.7.0 exactly, but is
//! implemented locally because that crate only supports egui ≤ 0.33 while
//! FreeDF uses egui 0.36 (there is no `egui36` feature).

use eframe::egui;
use egui::{Color32, CornerRadius, FontId, Margin, Stroke, TextStyle, Vec2};

/// Base font size (1rem).
const BASE: f32 = 16.0;
/// Spacing unit (4 px grid).
const SP: f32 = 4.0;

/// One Catppuccin flavor (26 colors).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub rosewater: Color32,
    pub flamingo: Color32,
    pub pink: Color32,
    pub mauve: Color32,
    pub red: Color32,
    pub maroon: Color32,
    pub peach: Color32,
    pub yellow: Color32,
    pub green: Color32,
    pub teal: Color32,
    pub sky: Color32,
    pub sapphire: Color32,
    pub blue: Color32,
    pub lavender: Color32,
    pub text: Color32,
    pub subtext1: Color32,
    pub subtext0: Color32,
    pub overlay2: Color32,
    pub overlay1: Color32,
    pub overlay0: Color32,
    pub surface2: Color32,
    pub surface1: Color32,
    pub surface0: Color32,
    pub base: Color32,
    pub mantle: Color32,
    pub crust: Color32,
}

const fn c32(r: u8, g: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, g, b)
}

/// Catppuccin **Mocha** — used for the dark theme.
pub const MOCHA: Palette = Palette {
    rosewater: c32(0xF5, 0xE0, 0xDC),
    flamingo: c32(0xF2, 0xCD, 0xCD),
    pink: c32(0xF5, 0xC2, 0xE7),
    mauve: c32(0xCB, 0xA6, 0xF7),
    red: c32(0xF3, 0x8B, 0xA8),
    maroon: c32(0xEB, 0xA0, 0xAC),
    peach: c32(0xFA, 0xB3, 0x87),
    yellow: c32(0xF9, 0xE2, 0xAF),
    green: c32(0xA6, 0xE3, 0xA1),
    teal: c32(0x94, 0xE2, 0xD5),
    sky: c32(0x89, 0xDC, 0xEB),
    sapphire: c32(0x74, 0xC7, 0xEC),
    blue: c32(0x89, 0xB4, 0xFA),
    lavender: c32(0xB4, 0xBE, 0xFE),
    text: c32(0xCD, 0xD6, 0xF4),
    subtext1: c32(0xBA, 0xC2, 0xDE),
    subtext0: c32(0xA6, 0xAD, 0xC8),
    overlay2: c32(0x93, 0x99, 0xB2),
    overlay1: c32(0x7F, 0x84, 0x9C),
    overlay0: c32(0x6C, 0x70, 0x86),
    surface2: c32(0x58, 0x5B, 0x70),
    surface1: c32(0x45, 0x47, 0x5A),
    surface0: c32(0x31, 0x32, 0x44),
    base: c32(0x1E, 0x1E, 0x2E),
    mantle: c32(0x18, 0x18, 0x25),
    crust: c32(0x11, 0x11, 0x1B),
};

/// Catppuccin **Latte** — used for the light theme.
pub const LATTE: Palette = Palette {
    rosewater: c32(0xDC, 0x8A, 0x78),
    flamingo: c32(0xDD, 0x78, 0x78),
    pink: c32(0xEA, 0x76, 0xCB),
    mauve: c32(0x88, 0x39, 0xEF),
    red: c32(0xD2, 0x0F, 0x39),
    maroon: c32(0xE6, 0x45, 0x53),
    peach: c32(0xFE, 0x64, 0x0B),
    yellow: c32(0xDF, 0x8E, 0x1D),
    green: c32(0x40, 0xA0, 0x2B),
    teal: c32(0x17, 0x92, 0x99),
    sky: c32(0x04, 0xA5, 0xE5),
    sapphire: c32(0x20, 0x9F, 0xB5),
    blue: c32(0x1E, 0x66, 0xF5),
    lavender: c32(0x72, 0x87, 0xFD),
    text: c32(0x4C, 0x4F, 0x69),
    subtext1: c32(0x5C, 0x5F, 0x77),
    subtext0: c32(0x6C, 0x6F, 0x85),
    overlay2: c32(0x7C, 0x7F, 0x93),
    overlay1: c32(0x8C, 0x8F, 0xA1),
    overlay0: c32(0x9C, 0xA0, 0xB0),
    surface2: c32(0xAC, 0xB0, 0xBE),
    surface1: c32(0xBC, 0xC0, 0xCC),
    surface0: c32(0xCC, 0xD0, 0xDA),
    base: c32(0xEF, 0xF1, 0xF5),
    mantle: c32(0xE6, 0xE9, 0xEF),
    crust: c32(0xDC, 0xE0, 0xE8),
};

/// Applies the design system to both themes.
pub fn install(ctx: &egui::Context) {
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        ctx.style_mut_of(theme, |style| apply(style, theme));
    }
}

fn apply(style: &mut egui::Style, theme: egui::Theme) {
    // --- Typography: 1rem base with a modest scale ---------------------
    style
        .text_styles
        .insert(TextStyle::Small, FontId::new(BASE * 0.875, egui::FontFamily::Proportional)); // 14
    style
        .text_styles
        .insert(TextStyle::Body, FontId::new(BASE, egui::FontFamily::Proportional)); // 16
    style
        .text_styles
        .insert(TextStyle::Button, FontId::new(BASE * 0.9375, egui::FontFamily::Proportional)); // 15
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::new(BASE * 1.25, egui::FontFamily::Proportional)); // 20
    style.text_styles.insert(
        TextStyle::Monospace,
        FontId::new(BASE * 0.9375, egui::FontFamily::Monospace), // 15
    );

    // --- Spacing: 4 px grid, compact controls -------------------------
    style.spacing.item_spacing = Vec2::new(SP * 2.0, SP * 1.5); // 8, 6
    style.spacing.window_margin = Margin::same(8); // 8
    style.spacing.button_padding = Vec2::new(SP * 2.5, SP * 1.25); // 10, 5
    style.spacing.interact_size = Vec2::new(0.0, SP * 7.0); // min height 28
    style.spacing.indent = SP * 4.0; // 16
    style.spacing.icon_width = 18.0;
    style.spacing.icon_width_inner = 14.0;
    style.spacing.slider_width = 100.0;
    style.spacing.slider_rail_height = 5.0;

    // --- Catppuccin visuals (Mocha dark / Latte light) -----------------
    let palette = if theme == egui::Theme::Dark { MOCHA } else { LATTE };
    apply_catppuccin(&mut style.visuals, &palette, theme == egui::Theme::Light);

    // --- Consistent rounded corners --------------------------------------
    let radius = CornerRadius::same(8);
    let visuals = &mut style.visuals;
    for w in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
        &mut visuals.widgets.open,
    ] {
        w.corner_radius = radius;
    }
    visuals.window_corner_radius = radius;
    visuals.menu_corner_radius = radius;
}

/// Replicates `catppuccin_egui::Theme::visuals()` for egui 0.36.
fn apply_catppuccin(v: &mut egui::Visuals, p: &Palette, is_latte: bool) {
    let old = v.clone();
    let shadow_color = if is_latte {
        Color32::from_black_alpha(25)
    } else {
        Color32::from_black_alpha(96)
    };
    *v = egui::Visuals {
        hyperlink_color: p.rosewater,
        faint_bg_color: p.surface0,
        extreme_bg_color: p.crust,
        code_bg_color: p.mantle,
        warn_fg_color: p.peach,
        error_fg_color: p.maroon,
        window_fill: p.base,
        panel_fill: p.base,
        window_stroke: Stroke {
            color: p.overlay1,
            ..old.window_stroke
        },
        widgets: egui::style::Widgets {
            noninteractive: widget_visual(&old.widgets.noninteractive, p, p.base),
            inactive: widget_visual(&old.widgets.inactive, p, p.surface0),
            hovered: widget_visual(&old.widgets.hovered, p, p.surface2),
            active: widget_visual(&old.widgets.active, p, p.surface1),
            open: widget_visual(&old.widgets.open, p, p.surface0),
        },
        selection: egui::style::Selection {
            bg_fill: p.blue.linear_multiply(if is_latte { 0.4 } else { 0.2 }),
            stroke: Stroke {
                color: p.text,
                ..old.selection.stroke
            },
        },
        window_shadow: egui::epaint::Shadow {
            color: shadow_color,
            ..old.window_shadow
        },
        popup_shadow: egui::epaint::Shadow {
            color: shadow_color,
            ..old.popup_shadow
        },
        dark_mode: !is_latte,
        ..old
    };
}

fn widget_visual(old: &egui::style::WidgetVisuals, p: &Palette, bg_fill: Color32) -> egui::style::WidgetVisuals {
    egui::style::WidgetVisuals {
        bg_fill,
        weak_bg_fill: bg_fill,
        bg_stroke: Stroke {
            color: p.overlay1,
            ..old.bg_stroke
        },
        fg_stroke: Stroke {
            color: p.text,
            ..old.fg_stroke
        },
        ..old.clone()
    }
}
