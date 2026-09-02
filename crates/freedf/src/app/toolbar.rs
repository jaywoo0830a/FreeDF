//! Three-tier toolbar: panel/page toggles, drawing-tool picker with drag-to-reorder, and per-tool settings.
//!
//! Extracted from `app.rs` during the layered refactor — these are
//! `FreeDfApp` methods living in a submodule (`use super::*` re-exports the
//! app state types and shared helpers).

use super::*;

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
                if ui
                    .button(icon_text(ui, "Export", icons::IMAGE))
                    .on_hover_text("Export current page as PNG (Ctrl+E)")
                    .clicked()
                {
                    self.export_png();
                }
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
                        (InsertTarget::FrontBegin, "Front begin"),
                        (InsertTarget::FrontEnd, "Front end"),
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

                // Paper (grid / ruling / color) — applied per page;
                // paper size selects the size for new pages & notes.
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
                    .on_hover_text("Style applied to the current page");
                for (i, paper) in PAPER_COLORS.iter().enumerate() {
                    let color =
                        Color32::from_rgba_unmultiplied(paper[0], paper[1], paper[2], paper[3]);
                    let selected = self.paper_color == *paper;
                    if color_circle_swatch(ui, ("paper_swatch", i), color, selected)
                        .on_hover_text("Paper color")
                        .clicked()
                    {
                        self.paper_color = *paper;
                        self.apply_paper_to_current_page();
                        self.save_default_session();
                        self.save_session();
                    }
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
                    .on_hover_text("Size of new pages & new notes");
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
                    ToolType::Pen | ToolType::Ballpoint | ToolType::Fountain => {
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
                        if ui
                            .add(egui::Slider::new(&mut self.pen_width, 0.5..=12.0).text("Width"))
                            .changed()
                        {
                            self.save_session();
                        }
                        // 도구별 프로필 설명 (같은 Width여도 실제 굵기가 다름).
                        let hint = ink_profile_hint(self.tool);
                        if !hint.is_empty() {
                            ui.label(
                                egui::RichText::new(hint)
                                    .weak()
                                    .small()
                                    .color(ui.visuals().weak_text_color()),
                            );
                        }
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
                        if ui.checkbox(&mut self.pressure_enabled, "Pressure").changed() {
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
                                .changed()
                            {
                                self.save_session();
                            }
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
