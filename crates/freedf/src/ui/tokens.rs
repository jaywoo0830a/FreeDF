//! 디자인 토큰 — 컴포넌트 시스템의 **단일 진실 원천**.
//!
//! 왜 필요한가: 예전에는 컴포넌트마다 여백·반지름·글자 크기·타깃 크기가
//! 호출부 곳곳에 흩어져 있었습니다(실측: 메뉴 정렬 버튼 20×28, 설정 기어 16×28 —
//! 최소 타깃 미달). 값이 흩어지면 "다른 화면에서 다르게 보이는" 문제가 생기고
//! 접근성 하한을 지킬 수 없습니다.
//!
//! 규칙:
//! 1. 컴포넌트는 **여기 있는 값만** 씁니다(매직 넘버 금지).
//! 2. 인터랙티브 요소는 [`target::MIN`] 이상이어야 합니다 —
//!    [`crate::ui::a11y`]가 이를 강제하고 위반을 수집합니다.
//! 3. 여백은 8px 그리드(`space`)를 따릅니다(Bootstrap `$spacer` 리듬과 동일).
//! 4. 색은 [`crate::theme::nord::semantic`]의 의미 토큰만 씁니다(원시 팔레트 금지).

use eframe::egui;

/// 인터랙티브 타깃(터치/클릭 영역) 하한 — 접근성 계약.
/// 컴포넌트 실제 크기는 [`crate::ui::scale`]의 모듈러 정거장 중 이 하한 이상을 고른다.
pub mod target {
    use crate::ui::scale::REM;

    /// 절대 하한 (1.5rem). 이보다 작은 클릭 영역은 만들지 않습니다.
    pub const MIN: f32 = 1.5 * REM;
    /// 데스크톱 기본 (1.75rem) — 대부분의 아이콘 버튼.
    pub const COMFORT: f32 = 1.75 * REM;
    /// 태블릿/터치 기본 (2rem) — 손가락 입력이 주가 되는 컨트롤.
    pub const TOUCH: f32 = 2.0 * REM;
    /// 목록/메뉴 행 높이 (1.75rem) — 행 전체가 타깃일 때.
    pub const ROW: f32 = 1.75 * REM;
}

/// 여백 — 8px 그리드(`layout::SP_*`와 동일 리듬), rem 기반 표현.
pub mod space {
    use crate::ui::scale::REM;

    pub const XS: f32 = 0.125 * REM;
    pub const SM: f32 = 0.25 * REM;
    pub const MD: f32 = 0.5 * REM;
    pub const LG: f32 = 0.75 * REM;
    pub const XL: f32 = 1.0 * REM;
}

/// 모서리 반지름 — rem 기반 표현.
pub mod radius {
    use crate::ui::scale::REM;

    pub const SM: f32 = 0.25 * REM;
    pub const MD: f32 = 0.375 * REM;
    pub const LG: f32 = 0.5 * REM;
    /// 플로팅 오버레이 창 (0.75rem).
    pub const XL: f32 = 0.75 * REM;
}

/// 프레임 **내부 마진** 토큰 — 프레임 종류별로 이름을 붙여 한 곳에서 정의합니다.
/// (예전에는 badge/card/toast/dialog마다 `Margin::symmetric(6,2)` 같은 원시
/// 숫자가 흩어져 있었고, 8px 그리드를 벗어난 6·10 같은 값도 섞여 있었습니다.)
pub mod margin {
    use eframe::egui;

    use super::space;

    /// 배지(badge) — 꽉 찬 소형 프레임.
    pub const BADGE: egui::Margin = egui::Margin::symmetric(space::SM as i8, space::XS as i8);
    /// 키보드 키(<kbd>).
    pub const KBD: egui::Margin = egui::Margin::symmetric(space::SM as i8, space::XS as i8);
    /// 카드/알럿/토스트 — 콘텐츠 프레임 표준.
    pub const CARD: egui::Margin = egui::Margin::symmetric(space::LG as i8, space::MD as i8);
    /// 다이얼로그/모달 본문.
    pub const DIALOG: egui::Margin = egui::Margin::symmetric(space::XL as i8, space::LG as i8);
    /// 기본 카드 컨테이너 (사방 동일).
    pub const CONTAINER: egui::Margin = egui::Margin::same(space::MD as i8);
    /// 탭/칩 — 가로 넉넉·세로 타이트.
    pub const CHIP: egui::Margin = egui::Margin::symmetric(space::MD as i8, space::SM as i8);
    /// 캔버스 오버레이 미니 프레임.
    pub const OVERLAY: egui::Margin = egui::Margin::same(space::SM as i8);
}

/// 글자 크기(pt).
pub mod font {
    pub const CAPTION: f32 = 11.0;
    pub const SMALL: f32 = 12.0;
    pub const BODY: f32 = 14.0;
    pub const TITLE: f32 = 16.0;
}

/// 선 두께.
pub mod stroke {
    pub const HAIRLINE: f32 = 1.0;
    pub const FOCUS: f32 = 2.0;
}

/// 상태 표시자(행 오른쪽의 스위치 트랙) — 토글/라디오 행이 **켜짐/꺼짐을
/// 화면에 보여주도록** 하는 최소 장치. 예전 More 오버레이는 체크박스를 썼지만
/// 행으로 평탄화하면서 상태가 사라졌고, 그래서 명령 행과 토글 행이 똑같이
/// 보이는 어포던스 회귀가 있었습니다(스크린샷 리뷰에서 발견).
pub mod switch {
    /// 트랙 폭.
    pub const W: f32 = 26.0;
    /// 트랙 높이 (= 모서리 반지름의 2배).
    pub const H: f32 = 14.0;
    /// 노브 반지름.
    pub const KNOB: f32 = 5.0;
}

/// 컴포넌트의 상호작용 상태 — 색을 **상태에서 파생**시켜 모든 컴포넌트가
/// 같은 규칙으로 보이게 합니다(예전에는 컴포넌트마다 즉석에서 색을 만들었음).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    Rest,
    Hover,
    Active,
    Selected,
    Disabled,
}

impl State {
    /// `egui::Response`에서 상태를 한 곳에서 판정합니다.
    pub fn of(resp: &egui::Response, selected: bool) -> Self {
        if !resp.enabled() {
            Self::Disabled
        } else if resp.is_pointer_button_down_on() {
            Self::Active
        } else if resp.hovered() {
            Self::Hover
        } else if selected {
            Self::Selected
        } else {
            Self::Rest
        }
    }

    /// 이 상태의 배경 채움. `None` = 채우지 않음.
    pub fn fill(self, ui: &egui::Ui, selected_accent: bool) -> Option<egui::Color32> {
        use crate::theme::nord::semantic as s;
        match self {
            Self::Rest => None,
            Self::Hover => Some(ui.visuals().widgets.hovered.weak_bg_fill),
            Self::Active => Some(ui.visuals().widgets.active.weak_bg_fill),
            Self::Selected => Some(if selected_accent {
                s::ACCENT_SELECT.gamma_multiply(0.35)
            } else {
                ui.visuals().selection.bg_fill
            }),
            Self::Disabled => None,
        }
    }

    /// 이 상태의 테두리.
    pub fn stroke(self, _ui: &egui::Ui) -> Option<egui::Stroke> {
        use crate::theme::nord::semantic as s;
        match self {
            Self::Disabled => Some(egui::Stroke::new(stroke::HAIRLINE, s::BORDER_WEAK)),
            _ => None,
        }
    }

    /// 이 상태의 글자색.
    pub fn text(self, _ui: &egui::Ui) -> egui::Color32 {
        use crate::theme::nord::semantic as s;
        match self {
            Self::Disabled => s::TEXT_FAINT,
            Self::Rest | Self::Hover | Self::Active => s::TEXT_PRIMARY,
            Self::Selected => s::TEXT_STRONG,
        }
    }
}

/// 최소 타깃을 만족시키지 못하는 사각형을 **사방으로 확장**합니다.
/// (시각 크기는 유지하면서 클릭 영역만 넓히는 것이 접근성 표준 방식입니다.)
pub fn expand_to_target(rect: egui::Rect, min: f32) -> egui::Rect {
    let dx = ((min - rect.width()) / 2.0).max(0.0);
    let dy = ((min - rect.height()) / 2.0).max(0.0);
    rect.expand2(egui::vec2(dx, dy))
}

/// 공통 수직 여백 — 컴포넌트가 같은 호흡을 갖도록.
pub fn item_gap() -> egui::Vec2 {
    egui::vec2(space::MD, space::MD)
}
