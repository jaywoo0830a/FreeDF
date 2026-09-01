//! Font setup — bundle **Inter** as the app's single UI font, plus Phosphor icons.
//!
//! Inter is OFL-licensed (The Inter Project Authors). We embed the regular face
//! so the app works without external font files; egui synthesizes bold/italic
//! from it.

use eframe::egui;
use std::sync::Arc;

/// Inter Regular (TrueType).
pub const INTER_REGULAR: &[u8] = include_bytes!("../assets/fonts/Inter-Regular.ttf");

/// Replaces egui's default proportional/monospace fonts with Inter so the
/// whole UI uses a single font family. The built-in fonts are kept as
/// fallbacks for glyphs Inter lacks (emoji, symbols, CJK, etc.), and the
/// Phosphor icon font is registered for toolbar icons.
pub fn install_inter(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    fonts.font_data.insert(
        "inter".to_owned(),
        Arc::new(egui::FontData::from_static(INTER_REGULAR)),
    );

    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        let mut list = vec!["inter".to_owned()];
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
