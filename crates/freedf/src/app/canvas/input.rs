//! 캔버스 입력 — 팬/줌(5% 스텝)/스크롤/필기 시작/포커스 제스처.

use super::*;
use freedf_core::input_events::{PointerEvent, PointerPhase, PointerSource};

/// 보류된 Down 에지의 최대 수명 (ms). 이보다 오래 보관한 에지는 승격하지
/// 않는다 — 이미 끝난 접촉이 later 프레스에 뒤늦게 붙는 것을 막는다.
pub(crate) const PENDING_DOWN_TTL_MS: u64 = 250;

/// 게이트에 막힌 Down 에지의 보류 상태 (**P1~P3 회귀 수정**).
///
/// 계약: **에지는 파괴되지 않는다.** 캔버스 정책 게이트가 거짓인 프레임의
/// Down은 버리지 않고 여기 보관했다가, 게이트가 참이 되고 그 소스의 접촉
/// 증거가 남아 있는 프레임에 **원래 접촉점 그대로** 승격한다.
///
/// 왜 필요한가: 리팩터 전에는 `primary_down && (down_on || dragged())`를
/// **매 프레임 폴링**해 게이트가 늦게 참이 되어도 획이 살아났다(늦은 시작).
/// 커맨드 경로(허브 → 툴 → BeginStroke)로 옮기면서 Down "에지 1회"만
/// 평가하게 됐고, 그 한 프레임이 게이트에 막혀 소비되면 **프레스 전체가
/// 무음 유실**됐다 (`tmp/0916debug.log`: Pen Down 7건 중 3건이 프레스 내내
/// `cmds(b/ext/end)=0/0/0`).
///
/// 힐 로직(되돌린 패치)과의 차이: 승격 대상은 **보류된 Down 에지**뿐이다.
/// "세션 밖 Drag"를 승격하지 않으므로, 펜을 뗀 뒤 같은 프레임에 쏟아지는
/// 꼬리 hover 이벤트가 새 세션을 열어 점을 연발하지 않는다.
///
/// egui에 의존하지 않는 순수 상태기계 — 전이는 [`Self::promote_if`] 하나다.
#[derive(Debug, Default)]
pub(crate) struct PendingDown {
    /// (막힌 Down 에지, 보관 시작 ms).
    held: Option<(PointerEvent, u64)>,
}

impl PendingDown {
    /// 막힌 Down을 보류한다 — 같은 프레스의 더 새로운 에지가 오면 최신이 이긴다.
    pub(crate) fn retain(&mut self, ev: PointerEvent, now_ms: u64) {
        self.held = Some((ev, now_ms));
    }

    /// 보류 폐기 (프레스 종료 Up / 팬이 프레스를 가져감 / 새 프레스 시작 /
    /// 포커스 유예). 반환: 폐기된 에지 (진단용).
    pub(crate) fn cancel(&mut self) -> Option<PointerEvent> {
        self.held.take().map(|(ev, _)| ev)
    }

    /// 보관 중인 에지 (승격 증거 판정/진단용).
    pub(crate) fn held(&self) -> Option<&PointerEvent> {
        self.held.as_ref().map(|(ev, _)| ev)
    }

    /// 보관 중이면 (에지, 보관 경과 ms).
    pub(crate) fn held_ms(&self, now_ms: u64) -> Option<(&PointerEvent, u64)> {
        self.held
            .as_ref()
            .map(|(ev, t)| (ev, now_ms.saturating_sub(*t)))
    }

    /// 승격 판정 — 참이면 보류를 비우고 그 Down을 돌려준다 (호출자가 이번
    /// 프레임 이벤트 **맨 앞**에 넣어 `begin → extend` 순서를 만든다).
    ///
    /// 조건(모두 참):
    /// ① 팬 프레임이 아님 ② TTL 안 ③ 캔버스 게이트가 *지금* 참
    /// ④ 그 소스의 접촉 증거가 있음
    ///
    /// TTL 만료는 이 프레스의 승격 기회를 영구히 끝낸다 (보류도 비운다).
    pub(crate) fn promote_if(
        &mut self,
        now_ms: u64,
        panning: bool,
        gate_now: bool,
        contact_evidence: bool,
    ) -> Option<PointerEvent> {
        let (_, since) = self.held.as_ref()?;
        let fresh = now_ms.saturating_sub(*since) <= PENDING_DOWN_TTL_MS;
        if !fresh {
            self.held = None;
            return None;
        }
        if panning || !gate_now || !contact_evidence {
            return None;
        }
        self.held.take().map(|(ev, _)| ev)
    }
}

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
        let mut pointer_events: Vec<PointerEvent> = Vec::new();
        // 이번 프레임의 시각 — 보류 에지 승격/만료 판정에 쓴다 (대여 충돌 회피).
        let now_ms = self.now_ms();
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
            freedf_core::input_events::InputEvent::Pointer(p) => match p.phase {
                PointerPhase::Down => {
                    // 캔버스 위에서 시작한 누름만 툴 세션을 연다 — 툴바/오버레이 위
                    // 누름은 egui 위젯이 소비. Drag/Up은 항상 통과 (세션 닫기
                    // 보장; 열려 있지 않으면 툴이 무시한다).
                    //
                    // ── 에지는 파괴되지 않는다 (P1~P3 회귀 수정) ──────────────
                    // 게이트가 거짓인 프레임의 Down은 **보류**한다. 리팩터 전에는
                    // 이 조건을 매 프레임 폴링해 늦은 시작이 복구됐지만, 커맨드
                    // 경로는 Down 에지 1회만 평가하므로 그 1회가 막히면 프레스
                    // 전체가 무음 유실됐다 (0916debug.log: 7건 중 3건).
                    if !(response.is_pointer_button_down_on() || response.dragged()) {
                        if !panning {
                            self.pending_down.retain(p, now_ms);
                        }
                        return;
                    }
                    // 정상 통과한 새 프레스 — 이전 프레스의 보류는 무효다.
                    self.pending_down.cancel();
                    // ── 포커스 제스처 (스플릿 뷰) ─────────────────────────────
                    // ① 아직 포커스 없음 → 이 프레스는 잉크 없이 포커스만 요청
                    //    (한 번만). ② 포커스 획득 직후 유예 중인 누름도 삼킵니다.
                    // **의도적 삼킴**이라 보류하지 않는다 (복구하면 제스처가 무력화).
                    let unfocused = ctx.input(|i| i.viewport().focused == Some(false));
                    if unfocused {
                        if !self.focus_grabbed {
                            self.focus_grabbed = true;
                            self.focus_swallow_next_click = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                            return;
                        }
                    } else if self.focus_grace_until_ms.is_some_and(|t| now_ms < t) {
                        return;
                    }
                    pointer_events.push(p);
                }
                PointerPhase::Up => {
                    // 프레스 종료 — 보류된 Down은 승격 기회를 잃는다.
                    // (같은 소스의 Up만: 반대 소스의 Up이 이 프레스를 닫지 못하게.)
                    if self
                        .pending_down
                        .held()
                        .is_some_and(|d| d.source == p.source)
                    {
                        self.pending_down.cancel();
                    }
                    pointer_events.push(p);
                }
                PointerPhase::Drag => pointer_events.push(p),
            },
        });
        self.input_hub = hub;

        // ── 보류된 Down 에지 승격 (P1~P3 회귀 수정) ─────────────────────────
        // 게이트에 막혀 보관된 Down을, 게이트가 *지금* 참이고 그 소스의 접촉
        // 증거가 남아 있는 프레임에 **원래 접촉점 그대로** 승격한다 — 리팩터 전
        // 폴링(늦은 시작) 동작의 복원이고, 첫 점이 실제 접촉점이라 더 정확하다.
        // 세션 밖 Drag는 절대 승격하지 않으므로(보류가 있어야만 승격) 펜을 뗀 뒤
        // 도착하는 꼬리 hover 이벤트가 점을 만들지 않는다 — 되돌린 힐 로직의
        // "점 연발" 원인을 구조적으로 제거한다.
        if panning {
            if let Some(d) = self.pending_down.cancel() {
                pen_trace(&format!(
                    "STROKE-DROP: 팬 프레임이 프레스를 가져감 — 보류 Down 폐기 source={:?}",
                    d.source
                ));
            }
        } else if self.focus_grace_until_ms.is_some_and(|t| now_ms < t) {
            // 포커스 유예 중 — 의도적 삼킴이므로 승격하지 않는다.
            self.pending_down.cancel();
        } else {
            let gate_now = response.is_pointer_button_down_on() || response.dragged();
            let primary_down_now = ctx.input(|i| i.pointer.primary_down());
            let evidence = self.pending_down.held_ms(now_ms).is_some_and(|(d, _)| {
                // ① egui가 프레스를 인지(마우스/펜 공통). ② 펜/패드 어댑터는
                // 접촉 중에만 Drag를 만든다 → 같은 소스의 Drag도 접촉 증명이다
                // (evdev가 egui보다 빠른 프레임에서 승격이 하루 늦지 않게).
                primary_down_now
                    || (matches!(d.source, PointerSource::Pen | PointerSource::Pad)
                        && pointer_events
                            .iter()
                            .any(|p| p.source == d.source && p.phase == PointerPhase::Drag))
            });
            let held = self
                .pending_down
                .held_ms(now_ms)
                .map(|(d, age)| (d.source, d.point, age));
            match self
                .pending_down
                .promote_if(now_ms, false, gate_now, evidence)
            {
                Some(d) => {
                    if let Some((_, _, age)) = held {
                        pen_trace(&format!(
                            "STROKE-RECOVER: 보류 Down 승격 — source={:?} point={:?} delay={age}ms (게이트 늦은 시작 복구)",
                            d.source, d.point
                        ));
                    }
                    // 이번 프레임 Drag들보다 **앞**에 놓아 begin → extend 순서를 지킨다.
                    pointer_events.insert(0, d);
                }
                None => {
                    if let Some((src, _, age)) = held {
                        if age > PENDING_DOWN_TTL_MS {
                            pen_trace(&format!(
                                "STROKE-DROP: 보류 Down 만료 — source={src:?} age={age}ms (이 프레스 유실; gate={gate_now} evidence={evidence})"
                            ));
                        }
                    }
                }
            }
        }

        // 활성 툴 동기화 — 워크스페이스가 진실원 (컨트롤 맵 전환·홀드 포함),
        // self.tool은 렌더/저장용 파생 캐시다.
        if let Some(t) = tool_type_of_name(self.workspace.active_name()) {
            if self.tool != t {
                self.tool = t;
            }
        }

        if !panning {
            // 툴 상태기계에 포인터를 먹인다 — 툴은 문서 커맨드만 생산한다.
            for p in &pointer_events {
                self.workspace
                    .handle(&freedf_core::input_events::InputEvent::Pointer(*p));
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

        // 놓친 Up 보험 — 잉크 세션이 열려 있는데 버튼이 올라왔다면 마무리한다.
        // (EndStroke 커맨드가 이미 처리했을 것이지만 스트림 유실에 대비한다.)
        if !primary_down && self.active_stroke.is_some() {
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
}
