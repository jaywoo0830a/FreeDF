//! 페이지 내 단어 검색.
//!
//! pdfium에서 추출한 텍스트 런(문자별 좌표 포함)을 받아
//! 대소문자 무시 검색을 수행하고, 일치 구간의 하이라이트 사각형을 계산합니다.
//! 순수 데이터 연산이라 GUI 없이 단위 테스트로 검증합니다.

use serde::{Deserialize, Serialize};

/// 페이지의 텍스트 한 런(연속된 스타일의 텍스트)과 문자별 좌표.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextRun {
    pub text: String,
    /// 런 전체 경계 (페이지 포인트) [x0, y0, x1, y1]
    pub rect: [f32; 4],
    /// 각 문자(chars)의 경계. `text`의 char 수와 1:1 정렬.
    pub char_rects: Vec<[f32; 4]>,
}

impl TextRun {
    pub fn new(text: impl Into<String>, rect: [f32; 4], char_rects: Vec<[f32; 4]>) -> Self {
        Self {
            text: text.into(),
            rect,
            char_rects,
        }
    }
}

/// 검색 일치 결과 하나.
#[derive(Debug, Clone, PartialEq)]
pub struct TextMatch {
    /// 몇 번째 런에서 찾았는지
    pub run: usize,
    /// 일치 시작 char 인덱스 (inclusive)
    pub char_start: usize,
    /// 일치 끝 char 인덱스 (exclusive)
    pub char_end: usize,
    /// 하이라이트 사각형 (페이지 포인트) [x0, y0, x1, y1]
    pub rect: [f32; 4],
    /// 실제 일치한 문자열
    pub matched: String,
}

/// 모든 런에서 `query`를 검색합니다. 대소문자는 무시합니다.
/// 빈 쿼리는 빈 결과를 반환합니다. 같은 런 내 겹침은 비겹침(non-overlapping)으로 처리합니다.
pub fn find_matches(runs: &[TextRun], query: &str) -> Vec<TextMatch> {
    let needle: Vec<char> = query.trim().chars().collect();
    if needle.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (ri, run) in runs.iter().enumerate() {
        let hay: Vec<char> = run.text.chars().collect();
        let mut i = 0;
        while i + needle.len() <= hay.len() {
            let mut ok = true;
            for (j, nc) in needle.iter().enumerate() {
                if !chars_eq(hay[i + j], *nc) {
                    ok = false;
                    break;
                }
            }
            if ok {
                let rect = rect_for_range(run, i, i + needle.len());
                let matched: String = hay[i..i + needle.len()].iter().collect();
                out.push(TextMatch {
                    run: ri,
                    char_start: i,
                    char_end: i + needle.len(),
                    rect,
                    matched,
                });
                i += needle.len();
            } else {
                i += 1;
            }
        }
    }
    out
}

fn chars_eq(a: char, b: char) -> bool {
    a.to_lowercase().eq(b.to_lowercase())
}

/// 일치 구간의 사각형. 문자 좌표가 있으면 그 합집합, 없으면 런 사각형 비례 추정.
fn rect_for_range(run: &TextRun, start: usize, end: usize) -> [f32; 4] {
    if end <= run.char_rects.len() {
        let mut r = run.char_rects[start];
        for c in &run.char_rects[start..end] {
            r = union(r, *c);
        }
        r
    } else {
        // 문자 좌표가 없으면 런 사각형 안에서 문자 수 비율로 추정
        let n = run.text.chars().count().max(1) as f32;
        let s = start as f32 / n;
        let e = end as f32 / n;
        let w = run.rect[2] - run.rect[0];
        let x0 = run.rect[0] + w * s;
        let x1 = run.rect[0] + w * e;
        [x0, run.rect[1], x1, run.rect[3]]
    }
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
///
/// 순수 데이터 연산이라 GUI 없이 단위 테스트로 검증합니다.
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
///   (표시 공간 — 세로 뒤집기/회전은 호출부가 미리 적용).
/// - 같은 줄(세로로 겹침)에 닿은 글자는 x 범위를 합쳐 한 밴드로 만듭니다
///   (글자 사이 공백 포함). 서로 다른 줄은 절대 합치지 않습니다.
/// - **필압은 관여하지 않습니다** — 칠할 두께는 항상 그 줄의 높이와 같습니다.
///
/// 순수 데이터 연산이라 GUI 없이 단위 테스트로 검증합니다.
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

/// 탭한 위치에 있는 **단어**를 찾습니다 (사전 오버레이용).
///
/// `chars`는 pdfium에서 추출한 (글자, 표시 공간 사각형) 목록입니다.
/// - 점을 포함하는 글자(여백 2pt)를 찾은 뒤, 같은 줄(세로 중심이 가까움)에서
///   글자 간 간격이 `글자 높이 × 0.6` 이하인 이웃으로 좌우로 확장해 단어를 만듭니다.
/// - 공백/구두점만 있거나 아무 글자도 없으면 `None`.
///
/// 순수 데이터 연산이라 GUI 없이 단위 테스트로 검증합니다.
pub fn word_at(chars: &[(String, [f32; 4])], point: [f32; 2]) -> Option<(String, [f32; 4])> {
    if chars.is_empty() {
        return None;
    }
    // 1) 점 근처(여백 2pt)의 글자 중 **중심이 가장 가까운** 글자 찾기 —
    //    단어 사이 공백을 탭하면 공백(→None)이 선택되도록 합니다.
    let margin = 2.0f32;
    let mut best: Option<(usize, f32)> = None;
    for (i, (_, r)) in chars.iter().enumerate() {
        let hit = point[0] >= r[0] - margin
            && point[0] <= r[2] + margin
            && point[1] >= r[1] - margin
            && point[1] <= r[3] + margin;
        if !hit {
            continue;
        }
        let cx = (r[0] + r[2]) * 0.5;
        let cy = (r[1] + r[3]) * 0.5;
        let d2 = (point[0] - cx).powi(2) + (point[1] - cy).powi(2);
        if best.map_or(true, |(_, bd)| d2 < bd) {
            best = Some((i, d2));
        }
    }
    let idx = best?.0;
    if chars[idx].0.trim().is_empty() {
        return None;
    }
    let h = (chars[idx].1[3] - chars[idx].1[1]).max(1.0);
    let gap_max = h * 0.6;
    let cy = (chars[idx].1[1] + chars[idx].1[3]) * 0.5;
    // 같은 줄 판정: 세로 중심이 글자 높이의 0.75 이내.
    let same_line = |r: &[f32; 4]| ((r[1] + r[3]) * 0.5 - cy).abs() <= h * 0.75;
    // 2) 왼쪽으로 확장 (공백은 단어 경계 — 통과하지 않음).
    let mut start = idx;
    while start > 0 {
        if chars[start - 1].0.trim().is_empty() {
            break;
        }
        let prev = &chars[start - 1].1;
        if prev[0] < chars[idx].1[0]
            && same_line(prev)
            && chars[start].1[0] - prev[2] <= gap_max
        {
            start -= 1;
        } else {
            break;
        }
    }
    // 3) 오른쪽으로 확장 (공백은 단어 경계).
    let mut end = idx;
    while end + 1 < chars.len() {
        if chars[end + 1].0.trim().is_empty() {
            break;
        }
        let next = &chars[end + 1].1;
        if next[0] > chars[idx].1[0] && same_line(next) && next[0] - chars[end].1[2] <= gap_max {
            end += 1;
        } else {
            break;
        }
    }
    // 4) 단어 조립 (공백은 제거, 아포스트로피/하이픈은 유지).
    let mut word = String::new();
    let mut bb = chars[start].1;
    for (text, r) in &chars[start..=end] {
        if !text.trim().is_empty() {
            word.push_str(text);
        }
        bb[0] = bb[0].min(r[0]);
        bb[1] = bb[1].min(r[1]);
        bb[2] = bb[2].max(r[2]);
        bb[3] = bb[3].max(r[3]);
    }
    if word.is_empty() {
        None
    } else {
        Some((word, bb))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str, char_rects: Vec<[f32; 4]>) -> TextRun {
        let rect = if char_rects.is_empty() {
            [0.0, 0.0, 100.0, 20.0]
        } else {
            union(char_rects[0], *char_rects.last().unwrap())
        };
        TextRun::new(text, rect, char_rects)
    }

    #[test]
    fn finds_word_with_correct_text() {
        let runs = vec![run("hello world", vec![])];
        let m = find_matches(&runs, "world");
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].matched, "world");
        assert_eq!(m[0].run, 0);
    }

    #[test]
    fn case_insensitive() {
        let runs = vec![run("Hello HELLO hello", vec![])];
        let m = find_matches(&runs, "hello");
        assert_eq!(m.len(), 3);
    }

    #[test]
    fn multiple_occurrences_and_across_runs() {
        let runs = vec![
            run("the cat", vec![]),
            run("the dog and the bird", vec![]),
        ];
        let m = find_matches(&runs, "the");
        assert_eq!(m.len(), 3);
        assert_eq!(m.iter().map(|x| x.run).collect::<Vec<_>>(), vec![0, 1, 1]);
    }

    #[test]
    fn empty_query_and_no_match() {
        let runs = vec![run("abc", vec![])];
        assert!(find_matches(&runs, "").is_empty());
        assert!(find_matches(&runs, "   ").is_empty());
        assert!(find_matches(&runs, "xyz").is_empty());
    }

    #[test]
    fn unicode_korean_search() {
        let runs = vec![run("안녕하세요 세계입니다", vec![])];
        let m = find_matches(&runs, "세계");
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].matched, "세계");
    }

    #[test]
    fn char_rect_union_is_computed() {
        // "abc" 각 문자의 좌표
        let char_rects = vec![
            [0.0, 0.0, 10.0, 20.0],
            [10.0, 0.0, 20.0, 20.0],
            [20.0, 0.0, 30.0, 20.0],
        ];
        let runs = vec![run("abc", char_rects.clone())];
        let m = find_matches(&runs, "abc");
        assert_eq!(m[0].rect, [0.0, 0.0, 30.0, 20.0]);

        let runs2 = vec![run("abc", char_rects.clone())];
        let m2 = find_matches(&runs2, "bc");
        assert_eq!(m2[0].rect, [10.0, 0.0, 30.0, 20.0]);
    }

    #[test]
    fn proportional_fallback_without_char_rects() {
        // "hello" 5글자, 런 [0..100]
        let runs = vec![run("hello", vec![])];
        let m = find_matches(&runs, "ell");
        // 1..4 문자 → x0=20, x1=80
        assert!((m[0].rect[0] - 20.0).abs() < 1e-3);
        assert!((m[0].rect[2] - 80.0).abs() < 1e-3);
    }

    #[test]
    fn non_overlapping_matches() {
        let runs = vec![run("aaaa", vec![])];
        let m = find_matches(&runs, "aa");
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].char_start, 0);
        assert_eq!(m[1].char_start, 2);
    }

    #[test]
    fn query_is_trimmed() {
        let runs = vec![run("hello world", vec![])];
        assert_eq!(find_matches(&runs, "  world  ").len(), 1);
    }

    #[test]
    fn text_highlights_union_same_line_and_filter_far() {
        // 한 줄에 여러 런, 그 아래 다른 줄 하나.
        let runs = vec![
            TextRun::new("Hello ", [10.0, 10.0, 60.0, 26.0], vec![]),
            TextRun::new("World", [60.0, 10.0, 120.0, 26.0], vec![]),
            TextRun::new("Below", [10.0, 40.0, 60.0, 56.0], vec![]),
        ];
        // 위 줄(첫 두 런)만 덮는 스트로크.
        let rects = text_line_highlights(&runs, [0.0, 5.0, 200.0, 30.0], 2.0);
        assert_eq!(rects.len(), 1);
        // 같은 줄 두 런이 하나로 합쳐져야 함.
        let r = rects[0];
        assert!((r[0] - 10.0).abs() < 1e-3);
        assert!((r[2] - 120.0).abs() < 1e-3);
        assert!(r[3] <= 30.0, "아랫줄은 포함되면 안 됨");
    }

    #[test]
    fn text_highlights_no_touch_returns_empty() {
        let runs = vec![TextRun::new("Hello", [10.0, 10.0, 60.0, 26.0], vec![])];
        // 텍스트와 멀리 떨어진 스트로크.
        assert!(text_line_highlights(&runs, [200.0, 200.0, 300.0, 260.0], 2.0).is_empty());
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
        // 1행 전체를 덮는 드래그 → 밴드 1개, 2행 미포함.
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
        // 두 줄 모두 덮는 드래그 → 줄마다 밴드 (절대 합치지 않음).
        let rects = char_line_highlights(&chars, [0.0, 0.0, 500.0, 90.0], 3.0);
        assert_eq!(rects.len(), 2, "줄이 다르면 밴드도 분리");
        assert!(rects[0][1] < rects[1][1], "위쪽 줄 먼저");
    }

    #[test]
    fn char_highlights_partial_drag_hits_only_touched_chars() {
        let chars = two_line_chars();
        // 1행의 첫 덩어리(x 20..60)만 겹치게 1행 높이 안에서 드래그.
        let rects = char_line_highlights(&chars, [30.0, 5.0, 50.0, 25.0], 3.0);
        assert_eq!(rects.len(), 1);
        let r = rects[0];
        assert!((r[0] - 20.0).abs() < 1e-3);
        assert!((r[2] - 60.0).abs() < 1e-3, "닿은 글자까지만");
    }

    #[test]
    fn char_highlights_margin_touches_adjacent() {
        let chars = two_line_chars();
        // 텍스트 바로 위(살짝 떨어진) 선을 그어도 margin 덕에 닿음.
        let rects = char_line_highlights(&chars, [0.0, 3.0, 500.0, 8.0], 3.0);
        assert!(!rects.is_empty(), "margin으로 1행과 접촉");
    }

    #[test]
    fn char_highlights_empty_area_is_empty() {
        let chars = two_line_chars();
        assert!(char_line_highlights(&chars, [400.0, 300.0, 450.0, 330.0], 3.0).is_empty());
    }

    // ---------- word_at (사전 오버레이) ----------

    /// "Hello world"를 글자 단위(폭 8pt, 간격 2pt)로 만듭니다.
    fn word_chars() -> Vec<(String, [f32; 4])> {
        let text = "Hello world";
        let mut out = Vec::new();
        let mut x = 100.0f32;
        for ch in text.chars() {
            let w = if ch == ' ' { 6.0 } else { 8.0 };
            out.push((ch.to_string(), [x, 50.0, x + w, 68.0]));
            x += w + 2.0;
        }
        out
    }

    #[test]
    fn word_at_finds_whole_word() {
        let chars = word_chars();
        // "Hello"의 두 번째 글자 'e' 위를 탭.
        let (word, bb) = word_at(&chars, [117.0, 59.0]).expect("word found");
        assert_eq!(word, "Hello");
        assert!((bb[0] - 100.0).abs() < 1e-3);
    }

    #[test]
    fn word_at_finds_second_word() {
        let chars = word_chars();
        let (word, _) = word_at(&chars, [165.0, 59.0]).expect("word found");
        assert_eq!(word, "world");
    }

    #[test]
    fn word_at_misses_gap_and_empty_area() {
        let chars = word_chars();
        // 두 단어 사이 공백 위치.
        assert!(word_at(&chars, [150.0, 59.0]).is_none());
        // 빈 곳.
        assert!(word_at(&chars, [300.0, 300.0]).is_none());
    }

    #[test]
    fn word_at_keeps_apostrophes() {
        let chars = vec![
            ("w".to_string(), [0.0, 0.0, 8.0, 18.0]),
            ("o".to_string(), [10.0, 0.0, 18.0, 18.0]),
            ("n".to_string(), [20.0, 0.0, 28.0, 18.0]),
            ("'".to_string(), [30.0, 0.0, 36.0, 18.0]),
            ("t".to_string(), [38.0, 0.0, 46.0, 18.0]),
        ];
        let (word, _) = word_at(&chars, [12.0, 9.0]).expect("word found");
        assert_eq!(word, "won't");
    }
}
