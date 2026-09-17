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

/// 두 사각형의 합집합(바운딩 박스) — 하이라이트 사각형 계산과 테스트가 공유합니다.
pub fn union(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}

// 탭한 위치의 **단어** 찾기, 글자/런 단위 하이라이트 판정은
// `crate::text` 모듈로 이동했습니다 — 같은 좌표 규약을 한 곳에서 공유합니다.
