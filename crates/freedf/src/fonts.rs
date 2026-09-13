//! Font setup — bundle **Asta Sans** (default UI + Hangul / CJK) with
//! **Inter** + **NanumGothic** and **Phosphor** icons.
//!
//! Asta Sans is the app's default proportional font (covers Latin AND Hangul,
//! variable weights 300–800, mirrored from Google Fonts); NanumGothic stays as
//! a Hangul/CJK fallback and Inter as a Latin fallback. egui synthesizes bold/
//! italic from the embedded regular face; the built-in fonts remain fallbacks
//! for glyphs none of these cover (emoji, symbols, etc.).

use eframe::egui;
use std::sync::Arc;

/// Asta Sans Regular (TrueType, Google Fonts) — default UI + Hangul font.
pub const ASTA_SANS_REGULAR: &[u8] = include_bytes!("../assets/fonts/AstaSans-Regular.ttf");

/// Keep Inter (Latin UI) as a compact Latin fallback.
pub const INTER_REGULAR: &[u8] = include_bytes!("../assets/fonts/Inter-Regular.ttf");

/// NanumGothic Regular (TrueType) — Hangul / Korean fallback.
pub const NANUM_GOTHIC_REGULAR: &[u8] =
    include_bytes!("../assets/fonts/NanumGothic-Regular.ttf");

/// Replaces egui's default proportional font with **Asta Sans** first (so it
/// becomes the default UI/Hangul font), then falls back to Inter (Latin) and
/// NanumGothic (Hangul/CJK), then the built-in fonts. Monospace keeps a clean
/// Latin face so columnar text stays aligned but still gains Hangul fallback.
pub fn install_inter(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    fonts.font_data.insert(
        "asta_sans".to_owned(),
        Arc::new(egui::FontData::from_static(ASTA_SANS_REGULAR)),
    );
    fonts.font_data.insert(
        "inter".to_owned(),
        Arc::new(egui::FontData::from_static(INTER_REGULAR)),
    );
    fonts.font_data.insert(
        "nanum_gothic".to_owned(),
        Arc::new(egui::FontData::from_static(NANUM_GOTHIC_REGULAR)),
    );

    // Proportional: Asta Sans (default) → Inter (Latin) → Nanum (Hangul) → built-ins
    let mut prop = vec![
        "asta_sans".to_owned(),
        "inter".to_owned(),
        "nanum_gothic".to_owned(),
    ];
    if let Some(existing) = fonts.families.get(&egui::FontFamily::Proportional) {
        for name in existing {
            if !prop.contains(name) {
                prop.push(name.clone());
            }
        }
    }
    fonts
        .families
        .insert(egui::FontFamily::Proportional, prop);

    // Monospace: Inter-ish Latin first (keeps alignment), Asta + Nanum for Hangul.
    let mut mono = vec![
        "inter".to_owned(),
        "asta_sans".to_owned(),
        "nanum_gothic".to_owned(),
    ];
    if let Some(existing) = fonts.families.get(&egui::FontFamily::Monospace) {
        for name in existing {
            if !mono.contains(name) {
                mono.push(name.clone());
            }
        }
    }
    fonts.families.insert(egui::FontFamily::Monospace, mono);

    // Phosphor icon font (uses its own font family)
    egui_phosphor_icons::add_fonts(&mut fonts);

    ctx.set_fonts(fonts);
}
