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
    /// 이 크기의 **실제 한 변** — [`crate::ui::scale`] 모듈러 정거장(×1.5) 중
    /// [`crate::ui::tokens::target`] 접근성 하한을 만족하는 가장 작은 정거장.
    pub fn target(self) -> f32 {
        use crate::ui::scale::{S_24, S_36, S_52};
        match self {
            Self::Small => S_24, // 24px (1.5rem) ≥ target::MIN
            Self::Medium => S_36, // 36px (2.25rem) ≥ target::COMFORT
            Self::Touch => S_52, // 52px (3.25rem) ≥ target::TOUCH
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
            // 표준 컨트롤 높이(S_36, 2.25rem) — 행/툴바와 같은 모듈러 리듬.
            // (Small은 미니 아이콘 전용 — 명시적으로 `.size(Size::Small)`로.)
            size: Size::Medium,
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
            height: crate::ui::scale::S_36, // 행 높이 = 모듈러 정거장 (2.25rem; 하한은 target::ROW)
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
        let on_now = self.value.as_ref().map(|v| **v).unwrap_or(false);
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
        // 세로는 **행 높이 전체**를 슬롯으로 확보합니다 — 표준 컨트롤(S_36)이
        // 행과 같은 높이라 인셋 없이 정확히 안착합니다. (예전에는 COMFORT(28) 기준
        // 인셋이라 36 행에서 표준 컨트롤이 행 밖으로 4pt 넘쳤습니다.)
        let trailing_reserve = if self.trailing_width > 0.0 {
            self.trailing_width + space::SM
        } else {
            space::SM
        };
        // 상태 표시기 자리 — 토글/라디오는 오른쪽에 상태가 **보여야** 합니다.
        // 트레일링 컨트롤이 있으면 그 왼쪽에 나란히 놓습니다(호출부 변경 불필요).
        let indicator = matches!(self.role, a11y::Role::Toggle | a11y::Role::Radio);
        let reserve =
            trailing_reserve + if indicator { tokens::switch::W + space::SM } else { 0.0 };
        let inset_y = 0.0;
        let trect = egui::Rect::from_min_max(
            egui::pos2(rect.right() - reserve, rect.top() + inset_y),
            egui::pos2(rect.right() - space::SM, rect.bottom() - inset_y),
        );

        // 상태 표시자 (행 배경 위, 트레일링 슬롯 왼쪽).
        if indicator && ui.is_rect_visible(rect) {
            use crate::theme::nord::semantic as s;
            let track = egui::Rect::from_center_size(
                egui::pos2(
                    rect.right() - trailing_reserve - tokens::switch::W * 0.5,
                    rect.center().y,
                ),
                egui::vec2(tokens::switch::W, tokens::switch::H),
            );
            match self.role {
                a11y::Role::Toggle => {
                    let (bg, knob_x) = if on_now {
                        (s::ACCENT_SELECT, track.right() - tokens::switch::H * 0.5)
                    } else {
                        (s::BORDER_WEAK, track.left() + tokens::switch::H * 0.5)
                    };
                    let bg = if self.enabled {
                        bg
                    } else {
                        bg.gamma_multiply(0.5)
                    };
                    ui.painter().rect_filled(track, tokens::switch::H * 0.5, bg);
                    ui.painter().circle_filled(
                        egui::pos2(knob_x, track.center().y),
                        tokens::switch::KNOB,
                        if self.enabled {
                            s::TEXT_STRONG
                        } else {
                            s::TEXT_FAINT
                        },
                    );
                }
                a11y::Role::Radio if self.selected => {
                    ui.painter().text(
                        track.center(),
                        egui::Align2::CENTER_CENTER,
                        egui_phosphor_icons::icons::CHECK.0,
                        egui::FontId::new(
                            tokens::font::BODY,
                            egui::FontFamily::Name("phosphor-regular".into()),
                        ),
                        if self.enabled {
                            s::ACCENT_SELECT
                        } else {
                            s::TEXT_FAINT
                        },
                    );
                }
                _ => {}
            }
        }
        let trailing = ui
            .scope_builder(
                egui::UiBuilder::new()
                    .max_rect(trect)
                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
                trailing,
            )
            .inner;

        // egui의 TopDown 레이아웃은 커서를 `widget_rect.max.y + spacing`으로 **덮어씁니다**
        // (`egui::layout::Layout::advance_after_rects`). 트레일링 자식 ui가 그 주체라서
        // 행보다 위쪽에서 커서가 멈추고, 다음 행이 6~11pt **겹칩니다**(실측: stride 17 < 28).
        // 그래서 행 사각형 기준으로 커서를 다시 전진시켜 계약(행 높이 + 간격)을 고정합니다.
        ui.advance_cursor_after_rect(rect);

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


/// 탭 창의 탭 한 개 — 좌측 레일 항목 + 콘텐츠 라우팅 키.
pub struct TabItem<'a> {
    /// 계측 id 조각 (`settings.tab.<id>`) — 공개 계약이라 바꾸지 마세요.
    pub id: &'a str,
    /// 화면 라벨이자 접근성 이름.
    pub label: &'a str,
    /// 레일 아이콘(있으면 메뉴 행과 같은 아이콘 레일에 정렬됩니다).
    pub icon: Option<Icon>,
    /// 툴팁 힌트.
    pub hint: String,
}

impl<'a> TabItem<'a> {
    pub fn new(id: &'a str, label: &'a str) -> Self {
        Self {
            id,
            label,
            icon: None,
            hint: String::new(),
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
}

/// 탭 창(설정·환경설정) 크롬 — 제목 · 좌측 탭 레일 · 콘텐츠 패널 · 닫기.
///
/// 왜 컴포넌트인가: 예전 Settings 창은 창 크롬·탭 레일·콘텐츠 배치가 앱 코드 안에
/// 흩어져 있어 **테스트할 수 없었고**(헤드리스에서 창을 띄우려면 앱 전체가 필요),
/// 레일 항목은 최소 타깃·접근성 이름·계측이 전부 없었습니다(실측: 선택 라벨
/// 20pt, `settings.*` 계측 0개). 크롬을 키트로 옮기면:
///
/// * 디자인 일관성 — 메뉴/갤러리와 **같은 행 컴포넌트·같은 높이·같은 아이콘 레일**.
/// * 접근성 — 모든 레일 항목이 [`a11y::finish`]를 통과(이름·타깃·선택 상태 보고).
/// * 테스트 가능 — 헤드리스에서 창을 그려 계약을 검증(앱 상태 불필요),
///   실앱에서는 `settings.tab.*` id로 자동화가 탭을 순회.
///
/// 콘텐츠는 **선택된 탭에 대해서만** 클로저가 호출됩니다(닫혀 있으면 호출 안 됨).
pub struct TabbedWindow<'a> {
    prefix: &'a str,
    title: &'a str,
    subtitle: &'a str,
    tabs: &'a [TabItem<'a>],
    selected: usize,
    open: bool,
    size: egui::Vec2,
    /// 창 자동 크기 상한 (None = 무제한).
    max_size: Option<egui::Vec2>,
    rail_width: f32,
}

/// 탭 창 렌더 결과.
pub struct TabsOutcome {
    /// 창이 아직 열려 있는가(닫기 버튼/X/Esc 반영).
    pub open: bool,
    /// 선택된 탭 인덱스(범위 클램프 후).
    pub selected: usize,
    /// 창 **프레임** rect — 캡처 크롭·존재 판정용(닫혀 있으면 `None`).
    pub frame: Option<egui::Rect>,
}

impl<'a> TabbedWindow<'a> {
    /// `prefix`는 계측 id 접두입니다(`"settings"` → `settings.window` …).
    pub fn new(
        prefix: &'a str,
        title: &'a str,
        tabs: &'a [TabItem<'a>],
        selected: usize,
        open: bool,
    ) -> Self {
        Self {
            prefix,
            title,
            subtitle: "",
            tabs,
            selected,
            open,
            size: egui::vec2(crate::ui::scale::rem(46), crate::ui::scale::rem(33)),
            // 창 자동 크기 상한 — 콘텐츠(ScrollArea fill)와 창 크기의 피드백 루프가
            // 있으면 창이 화면 끝까지 자랍니다(실측: edge_scroll 탭, 820px 전체).
            // 대화상자 상한 960×640 — 그리드 콘텐츠 최소 폭(~865)도 수용합니다.
            max_size: Some(egui::vec2(crate::ui::scale::rem(60), crate::ui::scale::rem(40))),
            rail_width: crate::ui::scale::rem(13),
        }
    }

    /// 제목 아래 조작 힌트 (예: "Esc to close · ↑↓ to switch sections").
    pub fn subtitle(mut self, s: &'a str) -> Self {
        self.subtitle = s;
        self
    }

    pub fn size(mut self, w: f32, h: f32) -> Self {
        self.size = egui::vec2(w, h);
        self
    }

    pub fn rail_width(mut self, w: f32) -> Self {
        self.rail_width = w;
        self
    }

    /// 창을 그립니다. 반환값 = (아직 열려 있는가, 선택된 탭 인덱스).
    ///
    /// `content`는 **선택된 탭에 대해서만** 호출되며, 콘텐츠 패널 안쪽 `ui`를
    /// 받습니다(입력 컨트롤의 최소 타깃은 여기서 일괄 강제됩니다).
    pub fn show<R>(
        self,
        ctx: &egui::Context,
        content: impl FnOnce(&mut egui::Ui, &TabItem<'a>) -> R,
    ) -> TabsOutcome {
        let mut open = self.open;
        let mut close_requested = false;
        let mut selected = self.selected.min(self.tabs.len().saturating_sub(1));
        if self.tabs.is_empty() {
            return TabsOutcome {
                open,
                selected,
                frame: None,
            };
        }
        let prefix = self.prefix;
        let title = self.title;
        let subtitle = self.subtitle;
        let tabs = self.tabs;
        let size = self.size;
        let max_size = self.max_size;
        let rail_width = self.rail_width;
        let close_id = format!("{prefix}.close");
        let heading_id = format!("{prefix}.title");

        let shown = egui::Window::new(title)
            .id(egui::Id::new((prefix, "window")))
            .default_size(size)
            .resizable(true)
            // 창 자동 크기 상한 (피드백 루프 차단 — 실측 주석은 TabbedWindow::new 참조).
            .max_size(max_size.unwrap_or(egui::vec2(f32::INFINITY, f32::INFINITY)))
            // 실측: 1280px 뷰포트에서 기본 위치로 열면 창이 오른쪽으로 넘쳐
            // 슬라이더 값 박스가 잘렸습니다 — 화면 안으로 클램프합니다.
            .constrain(true)
            // egui 기본 타이틀바를 끕니다 — 우리 헤더(제목·힌트·닫기)가 유일한
            // 크롬입니다. 실측: 기본 타이틀바와 우리 헤더가 ✕/제목을 **중복**
            // 표시하고, 헤더가 넘치면 왼쪽 제목이 잘렸습니다.
            .title_bar(false)
            .open(&mut open)
            .show(ctx, |ui| {
                // 창 내용 rect(레이아웃 검증용) — 창 **프레임** 계약 id
                // (`settings.window`)는 호출부가 프레임 rect로 공개합니다.
                crate::app::dev::publish_rect(ui, format!("{prefix}.pane"), ui.max_rect());

                // ── 헤더: 제목 · 힌트 · 닫기(명시적 버튼 = Esc 없이도 닫힘) ──
                let header = ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(title)
                            .strong()
                            .size(tokens::font::TITLE),
                    );
                    if !subtitle.is_empty() {
                        // 넘치면 줄임(…) — 넘친 헤더가 제목을 밀어내지 않게.
                        ui.add(
                            egui::Label::new(egui::RichText::new(subtitle).weak().small())
                                .truncate(),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let close = IconButton::new(egui_phosphor_icons::icons::X, "Close")
                            .hint("Close this window (Esc)")
                            .test_id(close_id.as_str())
                            .show(ui);
                        if close.clicked() {
                            close_requested = true;
                        }
                    });
                });
                let header_h = header.response.rect.height();
                ui.separator();

                // 본문 높이는 **창 자신의 크기**(`max_rect`)에서 헤더를 뺀 값입니다.
                // `available_size().y`에 기대면 내용이 창을 다시 줄이는 되먹임이
                // 생겨 창이 389pt로 줄고 마지막 탭이 화면 밖으로 나갑니다(실측).
                let header_sep = ui.spacing().item_spacing.y;
                let body_h = (ui.max_rect().height() - header_h - header_sep)
                    .max(crate::ui::scale::rem(8));
                let rail_h = body_h;
                let content_h = (body_h - crate::ui::scale::rem(2)).max(crate::ui::scale::rem(8));

                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(rail_width, rail_h),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            egui::ScrollArea::vertical()
                                .id_salt((prefix, "rail"))
                                .max_height(rail_h)
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    for (i, tab) in tabs.iter().enumerate() {
                                        let id = format!("{prefix}.tab.{}", tab.id);
                                        let icon = tab
                                            .icon
                                            .unwrap_or(egui_phosphor_icons::icons::DOT_OUTLINE);
                                        let (row, _) = Row::radio(&id, tab.label, i == selected)
                                            .icon(icon)
                                            .hint(tab.hint.clone())
                                            .show(ui, |_| ());
                                        if row.clicked() {
                                            selected = i;
                                        }
                                    }
                                });
                        },
                    );
                    ui.separator();
                    // 콘텐츠 폭은 **separator 뒤 실제 남은 폭**입니다 — 미리
                    // max_rect로 빼면 separator·item_spacing 폭이 빠져 콘텐츠가
                    // 패널을 넘쳤습니다(실측: 값 칩이 창 우측에서 잘림).
                    let content_w = ui.available_width().max(crate::ui::scale::rem(16));
                    let content_alloc = ui.allocate_ui_with_layout(
                        egui::vec2(content_w, body_h),
                        egui::Layout::top_down(egui::Align::LEFT),
                        |ui| {
                            let tab = &tabs[selected];
                            let heading = format!("{} settings", tab.label);
                            let resp = ui.label(
                                egui::RichText::new(&heading)
                                    .strong()
                                    .size(tokens::font::TITLE),
                            );
                            let _ = a11y::finish(
                                ui,
                                Spec::label(heading_id.as_str(), &heading)
                                    .hint(tab.hint.as_str()),
                                resp,
                            );
                            egui::ScrollArea::vertical()
                                .id_salt((prefix, "content"))
                                .max_height(content_h)
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    // **디자인 시스템 일괄 적용**: 이 패널의 모든 입력
                                    // (슬라이더·체크박스·DragValue·텍스트·접기 헤더)이
                                    // 최소 타깃(28pt = COMFORT)을 갖습니다. 예전에는
                                    // egui 기본값(~18pt)이라 11개 탭 전부 하한 미달이었습니다.
                                    ui.spacing_mut().interact_size.y =
                                        tokens::target::COMFORT;
                                    // 스크롤바 자리 확보 — 슬라이더 값(0.300 등)이
                                    // 스크롤바에 가리는 실측 회귀를 막습니다.
                                    let inner_w = (ui.available_width() - space::MD)
                                        .max(crate::ui::scale::rem(8));
                                    ui.set_max_width(inner_w);
                                    // 슬라이더가 **패널 폭을 채우게** 합니다 — 값 칩이
                                    // 오른쪽 끝에 정렬돼 11개 탭이 같은 리듬을 갖습니다.
                                    ui.spacing_mut().slider_width = (inner_w - 72.0).max(80.0);
                                    content(ui, tab);
                                });
                        },
                    );
                    // 콘텐츠 패널 rect는 **할당된 패널**입니다. ScrollArea의 clip은
                    // 내용이 짧은 탭에서 창 전체로 넓어져 레일과 겹친 것처럼
                    // 보입니다(실측: gamepad 탭) — 그래서 할당 rect를 씁니다.
                    let content_id = format!("{prefix}.content.{}", tabs[selected].id);
                    crate::app::dev::publish_rect(
                        ui,
                        content_id,
                        content_alloc.response.rect,
                    );
                });
            });

        if close_requested {
            open = false;
        }
        TabsOutcome {
            open,
            selected,
            frame: if open {
                // 프레임 rect는 창 마진까지 포함해 화면 밖으로 살짝 나갈 수 있어
                // (실측: 1280px 뷰포트에서 +12px) 감사의 offscreen 오탐과 크롭
                // 어긋남이 생겼습니다 — 화면 안으로 클램프해 공개합니다.
                shown.map(|r| r.response.rect.intersect(ctx.content_rect()))
            } else {
                None
            },
        }
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

    /// 행은 **세로로 겹치면 안 됩니다**(겹치면 아래 행이 위 행의 하단 클릭 영역을
    /// 훔칩니다 — eguidev `layout_issues`의 `overlap`으로 실측된 결함).
    /// 이 테스트가 팝업 밖에서의 순수 레이아웃 계약을 고정합니다.
    #[test]
    fn rows_stack_without_vertical_overlap() {
        let _g = a11y::test_guard();
        a11y::reset_issues();
        let ctx = headless();
        let mut rects: Vec<egui::Rect> = Vec::new();
        frame(&ctx, vec![], |ui| {
            let (a, _) = Row::action("t.row.a", "Row A").show(ui, |_| ());
            let (b, _) = Row::action("t.row.b", "Row B").show(ui, |_| ());
            rects = vec![a.rect, b.rect];
        });
        assert_eq!(rects.len(), 2);
        let expected_h = crate::ui::scale::S_36;
        assert!(
            (rects[0].height() - expected_h).abs() < 0.5,
            "행 높이가 모듈러 정거장 S_36({})와 다릅니다: {}",
            expected_h,
            rects[0].height()
        );
        assert!(
            rects[1].top() >= rects[0].bottom(),
            "행이 세로로 겹칩니다: A={:?} B={:?} (stride {:.1} < height {:.1})",
            rects[0],
            rects[1],
            rects[1].top() - rects[0].top(),
            rects[0].height()
        );
    }

    /// 트레일링 슬롯은 **행 안쪽**에 머물러야 합니다. 밖으로 나가면 팝업이 그만큼
    /// 커지고 뒤따르는 행들이 넓어진 폭을 따라가 레이아웃이 연쇄로 깨집니다(실측:
    /// 라벨 있는 세그먼트가 92pt 예약을 넘겨 팝업이 304 → 522pt로 커짐).
    /// 슬롯 안의 **컨트롤**도 행을 넘으면 안 됩니다(실측: 28pt ⚙가 24pt 슬롯에서
    /// 4pt 넘쳐 다음 행과 부분 겹침).
    #[test]
    fn row_trailing_stays_inside_the_row() {
        let _g = a11y::test_guard();
        a11y::reset_issues();
        let ctx = headless();
        let row_width = 300.0;
        let mut seen: Vec<(egui::Rect, egui::Rect, Vec<egui::Rect>)> = Vec::new();
        frame(&ctx, vec![], |ui| {
            ui.set_max_width(row_width);
            for id in ["t.trail.a", "t.trail.b"] {
                let mut controls: Vec<egui::Rect> = Vec::new();
                let (row, slot) = Row::action(id, "Trailing")
                    .trailing_width(segment_group_width(2))
                    .show(ui, |ui| {
                        controls.push(
                            IconButton::new(egui_phosphor_icons::icons::GEAR, "Slot")
                                .show(ui)
                                .rect,
                        );
                        controls.push(
                            IconButton::new(
                                egui_phosphor_icons::icons::MAGNIFYING_GLASS,
                                "Slot 2",
                            )
                            .show(ui)
                            .rect,
                        );
                        ui.min_rect()
                    });
                seen.push((row.rect, slot, controls));
            }
        });
        for (row, slot, controls) in &seen {
            assert!(
                slot.width() <= segment_group_width(2) + tokens::space::SM,
                "트레일링 슬롯 내용이 예약({:.0})을 넘었습니다: {:.1}",
                segment_group_width(2),
                slot.width()
            );
            assert!(
                slot.left() >= row.left() && slot.right() <= row.right(),
                "트레일링 슬롯이 행 밖으로 나갔습니다: row={row:?} slot={slot:?}"
            );
            for c in controls {
                assert!(
                    c.top() >= row.top() - 0.5 && c.bottom() <= row.bottom() + 0.5,
                    "트레일링 컨트롤이 행 밖으로 넘쳤습니다: row={row:?} control={c:?}"
                );
            }
        }
    }

    /// 탭 창은 **선택된 탭의 내용만** 그립니다(닫혀 있으면 아무것도).
    /// 그리고 모든 레일 항목이 계약(이름·최소 타깃)을 통과해야 합니다.
    /// 앱 상태 없이 크롬만 검증하므로 0.0x초에 돕니다.
    #[test]
    fn tabbed_window_renders_only_the_selected_tab() {
        let _g = a11y::test_guard();
        a11y::reset_issues();
        let ctx = headless();
        let items = vec![
            TabItem::new("draw", "Draw").icon(egui_phosphor_icons::icons::PENCIL),
            TabItem::new("cursor", "Cursor").icon(egui_phosphor_icons::icons::CURSOR),
            TabItem::new("paper", "Paper").icon(egui_phosphor_icons::icons::FILE_TEXT),
        ];
        let mut drawn: Vec<String> = Vec::new();
        let mut returned: TabsOutcome = TabsOutcome {
            open: false,
            selected: 0,
            frame: None,
        };
        frame(&ctx, vec![], |ui| {
            returned = TabbedWindow::new("settings", "Settings", &items, 1, true)
            .subtitle("Esc to close")
            .show(ui.ctx(), |ui, item| {
                drawn.push(item.id.to_string());
                // 콘텐츠 패널의 입력은 최소 타깃(COMFORT)을 갖습니다.
                assert!(
                    ui.spacing().interact_size.y >= tokens::target::MIN,
                    "콘텐츠 패널 입력 타깃이 {}pt 입니다",
                    ui.spacing().interact_size.y
                );
            });
    });
    assert_eq!(drawn, vec!["cursor".to_string()], "선택된 탭만 그려야 합니다");
    assert!(returned.open && returned.selected == 1);
    assert!(returned.frame.is_some(), "열린 창은 프레임 rect를 공개합니다");
    a11y::assert_clean();
    }

    /// 닫혀 있으면 내용을 그리지 않고, 열려 있으면 **선택된 탭만** 그립니다.
    /// (시각 검증은 스크린샷 리뷰가 담당 — 테스트는 최소로 유지합니다.)
    #[test]
    fn tabbed_window_draws_only_the_selected_tab() {
        let ctx = headless();
        let items = vec![TabItem::new("a", "A"), TabItem::new("b", "B")];
        let mut drawn: Vec<String> = Vec::new();
        let mut closed = (true, 0);
        let mut opened = (false, 99);
        frame(&ctx, vec![], |ui| {
            let a = TabbedWindow::new("settings", "Settings", &items, 0, false)
                .show(ui.ctx(), |_, item| drawn.push(item.id.to_string()));
            closed = (a.open, a.selected);
            let b = TabbedWindow::new("settings", "Settings", &items, 99, true)
                .show(ui.ctx(), |_, item| drawn.push(item.id.to_string()));
            opened = (b.open, b.selected);
        });
        assert_eq!(closed, (false, 0), "닫힌 창은 상태를 유지해야 합니다");
        assert_eq!(opened, (true, 1), "범위를 벗어난 선택은 마지막 탭으로 클램프");
        assert_eq!(drawn, vec!["b".to_string()], "선택된 탭만 그려야 합니다");
    }

    /// 토글/라디오 행은 **상태 표시자 자리를 비워 둬야** 합니다(그러지 않으면
    /// 트레일링 컨트롤이 스위치를 덮어 켜짐/꺼짐이 안 보입니다 — 실측 회귀).
    #[test]
    fn toggle_row_reserves_space_for_the_state_indicator() {
        let _g = a11y::test_guard();
        a11y::reset_issues();
        let ctx = headless();
        let mut seen: Vec<(egui::Rect, egui::Rect)> = Vec::new();
        frame(&ctx, vec![], |ui| {
            ui.set_max_width(300.0);
            let mut on = false;
            let (row, slot) = Row::toggle("t.toggle.a", "Toggle", &mut on)
                .trailing_width(tokens::target::COMFORT)
                .show(ui, |ui| {
                    IconButton::new(egui_phosphor_icons::icons::GEAR, "Slot").show(ui);
                    ui.min_rect()
                });
            seen.push((row.rect, slot));
        });
        for (row, slot) in &seen {
            assert!(
                slot.right() <= row.right() - tokens::switch::W,
                "상태 표시자 자리가 예약되지 않았습니다: row={row:?} slot={slot:?}"
            );
        }
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

