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
}
