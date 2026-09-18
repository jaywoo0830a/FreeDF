//! Design-system element kit — color tones, badges, alerts, cards, keyboard,
//! code, tabs.
//!
//! Kept small and self-contained (no cross-module props) so it can be reused
//! anywhere. Colors follow the `Tone` semantic palette.

#![allow(dead_code)] // Design-system kit: unused elements stay quiet until wired in.

use eframe::egui;

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
        .inner_margin(crate::ui::tokens::margin::BADGE)
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).color(fg).small())
        })
        .inner
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

/// Alert / callout box (<div class="alert alert-*">).
pub fn alert(ui: &mut egui::Ui, tone: Tone, title: impl Into<String>, message: impl Into<String>) {
    let title = title.into();
    let message = message.into();
    let (fg, fill) = tone.pair(ui);
    egui::Frame::new()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, fg.gamma_multiply(0.6)))
        .corner_radius(6.0)
        .inner_margin(crate::ui::tokens::margin::CARD)
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(title).strong().color(fg));
                if !message.is_empty() {
                    ui.label(egui::RichText::new(message).weak().small());
                }
            });
        });
}

/// Keyboard key styling (<kbd>).
pub fn kbd(ui: &mut egui::Ui, text: &str) -> egui::Response {
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .corner_radius(4.0)
        .inner_margin(crate::ui::tokens::margin::KBD)
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).monospace().small().strong())
        })
        .inner
}

/// A titled card box.
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
        .inner_margin(crate::ui::tokens::margin::CARD)
        .show(ui, |ui| {
            if let Some(t) = title {
                ui.label(egui::RichText::new(t).strong().color(fg));
            }
            content(ui)
        })
        .inner
}
