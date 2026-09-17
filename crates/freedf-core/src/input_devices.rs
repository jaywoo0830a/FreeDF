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
    /// 장치 능력 (능력 협상) — "틸트를 보고하는 장치인가"를 **묻는** 창구.
    /// 렌더가 스트림 존재로 근사하던 판정(C3)의 대체물이다.
    caps: crate::pen_input::PenCapabilities,
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
    /// 장치 능력을 선언한 어댑터 — 능력 협상의 시작점 (장치 열거가 아는 사실).
    pub fn with_capabilities(caps: crate::pen_input::PenCapabilities) -> Self {
        Self {
            caps,
            ..Self::default()
        }
    }

    /// 장치 능력 선언 (열거 이후에 알게 됐을 때).
    pub fn set_capabilities(&mut self, caps: crate::pen_input::PenCapabilities) {
        self.caps = caps;
    }

    /// 이 장치가 틸트를 보고하는가 — 렌더/모델의 **능력 질의**.
    /// (스트림이 존재하는가와는 다른 질문이다: 압력만 보고하는 펜도 스트림은 있다.)
    pub fn tilt_supported(&self) -> bool {
        self.caps.has_tilt
    }

    /// 외부 훅(HID/WM_POINTER)이 틸트를 직접 주입합니다 — 장치 상태의 소유자는
    /// 어댑터이므로, 앱이 별도 벡터를 들고 다니지 않게 하는 입구다.
    pub fn set_tilt(&mut self, tilt: [f32; 2]) {
        self.tilt = [tilt[0].clamp(-90.0, 90.0), tilt[1].clamp(-90.0, 90.0)];
    }

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
        // 틸트는 **벡터 그대로** 나른다: 방향(방위각)이 어휘에 실려야 소비자가
        // 별도 벡터를 들고 다니지 않는다 (C2).
        let pressure = st.pressure.unwrap_or(1.0);
        let tilt = self.tilt;

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
