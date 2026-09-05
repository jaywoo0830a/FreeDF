//! 재사용 가능한 UI 컴포넌트 — React 스타일 (props + 상태 연결 분리).
//!
//! 툴바/설정 창 등에서 반복되던 egui 코드 패턴을 한 곳으로 모읍니다:
//!
//! ```ignore
//! // React의 <IconButton icon=… label=… onClick=… />에 해당.
//! if icon_button(ui, IconButton::new(icons::FLOPPY_DISK, "Save")
//!     .hint("Save annotations (Ctrl+S)"))
//!     .clicked() { /* onClick */ }
//! ```
//!
//! - [`icon_button`]: 아이콘+라벨 버튼 (enabled/selected/frame props)
//! - [`icon_toggle`]: 상태값에 바인딩된 토글 버튼 (`.changed()`로 판정)
//! - [`icon_select`]: 선택 하이라이트 아이콘 버튼
//! - [`icon_label`]: 아이콘+라벨 텍스트 (그룹 제목)
//!
//! 모든 컴포넌트는 툴팁(`hint`)을 내장합니다 — 호출부에서 반복하던
//! `.on_hover_text(...)`를 props로 옮겼습니다.

use eframe::egui;
use egui_phosphor_icons::Icon;

/// [`icon_button`]의 props — React의 `<IconButton>` 속성에 해당.
#[derive(Clone, Copy)]
pub(crate) struct IconButton<'a> {
    pub icon: Icon,
    pub label: &'a str,
    pub hint: &'a str,
    pub enabled: bool,
    pub selected: bool,
    /// false = 프레임 없는 텍스트 스타일 버튼.
    pub frame: bool,
}

impl<'a> IconButton<'a> {
    pub fn new(icon: Icon, label: &'a str) -> Self {
        Self {
            icon,
            label,
            hint: "",
            enabled: true,
            selected: false,
            frame: true,
        }
    }

    pub fn hint(mut self, hint: &'a str) -> Self {
        self.hint = hint;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn frame(mut self, frame: bool) -> Self {
        self.frame = frame;
        self
    }
}

/// 아이콘+라벨 버튼 — 툴팁 내장, `.clicked()`로 판정.
pub(crate) fn icon_button(ui: &mut egui::Ui, props: IconButton<'_>) -> egui::Response {
    let mut b = egui::Button::new(crate::app::icon_text(ui, props.label, props.icon));
    if props.selected {
        b = b.selected(true);
    }
    if !props.frame {
        b = b.frame(false);
    }
    ui.add_enabled(props.enabled, b)
        .on_hover_text(props.hint)
}

/// 상태값에 바인딩된 토글 버튼 — `resp.changed()`로 판정.
/// (값은 egui가 직접 갱신합니다 — React의 제어 컴포넌트와 동일.)
pub(crate) fn icon_toggle(
    ui: &mut egui::Ui,
    on: &mut bool,
    icon: Icon,
    label: &str,
    hint: &str,
) -> egui::Response {
    ui.toggle_value(on, crate::app::icon_text(ui, label, icon))
        .on_hover_text(hint)
}

/// 선택 하이라이트 아이콘 버튼 (정렬 버튼 등 라디오 그룹용).
pub(crate) fn icon_select(
    ui: &mut egui::Ui,
    selected: bool,
    icon: Icon,
    label: &str,
    hint: &str,
) -> egui::Response {
    ui.selectable_label(selected, crate::app::icon_text(ui, label, icon))
        .on_hover_text(hint)
}

/// 아이콘+라벨 텍스트 (그룹 제목).
pub(crate) fn icon_label(ui: &mut egui::Ui, icon: Icon, label: &str) -> egui::Response {
    ui.label(crate::app::icon_text(ui, label, icon))
}
