//! `soak` 모듈 단위 테스트 — `src/soak.rs`의 `#[cfg(test)]` 모듈에서 이동했습니다.

use freedf_canvas::soak::*;
use freedf_canvas::scene::Stroke;
use freedf_canvas::scene::{LayerKind, StrokeId, StrokePoint};
use freedf_canvas::geom::PagePoint;

fn stroke(id: u64, tool: freedf_core::model::ToolType) -> Stroke {
    Stroke {
        id: StrokeId(id),
        kind: LayerKind::Ink,
        tool,
        color: [0, 0, 0, 255],
        base_width: 2.0,
        points: vec![StrokePoint {
            position: PagePoint::new(0.0, 0.0),
            pressure: 1.0,
            t_ms: 10,
            width: 0.0,
        }],
        created_ms: 0,
    }
}

/// 모든 획이 정착한 것으로 간주하는 판정 함수.
fn settled_all(_: &Stroke) -> u64 {
    u64::MAX
}

#[test]
fn new_from_detects_tail_append_by_count() {
    let mut st = InkSettling::new();
    st.add(0, vec![stroke(1, freedf_core::model::ToolType::Pen)], 5);
    // rev가 같으면 새 획 없음.
    assert_eq!(st.new_from(0, 1, 5), None);
    // rev 증가 + 개수 증가 → 꼬리 인덱스 1 (settled 0 + young 1).
    assert_eq!(st.new_from(0, 2, 6), Some(1));
}

#[test]
fn add_resets_when_page_changes() {
    let mut st = InkSettling::new();
    st.add(3, vec![stroke(1, freedf_core::model::ToolType::Pen)], 5);
    st.add(4, vec![stroke(2, freedf_core::model::ToolType::Pen)], 6);
    assert_eq!(st.young.len(), 1, "이전 페이지 젊은 획은 버려짐");
    assert_eq!(st.young[0].id.0, 2);
    assert_eq!(st.settled, 0);
    assert_eq!(st.page, Some(4));
}

#[test]
fn sweep_moves_settled_and_keeps_young() {
    let mut st = InkSettling::new();
    st.add(
        0,
        vec![
            stroke(1, freedf_core::model::ToolType::Pen),
            stroke(2, freedf_core::model::ToolType::Fountain),
        ],
        5,
    );
    // 1번 획만 정착.
    let out = st.sweep(|s| if s.id.0 == 1 { u64::MAX } else { 42 }, 100);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].id.0, 1);
    assert_eq!(st.young.len(), 1);
    assert_eq!(st.young[0].id.0, 2);
    assert_eq!(st.settled, 1);
    // 다음 sweep에서 나머지도 정착.
    let out = st.sweep(settled_all, 200);
    assert_eq!(out.len(), 1);
    assert_eq!(st.young.len(), 0);
    assert_eq!(st.settled, 2);
}

#[test]
fn deleted_detects_shrink_only() {
    let mut st = InkSettling::new();
    st.add(0, vec![stroke(1, freedf_core::model::ToolType::Pen)], 5);
    // rev 변경 + 개수 증가는 삭제 아님.
    assert!(!st.deleted(0, 2, 6));
    // 개수 같으면 삭제로 간주 (id 교체 가능성).
    assert!(st.deleted(0, 1, 6));
    // 개수 감소도 삭제.
    assert!(st.deleted(0, 0, 6));
    // 다른 페이지는 무관.
    assert!(!st.deleted(1, 0, 6));
}

#[test]
fn resync_recomputes_from_store() {
    let mut st = InkSettling::new();
    st.add(0, vec![stroke(1, freedf_core::model::ToolType::Pen)], 5);
    st.resync(0, 4, 9, vec![stroke(4, freedf_core::model::ToolType::Pen)]);
    assert_eq!(st.settled, 3, "4획 중 젊은 1획 → 정착 3");
    assert_eq!(st.rev.0, 9);
    assert_eq!(st.page, Some(0));
}

#[test]
fn reset_forgets_everything() {
    let mut st = InkSettling::new();
    st.add(0, vec![stroke(1, freedf_core::model::ToolType::Pen)], 5);
    st.reset();
    assert!(st.young.is_empty());
    assert_eq!(st.settled, 0);
    assert_eq!(st.rev.0, u64::MAX);
    assert_eq!(st.page, None);
}

#[test]
fn pacing_trades_more_work_for_smoothness_as_hz_rises() {
    let mut prev = ink_pacing_for(60);
    // 60Hz = 기존 동작과 동일한 기준선.
    assert_eq!(prev.active_geom_ms, 16);
    assert_eq!(prev.soak_scale, 1.0);
    for hz in [120, 144, 240] {
        let p = ink_pacing_for(hz);
        assert!(
            p.active_geom_ms < prev.active_geom_ms,
            "{hz}Hz는 재구성 주기가 더 짧아야 함"
        );
        assert!(
            p.soak_scale > prev.soak_scale,
            "{hz}Hz는 스밈 그라데이션이 더 촘촘해야 함"
        );
        prev = p;
    }
}

#[test]
fn snap_refresh_returns_nearest_preset() {
    assert_eq!(snap_refresh_hz(0), 60);
    assert_eq!(snap_refresh_hz(61), 60);
    assert_eq!(snap_refresh_hz(100), 120);
    assert_eq!(snap_refresh_hz(130), 120);
    assert_eq!(snap_refresh_hz(135), 144);
    assert_eq!(snap_refresh_hz(999), 240);
}
