//! Toast notifications — a small, reusable snackbar/toast queue.
//!
//! Rendered top-right as stacked cards, auto-expiring with time and manually
//! dismissible. Prop-free: you push [`Toast`]es, call [`ToastQueue::show`] each
//! frame with the egui context, and it draws + prunes itself.

use eframe::egui;

/// Toast severity — drives color and icon.
#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // some severities are not pushed yet (Success/Warning/Danger).
pub enum ToastKind {
    Info,
    Success,
    Warning,
    Danger,
}

/// A single toasts.
pub struct Toast {
    /// Stable key (React-style). Reserved — the render currently iterates by index.
    #[allow(dead_code)]
    pub id: u64,
    pub kind: ToastKind,
    pub title: String,
    pub message: String,
    pub created_ms: f64,
}

impl Toast {
    /// Border/fill colors adapted to the current theme.
    fn visuals(&self, ui: &egui::Ui) -> (egui::Color32, egui::Color32) {
        use crate::theme::nord::semantic::COLOR_SUCCESS;
        let accent = match self.kind {
            ToastKind::Info => ui.visuals().weak_text_color(),
            ToastKind::Success => COLOR_SUCCESS,
            ToastKind::Warning => ui.visuals().warn_fg_color,
            ToastKind::Danger => ui.visuals().error_fg_color,
        };
        (accent.gamma_multiply(0.10), accent.gamma_multiply(0.9))
    }
}

/// A bounded queue of toasts + the render logic.
pub struct ToastQueue {
    pub toasts: Vec<Toast>,
    next_id: u64,
    pub max: usize,
    pub duration_ms: f64,
}

impl Default for ToastQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl ToastQueue {
    pub fn new() -> Self {
        Self {
            toasts: Vec::new(),
            next_id: 0,
            max: 4,
            duration_ms: 4500.0,
        }
    }

    pub fn push(&mut self, kind: ToastKind, title: impl Into<String>, message: impl Into<String>, now_ms: f64) {
        if self.next_id.checked_add(1).is_none() {
            self.next_id = 0;
        }
        self.next_id += 1;
        self.toasts.push(Toast {
            id: self.next_id,
            kind,
            title: title.into(),
            message: message.into(),
            created_ms: now_ms,
        });
        while self.toasts.len() > self.max {
            self.toasts.remove(0);
        }
    }

    /// Clear all toasts.
    #[allow(dead_code)] // reserve API — callers (e.g. success/error flows) will use it.
    pub fn clear(&mut self) {
        self.toasts.clear();
    }

    /// Prune expired toasts and render the remaining stack (top-right).
    pub fn show(&mut self, ctx: &egui::Context) {
        let now_ms = ctx.input(|i| i.time) * 1000.0;
        self.toasts.retain(|t| now_ms - t.created_ms < self.duration_ms);
        if self.toasts.is_empty() {
            return;
        }

        let mut dismiss: Option<usize> = None;
        egui::Area::new(egui::Id::new("toast_area"))
            .anchor(egui::Align2::RIGHT_TOP, [-16.0, 16.0])
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
                for (i, t) in self.toasts.iter().enumerate() {
                    let (fill, border) = t.visuals(ui);
                    egui::Frame::new()
                        .fill(fill)
                        .stroke(egui::Stroke::new(1.0, border))
                        .corner_radius(6.0)
                        .inner_margin(egui::Margin::symmetric(10, 8))
                        .show(ui, |ui| {
                            ui.set_width(280.0);
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(egui::RichText::new(&t.title).strong());
                                    if !t.message.is_empty() {
                                        ui.label(
                                            egui::RichText::new(&t.message).weak().small(),
                                        );
                                    }
                                });
                                if ui.small_button("x").on_hover_text("Dismiss").clicked() {
                                    dismiss = Some(i);
                                }
                            });
                        });
                }
            });
        if let Some(i) = dismiss {
            self.toasts.remove(i);
        }
    }
}