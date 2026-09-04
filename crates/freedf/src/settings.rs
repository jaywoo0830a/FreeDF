//! 앱 세션 상태 — 전역 기본값과 문서별 GUI 상태를 모두 `SessionState` 하나로 관리.
//!
//! FreeDF v2: 영속화는 PostgreSQL이 담당합니다.
//! - **전역 기본 세션** → `app_state` 테이블 (key = 'session')
//! - **문서별 세션** → `sessions` 테이블 (doc_id 기준)
//!
//! 이 모듈은 상태 구조체와 기본값만 정의하고, JSONB 변환은 `serde_json`으로 앱이 처리합니다.

use freedf_core::ink::InkGrain;
use freedf_core::paper::PaperSurfaceSettings;
use freedf_core::model::ToolType;
use freedf_core::paper::{PaperSize, PaperStyle, PaperStyleSettings};
use freedf_core::pen::{BallPenProfile, ColorFamily, FountainProfile, InkSoak};
use freedf_core::transform::{PageAlign, MAX_ZOOM, MIN_ZOOM};
use serde::{Deserialize, Serialize};

// ── 범위 검증 뉴타입 ─────────────────────────────────────────────
// 스칼라 설정값은 원시 f32/u8 대신 아래 뉴타입으로만 선언합니다.
// 규칙: "불가능한 값은 타입으로 존재할 수 없다" — 생성자(new)와
// 역직렬화(Deserialize)가 모두 클램프하므로, 앱 코드는 별도 검증 없이
// `.get()`으로 꺼내 쓰기만 하면 됩니다.
// 새 범위 타입은 `bounded_f32!` 매크로로 같은 양식을 강제합니다.

/// `f32` 범위 제한 타입 선언 — 생성/역직렬화 시점에 `[min, max]`로 클램프.
macro_rules! bounded_f32 {
    ($name:ident, $min:expr, $max:expr, $default:expr, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, Copy, PartialEq)]
        pub struct $name(f32);

        impl $name {
            /// 범위 밖 값은 생성 시점에 클램프됩니다.
            pub fn new(v: f32) -> Self {
                Self(v.clamp($min, $max))
            }

            pub fn get(self) -> f32 {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new($default)
            }
        }

        impl Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_f32(self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                Ok(Self::new(f32::deserialize(d)?))
            }
        }
    };
}

bounded_f32!(InkWidth, 0.5, 12.0, 2.0, "필기 두께 (볼펜/만년필 공용, 0.5..12).");
bounded_f32!(HighlighterWidth, 4.0, 40.0, 16.0, "하이라이터 두께 (4..40).");
bounded_f32!(EraserRadius, 4.0, 60.0, 16.0, "지우개 반경 (4..60).");
bounded_f32!(Zoom, MIN_ZOOM, MAX_ZOOM, 1.0, "줌 배율 (MIN_ZOOM..MAX_ZOOM).");
bounded_f32!(TextureStrength, 0.0, 1.0, 0.25, "종이 질감 강도 (0..1).");
bounded_f32!(SmoothingStrength, 0.0, 1.0, 0.4, "스무딩 강도 (0..1).");
bounded_f32!(EdgeZone, 8.0, 300.0, 72.0, "엣지 반응 영역 폭 (8..300 px).");
bounded_f32!(EdgeSpeed, 20.0, 4000.0, 480.0, "엣지 스크롤 최대 속도 (20..4000 px/s).");
bounded_f32!(EdgeOverscroll, 0.0, 2000.0, 64.0, "페이지 바깥 패닝 여유 (0..2000 px).");
bounded_f32!(EdgeDelay, 0.0, 3.0, 0.5, "엣지 반응 지연 (0..3초).");
bounded_f32!(FocusDwell, 0.0, 5.0, 0.5, "창 포커스 지연 (0..5초).");
bounded_f32!(CustomPaperDim, 100.0, 2400.0, 595.276, "사용자 용지 한 변 (100..2400 pt).");

/// 종이 질감 프리셋 단계 (0..=4).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextureLevel(u8);

impl TextureLevel {
    pub fn new(v: u8) -> Self {
        Self(v.min(4))
    }

    pub fn get(self) -> u8 {
        self.0
    }
}

impl Default for TextureLevel {
    fn default() -> Self {
        Self(2)
    }
}

impl Serialize for TextureLevel {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u8(self.0)
    }
}

impl<'de> Deserialize<'de> for TextureLevel {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        u8::deserialize(d).map(Self::new)
    }
}

/// 필기 도구(볼펜/만년필) 공용 잉크 묶음 — 두 도구가 같은 형태를 공유합니다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InkToolState {
    pub color: [u8; 4],
    pub width: InkWidth,
    /// 잉크 스밈.
    pub soak: InkSoak,
    /// 잉크 질감.
    pub grain: InkGrain,
}

impl Default for InkToolState {
    /// 볼펜 기준 기본값 (만년필은 [`InkToolState::fountain_default`]).
    fn default() -> Self {
        Self {
            color: [26, 26, 28, 255],
            width: InkWidth::default(),
            soak: InkSoak::ballpoint_default(),
            grain: InkGrain::default(),
        }
    }
}

impl InkToolState {
    /// 만년필 기준 기본값 — 스밈만 다르고 나머지는 볼펜과 동일.
    fn fountain_default() -> Self {
        Self {
            soak: InkSoak::fountain_default(),
            ..Self::default()
        }
    }
}

/// 볼펜 설정 묶음 — 잉크 설정 + 물리 모델 프로파일.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PenState {
    pub ink: InkToolState,
    pub profile: BallPenProfile,
}

impl Default for PenState {
    fn default() -> Self {
        Self {
            ink: InkToolState::default(),
            profile: BallPenProfile::default(),
        }
    }
}

/// 만년필 설정 묶음 — 볼펜과 완전히 독립.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FountainState {
    pub ink: InkToolState,
    pub profile: FountainProfile,
}

impl Default for FountainState {
    fn default() -> Self {
        Self {
            ink: InkToolState::fountain_default(),
            profile: FountainProfile::default(),
        }
    }
}

/// 하이라이터 묶음 — 색/두께만 (잉크 물리 설정 없음).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HighlighterState {
    pub color: [u8; 4],
    pub width: HighlighterWidth,
}

impl Default for HighlighterState {
    fn default() -> Self {
        Self {
            color: [255, 230, 109, 120],
            width: HighlighterWidth::default(),
        }
    }
}

/// 활성 도구 + 도구 공통 설정 묶음.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolState {
    pub active: ToolType,
    pub color_family: ColorFamily,
    pub eraser_radius: EraserRadius,
    pub pressure_enabled: bool,
    /// 마우스/트랙패드로도 잉크를 그릴지 (기본 off — 펜 전용 필기)
    pub mouse_draws: bool,
}

impl Default for ToolState {
    fn default() -> Self {
        Self {
            active: ToolType::Pen,
            color_family: ColorFamily::Black,
            eraser_radius: EraserRadius::default(),
            pressure_enabled: true,
            mouse_draws: false,
        }
    }
}

/// 화면 뷰 상태 (줌/팬/정렬/줌잠금) — 문서별 세션에서만 의미 있습니다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewState {
    /// 줌 배율 (화면 픽셀 / 페이지 포인트)
    pub zoom: Zoom,
    /// 페이지 가로 오프셋 (화면 좌표)
    pub pan_x: f32,
    pub pan_y: f32,
    pub page_align: PageAlign,
    /// 줌 잠금 — 잠그면 휠/핀치/단축키/버튼 줌이 전부 무시됩니다 (실수 방지)
    pub zoom_lock: bool,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            zoom: Zoom::default(),
            pan_x: 0.0,
            pan_y: 0.0,
            page_align: PageAlign::Center,
            zoom_lock: false,
        }
    }
}

/// 종이 외관 묶음 (스타일/색/크기/서라운드).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PaperState {
    pub style: PaperStyle,
    pub color: [u8; 4],
    pub size: PaperSize,
    /// 사용자 정의 용지 크기 [가로, 세로] (포인트). `PaperSize::Custom`일 때 사용.
    pub custom_size: Option<[CustomPaperDim; 2]>,
    /// 캔버스(페이지 뒤 서라운드) 배경색.
    pub canvas_color: [u8; 4],
    /// 스타일별(Ruled/Grid/Dotted) 줄/점 세부설정 — 각 스타일 독립.
    pub style_settings: PaperStyleSettings,
}

impl Default for PaperState {
    fn default() -> Self {
        Self {
            style: PaperStyle::Blank,
            color: [255, 255, 255, 255],
            size: PaperSize::A4,
            custom_size: None,
            canvas_color: default_canvas_color(),
            style_settings: PaperStyleSettings::default(),
        }
    }
}

/// 종이 질감 묶음 (프리셋 단계 + 커스텀 값).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextureState {
    /// 질감 표시 여부 (기본 켜짐).
    pub enabled: bool,
    /// 강도 0..1 (기본 0.25 — 은은한 수준).
    pub strength: TextureStrength,
    /// 초보자 프리셋 단계 0..=4 (Lowest..Highest). Custom이 꺼져 있을 때
    /// 이 단계가 강도·표면 값을 지배합니다.
    pub level: TextureLevel,
    /// 상세 값(강도·표면·조명)을 직접 조절할지.
    pub custom: bool,
    /// 종이 표면 물리 모델 (요철·조명·반사율).
    pub surface: PaperSurfaceSettings,
}

impl Default for TextureState {
    fn default() -> Self {
        Self {
            enabled: true,
            strength: TextureStrength::default(),
            level: TextureLevel::default(),
            custom: false,
            surface: PaperSurfaceSettings::default(),
        }
    }
}

/// 패널/팔레트 표시 묶음.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PanelsState {
    pub show_notes: bool,
    pub show_outline: bool,
    /// 캔버스 오른쪽 필기구/색상 팔레트 표시 여부 (전역 기본값)
    pub show_palette: bool,
    /// 자주 쓰는 펜 색상 팔레트 (전역 기본값)
    pub favorite_colors: Vec<[u8; 4]>,
    /// 하이라이터가 문서 텍스트를 인식해 깔끔하게 칠하는 모드.
    pub text_highlight_snap: bool,
    /// 도구 선택기 순서 (드래그 앤 드롭 재정렬, 전역 기본값)
    pub tool_order: Vec<ToolType>,
}

impl Default for PanelsState {
    fn default() -> Self {
        Self {
            show_notes: true,
            show_outline: false,
            show_palette: true,
            favorite_colors: default_favorite_colors(),
            text_highlight_snap: false,
            tool_order: ToolType::default_order(),
        }
    }
}

/// 펜 입력 스무딩 묶음.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SmoothingState {
    /// 강도 0..1 — 0이면 원본 그대로.
    pub strength: SmoothingStrength,
    /// 사용 여부 (기본 off — OTD 등 외부 드라이버 안정화와 충돌 방지)
    pub enabled: bool,
}

impl Default for SmoothingState {
    fn default() -> Self {
        Self {
            strength: SmoothingStrength::default(),
            enabled: false,
        }
    }
}

/// 엣지 자동 스크롤 묶음 (전역).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EdgeAutoscrollState {
    /// 커서가 캔버스 가장자리 근처에 닿으면 그 방향으로 자동 패닝 (기본 꺼짐).
    pub enabled: bool,
    /// **펜으로 커서를 움직일 때만**(호버/접촉) 반응할지 (기본 true).
    pub pen_only: bool,
    /// 반응 영역 폭 (화면 px)
    pub zone: EdgeZone,
    /// 방향별 최대 속도 [왼쪽, 오른쪽, 위, 아래] (화면 px/초)
    pub speeds: [EdgeSpeed; 4],
    /// 페이지(문서) 바깥으로 더 패닝할 수 있는 여유 (화면 px)
    pub overscroll: EdgeOverscroll,
    /// 활성 가장자리의 "숨쉬는" 글로우 표시.
    pub pulse: bool,
    /// 방향별 반응 지연(초) [왼쪽, 오른쪽, 위, 아래] — 0이면 즉시.
    pub delays: [EdgeDelay; 4],
}

impl Default for EdgeAutoscrollState {
    fn default() -> Self {
        Self {
            enabled: false,
            pen_only: true,
            zone: EdgeZone::default(),
            speeds: [EdgeSpeed::default(); 4],
            overscroll: EdgeOverscroll::default(),
            pulse: true,
            delays: [EdgeDelay::default(); 4],
        }
    }
}

/// 창 포커스 추적 묶음 (스플릿 뷰, 창마다 독립).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowFocusState {
    /// 커서가 창 위에서 움직이면 이 창을 포커스할지 (기본 꺼짐).
    pub on_move: bool,
    /// 지연(초) — 커서가 창 위에 이 시간 이상 머물면 포커스.
    /// 0이면 커서가 올라가는 즉시 포커스합니다.
    pub dwell_sec: FocusDwell,
}

impl Default for WindowFocusState {
    fn default() -> Self {
        Self {
            on_move: false,
            dwell_sec: FocusDwell::default(),
        }
    }
}

/// 전역 보조 기능 묶음.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GlobalState {
    /// 디버그 HUD(실시간 입력값 오버레이) 표시 여부.
    pub debug_hud: bool,
    /// 왼손잡이 여부 — 펜 커서 배럴이 왼쪽(2·3사분면)만 가리킵니다.
    /// (오른손잡이는 오른쪽 1·4사분면만)
    pub left_handed: bool,
    /// 단어 탭 시 사전 오버레이 표시 여부.
    pub dictionary_enabled: bool,
}

impl Default for GlobalState {
    fn default() -> Self {
        Self {
            debug_hud: false,
            left_handed: false,
            dictionary_enabled: false,
        }
    }
}

/// GUI 세션 상태.
///
/// - 전역 기본값: 시작 시 DB `app_state('session')`에서 로드해 툴바 기본값
///   (펜 색, 용지, 도구 등)을 복원합니다.
/// - 문서별 상태: DB `sessions` 테이블에 저장해 문서를 다시 열 때
///   마지막 페이지·줌/팬·도구·펜·용지를 복원합니다.
///
/// **구성 규칙 (모든 설정 묶음이 따르는 하나의 양식):**
/// 1. 관련 필드는 전부 `#[serde(default)]` 그룹 구조체로 묶습니다 (플랫 필드 없음).
/// 2. 기본값은 각 그룹의 `Default` 한 곳에서만 정의합니다.
/// 3. 스칼라 값 범위는 `bounded_f32!`/`TextureLevel` 뉴타입이 타입으로 강제 —
///    앱 코드는 검증 없이 `.get()`으로 읽기만 합니다.
/// 4. 교차 필드 보정(프리셋 반영/팔레트 수리)만 [`SessionState::sanitized`]에서.
/// 5. 앱 ↔ 세션 매핑은 `FreeDfApp::capture_session` / `apply_session` 한 쌍으로만.
/// 6. `#[serde(default)]`라 과거 버전 파일(필드 누락/구조 변경)도 안전하게 로드됩니다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionState {
    /// 마지막으로 열었던 페이지 인덱스
    pub page: usize,
    /// 활성 도구 + 도구 공통 설정.
    pub tool: ToolState,
    /// 볼펜 설정.
    pub pen: PenState,
    /// 만년필 설정.
    pub fountain: FountainState,
    /// 하이라이터 설정.
    pub highlighter: HighlighterState,
    /// 화면 뷰 (줌/팬/정렬/줌잠금) — 문서별 상태.
    pub view: ViewState,
    /// 종이 외관.
    pub paper: PaperState,
    /// 종이 질감.
    pub texture: TextureState,
    /// 패널/팔레트 표시.
    pub panels: PanelsState,
    /// 펜 입력 스무딩.
    pub smoothing: SmoothingState,
    /// 엣지 자동 스크롤 (전역).
    pub edge_autoscroll: EdgeAutoscrollState,
    /// 창 포커스 추적 (전역).
    pub window_focus: WindowFocusState,
    /// 전역 보조 기능 (HUD/왼손잡이/사전).
    pub global: GlobalState,
}

/// 캔버스 서라운드 기본색 — Nord NORD0 (#2E3440, 다크 테마 기본).
fn default_canvas_color() -> [u8; 4] {
    crate::theme::nord::semantic::PAGE_SURROUND.to_array()
}

/// 자주 쓰는 색 팔레트(원형 휠/사이드 팔레트 공용) 최대 개수 — 기본 8색.
pub const MAX_FAVORITE_COLORS: usize = 8;

/// 기본 즐겨찾기 색상 3개 (GoodNotes 블랙/레드/블루).
fn default_favorite_colors() -> Vec<[u8; 4]> {
    vec![
        [26, 26, 28, 255],   // Black (#1A1A1C)
        [255, 71, 66, 255],  // Red (#FF4742)
        [72, 166, 235, 255], // Blue (#48A6EB)
    ]
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            page: 0,
            tool: ToolState::default(),
            pen: PenState::default(),
            fountain: FountainState::default(),
            highlighter: HighlighterState::default(),
            view: ViewState::default(),
            paper: PaperState::default(),
            texture: TextureState::default(),
            panels: PanelsState::default(),
            smoothing: SmoothingState::default(),
            edge_autoscroll: EdgeAutoscrollState::default(),
            window_focus: WindowFocusState::default(),
            global: GlobalState::default(),
        }
    }
}

impl SessionState {
    /// 로드된 상태 정규화 — 스칼라 범위는 뉴타입이 이미 강제하므로
    /// **교차 필드 보정만** 담당합니다.
    ///
    /// 모든 소비 경로(앱 시작 기본 세션, 문서 세션 적용)가 이 함수를 통과합니다.
    /// 새 필드를 추가할 때는 그룹에 넣고, 교차 의존성이 있으면 여기에 보정을 추가하세요.
    pub fn sanitized(mut self) -> Self {
        // Custom이 꺼져 있으면 프리셋 단계가 강도·표면 값을 지배합니다.
        if !self.texture.custom {
            let (strength, surface) =
                freedf_core::paper::paper_texture_preset(self.texture.level.get());
            self.texture.strength = TextureStrength::new(strength);
            self.texture.surface = surface;
        }
        self.panels.favorite_colors.truncate(MAX_FAVORITE_COLORS);
        if self.panels.favorite_colors.is_empty() {
            self.panels.favorite_colors = default_favorite_colors();
        }
        // 저장된 순서에 새로 추가된 도구(예: 만년필)가 없으면 기본 위치에 보충.
        for t in ToolType::default_order() {
            if !self.panels.tool_order.contains(&t) {
                self.panels.tool_order.push(t);
            }
        }
        self
    }

    /// JSONB 값으로 변환 (DB 저장용).
    pub fn to_json_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_else(|_| serde_json::Value::Null)
    }

    /// JSONB 값에서 복원. 실패/누락 시 기본값.
    pub fn from_json_value(value: serde_json::Value) -> Self {
        serde_json::from_value(value).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 그룹화된 구조가 JSON 왕복에서 그대로 보존되어야 합니다.
    #[test]
    fn session_roundtrips_through_json() {
        let mut s = SessionState::default();
        s.page = 3;
        s.pen.ink.width = InkWidth::new(7.5);
        s.paper.style = PaperStyle::Grid;
        let back = SessionState::from_json_value(s.to_json_value());
        assert_eq!(back, s);
    }

    /// 과거 버전의 평면 필드(예: pen_width)는 무시되고 그룹 기본값이 사용됩니다.
    #[test]
    fn old_flat_fields_fall_back_to_group_defaults() {
        let v = serde_json::json!({ "pen_width": 99.0, "show_notes": false });
        let s = SessionState::from_json_value(v);
        assert_eq!(s.pen.ink.width, InkWidth::default());
        assert_eq!(s.panels.show_notes, true);
    }

    /// 뉴타입이 역직렬화 시점에 범위를 강제합니다 — 불가능한 값은 존재할 수 없습니다.
    #[test]
    fn bounded_types_clamp_on_deserialize() {
        let v = serde_json::json!({ "pen": { "ink": { "width": 99.0 } } });
        let s = SessionState::from_json_value(v);
        assert_eq!(s.pen.ink.width.get(), 12.0);

        let v = serde_json::json!({ "highlighter": { "width": -3.0 } });
        let s = SessionState::from_json_value(v);
        assert_eq!(s.highlighter.width.get(), 4.0);

        let v = serde_json::json!({ "view": { "zoom": 999.0 } });
        let s = SessionState::from_json_value(v);
        assert_eq!(s.view.zoom.get(), MAX_ZOOM);

        let v = serde_json::json!({ "edge_autoscroll": { "zone": 0.0, "speeds": [1.0, 1.0, 1.0, 1.0] } });
        let s = SessionState::from_json_value(v);
        assert_eq!(s.edge_autoscroll.zone.get(), 8.0);
        assert!(s.edge_autoscroll.speeds.iter().all(|v| v.get() == 20.0));
    }

    /// sanitized는 팔레트/도구 순서 같은 교차 필드만 보정합니다.
    #[test]
    fn sanitized_repairs_cross_field_state() {
        let mut s = SessionState::default();
        s.panels.favorite_colors.clear();
        s.panels.tool_order.clear();
        let s = s.sanitized();
        assert_eq!(s.panels.favorite_colors, default_favorite_colors());
        assert_eq!(s.panels.tool_order, ToolType::default_order());
    }

    /// Custom이 꺼져 있으면 프리셋 단계가 강도·표면을 지배합니다.
    #[test]
    fn sanitized_applies_preset_when_not_custom() {
        let mut s = SessionState::default();
        s.texture.custom = false;
        s.texture.level = TextureLevel::new(4);
        s.texture.strength = TextureStrength::new(0.9); // 무시되어야 함.
        let (preset_strength, preset_surface) =
            freedf_core::paper::paper_texture_preset(4);
        let s = s.sanitized();
        assert_eq!(s.texture.strength, TextureStrength::new(preset_strength));
        assert_eq!(s.texture.surface, preset_surface);
    }
}
