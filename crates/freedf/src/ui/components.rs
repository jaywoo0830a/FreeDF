//! Generic presentational components (Bootstrap-flavored naming).
//!
//! These are the **light atoms** of the FreeDF component library — pure render
//! helpers that take props and never mutate app state:
//! - [`pill`]        -> `<span class="badge">`
//! - [`caption`]     -> `<small class="text-muted">`
//! - [`help`]        -> `<div class="form-text">`
//! - [`placeholder`] -> empty-state block
//!
//! Pair them with the layout kit (`crate::ui::layout`) to build screens from
//! small pieces.
//!
//! NOTE: the richer element set (badges / alerts / cards / kbd / tabs / …)
//! lives in `crate::ui::ds` — the single canonical design-system kit. This
//! module deliberately keeps only the light atoms to avoid the duplicated
//! element library that used to live here.

#![allow(dead_code)]

use crate::ui::layout::SP_2;
use eframe::egui;

/// A small status pill / tag.
pub fn pill(ui: &mut egui::Ui, text: &str, selected: bool) -> egui::Response {
    let color = if selected {
        ui.visuals().strong_text_color()
    } else {
        ui.visuals().weak_text_color()
    };
    ui.label(egui::RichText::new(text).color(color).small())
}

/// Caption / group label («small text-muted»).
pub fn caption(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.label(egui::RichText::new(text).weak().small())
}

/// Subtle helper line under a control («form-text»).
pub fn help(ui: &mut egui::Ui, text: &str) -> egui::Response {
    caption(ui, text)
}

/// An empty-state placeholder block.
pub fn placeholder(ui: &mut egui::Ui, text: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space(SP_2);
        ui.label(egui::RichText::new(text).weak().small());
        ui.add_space(SP_2);
    });
}
