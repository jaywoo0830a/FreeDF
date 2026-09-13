//! Generic presentational components (Bootstrap-flavored naming).
//!
//! These are pure render helpers that take props and never mutate app state —
//! the "atoms" of the FreeDF component library:
//! - [`pill`]    -> `<span class="badge">`
//! - [`caption`] -> `<small class="text-muted">`
//! - [`help`]    -> `<div class="form-text">`
//! - [`placeholder`] -> empty-state block
//!
//! Pair them with the layout kit (`crate::ui::layout`) and the composite
//! toolbar in `crate::ui::toolbar` to build screens from small pieces.
//!
//! NOTE: the richer element set (badges / alerts / cards / …) lives in
//! `crate::ui::ds`; this module keeps the light atoms. Unused atoms are kept
//! quiet via `dead_code` until a feature wires them in.

#![allow(dead_code)]

use eframe::egui;
use crate::ui::layout::SP_2;

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
// ---------------------------------------------------------------------------
// Extended element library (Bootstrap / React web-flavor).
// ---------------------------------------------------------------------------

/// Semantic color tone (Bootstrap `.bg-*`/`.text-*` mapping).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Neutral,
    Primary,
    Success,
    Warning,
    Danger,
    Info,
}

impl Tone {
    /// Returns `(foreground, fill)` adapted to the current theme.
    pub fn pair(self, ui: &egui::Ui) -> (egui::Color32, egui::Color32) {
        use crate::theme::nord::semantic::COLOR_SUCCESS;
        let accent = match self {
            Tone::Neutral => ui.visuals().weak_text_color(),
            Tone::Primary => ui.visuals().selection.bg_fill,
            Tone::Success => COLOR_SUCCESS,
            Tone::Warning => ui.visuals().warn_fg_color,
            Tone::Danger => ui.visuals().error_fg_color,
            Tone::Info => ui.visuals().hyperlink_color,
        };
        (accent, accent.gamma_multiply(0.14))
    }
}

/// Small status badge (<span class="badge bg-*">).
pub fn badge(ui: &mut egui::Ui, text: &str, tone: Tone) -> egui::Response {
    let (fg, fill) = tone.pair(ui);
    egui::Frame::new()
        .fill(fill)
        .corner_radius(8.0)
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| ui.label(egui::RichText::new(text).color(fg).small()))
        .inner
}

/// Removable tag — returns `true` when the × is clicked.
pub fn tag(ui: &mut egui::Ui, text: &str, tone: Tone) -> bool {
    let (fg, fill) = tone.pair(ui);
    let mut removed = false;
    egui::Frame::new()
        .fill(fill)
        .corner_radius(10.0)
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(egui::RichText::new(text).color(fg).small());
                if ui.small_button("x").clicked() {
                    removed = true;
                }
            });
        });
    removed
}

/// Alert / callout box (<div class="alert alert-*">).
pub fn alert(ui: &mut egui::Ui, tone: Tone, title: &str, message: &str) {
    let (fg, fill) = tone.pair(ui);
    egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, fg.gamma_multiply(0.6)))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(title).strong().color(fg));
                if !message.is_empty() {
                    ui.label(egui::RichText::new(message).weak().small());
                }
            });
        });
}

/// Progress bar (`<div class="progress">`).
pub fn progress(ui: &mut egui::Ui, value: f32, tone: Tone) -> egui::Response {
    let (fg, _) = tone.pair(ui);
    ui.add(egui::ProgressBar::new(value.clamp(0.0, 1.0)).fill(fg))
        .on_hover_text(format!("{:.0}%", value.clamp(0.0, 1.0) * 100.0))
}

/// Loading spinner (egui built-in, wrapped).
pub fn spinner(ui: &mut egui::Ui) -> egui::Response {
    ui.spinner()
}

/// A colored status dot + optional label (connected/offline, etc.).
pub fn status_dot(ui: &mut egui::Ui, tone: Tone, label: &str) {
    let (fg, _) = tone.pair(ui);
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
        ui.painter().circle_filled(rect.center(), 4.0, fg);
        if !label.is_empty() {
            ui.label(egui::RichText::new(label).small());
        }
    });
}
/// Keyboard key styling (<kbd>).
pub fn kbd(ui: &mut egui::Ui, text: &str) -> egui::Response {
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .corner_radius(4.0)
        .inner_margin(egui::Margin::symmetric(5, 2))
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(text).monospace().small().strong(),
            )
        })
        .inner
}

/// Inline code snippet with an optional copy button.
/// Returns `true` when the copy button was clicked.
pub fn code(ui: &mut egui::Ui, text: &str, copyable: bool) -> bool {
    let mut copied = false;
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .corner_radius(4.0)
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(text).monospace().small());
                if copyable
                    && ui
                        .small_button("copy")
                        .on_hover_text("Copy to clipboard")
                        .clicked()
                {
                    ui.ctx().copy_text(text.to_owned());
                    copied = true;
                }
            });
        });
    copied
}

/// Breadcrumb path (Home / Notes / Sub).
pub fn breadcrumb(ui: &mut egui::Ui, parts: &[&str]) {
    ui.horizontal(|ui| {
        for (i, part) in parts.iter().enumerate() {
            if i > 0 {
                ui.label(egui::RichText::new("/").weak().small());
            }
            let last = i == parts.len() - 1;
            let t = if last {
                egui::RichText::new(*part).small().strong().color(ui.visuals().strong_text_color())
            } else {
                egui::RichText::new(*part).small().color(ui.visuals().weak_text_color())
            };
            ui.label(t);
        }
    });
}

/// Centered separator with a label: ─── Label ─── .
pub fn divider_label(ui: &mut egui::Ui, text: &str) {
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        ui.separator();
        ui.label(egui::RichText::new(text).weak().small());
        ui.separator();
        ui.add_space(4.0);
    });
}

/// Circular avatar with the first character of `text`.
pub fn avatar(ui: &mut egui::Ui, text: &str, tone: Tone) {
    let (fg, fill) = tone.pair(ui);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
    let p = ui.painter();
    p.circle_filled(rect.center(), 12.0, fill);
    let ch = text.chars().next().map(|c| c.to_string()).unwrap_or_default();
    let galley = p.layout_no_wrap(
        ch,
        egui::FontId::proportional(12.0),
        fg,
    );
    let pos = rect.center() - galley.size() / 2.0;
    p.galley(pos, galley, fg);
}

/// Count pill (unread counts etc.).
pub fn count(ui: &mut egui::Ui, n: usize, tone: Tone) -> egui::Response {
    badge(ui, &format!("{n}"), tone)
}

/// Horizontal tab bar — updates `*selected` on click.
pub fn tabs(ui: &mut egui::Ui, selected: &mut usize, labels: &[&str]) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (i, label) in labels.iter().enumerate() {
            if ui.selectable_label(*selected == i, *label).clicked() {
                *selected = i;
            }
        }
    });
}

/// A titled card box (layout::Container + optional header).
pub fn card<R>(
    ui: &mut egui::Ui,
    title: Option<&str>,
    tone: Tone,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let (fg, fill) = tone.pair(ui);
    egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, fg.gamma_multiply(0.5)))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::symmetric(10, 8))
        .show(ui, |ui| {
            if let Some(t) = title {
                ui.label(egui::RichText::new(t).strong().color(fg));
            }
            content(ui)
        })
        .inner
}
}

/// probe
pub fn __probe_ok() {}