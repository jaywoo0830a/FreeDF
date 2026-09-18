//! `input_hub` 모듈 단위 테스트 — `src/input_hub.rs`의 `#[cfg(test)]` 모듈에서 이동했습니다.

use freedf_core::input_events::{ActionSource, ControlKind, ControlPhase, NO_TILT};
use freedf_core::input_events::{InputEvent, PointerPhase, PointerSource};
use freedf_core::input_hub::*;

fn pointer(source: PointerSource, phase: PointerPhase) -> InputEvent {
    InputEvent::pointer(source, phase, [1.0, 2.0], 0.5, NO_TILT)
}

#[test]
fn one_pointer_at_a_time_other_source_is_dropped() {
    let mut hub = Hub::new();
    assert!(hub.emit(pointer(PointerSource::Pen, PointerPhase::Down)));
    // 펜이 점유 중 — 마우스 Down/Up은 drop (엉뚱한 Up이 점유를 풀지도 못한다).
    assert!(!hub.emit(pointer(PointerSource::Mouse, PointerPhase::Down)));
    assert!(!hub.emit(pointer(PointerSource::Mouse, PointerPhase::Up)));
    assert_eq!(hub.dropped(), 2, "drop은 카운터로 관측된다 (진단 계약 4.3)");
    assert_eq!(hub.pending(), 1);
    hub.take(|_| {});
    // 펜이 여전히 점유 중 (Up이 없음) — 큐를 비워도 규칙은 유지된다.
    assert!(!hub.emit(pointer(PointerSource::Mouse, PointerPhase::Down)));
    // 펜 Up으로 점유 해제 — 이제 마우스가 들어온다.
    assert!(hub.emit(pointer(PointerSource::Pen, PointerPhase::Up)));
    hub.take(|_| {});
    assert!(hub.emit(pointer(PointerSource::Mouse, PointerPhase::Down)));
}

#[test]
fn same_source_drag_flows_and_up_releases() {
    let mut hub = Hub::new();
    assert!(hub.emit(pointer(PointerSource::Pen, PointerPhase::Down)));
    assert!(hub.emit(pointer(PointerSource::Pen, PointerPhase::Drag)));
    assert!(hub.emit(pointer(PointerSource::Pen, PointerPhase::Up)));
    assert!(hub.emit(pointer(PointerSource::Mouse, PointerPhase::Down)));
    assert_eq!(hub.pending(), 4);
}

#[test]
fn order_is_preserved_and_actions_controls_pass_through() {
    let mut hub = Hub::new();
    hub.emit(pointer(PointerSource::Pen, PointerPhase::Down));
    hub.emit(InputEvent::action(ActionSource::Keyboard, "tool:pen", None));
    hub.emit(InputEvent::control(
        ControlKind::StylusButton,
        1,
        ControlPhase::Down,
    ));
    hub.emit(pointer(PointerSource::Pen, PointerPhase::Up));
    let mut kinds = Vec::new();
    hub.take(|ev| kinds.push(ev.kind()));
    assert_eq!(kinds.len(), 4);
}

#[test]
fn replay_feeds_script_without_hardware() {
    let mut hub = Hub::new();
    replay(
        &mut hub,
        [
            pointer(PointerSource::Pen, PointerPhase::Down),
            pointer(PointerSource::Pen, PointerPhase::Up),
        ],
    );
    assert_eq!(hub.pending(), 2);
}
