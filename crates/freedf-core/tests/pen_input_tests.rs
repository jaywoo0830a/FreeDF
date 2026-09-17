//! `pen_input` 모듈 단위 테스트 — `src/pen_input.rs`의 `#[cfg(test)]` 모듈에서 이동했습니다.

use freedf_core::pen_input::*;

#[test]
fn list_devices_returns_without_crashing() {
    // 장치가 있든 없든 패닉 없이 반환해야 합니다.
    let _ = list_devices();
}

#[test]
fn pen_state_default_is_neutral() {
    let s = PenState::default();
    assert_eq!(s.tilt, [0.0, 0.0]);
    assert!(s.pressure.is_none());
    assert!(!s.contact);
    assert!(!s.buttons.button1);
    assert!(!s.buttons.button2);
}
