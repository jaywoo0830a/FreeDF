//! 앱 세션 상태 — 전역 기본값과 문서별 GUI 상태를 모두 `SessionState` 하나로 관리.
//!
//! FreeDF v2: 영속화는 PostgreSQL이 담당합니다.
//! - **전역 기본 세션** → `app_state` 테이블 (key = 'session')
//! - **문서별 세션** → `sessions` 테이블 (doc_id 기준)
//!
//! 이 모듈은 상태 구조체와 기본값만 정의하고, JSONB 변환은 `serde_json`으로 앱이 처리합니다.

use freedf_core::ink::InkGrain;
use freedf_core::model::ToolType;
use freedf_core::paper::{PaperSize, PaperStyle, PaperStyleSettings};
use freedf_core::pen::{BallPenProfile, ColorFamily, FountainProfile, InkSoak};
use freedf_core::transform::PageAlign;
use serde::{Deserialize, Serialize};

/// GUI 세션 상태.
///
/// - 전역 기본값: 시작 시 `<data>/session.json`에서 로드해 툴바 기본값
///   (펜 색, 용지, 도구 등)을 복원합니다.
/// - 문서별 상태: 노트 폴더(또는 PDF 옆 사이드카)의 `session.json`에 저장해
///   문서를 다시 열 때 마지막 페이지·줌/팬·도구·펜·용지를 복원합니다.
///
/// `#[serde(default)]`라 과거 버전 파일(필드 누락)도 안전하게 로드됩니다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionState {
    /// 마지막으로 열었던 페이지 인덱스
    pub page: usize,
    pub tool: ToolType,
    pub color_family: ColorFamily,
    pub pen_color: [u8; 4],
    pub pen_width: f32,
    /// 만년필 전용 색/두께 — 볼펜과 완전히 독립.
    pub fountain_color: [u8; 4],
    pub fountain_width: f32,
    pub hi_color: [u8; 4],
    pub hi_width: f32,
    pub eraser_radius: f32,
    pub pressure_enabled: bool,
    /// 디버그 HUD(실시간 입력값 오버레이) 표시 여부 (전역).
    #[serde(default)]
    pub debug_hud: bool,
    /// 왼손잡이 여부 — 펜 커서 배럴이 왼쪽(2·3사분면)만 가리킵니다.
    /// (오른손잡이는 오른쪽 1·4사분면만)
    #[serde(default)]
    pub left_handed: bool,
    /// 일반 펜(볼펜/젤펜) 물리 모델 프로파일
    #[serde(default)]
    pub pen_profile: BallPenProfile,
    /// 줌 배율 (화면 픽셀 / 페이지 포인트)
    pub zoom: f32,
    /// 페이지 가로 오프셋 (화면 좌표)
    pub pan_x: f32,
    pub pan_y: f32,
    pub page_align: PageAlign,
    pub paper_style: PaperStyle,
    pub paper_color: [u8; 4],
    pub paper_size: PaperSize,
    /// 캔버스(페이지 뒤 서라운드) 배경색 — 전역 기본값 + 탭별 상태.
    #[serde(default = "default_canvas_color")]
    pub canvas_color: [u8; 4],
    /// 스타일별(Ruled/Grid/Dotted) 줄/점 세부설정 — 각 스타일 독립.
    /// 한 스타일의 값 변경은 그 스타일을 쓰는 모든 페이지에 반영됩니다.
    #[serde(default)]
    pub paper_style_settings: PaperStyleSettings,
    pub show_notes: bool,
    pub show_outline: bool,
    /// Library / Outline 패널 폭 (탭별·문서별로 독립 유지)
    #[serde(default = "default_library_width")]
    pub library_width: f32,
    #[serde(default = "default_outline_width")]
    pub outline_width: f32,
    /// 캔버스 오른쪽 필기구/색상 팔레트 표시 여부 (전역 기본값)
    #[serde(default = "default_show_palette")]
    pub show_palette: bool,
    /// 자주 쓰는 펜 색상 팔레트 (전역 기본값, 최대 3개)
    #[serde(default = "default_favorite_colors")]
    pub favorite_colors: Vec<[u8; 4]>,
    /// 하이라이터가 문서 텍스트를 인식해 깔끔하게 칠하는 모드 (전역 기본값)
    /// 기본은 꺼짐 — 일반(자유선) 하이라이트가 기본 동작입니다.
    #[serde(default = "default_false")]
    pub text_highlight_snap: bool,
    /// 도구 선택기 순서 (드래그 앤 드롭 재정렬, 전역 기본값)
    #[serde(default = "default_tool_order")]
    pub tool_order: Vec<ToolType>,
    /// 줌 잠금 — 잠그면 휠/핀치/단축키/버튼 줌이 전부 무시됩니다 (실수 방지)
    #[serde(default = "default_false")]
    pub zoom_lock: bool,
    /// 엣지 자동 스크롤 — 커서가 캔버스 가장자리 근처에 닿으면 그 방향으로
    /// 자동 패닝 (전역). 기본은 꺼짐.
    #[serde(default = "default_false")]
    pub edge_autoscroll: bool,
    /// 엣지 반응 영역 폭 (화면 px)
    #[serde(default = "default_edge_zone")]
    pub edge_zone: f32,
    /// 엣지 자동 스크롤 방향별 최대 속도 [왼쪽, 오른쪽, 위, 아래] (화면 px/초)
    #[serde(default = "default_edge_speeds")]
    pub edge_speeds: [f32; 4],
    /// 커서가 창 위에서 움직이면 이 창을 포커스할지 (스플릿 뷰, 창마다 독립).
    /// 기본은 꺼짐.
    #[serde(default = "default_false")]
    pub window_focus_on_move: bool,
    /// Window Focus 지연(초) — 커서가 창 위에 이 시간 이상 머물면 포커스.
    /// 0이면 커서가 올라가는 즉시 포커스합니다.
    #[serde(default = "default_focus_dwell")]
    pub window_focus_dwell_sec: f32,
    /// 페이지(문서) 바깥으로 더 패닝할 수 있는 여유 (화면 px)
    #[serde(default = "default_edge_overscroll")]
    pub edge_overscroll: f32,
    /// 엣지 자동 스크롤 활성 가장자리의 "숨쉬는" 글로우 표시
    #[serde(default = "default_true")]
    pub edge_pulse: bool,
    /// 엣지 자동 스크롤 방향별 반응 지연(초) [왼쪽, 오른쪽, 위, 아래] —
    /// 0이면 가장자리에 닿는 즉시 스크롤이 시작됩니다.
    #[serde(default = "default_edge_delays")]
    pub edge_delays: [f32; 4],
    /// 펜 입력 스무딩(안정화) 강도 0..1 — 0이면 원본 그대로
    #[serde(default = "default_smoothing")]
    pub smoothing: f32,
    /// 스무딩 사용 여부 (기본 off — OTD 등 외부 드라이버 안정화와 충돌 방지)
    #[serde(default = "default_false")]
    pub smoothing_enabled: bool,
    /// 일반 펜(볼펜) 잉크 스밈 — 은은하게 진해짐
    #[serde(default)]
    pub pen_soak: InkSoak,
    /// 만년필 잉크 스밈 — 옅게 시작해 뚜렷하게 진해짐
    #[serde(default)]
    pub fountain_soak: InkSoak,
    /// 일반 펜(볼펜) 잉크 질감 — 입체적 불균일(흐름/위킹/뭉침/레일로드)
    #[serde(default)]
    pub pen_grain: InkGrain,
    /// 만년필 잉크 질감 — 볼펜과 완전히 독립
    #[serde(default)]
    pub fountain_grain: InkGrain,
    /// 만년필 물리 모델 프로파일
    #[serde(default)]
    pub fountain_profile: FountainProfile,
    /// 사용자 정의 용지 크기 [가로, 세로] (포인트). `PaperSize::Custom`일 때 사용.
    #[serde(default)]
    pub custom_paper_size: Option<[f32; 2]>,
    /// 마우스/트랙패드로도 잉크를 그릴지 (기본 off — 펜 전용 필기)
    #[serde(default = "default_false")]
    pub mouse_draws: bool,
    /// 단어 탭 시 사전 오버레이 표시 여부 (전역)
    #[serde(default = "default_false")]
    pub dictionary_enabled: bool,
}

/// 이전 버전 세션 파일(필드 없음)에서도 팔레트를 기본 표시.
fn default_show_palette() -> bool {
    true
}

/// 캔버스 서라운드 기본색 — Nord NORD0 (#2E3440, 다크 테마 기본).
fn default_canvas_color() -> [u8; 4] {
    crate::theme::nord::semantic::PAGE_SURROUND.to_array()
}

/// 자주 쓰는 색 팔레트(원형 휠/사이드 팔레트 공용) 최대 개수 — 기본 8색.
pub const MAX_FAVORITE_COLORS: usize = 8;

fn default_false() -> bool {
    false
}

/// 엣지 자동 스크롤 반응 영역 기본 폭 (화면 px).
fn default_edge_zone() -> f32 {
    72.0
}

/// 엣지 자동 스크롤 방향별 기본 최대 속도 [왼쪽, 오른쪽, 위, 아래] (화면 px/초).
fn default_edge_speeds() -> [f32; 4] {
    [480.0; 4]
}

/// 페이지 바깥 패닝 여유 기본값 (화면 px).
fn default_edge_overscroll() -> f32 {
    64.0
}

fn default_true() -> bool {
    true
}

/// Window Focus 기본 지연(초) — 0 = 커서가 올라가는 즉시 포커스.
fn default_focus_dwell() -> f32 {
    0.0
}

/// 엣지 자동 스크롤 방향별 반응 지연 기본값 (초, 0 = 즉시).
fn default_edge_delays() -> [f32; 4] {
    [0.0; 4]
}

fn default_smoothing() -> f32 {
    0.4
}

fn default_tool_order() -> Vec<ToolType> {
    ToolType::default_order()
}

fn default_library_width() -> f32 {
    260.0
}

fn default_outline_width() -> f32 {
    240.0
}

/// 이전 버전 세션 파일(필드 없음)에서도 기본 즐겨찾기 색상 3개 (GoodNotes 블랙/레드/블루).
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
            pen_color: [26, 26, 28, 255],
            pen_width: 2.0,
            fountain_color: [26, 26, 28, 255],
            fountain_width: 2.0,
            hi_color: [255, 230, 109, 120],
            hi_width: 16.0,
            eraser_radius: 16.0,
            pressure_enabled: true,
            debug_hud: false,
            left_handed: false,
            pen_profile: BallPenProfile::default(),
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            page_align: PageAlign::Center,
            paper_style: PaperStyle::Blank,
            paper_color: [255, 255, 255, 255],
            paper_size: PaperSize::A4,
            canvas_color: default_canvas_color(),
            paper_style_settings: PaperStyleSettings::default(),
            show_notes: true,
            show_outline: false,
            library_width: 260.0,
            outline_width: 240.0,
            show_palette: true,
            favorite_colors: vec![
                [26, 26, 28, 255],   // Black (#1A1A1C)
                [255, 71, 66, 255],  // Red (#FF4742)
                [72, 166, 235, 255], // Blue (#48A6EB)
            ],
            text_highlight_snap: false,
            tool_order: ToolType::default_order(),
            zoom_lock: false,
            edge_autoscroll: false,
            edge_zone: 72.0,
            edge_speeds: [480.0; 4],
            window_focus_on_move: false,
            window_focus_dwell_sec: 0.0,
            edge_overscroll: 64.0,
            edge_pulse: true,
            edge_delays: [0.0; 4],
            smoothing: 0.4,
            smoothing_enabled: false,
            pen_soak: InkSoak::ballpoint_default(),
            fountain_soak: InkSoak::fountain_default(),
            pen_grain: InkGrain::default(),
            fountain_grain: InkGrain::default(),
            fountain_profile: FountainProfile::default(),
            custom_paper_size: None,
            mouse_draws: false,
            dictionary_enabled: false,
        }
    }
}

impl SessionState {
    /// JSONB 값으로 변환 (DB 저장용).
    pub fn to_json_value(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or_else(|_| serde_json::Value::Null)
    }

    /// JSONB 값에서 복원. 실패/누락 시 기본값.
    pub fn from_json_value(value: serde_json::Value) -> Self {
        serde_json::from_value(value).unwrap_or_default()
    }
}
