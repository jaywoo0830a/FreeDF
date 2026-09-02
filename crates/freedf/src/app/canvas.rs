//! Page canvas: pan/zoom input, page painting, text highlight, palette & nav overlays, custom cursors, page rendering.
//!
//! Extracted from `app.rs` during the layered refactor — these are
//! `FreeDfApp` methods living in a submodule (`use super::*` re-exports the
//! app state types and shared helpers).

use super::*;

impl FreeDfApp {
    pub(crate) fn current_drawing_style(&self) -> ([u8; 4], f32) {
        match self.tool {
            ToolType::Pen | ToolType::Ballpoint | ToolType::Fountain => {
                (self.pen_color, self.pen_width)
            }
            ToolType::Highlighter => (self.hi_color, self.hi_width),
            _ => ([0, 0, 0, 255], 2.0),
        }
    }

    /// Pen pressure from touch events (Windows Ink). egui reports force via
    /// `Event::Touch { force: Some(f) }`; falls back to full pressure for mouse.
    pub(crate) fn sample_pressure(&self, ctx: &egui::Context) -> f32 {
        if !self.pressure_enabled {
            return 1.0;
        }
        let force: Option<f32> = ctx.input(|i| {
            i.events
                .iter()
                .rev()
                .find_map(|e| match e {
                    egui::Event::Touch { force: Some(f), .. } => Some(*f),
                    _ => None,
                })
        });
        force.map(|f| f.clamp(0.0, 1.0)).unwrap_or(1.0)
    }

    pub(crate) fn finish_stroke(&mut self) {
        if let Some(active) = self.active_stroke.take() {
            if active.points.is_empty() {
                return;
            }
            // 하이라이터 + 텍스트 인식 모드면 스와이프가 닿은 문서 텍스트 위로
            // 깔끔한 하이라이트를 만들어 저장하고, 원본 자유선은 버립니다.
            if active.tool == ToolType::Highlighter
                && self.text_highlight_snap
                && self.document.is_some()
                && self.add_text_highlights(&active)
            {
                return;
            }
            let id = self.store.add_stroke(
                self.current_page,
                active.tool,
                active.color,
                active.width,
                active.points,
            );
            if let Some(stroke) = self.store.stroke(self.current_page, id).cloned() {
                self.history.push(Edit::AddStrokes {
                    page: self.current_page,
                    strokes: vec![stroke.clone()],
                });
                self.logger.log(AppEvent::StrokeAdded {
                    page: self.current_page,
                    points: stroke.points.len(),
                    tool: tool_label(active.tool).to_string(),
                    width: active.width,
                });
            }
            self.autosave();
        }
    }

    /// 스트로크가 닿은 텍스트 줄 위로 하이라이트 사각형 스트로크를 추가합니다.
    /// 성공(텍스트 하이라이트를 만든 경우)하면 `true`를 반환합니다.
    pub(crate) fn add_text_highlights(&mut self, active: &ActiveStroke) -> bool {
        let Some(doc) = &self.document else {
            return false;
        };
        let (mut x0, mut y0, mut x1, mut y1) =
            (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in &active.points {
            x0 = x0.min(p.x);
            y0 = y0.min(p.y);
            x1 = x1.max(p.x);
            y1 = y1.max(p.y);
        }
        if x1 < x0 || y1 < y0 {
            return false;
        }
        // 항상 현재 페이지의 텍스트 런을 새로 읽습니다 (이전 페이지/검색의
        // 캐시 `search_runs`를 쓰면 페이지를 넘긴 뒤 좌표가 어긋날 수 있음).
        let runs = doc.page_text_runs(self.current_page).unwrap_or_default();
        if runs.is_empty() {
            // 페이지에 선택 가능한 텍스트가 전혀 없음(스캔/이미지 PDF 등).
            self.status = Some(
                "No selectable text on this page — drew a free-form highlight."
                    .to_string(),
            );
            return false;
        }
        let rects = text_line_highlights(&runs, [x0, y0, x1, y1], 6.0);
        if rects.is_empty() {
            return false;
        }
        let mut strokes = Vec::new();
        for r in rects {
            let line_h = (r[3] - r[1]).max(2.0);
            let yc = (r[1] + r[3]) * 0.5;
            // 두께가 정확히 줄 높이가 되도록(비율 1.0) 필압 역산.
            let pressure = self.pressure_curve.pressure_of(line_h, line_h);
            let id = self.store.add_stroke(
                self.current_page,
                ToolType::Highlighter,
                active.color,
                line_h,
                vec![
                    StrokePoint::new(r[0], yc, pressure),
                    StrokePoint::new(r[2], yc, pressure),
                ],
            );
            if let Some(st) = self.store.stroke(self.current_page, id).cloned() {
                strokes.push(st);
            }
        }
        if strokes.is_empty() {
            return false;
        }
        self.history.push(Edit::AddStrokes {
            page: self.current_page,
            strokes: strokes.clone(),
        });
        self.logger.log(AppEvent::StrokeAdded {
            page: self.current_page,
            points: strokes.len() * 2,
            tool: "Highlighter".to_string(),
            width: active.width,
        });
        self.autosave();
        true
    }

    pub(crate) fn commit_dot(&mut self, point: [f32; 2], pressure: f32) {
        let (color, width) = self.current_drawing_style();
        self.active_stroke = Some(ActiveStroke {
            tool: self.tool,
            color,
            width,
            points: vec![StrokePoint::new(point[0], point[1], pressure)],
        });
        self.finish_stroke();
    }

    // ---------- Texture rendering ----------

    pub(crate) fn ensure_texture(&mut self, ctx: &egui::Context) {
        let Some(doc) = &self.document else {
            return;
        };
        let ppp = ctx.pixels_per_point();
        let target_w = self.page_size_pts[0] * self.view.zoom * ppp;
        let needs_render = self.render_dirty
            || self.texture.is_none()
            || (self.last_render_zoom - self.view.zoom).abs() / self.view.zoom.max(1e-3) > 0.15
            || (self.last_render_ppp - ppp).abs() > 0.01;

        if !needs_render {
            return;
        }

        match doc.render_page(self.current_page, target_w, 4096.0 * ppp) {
            Ok(rendered) => {
                let img = egui::ColorImage::from_rgba_unmultiplied(
                    [rendered.width, rendered.height],
                    &rendered.rgba,
                );
                if let Some(t) = self.texture.as_mut() {
                    t.set(img, egui::TextureOptions::LINEAR);
                } else {
                    self.texture =
                        Some(ctx.load_texture("page", img, egui::TextureOptions::LINEAR));
                }
                self.last_render_zoom = self.view.zoom;
                self.last_render_ppp = ppp;
                self.render_dirty = false;
            }
            Err(e) => self.status = Some(format!("Render error: {e}")),
        }
    }

    // ---------- Search ----------

    pub(crate) fn canvas(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let canvas = response.rect;
        let origin = canvas.min;
        let canvas_size = [canvas.width(), canvas.height()];
        // Preserve the zoom when the canvas resizes (panel toggles / window
        // resize): re-center the page at the current zoom instead of re-fitting.
        let resized = (self.prev_canvas[0] - canvas_size[0]).abs() > 2.0
            || (self.prev_canvas[1] - canvas_size[1]).abs() > 2.0;
        if self.document.is_some() && self.pending_fit.is_none() && resized {
            self.view
                .align_page(self.page_size_pts, canvas_size, TOP_MARGIN, self.page_align);
            self.render_dirty = true;
        }
        self.prev_canvas = canvas_size;
        self.last_canvas = canvas_size;

        // Background behind the page (Nord canvas surround — dark mode)
        let bg = crate::theme::nord::semantic::PAGE_SURROUND;
        painter.rect_filled(canvas, egui::CornerRadius::ZERO, bg);

        if self.document.is_none() {
            ui.painter_at(canvas).text(
                canvas.center(),
                egui::Align2::CENTER_CENTER,
                "Open a PDF or create a note to start annotating (Ctrl+O)",
                egui::TextStyle::Heading.resolve(ui.style()),
                ui.visuals().weak_text_color(),
            );
            return;
        }

        // Apply pending fit + render cache
        self.apply_pending_fit(canvas_size);
        self.ensure_texture(&ctx);

        // ---------- Input ----------
        self.handle_canvas_input(&ctx, &response, origin, canvas_size);
        // Keep the page within the canvas (no infinite panning)
        self.view.clamp_pan(self.page_size_pts, canvas_size, CANVAS_MARGIN);

        // Advance the page transition animation
        let mut animating = false;
        if let Some(anim) = &mut self.page_anim {
            let dt = ctx.input(|i| i.stable_dt).max(1e-4);
            anim.progress += dt / PAGE_ANIM_SECS;
            animating = anim.progress < 1.0;
            if !animating {
                self.page_anim = None;
                self.prev_texture = None;
            }
        }
        if animating {
            ctx.request_repaint();
        }

        // ---------- Draw ----------
        let page_view = self.view.page_size_to_view(self.page_size_pts[0], self.page_size_pts[1]);
        let page_rect = Rect::from_min_size(
            origin + Vec2::new(self.view.pan_x, self.view.pan_y),
            Vec2::new(page_view[0], page_view[1]),
        );

        // Paper color tint applied to the page image (colored paper).
        let paper = self.current_page_paper();
        let paper_tint = Color32::from_rgba_unmultiplied(
            paper.color[0],
            paper.color[1],
            paper.color[2],
            255,
        );

        // During a transition, draw the outgoing + incoming pages sliding.
        let mut anim_dx = 0.0_f32;
        if let (Some(anim), Some(prev)) = (&self.page_anim, &self.prev_texture) {
            let w = page_rect.width();
            let dir = anim.direction;
            let p = anim.progress;
            let old_off = -p * dir * w;
            let new_off = (1.0 - p) * dir * w;
            anim_dx = new_off;

            let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
            // Outgoing page (old texture)
            painter.rect_filled(
                page_rect.translate(Vec2::new(old_off, 0.0)).expand(6.0),
                egui::CornerRadius::same(4),
                Color32::from_black_alpha(70),
            );
            painter.image(
                prev.id(),
                page_rect.translate(Vec2::new(old_off, 0.0)),
                uv,
                paper_tint,
            );
            // Incoming page (new texture)
            painter.rect_filled(
                page_rect.translate(Vec2::new(new_off, 0.0)).expand(6.0),
                egui::CornerRadius::same(4),
                Color32::from_black_alpha(70),
            );
            if let Some(tex) = &self.texture {
                painter.image(
                    tex.id(),
                    page_rect.translate(Vec2::new(new_off, 0.0)),
                    uv,
                    paper_tint,
                );
            }
        }

        // Current-page rect/origin (shifted during a transition so border & ink follow)
        let draw_rect = page_rect.translate(Vec2::new(anim_dx, 0.0));
        let draw_origin = origin + Vec2::new(anim_dx, 0.0);

        // Page shadow, image and border (single page when not mid-transition)
        if self.page_anim.is_none() {
            painter.rect_filled(
                draw_rect.expand(6.0),
                egui::CornerRadius::same(4),
                Color32::from_black_alpha(70),
            );
            if let Some(tex) = &self.texture {
                painter.image(
                    tex.id(),
                    draw_rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    paper_tint,
                );
            }
            painter.rect_stroke(
                draw_rect,
                egui::CornerRadius::same(2),
                Stroke::new(1.0, Color32::from_gray(120)),
                egui::StrokeKind::Inside,
            );
            // Paper grid / ruling (only for notes)
            if self.current_note.is_some() {
                self.paint_paper(&painter, draw_origin);
            }
        }

        // Search highlights (under ink so annotations stay readable)
        self.paint_search_highlights(&painter, draw_origin);

        // Annotation strokes
        let strokes: Vec<_> = self.store.strokes_on(self.current_page).to_vec();
        for stroke in &strokes {
            self.paint_stroke(&painter, stroke, draw_origin);
        }
        if let Some(active) = &self.active_stroke {
            self.paint_active(&painter, active, draw_origin);
        }

        // Tool cursor — custom sprite over the page, OS cursor restored
        // everywhere else (so it never disappears outside the canvas).
        if let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) {
            if draw_rect.contains(pos) {
                ctx.set_cursor_icon(egui::CursorIcon::None);
                let time = ctx.input(|i| i.time) as f32;
                self.paint_custom_cursor(&painter, pos, time);
            } else {
                ctx.set_cursor_icon(egui::CursorIcon::Default);
            }
        } else {
            ctx.set_cursor_icon(egui::CursorIcon::Default);
        }

        // Zoom hint
        if self.document.is_some() && self.view.zoom >= 4.0 {
            painter.text(
                canvas.left_top() + Vec2::new(10.0, 10.0),
                egui::Align2::LEFT_TOP,
                "Ctrl+wheel: zoom / wheel: scroll & page / middle button: pan",
                egui::TextStyle::Small.resolve(ui.style()),
                ui.visuals().text_color(),
            );
        }

        // Floating page navigation overlay (bottom-center, semi-transparent).
        self.canvas_nav_overlay(&ctx, canvas);
        // Floating writing-tool / color palette (right-center of the canvas).
        self.canvas_palette_overlay(&ctx, canvas);
    }

    /// 페이지 내비게이션 오버레이: Prev/Next, 줌, Fit Width/Height를
    /// 캔버스 중앙 하단에 반투명하게 고정 표시합니다.
    pub(crate) fn canvas_nav_overlay(&mut self, ctx: &egui::Context, canvas: Rect) {
        let page_count = self.document.as_ref().map(|d| d.page_count()).unwrap_or(0);
        let can_prev = self.current_page > 0;
        let can_next = self.current_page + 1 < page_count;
        let fill = crate::theme::nord::semantic::overlay_bg();
        let stroke = crate::theme::nord::semantic::OVERLAY_BORDER;

        // 캔버스 중앙(왼쪽 패널이 열려 있어도)에 정렬되도록 화면 중앙 대비 오프셋.
        let screen = ctx.input(|i| i.raw.screen_rect).unwrap_or(canvas);
        let dx = canvas.center().x - screen.center().x;

        egui::Area::new(egui::Id::new("canvas_nav_overlay"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(dx, -12.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(fill)
                    .stroke(Stroke::new(1.0, stroke))
                    .corner_radius(8.0)
                    .inner_margin(egui::Margin::same(5))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                            if ui
                                .add_enabled(
                                    can_prev,
                                    egui::Button::new(icon_text(ui, "Prev", icons::CARET_LEFT)),
                                )
                                .on_hover_text("Previous page")
                                .clicked()
                            {
                                self.prev_page();
                            }
                            let mut page_num = self.current_page + 1;
                            if ui
                                .add(
                                    egui::DragValue::new(&mut page_num)
                                        .range(1..=page_count.max(1)),
                                )
                                .on_hover_text("Page number")
                                .changed()
                            {
                                self.goto_page(page_num.saturating_sub(1));
                            }
                            ui.label(format!("/ {}", page_count.max(1)));
                            if ui
                                .add_enabled(
                                    can_next,
                                    egui::Button::new(icon_text(ui, "Next", icons::CARET_RIGHT)),
                                )
                                .on_hover_text("Next page")
                                .clicked()
                            {
                                self.next_page();
                            }
                            ui.separator();
                            if ui
                                .button(icon_text(ui, "", icons::MAGNIFYING_GLASS_MINUS))
                                .on_hover_text("Zoom out")
                                .clicked()
                            {
                                self.zoom_by(1.0 / 1.25);
                            }
                            ui.label(format!("{:.0}%", self.view.zoom / ZOOM_100_PERCENT * 100.0));
                            if ui
                                .button(icon_text(ui, "", icons::MAGNIFYING_GLASS_PLUS))
                                .on_hover_text("Zoom in")
                                .clicked()
                            {
                                self.zoom_by(1.25);
                            }
                            ui.separator();
                            if ui
                                .button(icon_text(ui, "Fit Width", icons::ARROWS_HORIZONTAL))
                                .on_hover_text("Fit width")
                                .clicked()
                            {
                                self.fit_width();
                            }
                            if ui
                                .button(icon_text(ui, "Fit Height", icons::ARROWS_VERTICAL))
                                .on_hover_text("Fit height")
                                .clicked()
                            {
                                self.fit_height();
                            }
                        });
                    });
            });
    }

    /// 굿노트식 필기구 전용 세로 팔레트: 캔버스 오른쪽 중앙에 도구 선택과
    /// 자주 쓰는 색상(즐겨찾기)을 반투명 오버레이로 띄웁니다.
    pub(crate) fn canvas_palette_overlay(&mut self, ctx: &egui::Context, canvas: Rect) {
        if !self.show_palette || self.document.is_none() {
            return;
        }
        let fill = crate::theme::nord::semantic::overlay_bg();
        let stroke = crate::theme::nord::semantic::OVERLAY_BORDER;
        // 캔버스 오른쪽 끝에 붙도록 화면 대비 오프셋.
        let screen = ctx.input(|i| i.raw.screen_rect).unwrap_or(canvas);
        let dx = canvas.right() - screen.right() - 14.0;

        let mut to_add = false;
        let mut to_remove: Option<usize> = None;

        egui::Area::new(egui::Id::new("canvas_palette_overlay"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::RIGHT_CENTER, egui::vec2(dx, 0.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(fill)
                    .stroke(Stroke::new(1.0, stroke))
                    .corner_radius(8.0)
                    .inner_margin(egui::Margin::same(5))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                        // 도구 선택 (세로, 설정된 순서를 따름).
                        let order = self.tool_order.clone();
                        for tool in order {
                            let label = tool.label();
                            if ui
                                .selectable_label(
                                    self.tool == tool,
                                    icon_text(ui, "", tool_icon(tool)),
                                )
                                .on_hover_text(label)
                                .clicked()
                            {
                                self.tool = tool;
                                self.save_session();
                            }
                        }
                        ui.separator();

                        // 현재 펜 색 + 즐겨찾기에 추가 버튼.
                        let cur = Color32::from_rgba_unmultiplied(
                            self.pen_color[0],
                            self.pen_color[1],
                            self.pen_color[2],
                            self.pen_color[3],
                        );
                        if color_circle_swatch(ui, "current_color", cur, false)
                            .on_hover_text("Current pen color")
                            .clicked()
                        {
                            self.tool = ToolType::Pen;
                            self.save_session();
                        }
                        if ui
                            .add(egui::Button::new(icon_text(ui, "", icons::PLUS)).frame(false))
                            .on_hover_text("Add current color to favorites")
                            .clicked()
                        {
                            to_add = true;
                        }
                        ui.separator();

                        // 자주 쓰는 색상 (클릭 = 적용, 우클릭 = 제거).
                        for i in 0..self.favorite_colors.len() {
                            let c = self.favorite_colors[i]; // Copy
                            let col = Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]);
                            let selected = self.pen_color == c;
                            let resp = color_circle_swatch(ui, ("fav_swatch", i), col, selected);
                            if resp
                                .clone()
                                .on_hover_text("Set pen color (right-click to remove)")
                                .clicked()
                            {
                                self.pen_color = c;
                                self.tool = ToolType::Pen;
                                self.save_default_session();
                                self.save_session();
                            }
                            if resp.secondary_clicked() {
                                to_remove = Some(i);
                            }
                        }
                    });
            });

        if to_add {
            let c = self.pen_color;
            if !self.favorite_colors.contains(&c) && self.favorite_colors.len() < 16 {
                self.favorite_colors.push(c);
                self.save_default_session();
            }
        }
        if let Some(i) = to_remove {
            if i < self.favorite_colors.len() {
                self.favorite_colors.remove(i);
                self.save_default_session();
            }
        }
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
    pub(crate) fn paint_paper(&self, painter: &egui::Painter, origin: Pos2) {
        let w = self.page_size_pts[0];
        let h = self.page_size_pts[1];
        let paper = self.current_page_paper();
        let style = paper.style;
        let spacing = paper.spacing;
        let line = Color32::from_rgba_unmultiplied(120, 120, 140, 100);
        for [x0, y0, x1, y1] in paper_lines(w, h, style, spacing) {
            let a = self.view.page_to_view([x0, y0]);
            let b = self.view.page_to_view([x1, y1]);
            painter.line_segment(
                [origin + Vec2::new(a[0], a[1]), origin + Vec2::new(b[0], b[1])],
                Stroke::new(2.0, line),
            );
        }
        for [x, y] in paper_dots(w, h, style, spacing) {
            let v = self.view.page_to_view([x, y]);
            painter.circle_filled(origin + Vec2::new(v[0], v[1]), 2.0, line);
        }
    }

    // ---------- Input handling ----------

    pub(crate) fn handle_canvas_input(
        &mut self,
        ctx: &egui::Context,
        response: &egui::Response,
        origin: Pos2,
        canvas_size: [f32; 2],
    ) {
        let pointer_abs = response.interact_pointer_pos();

        // Zoom (pinch / trackpad pinch / Ctrl+wheel / Ctrl+two-finger scroll)
        let (zoom_delta, scroll) = ctx.input(|i| (i.zoom_delta(), i.smooth_scroll_delta));
        let scroll_x = scroll.x;
        let scroll_y = scroll.y;
        let ctrl_down = ctx.input(|i| i.modifiers.ctrl);
        let dt = ctx.input(|i| i.stable_dt).max(1e-4);
        let pointer_any_down = ctx.input(|i| i.pointer.any_down());

        // If egui already folded a pinch / Ctrl+scroll into zoom_delta, use it.
        // Otherwise synthesize zoom from Ctrl + wheel: discrete +1% per notch
        // that slowly accelerates (up to +8%) while you keep scrolling.
        let mut zoom_factor = zoom_delta;
        let mut scroll_zoom = false;
        let mut ctrl_wheel_notches = 0.0f32;
        {
            // Count raw wheel notches this frame (egui's smooth_scroll_delta is
            // smoothed, so a single notch can look like a huge jump).
            let events: Vec<egui::Event> = ctx.input(|i| i.events.iter().cloned().collect());
            for ev in &events {
                if let egui::Event::MouseWheel {
                    unit,
                    delta,
                    modifiers,
                    ..
                } = ev
                {
                    if modifiers.ctrl {
                        let n = match unit {
                            egui::MouseWheelUnit::Line => delta.y,
                            egui::MouseWheelUnit::Point => delta.y / 50.0,
                            egui::MouseWheelUnit::Page => delta.y,
                        };
                        ctrl_wheel_notches += n;
                    }
                }
            }
        }
        if ctrl_down && ctrl_wheel_notches.abs() > 1e-4 && (zoom_delta - 1.0).abs() <= 1e-4 {
            // Restart the ramp if the user paused between notches, then
            // accelerate from +1% up to +8% per notch while scrolling fast.
            let now = ctx.input(|i| i.time);
            if now - self.zoom_accel_last > 0.3 {
                self.zoom_accel = 0.0;
            }
            self.zoom_accel = (self.zoom_accel + 0.01 * ctrl_wheel_notches.abs()).min(0.08);
            self.zoom_accel_last = now;
            let dir = ctrl_wheel_notches.signum();
            zoom_factor = (1.0 + self.zoom_accel * dir).clamp(0.5, 2.0);
            scroll_zoom = true;
        } else if ctrl_down && ctx.input(|i| i.time) - self.zoom_accel_last > 0.3 {
            // Reset the acceleration ramp once scrolling pauses.
            self.zoom_accel = 0.0;
        }

        let zooming = (zoom_factor - 1.0).abs() > 1e-4;
        // Pinch / trackpad pinch already arrive as a *continuous* zoom_delta,
        // so they are applied immediately (they are smooth by nature). A
        // discrete Ctrl+wheel notch instead only sets an eased *target*: the
        // real zoom glides toward it over a few frames instead of jumping.
        let continuous_zoom = (zoom_delta - 1.0).abs() > 1e-4 && !scroll_zoom;
        if zooming && (response.hovered() || scroll_zoom) {
            // Anchor at the pointer when available, otherwise the canvas center.
            let anchor_ui = pointer_abs
                .map(|abs| [abs.x - origin.x, abs.y - origin.y])
                .unwrap_or([canvas_size[0] * 0.5, canvas_size[1] * 0.5]);
            if continuous_zoom {
                self.view.zoom_at(anchor_ui, zoom_factor, MIN_ZOOM, MAX_ZOOM);
                self.render_dirty = true;
                self.zoom_target = None;
                self.zoom_anchor_page = None;
                self.zoom_anchor_ui = None;
                ctx.request_repaint();
            } else {
                // Ctrl+wheel: remember the page point under the cursor, then
                // animate zoom toward the target (compounds if still gliding).
                let page = [
                    (anchor_ui[0] - self.view.pan_x) / self.view.zoom,
                    (anchor_ui[1] - self.view.pan_y) / self.view.zoom,
                ];
                self.zoom_anchor_ui = Some(anchor_ui);
                self.zoom_anchor_page = Some(page);
                let base = self.zoom_target.unwrap_or(self.view.zoom);
                let t = (base * zoom_factor).clamp(MIN_ZOOM, MAX_ZOOM);
                self.zoom_target = Some(t);
                ctx.request_repaint();
            }
        }
        // Cancel an in-flight zoom animation as soon as the user starts a
        // gesture (drawing / panning), snapping to the final zoom cleanly.
        if pointer_any_down {
            if let Some(t) = self.zoom_target {
                self.view.zoom = t.clamp(MIN_ZOOM, MAX_ZOOM);
                self.render_dirty = true;
            }
            self.zoom_target = None;
            self.zoom_anchor_page = None;
            self.zoom_anchor_ui = None;
        }
        // Drive the eased zoom toward its target every frame (smooth glide).
        if let Some(target) = self.zoom_target {
            let diff = (target - self.view.zoom).abs();
            if diff < 1e-4 {
                self.view.zoom = target.clamp(MIN_ZOOM, MAX_ZOOM);
                self.zoom_target = None;
                self.zoom_anchor_page = None;
                self.zoom_anchor_ui = None;
            } else {
                let k = 1.0 - (-ZOOM_SMOOTH_RATE * dt).exp();
                let next = self.view.zoom + (target - self.view.zoom) * k;
                self.view.zoom = next.clamp(MIN_ZOOM, MAX_ZOOM);
                // Keep the anchored page point under the cursor during the glide.
                if let (Some(ui_p), Some(pg)) = (self.zoom_anchor_ui, self.zoom_anchor_page) {
                    self.view.pan_x = ui_p[0] - pg[0] * self.view.zoom;
                    self.view.pan_y = ui_p[1] - pg[1] * self.view.zoom;
                    self.view
                        .clamp_pan(self.page_size_pts, canvas_size, CANVAS_MARGIN);
                }
                self.render_dirty = true;
                ctx.request_repaint();
            }
        }

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

        match self.tool {
            ToolType::Pen | ToolType::Ballpoint | ToolType::Fountain | ToolType::Highlighter => {
                let page_w = self.page_size_pts[0];
                let page_h = self.page_size_pts[1];
                if primary_down && (response.is_pointer_button_down_on() || response.dragged()) {
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
                        let pressure = self.sample_pressure(ctx);
                        if self.active_stroke.is_none() {
                            if inside {
                                let (color, width) = self.current_drawing_style();
                                self.active_stroke = Some(ActiveStroke {
                                    tool: self.tool,
                                    color,
                                    width,
                                    points: Vec::new(),
                                });
                            }
                        }
                        if let Some(st) = self.active_stroke.as_mut() {
                            if inside {
                                st.push(page, pressure);
                            }
                        }
                    }
                }
                if !primary_down && self.active_stroke.is_some() {
                    self.finish_stroke();
                }
                if response.clicked() && self.active_stroke.is_none() {
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
                            self.history.push(Edit::RemoveStrokes {
                                page: self.current_page,
                                strokes: removed.clone(),
                            });
                            self.logger.log(AppEvent::StrokeErased {
                                page: self.current_page,
                                strokes: removed.len(),
                            });
                            self.autosave();
                        }
                    }
                }
            }
            ToolType::Pan => {
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
            }
        }
    }

    // ---------- Stroke painting ----------

    pub(crate) fn paint_active(&self, painter: &egui::Painter, active: &ActiveStroke, origin: Pos2) {
        let stroke = freedf_core::model::Stroke {
            id: 0,
            tool: active.tool,
            color: active.color,
            width: active.width,
            points: active.points.clone(),
        };
        self.paint_stroke(painter, &stroke, origin);
    }

    pub(crate) fn paint_stroke(&self, painter: &egui::Painter, stroke: &freedf_core::model::Stroke, origin: Pos2) {
        let color = Color32::from_rgba_unmultiplied(
            stroke.color[0],
            stroke.color[1],
            stroke.color[2],
            stroke.color[3],
        );
        let zoom = self.view.zoom;
        let pts = &stroke.points;
        if pts.is_empty() {
            return;
        }
        if pts.len() == 1 {
            let v = self.view.page_to_view([pts[0].x, pts[0].y]);
            let center = origin + Vec2::new(v[0], v[1]);
            let r = (self.pressure_curve.apply(stroke.width * zoom, pts[0].pressure)
                * base_width_factor(stroke.tool)
                * 0.5)
                .max(0.75);
            painter.circle_filled(center, r, color);
            return;
        }
        let wfactor = base_width_factor(stroke.tool);
        for w in pts.windows(2) {
            let a = self.view.page_to_view([w[0].x, w[0].y]);
            let b = self.view.page_to_view([w[1].x, w[1].y]);
            let pressure = (w[0].pressure + w[1].pressure) * 0.5;
            let wpx = if uses_own_profile(stroke.tool) {
                // 볼펜/만년필: 전역 곡선 대신 자체 닙 프로파일 사용.
                let speed = ((w[1].x - w[0].x).powi(2) + (w[1].y - w[0].y).powi(2)).sqrt();
                (stroke.width * zoom * wfactor * ink_modifier(stroke.tool, pressure, speed))
                    .max(0.5)
            } else {
                (self.pressure_curve.apply(stroke.width * zoom, pressure) * wfactor).max(0.5)
            };
            let pa = origin + Vec2::new(a[0], a[1]);
            let pb = origin + Vec2::new(b[0], b[1]);
            painter.line_segment([pa, pb], Stroke::new(wpx, color));
        }
    }

    /// Draws a custom cursor sprite confined to the canvas, previewing the
    /// current tool's shape and color (Pen = translucent gray circle,
    /// Highlighter = colored rectangle, Eraser = white translucent circle).
    pub(crate) fn paint_custom_cursor(&self, painter: &egui::Painter, pos: Pos2, time: f32) {
        match self.tool {
            ToolType::Pen | ToolType::Ballpoint | ToolType::Fountain => {
                match self.pen_cursor_style {
                    PenCursorStyle::Dot => {
                        // 작은 점.
                        let rect = Rect::from_center_size(pos, Vec2::splat(4.0));
                        painter.rect_filled(
                            rect,
                            2.0,
                            Color32::from_rgba_unmultiplied(120, 120, 120, 230),
                        );
                        painter.rect_stroke(
                            rect,
                            2.0,
                            Stroke::new(1.0, Color32::from_rgba_unmultiplied(70, 70, 70, 220)),
                            egui::StrokeKind::Outside,
                        );
                    }
                    PenCursorStyle::Round => {
                        // 펜 색/굵기를 미리보는 원 + 호흡 링(인터랙션 힌트).
                        let color = Color32::from_rgba_unmultiplied(
                            self.pen_color[0],
                            self.pen_color[1],
                            self.pen_color[2],
                            (self.pen_color[3] as f32 * 0.85).max(40.0) as u8,
                        );
                        let r = (self.pen_width
                            * base_width_factor(self.tool)
                            * self.view.zoom
                            * 0.5)
                            .clamp(2.5, 16.0);
                        let breath = 1.0 + 0.06 * (time * 3.0).sin();
                        // 흰 용지 위에서도 보이도록 어두운 윤곽을 먼저.
                        painter.circle_stroke(
                            pos,
                            r + 1.5,
                            Stroke::new(1.0, Color32::from_black_alpha(90)),
                        );
                        painter.circle_filled(pos, r, color);
                        painter.circle_stroke(
                            pos,
                            r,
                            Stroke::new(1.0, Color32::from_white_alpha(140)),
                        );
                        // 살짝 숨쉬는 바깥 링 — 커서가 살아있음을 알려줍니다.
                        painter.circle_stroke(
                            pos,
                            (r + 5.0) * breath,
                            Stroke::new(1.0, Color32::from_white_alpha(60)),
                        );
                        painter.circle_filled(pos, 1.2, Color32::from_white_alpha(200));
                    }
                }
            }
            ToolType::Highlighter => {
                // Translucent rectangle in the highlighter color.
                let color = Color32::from_rgba_unmultiplied(
                    self.hi_color[0],
                    self.hi_color[1],
                    self.hi_color[2],
                    (self.hi_color[3] as f32 * 0.9) as u8,
                );
                let rect = Rect::from_center_size(pos, Vec2::new(22.0, 30.0));
                painter.rect_filled(rect, 4.0, color);
                painter.rect_stroke(
                    rect,
                    4.0,
                    Stroke::new(1.0, Color32::from_white_alpha(170)),
                    egui::StrokeKind::Inside,
                );
            }
            ToolType::Eraser => {
                // White translucent circle with a soft dark drop shadow so it
                // reads clearly even on white paper.
                let r = self.eraser_radius.max(6.0);
                painter.circle_filled(
                    pos + Vec2::new(2.5, 2.5),
                    r,
                    Color32::from_black_alpha(40),
                );
                painter.circle_filled(pos, r, Color32::from_white_alpha(85));
                painter.circle_stroke(pos, r, Stroke::new(2.0, Color32::from_white_alpha(215)));
                painter.circle_filled(pos, 2.0, Color32::from_black_alpha(110));
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
