//! 앱 세션 영속화 — 전역 기본값과 문서별 GUI 상태를 모두 `SessionState` 하나로 관리.
//!
//! - **전역 기본 세션**(마지막 펜 색, 용지, 도구 등) → `<data>/session.json`
//! - **문서별 세션**(마지막 페이지, 줌/팬 등) → 노트 폴더(또는 PDF 옆) `session.json`

use freedf_core::model::ToolType;
use freedf_core::paper::{PaperSize, PaperStyle};
use freedf_core::pen::{ColorFamily, PressureCurve};
use freedf_core::transform::PageAlign;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// GUI 세션 상태.
///
/// - 전역 기본값: 시작 시 `<data>/session.json`에서 로드해 툴바 기본값
///   (펜 색, 용지, 도구 등)을 복원합니다.
/// - 문서별 상태: 노트 폴더(또는 PDF 옆 사이드카)의 `session.json`에 저장해
///   문서를 다시 열 때 마지막 페이지·줌/팬·도구·펜·용지를 복원합니다.
///
/// `#[serde(default)]`라 과거 버전 파일(필드 누락)도 안전하게 로드됩니다.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionState {
    /// 마지막으로 열었던 페이지 인덱스
    pub page: usize,
    pub tool: ToolType,
    pub color_family: ColorFamily,
    pub pen_color: [u8; 4],
    pub pen_width: f32,
    pub hi_color: [u8; 4],
    pub hi_width: f32,
    pub eraser_radius: f32,
    pub pressure_enabled: bool,
    pub pressure_curve: PressureCurve,
    /// 줌 배율 (화면 픽셀 / 페이지 포인트)
    pub zoom: f32,
    /// 페이지 가로 오프셋 (화면 좌표)
    pub pan_x: f32,
    pub pan_y: f32,
    pub page_align: PageAlign,
    pub paper_style: PaperStyle,
    pub paper_color: [u8; 4],
    pub paper_size: PaperSize,
    pub show_notes: bool,
    pub show_outline: bool,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            page: 0,
            tool: ToolType::Pen,
            color_family: ColorFamily::Black,
            pen_color: [20, 20, 20, 255],
            pen_width: 2.5,
            hi_color: [255, 235, 59, 90],
            hi_width: 16.0,
            eraser_radius: 16.0,
            pressure_enabled: true,
            pressure_curve: PressureCurve::default(),
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            page_align: PageAlign::Center,
            paper_style: PaperStyle::Blank,
            paper_color: [255, 255, 255, 255],
            paper_size: PaperSize::A4,
            show_notes: true,
            show_outline: false,
        }
    }
}

/// 파일에서 JSON으로 로드. 없거나 깨졌으면 기본값.
pub fn load_json<T: for<'de> Deserialize<'de> + Default>(path: &Path) -> T {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// JSON으로 저장 (부모 폴더 자동 생성).
pub fn save_json<T: Serialize>(value: &T, path: &Path) {
    if let Ok(json) = serde_json::to_string_pretty(value) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, json);
    }
}

impl SessionState {
    /// 파일에서 로드합니다. 없거나 깨졌으면 기본값.
    pub fn load(path: &Path) -> Self {
        load_json(path)
    }

    /// JSON으로 저장합니다 (부모 폴더 자동 생성).
    pub fn save(&self, path: &Path) {
        save_json(self, path);
    }
}
