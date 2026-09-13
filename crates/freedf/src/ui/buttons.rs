//! Three-tier button system (Bootstrap `btn-variant` mapping).
//!
//! Tiers:
//! - [`ButtonKind::Primary`]   -> `.btn-primary`   filled, emphasized CTA
//! - [`ButtonKind::Secondary`] -> `.btn-secondary` default / neutral
//! - [`ButtonKind::Ghost`]     -> `.btn-ghost/text` frame-less, subtle
//! Plus modifiers: `danger`, `selected`, `enabled`, sizes, optional icon.
//!
//! Every variant returns an `egui::Response`, so callers decide the handler:
//! `if need(b1).clicked() { .. }`.

#![allow(dead_code)] // pre-built kit; wired per call-site as features land.

use eframe::egui;
use egui_phosphor_icons::Icon;

/// Visual tier (Bootstrap `.btn-*`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Primary,
    Secondary,
    Ghost,
}

/// Relative control size.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ButtonSize {
    Small,
    Medium,
    Large,
}

/// Returns a text color with sufficient contrast against `bg`.
fn on_color(bg: egui::Color32) -> egui::Color32 {
    let [r, g, b, _] = bg.to_array();
    let lum = (0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32) / 255.0;
    if lum > 0.6 {
        egui::Color32::BLACK
    } else {
        egui::Color32::WHITE
    }
}

/// Button builder — `Button::primary("Save").icon(..).hint(..).show(ui)`.
pub struct Button<'a> {
    kind: ButtonKind,
    size: ButtonSize,
    icon: Option<Icon>,
    label: &'a str,
    hint: String,
    enabled: bool,
    selected: bool,
    danger: bool,
    frame: bool,
}

impl<'a> Button<'a> {
    pub fn primary(label: &'a str) -> Self {
        Self::base(ButtonKind::Primary, label)
    }
    pub fn secondary(label: &'a str) -> Self {
        Self::base(ButtonKind::Secondary, label)
    }
    pub fn ghost(label: &'a str) -> Self {
        Self::base(ButtonKind::Ghost, label)
    }

    fn base(kind: ButtonKind, label: &'a str) -> Self {
        Self {
            kind,
            size: ButtonSize::Medium,
            icon: None,
            label,
            hint: String::new(),
            enabled: true,
            selected: false,
            danger: false,
            frame: kind != ButtonKind::Ghost,
        }
    }

    pub fn icon(mut self, ic: Icon) -> Self {
        self.icon = Some(ic);
        self
    }
    pub fn size(mut self, s: ButtonSize) -> Self {
        self.size = s;
        self
    }
    pub fn hint(mut self, h: impl Into<String>) -> Self {
        self.hint = h.into();
        self
    }
    pub fn enabled(mut self, on: bool) -> Self {
        self.enabled = on;
        self
    }
    pub fn selected(mut self, on: bool) -> Self {
        self.selected = on;
        self
    }
    pub fn danger(mut self, on: bool) -> Self {
        self.danger = on;
        self
    }
    pub fn framed(mut self, on: bool) -> Self {
        self.frame = on;
        self
    }

    pub fn show(self, ui: &mut egui::Ui) -> egui::Response {
        let (mut fill, mut custom_text) = match self.kind {
            ButtonKind::Primary => {
                let f = ui.visuals().selection.bg_fill;
                (Some(f), Some(on_color(f)))
            }
            ButtonKind::Secondary => (None, None),
            ButtonKind::Ghost => (None, None),
        };
        if self.danger {
            let f = ui.visuals().error_fg_color.gamma_multiply(0.85);
            fill = Some(f);
            custom_text = Some(egui::Color32::WHITE);
        }

        let font = egui::FontId::proportional(match self.size {
            ButtonSize::Small => 12.0,
            ButtonSize::Medium => 14.0,
            ButtonSize::Large => 16.0,
        });

        let text = match self.icon {
            Some(ic) => crate::app::icon_text(ui, self.label, ic),
            None => match custom_text {
                Some(c) => egui::WidgetText::from(
                    egui::RichText::new(self.label).color(c).font(font),
                ),
                None => egui::WidgetText::from(
                    egui::RichText::new(self.label).font(font),
                ),
            },
        };

        let mut b = egui::Button::new(text);
        if let Some(f) = fill {
            b = b.fill(f);
        }
        if self.selected {
            b = b.selected(true);
        }
        if !self.frame {
            b = b.frame(false);
        }
        ui.add_enabled(self.enabled, b)
            .on_hover_text(self.hint)
    }
}

/// Terminal helpers — one-liners for the common cases.
pub fn btn_primary(ui: &mut egui::Ui, label: &str) -> egui::Response {
    Button::primary(label).show(ui)
}
pub fn btn_secondary(ui: &mut egui::Ui, label: &str) -> egui::Response {
    Button::secondary(label).show(ui)
}
pub fn btn_ghost(ui: &mut egui::Ui, label: &str) -> egui::Response {
    Button::ghost(label).show(ui)
}