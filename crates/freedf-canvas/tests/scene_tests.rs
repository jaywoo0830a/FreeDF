//! `scene` 모듈 단위 테스트 — `src/scene.rs`의 `#[cfg(test)]` 모듈에서 이동했습니다.

use freedf_canvas::geom::PagePoint;
use freedf_canvas::scene::*;

fn stroke(id: u64) -> Stroke {
    Stroke {
        id: StrokeId(id),
        kind: LayerKind::Ink,
        tool: freedf_core::model::ToolType::Pen,
        color: [0, 0, 0, 255],
        base_width: 2.0,
        points: vec![StrokePoint {
            position: PagePoint::new(id as f32, 0.0),
            pressure: 0.5,
            t_ms: 0,
            width: 0.0,
        }],
        created_ms: 0,
    }
}

/// 계약: add/remove가 rev를 증가시킵니다.
#[test]
fn mutations_bump_revision() {
    let mut store = SceneStore::new();
    assert_eq!(store.rev(), Revision(0));
    store.add(stroke(1));
    assert_eq!(store.rev(), Revision(1));
    store.add(stroke(2));
    assert_eq!(store.rev(), Revision(2));
    assert!(store.remove(StrokeId(1)).is_some());
    assert_eq!(store.rev(), Revision(3));
    assert!(
        store.remove(StrokeId(99)).is_none(),
        "없는 획 삭제는 rev 불변"
    );
    assert_eq!(store.rev(), Revision(3));
}

/// 계약: changes_since는 그 rev 이후의 변경만 반환 — 증분 굽기의 근거.
#[test]
fn changes_since_returns_only_newer_ops() {
    let mut store = SceneStore::new();
    store.add(stroke(1));
    let mid = store.add(stroke(2));
    store.add(stroke(3));
    store.remove(StrokeId(1));
    let end = store.rev();

    let from_mid = store.changes_since(mid);
    assert_eq!(from_mid.from_revision, mid);
    assert_eq!(from_mid.to_revision, end);
    assert_eq!(from_mid.added.len(), 1, "mid 이후 추가는 3번 하나");
    assert_eq!(from_mid.added[0].id, StrokeId(3));
    assert_eq!(from_mid.removed, vec![StrokeId(1)], "삭제는 mid 이후");

    let all = store.changes_since(Revision(0));
    assert_eq!(all.added.len(), 3);
    assert_eq!(all.removed, vec![StrokeId(1)]);
}

/// 계약: 스냅샷은 rev와 내용이 일치해야 합니다 (낡은 스냅샷 감지 근거).
#[test]
fn snapshot_carries_revision_and_strokes() {
    let mut store = SceneStore::new();
    store.add(stroke(1));
    let snap = store.snapshot();
    assert_eq!(snap.revision, store.rev());
    assert_eq!(snap.strokes, store.strokes());
}
