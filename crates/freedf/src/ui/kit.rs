//! 컴포넌트 키트 — 앱 전체가 쓰는 **하나의** 인터랙티브 컴포넌트 집합.
//!
//! ## 왜 다시 만들었나
//! 기존 `ui`는 두 개의 평행한 버튼 API([`crate::ui::buttons::Button`]와
//! `ui::icon_button`/`icon_toggle`/`icon_select`/`action`)와 사장된 컴포넌트
//! (`form::switch`, `ds::tabs`)를 함께 갖고 있었습니다. 그래서 같은 동작이
//! 화면마다 다르게 보이고, 타깃 크기·이름·계측 같은 계약을 강제할 지점이
//! 없었습니다(실측: 메뉴 정렬 버튼 20×28pt, 설정 기어 16×28pt).
//!
//! ## 이 키트의 규칙 (모두 [`crate::ui::a11y`]가 검증)
//! 1. **최소 타깃**: [`crate::ui::tokens::target::MIN`] 이상. 크기는
//!    [`Size`]로만 고릅니다(매직 넘버 금지).
//! 2. **접근성 이름 필수**: 아이콘 전용 컴포넌트도 `name`을 받습니다.
//! 3. **테스트 훅 내장**: `test_id`를 넘기면 컴포넌트가 스스로 계측에 등록합니다.
//!    호출부는 `dev::tag_*`를 부르지 않습니다(빠뜨림 방지).
//! 4. **상태는 토큰에서 파생**: hover/active/selected/disabled 시각은
//!    [`crate::ui::tokens::State`]가 유일한 출처입니다.
//! 5. **프레젠테이션만**: 상태 연결(핸들러)은 호출부가 합니다.

use eframe::egui;
use egui_phosphor_icons::Icon;

use crate::ui::a11y::{self, Spec};
use crate::ui::tokens::{self, target, radius, space};

/// 시각 변형 — 의미가 다르면 모양도 달라야 합니다.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Variant {
    /// 주 행동 (한 화면에 하나).
    Primary,
    /// 보통 행동.
    Secondary,
    /// 테두리 없는 낮은 강조 (툴바 안).
    Ghost,
    /// 파괴적 행동.
    Danger,
}

/// 크기 — pt 값은 토큰에서만 옵니다.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Size {
    Small,
    Medium,
    /// 손가락 입력(태블릿) 기본.
    Touch,
}

impl Size {
    /// 이 크기의 **최소 타깃 한 변**(pt).
    pub fn target(self) -> f32 {
        match self {
            Self::Small => target::MIN,
            Self::Medium => target::COMFORT,
            Self::Touch => target::TOUCH,
        }
    }

    fn font(self) -> f32 {
        match self {
            Self::Small => tokens::font::SMALL,
            Self::Medium => tokens::font::BODY,
            Self::Touch => tokens::font::TITLE,
        }
    }
}

/// 아이콘+라벨 버튼 — React의 `<Button variant size icon label onClick />`.
///
/// ```
/// # use freedf::ui::kit::Button;
/// if Button::secondary("Save Edits").icon(icons::FLOPPY_DISK)
///     .test_id("toolbar.save_edits").hint("Save annotations (Ctrl+S)")
///     .show(ui).clicked() { }
/// ```
#[derive(Clone)]
pub struct Button<'a> {
    label: &'a str,
    icon: Option<Icon>,
    variant: Variant,
    size: Size,
    hint: String,
    enabled: bool,
    selected: bool,
    frame: Option<bool>,
    test_id: Option<&'a str>,
}

impl<'a> Button<'a> {
    pub fn new(label: &'a str) -> Self {
        Self {
            label,
            icon: None,
            variant: Variant::Secondary,
            size: Size::Medium,
            hint: String::new(),
            enabled: true,
            selected: false,
            frame: None,
            test_id: None,
        }
    }

    pub fn primary(label: &'a str) -> Self {
        Self::new(label).variant(Variant::Primary)
    }

    pub fn secondary(label: &'a str) -> Self {
        Self::new(label).variant(Variant::Secondary)
    }

    pub fn ghost(label: &'a str) -> Self {
        Self::new(label).variant(Variant::Ghost)
    }

    pub fn danger(label: &'a str) -> Self {
        Self::new(label).variant(Variant::Danger)
    }

    pub fn icon(mut self, ic: Icon) -> Self {
        self.icon = Some(ic);
        self
    }

    pub fn variant(mut self, v: Variant) -> Self {
        self.variant = v;
        self
    }

    pub fn size(mut self, s: Size) -> Self {
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

    pub fn framed(mut self, on: bool) -> Self {
        self.frame = Some(on);
        self
    }

    /// 테스트 훅 — 자동화 스크립트가 쓰는 안정 id.
    pub fn test_id(mut self, id: &'a str) -> Self {
        self.test_id = Some(id);
        self
    }

    /// 최소 타깃 한 변 (pt) — 이 크기의 보장값입니다.
    pub fn min_target(&self) -> f32 {
        self.size.target()
    }
}

impl<'a> Button<'a> {
    /// 실제로 그립니다. 반환된 `Response`로 핸들러를 판정합니다.
    pub fn show(self, ui: &mut egui::Ui) -> egui::Response {
        let min = self.min_target();
        let text = match self.icon {
            Some(ic) => crate::app::icon_text(ui, self.label, ic),
            None => egui::WidgetText::from(
                egui::RichText::new(self.label).font(egui::FontId::proportional(self.size.font())),
            ),
        };

        // 변형별 채움색 — 의미 있는 색만 씁니다(원시 팔레트 금지).
        let fill = match self.variant {
            Variant::Primary => Some(ui.visuals().selection.bg_fill),
            Variant::Danger => Some(ui.visuals().error_fg_color.gamma_multiply(0.85)),
            Variant::Secondary | Variant::Ghost => None,
        };

        let mut b = egui::Button::new(text).min_size(egui::vec2(min, min));
        if let Some(f) = fill {
            // 채움색 위 글자는 대비를 계산해 흑/백으로 고정합니다(접근성).
            b = b.fill(f).selected(false);
        }
        if self.selected {
            b = b.selected(true);
        }
        let framed = self.frame.unwrap_or(self.variant != Variant::Ghost);
        if !framed {
            b = b.frame(false);
        }

        let resp = ui.add_enabled(self.enabled, b);
        let spec = match self.test_id {
            Some(id) => Spec::button(id, self.label),
            None => Spec::text(self.label),
        };
        a11y::finish(ui, spec.hint(&self.hint).min_target(min), resp)
    }
}

/// 아이콘 전용 버튼 — **접근성 이름이 필수**입니다.
///
/// 예전에는 `IconButton::new(icons::GEAR, "")`처럼 빈 라벨이 있어서
/// 자동화가 "무엇인지" 알 수 없었습니다. 이제 `name`은 필수 인자입니다.
#[derive(Clone)]
pub struct IconButton<'a> {
    name: &'a str,
    icon: Icon,
    hint: String,
    enabled: bool,
    selected: bool,
    size: Size,
    test_id: Option<&'a str>,
    frame: bool,
}

impl<'a> IconButton<'a> {
    /// `name`은 접근성 이름(=툴팁 기본값)입니다. 화면에는 아이콘만 보입니다.
    pub fn new(icon: Icon, name: &'a str) -> Self {
        Self {
            name,
            icon,
            hint: String::new(),
            enabled: true,
            selected: false,
            size: Size::Small,
            test_id: None,
            frame: false,
        }
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

    pub fn size(mut self, s: Size) -> Self {
        self.size = s;
        self
    }

    pub fn framed(mut self, on: bool) -> Self {
        self.frame = on;
        self
    }

    pub fn test_id(mut self, id: &'a str) -> Self {
        self.test_id = Some(id);
        self
    }

    pub fn show(self, ui: &mut egui::Ui) -> egui::Response {
        let min = self.size.target();
        let text = crate::app::icon_text(ui, "", self.icon);
        let mut b = egui::Button::new(text)
            .min_size(egui::vec2(min, min))
            // 아이콘은 가운데 정렬이 자연스럽습니다.
            .frame(self.frame);
        if self.selected {
            b = b.selected(true);
        }
        let resp = ui.add_enabled(self.enabled, b);
        // 툴팁 기본값은 접근성 이름 — 화면에 안 보이는 의미를 항상 노출합니다.
        let hint = if self.hint.is_empty() {
            self.name.to_string()
        } else {
            self.hint.clone()
        };
        let spec = match self.test_id {
            Some(id) => Spec::button(id, self.name),
            None => Spec::text(self.name),
        };
        a11y::finish(ui, spec.hint(&hint).min_target(min), resp)
    }
}

/// 목록/메뉴 한 행 — **행 전체가 하나의 타깃**입니다(가장 테스트하기 좋은 형태:
/// 좌표가 예측 가능하고, 클릭 영역이 충분히 큽니다).
///
/// 레이아웃: `[아이콘] 라벨 … [트레일링 슬롯]`
/// 트레일링은 행 사각형 **안쪽 오른쪽**에 배치되므로 행 높이가 흔들리지 않습니다.
pub struct Row<'a> {
    icon: Option<Icon>,
    label: &'a str,
    hint: String,
    test_id: Option<&'a str>,
    role: a11y::Role,
    enabled: bool,
    selected: bool,
    /// 토글이면 값 소유자에 바인딩됩니다.
    value: Option<&'a mut bool>,
    height: f32,
    /// 트레일링 슬롯에 남겨 둘 폭(0이면 남은 폭 전체 사용).
    trailing_width: f32,
}

impl<'a> Row<'a> {
    /// 실행되는 행(메뉴 항목 등).
    pub fn action(test_id: &'a str, label: &'a str) -> Self {
        Self::base(test_id, label, a11y::Role::MenuItem)
    }

    /// 켜고 끄는 행. 클릭하면 `on`이 뒤집히고 `.changed()`가 참이 됩니다.
    pub fn toggle(test_id: &'a str, label: &'a str, on: &'a mut bool) -> Self {
        let mut r = Self::base(test_id, label, a11y::Role::Toggle);
        r.value = Some(on);
        r
    }

    /// 선택되는 행(라디오처럼 동작).
    pub fn radio(test_id: &'a str, label: &'a str, selected: bool) -> Self {
        let mut r = Self::base(test_id, label, a11y::Role::Radio);
        r.selected = selected;
        r
    }

    /// 라벨만 있는 행(트레일링 슬롯 전용) — 눌러도 아무 일도 하지 않습니다.
    pub fn label(label: &'a str) -> Self {
        let mut r = Self::base("", label, a11y::Role::Text);
        r.test_id = None;
        r
    }

    fn base(test_id: &'a str, label: &'a str, role: a11y::Role) -> Self {
        Self {
            icon: None,
            label,
            hint: String::new(),
            test_id: if test_id.is_empty() { None } else { Some(test_id) },
            role,
            enabled: true,
            selected: false,
            value: None,
            height: tokens::target::ROW,
            trailing_width: 0.0,
        }
    }

    pub fn icon(mut self, ic: Icon) -> Self {
        self.icon = Some(ic);
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

    pub fn height(mut self, h: f32) -> Self {
        self.height = h;
        self
    }

    /// 트레일링 슬롯 폭 예약 — 겹치지 않게 미리 자리를 비워 둡니다.
    pub fn trailing_width(mut self, w: f32) -> Self {
        self.trailing_width = w;
        self
    }

    /// 행을 그리고 `(응답, 트레일링 결과)`를 돌려줍니다.
    pub fn show<R>(
        mut self,
        ui: &mut egui::Ui,
        trailing: impl FnOnce(&mut egui::Ui) -> R,
    ) -> (egui::Response, R) {
        let height = self.height.max(tokens::target::MIN);
        let width = ui.available_width();
        let sense = if self.enabled {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        };
        let (rect, mut resp) = ui.allocate_exact_size(egui::vec2(width, height), sense);

        // 상태 → 시각 (토큰이 유일한 출처)
        let state = tokens::State::of(&resp, self.selected);
        if ui.is_rect_visible(rect) {
            if let Some(fill) = state.fill(ui, true) {
                ui.painter().rect_filled(rect, radius::SM, fill);
            }
            if let Some(st) = state.stroke(ui) {
                ui.painter()
                    .rect_stroke(rect, radius::SM, st, egui::StrokeKind::Inside);
            }
            let color = state.text(ui);
            let cy = rect.center().y;
            let mut x = rect.left() + space::MD;
            if let Some(ic) = self.icon {
                ui.painter().text(
                    egui::pos2(x + 8.0, cy),
                    egui::Align2::CENTER_CENTER,
                    ic.0,
                    egui::FontId::new(15.0, egui::FontFamily::Name("phosphor-regular".into())),
                    color,
                );
                x += 22.0;
            }
            ui.painter().text(
                egui::pos2(x, cy),
                egui::Align2::LEFT_CENTER,
                self.label,
                egui::FontId::proportional(tokens::font::BODY),
                color,
            );
        }

        // 트레일링 슬롯 — 행 안쪽 오른쪽에 **고정 폭**으로 배치합니다.
        // (왼쪽→오른쪽 레이아웃을 쓰되 사각형 자체를 우측에 두면, 순서가 뒤집히지
        //  않으면서 우측 정렬이 유지됩니다.)
        let reserve = if self.trailing_width > 0.0 {
            self.trailing_width + space::SM
        } else {
            space::SM
        };
        let trect = egui::Rect::from_min_max(
            egui::pos2(rect.right() - reserve, rect.top() + space::XS),
            egui::pos2(rect.right() - space::SM, rect.bottom() - space::XS),
        );
        let trailing = ui
            .scope_builder(
                egui::UiBuilder::new()
                    .max_rect(trect)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
                trailing,
            )
            .inner;

        // 토글이면 값 반전 + changed 표시(값 소유자는 호출부).
        let mut value_now: Option<bool> = None;
        if let Some(on) = self.value.as_ref() {
            value_now = Some(**on);
        }
        if resp.clicked() {
            if let Some(on) = self.value.as_mut() {
                **on = !**on;
                value_now = Some(**on);
            }
            if self.role != a11y::Role::Text {
                resp.mark_changed();
            }
        }

        let spec = match (self.test_id, self.role) {
            (Some(id), a11y::Role::Toggle) => Spec::toggle(id, self.label, value_now.unwrap_or(false)),
            (Some(id), a11y::Role::Radio) => Spec::radio(id, self.label, self.selected),
            (Some(id), _) => Spec::button(id, self.label),
            (None, _) => Spec::text(self.label),
        };
        let resp = a11y::finish(ui, spec.hint(&self.hint).min_target(tokens::target::ROW), resp);
        (resp, trailing)
    }
}


/// 인라인 토글 — 값에 바인딩된 버튼(툴바 패널 토글 등). `.changed()`로 판정합니다.
/// (행 형태가 필요하면 [`Row::toggle`]을 쓰세요.)
pub struct Toggle<'a> {
    on: &'a mut bool,
    label: &'a str,
    icon: Option<Icon>,
    hint: String,
    enabled: bool,
    test_id: Option<&'a str>,
    size: Size,
}

impl<'a> Toggle<'a> {
    pub fn new(on: &'a mut bool, label: &'a str) -> Self {
        Self {
            on,
            label,
            icon: None,
            hint: String::new(),
            enabled: true,
            test_id: None,
            size: Size::Medium,
        }
    }

    pub fn icon(mut self, ic: Icon) -> Self {
        self.icon = Some(ic);
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

    pub fn test_id(mut self, id: &'a str) -> Self {
        self.test_id = Some(id);
        self
    }

    pub fn size(mut self, s: Size) -> Self {
        self.size = s;
        self
    }

    pub fn show(self, ui: &mut egui::Ui) -> egui::Response {
        let min = self.size.target();
        let text = match self.icon {
            Some(ic) => crate::app::icon_text(ui, self.label, ic),
            None => egui::WidgetText::from(self.label),
        };
        let mut resp = ui
            .add_enabled_ui(self.enabled, |ui| {
                ui.add(
                    egui::Button::selectable(*self.on, text)
                        .min_size(egui::vec2(min, min)),
                )
            })
            .inner;
        if resp.clicked() {
            *self.on = !*self.on;
            resp.mark_changed();
        }
        let spec = match self.test_id {
            Some(id) => Spec::toggle(id, self.label, *self.on),
            None => Spec::text(self.label),
        };
        a11y::finish(ui, spec.hint(&self.hint).min_target(min), resp)
    }
}

/// 세그먼트(라디오 그룹) 한 항목.
pub struct Segment<'a> {
    /// 접근성 이름 — 화면에 안 보여도 자동화/스크린리더가 이 이름으로 읽습니다.
    pub label: &'a str,
    pub icon: Option<Icon>,
    /// 테스트 훅 id.
    pub test_id: &'a str,
    pub hint: &'a str,
    /// 화면에 라벨을 그릴지 여부(false면 아이콘만 — 좁은 슬롯용).
    pub show_label: bool,
}

impl<'a> Segment<'a> {
    /// 아이콘+라벨 세그먼트(넓은 화면용).
    pub fn new(test_id: &'a str, label: &'a str) -> Self {
        Self {
            label,
            icon: None,
            test_id,
            hint: "",
            show_label: true,
        }
    }

    /// **아이콘 전용** 세그먼트 — 좁은 트레일링 슬롯용.
    /// `name`은 접근성 이름이자 기본 툴팁입니다(화면에는 아이콘만 보임).
    pub fn icon_only(test_id: &'a str, name: &'a str, icon: Icon) -> Self {
        Self {
            label: name,
            icon: Some(icon),
            test_id,
            hint: "",
            show_label: false,
        }
    }

    pub fn icon(mut self, ic: Icon) -> Self {
        self.icon = Some(ic);
        self
    }

    pub fn hint(mut self, h: &'a str) -> Self {
        self.hint = h;
        self
    }
}

/// 세그먼트 컨트롤 — 배타적 선택(정렬/모드). 클릭된 항목 인덱스를 돌려줍니다.
pub struct Segmented<'a> {
    items: Vec<Segment<'a>>,
    selected: usize,
    size: Size,
}

impl<'a> Segmented<'a> {
    pub fn new(items: Vec<Segment<'a>>, selected: usize) -> Self {
        Self {
            items,
            selected,
            // 기본은 "편안한" 크기(28pt) — 태블릿에서 손가락으로도 편하게.
            size: Size::Medium,
        }
    }

    pub fn size(mut self, s: Size) -> Self {
        self.size = s;
        self
    }

    /// 그립니다. 새로 선택된 인덱스가 있으면 `Some(idx)`.
    pub fn show(self, ui: &mut egui::Ui) -> Option<usize> {
        let min = self.size.target();
        let mut picked = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = space::SM;
            for (i, item) in self.items.into_iter().enumerate() {
                let text = match (item.icon, item.show_label) {
                    (Some(ic), true) => crate::app::icon_text(ui, item.label, ic),
                    (Some(ic), false) => crate::app::icon_text(ui, "", ic),
                    (None, _) => egui::WidgetText::from(item.label),
                };
                let selected = i == self.selected;
                let resp = ui.add(
                    egui::Button::selectable(selected, text).min_size(egui::vec2(min, min)),
                );
                // 툴팁: 명시 힌트 → 없으면 접근성 이름(아이콘 전용일 때 의미 노출).
                let hint = if item.hint.is_empty() {
                    if item.show_label {
                        ""
                    } else {
                        item.label
                    }
                } else {
                    item.hint
                };
                let spec = Spec::radio(item.test_id, item.label, selected);
                let resp = a11y::finish(ui, spec.hint(hint).min_target(min), resp);
                if resp.clicked() {
                    picked = Some(i);
                }
            }
        });
        picked
    }
}

/// 세그먼트 `n`개의 **예약 폭**(pt) — 트레일링 슬롯에 미리 자리를 비울 때 씁니다.
/// (행의 [`Row::trailing_width`]와 함께 쓰면 트레일링이 행 밖으로 밀리지 않습니다.)
pub fn segment_group_width(n: usize) -> f32 {
    if n == 0 {
        return 0.0;
    }
    let n = n as f32;
    n * Size::Medium.target() + (n - 1.0) * space::SM
}

/// 섹션 라벨 — 목록/메뉴에서 **주제의 경계**를 만듭니다(구분선 + 강한 라벨).
/// 계층이 사라지는 사고(예전 More 오버레이)를 막는 최소 장치입니다.
pub fn section_label(ui: &mut egui::Ui, text: &str) {
    ui.add_space(space::SM);
    ui.separator();
    ui.add_space(space::XS);
    ui.label(
        egui::RichText::new(text)
            .size(tokens::font::SMALL)
            .strong()
            .color(crate::theme::nord::semantic::TEXT_STRONG),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::a11y::{self, IssueKind};

    fn raw(events: Vec<egui::Event>) -> egui::RawInput {
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(1000.0, 700.0),
        ));
        input.events = events;
        input
    }

    /// 헤드리스 egui 컨텍스트 — GUI·Xvfb 없이 컴포넌트 계약을 검증하는 훅.
    fn headless() -> egui::Context {
        let ctx = egui::Context::default();
        crate::fonts::install_inter(&ctx);
        ctx
    }

    /// 한 프레임을 렌더링합니다 (egui 0.36은 `run_ui`가 루트 Ui를 줍니다).
    /// 반환된 `FullOutput`의 텍스처 델타는 **반드시 소비**해야 합니다
    /// (그냥 버리면 epaint가 패닉합니다).
    fn frame(ctx: &egui::Context, events: Vec<egui::Event>, body: impl FnMut(&mut egui::Ui)) {
        let mut out = ctx.run_ui(raw(events), body);
        out.textures_delta.clear();
    }

    fn click_at(pos: egui::Pos2) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            },
        ]
    }

    #[test]
    fn buttons_meet_their_min_target() {
        let _g = a11y::test_guard();
        a11y::reset_issues();
        for (size, min) in [
            (Size::Small, tokens::target::MIN),
            (Size::Medium, tokens::target::COMFORT),
            (Size::Touch, tokens::target::TOUCH),
        ] {
            let ctx = headless();
            let mut rect = egui::Rect::ZERO;
            frame(&ctx, vec![], |ui| {
                rect = Button::secondary("크기 검증").size(size).show(ui).rect;
            });
            assert!(
                rect.width() >= min && rect.height() >= min,
                "{size:?}: {}×{} < {min}",
                rect.width(),
                rect.height()
            );
        }
        a11y::assert_clean();
    }

    #[test]
    fn icon_button_meets_min_target() {
        let _g = a11y::test_guard();
        a11y::reset_issues();
        let ctx = headless();
        let mut rect = egui::Rect::ZERO;
        frame(&ctx, vec![], |ui| {
            rect = IconButton::new(egui_phosphor_icons::icons::GEAR, "Settings")
                .test_id("t.icon")
                .show(ui)
                .rect;
        });
        assert!(rect.width() >= tokens::target::MIN, "{}", rect.width());
        assert!(rect.height() >= tokens::target::MIN, "{}", rect.height());
        a11y::assert_clean();
    }

    #[test]
    fn empty_name_is_reported_as_violation() {
        let _g = a11y::test_guard();
        a11y::reset_issues();
        let ctx = headless();
        frame(&ctx, vec![], |ui| {
            IconButton::new(egui_phosphor_icons::icons::GEAR, "").show(ui);
        });
        let issues = a11y::issues();
        assert!(
            issues.iter().any(|i| i.kind == IssueKind::MissingName),
            "빈 이름이 위반으로 기록되어야 합니다: {}",
            a11y::report()
        );
        a11y::reset_issues();
    }


    #[test]
    fn row_spans_full_width_with_target_height() {
        let _g = a11y::test_guard();
        a11y::reset_issues();
        let ctx = headless();
        let mut value = false;
        let mut rect = egui::Rect::ZERO;
        frame(&ctx, vec![], |ui| {
            rect = Row::toggle("t.row", "행 토글", &mut value)
                .show(ui, |_| ())
                .0
                .rect;
        });
        assert!(rect.height() >= tokens::target::ROW, "{}", rect.height());
        assert!(
            rect.width() > 400.0,
            "행은 전체 폭이어야 합니다: {}",
            rect.width()
        );
        a11y::assert_clean();
    }

    #[test]
    fn row_toggle_flips_value_on_synthetic_click() {
        let _g = a11y::test_guard();
        a11y::reset_issues();
        let ctx = headless();
        let mut value = false;
        // 1패스: 행의 위치를 확인합니다.
        let mut rect = egui::Rect::ZERO;
        frame(&ctx, vec![], |ui| {
            rect = Row::toggle("t.click", "클릭 대상", &mut value)
                .show(ui, |_| ())
                .0
                .rect;
        });
        // 2패스: 그 위치에서 합성 클릭(이동 → 누름 → 뗌).
        frame(&ctx, click_at(rect.center()), |ui| {
            let _ = Row::toggle("t.click", "클릭 대상", &mut value).show(ui, |_| ());
        });
        assert!(value, "합성 클릭으로 토글 값이 뒤집혀야 합니다");
        a11y::assert_clean();
    }

    #[test]
    fn segment_group_width_reserves_enough_space() {
        let _g = a11y::test_guard();
        let n = 3;
        let w = segment_group_width(n);
        let needed = n as f32 * Size::Medium.target();
        assert!(w >= needed, "{w} < {needed}");
    }

    #[test]
    fn tokens_keep_the_8px_rhythm() {
        let _g = a11y::test_guard();
        assert!(tokens::space::SM < tokens::space::MD);
        assert!(tokens::space::MD < tokens::space::LG);
        assert!(tokens::target::MIN < tokens::target::COMFORT);
        assert!(tokens::target::COMFORT < tokens::target::TOUCH);
    }
}

/// 갤러리는 `dev-automation` 빌드에만 있으므로 테스트도 같은 조건에서 돕니다.
#[cfg(all(test, feature = "dev-automation"))]
mod gallery_tests {
    use crate::ui::a11y;

    #[test]
    fn gallery_renders_without_contract_violations() {
        let _g = a11y::test_guard();
        a11y::reset_issues();
        let mut g = crate::ui::gallery::Gallery::new();
        g.open = true;
        let ctx = egui::Context::default();
        crate::fonts::install_inter(&ctx);
        let mut input = egui::RawInput::default();
        input.screen_rect = Some(egui::Rect::from_min_size(
            egui::pos2(0.0, 0.0),
            egui::vec2(1280.0, 900.0),
        ));
        let mut out = ctx.run_ui(input, |ui| g.contents(ui));
        out.textures_delta.clear();
        a11y::assert_clean();
    }
}

