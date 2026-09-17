//! `input_controlmap` 모듈 단위 테스트 — `src/input_controlmap.rs`의 `#[cfg(test)]` 모듈에서 이동했습니다.

use freedf_core::input_controlmap::*;
use freedf_core::input_events::{
    ActionMode, ControlEvent, ControlKind, ControlPhase, InputEvent,
};

fn ev(control: ControlKind, index: u8, phase: ControlPhase) -> ControlEvent {
    ControlEvent {
        control,
        index,
        phase,
    }
}

#[test]
fn default_button1_is_tap_color_wheel() {
    let map = ControlMap::with_defaults();
    let a = map.translate(&ev(ControlKind::StylusButton, 1, ControlPhase::Down));
    match a {
        Some(InputEvent::Action(act)) => {
            assert_eq!(act.key, "color-wheel");
            assert_eq!(act.mode, Some(ActionMode::Tap));
        }
        other => panic!("action이 나와야 한다: {other:?}"),
    }
    // 탭은 Up에서 발화하지 않는다.
    assert!(map
        .translate(&ev(ControlKind::StylusButton, 1, ControlPhase::Up))
        .is_none());
    // 미바인딩은 조용히 무시.
    assert!(map
        .translate(&ev(ControlKind::StylusButton, 2, ControlPhase::Down))
        .is_none());
}

#[test]
fn hold_binding_speaks_both_edges() {
    let mut map = ControlMap::default();
    map.set(
        ControlKind::StylusButton,
        2,
        Some(ControlBinding {
            action: "tool:eraser".into(),
            mode: BindingMode::Hold,
        }),
    );
    let on = map.translate(&ev(ControlKind::StylusButton, 2, ControlPhase::Down));
    let off = map.translate(&ev(ControlKind::StylusButton, 2, ControlPhase::Up));
    assert!(matches!(
        on,
        Some(InputEvent::Action(ref a)) if a.mode == Some(ActionMode::HoldOn)
    ));
    assert!(matches!(
        off,
        Some(InputEvent::Action(ref a)) if a.mode == Some(ActionMode::HoldOff)
    ));
}

#[test]
fn control_key_is_stable_per_identity_pair() {
    assert_eq!(control_key(ControlKind::StylusButton, 1), "stylus-button:1");
    assert_eq!(control_key(ControlKind::ExpressKey, 12), "express-key:12");
}
