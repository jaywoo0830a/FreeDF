//! 원형 색상 팔레트 **휠 싱크** — 오버레이 프레스의 목적지 (C1).
//!
//! 종전에는 오버레이가 egui 원시 이벤트(`frame_tap_pos`)를 **다시 읽어** 탭을
//! 판정했다 — 잉크와 **다른 시계**로 같은 프레스를 해석한 것이다. 그 결과
//! 삼킴 표식과 "적체된 이벤트" 문제가 따라붙었다.
//!
//! 이제 휠은 라우터의 **싱크**다: 프레스가 오면(라우터가 승인) 이벤트가 안고 온
//! 좌표로 히트테스트하고, 결과를 **의도**(intent)로 앱에 남긴다. 시계도 egui
//! 레벨도 샘플링하지 않는다 — 잉크와 같은 어휘, 같은 기하, 같은 시계다.
//!
//! 우선순위: 휠이 열려 있는 동안 프레스는 **휠 소유**다 (복합 싱크
//! [`super::CanvasSinks`]에서 잉크보다 앞선다 — 구 동작 "휠이 열려 있으면 캔버스는
//! 잉크를 받지 않는다"의 구조적 표현).
//!
//! 렌더는 여전히 오버레이(egui)의 몫이다: 이 모듈은 **순수 기하 + 판정**만
//! 갖고, egui 타입을 import 하지 않는다 (`[f32; 2]` 좌표).

use freedf_core::input_events::{PointerEvent, PointerPhase};

use super::session_router::{Ctx, Decision, SessionView, Sink};

/// 탭이 휠의 어디에 닿았는지 — 순수 기하 판정 결과.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WheelHit {
    /// 중앙(도넛 구멍) — 지우개 도구로 전환.
    Center,
    /// 둘레 i번째 색 — 그 색을 적용.
    Swatch(usize),
    /// 뒷판 빈 곳 — 그냥 닫기.
    Backplate,
    /// 휠 바깥 — 닫기 (잉크를 만들지 않는다).
    Outside,
}

/// 휠 기하 — **히트테스트 수학의 유일한 소유자**.
///
/// 반지름/중심/스와치 개수는 앱(캔버스 레이아웃 상수)이 주입한다: 렌더가 그린
/// 자리와 판정이 보는 자리가 같은 값이 된다. egui 타입을 쓰지 않으므로
/// 장치/라우터 축에서도 그대로 쓸 수 있다.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct WheelGeom {
    pub center: [f32; 2],
    /// 바깥 반지름 — 이 밖은 `Outside`.
    pub back_r: f32,
    /// 스와치 중심이 놓이는 반지름.
    pub ring_r: f32,
    /// 스와치 반지름 (히트 판정 여유는 `hit`이 정한다).
    pub swatch_r: f32,
    /// 중앙 구멍 반지름.
    pub center_r: f32,
    /// 둘레 색 개수 (레이아웃).
    pub ring_len: usize,
}

impl WheelGeom {
    /// i번째 스와치 위치 — 12시 방향부터 시계 방향으로 균등 배치.
    pub fn swatch_pos(&self, i: usize) -> [f32; 2] {
        if self.ring_len == 0 {
            return self.center;
        }
        let angle = -std::f32::consts::TAU / 4.0
            + std::f32::consts::TAU * (i as f32) / (self.ring_len as f32);
        [
            self.center[0] + angle.cos() * self.ring_r,
            self.center[1] + angle.sin() * self.ring_r,
        ]
    }

    fn dist(&self, p: [f32; 2]) -> f32 {
        (p[0] - self.center[0]).hypot(p[1] - self.center[1])
    }

    /// 탭 좌표 판정 — 순서: 중앙 → 바깥 → 둘레 스와치 → 뒷판.
    pub fn hit(&self, p: [f32; 2]) -> WheelHit {
        if self.dist(p) <= self.center_r {
            return WheelHit::Center;
        }
        if self.dist(p) > self.back_r {
            return WheelHit::Outside;
        }
        for i in 0..self.ring_len {
            let s = self.swatch_pos(i);
            if (p[0] - s[0]).hypot(p[1] - s[1]) <= self.swatch_r + 3.0 {
                return WheelHit::Swatch(i);
            }
        }
        WheelHit::Backplate
    }
}

/// 휠 싱크가 앱에 남기는 의도 — 싱크는 앱 상태(색/툴)를 모른다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WheelIntent {
    /// 지우개 툴로 전환 (중앙 탭).
    SelectEraser,
    /// i번째 팔레트 색 적용 (앱이 자기 팔레트로 해석한다).
    PickSwatch(usize),
    /// 휠 닫기 (적용/뒷판/바깥 탭 모두).
    Close,
}

/// 휠 싱크 — 열려 있는 동안 프레스를 **전부 소유**한다.
///
/// 열림 상태/기하는 렌더(오버레이)가 소유하고 앱이 매 프레임 주입한다 —
/// 이 싱크는 "이 프레스가 휠의 어디인가"만 판정한다.
#[derive(Debug, Default)]
pub(crate) struct WheelSink {
    open: bool,
    geom: Option<WheelGeom>,
    intents: Vec<WheelIntent>,
    /// 마지막으로 승인한 목적지 이름 (진단).
    last_admitted: Option<&'static str>,
}

impl WheelSink {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// 이번 프레임의 열림 상태 (오버레이 소유 상태의 스냅샷).
    ///
    /// 프레스마다 갱신한다: 같은 프레임에 펜 버튼(휠 토글)과 팁 프레스가 함께
    /// 오는 경우, 큐 순서가 곧 시간 순서다 (컨트롤이 먼저면 그 프레스는 휠 소유).
    pub(crate) fn set_open(&mut self, open: bool) {
        self.open = open;
    }

    /// 이번 프레임의 휠 기하 (렌더가 쓰는 값 그대로).
    pub(crate) fn set_geometry(&mut self, geom: WheelGeom) {
        self.geom = Some(geom);
    }

    /// 판정 결과를 비워 돌려준다 — 순서 보존 (적용 → 닫기).
    pub(crate) fn drain_intents(&mut self) -> Vec<WheelIntent> {
        std::mem::take(&mut self.intents)
    }

    /// 열려 있는가 (진단/테스트).
    #[allow(dead_code)] // 진단/테스트 API
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    /// 방금 이 싱크가 승인한 프레스가 있었는가 (진단).
    #[allow(dead_code)] // 진단 API
    pub(crate) fn last_admitted(&self) -> Option<&'static str> {
        self.last_admitted
    }
}

impl Sink for WheelSink {
    fn name(&self) -> &'static str {
        "wheel"
    }

    fn admit(&mut self, _session: &SessionView, _ctx: &Ctx) -> Decision {
        // 열려 있는 동안 **모든** 프레스가 휠의 것이다 (안/밖 구분은 handle 이
        // 기하로 한다 — 밖이면 닫기만). 닫혀 있으면 관심 없다: 잉크가 받는다.
        if self.open {
            self.last_admitted = Some("wheel");
            Decision::Now
        } else {
            self.last_admitted = None;
            Decision::Refuse
        }
    }

    fn handle(&mut self, evs: &[PointerEvent]) {
        // 세션의 첫 Down 만 판정한다 (이후 Drag/Up 은 같은 프레스의 꼬리).
        let Some(down) = evs.iter().find(|e| e.phase == PointerPhase::Down) else {
            return;
        };
        let Some(geom) = self.geom else {
            // 기하 미주입 — 판정 불가. 프레스를 먹고 닫는다 (잉크 오염 방지).
            self.intents.push(WheelIntent::Close);
            return;
        };
        match geom.hit(down.point) {
            WheelHit::Center => {
                self.intents.push(WheelIntent::SelectEraser);
                self.intents.push(WheelIntent::Close);
            }
            WheelHit::Swatch(i) => {
                self.intents.push(WheelIntent::PickSwatch(i));
                self.intents.push(WheelIntent::Close);
            }
            // 뒷판 빈 곳 / 바깥 — 닫기만. 점을 만들지 않는다: 이 프레스는
            // "휠 소유"이므로 잉크 세션이 애초에 열리지 않았다.
            WheelHit::Backplate | WheelHit::Outside => self.intents.push(WheelIntent::Close),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::input::ink_sink::InkSink;
    use crate::app::input::session_router::{Outcome, SessionRouter};
    use crate::app::input::CanvasSinks;
    use freedf_core::input_events::{ControlKind, ControlPhase, EventKind, InputEvent, PointerSource, NO_TILT};

    fn geom(center: [f32; 2], ring_len: usize) -> WheelGeom {
        WheelGeom {
            center,
            back_r: 56.0,
            ring_r: 34.0,
            swatch_r: 12.0,
            center_r: 15.0,
            ring_len,
        }
    }

    fn ctx(now_ms: u64) -> Ctx {
        Ctx {
            now_ms,
            evidence: true,
            panning: false,
            focus_grace: false,
            focus_grab_pending: false,
        }
    }

    fn pen(phase: PointerPhase, point: [f32; 2]) -> PointerEvent {
        PointerEvent {
            source: PointerSource::Pen,
            phase,
            point,
            pressure: 0.5,
            tilt: NO_TILT,
        }
    }

    fn canvas_router(mut wheel: WheelSink) -> SessionRouter<CanvasSinks> {
        let mut ink = InkSink::new();
        ink.set_canvas([0.0, 0.0], [1000.0, 1000.0]);
        wheel.set_geometry(geom([100.0, 100.0], 4));
        let mut sinks = CanvasSinks::new();
        sinks.wheel = wheel;
        sinks.ink = ink;
        SessionRouter::new(vec![sinks])
    }

    /// 기하 판정 — 순수 함수 (egui/시계 없음).
    #[test]
    fn geometry_hit_test_is_pure() {
        let g = geom([100.0, 100.0], 4);
        assert_eq!(g.hit([100.0, 100.0]), WheelHit::Center);
        for i in 0..4 {
            assert_eq!(g.hit(g.swatch_pos(i)), WheelHit::Swatch(i));
        }
        assert_eq!(g.hit([100.0, 200.0]), WheelHit::Outside);
        // 스와치 사이(링 위 빈 각도)는 뒷판.
        let mid = [
            (g.swatch_pos(0)[0] + g.swatch_pos(3)[0]) / 2.0,
            (g.swatch_pos(0)[1] + g.swatch_pos(3)[1]) / 2.0,
        ];
        assert_eq!(g.hit(mid), WheelHit::Backplate);
    }

    /// 열려 있는 동안 프레스는 휠 소유 — 잉크 세션은 열리지 않는다 (우선순위).
    #[test]
    fn open_wheel_claims_presses_before_ink() {
        let mut wheel = WheelSink::new();
        wheel.set_open(true);
        let mut router = canvas_router(wheel);

        // 스와치 0 탭 — 휠이 승인하고 색 적용 의도를 남긴다.
        let p = router.sinks_mut()[0].wheel.geom.unwrap().swatch_pos(0);
        let rep = router.dispatch(&pen(PointerPhase::Down, p), &ctx(1000));
        assert_eq!(rep.outcome, Outcome::Admitted);
        router.dispatch(&pen(PointerPhase::Up, p), &ctx(1010));
        let intents = router.sinks_mut()[0].wheel.drain_intents();
        assert_eq!(intents, vec![WheelIntent::PickSwatch(0), WheelIntent::Close]);
        assert!(
            router.sinks_mut()[0].ink.drain().is_empty(),
            "휠 소유 프레스는 잉크 세션을 열지 않는다 (다른 시계 판정의 대체)"
        );
    }

    /// 닫혀 있으면 휠은 관심 없다 — 잉크가 정상으로 받는다.
    #[test]
    fn closed_wheel_refuses_and_ink_takes_it() {
        let mut wheel = WheelSink::new();
        wheel.set_open(false);
        let mut router = canvas_router(wheel);

        let p = [100.0, 100.0];
        let rep = router.dispatch(&pen(PointerPhase::Down, p), &ctx(1000));
        assert_eq!(rep.outcome, Outcome::Admitted);
        router.dispatch(&pen(PointerPhase::Up, p), &ctx(1010));
        assert!(router.sinks_mut()[0].wheel.drain_intents().is_empty());
        assert_eq!(
            router.sinks_mut()[0].ink.drain().len(),
            2,
            "휠이 닫혀 있으면 같은 획이 잉크로 간다"
        );
    }

    /// 바깥 탭 — 닫기만 (점 없음). 잉크에도 가지 않는다.
    #[test]
    fn outside_tap_closes_without_ink() {
        let mut wheel = WheelSink::new();
        wheel.set_open(true);
        let mut router = canvas_router(wheel);

        let far = [900.0, 900.0];
        router.dispatch(&pen(PointerPhase::Down, far), &ctx(1000));
        router.dispatch(&pen(PointerPhase::Up, far), &ctx(1010));
        assert_eq!(
            router.sinks_mut()[0].wheel.drain_intents(),
            vec![WheelIntent::Close]
        );
        assert!(router.sinks_mut()[0].ink.drain().is_empty(), "유령 점 없음");
    }

    /// 컨트롤(펜 버튼)은 휠 소유 프레스와 무관하게 계속 흐른다 — 휠을 버튼으로
    /// 닫을 수 있어야 한다 (프레스만 휠 소유이지, 입력 전체가 아니다).
    #[test]
    fn pen_button_still_flows_while_wheel_open() {
        let mut hub = freedf_core::input_hub::Hub::new();
        hub.emit(InputEvent::pointer(
            PointerSource::Pen,
            PointerPhase::Down,
            [100.0, 100.0],
            0.5,
            NO_TILT,
        ));
        hub.emit(InputEvent::control(
            ControlKind::StylusButton,
            1,
            ControlPhase::Down,
        ));
        let mut controls = 0;
        hub.take(|ev| {
            if ev.kind() == EventKind::Control {
                controls += 1;
            }
        });
        assert_eq!(controls, 1, "휠 프레임에도 컨트롤 에지는 도착한다");
    }
}