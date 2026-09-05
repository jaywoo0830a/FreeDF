//! # Primitive design tokens
//!
//! Raw values — the building blocks of the theme. Colors are the official
//! **Nord** 16-color palette; spacing and typography are numeric tokens.
//!
//! Nothing here knows about `egui::Style`/`Visuals`. The semantic tokens in
//! [`super::nord::semantic`] map these primitives to UI roles.

// The full palette is defined as a token library; not every token is consumed
// by the current theme (they stay available for future roles / theming).
#![allow(dead_code)]

use egui::Color32;

/// Raw color tokens — the official Nord palette (16 colors + helpers).
pub mod colors {
    use super::Color32;

    // --- Polar Night (dark neutrals) ---------------------------------
    /// Darkest background — window / canvas surround.
    pub const NORD0: Color32 = Color32::from_rgb(0x2E, 0x34, 0x40);
    /// Slightly lighter background — panels, widgets.
    pub const NORD1: Color32 = Color32::from_rgb(0x3B, 0x42, 0x52);
    /// Mid background — hover / selection surfaces.
    pub const NORD2: Color32 = Color32::from_rgb(0x43, 0x4C, 0x5E);
    /// Lighter background — borders, muted / inactive elements.
    pub const NORD3: Color32 = Color32::from_rgb(0x4C, 0x56, 0x6A);

    // --- Snow Storm (light text) --------------------------------------
    /// Faint text.
    pub const NORD4: Color32 = Color32::from_rgb(0xD8, 0xDE, 0xE9);
    /// Normal text.
    pub const NORD5: Color32 = Color32::from_rgb(0xE5, 0xE9, 0xF0);
    /// Strong / emphasized text.
    pub const NORD6: Color32 = Color32::from_rgb(0xEC, 0xEF, 0xF4);

    // --- Frost (blues / teals) ----------------------------------------
    /// Teal — links, emphasis.
    pub const NORD7: Color32 = Color32::from_rgb(0x8F, 0xBC, 0xBB);
    /// Bright cyan — buttons, actions.
    pub const NORD8: Color32 = Color32::from_rgb(0x88, 0xC0, 0xD0);
    /// Blue — selection, focus.
    pub const NORD9: Color32 = Color32::from_rgb(0x81, 0xA1, 0xC1);
    /// Deep blue — active elements.
    pub const NORD10: Color32 = Color32::from_rgb(0x5E, 0x81, 0xAC);

    // --- Aurora (accents) ---------------------------------------------
    /// Red — errors.
    pub const NORD11: Color32 = Color32::from_rgb(0xBF, 0x61, 0x6A);
    /// Orange — warnings.
    pub const NORD12: Color32 = Color32::from_rgb(0xD0, 0x87, 0x70);
    /// Yellow — highlights.
    pub const NORD13: Color32 = Color32::from_rgb(0xEB, 0xCB, 0x8B);
    /// Green — success.
    pub const NORD14: Color32 = Color32::from_rgb(0xA3, 0xBE, 0x8C);
    /// Purple — misc accents.
    pub const NORD15: Color32 = Color32::from_rgb(0xB4, 0x8E, 0xAD);

    // --- Derived -------------------------------------------------------
    /// The PDF page itself stays white even in dark mode (readability).
    pub const PAGE: Color32 = Color32::WHITE;
    /// Pure black.
    pub const BLACK: Color32 = Color32::BLACK;
}

/// Raw spacing tokens (4 px grid).
pub mod spacing {
    /// Base grid unit.
    pub const UNIT: f32 = 4.0;
    /// Horizontal gap between widgets.
    pub const ITEM_X: f32 = 8.0;
    /// Vertical gap between widgets.
    pub const ITEM_Y: f32 = 8.0;
    /// Window / panel margin.
    pub const WINDOW_MARGIN: i8 = 16;
    /// Button inner padding `(x, y)`.
    pub const BUTTON_PAD: (f32, f32) = (8.0, 4.0);
    /// Widget corner radius.
    pub const CORNER_RADIUS: u8 = 4;
    /// Minimum interactive widget height.
    pub const INTERACT_H: f32 = 28.0;
    /// Slider width.
    pub const SLIDER_W: f32 = 120.0;
    /// Icon size.
    pub const ICON: f32 = 20.0;
    /// Icon inner size.
    pub const ICON_INNER: f32 = 16.0;
    /// Slider rail height.
    pub const SLIDER_RAIL_H: f32 = 4.0;
}

/// Raw typography tokens (1rem = 16 px base).
pub mod typography {
    /// Base size (1rem).
    pub const BASE: f32 = 16.0;
    /// Small text.
    pub const SMALL: f32 = 16.0;
    /// Body text.
    pub const BODY: f32 = 16.0;
    /// Button text.
    pub const BUTTON: f32 = 16.0;
    /// Heading text.
    pub const HEADING: f32 = 20.0;
    /// Monospace text.
    pub const MONO: f32 = 16.0;
}
