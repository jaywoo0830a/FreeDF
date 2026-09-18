//! `canvas_port` 모듈 단위 테스트 — `src/canvas_port.rs`의 `#[cfg(test)]` 모듈에서 이동했습니다.

use freedf_canvas::canvas_port::*;

fn head() -> StrokeHead {
    StrokeHead {
        tool: "pen".into(),
        point: [1.0, 2.0],
        pressure: 0.5,
    }
}

/// 계약: 라이브 세션은 begin → tail* → end(+commit) 순서다.
#[test]
fn live_session_records_in_contract_order() {
    let mut s = CanvasRecorder::new();
    assert_eq!(s.capabilities(), &[CAP_CORE]);
    s.begin_live(1, &head());
    s.draw_live_tail(
        1,
        &[LivePoint {
            pos: [3.0, 2.0],
            pressure: 0.6,
        }],
    );
    s.end_live(1, &Committed { tool: "pen".into() });
    assert_eq!(s.op_kinds(), vec!["beginLive", "drawLiveTail", "endLive"]);
    // tail은 새 점만 — O(Δ) 계약.
    assert_eq!(s.ops[1], CanvasOp::DrawLiveTail { id: 1, points: 1 });
}

/// 계약: invalidate는 영역을 데이터로 지시한다 (지우개=원, undo=페이지).
#[test]
fn invalidate_carries_region_data() {
    let mut s = CanvasRecorder::new();
    s.invalidate(Region::Circle {
        center: [5.0, 5.0],
        radius: 8.0,
    });
    s.invalidate(Region::Page);
    assert_eq!(
        s.ops,
        vec![
            CanvasOp::Invalidate {
                region: Region::Circle {
                    center: [5.0, 5.0],
                    radius: 8.0
                }
            },
            CanvasOp::Invalidate {
                region: Region::Page
            },
        ]
    );
}

/// 계약: 'overlay' 능력은 선언한 구현에만 존재한다.
#[test]
fn overlay_capability_is_declared_not_inherited() {
    let core = CanvasRecorder::new();
    assert!(!core.has_capability(CAP_OVERLAY));
    let mut ov = CanvasRecorder::with_overlay();
    assert!(ov.has_capability(CAP_OVERLAY));
    ov.overlay(&OverlayShape::Circle {
        center: [0.0, 0.0],
        radius: 3.0,
        color: [0, 0, 0, 255],
    });
    assert_eq!(ov.op_kinds(), vec!["overlay"]);
}
