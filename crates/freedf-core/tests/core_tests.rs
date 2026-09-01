//! freedf-core 통합 테스트.
//!
//! 실제 사용 시나리오(그리기 → 지우개 → 실행취소 → 저장/불러오기)를
//! GUI 없이 검증합니다.

use freedf_core::history::{Edit, History};
use freedf_core::model::{StrokePoint, ToolType};
use freedf_core::store::AnnotationStore;
use freedf_core::transform::{ViewTransform, MIN_ZOOM, MAX_ZOOM};

/// 시나리오: 펜으로 필기 → 형광펜 강조 → 지우개로 일부 삭제 → undo.
#[test]
fn full_annotation_scenario() {
    let mut store = AnnotationStore::new();
    let mut history = History::new(64);

    // 1) 펜 스트로크 2개 그리기
    let pen_id = store.add_stroke(
        0,
        ToolType::Pen,
        [20, 20, 20, 255],
        2.0,
        vec![
            StrokePoint::new(50.0, 60.0, 0.3),
            StrokePoint::new(80.0, 90.0, 0.7),
            StrokePoint::new(120.0, 100.0, 0.9),
        ],
    );
    history.push(Edit::AddStrokes {
        page: 0,
        strokes: vec![store.stroke(0, pen_id).cloned().unwrap()],
    });

    // 2) 형광펜 스트로크
    let hi_id = store.add_stroke(
        0,
        ToolType::Highlighter,
        [255, 235, 59, 90],
        16.0,
        vec![StrokePoint::new(50.0, 55.0, 0.5), StrokePoint::new(150.0, 55.0, 0.5)],
    );
    history.push(Edit::AddStrokes {
        page: 0,
        strokes: vec![store.stroke(0, hi_id).cloned().unwrap()],
    });

    assert_eq!(store.stroke_count_on(0), 2);
    assert_eq!(store.total_stroke_count(), 2);

    // 3) 지우개로 형광펜 지우기
    let erased = store.erase_at(0, [150.0, 55.0], 20.0);
    assert_eq!(erased.len(), 1);
    assert_eq!(erased[0].id, hi_id);
    history.push(Edit::RemoveStrokes {
        page: 0,
        strokes: erased,
    });
    assert_eq!(store.stroke_count_on(0), 1);

    // 4) undo → 형광펜 복원
    let inverse = history.undo().expect("undo available");
    store.apply_edit(&inverse);
    assert_eq!(store.stroke_count_on(0), 2);
    assert!(store.stroke(0, hi_id).is_some());

    // 5) undo → (복원된) 형광펜이 다시 제거되어 펜만 남음
    let inverse = history.undo().expect("undo available");
    store.apply_edit(&inverse);
    assert_eq!(store.stroke_count_on(0), 1);
    assert!(store.stroke(0, hi_id).is_none());
    assert!(store.stroke(0, pen_id).is_some());

    // 6) redo → 형광펜 복원
    let edit = history.redo().expect("redo available");
    store.apply_edit(&edit);
    assert_eq!(store.stroke_count_on(0), 2);
    assert!(store.stroke(0, pen_id).is_some());
    assert!(store.stroke(0, hi_id).is_some());
}

/// 시나리오: 여러 페이지에 메모 → 페이지 전체 지우기 → undo로 복구.
#[test]
fn multi_page_and_clear_page() {
    let mut store = AnnotationStore::new();
    let mut history = History::new(64);

    let mut ids = Vec::new();
    for page in 0..3 {
        let id = store.add_stroke(
            page,
            ToolType::Pen,
            [0, 0, 0, 255],
            2.0,
            vec![StrokePoint::new(10.0 + page as f32, 10.0, 0.5)],
        );
        ids.push(id);
        history.push(Edit::AddStrokes {
            page,
            strokes: vec![store.stroke(page, id).cloned().unwrap()],
        });
    }
    assert_eq!(store.page_count(), 3);

    // 1페이지 전체 지우기
    let cleared = store.clear_page(1);
    assert_eq!(cleared.len(), 1);
    history.push(Edit::RemoveStrokes {
        page: 1,
        strokes: cleared,
    });
    assert_eq!(store.stroke_count_on(1), 0);
    assert_eq!(store.stroke_count_on(0), 1);
    assert_eq!(store.stroke_count_on(2), 1);

    // undo → 1페이지 복구
    let inverse = history.undo().expect("undo");
    store.apply_edit(&inverse);
    assert_eq!(store.stroke_count_on(1), 1);
    assert!(store.stroke(1, ids[1]).is_some());
}

/// 시나리오: JSON 저장 → 새 인스턴스로 불러오기.
#[test]
fn save_and_load_annotations() {
    let mut store = AnnotationStore::new();
    store.add_stroke(
            2,
            ToolType::Highlighter,
            [255, 0, 0, 80],
            10.0,
            vec![StrokePoint::new(300.0, 400.0, 0.6)],
        );

    let json = store.to_json();

    let mut loaded = AnnotationStore::new();
    loaded = AnnotationStore::from_json(&json).expect("load");

    assert_eq!(loaded, store);
    assert_eq!(loaded.strokes_on(2).len(), 1);
    assert_eq!(loaded.strokes_on(2)[0].tool, ToolType::Highlighter);

    // 같은 문서에 이어서 그리면 기존 데이터와 합쳐짐
    loaded.add_stroke(
        2,
        ToolType::Pen,
        [0, 0, 0, 255],
        2.0,
        vec![StrokePoint::new(1.0, 1.0, 0.5)],
    );
    assert_eq!(loaded.stroke_count_on(2), 2);
}

/// 좌표 변환과 줌/팬의 일관성.
#[test]
fn transform_consistency_with_fit() {
    let page = [595.0, 842.0]; // A4
    let canvas = [1280.0, 800.0];

    let mut t = ViewTransform::identity();
    t.zoom = ViewTransform::fit_width_zoom(page[0], canvas[0], 16.0);
    t.center_page(page, canvas, 24.0);

    // 페이지 좌상단 → 뷰 (가로 중앙 근처)
    let top_left = t.page_to_view([0.0, 0.0]);
    assert!(top_left[0] >= 0.0);
    assert!((top_left[1] - 24.0).abs() < 1e-3);

    // 페이지 우하단 → 뷰 (가로로 캔버스를 넘지 않음)
    let bottom_right = t.page_to_view(page);
    assert!(bottom_right[0] <= canvas[0] + 1.0);

    // 뷰 좌표 → 페이지 → 뷰 왕복
    let v = [333.0, 200.0];
    let round = t.page_to_view(t.view_to_page(v));
    assert!((round[0] - v[0]).abs() < 1e-3 && (round[1] - v[1]).abs() < 1e-3);
}

/// 줌 상/하한 보장.
#[test]
fn zoom_bounds_are_respected() {
    let mut t = ViewTransform::identity();
    t.zoom_at([0.0, 0.0], 1e9, MIN_ZOOM, MAX_ZOOM);
    assert!(t.zoom <= MAX_ZOOM);
    t.zoom_at([0.0, 0.0], 1e-9, MIN_ZOOM, MAX_ZOOM);
    assert!(t.zoom >= MIN_ZOOM);
}
