//! 노트/페이지의 용지 스타일(그리드·줄·점선)과 배경 색.
//!
//! 순수 데이터/수학만 담고 있어 GUI 없이 단위 테스트로 검증합니다.
//! 좌표는 페이지 좌표계(포인트)입니다.

use serde::{Deserialize, Serialize};

/// 그리드/줄 스타일.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaperStyle {
    /// 흰 종이
    Blank,
    /// 가로줄 노트
    Ruled,
    /// 모눈종이
    Grid,
    /// 점선 모눈
    Dotted,
}

impl PaperStyle {
    pub fn label(self) -> &'static str {
        match self {
            PaperStyle::Blank => "Blank",
            PaperStyle::Ruled => "Ruled",
            PaperStyle::Grid => "Grid",
            PaperStyle::Dotted => "Dotted",
        }
    }

    pub fn all() -> [PaperStyle; 4] {
        [PaperStyle::Blank, PaperStyle::Ruled, PaperStyle::Grid, PaperStyle::Dotted]
    }
}

/// 용지 배경색 팔레트 (RGBA).
pub const PAPER_COLORS: &[[u8; 4]] = &[
    [255, 255, 255, 255], // White
    [253, 247, 231, 255], // Cream
    [244, 246, 251, 255], // Ice blue
    [240, 248, 241, 255], // Mint
    [244, 244, 244, 255], // Light gray
];

/// 기본 용지 색 (White).
pub const PAPER_WHITE: [u8; 4] = [255, 255, 255, 255];

/// 그리드 간격 (포인트, 약 6mm).
pub const GRID_SPACING_PTS: f32 = 24.0;

/// 용지 스타일에 따라 그릴 선분 [x0, y0, x1, y1] (페이지 포인트)을 반환합니다.
/// Ruled는 가로줄, Grid는 가로+세로, Blank/Dotted는 빈 벡터.
pub fn paper_lines(w: f32, h: f32, style: PaperStyle) -> Vec<[f32; 4]> {
    let mut out = Vec::new();
    match style {
        PaperStyle::Blank | PaperStyle::Dotted => {}
        PaperStyle::Ruled => {
            let mut y = GRID_SPACING_PTS;
            while y < h {
                out.push([0.0, y, w, y]);
                y += GRID_SPACING_PTS;
            }
        }
        PaperStyle::Grid => {
            let mut y = GRID_SPACING_PTS;
            while y < h {
                out.push([0.0, y, w, y]);
                y += GRID_SPACING_PTS;
            }
            let mut x = GRID_SPACING_PTS;
            while x < w {
                out.push([x, 0.0, x, h]);
                x += GRID_SPACING_PTS;
            }
        }
    }
    out
}

/// Dotted 스타일의 점 위치 [x, y] (페이지 포인트).
pub fn paper_dots(w: f32, h: f32, style: PaperStyle) -> Vec<[f32; 2]> {
    if style != PaperStyle::Dotted {
        return Vec::new();
    }
    let mut out = Vec::new();
    let half = GRID_SPACING_PTS / 2.0;
    let mut y = half;
    while y < h {
        let mut x = half;
        while x < w {
            out.push([x, y]);
            x += GRID_SPACING_PTS;
        }
        y += GRID_SPACING_PTS;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_has_no_lines_or_dots() {
        assert!(paper_lines(595.0, 842.0, PaperStyle::Blank).is_empty());
        assert!(paper_dots(595.0, 842.0, PaperStyle::Blank).is_empty());
    }

    #[test]
    fn ruled_only_horizontal() {
        let lines = paper_lines(595.0, 100.0, PaperStyle::Ruled);
        assert!(!lines.is_empty());
        for l in &lines {
            // 가로줄: y0 == y1, 페이지 폭을 가로지름
            assert!((l[1] - l[3]).abs() < 1e-3);
            assert!((l[0]).abs() < 1e-3 && (l[2] - 595.0).abs() < 1e-3);
        }
    }

    #[test]
    fn grid_has_horizontal_and_vertical() {
        let lines = paper_lines(595.0, 100.0, PaperStyle::Grid);
        assert!(!lines.is_empty());
        let horiz = lines.iter().filter(|l| (l[1] - l[3]).abs() < 1e-3).count();
        let vert = lines.iter().filter(|l| (l[0] - l[2]).abs() < 1e-3).count();
        assert!(horiz > 0);
        assert!(vert > 0);
    }

    #[test]
    fn dotted_has_dots_not_lines() {
        assert!(paper_lines(100.0, 100.0, PaperStyle::Dotted).is_empty());
        let dots = paper_dots(100.0, 100.0, PaperStyle::Dotted);
        assert!(!dots.is_empty());
        for d in &dots {
            assert!(d[0] > 0.0 && d[1] > 0.0);
        }
    }
}
