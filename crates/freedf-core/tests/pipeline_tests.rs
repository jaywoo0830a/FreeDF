//! `pipeline` 모듈 단위 테스트 — `src/pipeline.rs`의 `#[cfg(test)]` 모듈에서 이동했습니다.

use freedf_core::pipeline::*;
use freedf_core::model::{StrokePoint, ToolType};
use freedf_core::pen::Materials;

fn pipeline() -> InkPipeline {
    InkPipeline::new(Materials::default(), 3.0, 0.4)
}

// ---- LiveStroke: append-only + frontier + incremental bbox ----

#[test]
fn live_stroke_incremental_bbox_matches_scan() {
    let mut l = LiveStroke::begin(ToolType::Pen, [0, 0, 0, 255], 3.0);
    let pts = vec![
        StrokePoint::new(5.0, 5.0, 0.5),
        StrokePoint::new(1.0, 9.0, 0.5),
        StrokePoint::new(8.0, 2.0, 0.5),
    ];
    for p in pts {
        l.append(p);
    }
    let inc = l.bbox.expect("incremental bbox");
    let scanned = l.freeze(0, 0).bounding_box().expect("scan bbox");
    assert_eq!(inc, scanned, "incremental bbox == scanned bbox");
    let exp: [f32; 4] = [1.0, 2.0, 8.0, 9.0];
    assert_eq!(inc, exp);
}

#[test]
fn live_stroke_frontier_makes_tail_shrink_and_grow() {
    let mut l = LiveStroke::begin(ToolType::Pen, [0, 0, 0, 255], 3.0);
    assert_eq!(l.mesh_done, 0);
    assert_eq!(l.tail().len(), 0);
    l.append(StrokePoint::new(0.0, 0.0, 0.5));
    l.append(StrokePoint::new(1.0, 0.0, 0.5));
    assert_eq!(l.points.len(), 2);
    assert_eq!(l.mesh_done, 0, "append does not move the frontier");
    assert_eq!(l.tail().len(), 2, "everything is an unbaked tail");
    l.mark_meshed();
    assert_eq!(l.mesh_done, 2);
    assert_eq!(l.tail().len(), 0, "fully meshed");
    l.append(StrokePoint::new(2.0, 0.0, 0.5));
    assert_eq!(l.tail().len(), 1, "only the new point is in the tail");
    assert_eq!(l.points.len(), 3);
}

#[test]
fn live_stroke_fix_last_blocked_at_frontier() {
    let mut l = LiveStroke::begin(ToolType::Pen, [0, 0, 0, 255], 3.0);
    l.append(StrokePoint::new(0.0, 0.0, 0.5));
    l.append(StrokePoint::new(1.0, 0.0, 0.5));
    l.mark_meshed(); // frontier == len == 2
    let w0 = l.points.last().expect("last").width;
    l.fix_last(StrokePoint::new(1.0, 0.0, 0.9)); // at/before frontier — ignored (immutable prefix)
    assert_eq!(l.points.last().expect("last").width, w0, "immutable prefix must not change");
    l.append(StrokePoint::new(2.0, 0.0, 0.5)); // len 3 > frontier 2
    l.fix_last(StrokePoint {
        x: 2.0,
        y: 0.0,
        pressure: 0.5,
        t_ms: 0,
        width: 0.9,
    }); // after frontier — applied
    assert!((l.points.last().expect("last").width - 0.9).abs() < 1e-6);
}

#[test]
fn live_stroke_freeze_detaches_from_live() {
    let mut l = LiveStroke::begin(ToolType::Pen, [0, 0, 0, 255], 3.0);
    l.append(StrokePoint::new(0.0, 0.0, 0.5));
    l.append(StrokePoint::new(4.0, 4.0, 0.5));
    let frozen = l.freeze(7, 1000);
    assert_eq!(frozen.id, 7);
    assert_eq!(frozen.tool, ToolType::Pen);
    assert_eq!(frozen.points.len(), 2);
    // appended after freeze; the committed copy is unchanged.
    l.append(StrokePoint::new(9.0, 9.0, 0.5));
    assert_eq!(frozen.points.len(), 2, "frozen copy does not change");
    assert_eq!(l.points.len(), 3);
    let exp: [f32; 4] = [0.0, 0.0, 4.0, 4.0];
    assert_eq!(frozen.bounding_box().expect("bbox"), exp);
}

// ---- InkPipeline: down/drag/up orchestration ----

#[test]
fn pipeline_down_starts_with_locked_first_point() {
    let mut p = pipeline();
    let tip = p.down(ToolType::Pen, [10, 20, 30, 255], 100.0, 200.0, 0.7, 0.0, 0, 0.0);
    assert!(p.is_drawing());
    let l = p.live().expect("live");
    assert_eq!(l.points.len(), 1);
    assert!(l.points[0].width > 0.0, "first point width locked");
    assert!((tip.width - l.points[0].width).abs() < 1e-6);
}

#[test]
fn pipeline_drag_appends_and_locks_widths() {
    let mut p = pipeline();
    p.down(ToolType::Pen, [0, 0, 0, 255], 10.0, 20.0, 0.5, 0.0, 0, 0.0);
    p.drag(15.0, 22.0, 0.6, 0.016, 10);
    p.drag(20.0, 21.0, 0.7, 0.032, 20);
    let l = p.live().expect("live");
    assert!(l.points.len() >= 3, "each drag appends a point");
    for i in 0..l.points.len() {
        assert!(l.points[i].width > 0.0, "all points have locked widths");
    }
}

#[test]
fn pipeline_up_preserves_wysiwyg_no_penup_change() {
    let mut p = pipeline();
    p.down(ToolType::Pen, [0, 0, 0, 255], 10.0, 20.0, 0.5, 0.0, 0, 0.0);
    for i in 1..30 {
        p.drag(
            10.0 + i as f32 * 3.0,
            20.0 + (i as f32 * 0.4).sin(),
            0.6 + (i as f32 * 0.01).sin(),
            i as f64 * 0.016,
            i * 10,
        );
    }
    let before = p.live().expect("live").points.last().expect("last").width;
    let frozen = p.up(42).expect("up");
    let after = frozen.points.last().expect("last").width;
    assert!(
        (after - before).abs() < 1e-5,
        "penup width unchanged (WYSIWYG): seen={before} locked={after}",
    );
    assert!(!p.is_drawing(), "inactive after up");
    assert_eq!(frozen.id, 42);
}

#[test]
fn pipeline_second_stroke_starts_fresh() {
    let mut p = pipeline();
    p.down(ToolType::Pen, [0, 0, 0, 255], 0.0, 0.0, 0.5, 0.0, 0, 0.0);
    p.drag(1.0, 0.0, 0.5, 0.016, 10);
    let _ = p.up(1);
    assert!(!p.is_drawing());
    p.down(ToolType::Pen, [0, 0, 0, 255], 50.0, 50.0, 0.5, 0.0, 0, 0.0);
    let l = p.live().expect("live");
    assert_eq!(l.points.len(), 1, "new stroke does not inherit previous points");
}

/// 계약: `down`에 준 `tilt_mag`가 WidthLocker까지 전달돼 폭에 반영됩니다.
/// (BallPenProfile.tilt_k=0.35 → 기울기가 클수록 더 두꺼운 획)
#[test]
fn pipeline_down_threads_tilt_into_width_locker() {
    fn locked_width_after(tilt: f32) -> f32 {
        // smoothing 0 → 필터가 좌표를 왜곡하지 않고 틸트 폭만 비교합니다.
        let mut p = InkPipeline::new(Materials::default(), 3.0, 0.0);
        p.down(ToolType::Pen, [0, 0, 0, 255], 0.0, 0.0, 0.5, 0.0, 0, tilt);
        p.drag(10.0, 0.0, 0.5, 0.016, 16);
        p.drag(20.0, 0.0, 0.5, 0.032, 32);
        p.live().expect("live").points[1].width
    }
    let w0 = locked_width_after(0.0);
    let w1 = locked_width_after(1.0);
    assert!(
        w1 > w0,
        "tilt must widen the pen stroke (threaded through the pipeline): \
         tilt0={w0} vs tilt1={w1}"
    );
}

/// 계약: `smoothing <= 0.001`이면 1€ 필터를 건너뛰어 **raw 좌표**를 통과시킵니다
/// (앱의 \"스무딩 꺼짐 → raw push\" 경로와 행동 일치).
#[test]
fn pipeline_smoothing_zero_passes_raw_coords() {
    let mut p = InkPipeline::new(Materials::default(), 3.0, 0.0);
    p.down(ToolType::Pen, [0, 0, 0, 255], 10.0, 20.0, 0.5, 0.0, 0, 0.0);
    p.drag(15.0, 22.0, 0.6, 0.016, 16);
    p.drag(20.0, 21.0, 0.7, 0.032, 32);
    let pts = &p.live().expect("live").points;
    assert!((pts[1].x - 15.0).abs() < 1e-4, "raw x passthrough: {}", pts[1].x);
    assert!((pts[1].y - 22.0).abs() < 1e-4, "raw y passthrough: {}", pts[1].y);
    assert!((pts[2].x - 20.0).abs() < 1e-4, "raw x passthrough: {}", pts[2].x);
}
