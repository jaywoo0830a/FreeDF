//! Bootstrap 5 스타일 **폼 컴포넌트** — 모든 데이터 입력의 단일 경로.
//!
//! 대응표:
//! | Bootstrap | 여기 | 설명 |
//! |---|---|---|
//! | FormLabel | [`label`] | 컨트롤 위 작은 제목 |
//! | FormText | [`help`] | 도움말 텍스트 |
//! | FormControl | [`text`] / [`password`] | 텍스트 입력 |
//! | FormSelect | [`select`] | 콤보박스 |
//! | FormCheck | [`check`] | 체크박스 |
//! | FormSwitch | [`switch`] | 토글 스위치 |
//! | FormRange | [`range`] | 슬라이더 |
//! | NumberInput | [`number`] | 숫자 입력(DragValue) |
//! | FieldSet | [`fieldset`] | 접이식 섹션 |
//! | InputGroup | [`input_group`] | 접두/접미 장식 한 줄 |
//!
//! 모든 컨트롤은 툴팁 도움말을 내장합니다 — 호출부에서 `.on_hover_text(...)`
//! 를 반복하지 않습니다.

use eframe::egui;

/// 툴팁 도움말 내장 — help가 비면 붙이지 않습니다.
fn tip(resp: egui::Response, help: &str) -> egui::Response {
    if help.is_empty() {
        resp
    } else {
        resp.on_hover_text(help)
    }
}

/// <FormLabel> — 컨트롤 위/옆 작은 회색 제목.
pub(crate) fn label(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(egui::RichText::new(text).weak().small());
}

/// <FormText> — 컨트롤 아래 도움말 텍스트.
pub(crate) fn help(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(egui::RichText::new(text).weak().small());
}

/// <FieldSet> — 접이식 섹션.
pub(crate) fn fieldset(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    title: &str,
    default_open: bool,
    children: impl FnOnce(&mut egui::Ui),
) -> egui::collapsing_header::CollapsingResponse<()> {
    egui::CollapsingHeader::new(title)
        .id_salt(id_salt)
        .default_open(default_open)
        .show(ui, children)
}

/// <FormControl> 텍스트 입력 빌더.
pub(crate) struct TextInput<'a> {
    value: &'a mut String,
    hint: &'a str,
    help: &'a str,
    password: bool,
    width: Option<f32>,
}

impl<'a> TextInput<'a> {
    pub fn new(value: &'a mut String) -> Self {
        Self {
            value,
            hint: "",
            help: "",
            password: false,
            width: None,
        }
    }

    pub fn hint(mut self, h: &'a str) -> Self {
        self.hint = h;
        self
    }

    pub fn help(mut self, h: &'a str) -> Self {
        self.help = h;
        self
    }

    pub fn password(mut self, on: bool) -> Self {
        self.password = on;
        self
    }

    pub fn width(mut self, w: f32) -> Self {
        self.width = Some(w);
        self
    }

    /// 렌더 — `.changed()`/`.lost_focus()`/`.has_focus()`로 판정.
    pub fn show(self, ui: &mut egui::Ui) -> egui::Response {
        let mut te = egui::TextEdit::singleline(self.value).hint_text(self.hint);
        if self.password {
            te = te.password(true);
        }
        if let Some(w) = self.width {
            te = te.desired_width(w);
        }
        tip(ui.add(te), self.help)
    }
}

/// <FormControl> 텍스트 입력 — `form::text(&mut v).hint(..).show(ui)`.
pub(crate) fn text(value: &mut String) -> TextInput<'_> {
    TextInput::new(value)
}

/// <FormControl type=password> — `form::password(&mut v).show(ui)`.
pub(crate) fn password(value: &mut String) -> TextInput<'_> {
    TextInput::new(value).password(true)
}

/// <ColorPicker> — ColorEditButton, `.changed()`로 판정.
pub(crate) fn color(
    ui: &mut egui::Ui,
    value: &mut egui::Color32,
    help: &str,
) -> egui::Response {
    tip(ui.color_edit_button_srgba(value), help)
}

/// <FormSelect> — 콤보박스 (항목은 items 클로저로).
pub(crate) fn select(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    selected_text: impl Into<egui::WidgetText>,
    help: &str,
    items: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    let resp = egui::ComboBox::from_id_salt(id_salt)
        .selected_text(selected_text)
        .show_ui(ui, items)
        .response;
    tip(resp, help)
}

/// <FormCheck> — 체크박스, `.changed()`로 판정.
pub(crate) fn check(ui: &mut egui::Ui, on: &mut bool, text: &str, help: &str) -> egui::Response {
    tip(ui.checkbox(on, text), help)
}

/// <FormSwitch> — 토글 버튼, `.changed()`로 판정.
#[allow(dead_code)] // 토글 입력은 현재 툴바 아이콘 토글(ui::icon_toggle)을 사용.
pub(crate) fn switch(ui: &mut egui::Ui, on: &mut bool, text: &str, help: &str) -> egui::Response {
    ui.toggle_value(on, text).on_hover_text(help)
}

/// <FormRange> — f32 슬라이더, `.changed()`로 판정.
pub(crate) fn range(
    ui: &mut egui::Ui,
    value: &mut f32,
    r: std::ops::RangeInclusive<f32>,
    label_text: &str,
    help: &str,
) -> egui::Response {
    tip(ui.add(egui::Slider::new(value, r).text(label_text)), help)
}

/// <FormRange> 정수형 — 커스텀 포매터 지원 (프리셋 라벨 등).
pub(crate) fn range_i(
    ui: &mut egui::Ui,
    value: &mut i32,
    r: std::ops::RangeInclusive<i32>,
    label_text: &str,
    help: &str,
    formatter: Option<fn(i32, std::ops::RangeInclusive<i32>) -> String>,
) -> egui::Response {
    let mut slider = egui::Slider::new(value, r.clone()).text(label_text);
    if let Some(f) = formatter {
        slider = slider.custom_formatter(move |v, _| f(v as i32, r.clone()));
    }
    tip(ui.add(slider), help)
}

/// <NumberInput> — DragValue 빌더 (Bootstrap NumberInput 대응).
pub(crate) struct NumberInput<'a> {
    value: &'a mut f32,
    range: Option<std::ops::RangeInclusive<f32>>,
    speed: Option<f32>,
    prefix: &'a str,
    suffix: &'a str,
    decimals: Option<usize>,
    help: &'a str,
}

impl<'a> NumberInput<'a> {
    pub fn new(value: &'a mut f32) -> Self {
        Self {
            value,
            range: None,
            speed: None,
            prefix: "",
            suffix: "",
            decimals: None,
            help: "",
        }
    }

    pub fn range(mut self, r: std::ops::RangeInclusive<f32>) -> Self {
        self.range = Some(r);
        self
    }

    pub fn speed(mut self, s: f32) -> Self {
        self.speed = Some(s);
        self
    }

    pub fn prefix(mut self, p: &'a str) -> Self {
        self.prefix = p;
        self
    }

    pub fn suffix(mut self, s: &'a str) -> Self {
        self.suffix = s;
        self
    }

    pub fn decimals(mut self, n: usize) -> Self {
        self.decimals = Some(n);
        self
    }

    pub fn help(mut self, h: &'a str) -> Self {
        self.help = h;
        self
    }

    /// 렌더 — `.changed()`로 판정.
    pub fn show(self, ui: &mut egui::Ui) -> egui::Response {
        let mut dv = egui::DragValue::new(self.value);
        if let Some(r) = self.range {
            dv = dv.range(r);
        }
        if let Some(s) = self.speed {
            dv = dv.speed(s);
        }
        if !self.prefix.is_empty() {
            dv = dv.prefix(self.prefix);
        }
        if !self.suffix.is_empty() {
            dv = dv.suffix(self.suffix);
        }
        if let Some(d) = self.decimals {
            dv = dv.fixed_decimals(d);
        }
        tip(ui.add(dv), self.help)
    }
}

/// <NumberInput> 생성 숏컷.
pub(crate) fn number(value: &mut f32) -> NumberInput<'_> {
    NumberInput::new(value)
}

/// <InputGroup> — 접두/접미 장식이 있는 컨트롤 한 줄.
pub(crate) fn input_group<R>(
    ui: &mut egui::Ui,
    prefix: &str,
    suffix: &str,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    ui.horizontal(|ui| {
        if !prefix.is_empty() {
            ui.label(prefix);
        }
        let r = add(ui);
        if !suffix.is_empty() {
            ui.label(suffix);
        }
        r
    })
    .inner
}
