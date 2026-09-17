//! `transform` 모듈 단위 테스트 — `src/transform.rs`의 `#[cfg(test)]` 모듈에서 이동했습니다.

use freedf_core::transform::*;

fn assert_close(a: [f32; 2], b: [f32; 2], eps: f32) {
    assert!(
        (a[0] - b[0]).abs() <= eps && (a[1] - b[1]).abs() <= eps,
        "{a:?} vs {b:?}"
    );
}

#[test]
fn round_trip_page_view() {
    let mut t = ViewTransform::identity();
    t.zoom = 2.5;
    t.pan_x = 100.0;
    t.pan_y = -50.0;
    let page = [300.0, 200.0];
    let view = t.page_to_view(page);
    let back = t.view_to_page(view);
    assert_close(back, page, 1e-3);
}

#[test]
fn zoom_at_keeps_anchor_fixed() {
    let mut t = ViewTransform {
        zoom: 1.0,
        pan_x: 0.0,
        pan_y: 0.0,
    };
    let anchor = [400.0, 300.0]; // 뷰 좌표
    t.zoom_at(anchor, 2.0, MIN_ZOOM, MAX_ZOOM);
    // anchor가 가리키던 페이지 지점이 여전히 anchor에 있어야 함.
    assert_close(t.page_to_view(t.view_to_page(anchor)), anchor, 1e-3);
    assert!((t.zoom - 2.0).abs() < 1e-6);
}

#[test]
fn zoom_at_then_zoom_out_returns() {
    let mut t = ViewTransform::identity();
    let anchor = [123.0, 456.0];
    t.zoom_at(anchor, 3.0, MIN_ZOOM, MAX_ZOOM);
    t.zoom_at(anchor, 1.0 / 3.0, MIN_ZOOM, MAX_ZOOM);
    assert!((t.zoom - 1.0).abs() < 1e-3);
    assert!((t.pan_x - 0.0).abs() < 1e-3);
    assert!((t.pan_y - 0.0).abs() < 1e-3);
}

#[test]
fn clamp_pan_margin_is_the_overscroll_limit() {
    // margin이 클수록 페이지가 캔버스 밖으로 더 멀리 나갈 수 있습니다
    // (오버스크롤 — 모든 UI 모드에서 동일한 clamp 경로를 사용).
    let page = [595.0, 842.0];
    let canvas = [800.0, 600.0];
    let mut big = ViewTransform {
        zoom: 2.0,
        pan_x: 0.0,
        pan_y: -9999.0,
    };
    big.clamp_pan(page, canvas, 64.0);
    let mut small = ViewTransform {
        zoom: 2.0,
        pan_x: 0.0,
        pan_y: -9999.0,
    };
    small.clamp_pan(page, canvas, 16.0);
    // 아래쪽 한계 = canvas_h - view_h - margin → margin 64가 16보다 48 더 내려감
    // (pan_y는 더 음수).
    assert!((small.pan_y - big.pan_y - 48.0).abs() < 1e-3);
    assert!(big.pan_y < small.pan_y, "margin이 크면 더 아래까지 스크롤");
}

#[test]
fn clamp_pan_keeps_large_page_inside_canvas() {
    // 페이지가 캔버스보다 크면 양 끝을 여백 안으로 제한
    let mut t = ViewTransform {
        zoom: 1.0,
        pan_x: 9999.0,
        pan_y: -9999.0,
    };
    let page = [1000.0, 1400.0];
    let canvas = [800.0, 600.0];
    let margin = 16.0;
    t.clamp_pan(page, canvas, margin);
    // pan_x 범위: [800-1000-16=-216, 16]
    assert!(t.pan_x <= 16.0 && t.pan_x >= -216.0);
    // pan_y 범위: [600-1400-16=-816, 16]
    assert!(t.pan_y <= 16.0 && t.pan_y >= -816.0);
}

#[test]
fn clamp_pan_keeps_small_page_near_center() {
    // 페이지가 캔버스보다 작으면 여백 안에서만 이동
    let mut t = ViewTransform {
        zoom: 1.0,
        pan_x: 5000.0,
        pan_y: 5000.0,
    };
    let page = [200.0, 200.0];
    let canvas = [800.0, 600.0];
    t.clamp_pan(page, canvas, 16.0);
    // pan_x: [800-200-16=584, 16] -> [16, 584]
    assert!(t.pan_x >= 16.0 && t.pan_x <= 584.0);
    // pan_y: [600-200-16=384, 16] -> [16, 384]
    assert!(t.pan_y >= 16.0 && t.pan_y <= 384.0);
}

#[test]
fn clamp_pan_keeps_centered_page_unchanged() {
    // center_page 후 clamp는 위치를 바꾸지 않아야 함
    let page = [595.0, 842.0];
    let canvas = [1280.0, 820.0];
    let mut t = ViewTransform::identity();
    t.center_page(page, canvas, 16.0);
    let before = (t.pan_x, t.pan_y);
    t.clamp_pan(page, canvas, 16.0);
    assert!((t.pan_x - before.0).abs() < 1e-3);
    assert!((t.pan_y - before.1).abs() < 1e-3);
}

#[test]
fn align_page_positions_left_center_right() {
    let page = [400.0, 600.0];
    let canvas = [1000.0, 800.0];
    let m = 20.0;

    let mut t = ViewTransform::identity();
    t.align_page(page, canvas, m, PageAlign::Left);
    assert!((t.pan_x - m).abs() < 1e-3);

    let mut t = ViewTransform::identity();
    t.align_page(page, canvas, m, PageAlign::Right);
    // pan_x = canvas_w - page_w - margin
    assert!((t.pan_x - (canvas[0] - 400.0 - m)).abs() < 1e-3);

    let mut t = ViewTransform::identity();
    t.align_page(page, canvas, m, PageAlign::Center);
    assert!((t.pan_x - (canvas[0] - 400.0) / 2.0).abs() < 1e-3);

    // 세로는 항상 위쪽 정렬
    let mut t = ViewTransform::identity();
    t.align_page(page, canvas, m, PageAlign::Right);
    assert!((t.pan_y - m).abs() < 1e-3);
}

#[test]
fn align_page_wide_page_fills_canvas() {
    // 페이지가 캔버스보다 넓으면 margin 이상으로 벌어지지 않음
    let page = [2000.0, 600.0];
    let canvas = [1000.0, 800.0];
    let m = 20.0;
    let mut t = ViewTransform::identity();
    t.align_page(page, canvas, m, PageAlign::Right);
    assert!(t.pan_x >= m);
    let mut t = ViewTransform::identity();
    t.align_page(page, canvas, m, PageAlign::Left);
    assert!((t.pan_x - m).abs() < 1e-3);
}

#[test]
fn zoom_is_clamped() {
    let mut t = ViewTransform::identity();
    t.zoom_at([0.0, 0.0], 10_000.0, MIN_ZOOM, MAX_ZOOM);
    assert!((t.zoom - MAX_ZOOM).abs() < 1e-6);
    t.zoom_at([0.0, 0.0], 1e-9, MIN_ZOOM, MAX_ZOOM);
    assert!((t.zoom - MIN_ZOOM).abs() < 1e-6);
}

#[test]
fn pan_moves_page() {
    let mut t = ViewTransform::identity();
    t.pan_by(30.0, -20.0);
    assert_close([t.pan_x, t.pan_y], [30.0, -20.0], 1e-6);
    // 팬 후 원점의 페이지 좌표는 (-30, 20) 방향으로 이동
    assert_close(t.view_to_page([30.0, -20.0]), [0.0, 0.0], 1e-6);
}

#[test]
fn fit_width_zoom_respects_aspect() {
    let zoom = ViewTransform::fit_width_zoom(595.0, 1200.0, 16.0);
    let view_w = 595.0 * zoom;
    assert!((view_w - 1168.0).abs() < 1.0);
}

#[test]
fn fit_height_zoom_respects_aspect() {
    let zoom = ViewTransform::fit_height_zoom(842.0, 800.0, 16.0);
    let view_h = 842.0 * zoom;
    assert!((view_h - 768.0).abs() < 1.0);
}

#[test]
fn fit_page_zoom_never_exceeds() {
    // 세로가 더 긴 A4: 595 x 842
    let zoom = ViewTransform::fit_page_zoom([595.0, 842.0], [1000.0, 800.0], 16.0);
    let [w, h] = [595.0 * zoom, 842.0 * zoom];
    assert!(w <= 968.0 + 1.0 && h <= 768.0 + 1.0);
}

#[test]
fn center_page_places_top_left() {
    let mut t = ViewTransform {
        zoom: 2.0,
        pan_x: 9999.0,
        pan_y: 9999.0,
    };
    t.center_page([595.0, 842.0], [1400.0, 1000.0], 24.0);
    let view = t.page_to_view([0.0, 0.0]);
    // 페이지가 가로 중앙 + 상단 여백에 배치됨
    assert!((view[0] - (1400.0 - 595.0 * 2.0) / 2.0).abs() < 1e-3);
    assert!((view[1] - 24.0).abs() < 1e-3);
}

// ── 브라우저식 PgUp/PgDn (browser_page_step) ─────────────────────────
// 상수: page_h_pts=1400, zoom=1.0 → view_h=1400. canvas_h=600, margin=16.
// top=16, bottom=600-1400-16=-816. 한 단계=600(캔버스 높이).

#[test]
fn pgdn_on_fitting_page_goes_next() {
    // 페이지가 캔버스에 다 들어가면(스크롤 불가) 무조건 다음 페이지.
    assert_eq!(
        browser_page_step(500.0, 1.0, 600.0, 16.0, 16.0, true),
        PageStep::NextPage
    );
}

#[test]
fn pgup_on_fitting_page_goes_prev() {
    assert_eq!(
        browser_page_step(500.0, 1.0, 600.0, 16.0, 16.0, false),
        PageStep::PrevPage
    );
}

#[test]
fn pgdn_scrolls_down_one_viewport_before_advancing() {
    // 상단(pan_y=16)에서 PgDn → 페이지를 건너뛰지 않고 한 뷰포트만 아래로.
    let step = browser_page_step(1400.0, 1.0, 600.0, 16.0, 16.0, true);
    assert!(matches!(step, PageStep::ScrollTo { .. }));
    if let PageStep::ScrollTo { pan_y } = step {
        assert!((pan_y - (16.0 - 600.0)).abs() < 1e-3, "한 뷰포트만 이동: {pan_y}");
    }
}

#[test]
fn pgdn_at_bottom_advances_to_next_page() {
    // 이미 바닥(pan_y=-816)이면 PgDn → 다음 페이지.
    assert_eq!(
        browser_page_step(1400.0, 1.0, 600.0, 16.0, -816.0, true),
        PageStep::NextPage
    );
}

#[test]
fn pgup_at_top_goes_prev_page() {
    assert_eq!(
        browser_page_step(1400.0, 1.0, 600.0, 16.0, 16.0, false),
        PageStep::PrevPage
    );
}

#[test]
fn pgdn_sequence_clamps_at_bottom_then_advances() {
    let m = 16.0_f32;
    // ① 상단 → -584, ② -584 → -816(바닥), ③ 바닥 → 다음 페이지.
    let s1 = browser_page_step(1400.0, 1.0, 600.0, m, m, true);
    let pan1 = match s1 {
        PageStep::ScrollTo { pan_y } => pan_y,
        _ => panic!("첫 PgDn은 스크롤이어야 함: {s1:?}"),
    };
    assert!((pan1 + 584.0).abs() < 1e-3);
    let s2 = browser_page_step(1400.0, 1.0, 600.0, m, pan1, true);
    let pan2 = match s2 {
        PageStep::ScrollTo { pan_y } => pan_y,
        _ => panic!("둘째 PgDn은 스크롤이어야 함: {s2:?}"),
    };
    assert!((pan2 + 816.0).abs() < 1e-3, "바닥으로 클램프");
    assert_eq!(browser_page_step(1400.0, 1.0, 600.0, m, pan2, true), PageStep::NextPage);
}

#[test]
fn pgup_from_bottom_scrolls_back_to_top_then_prev() {
    let m = 16.0_f32;
    // 바닥(-816) → -216 → 16(상단) → 이전 페이지.
    let s1 = browser_page_step(1400.0, 1.0, 600.0, m, -816.0, false);
    let pan1 = match s1 {
        PageStep::ScrollTo { pan_y } => pan_y,
        _ => panic!("첫 PgUp은 스크롤이어야 함: {s1:?}"),
    };
    assert!((pan1 + 216.0).abs() < 1e-3);
    let s2 = browser_page_step(1400.0, 1.0, 600.0, m, pan1, false);
    let pan2 = match s2 {
        PageStep::ScrollTo { pan_y } => pan_y,
        _ => panic!("둘째 PgUp은 스크롤이어야 함: {s2:?}"),
    };
    assert!((pan2 - 16.0).abs() < 1e-3, "상단으로 클램프");
    assert_eq!(browser_page_step(1400.0, 1.0, 600.0, m, pan2, false), PageStep::PrevPage);
}

#[test]
fn pgdn_when_slightly_scrolled_uses_remaining_room() {
    // 바닥 근처(-800)에서 PgDn → 남은 여유만큼만(-816) 이동, 페이지는 안 넘어감.
    let step = browser_page_step(1400.0, 1.0, 600.0, 16.0, -800.0, true);
    match step {
        PageStep::ScrollTo { pan_y } => assert!((pan_y + 816.0).abs() < 1e-3),
        other => panic!("아직 여유가 있으면 스크롤이어야 함: {other:?}"),
    }
}
