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

/// 필기 도구(볼펜/만년필) 공용 묶음 — 두 도구가 같은 형태를 공유합니다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InkToolState {
    pub color: [u8; 4],
    pub width: f32,
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
            width: 2.0,
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

/// 하이라이터 묶음 — 색/두께만 (잉크 물리 설정 없음).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HighlighterState {
    pub color: [u8; 4],
    pub width: f32,
}

impl Default for HighlighterState {
    fn default() -> Self {
        Self {
            color: [255, 230, 109, 120],
            width: 16.0,
        }
    }
}

/// 화면 뷰 상태 (줌/팬/정렬) — 문서별 세션에서만 의미 있습니다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewState {
    /// 줌 배율 (화면 픽셀 / 페이지 포인트)
    pub zoom: f32,
    /// 페이지 가로 오프셋 (화면 좌표)
    pub pan_x: f32,
    pub pan_y: f32,
    pub page_align: PageAlign,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            page_align: PageAlign::Center,
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
    pub custom_size: Option<[f32; 2]>,
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
    pub strength: f32,
    /// 초보자 프리셋 단계 0..=4 (Lowest..Highest). Custom이 꺼져 있을 때
    /// 이 단계가 강도·표면 값을 지배합니다.
    pub level: u8,
    /// 상세 값(강도·표면·조명)을 직접 조절할지.
    pub custom: bool,
    /// 종이 표면 물리 모델 (요철·조명·반사율).
    pub surface: PaperSurfaceSettings,
}

impl Default for TextureState {
    fn default() -> Self {
        Self {
            enabled: true,
            strength: 0.25,
            level: 2,
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
    pub strength: f32,
    /// 사용 여부 (기본 off — OTD 등 외부 드라이버 안정화와 충돌 방지)
    pub enabled: bool,
}

impl Default for SmoothingState {
    fn default() -> Self {
        Self {
            strength: 0.4,
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
    pub zone: f32,
    /// 방향별 최대 속도 [왼쪽, 오른쪽, 위, 아래] (화면 px/초)
    pub speeds: [f32; 4],
    /// 페이지(문서) 바깥으로 더 패닝할 수 있는 여유 (화면 px)
    pub overscroll: f32,
    /// 활성 가장자리의 "숨쉬는" 글로우 표시.
    pub pulse: bool,
    /// 방향별 반응 지연(초) [왼쪽, 오른쪽, 위, 아래] — 0이면 즉시.
    pub delays: [f32; 4],
}

impl Default for EdgeAutoscrollState {
    fn default() -> Self {
        Self {
            enabled: false,
            pen_only: true,
            zone: 72.0,
            speeds: [480.0; 4],
            overscroll: 64.0,
            pulse: true,
            delays: [0.5; 4],
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
    pub dwell_sec: f32,
}

impl Default for WindowFocusState {
    fn default() -> Self {
        Self {
            on_move: false,
            dwell_sec: 0.5,
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
/// 1. 관련 필드는 위처럼 `#[serde(default)]` 그룹 구조체로 묶습니다.
/// 2. 기본값은 각 그룹의 `Default` 한 곳에서만 정의합니다.
/// 3. 값 범위(클램프)/정규화는 그룹이 아니라 [`SessionState::sanitized`]에서만.
/// 4. 앱 ↔ 세션 매핑은 `FreeDfApp::capture_session` / `apply_session` 한 쌍으로만.
/// 5. `#[serde(default)]`라 과거 버전 파일(필드 누락/구조 변경)도 안전하게 로드됩니다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionState {
    /// 마지막으로 열었던 페이지 인덱스
    pub page: usize,
    pub tool: ToolType,
    pub color_family: ColorFamily,
    /// 볼펜 설정 (색/두께/잉크) — 만년필과 완전히 독립.
    pub pen: InkToolState,
    /// 만년필 설정.
    pub fountain: InkToolState,
    /// 하이라이터 설정.
    pub highlighter: HighlighterState,
    pub eraser_radius: f32,
    pub pressure_enabled: bool,
    /// 디버그 HUD(실시간 입력값 오버레이) 표시 여부 (전역).
    pub debug_hud: bool,
    /// 왼손잡이 여부 — 펜 커서 배럴이 왼쪽(2·3사분면)만 가리킵니다.
    /// (오른손잡이는 오른쪽 1·4사분면만)
    pub left_handed: bool,
    /// 일반 펜(볼펜/젤펜) 물리 모델 프로파일
    pub pen_profile: BallPenProfile,
    /// 만년필 물리 모델 프로파일
    pub fountain_profile: FountainProfile,
    /// 화면 뷰 (줌/팬/정렬) — 문서별 상태.
    pub view: ViewState,
    /// 종이 외관.
    pub paper: PaperState,
    /// 종이 질감.
    pub texture: TextureState,
    /// 패널/팔레트 표시.
    pub panels: PanelsState,
    /// 줌 잠금 — 잠그면 휠/핀치/단축키/버튼 줌이 전부 무시됩니다 (실수 방지)
    pub zoom_lock: bool,
    /// 펜 입력 스무딩.
    pub smoothing: SmoothingState,
    /// 엣지 자동 스크롤 (전역).
    pub edge_autoscroll: EdgeAutoscrollState,
    /// 창 포커스 추적 (전역).
    pub window_focus: WindowFocusState,
    /// 마우스/트랙패드로도 잉크를 그릴지 (기본 off — 펜 전용 필기)
    pub mouse_draws: bool,
    /// 단어 탭 시 사전 오버레이 표시 여부 (전역)
    pub dictionary_enabled: bool,
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
            tool: ToolType::Pen,
            color_family: ColorFamily::Black,
            pen: InkToolState::default(),
            fountain: InkToolState::fountain_default(),
            highlighter: HighlighterState::default(),
            eraser_radius: 16.0,
            pressure_enabled: true,
            debug_hud: false,
            left_handed: false,
            pen_profile: BallPenProfile::default(),
            fountain_profile: FountainProfile::default(),
            view: ViewState::default(),
            paper: PaperState::default(),
            texture: TextureState::default(),
            panels: PanelsState::default(),
            zoom_lock: false,
            smoothing: SmoothingState::default(),
            edge_autoscroll: EdgeAutoscrollState::default(),
            window_focus: WindowFocusState::default(),
            mouse_draws: false,
            dictionary_enabled: false,
        }
    }
}

impl SessionState {
    /// 로드된 상태 정규화 — 과거 버전/외부 수정으로 인한 불량 값을 안전 범위로.
    ///
    /// **규칙: 값 범위 검사(클램프)와 보정은 여기 한 곳에서만 합니다.**
    /// 모든 소비 경로(앱 시작 기본 세션, 문서 세션 적용)가 이 함수를 통과합니다.
    /// 새 설정 필드를 추가할 때는 그룹 구조체에 넣고 여기에 클램프를 함께 추가하세요.
    pub fn sanitized(mut self) -> Self {
        self.pen.width = self.pen.width.clamp(0.5, 12.0);
        self.fountain.width = self.fountain.width.clamp(0.5, 12.0);
        self.highlighter.width = self.highlighter.width.clamp(4.0, 40.0);
        self.eraser_radius = self.eraser_radius.clamp(4.0, 60.0);
        self.view.zoom = self.view.zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        self.smoothing.strength = self.smoothing.strength.clamp(0.0, 1.0);
        self.texture.strength = self.texture.strength.clamp(0.0, 1.0);
        self.texture.level = self.texture.level.min(4);
        // Custom이 꺼져 있으면 프리셋 단계가 강도·표면 값을 지배합니다.
        if !self.texture.custom {
            let (strength, surface) =
                freedf_core::paper::paper_texture_preset(self.texture.level);
            self.texture.strength = strength;
            self.texture.surface = surface;
        }
        self.edge_autoscroll.zone = self.edge_autoscroll.zone.clamp(8.0, 300.0);
        for v in &mut self.edge_autoscroll.speeds {
            *v = v.clamp(20.0, 4000.0);
        }
        self.edge_autoscroll.overscroll = self.edge_autoscroll.overscroll.clamp(0.0, 2000.0);
        for v in &mut self.edge_autoscroll.delays {
            *v = v.clamp(0.0, 3.0);
        }
        self.window_focus.dwell_sec = self.window_focus.dwell_sec.clamp(0.0, 5.0);
        if let Some(c) = &mut self.paper.custom_size {
            c[0] = c[0].clamp(100.0, 2400.0);
            c[1] = c[1].clamp(100.0, 2400.0);
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
        s.pen.width = 7.5;
        s.paper.style = PaperStyle::Grid;
        let back = SessionState::from_json_value(s.to_json_value());
        assert_eq!(back, s);
    }

    /// 과거 버전의 평면 필드(예: pen_width)는 무시되고 그룹 기본값이 사용됩니다.
    #[test]
    fn old_flat_fields_fall_back_to_group_defaults() {
        let v = serde_json::json!({ "pen_width": 99.0, "show_notes": false });
        let s = SessionState::from_json_value(v);
        assert_eq!(s.pen.width, InkToolState::default().width);
        assert_eq!(s.panels.show_notes, true);
    }

    /// sanitized가 불량 값을 안전 범위로 클램프/보정합니다.
    #[test]
    fn sanitized_clamps_and_repairs() {
        let mut s = SessionState::default();
        s.pen.width = 99.0;
        s.highlighter.width = 0.0;
        s.texture.custom = true; // 커스텀 모드에서만 강도가 직접 사용됩니다.
        s.texture.strength = 5.0;
        s.panels.favorite_colors.clear();
        s.panels.tool_order.clear();
        let s = s.sanitized();
        assert_eq!(s.pen.width, 12.0);
        assert_eq!(s.highlighter.width, 4.0);
        assert_eq!(s.texture.strength, 1.0);
        assert_eq!(s.panels.favorite_colors, default_favorite_colors());
        assert_eq!(s.panels.tool_order, ToolType::default_order());
    }

    /// Custom이 꺼져 있으면 프리셋 단계가 강도·표면을 지배합니다.
    #[test]
    fn sanitized_applies_preset_when_not_custom() {
        let mut s = SessionState::default();
        s.texture.custom = false;
        s.texture.level = 4;
        s.texture.strength = 0.9; // 무시되어야 함.
        let (preset_strength, preset_surface) =
            freedf_core::paper::paper_texture_preset(4);
        let s = s.sanitized();
        assert_eq!(s.texture.strength, preset_strength);
        assert_eq!(s.texture.surface, preset_surface);
    }
}
