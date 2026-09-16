//! 잉크 싱크 — 세션 라우터의 생산 목적지 (마이그레이션 계획 2.3 단계).
//!
//! 구 게이트(`response.is_pointer_button_down_on() || response.dragged()`)를
//! **정책으로 강등**한 곳이다:
//! - `admit` = **순수 기하** (`down.point` 가 캔버스 안인가) + 프레임 정책 문맥
//!   ([`Ctx`]로 명시 전달된 팬/포커스 플래그). 시계도 egui 레벨도 샘플링하지
//!   않으므로 판정은 지연 없이 즉답한다 — hold/승격 경로는 생산에서 발동하지
//!   않는다 (라우터의 보류 장치는 하중을 지지지 않는 안전망).
//! - `handle` = 받은 이벤트를 순서대로 아웃박스에 적재한다. Down→Drag→Up 순서
//!   보존(및 보류 승격 시 온전한 세션 재생)은 라우터가 보장하고, 앱은 프레임
//!   말미에 아웃박스를 비워 워크스페이스에 먹인다.

use freedf_core::input_events::PointerEvent;

use super::session_router::{Ctx, Decision, SessionView, Sink};

/// 캔버스 기하 — 윈도 좌표의 직사각형 (min_x, min_y, max_x, max_y).
#[derive(Debug, Clone, Copy, PartialEq)]
struct CanvasRect {
    min_x: f32,
    min_y: f32,
    max_x: f32,
    max_y: f32,
}

impl CanvasRect {
    fn contains(&self, p: [f32; 2]) -> bool {
        p[0] >= self.min_x && p[0] <= self.max_x && p[1] >= self.min_y && p[1] <= self.max_y
    }
}

/// 잉크(툴 세션) 싱크. 툴 선택은 워크스페이스의 몫 — 이 싱크는 프레스를
/// "잉크 도구 세션으로 보낼지"만 판정한다.
#[derive(Debug, Default)]
pub(crate) struct InkSink {
    /// 라우터가 전달한 이벤트의 아웃박스 — 앱이 프레임 말미에 drain 한다.
    outbox: Vec<PointerEvent>,
    /// 이번 프레임의 캔버스 기하 — 앱이 매 프레임 주입한다 (기본값: 없음=전부 거절).
    canvas: Option<CanvasRect>,
    /// 포커스 획득 제스처로 삼킨 프레스가 있음 — 앱이 take 하면 Focus 커맨드를 보낸다.
    focus_grab_requested: bool,
}

impl InkSink {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// 이번 프레임의 캔버스 기하 주입 (윈도 좌표 — 이벤트 좌표와 같은 공간).
    pub(crate) fn set_canvas(&mut self, min: [f32; 2], size: [f32; 2]) {
        self.canvas = Some(CanvasRect {
            min_x: min[0],
            min_y: min[1],
            max_x: min[0] + size[0],
            max_y: min[1] + size[1],
        });
    }

    /// 아웃박스를 비워 돌려준다 — 순서는 라우터가 보존했다.
    pub(crate) fn drain(&mut self) -> Vec<PointerEvent> {
        std::mem::take(&mut self.outbox)
    }

    /// 이 프레임에 포커스 획득 제스처로 삼킨 프레스가 있었는가.
    pub(crate) fn take_focus_request(&mut self) -> bool {
        std::mem::take(&mut self.focus_grab_requested)
    }
}

impl Sink for InkSink {
    fn name(&self) -> &'static str {
        "ink"
    }

    fn admit(&mut self, session: &SessionView, ctx: &Ctx) -> Decision {
        // ① 팬이 이 프레임을 소유한다 — 잉크 금지 (구 배선의 "팬 프레임이
        //    프레스를 가져감" 정책).
        if ctx.panning {
            return Decision::Refuse;
        }
        // ② 순수 기하 — 이벤트가 스스로 안고 있는 좌표만 본다 (#2: 샘플링
        //    게이트의 완전한 퇴출). 캔버스 밖 프레스(툴바/오버레이)는 닫힌
        //    거절 — egui 위젯이 소비할 영역이다.
        let Some(rect) = self.canvas else {
            return Decision::Refuse; // 기하 미주입 — 방어적으로 거절
        };
        if !rect.contains(session.down.point) {
            return Decision::Refuse;
        }
        // ③ 포커스 제스처 — 의도적 삼킴이므로 **보류하지 않는다** (복구하면
        //    제스처가 무력화된다: 유예 프레스가 400ms 뒤에 재생되는 오동작).
        if ctx.focus_grace {
            return Decision::Refuse;
        }
        if ctx.focus_grab_pending {
            self.focus_grab_requested = true;
            return Decision::Refuse;
        }
        // ④ 기하 즉담 — hold/승격 없이 Down 에지가 온 프레임에서 세션이 열린다.
        Decision::Now
    }

    fn handle(&mut self, evs: &[PointerEvent]) {
        self.outbox.extend_from_slice(evs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::session_router::{Outcome, SessionRouter};
    use freedf_core::input_events::{PointerPhase, PointerSource, NO_TILT};

    fn down(p: [f32; 2]) -> PointerEvent {
        PointerEvent {
            source: PointerSource::Pen,
            phase: PointerPhase::Down,
            point: p,
            pressure: 0.3,
            tilt: NO_TILT,
        }
    }

    fn ctx(now: u64) -> Ctx {
        Ctx {
            now_ms: now,
            evidence: true,
            panning: false,
            focus_grace: false,
            focus_grab_pending: false,
        }
    }

    fn view<'a>(down: &'a PointerEvent) -> SessionView<'a> {
        SessionView {
            id: 1,
            source: down.source,
            down,
            drags: &[],
            age_ms: 0,
        }
    }

    #[test]
    fn geometry_admits_inside_and_refuses_outside() {
        let mut sink = InkSink::new();
        sink.set_canvas([100.0, 50.0], [400.0, 300.0]);
        let inside = down([300.0, 200.0]);
        let outside = down([50.0, 10.0]);
        assert_eq!(sink.admit(&view(&inside), &ctx(1000)), Decision::Now);
        assert_eq!(sink.admit(&view(&outside), &ctx(1000)), Decision::Refuse);
    }

    #[test]
    fn panning_and_focus_are_intentional_swallows() {
        let mut sink = InkSink::new();
        sink.set_canvas([0.0, 0.0], [400.0, 300.0]);
        let d = down([100.0, 100.0]);
        let mut panning_ctx = ctx(1000);
        panning_ctx.panning = true;
        assert_eq!(sink.admit(&view(&d), &panning_ctx), Decision::Refuse);
        assert!(!sink.take_focus_request());

        let mut grace_ctx = ctx(1000);
        grace_ctx.focus_grace = true;
        assert_eq!(sink.admit(&view(&d), &grace_ctx), Decision::Refuse);

        // 포커스 획득 제스처 — 삼키되, 앱이 알 수 있게 표식을 남긴다.
        let mut grab_ctx = ctx(1000);
        grab_ctx.focus_grab_pending = true;
        assert_eq!(sink.admit(&view(&d), &grab_ctx), Decision::Refuse);
        assert!(sink.take_focus_request(), "앱이 Focus 커맨드를 보낼 수 있게");
        assert!(!sink.take_focus_request(), "표식은 1회다");
    }

    #[test]
    fn handle_collects_in_order_and_drain_empties() {
        let mut sink = InkSink::new();
        sink.handle(&[down([1.0, 1.0]), down([2.0, 2.0])]);
        let out = sink.drain();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].point, [1.0, 1.0]);
        assert_eq!(out[1].point, [2.0, 2.0]);
        assert!(sink.drain().is_empty(), "drain 뒤 아웃박스는 비어 있다");
    }

    /// 실기 회귀 재현 → 고정: 펜 Down 이 egui보다 먼저 도착한 프레임(증거 없음)
    /// 에도 획이 잘리지 않는다.
    ///
    /// 종전 앱 보험(`!primary_down` → `finish_stroke`)은 그 프레임을 "Up 유실"로
    /// 오독해 1점 점으로 잘랐다 (writing.log: 28획 중 14획 점 부족, 종료 시
    /// live_pressure 0.07~0.14 = 펜이 눌린 채). 여기서는 파이프라인 수준에서
    /// 그 경로가 **구조적으로 불가능**함을 고정한다 — 첫 프레임의 증거가 거짓이어도
    /// 세션은 라이브로 남고, 뒤따르는 Drag 가 온전히 전달된다.
    #[test]
    fn lagging_evidence_frame_keeps_the_stroke_alive() {
        use freedf_core::input_commands::{check_well_formed, Command};
        use freedf_core::input_devices::PenEventAdapter;
        use freedf_core::input_events::InputEvent;
        use freedf_core::input_hub::Hub;
        use freedf_core::input_workspace::Workspace;
        use freedf_core::pen_input::{PenButtons, PenState};

        let pen_state = |contact: bool| PenState {
            tilt: [0.0, 0.0],
            pressure: Some(0.4),
            contact,
            buttons: PenButtons::default(),
        };
        let mut adapter = PenEventAdapter::default();
        let mut sink = InkSink::new();
        sink.set_canvas([0.0, 0.0], [1000.0, 1000.0]);
        let mut router = SessionRouter::new(vec![sink]);
        let mut ws = Workspace::new();

        let pump = |adapter: &mut PenEventAdapter,
                    router: &mut SessionRouter<InkSink>,
                    ws: &mut Workspace,
                        st: &PenState,
                        point: [f32; 2],
                        now: u64,
                        evidence: bool| {
            // 어댑터 → 허브 → 라우터 → 싱크 → 워크스페이스 (생산 배선과 동일 순서).
            let mut hub = Hub::new();
            for ev in adapter.update(st, Some(point)) {
                hub.emit(ev);
            }
            let c = Ctx {
                now_ms: now,
                evidence,
                panning: false,
                focus_grace: false,
                focus_grab_pending: false,
            };
            hub.take(|ev| {
                if let InputEvent::Pointer(p) = ev {
                    router.dispatch(&p, &c);
                }
            });
            router.frame(&c); // 프레임 말미 재판정 (워치독 포함)
            for p in router.sinks_mut()[0].drain() {
                ws.handle(&InputEvent::Pointer(p));
            }
        };

        // ① Down 이 egui보다 먼저 온 프레임 — 증거는 아직 거짓.
        pump(&mut adapter, &mut router, &mut ws, &pen_state(true), [30.0, 40.0], 1000, false);
        // ② egui가 따라잡기 전 프레임 몇 개 (16ms 간격 — stale 창 안).
        pump(&mut adapter, &mut router, &mut ws, &pen_state(true), [30.0, 40.0], 1016, false);
        pump(&mut adapter, &mut router, &mut ws, &pen_state(true), [30.0, 40.0], 1032, false);
        assert!(router.open_session().is_some(), "시계 지연은 획의 끝이 아니다");
        // ③ 접촉 유지 Drag → ④ 진짜 Up (펜을 뗀다).
        pump(&mut adapter, &mut router, &mut ws, &pen_state(true), [31.0, 41.0], 1040, true);
        pump(&mut adapter, &mut router, &mut ws, &pen_state(false), [32.0, 42.0], 1050, true);

        let mut cmds = Vec::new();
        ws.take_commands(|c| cmds.push(c));
        assert_eq!(
            cmds.iter().map(|c| c.kind()).collect::<Vec<_>>(),
            vec!["begin-stroke", "extend-stroke", "extend-stroke", "extend-stroke", "end-stroke"],
            "1점 점이 아니라 온전한 획 (접촉 유지 프레임마다 extend)"
        );
        assert!(matches!(cmds[0], Command::BeginStroke { point: [30.0, 40.0], .. }));
        assert!(check_well_formed(&cmds).is_ok());
        assert_eq!(router.ledger().len(), 1, "Down 1건 = 정산 1건 (유실 관측)");
    }
    /// Down 에지가 **같은 프레임**에 세션이 되고(판정 지연 0), 땜질 없이 유실이 없다.
    #[test]
    fn full_pipeline_zero_latency_stroke() {
        use freedf_core::input_commands::Command;
        use freedf_core::input_devices::PenEventAdapter;
        use freedf_core::input_events::InputEvent;
        use freedf_core::input_hub::Hub;
        use freedf_core::input_workspace::Workspace;
        use freedf_core::pen_input::PenState;

        let mut adapter = PenEventAdapter::default();
        let mut hub = Hub::new();
        let mut sink = InkSink::new();
        sink.set_canvas([0.0, 0.0], [1000.0, 1000.0]);
        let mut router = SessionRouter::new(vec![sink]);
        let mut ws = Workspace::new();

        // 프레스 1프레임 — Down 에지가 즉시 승인된다 (hold/보류 없음).
        let evs = adapter.update(
            &PenState {
                tilt: [0.0, 0.0],
                pressure: Some(0.5),
                contact: true,
                buttons: Default::default(),
            },
            Some([30.0, 40.0]),
        );
        for ev in evs {
            hub.emit(ev);
        }
        let c = Ctx {
            now_ms: 1000,
            evidence: true,
            panning: false,
            focus_grace: false,
            focus_grab_pending: false,
        };
        let mut delivered = 0;
        hub.take(|ev| {
            if let InputEvent::Pointer(p) = ev {
                let rep = router.dispatch(&p, &c);
                assert_eq!(rep.outcome, Outcome::Admitted);
                delivered += 1;
            }
        });
        assert_eq!(delivered, 1);
        // frame 은 할 일이 없다 — 보류가 없다.
        assert!(router.frame(&c).is_none());
        for p in router.sinks_mut()[0].drain() {
            ws.handle(&InputEvent::Pointer(p));
        }
        let mut cmds = Vec::new();
        ws.take_commands(|c| cmds.push(c));
        assert!(matches!(
            cmds.as_slice(),
            [Command::BeginStroke {
                point: [30.0, 40.0],
                ..
            }]
        ));
    }
}
