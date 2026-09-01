//! 앱 설정 — 마지막으로 사용한 펜 색과 용지 스타일을
//! 앱 데이터 폴더의 `settings.json`에 저장하고 시작 시 복원합니다.

use freedf_core::paper::{PaperSize, PaperStyle, PAPER_WHITE};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// 영속화할 설정.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    /// 마지막 사용 펜 색 (RGBA)
    pub pen_color: [u8; 4],
    /// 용지 스타일 (그리드/줄/점선) — 새 페이지 기본값
    pub paper_style: PaperStyle,
    /// 용지 배경 색 (RGBA) — 새 페이지 기본값
    pub paper_color: [u8; 4],
    /// 새 페이지/노트의 종이 크기
    pub paper_size: PaperSize,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            pen_color: [20, 20, 20, 255],
            paper_style: PaperStyle::Blank,
            paper_color: PAPER_WHITE,
            paper_size: PaperSize::A4,
        }
    }
}

impl AppSettings {
    /// 파일에서 로드합니다. 없거나 깨졌으면 기본값.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// JSON으로 저장합니다 (부모 폴더 자동 생성).
    pub fn save(&self, path: &Path) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(path, json);
        }
    }
}
