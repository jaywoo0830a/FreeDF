//! Font setup — bundle **Inter** (Latin UI) + **NanumGothic** (Hangul / CJK)
//! as the app's fonts, plus Phosphor icons.
//!
//! Inter is OFL-licensed (The Inter Project Authors); NanumGothic is
//! OFL-licensed (NAVER Corp.). We embed the regular faces so the app works
//! without external font files; egui synthesizes bold/italic from them.

use eframe::egui;
use std::sync::Arc;

/// Inter Regular (TrueType) — Latin UI font.
pub const INTER_REGULAR: &[u8] = include_bytes!("../assets/fonts/Inter-Regular.ttf");

/// NanumGothic Regular (TrueType) — Hangul / Korean fallback.
pub const NANUM_GOTHIC_REGULAR: &[u8] =
    include_bytes!("../assets/fonts/NanumGothic-Regular.ttf");

/// Replaces egui's default proportional/monospace fonts with Inter, then adds
/// NanumGothic as a fallback so Hangul (and other CJK) glyphs render correctly
/// (e.g. Korean PDF outlines / note titles). The built-in fonts stay as
/// fallbacks for glyphs both fonts lack (emoji, symbols, etc.), and the
/// Phosphor icon font is registered for toolbar icons.
pub fn install_inter(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    fonts.font_data.insert(
        "inter".to_owned(),
        Arc::new(egui::FontData::from_static(INTER_REGULAR)),
    );
    fonts.font_data.insert(
        "nanum_gothic".to_owned(),
        Arc::new(egui::FontData::from_static(NANUM_GOTHIC_REGULAR)),
    );

    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        // Inter first (Latin), NanumGothic second (Hangul / CJK fallback).
        let mut list = vec!["inter".to_owned(), "nanum_gothic".to_owned()];
        if let Some(existing) = fonts.families.get(&family) {
            for name in existing {
                if !list.contains(name) {
                    list.push(name.clone());
                }
            }
        }
        fonts.families.insert(family, list);
    }

    // Phosphor icon font (uses its own font family)
    egui_phosphor_icons::add_fonts(&mut fonts);

    ctx.set_fonts(fonts);
}
