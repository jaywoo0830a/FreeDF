//! `search` 모듈 단위 테스트 — `src/search.rs`의 `#[cfg(test)]` 모듈에서 이동했습니다.

use freedf_core::search::*;

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
    let runs = vec![run("the cat", vec![]), run("the dog and the bird", vec![])];
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
