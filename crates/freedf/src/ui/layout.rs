//! Layout primitives — a tiny Bootstrap/React-fragment-style layout kit.
//!
//! These are *pure* presentational helpers that only arrange widgets; they
//! never touch app state (props-only). Think:
//! - `HStack`  -> <div className="d-flex flex-row gap-2">
//! - `VStack`  -> <d-flex flex-column gap-2>
//! - `Divider` -> <hr> / vertical rule
//! - `Toolbar`/`ToolbarRow` -> the top ribbon structure
//!
//! Spacing follows an 8px grid (Bootstrap `$spacer` rhythm) so every row lines
//! up and gaps stay consistent across screens/DPIs.

use eframe::egui;

use crate::ui::tokens::space;

/// 8px 그리드 스페이서 (Bootstrap-style `--bs-gutter` rhythm).
/// `ui::tokens::space`의 **별칭**입니다 — 여백 어휘는 tokens 한 곳에서만 정의합니다.
pub const SP_1: f32 = space::SM; // .25rem  — tight
pub const SP_2: f32 = space::MD; // .5rem   — default gutter
#[allow(dead_code)] // reserved spacer in the 8px grid.
pub const SP_3: f32 = space::LG; // .75rem  — generous
#[allow(dead_code)] // reserved spacer in the 8px grid.
pub const SP_4: f32 = space::XL; // 1rem    — section gap

/// Horizontal stack (flex-row) with an explicit gap.
pub fn hstack<R>(ui: &mut egui::Ui, gap: f32, children: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let prev = ui.spacing_mut().item_spacing.x;
    ui.spacing_mut().item_spacing.x = gap;
    let r = children(ui);
    ui.spacing_mut().item_spacing.x = prev;
    r
}

/// Vertical stack (flex-column) with an explicit gap.
pub fn vstack<R>(ui: &mut egui::Ui, gap: f32, children: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let prev = ui.spacing_mut().item_spacing.y;
    ui.spacing_mut().item_spacing.y = gap;
    let r = children(ui);
    ui.spacing_mut().item_spacing.y = prev;
    r
}

/// Vertical divider — used between groups inside a horizontal toolbar row.
pub fn vdivider(ui: &mut egui::Ui) {
    ui.separator();
    ui.add_space(space::XS);
}

/// Horizontal separator — used between stacked toolbar rows.
pub fn hseparator(ui: &mut egui::Ui) {
    ui.add_space(space::SM);
    ui.separator();
    ui.add_space(space::SM);
}

/// A scrolling toolbar row (ribbon). Excess items scroll horizontally instead
/// of overflowing off-screen (matches the previous app-level `toolbar_row`).
pub fn toolbar_row<R>(ui: &mut egui::Ui, salt: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::ScrollArea::horizontal()
        .id_salt(("toolbar_row", salt))
        .auto_shrink([false, true]) // X: 폭 전체, Y: 내용물 높이로 수축(전체 영역 차지 방지)
        .show(ui, |ui| ui.horizontal(|ui| add(ui)).inner)
        .inner
}

/// Left-aligned inline group of controls (children on one horizontal line).
/// This is the basic building block of a toolbar row.
pub fn group<R>(ui: &mut egui::Ui, gap: f32, children: impl FnOnce(&mut egui::Ui) -> R) -> R {
    hstack(ui, gap, children)
}
