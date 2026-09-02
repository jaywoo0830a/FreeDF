//! Page canvas: pan/zoom input, page painting, text highlight, palette & nav overlays, custom cursors, page rendering.
//!
//! Extracted from `app.rs` during the layered refactor — these are
//! `FreeDfApp` methods living in a submodule (`use super::*` re-exports the
//! app state types and shared helpers).

use super::*;

/// 펜 기울기 벡터(도, ±90) → 모델용 0..1 크기.
fn tilt_magnitude(tilt: &[f32; 2]) -> f32 {
    let m = (tilt[0] * tilt[0] + tilt[1] * tilt[1]).sqrt();
    (m / 90.0).min(1.0).max(0.0)
}

/// 삼각분할 결과를 egui 메시 2개로 직접 구성합니다:
/// - `fill`: 겹침 없는 본체 채움 삼각형.
/// - `aa`: **경계 가장자리에만** 붙는 페더링(AA) 스트립 — 내부 공유 가장자리는
///   제외해 반투명 잉크의 이음새 얼룩을 막습니다.
///
/// egui의 `convex_polygon`을 쓰지 않는 이유: epaint의 페더링 테셀레이터가
/// 슬리버(가늘고 긴) 삼각형에서 점 법선을 폭발시켜 정점이 수천 픽셀 밖으로
/// 튀는 "빛살(starburst)" 깨짐이 생기기 때문입니다. 우리 법선은 세그먼트
/// 단위라 어떤 입력에도 유한합니다.
fn build_stroke_meshes(
    poly: &[[f32; 2]],
    tris: &[[u32; 3]],
    color: Color32,
    feather: f32,
) -> (egui::Mesh, egui::Mesh) {
    let mut fill = egui::Mesh::default();
    for p in poly {
        fill.vertices
            .push(egui::epaint::Vertex::untextured(egui::pos2(p[0], p[1]), color));
    }
    for t in tris {
        fill.indices.extend_from_slice(&[t[0], t[1], t[2]]);
    }

    let mut aa = egui::Mesh::default();
    if feather > 0.0 {
        use std::collections::HashMap;
        // 가장자리 공유 횟수 — 1이면 경계(외곽선) 가장자리.
        let mut counts: HashMap<(u32, u32), u32> = HashMap::new();
        for t in tris {
            for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                *counts.entry((a.min(b), a.max(b))).or_default() += 1;
            }
        }
        for t in tris {
            for (ia, ib) in [(0usize, 1usize), (1, 2), (2, 0)] {
                let (a, b) = (t[ia], t[ib]);
                if counts.get(&(a.min(b), a.max(b))) != Some(&1) {
                    continue; // 내부 공유 가장자리 — 페더링 없음.
                }
                let ic = 3 - ia - ib; // 삼각형의 남은 정점 = 안쪽.
                let pa = egui::pos2(poly[a as usize][0], poly[a as usize][1]);
                let pb = egui::pos2(poly[b as usize][0], poly[b as usize][1]);
                let pc = egui::pos2(poly[t[ic] as usize][0], poly[t[ic] as usize][1]);
                let d = pb - pa;
                let len = d.length();
                if len < 1e-4 {
                    continue;
                }
                let perp = Vec2::new(-d.y, d.x) / len;
                // 안쪽 정점(pc)의 반대 방향이 바깥.
                let nrm = if (pc - pa).dot(perp) < 0.0 { perp } else { -perp };
                let (oa, ob) = (pa + nrm * feather, pb + nrm * feather);
                let base = aa.vertices.len() as u32;
                aa.vertices
                    .push(egui::epaint::Vertex::untextured(pa, color));
                aa.vertices
                    .push(egui::epaint::Vertex::untextured(pb, color));
                aa.vertices
                    .push(egui::epaint::Vertex::untextured(oa, Color32::TRANSPARENT));
                aa.vertices
                    .push(egui::epaint::Vertex::untextured(ob, Color32::TRANSPARENT));
                aa.indices
                    .extend_from_slice(&[base, base + 1, base + 2, base + 1, base + 3, base + 2]);
            }
        }
    }
    (fill, aa)
}

impl FreeDfApp {
    pub(crate) fn current_drawing_style(&self) -> ([u8; 4], f32) {
        match self.tool {
            ToolType::Pen | ToolType::Fountain => (self.pen_color, self.pen_width),
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
            self.smooth_active = false;
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
            let created_ms = now_ms();
            // DB 시퀀스에서 id를 미리 할당받아 스토어/히스토리/DB 행이 같은
            // id를 공유하게 합니다 (undo/redo가 정확히 같은 행을 복원).
            let db_id = self.db.alloc_stroke_ids(1).first().copied();
            let id = match (self.doc_id, db_id) {
                (Some(doc_id), Some(sid)) => {
                    self.store.add_stroke_with_id(
                        self.current_page,
                        sid as u64,
                        active.tool,
                        active.color,
                        active.width,
                        active.points,
                    );
                    self.store
                        .set_stroke_created_ms(self.current_page, sid as u64, created_ms);
                    let strokes: Vec<_> = self
                        .store
                        .strokes_on(self.current_page)
                        .iter()
                        .filter(|s| s.id == sid as u64)
                        .cloned()
                        .collect();
                    self.db
                        .insert_strokes(doc_id, self.current_page as i32, &strokes);
                    sid as u64
                }
                _ => {
                    let id = self.store.add_stroke(
                        self.current_page,
                        active.tool,
                        active.color,
                        active.width,
                        active.points,
                    );
                    self.store
                        .set_stroke_created_ms(self.current_page, id, created_ms);
                    id
                }
            };
            if let Some(stroke) = self.store.stroke(self.current_page, id).cloned() {
                self.push_history(Edit::AddStrokes {
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
        }
    }

    /// 스트로크가 닿은 **글자**들을 줄 단위로 묶어 밴드 하이라이트를 만듭니다.
    ///
    /// pdfium `tight_bounds()`(글자별 박스)로 정밀 판정하며, 각 줄은 그 줄의
    /// 높이만큼의 반투명 밴드 하나로 칠합니다. **필압은 전혀 쓰지 않습니다.**
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
        // 항상 현재 페이지의 글자 좌표를 새로 읽습니다 (캐시 없음 → 정확).
        let char_rects = doc.page_char_rects(self.current_page).unwrap_or_default();
        if char_rects.is_empty() {
            // 페이지에 선택 가능한 텍스트가 없음(스캔/이미지 PDF 등).
            self.status = Some(
                "No selectable text on this page — drew a free-form highlight."
                    .to_string(),
            );
            return false;
        }
        // 닿은 글자를 줄 단위로 합쳐 연속 밴드로 만듭니다.
        let rects = char_line_highlights(&char_rects, [x0, y0, x1, y1], 4.0);
        if rects.is_empty() {
            return false;
        }
        // DB 시퀀스에서 밴드 수만큼 id를 미리 할당합니다.
        let ids = self.db.alloc_stroke_ids(rects.len());
        let created_ms = now_ms();
        let mut strokes = Vec::new();
        for (k, r) in rects.iter().enumerate() {
            // 밴드 높이 = 그 줄의 글자 높이(포인트). 필압은 1.0(무시).
            let line_h = (r[3] - r[1]).max(2.0);
            let yc = (r[1] + r[3]) * 0.5;
            let sid = ids.get(k).copied().map(|i| i as u64).unwrap_or(0);
            strokes.push(freedf_core::model::Stroke {
                id: sid,
                tool: ToolType::Highlighter,
                color: active.color,
                width: line_h,
                points: vec![
                    StrokePoint::new(r[0], yc, 1.0),
                    StrokePoint::new(r[2], yc, 1.0),
                ],
                created_ms,
            });
        }
        self.store.add_strokes(self.current_page, strokes.clone());
        if let Some(doc_id) = self.doc_id {
            self.db
                .insert_strokes(doc_id, self.current_page as i32, &strokes);
        }
        self.push_history(Edit::AddStrokes {
            page: self.current_page,
            strokes: strokes.clone(),
        });
        self.logger.log(AppEvent::StrokeAdded {
            page: self.current_page,
            points: strokes.len() * 2,
            tool: "Highlighter".to_string(),
            width: active.width,
        });
        true
    }

    pub(crate) fn commit_dot(&mut self, point: [f32; 2], pressure: f32) {
        let (color, width) = self.current_drawing_style();
        self.active_stroke = Some(ActiveStroke {
            tool: self.tool,
            color,
            width,
            points: vec![StrokePoint::with_time(point[0], point[1], pressure, now_ms())],
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
                // 애니메이션 종료 → 다음 페이지를 미리 렌더.
                self.prefetch_pending = true;
            }
        }
        if animating {
            ctx.request_repaint();
        } else if self.prefetch_pending {
            // 다음/이전 페이지 텍스처 프리페치 (CPU 래스터 대기 제거).
            self.prefetch_page(&ctx);
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
        // PgUp/PgDn 키 전환은 세로(위/아래)로, 그 외(내비게이션/휠/화살표)는
        // 기존처럼 가로로 슬라이드합니다.
        let mut anim_dx = 0.0_f32;
        let mut anim_dy = 0.0_f32;
        if let (Some(anim), Some(prev)) = (&self.page_anim, &self.prev_texture) {
            let dir = anim.direction;
            let p = anim.progress;
            let span = if anim.vertical {
                page_rect.height()
            } else {
                page_rect.width()
            };
            let old_off = -p * dir * span;
            let new_off = (1.0 - p) * dir * span;
            let old_vec = if anim.vertical {
                Vec2::new(0.0, old_off)
            } else {
                Vec2::new(old_off, 0.0)
            };
            let new_vec = if anim.vertical {
                Vec2::new(0.0, new_off)
            } else {
                Vec2::new(new_off, 0.0)
            };
            anim_dx = new_vec.x;
            anim_dy = new_vec.y;

            let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
            // Outgoing page (old texture)
            painter.rect_filled(
                page_rect.translate(old_vec).expand(6.0),
                egui::CornerRadius::same(4),
                Color32::from_black_alpha(70),
            );
            painter.image(
                prev.id(),
                page_rect.translate(old_vec),
                uv,
                paper_tint,
            );
            // Incoming page (new texture)
            painter.rect_filled(
                page_rect.translate(new_vec).expand(6.0),
                egui::CornerRadius::same(4),
                Color32::from_black_alpha(70),
            );
            if let Some(tex) = &self.texture {
                painter.image(
                    tex.id(),
                    page_rect.translate(new_vec),
                    uv,
                    paper_tint,
                );
            }
        }

        // Current-page rect/origin (shifted during a transition so border & ink follow)
        let draw_rect = page_rect.translate(Vec2::new(anim_dx, anim_dy));
        let draw_origin = origin + Vec2::new(anim_dx, anim_dy);

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

        // Tool cursor — custom sprite only when the pointer is actually over
        // the canvas (not covered by a floating overlay, not over a side
        // panel) *and* inside the page rect. `response.hovered()` is false when
        // an overlay Area sits on top, and false outside the canvas rect — so
        // the OS cursor is always restored elsewhere (it used to disappear:
        // `draw_rect` could extend past the canvas / under overlays and then
        // `CursorIcon::None` hid the pointer with no custom sprite drawn).
        let pointer_pos = ctx.input(|i| i.pointer.hover_pos());
        let over_page = pointer_pos
            .is_some_and(|pos| canvas.contains(pos) && draw_rect.contains(pos));
        if response.hovered() && over_page {
            ctx.set_cursor_icon(egui::CursorIcon::None);
            let time = ctx.input(|i| i.time) as f32;
            if let Some(pos) = pointer_pos {
                self.paint_custom_cursor(&painter, pos, time);
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
        // 사전 오버레이 (단어 탭 조회 결과).
        self.dict_overlay(&ctx);
    }

    /// 다음(또는 이전) 페이지를 미리 렌더해 둡니다 — 페이지 전환 시
    /// CPU 래스터 대기를 없애 부드럽게 넘어갑니다.
    fn prefetch_page(&mut self, ctx: &egui::Context) {
        self.prefetch_pending = false;
        let Some(doc) = &self.document else {
            return;
        };
        let next = if self.current_page + 1 < doc.page_count() {
            self.current_page + 1
        } else if self.current_page > 0 {
            self.current_page - 1
        } else {
            return;
        };
        // 이미 같은 페이지를 같은 줌으로 프리페치해 두었으면 스킵.
        if let Some((p, z, _)) = &self.prefetch {
            if *p == next && (*z - self.view.zoom).abs() < 1e-3 {
                return;
            }
        }
        let size = doc.page_size_pts(next);
        let ppp = ctx.pixels_per_point();
        let target_w = size[0] * self.view.zoom * ppp;
        if let Ok(rendered) = doc.render_page(next, target_w, 4096.0 * ppp) {
            let img = egui::ColorImage::from_rgba_unmultiplied(
                [rendered.width, rendered.height],
                &rendered.rgba,
            );
            self.prefetch = Some((
                next,
                self.view.zoom,
                ctx.load_texture("prefetch", img, egui::TextureOptions::LINEAR),
            ));
        }
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
                                .add_enabled(
                                    !self.zoom_lock,
                                    egui::Button::new(icon_text(
                                        ui,
                                        "",
                                        icons::MAGNIFYING_GLASS_MINUS,
                                    )),
                                )
                                .on_hover_text("Zoom out (locked: press the lock or Ctrl+L)")
                                .clicked()
                            {
                                self.zoom_by(1.0 / 1.25);
                            }
                            ui.label(format!("{:.0}%", self.view.zoom / ZOOM_100_PERCENT * 100.0));
                            if ui
                                .add_enabled(
                                    !self.zoom_lock,
                                    egui::Button::new(icon_text(
                                        ui,
                                        "",
                                        icons::MAGNIFYING_GLASS_PLUS,
                                    )),
                                )
                                .on_hover_text("Zoom in (locked: press the lock or Ctrl+L)")
                                .clicked()
                            {
                                self.zoom_by(1.25);
                            }
                            // 줌 잠금 토글 — 실수로 줌이 바뀌는 것을 방지합니다.
                            let lock_icon = if self.zoom_lock {
                                icons::LOCK_SIMPLE
                            } else {
                                icons::LOCK_SIMPLE_OPEN
                            };
                            if ui
                                .selectable_label(
                                    self.zoom_lock,
                                    icon_text(ui, "", lock_icon),
                                )
                                .on_hover_text(
                                    if self.zoom_lock {
                                        "Zoom locked — click to unlock (Ctrl+L)"
                                    } else {
                                        "Lock zoom in/out (Ctrl+L)"
                                    },
                                )
                                .clicked()
                            {
                                self.zoom_lock = !self.zoom_lock;
                                if self.zoom_lock {
                                    if let Some(t) = self.zoom_target {
                                        self.view.zoom = t.clamp(MIN_ZOOM, MAX_ZOOM);
                                        self.render_dirty = true;
                                    }
                                    self.zoom_target = None;
                                    self.zoom_anchor_page = None;
                                    self.zoom_anchor_ui = None;
                                }
                                self.save_default_session();
                                self.save_session();
                            }
                            ui.separator();
                            if ui
                                .add_enabled(
                                    !self.zoom_lock,
                                    egui::Button::new(icon_text(
                                        ui,
                                        "Fit Width",
                                        icons::ARROWS_HORIZONTAL,
                                    )),
                                )
                                .on_hover_text("Fit width")
                                .clicked()
                            {
                                self.fit_width();
                            }
                            if ui
                                .add_enabled(
                                    !self.zoom_lock,
                                    egui::Button::new(icon_text(
                                        ui,
                                        "Fit Height",
                                        icons::ARROWS_VERTICAL,
                                    )),
                                )
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
                        let full = self.favorite_colors.len() >= MAX_FAVORITE_COLORS;
                        if ui
                            .add_enabled(
                                !full,
                                egui::Button::new(icon_text(ui, "", icons::PLUS)).frame(false),
                            )
                            .on_hover_text(if full {
                                "Palette is full (3 colors) — right-click a swatch to remove one first"
                            } else {
                                "Add current color to favorites"
                            })
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
            if !self.favorite_colors.contains(&c) && self.favorite_colors.len() < MAX_FAVORITE_COLORS {
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
    ///
    /// The line **color and thickness are per-page settings** (`PagePaper`):
    /// thickness is stored in page points and scaled with the zoom so it stays
    /// proportional to the page, like real printed ruling.
    pub(crate) fn paint_paper(&self, painter: &egui::Painter, origin: Pos2) {
        let w = self.page_size_pts[0];
        let h = self.page_size_pts[1];
        let paper = self.current_page_paper();
        let style = paper.style;
        let spacing = paper.spacing;
        let line = Color32::from_rgba_unmultiplied(
            paper.line_color[0],
            paper.line_color[1],
            paper.line_color[2],
            paper.line_color[3],
        );
        let zoom = self.view.zoom;
        let stroke_w = (paper.line_width * zoom).clamp(0.5, 24.0);
        let dot_r = (paper.line_width * zoom * 0.4).clamp(0.6, 8.0);
        for [x0, y0, x1, y1] in paper_lines(w, h, style, spacing) {
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

        // 줌 잠금이면 모든 줌 입력(핀치/Ctrl+휠/트랙패드)을 무시합니다.
        if !self.zoom_lock {

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
                                // 새 스트로크 시작: 스무딩 필터를 리셋해
                                // 이전 획과 섞이지 않게 합니다.
                                let sm = OneEuroFilter::from_smoothing(self.smoothing);
                                self.smooth_x = sm;
                                self.smooth_y = sm;
                                self.smooth_p = sm;
                                self.smooth_active = true;
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
                                // 만년필 모델은 점별 시각으로 속도를 계산합니다.
                                let t_ms = now_ms();
                                // 1€ 필터(선택적) — OTD 같은 드라이버가 이미
                                // 안정화하는 환경에서는 꺼둘 수 있습니다.
                                if self.smoothing_enabled
                                    && self.smoothing > 0.001
                                    && self.smooth_active
                                {
                                    let t = ctx.input(|i| i.time);
                                    let sx = self.smooth_x.filter(page[0], t);
                                    let sy = self.smooth_y.filter(page[1], t);
                                    let sp = self.smooth_p.filter(pressure, t);
                                    st.push([sx, sy], sp.clamp(0.0, 1.0), t_ms);
                                } else {
                                    st.push(page, pressure, t_ms);
                                }
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

    pub(crate) fn paint_active(&self, painter: &egui::Painter, active: &ActiveStroke, origin: Pos2) {
        let stroke = freedf_core::model::Stroke {
            id: 0,
            tool: active.tool,
            color: active.color,
            width: active.width,
            points: active.points.clone(),
            created_ms: now_ms(),
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
        // 점별 화면(뷰) 좌표 — 줌이 두 번 적용되지 않도록 뷰 공간에서 계산.
        let pts_view: Vec<[f32; 2]> = pts
            .iter()
            .map(|p| {
                let v = self.view.page_to_view([p.x, p.y]);
                [origin.x + v[0], origin.y + v[1]]
            })
            .collect();
        // 점별 절반 두께 (화면 px).
        let n = pts.len();
        let mut halves: Vec<f32> = Vec::with_capacity(n);
        let round_caps = matches!(stroke.tool, ToolType::Pen | ToolType::Fountain);
        if stroke.tool == ToolType::Highlighter {
            // 마커: 필압/테이퍼 없이 일정한 두께.
            let h = (stroke.width * zoom * 0.5).max(0.5);
            halves.resize(n, h);
        } else if stroke.tool == ToolType::Fountain {
            // 만년필: 필압 × 속도 × 기울기 물리 모델. 점별 폭(pt)을
            // 모델로 계산해 화면 px로 환산합니다.
            let tilt = tilt_magnitude(&self.pen_tilt);
            let widths = self.fountain_profile.widths(stroke.width, pts, tilt);
            for w in widths {
                halves.push((w * zoom * 0.5).max(0.3));
            }
        } else {
            // 일반 펜: 필압·속도 영향이 작은 물리 모델 (선폭 변동폭이 좁음).
            let tilt = tilt_magnitude(&self.pen_tilt);
            let widths = self.pen_profile.widths(stroke.width, pts, tilt);
            for w in widths {
                halves.push((w * zoom * 0.5).max(0.3));
            }
        }
        let is_pen = stroke.tool == ToolType::Pen;
        // 잉크 번짐(블리드) 후광: 잉크 아래에 반투명 레이어 2겹을 깔아
        // 종이로 잉크가 퍼진 것처럼 보이게 합니다. 각 레이어는 겹침 없는
        // 삼각분할이라 얼룩이 없습니다.
        if is_pen && self.ink_bleed.enabled {
            let age_sec = if stroke.created_ms == 0 {
                0.0
            } else {
                (now_ms().saturating_sub(stroke.created_ms)) as f32 / 1000.0
            };
            // 획 길이 + 점별 시작/끝 거리.
            let len: f32 = pts
                .windows(2)
                .map(|w| ((w[1].x - w[0].x).powi(2) + (w[1].y - w[0].y).powi(2)).sqrt())
                .sum();
            let mut dists: Vec<(f32, f32)> = Vec::with_capacity(n); // (d0, d1)
            let mut acc = 0.0f32;
            for i in 0..n {
                if i > 0 {
                    let a = &pts[i - 1];
                    let b = &pts[i];
                    acc += ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
                }
                dists.push((acc, len - acc));
            }
            let radii: Vec<f32> = dists
                .iter()
                .map(|(d0, d1)| self.ink_bleed.radius(*d0, *d1, len, age_sec))
                .collect();
            // (추가 반경 비율, 알파 배율): 안쪽이 진하고 바깥이 옅은 2단.
            for (extra, alpha_k) in [(0.5f32, 0.35f32), (1.0, 0.12)] {
                let mut hb: Vec<f32> = Vec::with_capacity(n);
                for i in 0..n {
                    hb.push(halves[i] + radii[i] * extra);
                }
                let alpha = (color.a() as f32 * alpha_k).clamp(0.0, 255.0) as u8;
                let halo = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha);
                self.paint_filled_stroke(painter, &pts_view, &hb, true, halo);
            }
        }
        // 본체: 펜/만년필은 둥근 캡, 마커는 직선(butt) 끝 — 관례적 구분.
        self.paint_filled_stroke(painter, &pts_view, &halves, round_caps, color);
    }

    /// 외곽선 → 삼각분할 → 채움. 모든 삼각형이 겹치지 않아 반투명 색도
    /// 균일하게 칠해집니다.
    ///
    /// 주의: egui의 `Shape::convex_polygon`은 **페더링 테셀레이터가 슬리버
    /// (가늘고 긴) 삼각형에서 점 법선을 폭발**시켜 정점이 화면 밖으로 튀는
    /// "빛살(starburst)" 깨짐이 있었습니다. 그래서 여기서는 삼각분할 결과를
    /// **Mesh로 직접 구성**하고, AA 페더링도 경계 가장자리에만 우리가 직접
    /// 만듭니다 (내부 공유 가장자리는 제외 → 반투명 잉크의 이음새 얼룩 없음).
    fn paint_filled_stroke(
        &self,
        painter: &egui::Painter,
        pts_view: &[[f32; 2]],
        half_widths: &[f32],
        round_caps: bool,
        color: Color32,
    ) {
        let poly = freedf_core::pen::stroke_outline(pts_view, half_widths, round_caps);
        let tris = freedf_core::pen::triangulate_polygon(&poly);
        if tris.is_empty() {
            return;
        }
        let (fill, aa) = build_stroke_meshes(&poly, &tris, color, 1.0);
        painter.add(egui::Shape::mesh(fill));
        if !aa.is_empty() {
            painter.add(egui::Shape::mesh(aa));
        }
    }

    /// Draws a custom cursor sprite confined to the canvas, previewing the
    /// current tool's shape and color (Pen = translucent gray circle,
    /// Highlighter = colored rectangle, Eraser = white translucent circle).
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
                        let r = (self.pen_width * self.view.zoom * 0.5).clamp(2.5, 16.0);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 외곽선 → 삼각분할 → 메시 구성 전체를 돌려, 모든 정점이 유한하고
    /// 스트로크 근처에 머무는지 확인합니다 ("빛살/starburst" 깨짐 회귀 테스트).
    fn mesh_bounds(pts: &[[f32; 2]], halves: &[f32], round: bool) -> egui::Rect {
        let poly = freedf_core::pen::stroke_outline(pts, halves, round);
        let tris = freedf_core::pen::triangulate_polygon(&poly);
        let (fill, aa) = build_stroke_meshes(&poly, &tris, Color32::RED, 1.0);
        let mut bounds = egui::Rect::NOTHING;
        for mesh in [&fill, &aa] {
            for v in &mesh.vertices {
                assert!(v.pos.x.is_finite() && v.pos.y.is_finite(), "NaN 정점: {:?}", v.pos);
                bounds.extend_with(v.pos);
            }
            assert!(!mesh.indices.is_empty(), "빈 메시");
        }
        bounds
    }

    #[test]
    fn straight_stroke_mesh_stays_bounded() {
        let pts: Vec<[f32; 2]> = (0..24).map(|i| [20.0 + i as f32 * 8.0, 80.0]).collect();
        let halves: Vec<f32> = vec![1.5; 24];
        // 직선 렌즈도 본체가 빠지지 않아야 함 (면적 기준 완전 커버).
        let poly = freedf_core::pen::stroke_outline(&pts, &halves, true);
        let tris = freedf_core::pen::triangulate_polygon(&poly);
        let area: f32 = tris.iter().fold(0.0, |acc, t| {
            let (a, b, c) = (
                poly[t[0] as usize],
                poly[t[1] as usize],
                poly[t[2] as usize],
            );
            acc + ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() * 0.5
        });
        let expected = 184.0 * 3.0 + std::f32::consts::PI * 1.5 * 1.5;
        assert!((area - expected).abs() < 1.0, "직선 렌즈 면적: {area} vs {expected}");
        let b = mesh_bounds(&pts, &halves, true);
        assert!(b.min.x > 10.0 && b.max.x < 210.0, "x 경계: {b:?}");
        assert!(b.min.y > 70.0 && b.max.y < 90.0, "y 경계: {b:?}");
    }

    #[test]
    fn scribble_cluster_mesh_stays_bounded() {
        // 스크린샷과 같은 밀집 클러스터 + 급격한 루프 입력.
        let mut pts: Vec<[f32; 2]> = Vec::new();
        let mut t = 0.0f32;
        while t < std::f32::consts::TAU * 3.0 {
            let r = 20.0 + 8.0 * (t * 3.0).sin();
            pts.push([100.0 + r * t.cos(), 100.0 + r * t.sin() * 0.7]);
            t += 0.08;
        }
        let halves: Vec<f32> = pts
            .iter()
            .enumerate()
            .map(|(i, _)| 0.4 + 2.0 * (i as f32 / pts.len() as f32))
            .collect();
        let b = mesh_bounds(&pts, &halves, true);
        assert!(b.min.x > 50.0 && b.max.x < 150.0, "x 경계: {b:?}");
        assert!(b.min.y > 50.0 && b.max.y < 150.0, "y 경계: {b:?}");
    }

    #[test]
    fn duplicate_points_mesh_stays_bounded() {
        // 펜을 누른 채 정지(중복 점) → 필압 램프.
        let mut pts: Vec<[f32; 2]> = Vec::new();
        for _ in 0..6 {
            pts.push([50.0, 50.0]);
        }
        for i in 0..16 {
            pts.push([50.0 + i as f32 * 6.0, 50.0]);
        }
        let halves: Vec<f32> = (0..pts.len())
            .map(|i| 0.4 + 2.0 * (i as f32 / pts.len() as f32))
            .collect();
        let b = mesh_bounds(&pts, &halves, true);
        assert!(b.min.x > 30.0 && b.max.x < 160.0, "x 경계: {b:?}");
        assert!(b.min.y > 30.0 && b.max.y < 70.0, "y 경계: {b:?}");
    }
}
