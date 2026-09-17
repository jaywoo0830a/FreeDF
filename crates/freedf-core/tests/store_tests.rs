//! `store` 모듈 단위 테스트 — `src/store.rs`의 `#[cfg(test)]` 모듈에서 이동했습니다.

use freedf_core::store::*;
use freedf_core::history::Edit;
use freedf_core::model::{StrokePoint, ToolType};
use freedf_core::paper::PagePaper;

fn sample_points() -> Vec<StrokePoint> {
    vec![
        StrokePoint::new(0.0, 0.0, 0.5),
        StrokePoint::new(100.0, 50.0, 0.8),
    ]
}

#[test]
fn add_stroke_gives_unique_increasing_ids() {
    let mut store = AnnotationStore::new();
    let a = store.add_stroke(0, ToolType::Pen, [0, 0, 0, 255], 2.0, sample_points());
    let b = store.add_stroke(0, ToolType::Highlighter, [255, 255, 0, 90], 14.0, sample_points());
    assert_eq!(a, 0);
    assert_eq!(b, 1);
    assert_eq!(store.stroke_count_on(0), 2);
    assert_eq!(store.total_stroke_count(), 2);
}

#[test]
fn strokes_are_per_page() {
    let mut store = AnnotationStore::new();
    store.add_stroke(0, ToolType::Pen, [0, 0, 0, 255], 2.0, sample_points());
    store.add_stroke(2, ToolType::Pen, [0, 0, 0, 255], 2.0, sample_points());
    assert_eq!(store.stroke_count_on(0), 1);
    assert_eq!(store.stroke_count_on(1), 0);
    assert_eq!(store.stroke_count_on(2), 1);
    assert_eq!(store.page_count(), 2);
}

#[test]
fn remove_stroke_returns_and_removes() {
    let mut store = AnnotationStore::new();
    let id = store.add_stroke(0, ToolType::Pen, [0, 0, 0, 255], 2.0, sample_points());
    let removed = store.remove_stroke(0, id);
    assert!(removed.is_some());
    assert_eq!(removed.unwrap().id, id);
    assert_eq!(store.stroke_count_on(0), 0);
    assert!(store.remove_stroke(0, 999).is_none());
}

#[test]
fn erase_at_removes_intersecting_only() {
    let mut store = AnnotationStore::new();
    let near = store.add_stroke(
        0,
        ToolType::Pen,
        [0, 0, 0, 255],
        2.0,
        vec![StrokePoint::new(10.0, 10.0, 0.5)],
    );
    let far = store.add_stroke(
        0,
        ToolType::Pen,
        [0, 0, 0, 255],
        2.0,
        vec![StrokePoint::new(500.0, 500.0, 0.5)],
    );
    let removed = store.erase_at(0, [10.0, 10.0], 20.0);
    let removed_ids: Vec<u64> = removed.iter().map(|s| s.id).collect();
    assert_eq!(removed_ids, vec![near]);
    assert!(store.stroke(0, near).is_none());
    assert!(store.stroke(0, far).is_some());
}

#[test]
fn erase_respects_radius() {
    let mut store = AnnotationStore::new();
    let id = store.add_stroke(
        0,
        ToolType::Pen,
        [0, 0, 0, 255],
        2.0,
        vec![StrokePoint::new(10.0, 10.0, 0.5)],
    );
    // 반지름이 닿지 않으면 지워지지 않음
    let removed = store.erase_at(0, [11.0, 10.0], 0.5);
    assert!(removed.is_empty());
    assert_eq!(store.stroke_count_on(0), 1);
    // 정확히 점 위를 지우면 반지름이 매우 작아도 지워짐 (정확한 히트)
    let removed = store.erase_at(0, [10.0, 10.0], 0.001);
    assert_eq!(removed.len(), 1);
    assert_eq!(removed[0].id, id);
    assert_eq!(store.stroke_count_on(0), 0);
}

#[test]
fn rotate_strokes_maps_to_new_display_space() {
    let mut store = AnnotationStore::new();
    // 100×50 페이지에 네 모서리 점 스트로크.
    let id = store.add_stroke(
        0,
        ToolType::Pen,
        [0, 0, 0, 255],
        2.0,
        vec![
            StrokePoint::new(0.0, 0.0, 0.5),
            StrokePoint::new(100.0, 0.0, 0.5),
            StrokePoint::new(100.0, 50.0, 0.5),
            StrokePoint::new(0.0, 50.0, 0.5),
        ],
    );
    // 시계 90°: (x, y) → (H - y, x) — 새 표시 공간 50×100.
    store.rotate_strokes_on(0, 100.0, 50.0, true);
    let pts: Vec<[f32; 2]> = store
        .strokes_on(0)
        .iter()
        .find(|s| s.id == id)
        .unwrap()
        .points
        .iter()
        .map(|p| p.to_array())
        .collect();
    assert_eq!(
        pts,
        vec![
            [50.0, 0.0],
            [50.0, 100.0],
            [0.0, 100.0],
            [0.0, 0.0],
        ]
    );
    // 반시계 90° 복원: 새 공간 50×100 → (x, y) → (y, W - x).
    store.rotate_strokes_on(0, 50.0, 100.0, false);
    let pts: Vec<[f32; 2]> = store
        .strokes_on(0)
        .iter()
        .find(|s| s.id == id)
        .unwrap()
        .points
        .iter()
        .map(|p| p.to_array())
        .collect();
    assert_eq!(
        pts,
        vec![
            [0.0, 0.0],
            [100.0, 0.0],
            [100.0, 50.0],
            [0.0, 50.0],
        ]
    );
}

/// 회전 변환이 여러 단계에서도 일관적인지 검증합니다:
/// CW→CCW = 원위치, CW×4 = 원위치, CW×2 = 180° 회전.
/// (표시 크기는 매 단계 너비/높이가 뒤집힘)
#[test]
fn rotation_composes_across_multiple_steps() {
    let rotate_cw = |p: (f32, f32), _w: f32, h: f32| (h - p.1, p.0);
    let rotate_ccw = |p: (f32, f32), w: f32, _h: f32| (p.1, w - p.0);

    let start = (100.0f32, 50.0f32);
    let (w0, h0) = (595.0f32, 842.0f32);

    // CW → CCW = 원위치.
    let p1 = rotate_cw(start, w0, h0);
    let back = rotate_ccw(p1, h0, w0);
    assert!((back.0 - start.0).abs() < 1e-3 && (back.1 - start.1).abs() < 1e-3);

    // CW × 4 = 원위치.
    let mut p = start;
    let (mut w, mut h) = (w0, h0);
    for _ in 0..4 {
        p = rotate_cw(p, w, h);
        std::mem::swap(&mut w, &mut h);
    }
    assert!((p.0 - start.0).abs() < 1e-3 && (p.1 - start.1).abs() < 1e-3);

    // CW × 2 = 180° 회전: (W - x, H - y).
    let mut q = start;
    let (mut w2, mut h2) = (w0, h0);
    for _ in 0..2 {
        q = rotate_cw(q, w2, h2);
        std::mem::swap(&mut w2, &mut h2);
    }
    assert!((q.0 - (w0 - start.0)).abs() < 1e-3);
    assert!((q.1 - (h0 - start.1)).abs() < 1e-3);
}

#[test]
fn clear_page_removes_all() {
    let mut store = AnnotationStore::new();
    store.add_stroke(0, ToolType::Pen, [0, 0, 0, 255], 2.0, sample_points());
    store.add_stroke(0, ToolType::Pen, [0, 0, 0, 255], 2.0, sample_points());
    let cleared = store.clear_page(0);
    assert_eq!(cleared.len(), 2);
    assert_eq!(store.stroke_count_on(0), 0);
}

#[test]
fn json_round_trip_preserves_data() {
    let mut store = AnnotationStore::new();
    store.add_stroke(0, ToolType::Pen, [10, 20, 30, 255], 2.0, sample_points());
    store.add_stroke(1, ToolType::Highlighter, [200, 100, 0, 90], 12.0, sample_points());
    let json = store.to_json();
    let restored = AnnotationStore::from_json(&json).expect("JSON 파싱 실패");
    assert_eq!(restored, store);
    assert_eq!(restored.strokes_on(0)[0].color, [10, 20, 30, 255]);
    assert_eq!(restored.strokes_on(1)[0].tool, ToolType::Highlighter);
}

#[test]
fn json_parse_error_is_reported() {
    assert!(AnnotationStore::from_json("not json").is_err());
}

#[test]
fn paper_settings_are_per_page() {
    let mut store = AnnotationStore::new();
    assert_eq!(store.paper_on(0), None);
    let grid = PagePaper {
        style: freedf_core::paper::PaperStyle::Grid,
        color: [240, 248, 241, 255],
    };
    store.set_paper(0, grid);
    store.set_paper(2, PagePaper::default());
    assert_eq!(store.paper_on(0), Some(grid));
    assert_eq!(store.paper_on(1), None);
    assert_eq!(
        store.paper_on_or(1, PagePaper::default()),
        PagePaper::default()
    );
    assert_eq!(store.paper_on(2), Some(PagePaper::default()));
}

#[test]
fn paper_settings_survive_json_round_trip() {
    let mut store = AnnotationStore::new();
    store.add_stroke(0, ToolType::Pen, [0, 0, 0, 255], 2.0, sample_points());
    store.set_paper(0, PagePaper {
        style: freedf_core::paper::PaperStyle::Ruled,
        color: [253, 247, 231, 255],
    });
    let json = store.to_json();
    let restored = AnnotationStore::from_json(&json).expect("JSON 파싱 실패");
    assert_eq!(restored, store);
    assert_eq!(
        restored.paper_on(0).map(|p| p.style),
        Some(freedf_core::paper::PaperStyle::Ruled)
    );
}

#[test]
fn ids_stay_monotonic_after_remove_and_add() {
    let mut store = AnnotationStore::new();
    let id = store.add_stroke(0, ToolType::Pen, [0, 0, 0, 255], 2.0, sample_points());
    store.remove_stroke(0, id);
    let next = store.add_stroke(0, ToolType::Pen, [0, 0, 0, 255], 2.0, sample_points());
    assert!(next > id, "ID는 단조 증가해야 함");
}

#[test]
fn apply_edit_add_and_remove() {
    let mut store = AnnotationStore::new();
    let id = store.add_stroke(0, ToolType::Pen, [0, 0, 0, 255], 2.0, sample_points());
    let stroke = store.stroke(0, id).cloned().unwrap();

    let edit = Edit::RemoveStrokes {
        page: 0,
        strokes: vec![stroke.clone()],
    };
    store.apply_edit(&edit);
    assert_eq!(store.stroke_count_on(0), 0);

    store.apply_inverse(&edit);
    assert_eq!(store.stroke_count_on(0), 1);
    assert_eq!(store.stroke(0, id), Some(&stroke));
}
