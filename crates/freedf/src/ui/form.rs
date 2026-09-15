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

use crate::ui::a11y::{self, Spec};
use crate::ui::tokens;

/// 툴팁 도움말 내장 — help가 비면 붙이지 않습니다.
fn tip(resp: egui::Response, help: &str) -> egui::Response {
    if help.is_empty() {
        resp
    } else {
        resp.on_hover_text(help)
    }
}

/// 라벨 줄 — 1rem, 필수 `*` / `(optional)` 표시 (1rem, 약간 흐리게).
fn label_line(ui: &mut egui::Ui, label: &str, req: Option<bool>) {
    if label.is_empty() && req.is_none() {
        return;
    }
    const REM: f32 = 16.0;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = tokens::space::SM;
        if !label.is_empty() {
            ui.label(egui::RichText::new(label).strong().size(REM));
        }
        match req {
            Some(true) => {
                // 필수 * — 1rem, 에러 색을 살짝 흐리게.
                let faded = ui
                    .visuals()
                    .error_fg_color
                    .gamma_multiply(0.85);
                ui.label(egui::RichText::new("*").strong().size(REM).color(faded));
            }
            Some(false) => {
                ui.label(egui::RichText::new("(optional)").weak().size(REM));
            }
            None => {}
        }
    });
}

/// <FormGroup> — 라벨(위) + 힌트(선택) + 컨트롤(아래) **수직 배치** 빌더.
pub(crate) struct Group<'a> {
    label: &'a str,
    req: Option<bool>,
    hint: &'a str,
}

impl<'a> Group<'a> {
    pub fn new(label: &'a str) -> Self {
        Self {
            label,
            req: None,
            hint: "",
        }
    }

    /// 필수 표시 `*`.
    pub fn required(mut self) -> Self {
        self.req = Some(true);
        self
    }

    /// 선택 표시 `(optional)`.
    pub fn optional(mut self) -> Self {
        self.req = Some(false);
        self
    }

    /// 라벨과 컨트롤 사이 힌트 텍스트.
    pub fn hint(mut self, h: &'a str) -> Self {
        self.hint = h;
        self
    }

    /// 수직 배치 렌더 — 라벨(+표시) → 힌트 → 컨트롤.
    pub fn show<R>(self, ui: &mut egui::Ui, children: impl FnOnce(&mut egui::Ui) -> R) -> R {
        label_line(ui, self.label, self.req);
        if !self.hint.is_empty() {
            ui.label(egui::RichText::new(self.hint).weak().small());
        }
        children(ui)
    }
}

/// <FormGroup> 생성 — `form::group("Server URL").required().hint(..).show(ui, ..)`.
pub(crate) fn group(label: &str) -> Group<'_> {
    Group::new(label)
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
///
/// `id_salt`는 문자열입니다: egui id로도 쓰고, **자동화 계약 id**
/// (`settings.section.<id_salt>`)로도 쓰기 때문입니다(스크립트가 "이 탭에 어떤
/// 섹션이 있는가"를 읽습니다).
pub(crate) fn fieldset(
    ui: &mut egui::Ui,
    id_salt: &str,
    title: &str,
    default_open: bool,
    children: impl FnOnce(&mut egui::Ui),
) -> egui::collapsing_header::CollapsingResponse<()> {
    // 헤더 삼각형-텍스트 간격 — egui 기본 indent(~6)는 "▼Physics model"처럼
    // 아이콘과 글자가 붙습니다(스크린샷 리뷰). indent로 아이콘 칸과 간격을 확보합니다.
    let prev_indent = ui.spacing_mut().indent;
    ui.spacing_mut().indent = crate::ui::scale::hrem(3);
    let resp = egui::CollapsingHeader::new(title)
        .id_salt(id_salt)
        .default_open(default_open)
        .show(ui, children);
    ui.spacing_mut().indent = prev_indent;
    // 섹션 헤더 자체도 타깃입니다(WCAG 2.5.8) — 패널의 interact_size.y가
    // COMFORT(28)이므로 헤더 높이도 그만큼 확보됩니다.
    let id = format!("settings.section.{id_salt}");
    let _ = a11y::finish(
        ui,
        Spec::label(&id, title).min_target(tokens::target::ROW),
        resp.header_response.clone(),
    );
    resp
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

/// <ColorPicker> — ColorEditButton (라벨 위 + 입력 아래; 라벨이 비면 인라인),
/// `.changed()`로 판정.
pub(crate) fn color(
    ui: &mut egui::Ui,
    value: &mut egui::Color32,
    label: &str,
    help: &str,
) -> egui::Response {
    label_line(ui, label, None);
    // 어두운 색이 창 배경에 묻혀 버튼이 안 보이는 실측 회귀(canvas 탭) —
    // 견본 테두리를 그려 어떤 색이든 항상 보이게 합니다.
    let size = tokens::target::COMFORT;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let resp = ui.put(rect, |ui: &mut egui::Ui| ui.color_edit_button_srgba(value));
    // 견본은 **원형** — 프리셋 견본(원)과 같은 어휘를 유지합니다.
    ui.painter().circle_stroke(
        rect.center(),
        rect.width() * 0.5 - 2.0,
        egui::Stroke::new(
            tokens::stroke::HAIRLINE,
            ui.visuals().widgets.noninteractive.bg_stroke.color,
        ),
    );
    tip(resp, help)
}

/// <FormSelect> — 콤보박스 (라벨 위 + 입력 아래, 항목은 items 클로저로).
pub(crate) fn select(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    selected_text: impl Into<egui::WidgetText>,
    label: &str,
    help: &str,
    items: impl FnOnce(&mut egui::Ui),
) -> egui::Response {
    label_line(ui, label, None);
    let resp = egui::ComboBox::from_id_salt(id_salt)
        .selected_text(selected_text)
        .show_ui(ui, items)
        .response;
    tip(resp, help)
}

/// <FormCheck> — 체크박스 + 라벨 **한 줄** (Bootstrap form-check), `.changed()`로 판정.
/// (예전의 "라벨 위 + 체크박스 아래" 배치는 불리언 하나에 두 줄을 써 콘텐츠 리듬이
/// 흐트러졌습니다 — 스크린샷 리뷰에서 발견. 행 전체가 타깃이므로 접근성도 더 좋습니다.)
pub(crate) fn check(ui: &mut egui::Ui, on: &mut bool, text: &str, help: &str) -> egui::Response {
    // `add_sized`는 내용을 **중앙** 배치합니다 — 행 전체 폭에서 콤보+라벨이
    // 가운데 붕 뜨는 실측 회귀가 있어 좌측 정렬 레이아웃으로 감쌉니다.
    let resp = ui
        .allocate_ui_with_layout(
            egui::vec2(ui.available_width(), tokens::target::ROW),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| ui.checkbox(on, text),
        )
        .inner;
    tip(resp, help)
}

/// <FormSwitch> — 토글 버튼 (라벨 위 + 입력 아래), `.changed()`로 판정.
#[allow(dead_code)] // 토글 입력은 현재 툴바 아이콘 토글(ui::icon_toggle)을 사용.
pub(crate) fn switch(ui: &mut egui::Ui, on: &mut bool, text: &str, help: &str) -> egui::Response {
    label_line(ui, text, None);
    tip(ui.toggle_value(on, ""), help)
}

/// <FormRange> — f32 슬라이더 (라벨 위 + 입력 아래 수직 배치), `.changed()`로 판정.
///
/// 값 표시 소수점은 **범위 크기에서 파생**합니다 — 그렇지 않으면 0.3000(4dp)과
/// 0.350(3dp)이 섞여 값 칩의 폭·리듬이 흐트러졌습니다(스크린샷 리뷰에서 발견).
pub(crate) fn range(
    ui: &mut egui::Ui,
    value: &mut f32,
    r: std::ops::RangeInclusive<f32>,
    label_text: &str,
    help: &str,
) -> egui::Response {
    label_line(ui, label_text, None);
    let span = r.end() - r.start();
    let decimals: usize = if span < 2.0 {
        2
    } else if span < 10.0 {
        1
    } else {
        0
    };
    let slider = egui::Slider::new(value, r).custom_formatter(move |v, _| {
        format!("{:.*}", decimals, v)
    });
    tip(ui.add(slider), help)
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
    let mut slider = egui::Slider::new(value, r.clone());
    if let Some(f) = formatter {
        slider = slider.custom_formatter(move |v, _| f(v as i32, r.clone()));
    }
    label_line(ui, label_text, None);
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
    /// 수직 배치 라벨 (비어 있으면 인라인).
    label: &'a str,
    /// 필수 `*` / `(optional)` 표시.
    req: Option<bool>,
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
            label: "",
            req: None,
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

    #[allow(dead_code)] // 빌더 API 예약 (현재 접두사는 사용처 없음).
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

    /// 수직 배치 라벨 (위).
    pub fn label(mut self, l: &'a str) -> Self {
        self.label = l;
        self
    }

    /// 필수 `*` 표시.
    #[allow(dead_code)] // 현재 required 표시는 Group으로만 사용.
    pub fn required(mut self) -> Self {
        self.req = Some(true);
        self
    }

    /// 선택 `(optional)` 표시.
    pub fn optional(mut self) -> Self {
        self.req = Some(false);
        self
    }

    /// 렌더 — `.changed()`로 판정.
    pub fn show(self, ui: &mut egui::Ui) -> egui::Response {
        label_line(ui, self.label, self.req);
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

/// <FormTextarea> — 여러 줄 텍스트 입력, `.changed()`로 판정.
#[allow(dead_code)] // 예약 — 아직 사용처 없음.
pub(crate) fn textarea(
    ui: &mut egui::Ui,
    value: &mut String,
    hint: &str,
    rows: f32,
) -> egui::Response {
    ui.add_sized(
        [ui.available_width(), rows * 18.0],
        egui::TextEdit::multiline(value).hint_text(hint),
    )
}

/// <Segmented> — Bootstrap btn-group 스타일 라디오 선택 (0..options.len()).
#[allow(dead_code)] // 예약 — 아직 사용처 없음.
pub(crate) fn segmented(ui: &mut egui::Ui, selected: &mut usize, options: &[&str]) {
    ui.horizontal(|ui| {
        for (i, opt) in options.iter().enumerate() {
            if ui.selectable_label(*selected == i, *opt).clicked() {
                *selected = i;
            }
        }
    });
}
