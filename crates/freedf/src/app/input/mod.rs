//! 입력 소스(펜/마우스/트랙패드) 판정 — 기기 종류 추정과 소스별 활동 추적.
//!
//! - [`egui_adapter`]: egui 이벤트 → 통합 어휘 번역 (egui 쪽 장치 어댑터).
//!   egui 포인터 이벤트를 통합 어휘로 만드는 것은 이 모듈 하나로 한정한다 —
//!   어댑터 경계 바깥에서는 raw 이벤트가 아니라 허브를 향해야 한다.
//! - [`hooks::InputSources`]: 소스별 활동 추정 (판정 규칙은 이 파일에 모임).
//! - [`session_router`]: 프레스의 목적지와 완결을 소유하는 객체.
//! - [`ink_sink`] / [`wheel_sink`]: 라우터의 목적지들 (잉크 / 원형 휠).
//! - [`CanvasSinks`]: 캔버스 목적지들의 **우선순위** 복합 (휠 → 잉크).

pub(crate) mod egui_adapter;
pub(crate) mod hooks;
pub(crate) mod ink_sink;
pub(crate) mod session_router;
pub(crate) mod wheel_sink;

pub(crate) use hooks::InputSources;
pub(crate) use ink_sink::InkSink;
pub(crate) use session_router::{Ctx, Decision, SessionRouter, SessionView, Sink};
pub(crate) use wheel_sink::WheelSink;

use freedf_core::input_events::PointerEvent;

/// 캔버스 목적지 복합 — **우선순위의 소유자** (휠이 잉크보다 앞선다).
///
/// 라우터는 하나의 `Sink`만 보지만(세션의 목적지 = 복합), 어느 목적지가
/// 승인했는지는 여기서 정한다. 라우터 장부의 `sink` 이름은 이 복합의 이름
/// (`"canvas"`)이다 — 개별 목적지 이름은 `last_admitted()`로 관측한다.
#[derive(Debug, Default)]
pub(crate) struct CanvasSinks {
    /// 우선순위 1 — 열려 있으면 프레스를 전부 소유한다 (오버레이 프레스).
    pub(crate) wheel: WheelSink,
    /// 우선순위 2 — 잉크(툴 세션).
    pub(crate) ink: InkSink,
    target: Option<&'static str>,
}

impl CanvasSinks {
    pub(crate) fn new() -> Self {
        Self {
            wheel: WheelSink::new(),
            ink: InkSink::new(),
            target: None,
        }
    }

    /// 이번 프레스가 어느 목적지로 갔는가 (진단).
    #[allow(dead_code)] // 진단 API
    pub(crate) fn last_admitted(&self) -> Option<&'static str> {
        self.target
    }
}

impl Sink for CanvasSinks {
    fn name(&self) -> &'static str {
        // 세션의 목적지 이름은 **안정**해야 한다 (라우터가 이 이름으로 되찾는다).
        // 어느 내부 목적지가 받았는지는 `last_admitted()`가 나른다.
        "canvas"
    }

    fn admit(&mut self, session: &SessionView, ctx: &Ctx) -> Decision {
        // 순서 = 우선순위. 앞선 목적지가 즉답하면 뒤는 묻지도 않는다.
        match self.wheel.admit(session, ctx) {
            Decision::Refuse => {}
            d => {
                self.target = Some("wheel");
                return d;
            }
        }
        match self.ink.admit(session, ctx) {
            Decision::Refuse => {
                self.target = None;
                Decision::Refuse
            }
            d => {
                self.target = Some("ink");
                d
            }
        }
    }

    fn handle(&mut self, evs: &[PointerEvent]) {
        // 승인한 목적지가 소비한다 (admit 이 기록한 target).
        if self.target == Some("wheel") {
            self.wheel.handle(evs);
        } else {
            self.ink.handle(evs);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use freedf_core::input_events::{PointerPhase, PointerSource, NO_TILT};

    /// 복합 싱크의 우선순위 — 휠이 열려 있으면 잉크는 묻지도 않는다.
    #[test]
    fn wheel_precedes_ink_in_priority() {
        let mut sinks = CanvasSinks::new();
        sinks.ink.set_canvas([0.0, 0.0], [500.0, 500.0]);
        let down = PointerEvent {
            source: PointerSource::Pen,
            phase: PointerPhase::Down,
            point: [10.0, 10.0],
            pressure: 1.0,
            tilt: NO_TILT,
        };
        let ctx = Ctx {
            now_ms: 0,
            evidence: true,
            panning: false,
            focus_grace: false,
            focus_grab_pending: false,
        };
        let view = SessionView {
            id: 1,
            source: PointerSource::Pen,
            down: &down,
            drags: &[],
            age_ms: 0,
        };
        // 닫힌 휠 → 잉크.
        sinks.wheel.set_open(false);
        assert_eq!(sinks.admit(&view, &ctx), Decision::Now);
        assert_eq!(sinks.last_admitted(), Some("ink"));
        // 열린 휠 → 휠 (기하가 안이든 밖이든 소유 — 밖은 handle 이 닫는다).
        sinks.wheel.set_open(true);
        assert_eq!(sinks.admit(&view, &ctx), Decision::Now);
        assert_eq!(sinks.last_admitted(), Some("wheel"));
    }
}
