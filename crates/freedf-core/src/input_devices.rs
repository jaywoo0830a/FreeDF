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
///
/// **장치 상태의 소유자**이기도 하다: 틸트 벡터를 조건화(노이즈 필터)해 보관하고
/// [`Self::tilt`]로 노출한다 — 렌더/모델은 조건화된 값을 받고, 캔버스가 장치
/// 노이즈 사정을 알 필요가 없다 (레이어링: 장치 지식은 이 경계에서 끝난다).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PenEventAdapter {
    prev_buttons: crate::pen_input::PenButtons,
    prev_contact: bool,
    /// 조건화된 틸트 벡터 (도, ±90) — [`smooth_tilt`] 참조.
    tilt: [f32; 2],
}

/// 틸트 노이즈 필터 — 패드 진입 시(호버 시작) 격렬하게 떨리는 틸트 리포트를
/// 무시한다. 리포트당 최대 변화를 제한하고 EMA로 부드럽게 수렴시킨다.
///
/// 종전에는 캔버스(앱)가 이 필터를 들고 `pen_tilt`를 직접 갱신했다 — 장치
/// 노이즈라는 장치 사정이 앱 경계로 새어 나온 땜질이었다. 이제 어댑터가
/// 소유한다 (어댑터는 위치/압력 기본값 채움(능력 협상)과 같은 자리다).
pub fn smooth_tilt(prev: [f32; 2], next: [f32; 2]) -> [f32; 2] {
    const MAX_STEP: f32 = 24.0; // 한 리포트당 최대 변화(도) — 이보다 큰 점프는 잘라냄.
    const ALPHA: f32 = 0.3; // EMA 계수.
    let mut out = prev;
    for i in 0..2 {
        let step = (next[i] - prev[i]).clamp(-MAX_STEP, MAX_STEP);
        out[i] = prev[i] + step * ALPHA;
    }
    out
}

impl PenEventAdapter {
    /// 조건화된 틸트 벡터 (도, ±90) — 렌더/모델의 틸트 원천.
    pub fn tilt(&self) -> [f32; 2] {
        self.tilt
    }
    /// 펜 스트림 스냅샷 1건 → 통합 이벤트들.
    ///
    /// `point`: 이 순간의 커서 위치(경계 좌표). 펜 스트림 자체는 위치가 없고
    /// 위치는 OS 포인터가 따라오므로 호출자(앱)가 공급한다. `None`이면
    /// 위치가 필요한 포인터 이벤트는 만들지 않는다(컨트롤 에지는 위치와
    /// 무관하므로 계속 나온다).
    pub fn update(&mut self, st: &PenState, point: Option<[f32; 2]>) -> Vec<InputEvent> {
        let mut out = Vec::new();

        // 틸트 조건화 — 장치 노이즈 필터는 여기(장치 경계)서 끝난다. 이벤트가
        // 나르는 틸트와 렌더/모델이 보는 틸트가 같은 값이 된다 (조건화된 벡터).
        self.tilt = smooth_tilt(self.tilt, st.tilt);

        // 능력 협상의 끝 — 압력/기울기는 여기서 "항상 존재"하게 된다.
        let pressure = st.pressure.unwrap_or(1.0);
        let tilt = self.tilt[0].hypot(self.tilt[1]);

        // ── 에지 보존 (0916 계약 #4) ─────────────────────────────────────
        // 접촉 에지가 감지됐는데 이 패킷에 위치가 없으면, 에지를 **파괴하지
        // 않고** 다음 패킷으로 미룬다 — `prev_contact` 를 갱신하지 않는다.
        // evdev는 egui보다 빠른 시계라 위치 조회가 에지보다 늦을 수 있는데,
        // 여기서 에지를 소비해 버리면 위로 보낼 Down 자체가 사라진다 (0916
        // 유실의 상류 원인). 원칙: **위치 없는 Down 은 라우터에 도달하지
        // 않는다** — 에지는 위치가 확인된 첫 패킷에서 온전히 태어난다.
        let contact_edge = st.contact != self.prev_contact;
        if contact_edge && point.is_none() {
            // 에지 보류 — 접촉 상태만 "아직 미확정"으로 둔다. 버튼 에지는
            // 위치와 무관하므로 아래에서 계속 처리한다.
        } else {
            if contact_edge {
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
            self.prev_contact = st.contact;
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
        // prev_contact 갱신은 위 에지 보존 블록이 소유한다 — 위치 없는 에지는
        // "미확정" 상태로 남아 다음 패킷으로 미뤄진다.
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
        // 능력 협상: 압력/기울기가 어댑터에서 채워진다.
        assert_eq!(p.pressure, 0.7);
        // 틸트는 **조건화된 벡터**의 크기다 (raw [3,4] → EMA 0.3 → [0.9,1.2]).
        // 장치 노이즈 필터가 경계 안에 있으므로 첫 리포트는 raw보다 작다.
        let expect = (0.9f32 * 0.9 + 1.2 * 1.2).sqrt();
        assert!((p.tilt - expect).abs() < 1e-5, "tilt={} expect={expect}", p.tilt);
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
        // 첫 리포트: [3,4] → EMA 0.3 → [0.9, 1.2], 크기 = hypot.
        let t = a.tilt();
        assert!((t[0] - 0.9).abs() < 1e-5 && (t[1] - 1.2).abs() < 1e-5);
        assert!((p.tilt - (t[0] * t[0] + t[1] * t[1]).sqrt()).abs() < 1e-5);
        // 급격한 점프는 잘린다 (조건화가 어댑터 안에 있다).
        let mut wild = st(true, false, false);
        wild.tilt = [90.0, -90.0];
        a.update(&wild, Some([2.0, 2.0]));
        let t1 = a.tilt();
        assert!((t1[0] - t[0]).abs() <= 7.2 + 1e-3 && (t1[1] - t[1]).abs() <= 7.2 + 1e-3);
    }
}
