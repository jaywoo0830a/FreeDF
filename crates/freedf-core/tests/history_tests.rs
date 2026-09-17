//! `history` 모듈 단위 테스트 — `src/history.rs`의 `#[cfg(test)]` 모듈에서 이동했습니다.

use freedf_core::history::*;
use freedf_core::model::{PageIndex, Stroke};

fn stroke(id: u64) -> Stroke {
    Stroke {
        created_ms: 0,
        id,
        tool: freedf_core::model::ToolType::Pen,
        color: [0, 0, 0, 255],
        width: 2.0,
        points: vec![freedf_core::model::StrokePoint::new(1.0, 2.0, 0.5)],
    }
}

fn add_edit(page: PageIndex, strokes: Vec<Stroke>) -> Edit {
    Edit::AddStrokes { page, strokes }
}

#[test]
fn push_clears_redo() {
    let mut h = History::new(10);
    h.push(add_edit(0, vec![stroke(1)]));
    let _ = h.undo();
    assert!(h.can_redo());
    h.push(add_edit(0, vec![stroke(2)]));
    assert!(!h.can_redo());
    assert_eq!(h.undo_len(), 1);
}

#[test]
fn undo_redo_cycle() {
    let mut h = History::new(10);
    h.push(add_edit(0, vec![stroke(1)]));
    assert!(h.can_undo());

    let inverse = h.undo().expect("undo");
    assert!(matches!(inverse, Edit::RemoveStrokes { .. }));
    assert!(!h.can_undo());
    assert!(h.can_redo());

    let original = h.redo().expect("redo");
    assert!(matches!(original, Edit::AddStrokes { .. }));
    assert!(h.can_undo());
    assert!(!h.can_redo());
}

#[test]
fn undo_on_empty_is_none() {
    let mut h = History::new(10);
    assert!(h.undo().is_none());
    assert!(h.redo().is_none());
}

#[test]
fn limit_trims_oldest() {
    let mut h = History::new(3);
    for i in 0..5 {
        h.push(add_edit(0, vec![stroke(i)]));
    }
    assert_eq!(h.undo_len(), 3);
    // 가장 오래된 0,1은 잘려나가고 2,3,4가 남음
    let first = h.undo().expect("undo");
    match first {
        Edit::RemoveStrokes { strokes, .. } => assert_eq!(strokes[0].id, 4),
        _ => panic!("invalid edit"),
    }
}

#[test]
fn clear_resets() {
    let mut h = History::new(10);
    h.push(add_edit(0, vec![stroke(1)]));
    h.clear();
    assert!(!h.can_undo() && !h.can_redo());
    assert_eq!(h.undo_len(), 0);
}

#[test]
fn inverse_of_inverse_returns_original() {
    let edit = Edit::RemoveStrokes {
        page: 3,
        strokes: vec![stroke(1), stroke(2)],
    };
    let restored = edit.inverse().inverse();
    assert_eq!(edit, restored);
}
