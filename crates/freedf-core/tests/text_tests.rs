//! `text` 모듈 단위 테스트 — `src/text.rs`의 `#[cfg(test)]` 모듈에서 이동했습니다.

use freedf_core::search::TextRun;
use freedf_core::text::*;

// ── content_rect_to_display: 2026-09 pypdfium2 픽셀 실측값으로 검증 ──
// W=200, H=100 미디어박스, charbox (23.44, 80.0, 43.32, 108.72), scale 2 렌더에서
// 관측된 잉크 픽셀: rot0 (47,0,80,39), rot90 (160,47,199,80),
// rot180 (319,160,352,199), rot270 (0,319,39,352) — charbox는 잉크보다 약간 큼.

#[test]
fn display_mapping_matches_pixel_ground_truth() {
    let cb: [f32; 4] = [23.44, 80.0, 43.32, 108.72];
    let (w, h) = (200.0, 100.0);
    let approx =
        |a: [f32; 4], b: [f32; 4]| a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() < 1e-3);
    // None: (x, H−y)
    assert!(
        approx(
            content_rect_to_display(cb, w, h, PageRotation::None),
            [23.44, -8.72, 43.32, 20.0]
        ),
        "None 매핑"
    );
    // 90°: (y, x) → [b, l, t, r]
    assert!(
        approx(
            content_rect_to_display(cb, w, h, PageRotation::Degrees90),
            [80.0, 23.44, 108.72, 43.32]
        ),
        "90° 매핑"
    );
    // 180°: (W−x, y) → [W−r, b, W−l, t]
    assert!(
        approx(
            content_rect_to_display(cb, w, h, PageRotation::Degrees180),
            [156.68, 80.0, 176.56, 108.72]
        ),
        "180° 매핑"
    );
    // 270°: (H−y, W−x) → [H−t, W−r, H−b, W−l]
    assert!(
        approx(
            content_rect_to_display(cb, w, h, PageRotation::Degrees270),
            [-8.72, 156.68, 20.0, 176.56]
        ),
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
        chars.push(TextChar::new(
            ch.to_string(),
            [100.0 + i as f32 * 10.0, 10.0, 108.0 + i as f32 * 10.0, 26.0],
        ));
    }
    for (i, ch) in "dog".chars().enumerate() {
        chars.push(TextChar::new(
            ch.to_string(),
            [100.0 + i as f32 * 10.0, 60.0, 108.0 + i as f32 * 10.0, 76.0],
        ));
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
    let chars = vec![
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
    assert!(
        (r[2] - 300.0).abs() < 1e-3,
        "끝은 마지막 글자 오른쪽(공백 포함)"
    );
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
    assert!(char_line_highlights(&chars, [400.0, 300.0, 450.0, 330.0], 3.0).is_empty());
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
    assert!(text_line_highlights(&runs, [200.0, 200.0, 300.0, 260.0], 2.0).is_empty());
}
