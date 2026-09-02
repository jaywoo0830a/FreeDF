//! Three-tier toolbar: panel/page toggles, drawing-tool picker with drag-to-reorder, and per-tool settings.
//!
//! Extracted from `app.rs` during the layered refactor — these are
//! `FreeDfApp` methods living in a submodule (`use super::*` re-exports the
//! app state types and shared helpers).

use super::*;

/// 일반 펜(볼펜) 물리 모델의 실제 결과를 보여주는 미니 스트로크 미리보기.
fn pen_profile_preview(
    ui: &mut egui::Ui,
    color: Color32,
    width: f32,
    profile: &BallPenProfile,
) {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(150.0, 34.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 4.0, ui.visuals().faint_bg_color);
    let n = 48;
    let x0 = rect.left() + 3.0;
    let x1 = rect.right() - 3.0;
    let cy = rect.center().y;
    let amp = rect.height() * 0.30;
    // 필압 물결 + 속도 변화(느림→빠름)를 재현한 가상 스트로크.
    let mut pts: Vec<StrokePoint> = Vec::with_capacity(n);
    let step = (x1 - x0) / (n - 1) as f32;
    let mut t_ms = 0u64;
    for i in 0..n {
        let t = i as f32 / (n - 1) as f32;
        let x = x0 + (x1 - x0) * t;
        let y = cy + (t * 2.0 * std::f32::consts::PI).sin() * amp;
        let speed = 60.0 + 340.0 * (std::f32::consts::PI * t).sin().powi(2);
        if i > 0 {
            t_ms += (step / speed.max(1.0) * 1000.0) as u64;
        }
        let pressure = 0.3 + 0.7 * (t * 4.0 * std::f32::consts::PI).sin().abs();
        pts.push(StrokePoint::with_time(x, y, pressure, t_ms));
    }
    let widths = profile.widths(width, &pts, 0.0);
    let max_w = widths.iter().cloned().fold(0.5f32, f32::max);
    let scale = (rect.height() * 0.40 / max_w).clamp(0.3, 1.6);
    for i in 0..n - 1 {
        let wpx = (widths[i] + widths[i + 1]) * 0.5 * scale;
        let a = egui::pos2(pts[i].x, pts[i].y);
        let b = egui::pos2(pts[i + 1].x, pts[i + 1].y);
        painter.line_segment([a, b], Stroke::new(wpx, color));
    }
    let _ = resp.on_hover_text(
        "Live preview of the ballpen model:\nwidth = f(pressure, speed) — gentle, narrow range",
    );
}

/// 만년필 물리 모델의 실제 결과를 보여주는 미니 스트로크 미리보기.
/// 느린 시작(굵게) → 빠른 중간(가늘게) → 정지(잉크 고임)를 재현합니다.
fn fountain_profile_preview(
    ui: &mut egui::Ui,
    color: Color32,
    max_width: f32,
    profile: &FountainProfile,
) {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(150.0, 34.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 4.0, ui.visuals().faint_bg_color);
    let n = 48;
    let x0 = rect.left() + 3.0;
    let x1 = rect.right() - 3.0;
    let cy = rect.center().y;
    let amp = rect.height() * 0.30;
    // 가상 스트로크: 양끝 느림(굵게)·중간 빠름(가늘게), 필압 물결.
    let mut pts: Vec<StrokePoint> = Vec::with_capacity(n);
    let step = (x1 - x0) / (n - 1) as f32;
    let mut t_ms = 0u64;
    for i in 0..n {
        let t = i as f32 / (n - 1) as f32;
        let x = x0 + (x1 - x0) * t;
        let y = cy + (t * 2.0 * std::f32::consts::PI).sin() * amp;
        // 속도 프로파일: 양끝 20, 중앙 400 (px/초) → dt = step/speed.
        let speed = 20.0 + 380.0 * (std::f32::consts::PI * t).sin().powi(2);
        if i > 0 {
            t_ms += (step / speed.max(1.0) * 1000.0) as u64;
        }
        let pressure = 0.3 + 0.7 * (t * 4.0 * std::f32::consts::PI).sin().abs();
        pts.push(StrokePoint::with_time(x, y, pressure, t_ms));
    }
    let widths = profile.widths(max_width, &pts, 0.0);
    let max_w = widths.iter().cloned().fold(0.5f32, f32::max);
    let scale = (rect.height() * 0.40 / max_w).clamp(0.3, 1.6);
    for i in 0..n - 1 {
        let wpx = (widths[i] + widths[i + 1]) * 0.5 * scale;
        let a = egui::pos2(pts[i].x, pts[i].y);
        let b = egui::pos2(pts[i + 1].x, pts[i + 1].y);
        painter.line_segment([a, b], Stroke::new(wpx, color));
    }
    let _ = resp.on_hover_text(
        "Live preview of the fountain model:\nwidth = f(pressure × speed × tilt) + dwell blob",
    );
}

impl FreeDfApp {
    pub(crate) fn toolbar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("toolbar").show(ui, |ui| {
            // Compact spacing + padding; uniform control height for tidy rows
            ui.spacing_mut().button_padding = egui::vec2(9.0, 5.0);
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);
            ui.spacing_mut().interact_size = egui::vec2(0.0, 28.0);
            ui.add_space(4.0);
            // Row 1: panels / page tools / ink tools
            toolbar_row(ui, |ui| {
                ui.horizontal(|ui| {
                if ui
                    .toggle_value(&mut self.show_library, icon_text(ui, "Library", icons::NOTEBOOK))
                    .on_hover_text("Library (notes, PDFs, recents)")
                    .changed()
                {
                    // Zoom is preserved; the canvas re-centers on resize.
                    self.save_session();
                }
                if ui
                    .toggle_value(&mut self.show_outline, icon_text(ui, "Outline", icons::LIST_BULLETS))
                    .on_hover_text("Outline")
                    .changed()
                {
                    // Zoom is preserved; the canvas re-centers on resize.
                    self.save_session();
                }
                if ui
                    .toggle_value(&mut self.show_palette, icon_text(ui, "Palette", icons::PALETTE))
                    .on_hover_text("Writing-tool color palette (right side of canvas)")
                    .changed()
                {
                    self.save_default_session();
                }
                if ui
                    .toggle_value(
                        &mut self.dictionary.enabled,
                        icon_text(ui, "Dictionary", icons::BOOK_OPEN_TEXT),
                    )
                    .on_hover_text(
                        "Tap any word on the page to look it up in the dictionary.\n\
                         Needs internet once per word; results are cached in the database.",
                    )
                    .changed()
                {
                    self.save_default_session();
                }
                ui.separator();

                // Bookmark the current page + jump list.
                let bookmarked = self.store.is_bookmarked(self.current_page);
                if ui
                    .selectable_label(
                        bookmarked,
                        icon_text(ui, "Bookmark", icons::BOOKMARK_SIMPLE),
                    )
                    .on_hover_text(if bookmarked {
                        "Remove bookmark from this page"
                    } else {
                        "Bookmark this page"
                    })
                    .clicked()
                {
                    self.toggle_bookmark(self.current_page);
                }
                ui.menu_button(icon_text(ui, "Bookmarks", icons::BOOKMARKS_SIMPLE), |ui| {
                    let pages: Vec<PageIndex> = self.store.bookmarks().to_vec();
                    if pages.is_empty() {
                        ui.label("No bookmarks yet");
                        return;
                    }
                    for p in pages {
                        if ui.button(format!("Page {}", p + 1)).clicked() {
                            ui.close();
                            self.goto_page(p);
                        }
                    }
                    ui.separator();
                    if ui.button("Clear all bookmarks").clicked() {
                        self.clear_bookmarks();
                    }
                });
                ui.separator();

                if !self.show_library && !self.show_outline {
                    // With the side panels collapsed the canvas is wide, so let
                    // the page be aligned left / center / right.
                    ui.separator();
                    let aligns = [
                        (PageAlign::Left, icons::TEXT_ALIGN_LEFT, "Align left"),
                        (PageAlign::Center, icons::TEXT_ALIGN_CENTER, "Align center"),
                        (PageAlign::Right, icons::TEXT_ALIGN_RIGHT, "Align right"),
                    ];
                    for (a, ic, hint) in aligns {
                        if ui
                            .selectable_label(self.page_align == a, icon_text(ui, "", ic))
                            .on_hover_text(hint)
                            .clicked()
                        {
                            self.page_align = a;
                            self.realign();
                            self.save_session();
                        }
                    }
                }
                ui.separator();

                if ui
                    .add_enabled(
                        self.history.can_undo(),
                        egui::Button::new(icon_text(ui, "Undo", icons::ARROW_COUNTER_CLOCKWISE)),
                    )
                    .on_hover_text("Undo (Ctrl+Z)")
                    .clicked()
                {
                    self.undo();
                }
                if ui
                    .add_enabled(
                        self.history.can_redo(),
                        egui::Button::new(icon_text(ui, "Redo", icons::ARROW_CLOCKWISE)),
                    )
                    .on_hover_text("Redo (Ctrl+Y)")
                    .clicked()
                {
                    self.redo();
                }
                if ui
                    .button(icon_text(ui, "Clear", icons::X_CIRCLE))
                    .on_hover_text("Clear page")
                    .clicked()
                {
                    self.clear_page();
                }
                ui.separator();

                if ui
                    .button(icon_text(ui, "Save", icons::FLOPPY_DISK))
                    .on_hover_text("Save annotations (Ctrl+S)")
                    .clicked()
                {
                    self.save_annotations();
                }
                if ui
                    .button(icon_text(ui, "Load", icons::FOLDER_SIMPLE))
                    .on_hover_text("Load annotations")
                    .clicked()
                {
                    self.load_annotations();
                }
                ui.menu_button(icon_text(ui, "Export", icons::IMAGE), |ui| {
                    if ui.button("Export as PNG").clicked() {
                        ui.close();
                        self.export_with_format(ExportFormat::Png);
                    }
                    if ui.button("Export as JPG").clicked() {
                        ui.close();
                        self.export_with_format(ExportFormat::Jpg);
                    }
                    if ui.button("Export as PDF").clicked() {
                        ui.close();
                        self.export_with_format(ExportFormat::Pdf);
                    }
                })
                .response
                .on_hover_text("Export current page as PNG / JPG / PDF (Ctrl+E = PNG)");
                });
            });

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(2.0);

            // Row 2: Page (structure + paper styling)
            toolbar_row(ui, |ui| {
                ui.horizontal(|ui| {
                ui.label(icon_text(ui, "Page", icons::FILES));
                let page_count = self.document.as_ref().map(|d| d.page_count()).unwrap_or(0);
                ui.menu_button(icon_text(ui, "Insert Page", icons::PLUS_SQUARE), |ui| {
                    let insert = [
                        (InsertTarget::FromCurrent, "From current page"),
                        (InsertTarget::AtVeryFront, "At the very front"),
                        (InsertTarget::AtVeryBack, "At the very back"),
                        (InsertTarget::BeforeCurrent, "Before current page"),
                        (InsertTarget::AfterCurrent, "After current page"),
                    ];
                    for (target, label) in insert {
                        if ui
                            .add_enabled(page_count > 0, egui::Button::new(label))
                            .clicked()
                        {
                            ui.close();
                            self.insert_page_action(target);
                        }
                    }
                })
                .response
                .on_hover_text("Insert a blank page");
                ui.menu_button(icon_text(ui, "Rotate", icons::REPEAT), |ui| {
                    if ui
                        .add_enabled(page_count > 0, egui::Button::new("Rotate current page CW"))
                        .clicked()
                    {
                        ui.close();
                        self.rotate_page_action(true);
                    }
                    if ui
                        .add_enabled(
                            page_count > 0,
                            egui::Button::new("Rotate current page CCW"),
                        )
                        .clicked()
                    {
                        ui.close();
                        self.rotate_page_action(false);
                    }
                    if ui
                        .add_enabled(page_count > 0, egui::Button::new("Rotate all pages CW"))
                        .clicked()
                    {
                        ui.close();
                        self.rotate_all_pages_action(true);
                    }
                    if ui
                        .add_enabled(page_count > 0, egui::Button::new("Rotate all pages CCW"))
                        .clicked()
                    {
                        ui.close();
                        self.rotate_all_pages_action(false);
                    }
                })
                .response
                .on_hover_text("Rotate pages (CW = clockwise)");
                if ui
                    .add_enabled(
                        page_count > 1,
                        egui::Button::new(icon_text(ui, "Delete", icons::TRASH_SIMPLE)),
                    )
                    .on_hover_text("Delete this page")
                    .clicked()
                {
                    self.delete_page_action();
                }
                ui.separator();

                // Paper (grid / ruling / color) — applied to the **current
                // page**; new pages use these values as their defaults.
                // "Apply to all" pushes the current values onto every page.
                ui.label(icon_text(ui, "Paper", icons::NOTEBOOK));
                egui::ComboBox::from_id_salt("paper_style")
                    .selected_text(self.paper_style.label())
                    .show_ui(ui, |ui| {
                        for style in PaperStyle::all() {
                            let changed = ui
                                .selectable_value(&mut self.paper_style, style, style.label())
                                .changed();
                            if changed {
                                self.apply_paper_to_current_page();
                                self.save_default_session();
                                self.save_session();
                            }
                        }
                    })
                    .response
                    .on_hover_text(
                        "Paper style for the current page.\n\
                         New pages & new notes use it as their default.",
                    );
                for (i, paper) in PAPER_COLORS.iter().enumerate() {
                    let color =
                        Color32::from_rgba_unmultiplied(paper[0], paper[1], paper[2], paper[3]);
                    let selected = self.paper_color == *paper;
                    if color_circle_swatch(ui, ("paper_swatch", i), color, selected)
                        .on_hover_text("Paper color (current page)")
                        .clicked()
                    {
                        self.paper_color = *paper;
                        self.apply_paper_to_current_page();
                        self.save_default_session();
                        self.save_session();
                    }
                }
                // 프리셋 5색 외에 원하는 배경색을 직접 고릅니다.
                let mut paper_color = Color32::from_rgba_unmultiplied(
                    self.paper_color[0],
                    self.paper_color[1],
                    self.paper_color[2],
                    self.paper_color[3],
                );
                if ui
                    .color_edit_button_srgba(&mut paper_color)
                    .on_hover_text("Custom paper color (current page)")
                    .changed()
                {
                    self.paper_color = paper_color.to_array();
                    self.apply_paper_to_current_page();
                    self.save_default_session();
                    self.save_session();
                }
                egui::ComboBox::from_id_salt("paper_size")
                    .selected_text(self.paper_size.label())
                    .show_ui(ui, |ui| {
                        for size in PaperSize::all() {
                            let changed = ui
                                .selectable_value(&mut self.paper_size, size, size.label())
                                .changed();
                            if changed {
                                self.save_default_session();
                                self.save_session();
                            }
                        }
                    })
                    .response
                    .on_hover_text(
                        "Size of new pages & new notes (existing pages keep their size).",
                    );
                // 사용자 정의 크기: mm 단위 숫자 입력.
                if self.paper_size == PaperSize::Custom {
                    const MM_TO_PT: f32 = 72.0 / 25.4;
                    let mut w_mm = self.custom_paper_size[0] / MM_TO_PT;
                    let mut h_mm = self.custom_paper_size[1] / MM_TO_PT;
                    let w_changed = ui
                        .add(
                            egui::DragValue::new(&mut w_mm)
                                .range(50.0..=1200.0)
                                .speed(1.0)
                                .prefix("W "),
                        )
                        .on_hover_text("Custom page width (mm)")
                        .changed();
                    let h_changed = ui
                        .add(
                            egui::DragValue::new(&mut h_mm)
                                .range(50.0..=1200.0)
                                .speed(1.0)
                                .prefix("H "),
                        )
                        .on_hover_text("Custom page height (mm)")
                        .changed();
                    if w_changed || h_changed {
                        self.custom_paper_size =
                            [(w_mm * MM_TO_PT).clamp(100.0, 3400.0), (h_mm * MM_TO_PT).clamp(100.0, 3400.0)];
                        self.save_default_session();
                        self.save_session();
                    }
                }
                // 줄/격자 간격 (숫자 직접 입력).
                if ui
                    .add(
                        egui::DragValue::new(&mut self.paper_spacing)
                            .range(12.0..=120.0)
                            .speed(1.0)
                            .prefix("Spacing ")
                            .suffix("pt"),
                    )
                    .on_hover_text("Ruled/Grid/Dotted spacing applied to the current page")
                    .changed()
                {
                    self.paper_spacing = clamp_spacing(self.paper_spacing);
                    self.apply_paper_to_current_page();
                    self.save_default_session();
                    self.save_session();
                }
                // 줄/격자/점 색과 두께 (페이지에 바로 적용, 새 페이지 기본값으로 기억).
                let mut line_color = Color32::from_rgba_unmultiplied(
                    self.paper_line_color[0],
                    self.paper_line_color[1],
                    self.paper_line_color[2],
                    self.paper_line_color[3],
                );
                if ui
                    .color_edit_button_srgba(&mut line_color)
                    .on_hover_text("Line color (ruled / grid / dotted)")
                    .changed()
                {
                    self.paper_line_color =
                        [line_color.r(), line_color.g(), line_color.b(), line_color.a()];
                    self.apply_paper_to_current_page();
                    self.save_default_session();
                    self.save_session();
                }
                if ui
                    .add(
                        egui::DragValue::new(&mut self.paper_line_width)
                            .range(0.25..=8.0)
                            .speed(0.05)
                            .fixed_decimals(2)
                            .prefix("Line ")
                            .suffix("pt"),
                    )
                    .on_hover_text("Line thickness (ruled / grid / dotted)")
                    .changed()
                {
                    self.paper_line_width = clamp_line_width(self.paper_line_width);
                    self.apply_paper_to_current_page();
                    self.save_default_session();
                    self.save_session();
                }
                // 현재 툴바의 Paper 설정을 문서의 모든 페이지에 복사합니다.
                if ui
                    .add_enabled(
                        page_count > 0,
                        egui::Button::new(icon_text(ui, "Apply to all", icons::CHECK_SQUARE_OFFSET)),
                    )
                    .on_hover_text(
                        "Copy these paper settings (style/color/spacing/line) \
                         onto every page of this document.",
                    )
                    .clicked()
                {
                    self.apply_paper_to_all_pages();
                }
                });
            });

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(2.0);

            // Row 3: drawing tools (drag to reorder) + settings
            toolbar_row(ui, |ui| {
                ui.horizontal(|ui| {
                // ── 도구 선택기 (드래그 앤 드롭 재정렬) ─────────────
                let mut order = self.tool_order.clone();
                let mut rects: Vec<egui::Rect> = Vec::with_capacity(order.len());
                let mut src = self.tool_drag;
                let mut dst = self.tool_drop;
                for (i, tool) in order.iter().copied().enumerate() {
                    let label = tool.label();
                    let selected = self.tool == tool;
                    let btn = egui::Button::new(icon_text(ui, "", tool_icon(tool))).selected(selected);
                    let resp = ui
                        .add(btn.sense(egui::Sense::click_and_drag()))
                        .on_hover_text(format!("{label}  (drag to reorder)"));
                    rects.push(resp.rect);
                    if resp.clicked() {
                        self.tool = tool;
                        self.save_session();
                    }
                    if resp.drag_started() {
                        src = Some(i);
                        dst = Some(i);
                    }
                    if resp.dragged() && src == Some(i) {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                    }
                }
                // 드래그 중 포인터가 놓인 버튼을 드롭 대상으로 지정.
                if let Some(_s) = src {
                    if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
                        if let Some(idx) = rects.iter().position(|r| r.contains(pos)) {
                            dst = Some(idx);
                        }
                    }
                }
                // 놓으면 순서 이동 + 저장.
                let down = ui.input(|i| i.pointer.any_down());
                if src.is_some() && !down {
                    if let (Some(s), Some(d)) = (src, dst) {
                        if s != d && s < order.len() && d < order.len() {
                            let item = order.remove(s);
                            order.insert(d, item);
                            self.tool_order = order;
                            self.save_default_session();
                            self.save_session();
                        }
                    }
                    src = None;
                    dst = None;
                }
                self.tool_drag = src;
                self.tool_drop = dst;
                // 드롭 대상 표시 (작은 캐럿)
                if let Some(d) = self.tool_drop {
                    if self.tool_drag.is_some() {
                        if let Some(r) = rects.get(d) {
                            let x = r.left();
                            let y0 = r.top();
                            let y1 = r.bottom();
                            ui.painter().line_segment(
                                [egui::pos2(x, y0), egui::pos2(x, y1)],
                                egui::Stroke::new(
                                    2.0,
                                    egui::Color32::from_rgba_unmultiplied(255, 200, 60, 220),
                                ),
                            );
                        }
                    }
                }
                ui.separator();

                match self.tool {
                    ToolType::Pen => {
                        egui::ComboBox::from_id_salt("family")
                            .selected_text(self.color_family.label())
                            .show_ui(ui, |ui| {
                                for family in ColorFamily::all() {
                                    if ui
                                        .selectable_value(
                                            &mut self.color_family,
                                            family,
                                            family.label(),
                                        )
                                        .changed()
                                    {
                                        self.save_session();
                                    }
                                }
                            });
                        let swatches = Palette::swatches(self.color_family);
                        // Round color swatches forming a neat color bar.
                        for (i, swatch) in swatches.iter().enumerate() {
                            let color = Color32::from_rgba_unmultiplied(
                                swatch[0],
                                swatch[1],
                                swatch[2],
                                swatch[3],
                            );
                            let selected = *swatch == self.pen_color;
                            if color_circle_swatch(ui, ("pen_swatch", i), color, selected)
                                .on_hover_text("Pen color")
                                .clicked()
                            {
                                self.pen_color = *swatch;
                                self.save_default_session();
                                self.save_session();
                            }
                        }
                        // 계열 스와치 외의 원하는 색을 직접 고릅니다.
                        let mut pen_color = Color32::from_rgba_unmultiplied(
                            self.pen_color[0],
                            self.pen_color[1],
                            self.pen_color[2],
                            self.pen_color[3],
                        );
                        if ui
                            .color_edit_button_srgba(&mut pen_color)
                            .on_hover_text("Custom pen color")
                            .changed()
                        {
                            self.pen_color = pen_color.to_array();
                            self.save_default_session();
                            self.save_session();
                        }
                        // Width 슬라이더 툴팁.
                        let width_resp = ui
                            .add(egui::Slider::new(&mut self.pen_width, 0.5..=12.0).text("Width"))
                            .on_hover_text(
                                "Base line width (pt). The ballpen model varies it only \
                                 a little (±30%) by pressure & speed.",
                            );
                        if width_resp.changed() {
                            self.save_session();
                        }
                        // 미니 스트로크 미리보기: 실제 볼펜 모델 수식으로 그립니다.
                        let preview_color = Color32::from_rgba_unmultiplied(
                            self.pen_color[0],
                            self.pen_color[1],
                            self.pen_color[2],
                            self.pen_color[3],
                        );
                        pen_profile_preview(ui, preview_color, self.pen_width, &self.pen_profile);
                        egui::ComboBox::from_id_salt("pen_cursor_style")
                            .selected_text(self.pen_cursor_style.label())
                            .show_ui(ui, |ui| {
                                for style in PenCursorStyle::all() {
                                    ui.selectable_value(
                                        &mut self.pen_cursor_style,
                                        style,
                                        style.label(),
                                    );
                                }
                            })
                            .response
                            .on_hover_text("Pen cursor shape");
                        if ui
                            .checkbox(&mut self.pressure_enabled, "Pressure")
                            .on_hover_text(
                                "Use pen/tablet pressure. Off = always full pressure.",
                            )
                            .changed()
                        {
                            self.save_session();
                        }
                        if ui
                            .checkbox(&mut self.left_handed, "Left-handed")
                            .on_hover_text(
                                "Pen cursor barrel points to the LEFT half-plane\n\
                                 (right-handed = right half-plane).",
                            )
                            .changed()
                        {
                            self.save_default_session();
                            self.save_session();
                        }
                        if ui
                            .checkbox(&mut self.debug_hud, "Debug HUD")
                            .on_hover_text(
                                "Live input overlay: pressure, tilt, tip speed, tip width.\n\
                                 Use it to check what your device actually reports.",
                            )
                            .changed()
                        {
                            self.save_default_session();
                            self.save_session();
                        }
                        if ui
                            .checkbox(&mut self.mouse_draws, "Mouse ink")
                            .on_hover_text(
                                "Draw ink with the mouse/trackpad too.\n\
                                 Off (default): mouse & trackpad pan the page — \
                                 only a pen writes, like real note-taking apps.",
                            )
                            .changed()
                        {
                            self.save_default_session();
                            self.save_session();
                        }
                        // ── 일반 펜(볼펜) 물리 모델 파라미터 ──
                        let any_changed = ui
                            .add(
                                egui::Slider::new(
                                    &mut self.pen_profile.pressure_k,
                                    0.0..=0.5,
                                )
                                .text("Press k"),
                            )
                            .on_hover_text(
                                "Pressure influence (small for ballpens, 0.1~0.3).\n\
                                 0 = constant width.",
                            )
                            .changed()
                            | ui
                                .add(
                                    egui::Slider::new(&mut self.pen_profile.speed_k, 0.0..=0.3)
                                        .text("Speed k"),
                                )
                                .on_hover_text(
                                    "Speed influence (small, 0.05~0.15).\n\
                                     Fast strokes thin slightly.",
                                )
                                .changed()
                            | ui
                                .add(
                                    egui::Slider::new(
                                        &mut self.pen_profile.starve_v,
                                        200.0..=3000.0,
                                    )
                                    .text("Starve v"),
                                )
                                .on_hover_text(
                                    "Speed (pt/s) where ink starvation starts — above this \
                                     the line thins and breaks like a real ballpen.",
                                )
                                .changed()
                            | ui
                                .add(
                                    egui::Slider::new(&mut self.pen_profile.tilt_k, 0.0..=1.0)
                                        .text("Tilt"),
                                )
                                .on_hover_text(
                                    "Tilt influence (continuous, no threshold): laying the pen \
                                     down widens the line up to (1 + tilt_k) times.\n\
                                     egui/winit don't expose pen tilt — needs the HID/WM_POINTER \
                                     hook (`set_pen_tilt`) to feed values.",
                                )
                                .changed();
                        if any_changed {
                            self.save_default_session();
                            self.save_session();
                        }
                        // 필기 스무딩(안정화): 선택 기능. OTD(OpenTabletDriver) 등
                        // 드라이버가 이미 안정화하는 환경에서는 꺼두면 됩니다.
                        if ui
                            .checkbox(&mut self.smoothing_enabled, "Stabilize")
                            .on_hover_text(
                                "Optional tremor filtering (1€ filter).\n\
                                 Turn off if your tablet driver (e.g. OpenTabletDriver) \
                                 already smooths the input.",
                            )
                            .changed()
                        {
                            self.save_default_session();
                            self.save_session();
                        }
                        if self.smoothing_enabled
                            && ui
                                .add(
                                    egui::Slider::new(&mut self.smoothing, 0.0..=1.0)
                                        .text("Strength"),
                                )
                                .on_hover_text(
                                    "Filter strength while Stabilize is on.\n\
                                     0 = raw input, 1 = silky smooth.",
                                )
                                .changed()
                        {
                            self.smoothing = self.smoothing.clamp(0.0, 1.0);
                            self.save_default_session();
                            self.save_session();
                        }
                    }
                    ToolType::Fountain => {
                        // 만년필: 색/두께 모두 볼펜과 **완전히 독립**.
                        let swatches = Palette::swatches(self.color_family);
                        for (i, swatch) in swatches.iter().enumerate() {
                            let color = Color32::from_rgba_unmultiplied(
                                swatch[0],
                                swatch[1],
                                swatch[2],
                                swatch[3],
                            );
                            let selected = *swatch == self.fountain_color;
                            if color_circle_swatch(ui, ("fountain_swatch", i), color, selected)
                                .on_hover_text("Ink color")
                                .clicked()
                            {
                                self.fountain_color = *swatch;
                                self.save_default_session();
                                self.save_session();
                            }
                        }
                        let mut fountain_color = Color32::from_rgba_unmultiplied(
                            self.fountain_color[0],
                            self.fountain_color[1],
                            self.fountain_color[2],
                            self.fountain_color[3],
                        );
                        if ui
                            .color_edit_button_srgba(&mut fountain_color)
                            .on_hover_text("Custom ink color")
                            .changed()
                        {
                            self.fountain_color = fountain_color.to_array();
                            self.save_default_session();
                            self.save_session();
                        }
                        let nib_resp = ui
                            .add(
                                egui::Slider::new(&mut self.fountain_width, 0.5..=12.0)
                                    .text("Nib"),
                            )
                            .on_hover_text(
                                "Nib width = maximum line width (pt).\n\
                                 The model varies it by pressure, speed and tilt.",
                            );
                        if nib_resp.changed() {
                            self.save_session();
                        }
                        // 잉크 번짐(블리드): **만년필 전용** 선택 기능 — 그어진 뒤
                        // 잉크가 종이로 퍼져나가는 효과. 시작/중간/끝 구간별
                        // 속도(pt/초)를 따로 커스텀할 수 있고, 기본 활성화입니다.
                        if ui
                            .checkbox(&mut self.ink_bleed.enabled, "Ink bleed")
                            .on_hover_text(
                                "Fountain ink bleed: ink slowly spreads into the paper \
                                 after you write. Start/mid/end speeds are customizable.",
                            )
                            .changed()
                        {
                            self.save_default_session();
                            self.save_session();
                        }
                        if self.ink_bleed.enabled {
                            let any_changed = ui
                                .add(
                                    egui::Slider::new(
                                        &mut self.ink_bleed.start_rate,
                                        0.0..=3.0,
                                    )
                                    .text("Start speed"),
                                )
                                .on_hover_text(
                                    "How fast ink bleeds at the beginning of a stroke \
                                     (pt per second). 0 = no bleed there.",
                                )
                                .changed()
                                | ui
                                    .add(
                                        egui::Slider::new(
                                            &mut self.ink_bleed.mid_rate,
                                            0.0..=3.0,
                                        )
                                        .text("Mid speed"),
                                    )
                                    .on_hover_text(
                                        "How fast ink bleeds in the middle of a stroke \
                                         (pt per second).",
                                    )
                                    .changed()
                                | ui
                                    .add(
                                        egui::Slider::new(
                                            &mut self.ink_bleed.end_rate,
                                            0.0..=3.0,
                                        )
                                        .text("End speed"),
                                    )
                                    .on_hover_text(
                                        "How fast ink bleeds at the end of a stroke \
                                         (pt per second).",
                                    )
                                    .changed()
                                | ui
                                    .add(
                                        egui::Slider::new(
                                            &mut self.ink_bleed.max_spread_pt,
                                            1.0..=12.0,
                                        )
                                        .text("Max spread"),
                                    )
                                    .on_hover_text(
                                        "Upper limit of the bleed radius (pt).\n\
                                         Fresh ink spreads quickly at first and then \
                                         slows down until it reaches this size.",
                                    )
                                    .changed();
                            if any_changed {
                                self.save_default_session();
                                self.save_session();
                            }
                        }
                        // 모델 파라미터는 `self.fountain_profile`을 직접 수정합니다
                        // (장시간 borrow를 피해 저장 호출과 충돌하지 않게).
                        let any_changed = ui
                            .add(
                                egui::Slider::new(
                                    &mut self.fountain_profile.min_width_pt,
                                    0.1..=2.0,
                                )
                                .text("Min"),
                            )
                            .on_hover_text(
                                "Thinnest line width (pt) when writing fast and light.",
                            )
                            .changed()
                            | ui
                                .add(
                                    egui::Slider::new(
                                        &mut self.fountain_profile.pressure_alpha,
                                        0.3..=2.0,
                                    )
                                    .text("Press α"),
                                )
                                .on_hover_text(
                                    "Pressure sensitivity: how strongly pressure widens \
                                     the line (0.7~1.2 typical).",
                                )
                                .changed()
                            | ui
                                .add(
                                    egui::Slider::new(
                                        &mut self.fountain_profile.speed_beta,
                                        0.3..=3.0,
                                    )
                                    .text("Speed β"),
                                )
                                .on_hover_text(
                                    "Speed sensitivity: how strongly fast strokes thin \
                                     the line (1.0~1.5 typical).",
                                )
                                .changed()
                            | ui
                                .add(
                                    egui::Slider::new(
                                        &mut self.fountain_profile.speed_ref,
                                        10.0..=200.0,
                                    )
                                    .text("Speed ref"),
                                )
                                .on_hover_text(
                                    "Reference speed (pt/s) — at this speed the speed \
                                     factor is 0.5. Lower = thinner when writing normally.",
                                )
                                .changed()
                            | ui
                                .add(
                                    egui::Slider::new(
                                        &mut self.fountain_profile.tilt_k,
                                        0.0..=1.0,
                                    )
                                    .text("Tilt"),
                                )
                                .on_hover_text(
                                    "Tilt influence: laying the pen down widens the line.\n\
                                     Note: egui/winit don't expose pen tilt yet, so this \
                                     is 0 until a HID/WM_POINTER hook feeds set_pen_tilt.",
                                )
                                .changed();
                        if ui
                            .checkbox(&mut self.fountain_profile.italic, "Italic nib")
                            .on_hover_text(
                                "Stub/italic nib effect: strokes along the nib axis are \
                                 wide, across it are thin — no azimuth sensor needed, \
                                 the nib angle is fixed.",
                            )
                            .changed()
                        {
                            self.save_default_session();
                            self.save_session();
                        }
                        let any_changed2 = if self.fountain_profile.italic {
                            ui.add(
                                egui::Slider::new(
                                    &mut self.fountain_profile.nib_angle_deg,
                                    0.0..=180.0,
                                )
                                .text("Nib angle"),
                            )
                            .on_hover_text("Nib axis direction (degrees).")
                            .changed()
                                | ui
                                    .add(
                                        egui::Slider::new(
                                            &mut self.fountain_profile.italic_k,
                                            0.0..=0.6,
                                        )
                                        .text("Contrast"),
                                    )
                                    .on_hover_text(
                                        "Italic direction contrast (0.2~0.5 looks stub-like).",
                                    )
                                    .changed()
                        } else {
                            false
                        };
                        let dwell_changed = ui
                            .add(
                                egui::Slider::new(
                                    &mut self.fountain_profile.dwell_k,
                                    0.0..=0.5,
                                )
                                .text("Dwell"),
                            )
                            .on_hover_text(
                                "Ink pooling when the pen nearly stops — the classic \
                                 fountain-pen blob at the end of a stroke.",
                            )
                            .changed();
                        if any_changed || any_changed2 || dwell_changed {
                            self.save_default_session();
                            self.save_session();
                        }
                        let preview_color = Color32::from_rgba_unmultiplied(
                            self.pen_color[0],
                            self.pen_color[1],
                            self.pen_color[2],
                            self.pen_color[3],
                        );
                        fountain_profile_preview(
                            ui,
                            preview_color,
                            self.pen_width,
                            &self.fountain_profile,
                        );
                    }
                    ToolType::Highlighter => {
                        let mut color = Color32::from_rgba_unmultiplied(
                            self.hi_color[0],
                            self.hi_color[1],
                            self.hi_color[2],
                            self.hi_color[3],
                        );
                        if ui.color_edit_button_srgba(&mut color).changed() {
                            self.hi_color = color.to_array();
                            self.save_session();
                        }
                        if ui
                            .add(egui::Slider::new(&mut self.hi_width, 4.0..=40.0).text("Width"))
                            .changed()
                        {
                            self.save_session();
                        }
                        if ui
                            .checkbox(&mut self.text_highlight_snap, "Snap to text")
                            .on_hover_text(
                                "Highlight the recognized document text your stroke touches\n\
                                 (off = freehand translucent stroke)",
                            )
                            .changed()
                        {
                            self.save_default_session();
                            self.save_session();
                        }
                    }
                    ToolType::Eraser => {
                        if ui
                            .add(egui::Slider::new(&mut self.eraser_radius, 4.0..=60.0).text("Radius"))
                            .changed()
                        {
                            self.save_session();
                        }
                    }
                    ToolType::Pan => {}
                }
                });
            });

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(2.0);

            // Row 4: search (only while Ctrl+F is pressed)
            if self.show_search {
                toolbar_row(ui, |ui| {
                    ui.horizontal(|ui| {
                    ui.label(icon_text(ui, "Find", icons::MAGNIFYING_GLASS));
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.search_query)
                            .hint_text("Search in this page...")
                            .desired_width(200.0),
                    );
                    let submitted =
                        resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if self.focus_search {
                        resp.request_focus();
                        self.focus_search = false;
                    }
                    if ui.button("Find").clicked() || submitted {
                        self.search_update();
                    }
                    let can = !self.search_matches.is_empty();
                    if ui
                        .add_enabled(can, egui::Button::new(icon_text(ui, "", icons::CARET_UP)))
                        .on_hover_text("Previous match")
                        .clicked()
                    {
                        self.search_find(false);
                    }
                    if ui
                        .add_enabled(can, egui::Button::new(icon_text(ui, "", icons::CARET_DOWN)))
                        .on_hover_text("Next match")
                        .clicked()
                    {
                        self.search_find(true);
                    }
                    if !self.search_matches.is_empty() {
                        let cur = self.search_current.map(|c| c + 1).unwrap_or(0);
                        ui.label(format!("{cur}/{}", self.search_matches.len()));
                    }
                    if ui
                        .add(egui::Button::new(icon_text(ui, "", icons::X)).frame(false))
                        .on_hover_text("Close search (Ctrl+F)")
                        .clicked()
                    {
                        self.show_search = false;
                        self.search_clear();
                    }
                });
                });
                ui.add_space(4.0);
            }
        });
    }
}
