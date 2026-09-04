//! 캔버스 그리기 — 스트로크/용지/병합 잉크 메시/커스텀 커서/디버그 HUD.

use super::*;

impl FreeDfApp {
    /// 실시간 입력 디버그 HUD — 필압/틸트/속도/폭이 실제로 어떻게 들어오는지
    /// 바로 확인할 수 있습니다 (입력 장치가 필압을 보고하지 않으면 pressure가
    /// 계속 1.0으로 표시됩니다).
    pub(crate) fn paint_debug_hud(&self, ctx: &egui::Context, origin: Pos2) {
        let pressure = self.sample_pressure(ctx);
        let (_, p_src) = self.pressure_source(ctx);
        let (speed, tip_w, pts_n) = match &self.active_stroke {
            Some(st) if st.points.len() >= 2 => {
                let n = st.points.len();
                let a = st.points[n - 2];
                let b = st.points[n - 1];
                let dist = ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
                let dt = (b.t_ms.saturating_sub(a.t_ms)) as f32 / 1000.0;
                let v = if dt > 1e-4 { dist / dt } else { 0.0 };
                (v, st.points.last().map(|p| p.width).unwrap_or(0.0), n)
            }
            Some(st) => (
                0.0,
                st.points.first().map(|p| p.width).unwrap_or(0.0),
                st.points.len(),
            ),
            None => (0.0, 0.0, 0),
        };
        let (fps, touch_events) = ctx.input(|i| {
            let fps = if i.unstable_dt > 1e-4 {
                1.0 / i.unstable_dt
            } else {
                0.0
            };
            let touches = i
                .events
                .iter()
                .filter(|e| matches!(e, egui::Event::Touch { .. }))
                .count();
            (fps, touches)
        });
        let device = match self.input_device {
            InputDevice::Pen => "Pen",
            InputDevice::Mouse => "Mouse",
        };
        let is_fountain = self.tool == ToolType::Fountain;
        let (p_k, s_ref, t_k) = if is_fountain {
            (
                self.fountain_profile.pressure_alpha,
                self.fountain_profile.speed_ref,
                self.fountain_profile.tilt_k,
            )
        } else {
            (
                self.pen_profile.pressure_k,
                self.pen_profile.speed_max,
                self.pen_profile.tilt_k,
            )
        };
        let pen_src: &str = if cfg!(target_os = "windows") {
            "OTD"
        } else {
            "evdev"
        };
        egui::Area::new(egui::Id::new("freedf_debug_hud"))
            .fixed_pos(origin + egui::vec2(14.0, 14.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(220.0);
                    ui.strong("Debug HUD");
                    ui.label(format!(
                        "device: {device}  (touch events/frame: {touch_events})"
                    ));
                    ui.label(format!(
                        "pressure: {pressure:.3}  (src: {p_src})"
                    ));
                    ui.label(format!(
                        "tilt: [{:+.0}°, {:+.0}°]  (src: {})",
                        self.pen_tilt[0],
                        self.pen_tilt[1],
                        if self.pen_monitor.is_some() {
                            pen_src
                        } else {
                            "없음"
                        }
                    ));
                    ui.label(format!(
                        "pen buttons: b1={} b2={}",
                        self.pen_buttons.button1, self.pen_buttons.button2
                    ));
                    ui.label(format!("tip speed: {speed:.0} pt/s"));
                    ui.label(format!("tip width: {tip_w:.2} pt"));
                    ui.label(format!("active points: {pts_n}"));
                    ui.label(format!(
                        "pen stream: {}",
                        match self.last_pen_state_ms {
                            Some(t) => format!("수신됨 ({}ms 전)", now_ms().saturating_sub(t)),
                            None => "수신 없음 — OTD/장치 확인".to_string(),
                        }
                    ));
                    if let Some(v) = &self.pen_verdict {
                        ui.label(format!("verdict: {v}"));
                    }
                    ui.label("render: ribbon ≈ O(n) · 10ms 스로틀  (완성 획: 동일 리본)");
                    ui.label(format!("fps: {fps:.0}"));
                    ui.separator();
                    ui.label(format!(
                        "model: p_k={p_k:.2}  speed_ref/max={s_ref:.0}  tilt_k={t_k:.2}"
                    ));
                    ui.label(format!(
                        "tool: {}",
                        if is_fountain { "Fountain" } else { "Pen" }
                    ));
                });
            });
    }

    pub(crate) fn paint_search_highlights(&self, painter: &egui::Painter, origin: Pos2) {
        let match_fill = Color32::from_rgba_unmultiplied(255, 235, 60, 80);
        let current_fill = Color32::from_rgba_unmultiplied(255, 200, 40, 120);
        let current_stroke = Color32::from_rgb(255, 140, 0);
        for (i, m) in self.search_matches.iter().enumerate() {
            let r = m.rect;
            let a = self.view.page_to_view([r[0], r[1]]);
            let b = self.view.page_to_view([r[2], r[3]]);
            let rect = Rect::from_min_max(
                origin + Vec2::new(a[0], a[1]),
                origin + Vec2::new(b[0], b[1]),
            );
            if Some(i) == self.search_current {
                painter.rect_filled(rect, 2.0, current_fill);
                painter.rect_stroke(
                    rect,
                    2.0,
                    Stroke::new(2.0, current_stroke),
                    egui::StrokeKind::Inside,
                );
            } else {
                painter.rect_filled(rect, 2.0, match_fill);
            }
        }
    }

    /// Draws the paper grid / ruling / dots onto the page (notes only).
    ///
    /// The line **color and thickness are per-page settings** (`PagePaper`):
    /// thickness is stored in page points and scaled with the zoom so it stays
    /// proportional to the page, like real printed ruling.
    pub(crate) fn paint_paper(&self, painter: &egui::Painter, origin: Pos2) {
        let w = self.page_size_pts[0];
        let h = self.page_size_pts[1];
        let paper = self.current_page_paper();
        let style = paper.style;
        // 줄/격자/점 세부설정은 **스타일별 프리셋**을 참조합니다 — 페이지에는
        // 스타일만 저장되고, 프리셋을 바꾸면 그 스타일 페이지 전부가 함께 바뀝니다.
        let Some(ls) = self.paper_style_settings.of(style) else {
            return; // Blank — 그릴 줄 없음.
        };
        let spacing = ls.spacing;
        let line = Color32::from_rgba_unmultiplied(ls.color[0], ls.color[1], ls.color[2], ls.color[3]);
        let zoom = self.view.zoom;
        let stroke_w = (ls.width * zoom).clamp(0.5, 24.0);
        let dot_r = (ls.width * zoom * 0.4).clamp(0.6, 8.0);
        // 회전된 페이지는 줄도 종이와 함께 돌아야 합니다 (90/270° → 세로줄).
        let rotation = self
            .document
            .as_ref()
            .map(|d| d.page_rotation(self.current_page))
            .unwrap_or(freedf_core::text::PageRotation::None);
        for [x0, y0, x1, y1] in paper_lines_rotated(w, h, style, spacing, rotation) {
            let a = self.view.page_to_view([x0, y0]);
            let b = self.view.page_to_view([x1, y1]);
            painter.line_segment(
                [origin + Vec2::new(a[0], a[1]), origin + Vec2::new(b[0], b[1])],
                Stroke::new(stroke_w, line),
            );
        }
        for [x, y] in paper_dots(w, h, style, spacing) {
            let v = self.view.page_to_view([x, y]);
            painter.circle_filled(origin + Vec2::new(v[0], v[1]), dot_r, line);
        }
    }

    // ---------- Input handling ----------

    pub(crate) fn paint_active(
        &mut self,
        painter: &egui::Painter,
        active: &ActiveStroke,
        origin: Pos2,
    ) {
        // 나이 기준 = 첫 점의 시각(획 시작) — 그리는 동안에도 번짐이
        // 실시간으로 자라 펜을 떼는 순간 굵기가 튀지 않습니다.
        // (점 벡터 복사 없이 참조로 전달 — 매 프레임 O(n) 복사 제거)
        let created_ms = active
            .points
            .first()
            .map(|p| p.t_ms)
            .filter(|t| *t > 0)
            .unwrap_or_else(now_ms);
        self.paint_stroke(
            painter,
            active.tool,
            active.color,
            active.width,
            created_ms,
            &active.points,
            origin,
        );
    }

    /// 진행 중인 스트로크(또는 단일 스트로크)를 그립니다. 완성된 스트로크는
    /// [`FreeDfApp::build_ink_mesh`]의 병합 메시로 그려지고, 이 함수는
    /// 활성(진행 중) 획 전용입니다. 점의 폭은 입력 시점에 이미 잠겨 있으므로
    /// 여기서는 절대 다시 계산하지 않습니다.
    pub(crate) fn paint_stroke(
        &mut self,
        painter: &egui::Painter,
        tool: ToolType,
        color_in: [u8; 4],
        width: f32,
        created_ms: u64,
        pts: &[StrokePoint],
        origin: Pos2,
    ) {
        if pts.is_empty() {
            return;
        }
        let n = pts.len();
        let color =
            Color32::from_rgba_unmultiplied(color_in[0], color_in[1], color_in[2], color_in[3]);
        // ── 뷰포트 컬링: 페이지 bbox가 화면 밖이면 통째로 스킵 (줌인 시 큰 절약).
        let mut bb = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for p in pts {
            bb[0] = bb[0].min(p.x);
            bb[1] = bb[1].min(p.y);
            bb[2] = bb[2].max(p.x);
            bb[3] = bb[3].max(p.y);
        }
        {
            let pad = (width * 0.7).max(6.0);
            let a = self.view.page_to_view([bb[0] - pad, bb[1] - pad]);
            let b = self.view.page_to_view([bb[2] + pad, bb[3] + pad]);
            let rect = Rect::from_min_max(
                origin + egui::vec2(a[0], a[1]),
                origin + egui::vec2(b[0], b[1]),
            );
            if !rect.intersects(painter.clip_rect()) {
                return;
            }
        }
        let round_caps = matches!(tool, ToolType::Pen | ToolType::Fountain);
        let tilt = tilt_magnitude(&self.pen_tilt);
        let halves_pt = stroke_halves(tool, width, pts, &self.pen_profile, &self.fountain_profile, tilt);
        // ── 라이브 진단 (Debug HUD 켜져 있을 때만): 렌더 폭이 평평하면 경고.
        if pen_trace_on() && n >= 8 && now_ms().saturating_sub(self.pen_flat_log_ms) > 2000 {
            let (mut wmn, mut wmx) = (f32::MAX, f32::MIN);
            for h in &halves_pt {
                wmn = wmn.min(*h);
                wmx = wmx.max(*h);
            }
            let (mut pmn, mut pmx) = (f32::MAX, f32::MIN);
            for p in pts.iter() {
                pmn = pmn.min(p.pressure);
                pmx = pmx.max(p.pressure);
            }
            if wmx - wmn < 0.05 {
                self.pen_flat_log_ms = now_ms();
                pen_trace(&format!(
                    "LIVE-FLAT: n={n} widths=[{wmn:.3}..{wmx:.3}] pressure=[{pmn:.3}..{pmx:.3}] live_pressure={:?}",
                    self.live_pressure
                ));
            }
        }
        // 잉크 스밈 — **도구별**(볼펜은 은은하게, 만년필은 뚜렷하게).
        // 굵기는 쓴 그대로, 색만 점점 진해짐.
        let soak = if tool == ToolType::Fountain {
            self.fountain_soak
        } else {
            self.pen_soak
        };
        let soak_active =
            soak.enabled && matches!(tool, ToolType::Pen | ToolType::Fountain);
        let age_sec = if created_ms == 0 {
            soak.saturate_sec
        } else {
            (now_ms().saturating_sub(created_ms)) as f32 / 1000.0
        };
        let pts_pt: Vec<[f32; 2]> = pts.iter().map(|p| [p.x, p.y]).collect();
        // ── 10ms 스로틀: 지오메트리 재구성은 최대 100Hz — 그 사이엔
        // 캐시된 메시를 그대로 다시 그립니다.
        let now = now_ms();
        let view_key = (
            self.view.zoom,
            self.view.pan_x,
            self.view.pan_y,
            origin.x,
            origin.y,
            self.pen_soak,
            self.fountain_soak,
            self.pen_profile,
            self.fountain_profile,
            self.pen_grain,
            self.fountain_grain,
        );
        let key_eq = |a: &(f32, f32, f32, f32, f32, InkSoak, InkSoak, BallPenProfile, FountainProfile, InkGrain, InkGrain),
                      b: &(f32, f32, f32, f32, f32, InkSoak, InkSoak, BallPenProfile, FountainProfile, InkGrain, InkGrain)| {
            a.0 == b.0
                && a.1 == b.1
                && a.2 == b.2
                && a.3 == b.3
                && a.4 == b.4
                && a.5 == b.5
                && a.6 == b.6
                && a.7 == b.7
                && a.8 == b.8
                && a.9 == b.9
                && a.10 == b.10
        };
        if let Some((built_ms, n0, k0, mesh)) = &self.active_mesh {
            if *n0 == n && key_eq(k0, &view_key) {
                // 동일 입력·동일 뷰 — 캐시 그대로 (후광도 스로틀 단위로 갱신).
                painter.add(egui::Shape::mesh(mesh.clone()));
                return;
            }
            let view_changed = k0.0 != view_key.0
                || k0.1 != view_key.1
                || k0.2 != view_key.2
                || k0.3 != view_key.3
                || k0.4 != view_key.4;
            if !view_changed && now.saturating_sub(*built_ms) < ACTIVE_STROKE_GEOM_MS {
                // 새 점이 왔지만 스로틀 안 — 이번 프레임은 기존 메시 그대로.
                painter.add(egui::Shape::mesh(mesh.clone()));
                return;
            }
        }
        // ── 재구성 (리본 O(n) 단일 스캔 — 귀 자르기 없음).
        if pen_trace_on() {
            // 진단: 진행 중 렌더(리본)가 실제로 쓰는 폭 — 펜업 후 정착 렌더와 대조.
            let (mut hmn, mut hmx) = (f32::MAX, f32::MIN);
            for h in &halves_pt {
                hmn = hmn.min(*h);
                hmx = hmx.max(*h);
            }
            pen_trace(&format!(
                "ACTIVE-RENDER: n={n} half=[{hmn:.3}..{hmx:.3}] first_w={:.3} tip_w={:.3}",
                pts[0].width,
                pts[n - 1].width
            ));
        }
        let to_view = |p: [f32; 2]| -> Pos2 {
            let v = self.view.page_to_view(p);
            egui::pos2(origin.x + v[0], origin.y + v[1])
        };
        let feather_pt = 1.0 / self.view.zoom.max(1e-3); // 화면 1px에 해당하는 pt
        let mut mesh = egui::Mesh::default();
        // 잉크 스밈(도구별) + 잉크 질감(입체적 불균일) 합성:
        // **굵기는 쓴 그대로**, 좌우 정점 알파에 [포화 램프 × 질감 밀도]를 곱합니다.
        // 질감은 획 공간의 결정적 노이즈라 같은 획은 항상 같은 모양입니다.
        let alphas: Option<Vec<[f32; 2]>> =
            if matches!(tool, ToolType::Pen | ToolType::Fountain) {
                let grain = if tool == ToolType::Fountain {
                    self.fountain_grain
                } else {
                    self.pen_grain
                };
                let grain = ink_seed(grain, created_ms);
                let dens = stroke_ink_lr(tool, pts, grain);
                let now = now_ms();
                Some(
                    pts.iter()
                        .enumerate()
                        .map(|(i, p)| {
                            let sat = if soak_active {
                                let age = if p.t_ms == 0 {
                                    age_sec
                                } else {
                                    (now.saturating_sub(p.t_ms)) as f32 / 1000.0
                                };
                                soak.sat_at(age)
                            } else {
                                1.0
                            };
                            [
                                combine_saturation(sat, dens[i][0]),
                                combine_saturation(sat, dens[i][1]),
                            ]
                        })
                        .collect(),
                )
            } else {
                None
            };
        append_ribbon(
            &mut mesh,
            &freedf_core::pen::stroke_ribbon_lr(
                &pts_pt,
                &halves_pt,
                feather_pt,
                round_caps,
                alphas.as_deref(),
            ),
            &to_view,
            color,
        );
        let mesh = std::sync::Arc::new(mesh);
        painter.add(egui::Shape::mesh(mesh.clone()));
        self.active_mesh = Some((now, n, view_key, mesh));
    }

    /// 페이지의 **모든 완성 획**을 병합 잉크 메시 하나로 만듭니다.
    /// 진행 중 획과 **같은 리본 지오메트리**를 쓰므로 시각 차이가 없고,
    /// 드로우 콜은 페이지당 1개입니다.
    pub(crate) fn build_ink_mesh(
        &mut self,
        strokes: &[freedf_core::model::Stroke],
        origin: Pos2,
        clip: Rect,
        now: u64,
    ) -> Option<std::sync::Arc<egui::Mesh>> {
        let view = self.view;
        let to_view = |p: [f32; 2]| -> Pos2 {
            let v = view.page_to_view(p);
            egui::pos2(origin.x + v[0], origin.y + v[1])
        };
        let pen_soak = self.pen_soak;
        let fountain_soak = self.fountain_soak;
        let ball = self.pen_profile;
        let fountain = self.fountain_profile;
        let pen_grain = self.pen_grain;
        let fountain_grain = self.fountain_grain;
        let tilt = tilt_magnitude(&self.pen_tilt);
        let feather_pt = 1.0 / view.zoom.max(1e-3); // 화면 1px에 해당하는 pt

        // 1차 패스: 다음 재구성 시각 — **점별** (그 점이 닿은 시각 +
        // 포화 시간). 스밈이 켜진 볼펜/만년필 획 전부 포함.
        let mut next_settle = u64::MAX;
        for s in strokes {
            let soak = if s.tool == ToolType::Fountain {
                fountain_soak
            } else if s.tool == ToolType::Pen {
                pen_soak
            } else {
                continue;
            };
            if !soak.enabled {
                continue;
            }
            let deadline = (soak.saturate_sec.max(1e-3) * 1000.0) as u64;
            for p in &s.points {
                if p.t_ms > 0 {
                    let settle_ms = p.t_ms.saturating_add(deadline);
                    if now < settle_ms {
                        next_settle = next_settle.min(settle_ms);
                    }
                }
            }
        }
        self.ink_next_settle_ms = next_settle;

        let mut mesh = egui::Mesh::default();
        for s in strokes {
            if s.points.is_empty() {
                continue;
            }
            // 뷰포트 컬링.
            if let Some(bb) = s.bounding_box() {
                let pad = (s.width * 0.7).max(6.0);
                let a = view.page_to_view([bb[0] - pad, bb[1] - pad]);
                let b = view.page_to_view([bb[2] + pad, bb[3] + pad]);
                let rect = Rect::from_min_max(
                    origin + egui::vec2(a[0], a[1]),
                    origin + egui::vec2(b[0], b[1]),
                );
                if !rect.intersects(clip) {
                    continue;
                }
            }
            let color = Color32::from_rgba_unmultiplied(
                s.color[0],
                s.color[1],
                s.color[2],
                s.color[3],
            );
            let pts_pt: Vec<[f32; 2]> = s.points.iter().map(|p| [p.x, p.y]).collect();
            let halves = stroke_halves(s.tool, s.width, &s.points, &ball, &fountain, tilt);
            let round_caps = matches!(s.tool, ToolType::Pen | ToolType::Fountain);
            if Some(s.id) == self.last_finished_id {
                self.last_finished_id = None;
                if pen_trace_on() {
                    // 진단: 방금 끝난 획의 **정착 렌더** 폭 — ACTIVE-RENDER 로그와
                    // 대조해서 펜업 순간 뭐가 바뀌는지 확인.
                    let (mut hmn, mut hmx) = (f32::MAX, f32::MIN);
                    for h in &halves {
                        hmn = hmn.min(*h);
                        hmx = hmx.max(*h);
                    }
                    pen_trace(&format!(
                        "SETTLED-RENDER: id={} n={} half=[{hmn:.3}..{hmx:.3}] first_w={:.3} tip_w={:.3}",
                        s.id,
                        s.points.len(),
                        s.points[0].width,
                        s.points[s.points.len() - 1].width
                    ));
                }
            }
            // 잉크 스밈(도구별) + 질감 불균일 합성 — 좌우 정점 알파에
            // [포화 램프 × 밀도]를 곱합니다. 굵기는 쓴 그대로.
            let soak = if s.tool == ToolType::Fountain {
                fountain_soak
            } else {
                pen_soak
            };
            let soak_on = soak.enabled;
            let alphas: Option<Vec<[f32; 2]>> =
                if matches!(s.tool, ToolType::Pen | ToolType::Fountain) {
                    let grain = if s.tool == ToolType::Fountain {
                        fountain_grain
                    } else {
                        pen_grain
                    };
                    let created = if s.created_ms > 0 { s.created_ms } else { s.id };
                    let grain = ink_seed(grain, created);
                    let dens = stroke_ink_lr(s.tool, &s.points, grain);
                    Some(
                        s.points
                            .iter()
                            .enumerate()
                            .map(|(i, p)| {
                                let sat = if soak_on {
                                    let age = if p.t_ms == 0 {
                                        soak.saturate_sec
                                    } else {
                                        (now.saturating_sub(p.t_ms)) as f32 / 1000.0
                                    };
                                    soak.sat_at(age)
                                } else {
                                    1.0
                                };
                                [
                                    combine_saturation(sat, dens[i][0]),
                                    combine_saturation(sat, dens[i][1]),
                                ]
                            })
                            .collect(),
                    )
                } else {
                    None
                };
            append_ribbon(
                &mut mesh,
                &freedf_core::pen::stroke_ribbon_lr(
                    &pts_pt,
                    &halves,
                    feather_pt,
                    round_caps,
                    alphas.as_deref(),
                ),
                &to_view,
                color,
            );
        }
        Some(std::sync::Arc::new(mesh))
    }

    /// 병합 잉크 메시를 다시 만들어야 하는지 — 뷰/페이지/스트로크/설정이
    /// 바뀌었거나, 아직 번지고 있는(젊은) 블리드가 있거나, 방금 정착된
    /// 블리드가 있을 때만 재구성합니다.
    pub(crate) fn ink_needs_rebuild(&self, now: u64) -> bool {
        if self.ink_mesh.is_none() {
            return true;
        }
        let key = (
            self.current_page,
            self.store.rev(),
            self.store_generation,
            self.view.pan_x,
            self.view.pan_y,
            self.view.zoom,
            self.pen_soak,
            self.fountain_soak,
            self.pen_profile,
            self.fountain_profile,
            self.pen_grain,
            self.fountain_grain,
        );
        if key != self.ink_key {
            return true;
        }
        if self.ink_next_settle_ms != u64::MAX {
            // 방금 정착된 획이 있으면 이번 프레임에 한 번 더 재구성합니다.
            if self.ink_built_at < self.ink_next_settle_ms {
                return true;
            }
            // 아직 번지고 있으면 **50ms 스로틀**로만 재구성 (후광은 느리게
            // 자라므로 20Hz 갱신으로도 충분 — 사람은 못 느낍니다).
            if now < self.ink_next_settle_ms
                && now.saturating_sub(self.ink_built_at) >= HALO_GEOM_MS
            {
                return true;
            }
        }
        false
    }

    /// Draws a custom cursor sprite confined to the canvas, previewing the
    /// current tool's shape and color (Pen = 은색, Fountain = 금색 금속 닙 +
    /// 입체 그림자, Highlighter = colored rectangle, Eraser = white circle).
    /// 마우스 + 잉크 도구(mouse_draws 꺼짐)면 팬 십자선으로 표시합니다.
    pub(crate) fn paint_custom_cursor(&self, painter: &egui::Painter, pos: Pos2, time: f32) {
        // 실제로 쓰일 도구: 마우스는 기본적으로 팬처럼 동작.
        let mouse_panning = !self.mouse_draws
            && self.input_device == InputDevice::Mouse
            && matches!(
                self.tool,
                ToolType::Pen | ToolType::Fountain | ToolType::Highlighter | ToolType::Eraser
            );
        let tool = if mouse_panning {
            ToolType::Pan
        } else {
            self.tool
        };
        match tool {
            ToolType::Pen | ToolType::Fountain => {
                // ── 게임풍 금속 펜 닙 커서 — GPU 정점색 그라데이션(원통 음영) +
                // 배럴을 따라 **흐르는 반짝임 밴드** + 2겹 입체 그림자.
                // 만년필=금+뾰족 닙, 볼펜=은+볼 닙. 닙 끝(볼 중심)이 실제 좌표에 고정.
                let is_fountain = tool == ToolType::Fountain;
                // 금속 기본색 (금/은, f32 채널).
                let (br, bg, bb) = if is_fountain {
                    (0.82f32, 0.60, 0.08)
                } else {
                    (0.74, 0.77, 0.80)
                };
                let dark = if is_fountain {
                    Color32::from_rgb(74, 52, 6)
                } else {
                    Color32::from_rgb(58, 62, 68)
                };
                let bright = if is_fountain {
                    Color32::from_rgb(255, 240, 160)
                } else {
                    Color32::from_rgb(250, 252, 255)
                };
                let (az_raw, cos_pitch) = if self.pen_monitor.is_some() {
                    tilt_azimuth(&self.pen_tilt)
                } else if self.left_handed {
                    (-std::f32::consts::PI - DEFAULT_PEN_AZ, 1.0) // 위-왼쪽 기본.
                } else {
                    (DEFAULT_PEN_AZ, 1.0)
                };
                // 손잡이 반평면 제한 — 오른손잡이는 오른쪽, 왼손잡이는 왼쪽만.
                let az = clamp_azimuth_hand(az_raw, self.left_handed);
                let pitch = cos_pitch.acos();
                // 눕힐수록 배럴은 길게, 폭은 원근으로 좁아집니다.
                let len = 24.0 + 30.0 * pitch.sin();
                let w = (5.5 * cos_pitch).max(1.8);
                let dir = egui::vec2(az.cos(), az.sin());
                let perp = egui::vec2(-dir.y, dir.x);
                // 볼펜은 볼 반지름만큼 뒤에서 배럴이 시작 (볼이 좌표에 놓임).
                let ball_r = if is_fountain { 0.0 } else { 1.2 };
                let tip = pos - dir * ball_r;
                let tail = pos + dir * len;
                let tl = tail + perp * w;
                let tr = tail - perp * w;
                // 금속 채널 → 색 (k = 밝기 배율).
                let mk_col = |k: f32| -> Color32 {
                    let c = |v: f32| (v * k * 255.0).clamp(0.0, 255.0) as u8;
                    Color32::from_rgb(c(br), c(bg), c(bb))
                };

                // 1) 입체 그림자 2겹 (넓고 옅게 + 좁고 진하게) — **근접감** 반영:
                // 펜이 패드에 닿으면(접촉) 짙고 팁에 가깝게, 호버는 중간,
                // 패드에서 떨어져 리포트가 끊기면 옅고 멀어집니다 (입체감).
                let pen_contact = self
                    .pen_monitor
                    .as_ref()
                    .is_some_and(|m| m.snapshot().contact);
                let hover_age = self
                    .last_pen_state_ms
                    .map_or(u64::MAX, |t| now_ms().saturating_sub(t));
                let prox = if pen_contact {
                    1.0
                } else {
                    let base = if hover_age < 400 { 0.65 } else { 0.0 };
                    let fade = 1.0 - ((hover_age.saturating_sub(400)) as f32 / 900.0).clamp(0.0, 1.0);
                    (base + 0.65 * fade).clamp(0.0, 1.0)
                };
                let sh_scale = 1.3 - 0.55 * prox; // 멀수록 그림자가 더 떨어짐.
                let (sh1_a, sh2_a) = ((38.0 + 24.0 * prox) as u8, (62.0 + 42.0 * prox) as u8);
                for (sh, alpha) in [
                    (egui::vec2(4.0 * sh_scale, 5.0 * sh_scale), sh1_a),
                    (egui::vec2(2.0 * sh_scale, 2.5 * sh_scale), sh2_a),
                ] {
                    let sc = Color32::from_black_alpha(alpha);
                    painter.add(egui::Shape::convex_polygon(
                        vec![tip + sh, tr + sh, tl + sh],
                        sc,
                        Stroke::NONE,
                    ));
                    painter.circle_filled(tail + sh, w, sc);
                    if ball_r > 0.0 {
                        painter.circle_filled(pos + sh, ball_r, sc);
                    }
                }

                // 2) 배럴 — 링 3개 × 폭 컬럼 5개의 정점색 메시.
                // 폭 방향 = 원통 음영(어두운 쪽 → 밝은 림), 길이 방향 = 흐르는
                // 반짝임 밴드, 전체 = 은은한 숨(펄스).
                let glint_x = 0.45 + 0.45 * (time * 1.6).sin();
                let pulse = 0.94 + 0.06 * (time * 2.2).sin();
                let mut m = egui::Mesh::default();
                let rings = [0.0f32, 0.55, 1.0];
                let cols = [0.0f32, 0.25, 0.5, 0.75, 1.0];
                for &f in &rings {
                    let c = pos + dir * (ball_r + (len - ball_r) * f);
                    let rw = w * f.max(0.02);
                    for &u in &cols {
                        let p = c + perp * (rw * (u * 2.0 - 1.0));
                        let shade = 0.42 + 0.88 * u;
                        let glint = 1.0 + 1.35 * (-(f - glint_x).powi(2) / 0.018).exp();
                        let k = (shade * glint * pulse).clamp(0.0, 2.4);
                        m.vertices
                            .push(egui::epaint::Vertex::untextured(p, mk_col(k)));
                    }
                }
                let cc = cols.len() as u32;
                for ri in 0..rings.len() - 1 {
                    for ci in 0..cols.len() - 1 {
                        let a = ri as u32 * cc + ci as u32;
                        m.indices
                            .extend_from_slice(&[a, a + 1, a + cc, a + 1, a + cc + 1, a + cc]);
                    }
                }
                painter.add(egui::Shape::mesh(m));
                // 긴 변의 어두운 윤곽 + 꼬리 캡(중간톤 + 밝은 면).
                painter.line_segment([tip, tl], Stroke::new(1.0, dark));
                painter.line_segment([tip, tr], Stroke::new(1.0, dark));
                painter.circle_filled(tail, w, mk_col(0.55 * pulse));
                painter.circle_filled(tail + perp * (w * 0.45), w * 0.5, mk_col(1.5 * pulse));
                painter.circle_stroke(tail, w, Stroke::new(1.2, dark));

                // 3) 닙 끝.
                if is_fountain {
                    // 뾰족한 밝은 금속 팁 + 좌우로 미끄러지는 반짝임 점.
                    let t_len = 8.0;
                    let t1 = pos + dir * t_len + perp * 2.4;
                    let t2 = pos + dir * t_len - perp * 2.4;
                    painter.add(egui::Shape::convex_polygon(
                        vec![pos, t2, t1],
                        bright,
                        Stroke::new(1.0, dark),
                    ));
                    // 닙 숨구멍(원형 홀) — 만년필 특유의 디테일.
                    painter.circle_stroke(pos + dir * 5.0, 1.1, Stroke::new(1.0, dark));
                    let gx = (time * 3.0).sin() * 1.2;
                    painter.circle_filled(pos + dir * 2.5 + perp * gx, 1.1, Color32::WHITE);
                } else {
                    // 볼 — 방사형 그라데이션 팬(어두운 림 → 밝은 코어) + 회전 반사점.
                    let mut bm = egui::Mesh::default();
                    let ring_r = [1.0f32, 0.62, 0.3];
                    let ring_k = [0.35f32, 0.8, 1.7];
                    for (&rr, &rk) in ring_r.iter().zip(ring_k.iter()) {
                        for k in 0..12 {
                            let a = std::f32::consts::TAU * (k as f32 / 12.0);
                            let p = pos + egui::vec2(a.cos(), a.sin()) * (ball_r * rr);
                            bm.vertices
                                .push(egui::epaint::Vertex::untextured(p, mk_col(rk * pulse)));
                        }
                    }
                    let center = bm.vertices.len() as u32;
                    bm.vertices
                        .push(egui::epaint::Vertex::untextured(pos, mk_col(2.2 * pulse)));
                    for ri in 0..2usize {
                        for k in 0..12 {
                            let k2 = (k + 1) % 12;
                            let a0 = (ri * 12 + k) as u32;
                            let a1 = (ri * 12 + k2) as u32;
                            let b0 = ((ri + 1) * 12 + k) as u32;
                            let b1 = ((ri + 1) * 12 + k2) as u32;
                            bm.indices
                                .extend_from_slice(&[a0, a1, b0, a1, b1, b0]);
                        }
                    }
                    for k in 0..12 {
                        let k2 = (k + 1) % 12;
                        bm.indices
                            .extend_from_slice(&[(2 * 12 + k) as u32, center, (2 * 12 + k2) as u32]);
                    }
                    painter.add(egui::Shape::mesh(bm));
                    painter.circle_stroke(pos, ball_r, Stroke::new(1.2, dark));
                    // 회전하는 흰 반사점 — 구체감.
                    let sa = time * 1.4;
                    let sp = pos + egui::vec2(sa.cos(), sa.sin()) * (ball_r * 0.38);
                    painter.circle_filled(sp, ball_r * 0.22, Color32::WHITE);
                }
            }
            ToolType::Highlighter => {
                // 정밀한 마커 닙 커서: **작고 반듯한 사각형** — 두께는 실제
                // 하이라이트 두께와 같고, 왼쪽 시작점이 커서 위치에 고정됩니다
                // (그을 때 실제 선이 여기서 시작됨).
                let color = Color32::from_rgba_unmultiplied(
                    self.hi_color[0],
                    self.hi_color[1],
                    self.hi_color[2],
                    (self.hi_color[3] as f32 * 0.9) as u8,
                );
                let wpx = (self.hi_width * self.view.zoom).clamp(3.0, 90.0);
                let len = 14.0_f32; // 커서 길이는 짧게(힌트만)
                let half = wpx * 0.5;
                // 왼쪽 시작 모서리가 커서 위치.
                let min = pos + Vec2::new(0.0, -half);
                let rect = Rect::from_min_size(min, Vec2::new(len, wpx));
                // 반듯한 사각(모서리 없음) — 위치/크기를 정확히 미리보기.
                painter.rect_filled(rect, 0.0, color);
                painter.rect_stroke(
                    rect,
                    0.0,
                    Stroke::new(1.0, Color32::from_white_alpha(200)),
                    egui::StrokeKind::Inside,
                );
            }
            ToolType::Eraser => {
                // 그림자 없는 깔끔한 반투명 지우개 — 흰 원 + 테두리 + 중심점.
                let r = self.eraser_radius.max(6.0);
                painter.circle_filled(pos, r, Color32::from_white_alpha(85));
                painter.circle_stroke(pos, r, Stroke::new(2.0, Color32::from_white_alpha(215)));
                painter.circle_filled(pos, 2.0, Color32::from_gray(140));
            }
            ToolType::Pan => {
                // Small, compact "move" crosshair (much smaller than the OS grab hand).
                let c = Color32::from_gray(180);
                let s = 6.0;
                painter.line_segment(
                    [pos - Vec2::new(s, 0.0), pos + Vec2::new(s, 0.0)],
                    Stroke::new(1.5, c),
                );
                painter.line_segment(
                    [pos - Vec2::new(0.0, s), pos + Vec2::new(0.0, s)],
                    Stroke::new(1.5, c),
                );
                painter.circle_filled(pos, 2.0, c);
            }
        }
    }
}
