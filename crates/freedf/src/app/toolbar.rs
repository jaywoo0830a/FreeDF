//! Three-tier toolbar: panel/page toggles, drawing-tool picker with drag-to-reorder, and per-tool settings.
//!
//! Extracted from `app.rs` during the layered refactor — these are
//! `FreeDfApp` methods living in a submodule (`use super::*` re-exports the
//! app state types and shared helpers).

use super::*;

/// 펜의 **실제 잉크 수식**(필압 곡선)을 그대로 보여주는 미니 스트로크 미리보기.
/// 화면 렌더링과 같은 함수로 가상의 곡선을 그려 필압에 따른 두께 변화를
/// 한눈에 확인할 수 있습니다.
fn pen_profile_preview(
    ui: &mut egui::Ui,
    color: Color32,
    width: f32,
    curve: &PressureCurve,
) {
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(150.0, 34.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 4.0, ui.visuals().faint_bg_color);
    let n = 64;
    let x0 = rect.left() + 3.0;
    let x1 = rect.right() - 3.0;
    let cy = rect.center().y;
    let amp = rect.height() * 0.30;
    let mut pts: Vec<egui::Pos2> = Vec::with_capacity(n);
    let mut widths: Vec<f32> = Vec::with_capacity(n);
    let mut max_w: f32 = 0.5;
    for i in 0..n {
        let t = i as f32 / (n - 1) as f32;
        let x = x0 + (x1 - x0) * t;
        let y = cy + (t * 2.0 * std::f32::consts::PI).sin() * amp;
        // 필압이 물결처럼 오르내림 → 두께가 그대로 따라감.
        let pressure = 0.25 + 0.75 * (t * 3.0 * std::f32::consts::PI).sin().abs();
        let w = width * curve.apply(1.0, pressure);
        pts.push(egui::pos2(x, y));
        widths.push(w);
        max_w = max_w.max(w);
    }
    let scale = (rect.height() * 0.40 / max_w).clamp(0.3, 1.6);
    for i in 0..n - 1 {
        let wpx = (widths[i] + widths[i + 1]) * 0.5 * scale;
        painter.line_segment([pts[i], pts[i + 1]], Stroke::new(wpx, color));
    }
    let _ = resp.on_hover_text(
        "Live preview of the current pressure curve:\nwidth = f(pen pressure)",
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
                                "Stroke width. The line thickness follows the \
                                 pressure curve (see the preview below).",
                            );
                        if width_resp.changed() {
                            self.save_session();
                        }
                        // 미니 스트로크 미리보기: 실제 필압 수식으로 그립니다.
                        let preview_color = Color32::from_rgba_unmultiplied(
                            self.pen_color[0],
                            self.pen_color[1],
                            self.pen_color[2],
                            self.pen_color[3],
                        );
                        pen_profile_preview(ui, preview_color, self.pen_width, &self.pressure_curve);
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
                        if self.pressure_enabled {
                            if ui
                                .add(
                                    egui::Slider::new(
                                        &mut self.pressure_curve.min_ratio,
                                        0.1..=1.0,
                                    )
                                    .text("Min"),
                                )
                                .on_hover_text(
                                    "Min: thickness multiplier at the lightest touch.\n\
                                     e.g. Min=0.4 → the thinnest line is 40% of the Width.",
                                )
                                .changed()
                            {
                                self.save_session();
                            }
                            if ui
                                .add(
                                    egui::Slider::new(
                                        &mut self.pressure_curve.max_ratio,
                                        1.0..=3.0,
                                    )
                                    .text("Max"),
                                )
                                .on_hover_text(
                                    "Max: thickness multiplier at full pressure.\n\
                                     e.g. Max=1.4 → the boldest line is 140% of the Width.",
                                )
                                .changed()
                            {
                                self.save_session();
                            }
                        }
                        // 필기 스무딩(안정화): 손떨림을 줄여 선을 매끄럽게.
                        if ui
                            .add(egui::Slider::new(&mut self.smoothing, 0.0..=1.0).text("Smoothing"))
                            .on_hover_text(
                                "Stabilizer: filters hand tremor while keeping fast strokes \
                                 responsive. 0 = raw input, 1 = silky smooth.",
                            )
                            .changed()
                        {
                            self.smoothing = self.smoothing.clamp(0.0, 1.0);
                            self.save_default_session();
                            self.save_session();
                        }
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
