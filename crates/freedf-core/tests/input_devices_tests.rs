//! `input_devices` 모듈 단위 테스트 — `src/input_devices.rs`의 `#[cfg(test)]` 모듈에서 이동했습니다.

use freedf_core::input_devices::*;
use freedf_core::input_events::{
    ControlKind, ControlPhase, InputEvent, PointerEvent, PointerPhase, PointerSource,
};
use freedf_core::pen_input::{PenButtons, PenState};

fn st(contact: bool, b1: bool, b2: bool) -> PenState {
    PenState {
        tilt: [3.0, 4.0],
        pressure: Some(0.7),
        contact,
        buttons: PenButtons { button1: b1, button2: b2 },
    }
}

fn pen(ev: &InputEvent) -> &PointerEvent {
    match ev {
        InputEvent::Pointer(p) => p,
        _ => panic!("포인터 이벤트 아님: {ev:?}"),
    }
}

#[test]
fn contact_edges_become_down_and_up() {
    let mut a = PenEventAdapter::default();
    let evs = a.update(&st(true, false, false), Some([1.0, 1.0]));
    assert_eq!(evs.len(), 1);
    let p = pen(&evs[0]);
    assert_eq!(p.phase, PointerPhase::Down);
    assert_eq!(p.source, PointerSource::Pen);
    // 능력 협상: 압력/기울기가 어댑터에서 채워진다.
    assert_eq!(p.pressure, 0.7);
    // 틸트는 **조건화된 벡터**다 (raw [3,4] → EMA 0.3 → [0.9,1.2]) —
    // 장치 노이즈 필터가 경계 안에 있으므로 첫 리포트는 raw보다 작다.
    assert!((p.tilt[0] - 3.0 * 0.3).abs() < 1e-6, "이벤트가 조건화된 틸트 벡터를 나른다");
    assert!((p.tilt[1] - 1.2).abs() < 1e-6);
    let expect = (0.9f32 * 0.9 + 1.2 * 1.2).sqrt();
    assert!((p.tilt_magnitude() - expect).abs() < 1e-5, "크기는 파생값");
    let evs = a.update(&st(false, false, false), Some([1.0, 1.0]));
    assert_eq!(pen(&evs[0]).phase, PointerPhase::Up);
}

#[test]
fn sustained_contact_is_drag_and_pressure_default_is_filled() {
    let mut a = PenEventAdapter::default();
    let _ = a.update(&st(true, false, false), Some([1.0, 1.0])); // down
    let mut hover = st(true, false, false);
    hover.pressure = None; // 압력 미보고 장치
    let evs = a.update(&hover, Some([2.0, 2.0]));
    let p = pen(&evs[0]);
    assert_eq!(p.phase, PointerPhase::Drag);
    assert_eq!(p.pressure, 1.0); // 어댑터 기본값 — 툴은 fallback을 모른다
}

#[test]
fn button_edges_become_control_events_independent_of_position() {
    let mut a = PenEventAdapter::default();
    // 위치 없이도 컨트롤 에지는 나온다 (버튼은 위치와 무관).
    let evs = a.update(&st(false, true, false), None);
    assert_eq!(
        evs,
        vec![InputEvent::control(
            ControlKind::StylusButton,
            1,
            ControlPhase::Down
        )]
    );
    let evs = a.update(&st(false, false, true), None);
    assert_eq!(
        evs,
        vec![
            InputEvent::control(ControlKind::StylusButton, 1, ControlPhase::Up),
            InputEvent::control(ControlKind::StylusButton, 2, ControlPhase::Down),
        ]
    );
}

/// 0916 계약 #4 — 위치 없는 접촉 에지는 파괴되지 않고 다음 패킷으로
/// 미뤄진다. evdev(빠른 시계)가 egui(느린 시계)보다 먼저 접촉을 봐도,
/// 위로 보낼 Down 이 사라지지 않는다 (라우터 마이그레이션의 상류 전제).
#[test]
fn positionless_contact_edge_is_deferred_not_destroyed() {
    let mut a = PenEventAdapter::default();
    // 에지 프레임에 위치가 없다 — 이벤트는 안 만들지만 에지를 소비하지도 않는다.
    let evs = a.update(&st(true, false, false), None);
    assert!(evs.is_empty(), "위치 없는 에지는 이벤트로 나오지 않는다");
    // 다음 패킷에 위치 도착 — **Down** 이 살아난다 (Drag 로 훼손되지 않는다).
    let evs = a.update(&st(true, false, false), Some([7.0, 9.0]));
    assert_eq!(evs.len(), 1);
    let p = pen(&evs[0]);
    assert_eq!(p.phase, PointerPhase::Down, "에지 보존 — 온전한 Down");
    assert_eq!(p.point, [7.0, 9.0], "첫 점은 실제 위치다");
}

#[test]
fn positionless_up_edge_is_deferred_too() {
    let mut a = PenEventAdapter::default();
    let _ = a.update(&st(true, false, false), Some([1.0, 1.0])); // down
    // Up 에지에 위치가 없다 — 다음 패킷으로 미뤄진다.
    let evs = a.update(&st(false, false, false), None);
    assert!(evs.is_empty());
    let evs = a.update(&st(false, false, false), Some([1.0, 1.0]));
    assert_eq!(pen(&evs[0]).phase, PointerPhase::Up);
    // 미뤄진 뒤 접촉이 재개돼도 유령 Down 이 생기지 않는다 (prev_contact 미갱신).
    let evs = a.update(&st(true, false, false), Some([2.0, 2.0]));
    assert_eq!(pen(&evs[0]).phase, PointerPhase::Down);
}

/// 틸트 조건화는 **장치 경계**의 책임 — 캔버스(앱)에서 옮겨온 테스트.
#[test]
fn smooth_tilt_rejects_violent_jumps() {
    // 패드 진입 시 ±90° 스파이크가 연달아 와도 한 걸음이 24°×0.3 = 7.2°를
    // 넘지 않고, 같은 값이 계속되면 서서히 수렴합니다.
    let mut t = [0.0f32, 0.0];
    for _ in 0..8 {
        let prev = t;
        t = smooth_tilt(t, [90.0, -90.0]);
        assert!((t[0] - prev[0]).abs() <= 7.2 + 1e-3, "급격 점프 제한");
        assert!((t[1] - prev[1]).abs() <= 7.2 + 1e-3);
    }
    assert!(t[0] > 40.0 && t[1] < -40.0, "결국 목표로 수렴");
    // 상수 입력에는 정확히 수렴.
    let mut t2 = [10.0f32, -10.0];
    for _ in 0..50 {
        t2 = smooth_tilt(t2, [20.0, 5.0]);
    }
    assert!((t2[0] - 20.0).abs() < 0.5 && (t2[1] - 5.0).abs() < 0.5);
}

/// 어댑터가 조건화된 틸트를 보관/노출한다 — 이벤트가 나르는 틸트와
/// 렌더가 보는 틸트가 같은 값이 된다 (앱에서 필터를 들고 있을 이유가 없다).
#[test]
fn adapter_exposes_conditioned_tilt_vector() {
    let mut a = PenEventAdapter::default();
    let evs = a.update(&st(true, false, false), Some([1.0, 1.0]));
    let p = pen(&evs[0]);
    // 첫 리포트: [3,4] → EMA 0.3 → [0.9, 1.2] — 벡터가 **그대로** 실린다.
    let t = a.tilt();
    assert!((t[0] - 0.9).abs() < 1e-5 && (t[1] - 1.2).abs() < 1e-5);
    assert_eq!(p.tilt, t, "이벤트가 나르는 틸트 = 어댑터가 보관한 벡터");
    // 크기/방위각은 어휘의 파생값이다 (별도 필드 없음).
    assert!((p.tilt_magnitude() - (t[0] * t[0] + t[1] * t[1]).sqrt()).abs() < 1e-5);
    let (az, cos_pitch) = p.tilt_azimuth();
    assert!((az - (t[1]).atan2(t[0])).abs() < 1e-6, "방향이 보존된다");
    assert!((cos_pitch - 1.0).abs() < 0.02, "작은 기울기 = 거의 수직");
    // 급격한 점프는 잘린다 (조건화가 어댑터 안에 있다).
    let mut wild = st(true, false, false);
    wild.tilt = [90.0, -90.0];
    a.update(&wild, Some([2.0, 2.0]));
    let t1 = a.tilt();
    assert!((t1[0] - t[0]).abs() <= 7.2 + 1e-3 && (t1[1] - t[1]).abs() <= 7.2 + 1e-3);
}

/// 능력 협상 (C3) — "틸트를 보고하는 장치인가"는 **장치가 대답한다**.
/// 스트림 존재로 근사하지 않는다 (압력만 보고하는 펜도 스트림은 있다).
#[test]
fn capability_negotiation_reports_tilt_support() {
    use freedf_core::pen_input::PenCapabilities;
    // 모르면 낙관(보고하면 쓴다) — 구 동작 보존.
    assert!(PenEventAdapter::default().tilt_supported());
    // 열거가 "압력만"이라고 알려준 장치.
    let a = PenEventAdapter::with_capabilities(PenCapabilities {
        has_tilt: false,
        has_pressure: true,
    });
    assert!(!a.tilt_supported(), "능력 질의는 스트림 존재와 다른 질문이다");
    // 외부 훅(HID/WM_POINTER)의 틸트 주입도 장치 상태의 소유자를 거친다.
    let mut b = PenEventAdapter::default();
    b.set_tilt([200.0, -200.0]);
    assert_eq!(b.tilt(), [90.0, -90.0], "장치 어휘 범위로 클램프");
}
