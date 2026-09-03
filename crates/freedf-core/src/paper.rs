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

/// 기본 줄/격자/점 색 (약간 투명한 회색).
pub const PAPER_LINE: [u8; 4] = [120, 120, 140, 110];
/// 기본 줄/격자/점 두께 (포인트).
pub const PAPER_LINE_WIDTH_PT: f32 = 1.2;

fn default_line_color() -> [u8; 4] {
    PAPER_LINE
}

fn default_line_width() -> f32 {
    PAPER_LINE_WIDTH_PT
}

fn default_spacing() -> f32 {
    GRID_SPACING_PTS
}

/// 유효한 용지 라인 두께(포인트). 0.25~8로 제한.
pub fn clamp_line_width(w: f32) -> f32 {
    if !w.is_finite() || w <= 0.0 {
        PAPER_LINE_WIDTH_PT
    } else {
        w.clamp(0.25, 8.0)
    }
}

/// 표준 종이 크기.
///
/// 새 노트/페이지를 만들 때 물리적 PDF 페이지 크기로 사용합니다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaperSize {
    A3,
    A4,
    A5,
    Letter,
    Legal,
    /// 사용자 정의 크기 — 실제 치수는 앱 상태(custom_paper_size)가 들고 있습니다.
    Custom,
}

impl PaperSize {
    pub fn label(self) -> &'static str {
        match self {
            PaperSize::A3 => "A3",
            PaperSize::A4 => "A4",
            PaperSize::A5 => "A5",
            PaperSize::Letter => "Letter",
            PaperSize::Legal => "Legal",
            PaperSize::Custom => "Custom",
        }
    }

    pub fn all() -> [PaperSize; 6] {
        [
            PaperSize::A3,
            PaperSize::A4,
            PaperSize::A5,
            PaperSize::Letter,
            PaperSize::Legal,
            PaperSize::Custom,
        ]
    }

    /// 페이지 크기(포인트). 세로 방향 기준 [width, height].
    ///
    /// `Custom`은 자체 치수가 없어 A4를 반환합니다 — 호출부에서
    /// 사용자 정의 치수를 대신 사용해야 합니다.
    pub fn size_pts(self) -> [f32; 2] {
        match self {
            PaperSize::A3 => [841.89, 1190.55],
            PaperSize::A4 => [595.28, 841.89],
            PaperSize::A5 => [419.53, 595.28],
            PaperSize::Letter => [612.0, 792.0],
            PaperSize::Legal => [612.0, 1008.0],
            PaperSize::Custom => [595.28, 841.89],
        }
    }

    /// 주어진 페이지 크기(포인트)에 가장 가까운 표준 크기.
    /// 문서를 열 때 실제 페이지 크기를 표시/기록하는 데 사용합니다.
    pub fn matching(w: f32, h: f32) -> PaperSize {
        Self::all()
            .into_iter()
            .min_by(|a, b| {
                let sa = a.size_pts();
                let sb = b.size_pts();
                let da = (sa[0] - w).abs() + (sa[1] - h).abs();
                let db = (sb[0] - w).abs() + (sb[1] - h).abs();
                da.total_cmp(&db)
            })
            .unwrap_or(PaperSize::A4)
    }
}

impl Default for PaperSize {
    fn default() -> Self {
        PaperSize::A4
    }
}

/// 한 페이지의 용지 설정 — **스타일 + 배경 색만** 페이지별로 저장합니다.
///
/// 줄/격자/점의 간격·색·두께는 **스타일별 프리셋**([`PaperStyleSettings`])을
/// 렌더 시점에 참조하므로 페이지마다 중복 저장하지 않습니다. 즉,
/// 프리셋을 바꾸면 그 스타일을 쓰는 **모든 페이지**가 즉시 함께 바뀝니다.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PagePaper {
    pub style: PaperStyle,
    pub color: [u8; 4],
}

impl Default for PagePaper {
    fn default() -> Self {
        Self {
            style: PaperStyle::Blank,
            color: PAPER_WHITE,
        }
    }
}

/// 줄/격자/점의 세부설정 (간격/색/두께) — 스타일 하나의 정의.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LineStyle {
    /// 간격 (포인트). 0 이하면 `GRID_SPACING_PTS`로 대체.
    #[serde(default = "default_spacing")]
    pub spacing: f32,
    /// 색 (RGBA).
    #[serde(default = "default_line_color")]
    pub color: [u8; 4],
    /// 두께 (포인트).
    #[serde(default = "default_line_width")]
    pub width: f32,
}

impl Default for LineStyle {
    fn default() -> Self {
        Self {
            spacing: GRID_SPACING_PTS,
            color: PAPER_LINE,
            width: PAPER_LINE_WIDTH_PT,
        }
    }
}

/// **스타일별 독립 세부설정** — Ruled / Grid / Dotted 각각 자기만의
/// 간격·색·두께를 가집니다. Blank는 줄이 없어 항목이 없습니다.
///
/// 앱의 Paper 설정 창에서 "현재 선택된 스타일"의 값을 편집하며,
/// 한 스타일의 값 변경은 그 스타일을 쓰는 모든 페이지에 반영됩니다.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PaperStyleSettings {
    pub ruled: LineStyle,
    pub grid: LineStyle,
    pub dotted: LineStyle,
}

impl Default for PaperStyleSettings {
    fn default() -> Self {
        Self {
            ruled: LineStyle::default(),
            grid: LineStyle::default(),
            dotted: LineStyle::default(),
        }
    }
}

impl PaperStyleSettings {
    /// 스타일에 해당하는 세부설정 (Blank는 줄이 없어 `None`).
    pub fn of(&self, style: PaperStyle) -> Option<LineStyle> {
        match style {
            PaperStyle::Blank => None,
            PaperStyle::Ruled => Some(self.ruled),
            PaperStyle::Grid => Some(self.grid),
            PaperStyle::Dotted => Some(self.dotted),
        }
    }

    /// 스타일의 세부설정을 통째로 교체합니다 (Blank는 무시).
    pub fn set(&mut self, style: PaperStyle, value: LineStyle) {
        match style {
            PaperStyle::Ruled => self.ruled = value,
            PaperStyle::Grid => self.grid = value,
            PaperStyle::Dotted => self.dotted = value,
            PaperStyle::Blank => {}
        }
    }
}

/// 유효한 용지 간격(포인트). 너무 작으면 12, 크면 120으로 제한.
pub fn clamp_spacing(spacing: f32) -> f32 {
    if !spacing.is_finite() || spacing <= 0.0 {
        GRID_SPACING_PTS
    } else {
        spacing.clamp(12.0, 120.0)
    }
}

/// 용지 스타일에 따라 그릴 선분 [x0, y0, x1, y1] (페이지 포인트)을 반환합니다.
/// Ruled는 가로줄, Grid는 가로+세로, Blank/Dotted는 빈 벡터.
pub fn paper_lines(w: f32, h: f32, style: PaperStyle, spacing: f32) -> Vec<[f32; 4]> {
    let gap = clamp_spacing(spacing);
    let mut out = Vec::new();
    match style {
        PaperStyle::Blank | PaperStyle::Dotted => {}
        PaperStyle::Ruled => {
            let mut y = gap;
            while y < h {
                out.push([0.0, y, w, y]);
                y += gap;
            }
        }
        PaperStyle::Grid => {
            let mut y = gap;
            while y < h {
                out.push([0.0, y, w, y]);
                y += gap;
            }
            let mut x = gap;
            while x < w {
                out.push([x, 0.0, x, h]);
                x += gap;
            }
        }
    }
    out
}

/// Dotted 스타일의 점 위치 [x, y] (페이지 포인트).
pub fn paper_dots(w: f32, h: f32, style: PaperStyle, spacing: f32) -> Vec<[f32; 2]> {
    if style != PaperStyle::Dotted {
        return Vec::new();
    }
    let gap = clamp_spacing(spacing);
    let mut out = Vec::new();
    let half = gap / 2.0;
    let mut y = half;
    while y < h {
        let mut x = half;
        while x < w {
            out.push([x, y]);
            x += gap;
        }
        y += gap;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_settings_are_independent_per_style() {
        // Ruled/Grid/Dotted 각각 독립 — 한 스타일을 바꿔도 다른 스타일은 그대로.
        let mut s = PaperStyleSettings::default();
        let custom = LineStyle {
            spacing: 60.0,
            color: [200, 10, 10, 255],
            width: 3.0,
        };
        s.set(PaperStyle::Grid, custom);
        assert_eq!(s.of(PaperStyle::Grid), Some(custom));
        assert_eq!(s.of(PaperStyle::Ruled), Some(LineStyle::default()));
        assert_eq!(s.of(PaperStyle::Dotted), Some(LineStyle::default()));
        // Blank는 줄이 없음.
        assert_eq!(s.of(PaperStyle::Blank), None);
        // 직렬화 왕복.
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<PaperStyleSettings>(&json).unwrap(), s);
        // 빈 객체 → 기본값 (이전 세션 호환).
        let d: PaperStyleSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(d, PaperStyleSettings::default());
    }

    #[test]
    fn blank_has_no_lines_or_dots() {
        assert!(paper_lines(595.0, 842.0, PaperStyle::Blank, 24.0).is_empty());
        assert!(paper_dots(595.0, 842.0, PaperStyle::Blank, 24.0).is_empty());
    }

    #[test]
    fn ruled_only_horizontal() {
        let lines = paper_lines(595.0, 100.0, PaperStyle::Ruled, 24.0);
        assert!(!lines.is_empty());
        for l in &lines {
            // 가로줄: y0 == y1, 페이지 폭을 가로지름
            assert!((l[1] - l[3]).abs() < 1e-3);
            assert!((l[0]).abs() < 1e-3 && (l[2] - 595.0).abs() < 1e-3);
        }
    }

    #[test]
    fn grid_has_horizontal_and_vertical() {
        let lines = paper_lines(595.0, 100.0, PaperStyle::Grid, 24.0);
        assert!(!lines.is_empty());
        let horiz = lines.iter().filter(|l| (l[1] - l[3]).abs() < 1e-3).count();
        let vert = lines.iter().filter(|l| (l[0] - l[2]).abs() < 1e-3).count();
        assert!(horiz > 0);
        assert!(vert > 0);
    }

    #[test]
    fn dotted_has_dots_not_lines() {
        assert!(paper_lines(100.0, 100.0, PaperStyle::Dotted, 24.0).is_empty());
        let dots = paper_dots(100.0, 100.0, PaperStyle::Dotted, 24.0);
        assert!(!dots.is_empty());
        for d in &dots {
            assert!(d[0] > 0.0 && d[1] > 0.0);
        }
    }

    #[test]
    fn paper_sizes_are_portrait_and_positive() {
        for size in PaperSize::all() {
            let [w, h] = size.size_pts();
            assert!(w > 0.0 && h > w, "{size:?} should be portrait");
        }
        assert!(PaperSize::A5.size_pts()[0] < PaperSize::A4.size_pts()[0]);
        assert!(PaperSize::A4.size_pts()[0] < PaperSize::A3.size_pts()[0]);
    }

    #[test]
    fn matching_finds_nearest_size() {
        assert_eq!(PaperSize::matching(595.0, 842.0), PaperSize::A4);
        assert_eq!(PaperSize::matching(419.0, 595.0), PaperSize::A5);
        assert_eq!(PaperSize::matching(612.0, 792.0), PaperSize::Letter);
        assert_eq!(PaperSize::matching(612.0, 1008.0), PaperSize::Legal);
    }

    #[test]
    fn page_paper_default_is_blank_white() {
        let p = PagePaper::default();
        assert_eq!(p.style, PaperStyle::Blank);
        assert_eq!(p.color, PAPER_WHITE);
    }
}
