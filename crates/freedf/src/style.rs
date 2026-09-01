//! FreeDF design system.
//!
//! A single base font size (1rem = 16 px) drives a consistent type scale, a
//! 4 px spacing grid, control sizing, rounded corners, and the dark-brown
//! brand accent. Applied to both dark and light themes.

use eframe::egui;
use egui::{Color32, CornerRadius, FontId, Margin, Stroke, TextStyle, Vec2};

/// Brand color — dark brown.
pub const BRAND: Color32 = Color32::from_rgb(0x4E, 0x34, 0x2E);
/// Lighter brown for dark backgrounds (keeps contrast on dark panels).
pub const BRAND_ON_DARK: Color32 = Color32::from_rgb(0xC8, 0xA9, 0x92);

/// Base font size (1rem).
const BASE: f32 = 16.0;
/// Spacing unit (4 px grid).
const SP: f32 = 4.0;

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

    // --- Spacing: 4 px grid, generous controls -------------------------
    style.spacing.item_spacing = Vec2::new(SP * 2.5, SP * 2.0); // 10, 8
    style.spacing.window_margin = Margin::same(12); // 12
    style.spacing.button_padding = Vec2::new(SP * 3.5, SP * 1.75); // 14, 7
    style.spacing.interact_size = Vec2::new(0.0, SP * 8.5); // min height 34
    style.spacing.indent = SP * 6.0; // 24
    style.spacing.icon_width = 18.0;
    style.spacing.icon_width_inner = 14.0;
    style.spacing.slider_width = 120.0;
    style.spacing.slider_rail_height = 5.0;

    // --- Brand accent (dark brown) --------------------------------------
    let brand = if theme == egui::Theme::Dark {
        BRAND_ON_DARK
    } else {
        BRAND
    };
    let visuals = &mut style.visuals;
    visuals.selection.bg_fill =
        Color32::from_rgba_unmultiplied(brand.r(), brand.g(), brand.b(), 90);
    visuals.selection.stroke = Stroke::new(1.5, brand);
    visuals.hyperlink_color = brand;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, brand);
    visuals.widgets.active.bg_stroke = Stroke::new(1.5, brand);
    visuals.widgets.open.bg_stroke = Stroke::new(1.5, brand);
    visuals.widgets.hovered.fg_stroke = Stroke::new(1.5, brand);
    visuals.widgets.active.fg_stroke = Stroke::new(1.5, brand);

    // --- Consistent rounded corners --------------------------------------
    let radius = CornerRadius::same(8);
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
