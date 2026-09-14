//! 컴포넌트 갤러리 — 키트의 모든 컴포넌트를 **모든 변형/상태**로 한 화면에 그립니다.
//!
//! ## 왜 필요한가 (테스트 전용 훅)
//! * **시각 리뷰**: 사람/에이전트가 한 장의 스크린샷으로 컴포넌트 전체를 봅니다.
//! * **자동 스캔**: 모든 컴포넌트가 `gallery.*` id로 계측되므로 스크립트가
//!   "최소 타깃을 지키는가 / 눌리는가 / 상태가 반영되는가"를 한 번에 검증합니다
//!   (`smoketest/40_ui_gallery.luau`).
//! * **계약 검증**: 그리는 순간 [`crate::ui::a11y`]가 위반을 수집하므로,
//!   갤러리를 렌더링하는 것만으로 컴포넌트 계약 위반을 찾을 수 있습니다.
//!
//! 이 창은 `dev-automation` 빌드에서만 존재합니다(제품 UI에 노출되지 않음).

use eframe::egui;
use egui_phosphor_icons::icons;

use crate::ui::kit::{self, Button, IconButton, Row, Segment, Segmented, Size, Toggle};
use crate::ui::tokens;

/// 갤러리가 조작하는 상태 — 자동화가 값을 바꾸고 반영을 확인합니다.
pub struct Gallery {
    pub open: bool,
    toggle_inline: bool,
    toggle_row: bool,
    segment: usize,
    clicks: u32,
}

impl Default for Gallery {
    fn default() -> Self {
        Self::new()
    }
}

impl Gallery {
    pub fn new() -> Self {
        Self {
            open: false,
            toggle_inline: false,
            toggle_row: true,
            segment: 1,
            clicks: 0,
        }
    }

    /// 갤러리 창을 그립니다(`open`이 참일 때만).
    pub fn show(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        egui::Window::new("UI Gallery — 컴포넌트 계약")
            .open(&mut open)
            .default_width(560.0)
            .vscroll(true)
            .show(ctx, |ui| self.contents(ui));
        self.open = open;
    }
}

impl Gallery {
    /// 갤러리 내용(창 없이) — 헤드리스 계약 테스트가 직접 호출합니다.
    pub(crate) fn contents(&mut self, ui: &mut egui::Ui) {
        // ── Button: 변형 ───────────────────────────────────────────
        kit::section_label(ui, "Button — 변형 (variant)");
        ui.horizontal(|ui| {
            Button::primary("Primary")
                .test_id("gallery.button.primary")
                .hint("주 행동 — 화면에 하나")
                .show(ui);
            Button::secondary("Secondary")
                .test_id("gallery.button.secondary")
                .show(ui);
            Button::ghost("Ghost")
                .test_id("gallery.button.ghost")
                .show(ui);
            Button::danger("Danger")
                .test_id("gallery.button.danger")
                .show(ui);
        });

        // ── Button: 상태 ───────────────────────────────────────────
        kit::section_label(ui, "Button — 상태 (state)");
        ui.horizontal(|ui| {
            Button::secondary("Disabled")
                .test_id("gallery.button.disabled")
                .enabled(false)
                .show(ui);
            Button::secondary("Selected")
                .test_id("gallery.button.selected")
                .selected(true)
                .show(ui);
            Button::secondary("With icon")
                .icon(icons::FLOPPY_DISK)
                .test_id("gallery.button.icon")
                .show(ui);
        });

        // ── Button: 크기 ───────────────────────────────────────────
        kit::section_label(ui, "Button — 크기 (size)");
        ui.horizontal(|ui| {
            Button::secondary("Small ≥24")
                .size(Size::Small)
                .test_id("gallery.button.size.small")
                .show(ui);
            Button::secondary("Medium ≥28")
                .size(Size::Medium)
                .test_id("gallery.button.size.medium")
                .show(ui);
            Button::secondary("Touch ≥32")
                .size(Size::Touch)
                .test_id("gallery.button.size.touch")
                .show(ui);
        });


        // ── IconButton: 이름이 필수인 아이콘 전용 ───────────────────
        kit::section_label(ui, "IconButton — 아이콘 전용 (이름 필수)");
        ui.horizontal(|ui| {
            IconButton::new(icons::GEAR, "Settings")
                .test_id("gallery.icon_button.small")
                .show(ui);
            IconButton::new(icons::MAGNIFYING_GLASS, "Find")
                .size(Size::Medium)
                .test_id("gallery.icon_button.medium")
                .show(ui);
            IconButton::new(icons::PEN, "Pen tool")
                .size(Size::Touch)
                .test_id("gallery.icon_button.touch")
                .show(ui);
            IconButton::new(icons::TRASH, "Delete")
                .enabled(false)
                .test_id("gallery.icon_button.disabled")
                .show(ui);
        });

        // ── Toggle: 인라인 + 행 ────────────────────────────────────
        kit::section_label(ui, "Toggle — 인라인 · 행");
        ui.horizontal(|ui| {
            Toggle::new(&mut self.toggle_inline, "Inline toggle")
                .icon(icons::LIGHTNING)
                .test_id("gallery.toggle.inline")
                .show(ui);
        });
        let _ = Row::toggle("gallery.toggle.row", "Row toggle", &mut self.toggle_row)
            .icon(icons::EYE)
            .hint("행 전체가 타깃입니다")
            .show(ui, |_| ());

        // ── Segmented ──────────────────────────────────────────────
        kit::section_label(ui, "Segmented — 배타 선택");
        let picked = Segmented::new(
            vec![
                Segment::new("gallery.segment.left", "Left").icon(icons::TEXT_ALIGN_LEFT),
                Segment::new("gallery.segment.center", "Center").icon(icons::TEXT_ALIGN_CENTER),
                Segment::new("gallery.segment.right", "Right").icon(icons::TEXT_ALIGN_RIGHT),
            ],
            self.segment,
        )
        .show(ui);
        if let Some(i) = picked {
            self.segment = i;
        }

        // ── Row: 액션 · 라디오 · 트레일링 · 비활성 ─────────────────
        kit::section_label(ui, "Row — 액션 · 라디오 · 트레일링 · 비활성");
        let action = Row::action("gallery.row.action", "Action row")
            .icon(icons::LIGHTNING)
            .show(ui, |_| ())
            .0;
        if action.clicked() {
            self.clicks += 1;
        }
        let _ = Row::radio("gallery.row.radio", "Radio row", self.segment == 0)
            .icon(icons::MARKER_CIRCLE)
            .show(ui, |_| ());
        let _ = Row::action("gallery.row.trailing", "Row with trailing control")
            .icon(icons::GEAR)
            .trailing_width(tokens::target::COMFORT)
            .show(ui, |ui| {
                IconButton::new(icons::QUESTION, "Row help")
                    .test_id("gallery.row.trailing.button")
                    .show(ui);
            });
        let _ = Row::action("gallery.row.disabled", "Disabled row")
            .icon(icons::X)
            .enabled(false)
            .show(ui, |_| ());

        // ── 상태 노출 (자동화가 반영 여부를 확인) ───────────────────
        kit::section_label(ui, "상태 (자동화 확인용)");
        // 위반이 있으면 화면에서 바로 보이게 합니다 — 스크린샷 리뷰에서 즉시 발견.
        let issues = crate::ui::a11y::issues();
        if issues.is_empty() {
            ui.label(
                egui::RichText::new(format!(
                    "clicks={} segment={} inline={} row={} · a11y 위반 없음",
                    self.clicks, self.segment, self.toggle_inline, self.toggle_row
                ))
                .weak()
                .small(),
            );
        } else {
            ui.label(
                egui::RichText::new(crate::ui::a11y::report())
                    .color(crate::theme::nord::semantic::COLOR_ERROR),
            );
        }
    }
}

