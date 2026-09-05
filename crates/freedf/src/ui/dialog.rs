//! 다이얼로그/모달 공용 컴포넌트 — **균형 잡힌 여백과 마진**.
//!
//! 모든 설정 창·확인 모달이 이 컴포넌트를 통과해 동일한 콘텐츠 리듬을
//! 가집니다 (React의 <Dialog>/<Modal> + <Actions> 대응).
//!
//! - [`dialog`]: 부동 설정 창 (리사이즈/스크롤 지원)
//! - [`modal`]: 중앙 고정 모달 (확인·입력)
//! - [`actions`]: 오른쪽 정렬 버튼 행 (주 버튼이 가장 오른쪽)
//! - [`pad`]: 여백 리듬만 적용하는 원시 도우미 (기존 창에 끼워 넣기용)

use eframe::egui;

/// 콘텐츠 좌우 여백 (px).
pub(crate) const PAD_X: f32 = 16.0;
/// 콘텐츠 상하 여백 (px).
pub(crate) const PAD_Y: f32 = 12.0;
/// 위젯 간격 — 다이얼로그 안에서 항상 동일.
pub(crate) const ITEM_SPACING: (f32, f32) = (8.0, 8.0);
/// 모든 다이얼로그/모달의 최소 폭 (0.25rem 그리드, 400px).
pub(crate) const MIN_WIDTH: f32 = 400.0;

/// 부동 설정 창 — 공통 여백 리듬 적용.
pub(crate) fn dialog(
    ctx: &egui::Context,
    open: &mut bool,
    title: &str,
    width: f32,
    resizable: bool,
    scroll: bool,
    content: impl FnOnce(&mut egui::Ui),
) {
    egui::Window::new(title)
        .open(open)
        .resizable(resizable)
        .default_width(width)
        .min_width(MIN_WIDTH)
        .show(ctx, |ui| {
            pad(ui, scroll, content);
        });
}

/// 중앙 고정 모달 — collapsible/resizable 없음, 공통 여백 적용.
pub(crate) fn modal(
    ctx: &egui::Context,
    title: &str,
    width: f32,
    content: impl FnOnce(&mut egui::Ui),
) {
    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .default_width(width)
        .min_width(MIN_WIDTH)
        .show(ctx, |ui| {
            pad(ui, false, content);
        });
}

/// 오른쪽 정렬 액션 행 — OK/Cancel 등. 주 버튼을 **마지막**에 넘기면
/// 가장 오른쪽에 놓입니다 (오른쪽 정렬 레이아웃).
pub(crate) fn actions<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.add_space(8.0);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 0.0);
        add(ui)
    })
    .inner
}

/// 균형 잡힌 콘텐츠 리듬 — 여백/간격을 한 곳에서 정의합니다.
/// (기존 창 콘텐츠에 그대로 끼워 넣을 수 있는 원시 도우미.)
pub(crate) fn pad<R>(
    ui: &mut egui::Ui,
    scroll: bool,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    ui.spacing_mut().item_spacing = egui::vec2(ITEM_SPACING.0, ITEM_SPACING.1);
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(PAD_X as i8, PAD_Y as i8))
        .show(ui, |ui| {
            if scroll {
                egui::ScrollArea::vertical().show(ui, content).inner
            } else {
                content(ui)
            }
        })
        .inner
}
