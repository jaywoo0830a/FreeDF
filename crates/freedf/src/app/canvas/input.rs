//! 캔버스 입력 — 팬/줌(5% 스텝)/스크롤/필기 시작/포커스 제스처.

use super::*;

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
            // 1) 핀치/트랙패드 핀치 (연속 배율) → ln으로 스텝 수 환산 후 반올림.
            if (zoom_delta - 1.0).abs() > 1e-4 {
                steps += (zoom_delta.ln() / ZOOM_STEP.ln()).round();
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
                self.render_dirty = true;
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

        match self.tool {
            ToolType::Pen | ToolType::Fountain | ToolType::Highlighter => {
                let page_w = self.page_size_pts[0];
                let page_h = self.page_size_pts[1];
                if primary_down && (response.is_pointer_button_down_on() || response.dragged()) {
                    // ── 포커스 제스처 (스플릿 뷰) ─────────────────────────
                    // ① 아직 포커스 없음 → 이 프레스는 잉크 없이 포커스만
                    //    요청합니다 (한 번만 — 플랫폼이 무시하면 다음부터
                    //    그대로 그립니다). ② 이 프레스가 방금 포커스를 만든
                    //    직후(유예 중)라면 역시 삼킵니다.
                    let unfocused = ctx.input(|i| i.viewport().focused == Some(false));
                    if unfocused {
                        if !self.focus_grabbed {
                            self.focus_grabbed = true;
                            self.focus_swallow_next_click = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                            return;
                        }
                    } else if self.focus_grace_until_ms.is_some_and(|t| now_ms() < t) {
                        return;
                    }
                    if let Some(abs) = pointer_abs {
                        let p = abs - origin;
                        let raw = self.view.view_to_page([p.x, p.y]);
                        // 페이지(캔버스) 바깥에서는 필기 금지: 페이지 내부에서만
                        // 스트로크를 시작하고, 벗어나면 점을 추가하지 않습니다.
                        let inside = raw[0] >= 0.0
                            && raw[0] <= page_w
                            && raw[1] >= 0.0
                            && raw[1] <= page_h;
                        let page = [raw[0].clamp(0.0, page_w), raw[1].clamp(0.0, page_h)];
                        let (pressure, p_src) = self.pressure_source(ctx);
                        if self.active_stroke.is_none() {
                            if inside {
                                // 새 스트로크 시작: 스무딩 필터를 리셋해
                                // 이전 획과 섞이지 않게 합니다.
                                let sm = OneEuroFilter::from_smoothing(self.smoothing);
                                self.smooth_x = sm;
                                self.smooth_y = sm;
                                self.smooth_p = sm;
                                self.smooth_active = true;
                                let (color, width) = self.current_drawing_style();
                                // 선폭 확정기 — 점이 들어오는 즉시 폭을 잠급니다.
                                self.width_locker = Some(freedf_core::pen::WidthLocker::new(
                                    self.tool,
                                    width,
                                    self.pen_profile,
                                    self.fountain_profile,
                                    tilt_magnitude(&self.pen_tilt),
                                ));
                                self.active_stroke = Some(ActiveStroke {
                                    tool: self.tool,
                                    color,
                                    width,
                                    points: Vec::new(),
                                });
                                self.lift_cut_logged = false;
                                pen_trace(&format!(
                                    "stroke start: tool={:?} base_w={width:.1}pt pressure_enabled={} device={:?} p_k={:.2} s_k={:.2} src={p_src} tilt=[{:+.0},{:+.0}]",
                                    self.tool,
                                    self.pressure_enabled,
                                    self.input_device,
                                    self.pen_profile.pressure_k,
                                    self.pen_profile.speed_k,
                                    self.pen_tilt[0],
                                    self.pen_tilt[1]
                                ));
                            }
                        }
                        if let Some(st) = self.active_stroke.as_mut() {
                            if inside {
                                // ── 펜 떼기 직전 처리: 접촉이 해제됐거나 필압이
                                // 사실상 0으로 무너진 꼬리 리포트는 **버립니다** —
                                // 펜 떼는 순간 끝이 갑자기 가늘어지는 "확 바뀜"의
                                // 원인이었습니다. (첫 점 4개는 접촉 시작 타이밍
                                // 차이로 잘릴 수 있으니 점이 쌓인 뒤에만 적용)
                                let pen_lifted = !self.input_sources.pen_contact();
                                // 직전에는 힘이 있었는데 지금 1% 미만 → 리프트 꼬리.
                                let pressure_collapsed = pressure <= 0.01
                                    && st
                                        .points
                                        .last()
                                        .map_or(false, |q| q.pressure > 0.05);
                                let contact_lost = st.points.len() >= 4
                                    && (pen_lifted || pressure_collapsed);
                                if contact_lost {
                                    // 표시 중인 진행 획을 즉시 갱신하도록 캐시 무효화.
                                    self.active_mesh = None;
                                    if !self.lift_cut_logged {
                                        self.lift_cut_logged = true;
                                        pen_trace(
                                            "LIFT-CUT: 접촉 해제/필압 붕괴 뒤 도착한 꼬리 점 제거 (펜 떼는 순간 가늘어지는 것 방지)",
                                        );
                                    }
                                } else {
                                    // 만년필 모델은 점별 시각으로 속도를 계산합니다.
                                    let t_ms = now_ms();
                                    // 1€ 필터(선택적) — OTD 같은 드라이버가 이미
                                    // 안정화하는 환경에서는 꺼둘 수 있습니다.
                                    let (x, y, p) = if self.smoothing_enabled
                                        && self.smoothing > 0.001
                                        && self.smooth_active
                                    {
                                        let t = ctx.input(|i| i.time);
                                        let sx = self.smooth_x.filter(page[0], t);
                                        let sy = self.smooth_y.filter(page[1], t);
                                        let sp = self.smooth_p.filter(pressure, t);
                                        (sx, sy, sp.clamp(0.0, 1.0))
                                    } else {
                                        (page[0], page[1], pressure)
                                    };
                                    let raw = StrokePoint::with_time(x, y, p, t_ms);
                                    if let Some(locker) = &mut self.width_locker {
                                        // 이전 점의 폭을 확정하고 새 점을 잠급니다 —
                                        // 미래 점이 이전 폭을 바꾸는 일이 없습니다.
                                        let (locked_prev, tip) = locker.push(raw);
                                        if let Some(prev) = locked_prev {
                                            if let Some(last) = st.points.last_mut() {
                                                *last = prev;
                                            }
                                        }
                                        st.points.push(tip);
                                        // 진단: 25점마다 압력/잠금 폭을 남깁니다.
                                        if st.points.len() % 25 == 0 {
                                            pen_trace(&format!(
                                                "pt {}: pressure={p:.3} (src={p_src}) locked_w={:.3}",
                                                st.points.len(),
                                                st.points.last().map(|q| q.width).unwrap_or(0.0)
                                            ));
                                        }
                                    } else {
                                        st.push([x, y], p, t_ms);
                                    }
                                }
                            }
                        }
                    }
                }
                if !primary_down && self.active_stroke.is_some() {
                    self.finish_stroke();
                }
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
                    if self.focus_grace_until_ms.is_some_and(|t| now_ms() < t) {
                        return;
                    }
                    if let Some(abs) = pointer_abs {
                        let p = abs - origin;
                        let raw = self.view.view_to_page([p.x, p.y]);
                        // 클릭(점)도 페이지 내부일 때만 기록합니다.
                        if raw[0] >= 0.0
                            && raw[0] <= page_w
                            && raw[1] >= 0.0
                            && raw[1] <= page_h
                        {
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
            ToolType::Eraser => {
                if primary_down && (response.is_pointer_button_down_on() || response.dragged()) {
                    if let Some(abs) = pointer_abs {
                        let p = abs - origin;
                        let page = self.view.view_to_page([p.x, p.y]);
                        let radius = self.eraser_radius / self.view.zoom;
                        let removed = self.store.erase_at(self.current_page, page, radius);
                        if !removed.is_empty() {
                            // 지워진 행만 DB에서 삭제 (증분).
                            if let Some(doc_id) = self.doc_id {
                                let ids: Vec<i64> =
                                    removed.iter().map(|s| s.id as i64).collect();
                                self.db.delete_strokes(doc_id, &ids);
                            }
                            self.push_history(Edit::RemoveStrokes {
                                page: self.current_page,
                                strokes: removed.clone(),
                            });
                            self.logger.log(AppEvent::StrokeErased {
                                page: self.current_page,
                                strokes: removed.len(),
                            });
                        }
                    }
                }
            }
            ToolType::Pan => {}
        }
    }

    // ---------- Stroke painting ----------
}
