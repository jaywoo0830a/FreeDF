//! `clock` 모듈 단위 테스트 — `src/clock.rs`의 `#[cfg(test)]` 모듈에서 이동했습니다.

use freedf_canvas::clock::*;

/// 계약: FakeClock은 테스트가 시간을 완전히 통제하게 합니다.
#[test]
fn fake_clock_is_fully_controllable() {
    let clock = FakeClock::new(1_000);
    assert_eq!(clock.now_ms(), 1_000);
    clock.advance(250);
    assert_eq!(clock.now_ms(), 1_250);
    clock.set(42);
    assert_eq!(clock.now_ms(), 42);
}
