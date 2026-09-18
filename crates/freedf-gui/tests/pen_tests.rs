//! `pen` 모듈 테스트 — **코어 공급원 → gui 어댑터 배선**만 검증한다.
//!
//! 틸트 노이즈 필터/능력 협상 자체는 `freedf-core`가 보증한다(코어 테스트가
//! 소유). 여기서 보는 것은 gui가 고른 공급원, 파생값, 클램프, 상태 표시뿐이다.

use freedf_core::pen_input::{PenCapabilities, PenState};
use freedf_gui::canvas::now_ms;
use freedf_gui::pen::{PenInput, PenSource};

/// 장치 리포트 하나 (테스트 편의).
fn report(pressure: Option<f32>, tilt: [f32; 2], contact: bool) -> PenState {
    PenState {
        tilt,
        pressure,
        contact,
        ..Default::default()
    }
}

#[test]
fn polls_latest_report_and_conditions_tilt() {
    let (tx, rx) = std::sync::mpsc::channel();
    let mut pen = PenInput::from_receiver(rx, PenCapabilities::UNKNOWN, PenSource::Otd);
    assert!(!pen.poll(), "리포트가 없으면 false");
    tx.send(report(Some(0.4), [45.0, 0.0], true)).unwrap();
    tx.send(report(Some(0.6), [45.0, 0.0], true)).unwrap();
    assert!(pen.poll(), "새 리포트가 있으면 true");
    // 누적된 리포트 중 **마지막** 스냅샷이 반영된다.
    assert_eq!(pen.state().pressure, Some(0.6));
    assert!(pen.contact());
    // 틸트는 코어 어댑터의 노이즈 필터를 통과한 값
    // (리포트당 최대 24도 → EMA 0.3 = 7.2).
    let tilt = pen.tilt();
    assert!((tilt[0] - 7.2).abs() < 1e-4, "조건화된 틸트: {tilt:?}");
    assert!(pen.tilt_unit() > 0.0 && pen.tilt_unit() < 1.0);
}

#[test]
fn pressure_follows_setting_and_clamps_to_pipeline_contract() {
    let (tx, rx) = std::sync::mpsc::channel();
    let mut pen = PenInput::from_receiver(rx, PenCapabilities::UNKNOWN, PenSource::Otd);
    tx.send(report(Some(0.35), [0.0, 0.0], true)).unwrap();
    pen.poll();
    assert!((pen.pressure(true) - 0.35).abs() < 1e-6);
    assert_eq!(pen.pressure(false), 1.0, "설정 꺼짐 = 명목 1.0");
    // 장치가 범위를 벗어난 값을 보고해도 `InkPipeline` 계약(0..1)을 지킨다.
    tx.send(report(Some(1.8), [0.0, 0.0], true)).unwrap();
    pen.poll();
    assert_eq!(pen.pressure(true), 1.0);
    tx.send(report(Some(-0.5), [0.0, 0.0], true)).unwrap();
    pen.poll();
    assert_eq!(pen.pressure(true), 0.0);
    // 압력을 보고하지 않는 장치(마우스/단순 펜)는 명목 1.0.
    tx.send(report(None, [0.0, 0.0], true)).unwrap();
    pen.poll();
    assert_eq!(pen.pressure(true), 1.0);
}

#[test]
fn capability_query_gates_cursor_azimuth() {
    // 틸트 미지원 장치 — 방위각 없음(커서는 손잡이 기본값으로 폴백한다).
    let (_tx, rx) = std::sync::mpsc::channel();
    let pen = PenInput::from_receiver(rx, PenCapabilities::NONE, PenSource::Evdev);
    assert!(!pen.tilt_supported());
    assert_eq!(pen.cursor_azimuth(), None);
    // 압력만 보고하는 장치에서도 능력 질의는 정직하다 (스트림 존재 ≠ 틸트 지원).
    let (_tx2, rx2) = std::sync::mpsc::channel();
    let pen2 = PenInput::from_receiver(rx2, PenCapabilities::UNKNOWN, PenSource::Otd);
    assert!(pen2.tilt_supported());
    assert!(pen2.cursor_azimuth().is_some());
}

#[test]
fn proximity_reads_contact_then_fades_after_reports_stop() {
    let (tx, rx) = std::sync::mpsc::channel();
    let mut pen = PenInput::from_receiver(rx, PenCapabilities::UNKNOWN, PenSource::Otd);
    tx.send(report(Some(1.0), [0.0, 0.0], true)).unwrap();
    pen.poll();
    assert_eq!(pen.proximity(now_ms()), 1.0, "접촉 = 최대 근접감");
    // 호버(리포트 살아 있음) → 최대, 리포트가 끊기면 900ms에 걸쳐 0으로 사라진다.
    tx.send(report(Some(0.0), [0.0, 0.0], false)).unwrap();
    pen.poll();
    let now = now_ms();
    assert_eq!(pen.proximity(now), 1.0, "방금 리포트 = 최대 근접감");
    let mid = pen.proximity(now + 1000);
    assert!(mid > 0.0 && mid < 1.0, "감쇠 중간값: {mid}");
    assert_eq!(pen.proximity(now + 1300), 0.0, "리포트가 끊기면 사라진다");
    // 스트림이 아예 없으면 리포트 나이가 없으므로 0.
    assert_eq!(PenInput::silent().proximity(now), 0.0);
}

#[test]
fn silent_stream_reports_nominal_values() {
    let pen = PenInput::silent();
    assert_eq!(pen.source(), PenSource::None);
    assert_eq!(pen.source().label(), "none");
    assert_eq!(pen.pressure(true), 1.0);
    assert!(!pen.tilt_supported());
    assert_eq!(pen.tilt(), [0.0, 0.0]);
    assert_eq!(pen.tilt_magnitude(), 0.0);
    assert!(pen.age_ms(now_ms()).is_none());
}

#[test]
fn buttons_and_source_labels_are_exposed() {
    let (tx, rx) = std::sync::mpsc::channel();
    let mut pen = PenInput::from_receiver(rx, PenCapabilities::UNKNOWN, PenSource::Evdev);
    let mut state = report(Some(0.5), [0.0, 0.0], false);
    state.buttons.button1 = true;
    tx.send(state).unwrap();
    pen.poll();
    assert!(pen.buttons().button1);
    assert_eq!(pen.source().label(), "evdev");
    assert_eq!(PenSource::Otd.label(), "OTD");
}
