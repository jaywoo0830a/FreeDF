//! 캔버스 입력 — 팬/줌(5% 스텝)/스크롤/필기 시작/포커스 제스처.
//!
//! 프레스의 목적지와 완결은 세션 라우터가 소유한다 (0916 마이그레이션 —
//! `session-router-migration.md`). 구 샘플링 게이트와 땜질(PendingDown)은
//! 삭제됐다: 게이트는 잉크 싱크의 **순수 기하** 정책이, 보류/승격은 라우터의
//! 세션 상태기계와 장부가 대신한다. 이 파일은 프레임 정책 문맥을 계산해
//! 명시 전달하고, 싱크의 아웃박스를 워크스페이스로 흘려보내는 배선만 남는다.

use super::*;
use freedf_core::input_events::PointerPhase;
use crate::app::input::session_router::{Ctx, Outcome};

impl FreeDfApp {
    pub(crate) fn handle_canvas_input(
        &mut self,
        ctx: &egui::Context,
        response: &egui::Response,
        origin: Pos2,
        canvas_size: [f32; 2],
    ) {
        let pointer_abs = response.interact_pointer_pos();

        // ── 원형 색상 팔레트(펜 버튼)가 열려 있으면 — 탭을 휠이 전담합니다.
        if self.color_wheel_open {
            if let Some(abs) = frame_tap_pos(ctx) {
                let canvas_rect =
                    egui::Rect::from_min_size(origin, egui::vec2(canvas_size[0], canvas_size[1]));
                let wheel_center = self.color_wheel_center(canvas_rect);
                if abs.distance(wheel_center) <= WHEEL_BACK_R + 4.0 {
                    // 휠 안 탭 — color_wheel_overlay가 처리, 캔버스 입력은 스킵.
                    // (릴리스 점도 삼켜 휠 탭이 페이지에 점을 남기지 않게)
                    self.wheel_swallow_click = true;
                    return;
                }
                // 바깥 탭 — 닫고 점 없이 삼킵니다.
                self.color_wheel_open = false;
                self.wheel_swallow_click = true;
                return;
            }
        }

        // Zoom (pinch / trackpad pinch / Ctrl+wheel / Ctrl+two-finger scroll)
        let (zoom_delta, scroll) = ctx.input(|i| (i.zoom_delta(), i.smooth_scroll_delta));
        let scroll_x = scroll.x;
        let scroll_y = scroll.y;
        let ctrl_down = ctx.input(|i| i.modifiers.ctrl);
        let dt = ctx.input(|i| i.stable_dt).max(1e-4);
        let pointer_any_down = ctx.input(|i| i.pointer.any_down());

        // 줌 잠금이면 모든 줌 입력(핀치/Ctrl+휠/트랙패드)을 무시합니다.
        if !self.zoom_lock {
            // PDF 렌더러 특성상 연속 줌(애니메이션)은 매 프레임 재래스터라
            // 뭘 해도 렉이 걸립니다. 모든 줌 입력을 **고정 5% 스텝**으로
            // 양자화해 한 번에 적용합니다 — 스텝당 재렌더 1회만 발생합니다.
            let mut steps = 0.0f32;
            // 1) 핀치/트랙패드 핀치 (연속 배율) — 프레임 간 델타는 작아서,
            //    **잔여 스텝을 누적**해 5%씩 쌓이는 대로 계속 발사합니다
            //    (한 프레임 단위 반올림으로 자잘한 핀치가 유실되던 것 보완).
            if (zoom_delta - 1.0).abs() > 1e-4 {
                self.pinch_accum_steps += zoom_delta.ln() / ZOOM_STEP.ln();
                let whole = self.pinch_accum_steps.trunc();
                if whole.abs() >= 0.5 {
                    steps += whole;
                    self.pinch_accum_steps -= whole;
                }
            } else {
                // 핀치가 멈추면 잔여(반 스텝 미만)는 버립니다.
                self.pinch_accum_steps = 0.0;
            }
            // 2) Ctrl+휠 노치 → 노치당 1스텝 (±5%).
            let mut ctrl_notches = 0.0f32;
            if ctrl_down {
                // egui의 smooth_scroll_delta는 스무딩돼 노치 1개가 크게
                // 튈 수 있으므로, 이번 프레임의 원시 휠 이벤트를 셉니다.
                let events: Vec<egui::Event> =
                    ctx.input(|i| i.events.iter().cloned().collect());
                for ev in &events {
                    if let egui::Event::MouseWheel {
                        unit,
                        delta,
                        modifiers,
                        ..
                    } = ev
                    {
                        if modifiers.ctrl {
                            ctrl_notches += match unit {
                                egui::MouseWheelUnit::Line => delta.y,
                                egui::MouseWheelUnit::Point => delta.y / 50.0,
                                egui::MouseWheelUnit::Page => delta.y,
                            };
                        }
                    }
                }
                if ctrl_notches.abs() > 1e-4 {
                    steps += ctrl_notches.round();
                }
            }
            if steps.abs() >= 0.5 && (response.hovered() || ctrl_notches.abs() > 1e-4) {
                // 포인터가 있으면 그 아래 페이지 점을 앵커로, 없으면 캔버스 중심.
                let anchor_ui = pointer_abs
                    .map(|abs| [abs.x - origin.x, abs.y - origin.y])
                    .unwrap_or([canvas_size[0] * 0.5, canvas_size[1] * 0.5]);
                self.view.zoom_at(anchor_ui, ZOOM_STEP.powf(steps), MIN_ZOOM, MAX_ZOOM);
                self.mark_zoom_dirty();
                ctx.request_repaint();
            }
        } // end !zoom_lock (줌 잠금)

        // ── Animated scroll (mouse wheel / trackpad) ─────────────────────
        // Wheel/trackpad deltas are not applied in one jump. They accumulate
        // in `scroll_vel` (pending pixels) and are eased into a pan each frame,
        // so scrolling glides instead of stepping. A mostly-vertical gesture
        // over a fully-visible page still flips to the previous/next page.
        let page_h_px = self.page_size_pts[1] * self.view.zoom;
        let page_w_px = self.page_size_pts[0] * self.view.zoom;
        if (scroll_x.abs() + scroll_y.abs()) > 0.0 && response.hovered() && !ctrl_down {
            if page_h_px <= canvas_size[1] && scroll_x.abs() <= scroll_y.abs() {
                // Whole page height visible & mostly-vertical gesture -> page flip.
                // Content follows the fingers (natural scrolling): positive
                // scroll_y (fingers down) shows earlier content -> previous page.
                if scroll_y > 0.0 {
                    self.prev_page();
                } else {
                    self.next_page();
                }
                self.scroll_vel = Vec2::ZERO;
            } else {
                // Accumulate; the per-frame easing below glides smoothly.
                self.scroll_vel += Vec2::new(scroll_x, scroll_y);
            }
            ctx.request_repaint();
        }
        if self.scroll_vel.length_sq() > 1e-8 {
            let k = (1.0 - (-SCROLL_SMOOTH_RATE * dt).exp()).min(1.0);
            let step = self.scroll_vel * k;
            self.scroll_vel -= step;
            let dx = if page_w_px <= canvas_size[0] { 0.0 } else { step.x };
            let dy = if page_h_px <= canvas_size[1] { 0.0 } else { step.y };
            if dx != 0.0 || dy != 0.0 {
                self.view.pan_by(dx, dy);
                ctx.request_repaint();
            }
        } else if !pointer_any_down {
            self.scroll_vel = Vec2::ZERO;
        }

        // Middle-button pan
        let middle_down = ctx.input(|i| i.pointer.button_down(egui::PointerButton::Middle));
        if middle_down {
            if let Some(abs) = ctx.input(|i| i.pointer.interact_pos()) {
                if let Some(last) = self.middle_pan_last {
                    let d = abs - last;
                    self.view.pan_by(d.x, d.y);
                }
                self.middle_pan_last = Some(abs);
            }
        } else {
            self.middle_pan_last = None;
        }

        let primary_down = ctx.input(|i| i.pointer.primary_down());

        // ── 입력 장치 판별 (래치 + 유예 시간) ─────────────────────────────
        // egui 0.36 이벤트에는 장치 필드가 없어, Windows Ink 펜의 `Event::Touch`
        // 유무로 펜/마우스를 구분합니다. 펜 입력 중 일부 프레임에는 Touch
        // 이벤트가 아예 없을 수 있는데, 그때마다 Mouse로 뒤집히면 **필기 중
        // 팬(페이지 이동)으로 전환되어 페이지가 갑자기 확 이동**합니다
        // (펜을 떼는 순간 장치 변환이 감지되던 버그의 원인).
        // → 마지막 터치 후 1초간은 Pen으로 유지하고, 스트로크 진행 중에는
        //   절대 Mouse로 뒤집지 않습니다.
        let has_touch = ctx
            .input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Touch { .. })));
        let any_pointer = ctx.input(|i| {
            i.pointer.any_down() || i.pointer.any_pressed() || i.pointer.any_released()
        });
        if has_touch {
            self.input_device = InputDevice::Pen;
            self.last_touch_time = Some(ctx.input(|i| i.time));
        } else if any_pointer && self.active_stroke.is_none() {
            let now = ctx.input(|i| i.time);
            let stale = self.last_touch_time.map_or(true, |t| now - t > 1.0);
            if stale {
                self.input_device = InputDevice::Mouse;
            }
        }

        // ── 사전 오버레이: 단어 탭 조회 (다른 동작보다 우선) ─────────────
        if response.clicked() && self.dictionary.enabled && self.document.is_some() {
            if let Some(abs) = pointer_abs {
                let p = abs - origin;
                let raw = self.view.view_to_page([p.x, p.y]);
                let page_w = self.page_size_pts[0];
                let page_h = self.page_size_pts[1];
                if raw[0] >= 0.0 && raw[0] <= page_w && raw[1] >= 0.0 && raw[1] <= page_h {
                    self.lookup_word_at(raw, abs);
                    return;
                }
            }
        }

        // 마우스/트랙패드는 (mouse_draws가 꺼져 있으면) 모든 잉크 도구에서
        // 팬으로 동작 — 팬만 글을 쓰게 하는 범용 관례를 따릅니다.
        let panning = self.tool == ToolType::Pan
            || (!self.mouse_draws
                && self.input_device == InputDevice::Mouse
                && matches!(
                    self.tool,
                    ToolType::Pen | ToolType::Fountain | ToolType::Highlighter | ToolType::Eraser
                ));

        // ── 허브 소비 → 컨트롤 맵/워크스페이스 → 문서 커맨드 ─────────────────
        // 컨트롤(펜 버튼)은 팬 중에도 처리하고, 포인터는 캔버스 정책을 통과한
        // 것만 툴에 준다. 캔버스 밖 누름·포커스 제스처·팬 정책은 앱 경계의 몫 —
        // 워크스페이스와 툴 상태기계는 UI/장치 지식이 없다.
        let mut hub = std::mem::take(&mut self.input_hub);
        // 이번 프레임의 시각 — 보류 세션 승격/만료 판정에 쓴다 (대여 충돌 회피).
        let now_ms = self.now_ms();
        // ── 프레임 정책 문맥 — egui 상태를 프레임당 한 번 계산해 **명시 전달**한다.
        // 라우터/싱크는 이 문맥 밖의 아무것도 보지 못한다 (암묵 샘플링 금지 —
        // 구 게이트 `response.is_pointer_button_down_on()` 샘플링의 대체물).
        let unfocused = ctx.input(|i| i.viewport().focused == Some(false));
        let mut router_ctx = Ctx {
            now_ms,
            // 접촉 증거 — ① egui가 프레스를 안다(마우스/펜 공통) ② **펜이 표면에
            // 닿아 있다**(하드웨어 사실 — evdev/OTD와 같은 시계라 경합 없음)
            // ③ 이번 프레임에 같은 스트림의 Drag가 흘렀다(아래에서 켠다).
            // 워치독(라이브 세션 TTL)과 보류 승격이 이 값을 재료로 쓴다.
            evidence: primary_down || self.pen_contact,
            panning,
            focus_grace: self.focus_grace_until_ms.is_some_and(|t| now_ms < t),
            focus_grab_pending: unfocused && !self.focus_grabbed,
        };
        self.input_router.sinks_mut()[0]
            .set_canvas([origin.x, origin.y], canvas_size);
        hub.take(|ev| match ev {
            freedf_core::input_events::InputEvent::Control(c) => {
                // ── 창 간 격리: 두 창이 같은 펜 장치(evdev/OTD)를 공유하므로,
                // **포커스된 창만** 사이드 버튼에 반응한다 — 배경 창의 휠이
                // 함께 열리는 버그 방지 (PR1 동작 보존).
                if !wheel_toggle_allowed(ctx.input(|i| i.viewport().focused)) {
                    return;
                }
                // 원시 컨트롤 → 사용자 매핑 → action. 미바인딩은 조용히 무시.
                if let Some(action) = self.control_map.translate(&c) {
                    self.workspace.handle(&action);
                }
            }
            freedf_core::input_events::InputEvent::Action(a) => {
                self.workspace.handle(&freedf_core::input_events::InputEvent::Action(a));
            }
            freedf_core::input_events::InputEvent::Pointer(p) => {
                // ── 세션 라우터 — 프레스의 목적지와 완결을 소유한다 ──────────
                // 게이트는 더 이상 egui 레벨 샘플링이 아니라 이벤트가 안고 있는
                // 좌표의 **순수 기하**다 (잉크 싱크) — Down 에지 1회 판정이
                // 시계 경합으로 파괴될 여지가 구조적으로 없다 (0916 수정의
                // 땜질 PendingDown 은 라우터의 세션/장부로 대체됐다).
                if p.phase == PointerPhase::Drag {
                    router_ctx.evidence = true; // 같은 소스 Drag = 접촉 증거
                }
                let rep = self.input_router.dispatch(&p, &router_ctx);
                Self::log_router_report(rep, panning);
            }
        });
        self.input_hub = hub;

        // ── 포커스 획득 제스처 — 싱크가 삼킨 프레스의 표식을 소화한다 ─────────
        if self.input_router.sinks_mut()[0].take_focus_request() {
            self.focus_grabbed = true;
            self.focus_swallow_next_click = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }

        // ── 프레임 말미 — 보류 세션 재판정 (라우터의 유일한 시간 진입점) ─────
        // 기하 즉담 구조에서는 hold 가 발생하지 않아 생산 경로에서는 no-op —
        // 안전망이다 (어떤 정책이 hold 를 쓰더라도 유실은 장부로 정산된다).
        // 팬 프레임의 보류 세션도 여기서 취소 정산된다 (구 "팬이 프레스를
        // 가져감" 폐기 경로의 대체물).
        if let Some(rep) = self.input_router.frame(&router_ctx) {
            Self::log_router_report(rep, panning);
        }

        // 활성 툴 동기화 — 워크스페이스가 진실원 (컨트롤 맵 전환·홀드 포함),
        // self.tool은 렌더/저장용 파생 캐시다.
        if let Some(t) = tool_type_of_name(self.workspace.active_name()) {
            if self.tool != t {
                self.tool = t;
            }
        }

        if !panning {
            // 라우터가 순서를 보존해 모은 이벤트 (보류 승격 시 [down, …drags]
            // 온전한 세션 재생 포함)를 툴 상태기계에 먹인다 — 툴은 문서 커맨드만
            // 생산한다.
            for p in self.input_router.sinks_mut()[0].drain() {
                self.workspace
                    .handle(&freedf_core::input_events::InputEvent::Pointer(p));
            }
        } // 팬 프레임의 포인터는 팬 경로가 가져간다 (아래) — 툴 세션이 열려 있지
        //   않다는 것이 팬 정책의 전제다 (팬 중에는 Down이 툴에 안 간다).

        // 워크스페이스가 생산한 문서 커맨드를 앱 상태에 적용한다.
        // (take 중 워크스페이스를 밖에 꺼내 소유권 충돌을 피한다.)
        let mut ws = std::mem::take(&mut self.workspace);
        ws.take_commands(|cmd| self.execute_command(cmd, ctx, origin, canvas_size));
        self.workspace = ws;

        if panning {
            if response.dragged() || response.is_pointer_button_down_on() {
                if let Some(abs) = pointer_abs {
                    if let Some(last) = self.pan_last {
                        let d = abs - last;
                        self.view.pan_by(d.x, d.y);
                    }
                    self.pan_last = Some(abs);
                }
            }
            if !primary_down {
                self.pan_last = None;
            }
            return;
        }

        // 놓친 Up 보험 — **에지 스트림 기준**으로 판정한다 (라우터 세션 상태).
        //
        // 구 구현은 egui의 레벨(`primary_down`)을 읽어 "버튼이 올라왔다"고
        // 판단했다. 샘플링 게이트 시절에는 그 등식이 성립했다 — 게이트 자체가
        // egui의 점유 인정(`is_pointer_button_down_on`)이었으므로, 잉크 세션은
        // egui가 프레스를 아는 프레임에만 열렸다.
        //
        // 게이트가 **순수 기하**로 바뀐 뒤(0916 마이그레이션) 이 등식은 깨진다:
        // 펜의 Down 에지는 egui보다 먼저 도착할 수 있고(evdev/OTD가 빠른 시계),
        // 그 프레임의 `primary_down`은 아직 false다. 보험이 그 지연을 "Up 유실"로
        // 오독해 **획을 시작점에서 잘라 버렸다** — writing.log 실측: 28획 중 14획이
        // 1점(`점 부족`)으로 종료, 그중 13획은 종료 시점의 live_pressure가 0.07~0.14
        // (펜이 아직 눌려 있었다) = 가짜 Up. 정상 14획은 모두 live_pressure≈0.0.
        //
        // 이제 Up 유실은 라우터가 소유한다: 에지가 끊긴 채 기기 접촉 증거도 없으면
        // `frame` 의 라이브 워치독이 **합성 up** 을 정상 경로로 흘려보내 닫는다.
        // 여기는 그 뒤에 남는 마지막 그물 — 라우터 세션이 없는데 앱에 획이 남은
        // 경우만 마무리한다 (egui/시계를 읽지 않는다).
        if self.input_router.open_session().is_none() && self.active_stroke.is_some() {
            // 진단 — 보험이 발동했다면 그 자체가 이상 신호다 (에지 스트림과
            // 앱 상태가 어긋났다는 뜻). 다음 회귀 분석이 즉시 가능하게 남긴다.
            pen_trace("STROKE-FINISH: 놓친 Up 보험 발동 — 라우터 세션 없음 + 열린 획");
            self.finish_stroke();
        }

        if matches!(
            self.tool,
            ToolType::Pen | ToolType::Fountain | ToolType::Highlighter
        ) {
            // 탭(점) 커밋 — 드래그 없이 눌렀다 뗀 경우 (기존 로직 이동).
            if response.clicked() && self.active_stroke.is_none() {
                // 포커스용 탭은 점을 찍지 않습니다 (프레스에서 삼킨 표식
                // 또는 포커스 획득 직후 유예).
                if self.focus_swallow_next_click {
                    self.focus_swallow_next_click = false;
                    return;
                }
                if self.wheel_swallow_click {
                    self.wheel_swallow_click = false;
                    return;
                }
                if self.focus_grace_until_ms.is_some_and(|t| self.now_ms() < t) {
                    return;
                }
                if let Some(abs) = pointer_abs {
                    let p = abs - origin;
                    let raw = self.view.view_to_page([p.x, p.y]);
                    let page_w = self.page_size_pts[0];
                    let page_h = self.page_size_pts[1];
                    // 클릭(점)도 페이지 내부일 때만 기록합니다.
                    if raw[0] >= 0.0 && raw[0] <= page_w && raw[1] >= 0.0 && raw[1] <= page_h {
                        let page = [raw[0].clamp(0.0, page_w), raw[1].clamp(0.0, page_h)];
                        let pressure = self.sample_pressure(ctx);
                        self.commit_dot(page, pressure);
                    }
                }
            } else if !primary_down {
                // 클릭이 완성되지 않았으면 삼킴 표식을 폐기합니다.
                self.focus_swallow_next_click = false;
                self.wheel_swallow_click = false;
            }
        }
    }

    // ---------- Stroke painting ----------

    /// 라우터 정산 보고 → 진단 로그. 형식은 땜질(PendingDown) 시절과 유지해
    /// 기존 장비 로그(`freedf_pendebug.log`)와 비교 가능하게 한다 — 구
    /// STROKE-RECOVER/STROKE-DROP 행이 이제 라우터 장부 행으로 발행된다.
    fn log_router_report(
        rep: crate::app::input::session_router::Report,
        panning: bool,
    ) {
        match rep.outcome {
            Outcome::Promoted => pen_trace(&format!(
                "STROKE-RECOVER: 보류 세션 승격 — source={:?} (온전한 세션 [down, …drags] 재생)",
                rep.source
            )),
            Outcome::Expired => pen_trace(&format!(
                "STROKE-DROP: 보류 Down 만료 — source={:?} (이 프레스 유실; 끝난 프레스는 나중 프레스에 붙지 않는다)",
                rep.source
            )),
            Outcome::Cancelled if panning => {
                pen_trace("STROKE-DROP: 팬 프레임이 프레스를 가져감 — 보류 세션 취소")
            }
            Outcome::Cancelled => {
                pen_trace("STROKE-DROP: 보류 세션 취소 — 싱크가 프레스를 내려놓았다")
            }
            Outcome::Stale => pen_trace(&format!(
                "STROKE-DROP: Up 에지 유실 — 라이브 세션을 합성 up 으로 닫음 source={:?}",
                rep.source
            )),
            _ => {}
        }
    }
}
