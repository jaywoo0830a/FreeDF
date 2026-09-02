//! 페이지 텍스트 좌표 기하학 — pdfium이 추출한 글자 좌표를 다루는 순수 함수 모음.
//!
//! 사전(word_at)·하이라이트 스냅(char_line_highlights)·검색 하이라이트
//! (text_line_highlights)가 **하나의 모듈**에서 공용 좌표 규약을 공유합니다:
//!
//! - pdfium 좌표는 **콘텐츠 공간**: 좌하단 원점(y 위), `/Rotate` 미적용.
//! - 앱 좌표는 **표시 공간**: 좌상단 원점(y 아래), 회전 반영.
//!
//! GUI/pdfium 의존이 없어 단위 테스트로 전부 검증합니다.
//! 좌표 변환은 2026-09 pypdfium2 픽셀 렌더로 4방향 모두 실측 검증했습니다.

use serde::{Deserialize, Serialize};

use crate::search::TextRun;

/// 페이지 회전 상태 (PDF `/Rotate`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PageRotation {
    #[default]
    None,
    Degrees90,
    Degrees180,
    Degrees270,
}

impl PageRotation {
    pub fn from_degrees(deg: i32) -> Self {
        match deg {
            90 => PageRotation::Degrees90,
            180 => PageRotation::Degrees180,
            270 => PageRotation::Degrees270,
            _ => PageRotation::None,
        }
    }
}

/// 콘텐츠 공간 사각형 `[left, bottom, right, top]`(y 위, `/Rotate` 미적용)을
/// **표시 공간** `[x0, y0, x1, y1]`(y 아래, 회전 반영)으로 변환합니다.
///
/// pdfium(FPDF_RenderPageBitmap)은 회전을 "비회전 래스터를 회전"으로
/// 적용하므로 (pypdfium2 픽셀 실측):
/// - None:   `(x, y) → (x, H−y)`
/// - 90°:    `(x, y) → (y, x)`      — 비트맵 H×W
/// - 180°:   `(x, y) → (W−x, y)`    — 래스터의 y뒤집기와 180° 회전이 상쇄
/// - 270°:   `(x, y) → (H−y, W−x)`  — 비트맵 H×W
///
/// (`W, H`는 미디어박스 크기)
pub fn content_rect_to_display(
    r: [f32; 4],
    w: f32,
    h: f32,
    rot: PageRotation,
) -> [f32; 4] {
    let (x0, y0, x1, y1) = (r[0], r[1], r[2], r[3]);
    match rot {
        PageRotation::None => [x0, h - y1, x1, h - y0],
        PageRotation::Degrees90 => [y0, x0, y1, x1],
        PageRotation::Degrees180 => [w - x1, y0, w - x0, y1],
        PageRotation::Degrees270 => [h - y1, w - x1, h - y0, w - x0],
    }
}

/// 페이지의 글자 하나 (사전/하이라이트 입력).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextChar {
    pub text: String,
    /// 표시 공간 사각형 [x0, y0, x1, y1] (tight ink bounds).
    pub rect: [f32; 4],
}

impl TextChar {
    pub fn new(text: impl Into<String>, rect: [f32; 4]) -> Self {
        Self {
            text: text.into(),
            rect,
        }
    }
}

/// 탭한 위치의 **단어**를 찾습니다 (사전 오버레이용).
///
/// 기존 구현은 글자(tight ink bounds)에 직접 닿은 경우만 인식해서,
/// 글자 위아래 여백(어센더 위/디센더 아래)이나 단어 사이 공백을 탭하면
/// 인식 실패 또는 **다른 줄의 단어**를 골랐습니다. 이 구현은:
///
/// 1. 글자를 **줄**로 묶고 줄마다 `[x범위, 중심±줄높이/2]`의 **셀 박스**를 만듭니다.
/// 2. 포인트가 (margin 여유로) 포함된 줄 중 세로 중심이 가장 가까운 줄을 고릅니다.
/// 3. 그 줄에서 x 중심이 가장 가까운 글자에 닿아, 글자 간격 ≤ `줄높이×0.6`인
///    이웃으로 좌우 확장해 단어를 조립합니다 (공백은 갭으로 경계 판정).
///
/// 빈 영역(어느 줄 박스에도 안 닿음)은 `None`입니다.
pub fn word_at(
    chars: &[TextChar],
    point: [f32; 2],
    margin: f32,
) -> Option<(String, [f32; 4])> {
    // 공백/빈 글자는 제외 (경계는 간격으로 판정).
    let glyphs: Vec<&TextChar> = chars
        .iter()
        .filter(|c| !c.text.trim().is_empty())
        .collect();
    if glyphs.is_empty() {
        return None;
    }
    // ── 1) 줄 클러스터링: y 중심으로 정렬 후 세로로 겹치는 글자끼리 한 줄. ──
    let mut sorted = glyphs.clone();
    sorted.sort_by(|a, b| {
        a.rect[1]
            .total_cmp(&b.rect[1])
            .then(a.rect[0].total_cmp(&b.rect[0]))
    });
    let mut lines: Vec<Vec<&TextChar>> = Vec::new();
    for c in sorted {
        let cy = (c.rect[1] + c.rect[3]) * 0.5;
        let h = (c.rect[3] - c.rect[1]).max(1.0);
        let mut placed = false;
        for line in lines.iter_mut() {
            let (ly0, ly1) = line_y_extent(line);
            if cy >= ly0 - h * 0.5 && cy <= ly1 + h * 0.5 {
                line.push(c);
                placed = true;
                break;
            }
        }
        if !placed {
            lines.push(vec![c]);
        }
    }

    // ── 2) 줄마다: x정렬 + 줄높이(최대 글자높이) + 셀 박스. ──
    struct Line<'a> {
        chars: Vec<&'a TextChar>,
        height: f32,
        yc: f32,
        x0: f32,
        x1: f32,
        y0: f32,
        y1: f32,
    }
    let mut built: Vec<Line> = Vec::with_capacity(lines.len());
    for mut line in lines {
        line.sort_by(|a, b| a.rect[0].total_cmp(&b.rect[0]));
        let mut x0 = f32::MAX;
        let mut x1 = f32::MIN;
        let mut y0 = f32::MAX;
        let mut y1 = f32::MIN;
        let mut height = 1.0f32;
        for c in &line {
            x0 = x0.min(c.rect[0]);
            x1 = x1.max(c.rect[2]);
            y0 = y0.min(c.rect[1]);
            y1 = y1.max(c.rect[3]);
            height = height.max(c.rect[3] - c.rect[1]);
        }
        height = height.max(4.0);
        let yc = (y0 + y1) * 0.5;
        built.push(Line {
            chars: line,
            height,
            yc,
            x0,
            x1,
            y0: yc - height * 0.5,
            y1: yc + height * 0.5,
        });
    }

    // ── 3) 포인트가 닿은 줄 선택 (세로 중심 거리 최소). ──
    let mut best: Option<(&Line, f32)> = None;
    for line in &built {
        let hit = point[0] >= line.x0 - margin
            && point[0] <= line.x1 + margin
            && point[1] >= line.y0 - margin
            && point[1] <= line.y1 + margin;
        if !hit {
            continue;
        }
        let d = (point[1] - line.yc).abs();
        if best.map_or(true, |(_, bd)| d < bd) {
            best = Some((line, d));
        }
    }
    let line = best?.0;

    // ── 4) 줄 안에서 x 중심이 가장 가까운 글자 (시드). ──
    let mut seed: Option<(usize, f32)> = None;
    for (i, c) in line.chars.iter().enumerate() {
        let cx = (c.rect[0] + c.rect[2]) * 0.5;
        let d = (point[0] - cx).abs();
        if seed.map_or(true, |(_, bd)| d < bd) {
            seed = Some((i, d));
        }
    }
    let (si, _) = seed?;

    // ── 5) 좌우 확장: 글자 간격 ≤ 줄높이×0.5이면 이어짐 (공백 = 넓은 갭). ──
    let gap_max = (line.height * 0.5).max(1.0);
    let mut start = si;
    while start > 0 && line.chars[start].rect[0] - line.chars[start - 1].rect[2] <= gap_max {
        start -= 1;
    }
    let mut end = si;
    while end + 1 < line.chars.len() && line.chars[end + 1].rect[0] - line.chars[end].rect[2] <= gap_max
    {
        end += 1;
    }

    // ── 6) 단어 조립. ──
    let mut word = String::new();
    let mut bb = line.chars[start].rect;
    for c in &line.chars[start..=end] {
        word.push_str(&c.text);
        bb[0] = bb[0].min(c.rect[0]);
        bb[1] = bb[1].min(c.rect[1]);
        bb[2] = bb[2].max(c.rect[2]);
        bb[3] = bb[3].max(c.rect[3]);
    }
    if word.is_empty() {
        None
    } else {
        Some((word, bb))
    }
}

fn line_y_extent(line: &[&TextChar]) -> (f32, f32) {
    let mut y0 = f32::MAX;
    let mut y1 = f32::MIN;
    for c in line {
        y0 = y0.min(c.rect[1]);
        y1 = y1.max(c.rect[3]);
    }
    (y0, y1)
}

fn union(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}

/// 하이라이터 스트로크가 지나간 영역(`bbox`, 페이지 좌표)과 겹치는 텍스트를
/// 찾아, **줄 단위로 합친** 하이라이트 사각형 목록을 반환합니다.
///
/// - `bbox`는 스트로크 경계([x0, y0, x1, y1]).
/// - `margin`은 페이지 좌표로 몇 pt 안까지 텍스트에 닿은 것으로 칠지.
/// - 결과 각 사각형은 같은 줄(y 근처)에 있는 닿은 런들을 하나로 이어붙인 것.
pub fn text_line_highlights(runs: &[TextRun], bbox: [f32; 4], margin: f32) -> Vec<[f32; 4]> {
    let (bx0, by0, bx1, by1) = (
        bbox[0] - margin,
        bbox[1] - margin,
        bbox[2] + margin,
        bbox[3] + margin,
    );
    // 1) 스트로크에 닿은(겹치는) 런만 남긴다.
    let touched: Vec<&TextRun> = runs
        .iter()
        .filter(|r| {
            !r.text.trim().is_empty()
                && r.rect[0] <= bx1
                && r.rect[2] >= bx0
                && r.rect[1] <= by1
                && r.rect[3] >= by0
        })
        .collect();
    if touched.is_empty() {
        return Vec::new();
    }
    // 2) 닿은 런들을 줄(y 근처) 단위로 묶어 x 범위를 합친다.
    let mut lines: Vec<[f32; 4]> = Vec::new();
    for r in touched {
        let yc = (r.rect[1] + r.rect[3]) * 0.5;
        let h = (r.rect[3] - r.rect[1]).max(1.0);
        let mut placed = false;
        for line in lines.iter_mut() {
            let lyc = (line[1] + line[3]) * 0.5;
            if (yc - lyc).abs() < h * 0.8 {
                *line = union(*line, r.rect);
                placed = true;
                break;
            }
        }
        if !placed {
            lines.push(r.rect);
        }
    }
    lines
}

/// 드래그 영역(`bbox`, 표시/페이지 좌표)에 닿은 **글자별** 사각형들을 줄 단위로
/// 묶어, 각 줄마다 **하나의 연속 밴드**로 반환합니다.
///
/// - `char_rects`: pdfium `tight_bounds()`에서 얻은 글자 경계 `[x0,y0,x1,y1]`
///   (표시 공간 — `content_rect_to_display`로 변환된 것).
/// - 같은 줄(세로로 겹침)에 닿은 글자는 x 범위를 합쳐 한 밴드로 만듭니다
///   (글자 사이 공백 포함). 서로 다른 줄은 절대 합치지 않습니다.
pub fn char_line_highlights(
    char_rects: &[[f32; 4]],
    bbox: [f32; 4],
    margin: f32,
) -> Vec<[f32; 4]> {
    let (bx0, by0, bx1, by1) = (
        bbox[0] - margin,
        bbox[1] - margin,
        bbox[2] + margin,
        bbox[3] + margin,
    );
    // 1) 드래그에 닿은 글자만 남깁니다.
    let mut touched: Vec<[f32; 4]> = char_rects
        .iter()
        .filter(|r| r[2] >= bx0 && r[0] <= bx1 && r[3] >= by0 && r[1] <= by1)
        .copied()
        .collect();
    if touched.is_empty() {
        return Vec::new();
    }
    // 2) 읽기 순서(y, x)로 정렬 후, 세로로 겹치는 글자끼리 한 줄로 합칩니다.
    //    (인접 줄은 줄 간격 때문에 y가 겹치지 않으므로 섞이지 않음)
    touched.sort_by(|a, b| a[1].total_cmp(&b[1]).then(a[0].total_cmp(&b[0])));
    let mut lines: Vec<[f32; 4]> = Vec::new();
    for r in touched {
        let mut placed = false;
        for line in lines.iter_mut() {
            if r[1] <= line[3] && r[3] >= line[1] {
                *line = union(*line, r);
                placed = true;
                break;
            }
        }
        if !placed {
            lines.push(r);
        }
    }
    lines.sort_by(|a, b| a[1].total_cmp(&b[1]).then(a[0].total_cmp(&b[0])));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── content_rect_to_display: 2026-09 pypdfium2 픽셀 실측값으로 검증 ──
    // W=200, H=100 미디어박스, charbox (23.44, 80.0, 43.32, 108.72), scale 2 렌더에서
    // 관측된 잉크 픽셀: rot0 (47,0,80,39), rot90 (160,47,199,80),
    // rot180 (319,160,352,199), rot270 (0,319,39,352) — charbox는 잉크보다 약간 큼.

    #[test]
    fn display_mapping_matches_pixel_ground_truth() {
        let cb: [f32; 4] = [23.44, 80.0, 43.32, 108.72];
        let (w, h) = (200.0, 100.0);
        let approx = |a: [f32; 4], b: [f32; 4]| {
            a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() < 1e-3)
        };
        // None: (x, H−y)
        assert!(
            approx(content_rect_to_display(cb, w, h, PageRotation::None), [23.44, -8.72, 43.32, 20.0]),
            "None 매핑"
        );
        // 90°: (y, x) → [b, l, t, r]
        assert!(
            approx(content_rect_to_display(cb, w, h, PageRotation::Degrees90), [80.0, 23.44, 108.72, 43.32]),
            "90° 매핑"
        );
        // 180°: (W−x, y) → [W−r, b, W−l, t]
        assert!(
            approx(content_rect_to_display(cb, w, h, PageRotation::Degrees180), [156.68, 80.0, 176.56, 108.72]),
            "180° 매핑"
        );
        // 270°: (H−y, W−x) → [H−t, W−r, H−b, W−l]
        assert!(
            approx(content_rect_to_display(cb, w, h, PageRotation::Degrees270), [-8.72, 156.68, 20.0, 176.56]),
            "270° 매핑"
        );
    }

    #[test]
    fn display_mapping_non_rotated_simple() {
        // 콘텐츠 [10, 20, 30, 50] → 표시 [10, H−50, 30, H−20]
        assert_eq!(
            content_rect_to_display([10.0, 20.0, 30.0, 50.0], 200.0, 100.0, PageRotation::None),
            [10.0, 50.0, 30.0, 80.0]
        );
    }

    #[test]
    fn rot180_keeps_y_range_of_content() {
        // 실측 특이점: pdfium은 "비회전 래스터를 180° 회전"하므로 y는 콘텐츠 그대로,
        // x만 좌우 반전됩니다 (회귀 방지).
        let cb = [23.44, 80.0, 43.32, 108.72];
        let d = content_rect_to_display(cb, 200.0, 100.0, PageRotation::Degrees180);
        assert!((d[1] - cb[1]).abs() < 1e-3 && (d[3] - cb[3]).abs() < 1e-3);
        assert!((d[0] - (200.0 - cb[2])).abs() < 1e-3);
    }

    // ── word_at ──

    /// "Hello world" 글자 단위 (폭 8pt/공백 6pt, 간격 2pt, y 50..68).
    fn word_chars() -> Vec<TextChar> {
        let text = "Hello world";
        let mut out = Vec::new();
        let mut x = 100.0f32;
        for ch in text.chars() {
            let w = if ch == ' ' { 6.0 } else { 8.0 };
            out.push(TextChar::new(ch.to_string(), [x, 50.0, x + w, 68.0]));
            x += w + 2.0;
        }
        out
    }

    #[test]
    fn word_at_finds_whole_word() {
        let chars = word_chars();
        let (word, bb) = word_at(&chars, [117.0, 59.0], 2.0).expect("word found");
        assert_eq!(word, "Hello");
        assert!((bb[0] - 100.0).abs() < 1e-3);
    }

    #[test]
    fn word_at_finds_second_word() {
        let chars = word_chars();
        let (word, _) = word_at(&chars, [165.0, 59.0], 2.0).expect("word found");
        assert_eq!(word, "world");
    }

    #[test]
    fn word_at_tap_above_letters_still_hits() {
        // 글자 잉크 위(어센더 여백, y 48)를 탭해도 줄 셀 박스+margin으로 인식.
        let chars = word_chars();
        let (word, _) = word_at(&chars, [117.0, 48.0], 4.0).expect("word found");
        assert_eq!(word, "Hello");
    }

    #[test]
    fn word_at_tap_below_letters_still_hits() {
        let chars = word_chars();
        let (word, _) = word_at(&chars, [165.0, 70.0], 4.0).expect("word found");
        assert_eq!(word, "world");
    }

    #[test]
    fn word_at_gap_picks_nearest_word() {
        let chars = word_chars();
        // 두 단어 사이 공백(150pt) — 이제는 가까운 단어를 반환.
        let (word, _) = word_at(&chars, [150.0, 59.0], 4.0).expect("nearest word");
        assert!(word == "Hello" || word == "world");
        // 완전히 빈 영역.
        assert!(word_at(&chars, [300.0, 300.0], 4.0).is_none());
    }

    #[test]
    fn word_at_picks_correct_line_between_lines() {
        // 두 줄: 1행 y 10..26, 2행 y 60..76. 줄 박스 근처 탭 → 가까운 줄 선택.
        let mut chars = Vec::new();
        for (i, ch) in "cat".chars().enumerate() {
            chars.push(TextChar::new(ch.to_string(), [
                100.0 + i as f32 * 10.0,
                10.0,
                108.0 + i as f32 * 10.0,
                26.0,
            ]));
        }
        for (i, ch) in "dog".chars().enumerate() {
            chars.push(TextChar::new(ch.to_string(), [
                100.0 + i as f32 * 10.0,
                60.0,
                108.0 + i as f32 * 10.0,
                76.0,
            ]));
        }
        // 1행 바로 아래(y 29) → "cat", 2행 바로 위(y 57) → "dog".
        assert_eq!(word_at(&chars, [110.0, 29.0], 4.0).unwrap().0, "cat");
        assert_eq!(word_at(&chars, [110.0, 57.0], 4.0).unwrap().0, "dog");
    }

    #[test]
    fn word_at_keeps_apostrophes() {
        let chars = vec![
            TextChar::new("w", [0.0, 0.0, 8.0, 18.0]),
            TextChar::new("o", [10.0, 0.0, 18.0, 18.0]),
            TextChar::new("n", [20.0, 0.0, 28.0, 18.0]),
            TextChar::new("'", [30.0, 0.0, 36.0, 18.0]),
            TextChar::new("t", [38.0, 0.0, 46.0, 18.0]),
        ];
        let (word, _) = word_at(&chars, [12.0, 9.0], 2.0).expect("word found");
        assert_eq!(word, "won't");
    }

    #[test]
    fn word_at_scrambled_order_still_finds_word() {
        // 글자 입력 순서가 공간 순서와 다르더라도(내용 스트림 순서) 줄 클러스터링과
        // x 정렬 덕에 같은 단어를 찾습니다.
        let mut chars = vec![
            TextChar::new("l", [130.0, 50.0, 138.0, 68.0]),
            TextChar::new("H", [100.0, 50.0, 108.0, 68.0]),
            TextChar::new("o", [140.0, 50.0, 148.0, 68.0]),
            TextChar::new("e", [110.0, 50.0, 118.0, 68.0]),
            TextChar::new("l", [120.0, 50.0, 128.0, 68.0]),
        ];
        let (word, _) = word_at(&chars, [115.0, 59.0], 2.0).expect("word found");
        assert_eq!(word, "Hello");
    }

    // ── char_line_highlights (글자 단위 하이라이트 판정) ─────────────────

    /// 두 줄의 글자 사각형: 1행 y 10..26, 2행 y 60..76. 표시 공간 좌표.
    fn two_line_chars() -> Vec<[f32; 4]> {
        let mut v = Vec::new();
        // 1행: 서로 떨어진 3개 덩어리
        for (a, b) in [(20.0, 60.0), (80.0, 120.0), (160.0, 300.0)] {
            v.push([a, 10.0, b, 26.0]);
        }
        // 2행
        for (a, b) in [(30.0, 70.0), (90.0, 200.0)] {
            v.push([a, 60.0, b, 76.0]);
        }
        v
    }

    #[test]
    fn char_highlights_merge_same_line_into_one_band() {
        let chars = two_line_chars();
        let rects = char_line_highlights(&chars, [0.0, 0.0, 500.0, 30.0], 3.0);
        assert_eq!(rects.len(), 1, "1행만 닿음 → 밴드 1개");
        let r = rects[0];
        assert!((r[0] - 20.0).abs() < 1e-3, "시작은 첫 글자 왼쪽");
        assert!((r[2] - 300.0).abs() < 1e-3, "끝은 마지막 글자 오른쪽(공백 포함)");
        assert!((r[1] - 10.0).abs() < 1e-3 && (r[3] - 26.0).abs() < 1e-3);
    }

    #[test]
    fn char_highlights_keep_lines_separate() {
        let chars = two_line_chars();
        let rects = char_line_highlights(&chars, [0.0, 0.0, 500.0, 90.0], 3.0);
        assert_eq!(rects.len(), 2, "줄이 다르면 밴드도 분리");
        assert!(rects[0][1] < rects[1][1], "위쪽 줄 먼저");
    }

    #[test]
    fn char_highlights_partial_drag_hits_only_touched_chars() {
        let chars = two_line_chars();
        let rects = char_line_highlights(&chars, [30.0, 5.0, 50.0, 25.0], 3.0);
        assert_eq!(rects.len(), 1);
        let r = rects[0];
        assert!((r[0] - 20.0).abs() < 1e-3);
        assert!((r[2] - 60.0).abs() < 1e-3, "닿은 글자까지만");
    }

    #[test]
    fn char_highlights_margin_touches_adjacent() {
        let chars = two_line_chars();
        let rects = char_line_highlights(&chars, [0.0, 3.0, 500.0, 8.0], 3.0);
        assert!(!rects.is_empty(), "margin으로 1행과 접촉");
    }

    #[test]
    fn char_highlights_empty_area_is_empty() {
        let chars = two_line_chars();
        assert!(
            char_line_highlights(&chars, [400.0, 300.0, 450.0, 330.0], 3.0).is_empty()
        );
    }

    // ── text_line_highlights (런 단위, 기존 동작 유지) ─────────────────

    #[test]
    fn text_highlights_union_same_line_and_filter_far() {
        let runs = vec![
            TextRun::new("Hello ", [10.0, 10.0, 60.0, 26.0], vec![]),
            TextRun::new("World", [60.0, 10.0, 120.0, 26.0], vec![]),
            TextRun::new("Below", [10.0, 40.0, 60.0, 56.0], vec![]),
        ];
        let rects = text_line_highlights(&runs, [0.0, 5.0, 200.0, 30.0], 2.0);
        assert_eq!(rects.len(), 1);
        let r = rects[0];
        assert!((r[0] - 10.0).abs() < 1e-3);
        assert!((r[2] - 120.0).abs() < 1e-3);
        assert!(r[3] <= 30.0, "아랫줄은 포함되면 안 됨");
    }

    #[test]
    fn text_highlights_no_touch_returns_empty() {
        let runs = vec![TextRun::new("Hello", [10.0, 10.0, 60.0, 26.0], vec![])];
        assert!(
            text_line_highlights(&runs, [200.0, 200.0, 300.0, 260.0], 2.0).is_empty()
        );
    }
}
