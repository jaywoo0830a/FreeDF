//! 앱 세션 상태 — 전역 기본값과 문서별 GUI 상태를 모두 `SessionState` 하나로 관리.
//!
//! FreeDF v2: 영속화는 PostgreSQL이 담당합니다.
//! - **전역 기본 세션** → `app_state` 테이블 (key = 'session')
//! - **문서별 세션** → `sessions` 테이블 (doc_id 기준)
//!
//! 이 모듈은 상태 구조체와 기본값만 정의하고, JSONB 변환은 `serde_json`으로 앱이 처리합니다.

use freedf_core::model::ToolType;
use freedf_core::paper::{PaperSize, PaperStyle, PAPER_LINE, PAPER_LINE_WIDTH_PT};
use freedf_core::pen::{BallPenProfile, ColorFamily, FountainProfile, InkBleed};
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
    /// 프로파일(펜/만년필 모델) 기본값 마이그레이션 버전 — 이전 버전이면
    /// 저장된 프로파일 대신 새 기본값(더 강한 필압/속도/틸트 감도)을 씁니다.
    #[serde(default)]
    pub profile_version: u32,
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
    /// 줄/격자/점 간격 (포인트) 기본값
    #[serde(default = "default_paper_spacing")]
    pub paper_spacing: f32,
    /// 줄/격자/점 색 (RGBA) 기본값
    #[serde(default = "default_paper_line_color")]
    pub paper_line_color: [u8; 4],
    /// 줄/격자/점 두께 (포인트) 기본값
    #[serde(default = "default_paper_line_width")]
    pub paper_line_width: f32,
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
    /// 펜 입력 스무딩(안정화) 강도 0..1 — 0이면 원본 그대로
    #[serde(default = "default_smoothing")]
    pub smoothing: f32,
    /// 스무딩 사용 여부 (기본 off — OTD 등 외부 드라이버 안정화와 충돌 방지)
    #[serde(default = "default_false")]
    pub smoothing_enabled: bool,
    /// 잉크 번짐(블리드) 설정 (기본 off, 구간별 속도 커스텀 가능)
    #[serde(default)]
    pub ink_bleed: InkBleed,
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

/// 자주 쓰는 색 팔레트 최대 개수 (기본 3색 제한).
pub const MAX_FAVORITE_COLORS: usize = 3;

fn default_false() -> bool {
    false
}

fn default_smoothing() -> f32 {
    0.4
}

fn default_tool_order() -> Vec<ToolType> {
    ToolType::default_order()
}

fn default_paper_spacing() -> f32 {
    24.0
}

fn default_paper_line_color() -> [u8; 4] {
    PAPER_LINE
}

fn default_paper_line_width() -> f32 {
    PAPER_LINE_WIDTH_PT
}

fn default_library_width() -> f32 {
    260.0
}

fn default_outline_width() -> f32 {
    240.0
}

/// 이전 버전 세션 파일(필드 없음)에서도 기본 즐겨찾기 색상 3개 (검정/빨강/파랑).
fn default_favorite_colors() -> Vec<[u8; 4]> {
    vec![
        [20, 20, 20, 255],   // Black
        [200, 40, 40, 255],  // Red
        [29, 78, 216, 255],  // Blue
    ]
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            page: 0,
            tool: ToolType::Pen,
            color_family: ColorFamily::Black,
            pen_color: [20, 20, 20, 255],
            pen_width: 2.5,
            fountain_color: [20, 20, 20, 255],
            fountain_width: 2.0,
            hi_color: [255, 235, 59, 90],
            hi_width: 16.0,
            eraser_radius: 16.0,
            pressure_enabled: true,
            profile_version: 1,
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
            paper_spacing: 24.0,
            paper_line_color: PAPER_LINE,
            paper_line_width: PAPER_LINE_WIDTH_PT,
            show_notes: true,
            show_outline: false,
            library_width: 260.0,
            outline_width: 240.0,
            show_palette: true,
            favorite_colors: vec![
                [20, 20, 20, 255],   // Black
                [200, 40, 40, 255],  // Red
                [29, 78, 216, 255],  // Blue
            ],
            text_highlight_snap: false,
            tool_order: ToolType::default_order(),
            zoom_lock: false,
            smoothing: 0.4,
            smoothing_enabled: false,
            ink_bleed: InkBleed::default(),
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
