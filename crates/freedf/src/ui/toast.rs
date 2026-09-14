//! Toast notifications — a small, reusable snackbar/toast queue.
//!
//! Rendered top-right as stacked cards, auto-expiring with time and manually
//! dismissible. Prop-free: you push [`Toast`]es, call [`ToastQueue::show`] each
//! frame with the egui context, and it draws + prunes itself.

use eframe::egui;

/// 토스트 스택을 화면 아래에서 띄우는 거리 (pt).
/// 하단 상태바(약 26pt)를 덮지 않도록 그 위에 놓습니다.
const BOTTOM_OFFSET: f32 = 36.0;

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
    /// Stable key (React-style) — 계측 id(`toast.dismiss.<id>`)에도 쓰입니다.
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
    /// 지난 프레임에 그린 카드 사각형 — 포인터가 카드 위에 있는지(=이 레이어가
    /// 입력을 받아야 하는지) 판정하는 데 씁니다.
    card_rects: Vec<egui::Rect>,
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
            card_rects: Vec::new(),
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

    /// Prune expired toasts and render the remaining stack (bottom-right).
    ///
    /// **입력을 가로채지 않습니다.** egui의 [`egui::Area`]는 기본이
    /// `interactable: true`라서 그 사각형이 아래 위젯의 클릭을 전부 삼킵니다 —
    /// 예전에는 우측 상단에 떠서 토스트가 살아 있는 동안 `More`/`Settings`/
    /// `Save Edits`가 눌리지 않았습니다. 이제는 **포인터가 카드 위에 있을 때만**
    /// 레이어를 살리고(`interactable(over_card)`), 그 외에는 egui가 이 레이어를
    /// 히트테스트에서 제외하므로 클릭이 아래(툴바·캔버스)로 그대로 통과합니다.
    pub fn show(&mut self, ctx: &egui::Context) {
        let now_ms = ctx.input(|i| i.time) * 1000.0;
        self.toasts.retain(|t| now_ms - t.created_ms < self.duration_ms);
        if self.toasts.is_empty() {
            self.card_rects.clear();
            return;
        }

        // 지난 프레임의 카드 사각형으로 "입력을 받을지"를 정합니다 (1프레임 지연은
        // 무해합니다: 포인터가 카드로 이동한 다음 프레임에는 이미 살아 있습니다).
        let pointer = ctx.input(|i| i.pointer.hover_pos());
        let over_card = pointer.is_some_and(|p| self.card_rects.iter().any(|r| r.contains(p)));

        let mut dismiss: Option<usize> = None;
        let mut card_rects: Vec<egui::Rect> = Vec::with_capacity(self.toasts.len());
        egui::Area::new(egui::Id::new("toast_area"))
            .anchor(egui::Align2::RIGHT_BOTTOM, [-16.0, -BOTTOM_OFFSET])
            .order(egui::Order::Foreground)
            .interactable(over_card)
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
                for (i, t) in self.toasts.iter().enumerate() {
                    let (fill, border) = t.visuals(ui);
                    let card = egui::Frame::new()
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
                                let close = ui.small_button("x").on_hover_text("Dismiss");
                                crate::app::dev::tag_button(
                                    ui,
                                    format!("toast.dismiss.{}", t.id),
                                    "Dismiss",
                                    &close,
                                );
                                if close.clicked() {
                                    dismiss = Some(i);
                                }
                            });
                        });
                    // 카드 아무 곳이나 눌러도 닫힙니다 (✕를 정확히 겨냥하지 않아도 됨).
                    if over_card
                        && card
                            .response
                            .interact(egui::Sense::click())
                            .on_hover_text("Dismiss")
                            .clicked()
                    {
                        dismiss = Some(i);
                    }
                    card_rects.push(card.response.rect);
                }
                // 계측: 토스트가 어디를 덮고 있는지 스크립트가 측정할 수 있게 공개합니다
                // (이 계측이 없어서 "토스트가 툴바를 가로챈다"는 결함이 보이지 않았습니다).
                crate::app::dev::publish_rect(ui, "toast.stack", ui.min_rect());
            });
        self.card_rects = card_rects;
        if let Some(i) = dismiss {
            self.toasts.remove(i);
        }
    }
}