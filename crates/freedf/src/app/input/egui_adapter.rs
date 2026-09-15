//! egui 쪽 장치 어댑터 — egui 이벤트를 통합 어휘로 번역한다.
//!
//! ideation `idea4/devices.js`의 `attachMouse`/터치 어댑터에 해당한다.
//! 능력 협상이 여기서 끝난다: egui 포인터에는 압력/기울기가 없으므로
//! 어댑터가 기본값(1.0 / 0)을 채운다 — 소비자는 fallback을 모른다.
//!
//! 레이어링 규칙(목표): egui 포인터 이벤트를 직접 읽어 입력으로 해석하는
//! 곳은 이 모듈로 한정한다. 기존 `canvas/input.rs` 등의 직접 소비 경로는
//! PR2(툴/워크스페이스)에서 허브 경로로 걷어 낸다.

use freedf_core::input_events::{
    InputEvent, PointerPhase, PointerSource,
};

/// egui 프레임 이벤트 목록 → 통합 입력 이벤트들 (순서 보존).
pub(crate) fn translate(events: &[egui::Event]) -> Vec<InputEvent> {
    let mut out = Vec::new();
    for ev in events {
        match ev {
            // 주 버튼 — 마우스 포인터. 보조 버튼(오른쪽/가운데)은 포인터로
            // 만들지 않는다: 하드웨어 컨트롤(사이드 버튼) 축의 후보다.
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                ..
            } => {
                let phase = if *pressed {
                    PointerPhase::Down
                } else {
                    PointerPhase::Up
                };
                out.push(InputEvent::pointer(
                    PointerSource::Mouse,
                    phase,
                    [pos.x, pos.y],
                    1.0,
                    0.0,
                ));
            }
            egui::Event::PointerMoved(pos) => {
                // 드래그 여부는 허브 점유 상태가 판단한다 — 이동은 그대로 전달.
                out.push(InputEvent::pointer(
                    PointerSource::Mouse,
                    PointerPhase::Drag,
                    [pos.x, pos.y],
                    1.0,
                    0.0,
                ));
            }
            egui::Event::Touch { phase, pos, force, .. } => {
                let p = match phase {
                    egui::TouchPhase::Start => PointerPhase::Down,
                    egui::TouchPhase::Move => PointerPhase::Drag,
                    // End/Cancel 모두 "포인터 놓음" — Cancel(제스처 도중 차단)을
                    // 놓침으로 번역해야 허브의 점유 규칙이 풀린다.
                    egui::TouchPhase::End | egui::TouchPhase::Cancel => PointerPhase::Up,
                };
                out.push(InputEvent::pointer(
                    PointerSource::Pad,
                    p,
                    [pos.x, pos.y],
                    force.unwrap_or(1.0).clamp(0.0, 1.0),
                    0.0,
                ));
            }
            _ => {}
        }
    }
    out
}
