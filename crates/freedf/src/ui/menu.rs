//! 메뉴(오버레이/드롭다운) 행 키트 — React의 `<OverflowMenu>` 계열 컴포넌트.
//!
//! **목적: 모든 메뉴 항목이 같은 무게로 보이게 만드는 것.**
//! 예전 `More` 오버레이는 한 화면에 네 가지 컨트롤이 섞여 있었습니다 —
//! 원시 `ui.checkbox`, 무테두리 텍스트 버튼, `ui.menu_button` 서브메뉴,
//! 그리고 라벨 없는 아이콘 선택 버튼. 그래서 정보 계층이 아니라 나열처럼 보였습니다.
//!
//! 여기서는 행의 **뼈대를 하나로 고정**하고([`menu_row`]) 상태 표현만 골라 씁니다.
//! 트레일링 슬롯이 필요하면 [`menu_row`]를 직접 쓰세요(예: 정렬 3-세그먼트,
//! 그 행 전용 ⚙ 설정 버튼).
//!
//! | 컴포넌트 | 쓰임 | 시각 형태 |
//! |---|---|---|
//! | [`menu_section`] | 섹션 제목 | 구분선 + 굵은 라벨 |
//! | [`menu_toggle_row`] | 켜기/끄기 | [아이콘+라벨] (활성 시 하이라이트) |
//! | [`menu_action_row`] | 실행 | [아이콘+라벨] (호버 하이라이트) |
//! | [`menu_submenu_row`] | 하위 메뉴 | [아이콘+라벨] + `▸` |
//!
//! 프레젠테이션만 담당합니다(상태 없음) — 상태 연결과 계측 id는 호출부
//! (`app::toolbar::ribbon`)가 붙입니다.

use eframe::egui;

use crate::ui::layout::SP_1;
use crate::ui::{IconButton, icon_button, icon_label, icon_toggle};
use egui_phosphor_icons::Icon;

/// 모든 메뉴 행이 공유하는 높이 (pt) — 행마다 키가 달라 보이지 않게 고정합니다.
pub const ROW_H: f32 = 24.0;

/// 섹션 제목 — 앞 섹션과 **구분선**으로 분리하고, 본문보다 강한 라벨로 계층을 만듭니다.
/// (예전에는 작은 라벨만 있어서 섹션과 항목이 같은 층으로 보였습니다.)
pub fn menu_section(ui: &mut egui::Ui, label: &str) {
    ui.add_space(SP_1);
    ui.separator();
    ui.add_space(SP_1);
    ui.label(
        egui::RichText::new(label)
            .small()
            .strong()
            .color(crate::theme::nord::semantic::TEXT_STRONG),
    );
}

/// 메뉴 한 행의 뼈대 — 왼쪽은 `leading`(아이콘+라벨), 오른쪽은 `trailing` 슬롯.
///
/// 모든 행이 `ROW_H` 높이를 쓰고, 트레일링은 메뉴의 오른쪽 끝에 붙습니다.
/// (`trailing`에서 여러 위젯을 그리면 첫 위젯이 가장 오른쪽에 옵니다 —
///  왼쪽→오른쪽 순서로 보이게 하려면 역순으로 추가하세요.)
pub fn menu_row<R>(
    ui: &mut egui::Ui,
    leading: impl FnOnce(&mut egui::Ui) -> egui::Response,
    trailing: impl FnOnce(&mut egui::Ui) -> R,
) -> (egui::Response, R) {
    ui.horizontal(|ui| {
        ui.set_min_height(ROW_H);
        ui.spacing_mut().item_spacing.x = SP_1;
        let leading = leading(ui);
        // 남은 폭을 오른쪽 정렬 레이아웃에 넘겨 트레일링을 우측 끝에 붙입니다.
        let trailing = ui
            .with_layout(egui::Layout::right_to_left(egui::Align::Center), trailing)
            .inner;
        (leading, trailing)
    })
    .inner
}

/// 토글 행 — 행 왼쪽이 [아이콘+라벨] 토글입니다. `.changed()`로 판정합니다.
///
/// 체크박스 대신 툴바의 `icon_toggle`과 **같은 언어**를 씁니다(일관성).
pub fn menu_toggle_row<R>(
    ui: &mut egui::Ui,
    on: &mut bool,
    icon: Icon,
    label: &str,
    hint: &str,
    trailing: impl FnOnce(&mut egui::Ui) -> R,
) -> (egui::Response, R) {
    menu_row(
        ui,
        |ui| icon_toggle(ui, on, icon, label, hint),
        trailing,
    )
}

/// 액션 행 — [아이콘+라벨]을 누르면 실행됩니다. `.clicked()`로 판정합니다.
pub fn menu_action_row(ui: &mut egui::Ui, icon: Icon, label: &str, hint: &str) -> egui::Response {
    menu_row(
        ui,
        |ui| icon_button(ui, IconButton::new(icon, label).hint(hint).frame(false)),
        |_ui| (),
    )
    .0
}

/// 하위 메뉴 행 — [아이콘+라벨] + `▸`. 자식 메뉴는 `children`에서 그립니다.
#[allow(dead_code)] // 지금은 하위 메뉴를 쓰지 않지만 키트의 일부로 유지합니다.
pub fn menu_submenu_row<R>(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    hint: &str,
    children: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::Response {
    menu_row(
        ui,
        |ui| {
            ui.menu_button(crate::app::icon_text(ui, label, icon), children)
                .response
                .on_hover_text(hint)
        },
        |_ui| (),
    )
    .0
}

/// 트레일링 컨트롤의 최소 치 타깃 (pt) — 24pt 접근성 하한.
/// (측정에서 정렬 버튼 20pt, 기어 16pt로 드러나 이 상수를 도입했습니다.)
pub const TRAILING_TARGET: f32 = 24.0;

/// 트레일링용 아이 **선택** 버튼(정렬 3-세그먼트 등) — 최소 28×28 타깃.
pub fn menu_icon_select(
    ui: &mut egui::Ui,
    selected: bool,
    icon: Icon,
    hint: &str,
) -> egui::Response {
    ui.add_sized(
        egui::vec2(TRAILING_TARGET + 4.0, TRAILING_TARGET + 4.0),
        egui::Button::selectable(selected, crate::app::icon_text(ui, "", icon)),
    )
    .on_hover_text(hint)
}

/// 트레일링용 아이콘 버튼(행 전용 설정 등) — 최소 24×24 타깃.
pub fn menu_icon_button(ui: &mut egui::Ui, icon: Icon, hint: &str) -> egui::Response {
    ui.add_sized(
        egui::vec2(TRAILING_TARGET, TRAILING_TARGET),
        egui::Button::new(crate::app::icon_text(ui, "", icon)).frame(false),
    )
    .on_hover_text(hint)
}
/// 라벨만 있는 행(트레일링 슬롯 전용) — 정렬 3-세그먼트처럼 "행 제목 + 컨트롤"일 때.
/// `leading`이 응답을 돌려주지 않는 경우의 얇은 래퍼입니다.
pub fn menu_label_row<R>(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    trailing: impl FnOnce(&mut egui::Ui) -> R,
) -> (egui::Response, R) {
    menu_row(ui, |ui| icon_label(ui, icon, label), trailing)
}
