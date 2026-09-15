//! 장치 어댑터 — 장치 축 (계약 객체 ⑤ — ideation `idea4/devices.js`의 이식).
//!
//! raw 하드웨어 상태를 통합 어휘([`crate::input_events`])로 번역한다. 툴/
//! 워크스페이스를 몰라야 한다(레이어링 규칙 — 의존은 events 잎 하나뿐).
//! 능력 협상도 여기서 끝난다: 압력을 보고하지 않는 장치는 어댑터가 기본값을
//! 채우고, 툴은 fallback을 모른다.
//!
//! push는 이 경계에서만 일어난다 — 어댑터가 허브에 밀어 넣은 뒤부터는
//! 소비자(툴)가 당겨 간다. 에지(상태 변화) 감지도 어댑터 소유다: 하드웨어는
//! "지금 눌려 있는가"를 말할 뿐, "눌렸다/뗐다"는 이 경계의 해석이다.

use crate::input_events::{
    ControlKind, ControlPhase, InputEvent, PointerPhase, PointerSource,
};
use crate::pen_input::PenState;

/// 펜 스트림(evdev/OTD) → 통합 어휘 어댑터.
///
/// `pen_input::PenMonitor::poll()`의 스냅샷을 순서대로 먹여 스트로크 위상
/// (접촉 에지)과 사이드 버튼 에지를 이벤트로 뽑아내는 순수 상태기계다.
/// 하드웨어가 없어도 테스트할 수 있다.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PenEventAdapter {
    prev_buttons: crate::pen_input::PenButtons,
    prev_contact: bool,
}

impl PenEventAdapter {
    /// 펜 스트림 스냅샷 1건 → 통합 이벤트들.
    ///
    /// `point`: 이 순간의 커서 위치(경계 좌표). 펜 스트림 자체는 위치가 없고
    /// 위치는 OS 포인터가 따라오므로 호출자(앱)가 공급한다. `None`이면
    /// 위치가 필요한 포인터 이벤트는 만들지 않는다(컨트롤 에지는 위치와
    /// 무관하므로 계속 나온다).
    pub fn update(&mut self, st: &PenState, point: Option<[f32; 2]>) -> Vec<InputEvent> {
        let mut out = Vec::new();

        // 능력 협상의 끝 — 압력/기울기는 여기서 "항상 존재"하게 된다.
        let pressure = st.pressure.unwrap_or(1.0);
        let tilt = st.tilt[0].hypot(st.tilt[1]);

        if st.contact != self.prev_contact {
            let phase = if st.contact {
                PointerPhase::Down
            } else {
                PointerPhase::Up
            };
            if let Some(p) = point {
                out.push(InputEvent::pointer(
                    PointerSource::Pen,
                    phase,
                    p,
                    pressure,
                    tilt,
                ));
            }
        } else if st.contact {
            // 접촉 유지 = 드래그. 위치를 알 때만 (패킷 1건 = 포인터 1건).
            if let Some(p) = point {
                out.push(InputEvent::pointer(
                    PointerSource::Pen,
                    PointerPhase::Drag,
                    p,
                    pressure,
                    tilt,
                ));
            }
        }

        // 사이드 버튼 에지 — (종류, 번호) 쌍의 원시 컨트롤. 번역(사용자 매핑 →
        // action)은 컨트롤 맵 계층이 하고, 어댑터는 개수를 몰라도 된다.
        for (index, (now, prev)) in [
            (st.buttons.button1, self.prev_buttons.button1),
            (st.buttons.button2, self.prev_buttons.button2),
        ]
        .into_iter()
        .enumerate()
        {
            let index = index as u8 + 1;
            if now && !prev {
                out.push(InputEvent::control(
                    ControlKind::StylusButton,
                    index,
                    ControlPhase::Down,
                ));
            } else if !now && prev {
                out.push(InputEvent::control(
                    ControlKind::StylusButton,
                    index,
                    ControlPhase::Up,
                ));
            }
        }

        self.prev_buttons = st.buttons;
        self.prev_contact = st.contact;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input_events::{PointerEvent, PointerSource};
    use crate::pen_input::PenButtons;

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
        // 능력 협상: 압력/기울기가 어댑터에서 채워진다 (기기 값 그대로).
        assert_eq!(p.pressure, 0.7);
        assert_eq!(p.tilt, 5.0); // hypot(3, 4)
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
}
