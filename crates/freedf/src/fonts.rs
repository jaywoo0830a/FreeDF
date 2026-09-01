//! Font setup — bundle PT Serif as the app's single UI font, plus Phosphor icons.
//!
//! PT Serif is OFL-licensed (ParaType, 2010). We embed the regular face so the
//! app works without external font files; egui synthesizes bold/italic from it.

use eframe::egui;
use std::sync::Arc;

/// PT Serif Regular (TrueType).
pub const PT_SERIF_REGULAR: &[u8] = include_bytes!("../assets/fonts/PT_Serif-Web-Regular.ttf");

/// Replaces egui's default proportional/monospace fonts with PT Serif so the
/// whole UI uses a single font family. The built-in fonts are kept as
/// fallbacks for glyphs PT Serif lacks (emoji, symbols, CJK, etc.), and the
/// Phosphor icon font is registered for toolbar icons.
pub fn install_pt_serif(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    fonts.font_data.insert(
        "pt_serif".to_owned(),
        Arc::new(egui::FontData::from_static(PT_SERIF_REGULAR)),
    );

    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        let mut list = vec!["pt_serif".to_owned()];
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
