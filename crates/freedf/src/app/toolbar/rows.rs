//! 툴바 Row1~Row4 내용 — 패널 토글/페이지 그룹/도구 피커/검색.

use super::*;

impl FreeDfApp {
    /// Row 1: 패널 토글, 북마크, 정렬, Undo/Redo/Clear, Save/Load, Hide UI.
    pub(crate) fn row_top(&mut self, ui: &mut egui::Ui) {
                // Row 1: panels / page tools / ink tools
                toolbar_row(ui, "row1", |ui| {
                    ui.horizontal(|ui| {
                    // Show UI / Hide UI 토글 — 항상 툴바 **가장 왼쪽**에 상주합니다.
                    // 숨기면 캔버스+팔레트만 남고, 복귀는 우상단 플로팅 pill(☰)
                    // 또는 Ctrl+Shift+M.
                    if ui
                        .button(icon_text(ui, "Hide UI", icons::CORNERS_OUT))
                        .on_hover_text(
                            "Hide toolbars & panels — canvas + palette only.\n\
                             Bring them back with the floating ☰ pill (top-right)\n\
                             or Ctrl+Shift+M.",
                        )
                        .clicked()
                    {
                        self.manual_minimal = true;
                        self.narrow_chrome_expanded = false;
                        self.show_palette = true;
                        self.save_default_session();
                    }
                    // Window Focus — 단일 라벨 버튼 (상태는 선택 하이라이트).
                    // 클릭하면 설정 창(켜기/끄기 + 머무름 시간)이 열립니다.
                    if ui
                        .add(
                            egui::Button::new(icon_text(ui, "Window Focus", icons::CROSSHAIR))
                                .selected(self.window_focus_on_move),
                        )
                        .on_hover_text(
                            "Focus this window when the cursor stays over it for the dwell time.\n\
                             Click to open its settings (enable + dwell time).\n\
                             Turn off for windows that should not grab focus in split view.",
                        )
                        .clicked()
                    {
                        self.window_focus_settings_open = true;
                    }
                    ui.separator();
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
                    if ui
                        .toggle_value(
                            &mut self.server_settings_open,
                            icon_text(ui, "Media Server", icons::CLOUD),
                        )
                        .on_hover_text(
                            "Media server connection settings — upload & play audio recordings\n\
                             from your self-hosted VPS.",
                        )
                        .changed()
                    {
                        self.server_msg = None;
                    }
                    if ui
                        .toggle_value(&mut self.show_media, icon_text(ui, "Recordings", icons::MICROPHONE))
                        .on_hover_text("Recordings for this document — upload / play / delete")
                        .changed()
                    {
                        // 열릴 때 목록 갱신.
                        self.media_refresh();
                    }
                    ui.separator();
    
                    // 정렬(왼쪽/가운데/오른쪽) — 패널 상태와 무관하게 **항상 표시**
                    // (패널이 펼쳐지면 사라지던 버그 패턴 제거).
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
                        .button(icon_text(ui, "Clear Page", icons::X_CIRCLE))
                        .on_hover_text("Clear all ink on this page")
                        .clicked()
                    {
                        self.clear_page();
                    }
                    ui.separator();
    
                    if ui
                        .button(icon_text(ui, "Save Edits", icons::FLOPPY_DISK))
                        .on_hover_text("Save annotations (Ctrl+S)")
                        .clicked()
                    {
                        self.save_annotations();
                    }
                    if ui
                        .button(icon_text(ui, "Load Edits", icons::FOLDER_SIMPLE))
                        .on_hover_text("Load annotations")
                        .clicked()
                    {
                        self.load_annotations();
                    }
                    });
                });
    }

    /// Row 2: Page(Insert/Rotate/Delete), Canvas, Wheel, Paper.
    pub(crate) fn row_pages(&mut self, ui: &mut egui::Ui) {
                // Row 2: Page (structure + paper styling)
                toolbar_row(ui, "row2", |ui| {
                    ui.horizontal(|ui| {
                    ui.label(icon_text(ui, "Page", icons::FILES));
                    let page_count = self.document.as_ref().map(|d| d.page_count()).unwrap_or(0);
                    // 메뉴 대신 **전용 플로팅 창**을 엽니다 — 메뉴 안에서는
                    // 숫자를 타이핑하는 순간 닫히는 문제가 있어 창으로 분리.
                    if ui
                        .button(icon_text(ui, "Insert Page", icons::PLUS_SQUARE))
                        .on_hover_text("Insert blank pages — opens a small window")
                        .clicked()
                    {
                        self.insert_page_open = true;
                    }
                    ui.menu_button(icon_text(ui, "Rotate Page", icons::REPEAT), |ui| {
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
                            egui::Button::new(icon_text(ui, "Delete Page", icons::TRASH_SIMPLE)),
                        )
                        .on_hover_text("Delete this page")
                        .clicked()
                    {
                        self.delete_page_action();
                    }
                    ui.separator();
    
                    // Canvas (페이지 뒤 배경색) — 전용 설정 창.
                    // (Paper 그룹 앞에 배치 — 뒤에 두면 화면 밖으로 잘려 안 보임)
                    ui.label(icon_text(ui, "Canvas Color", icons::IMAGE));
                    let canvas_color = Color32::from_rgba_unmultiplied(
                        self.canvas_color[0],
                        self.canvas_color[1],
                        self.canvas_color[2],
                        self.canvas_color[3],
                    );
                    if color_circle_swatch(ui, "canvas_swatch", canvas_color, false)
                        .on_hover_text("Canvas background color — click to open settings")
                        .clicked()
                    {
                        self.canvas_settings_open = true;
                    }
                    // 엣지 자동 스크롤 — 라벨 버튼이 설정 창을 엽니다 (상태는
                    // 선택 하이라이트로 표시).
                    if ui
                        .add(
                            egui::Button::new(icon_text(
                                ui,
                                "Edge Auto Scroll",
                                icons::ARROWS_OUT_CARDINAL,
                            ))
                            .selected(self.edge_autoscroll),
                        )
                        .on_hover_text(
                            "Edge auto-scroll: cursor near the canvas edge pans the view.\n\
                             Click to open its settings (enable, edge zone, per-direction speeds).",
                        )
                        .clicked()
                    {
                        self.edge_scroll_settings_open = true;
                    }
                    ui.separator();

                    // Color wheel (원형 팔레트 색 지정) — 전용 설정 창.
                    if ui
                        .button(icon_text(ui, "Color Wheel", icons::PALETTE))
                        .on_hover_text("Color wheel palette colors — click to open settings")
                        .clicked()
                    {
                        self.wheel_settings_open = true;
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
                    // 세부 설정 전용 창 트리거 — 크기/간격/줄 색·두께/전체 적용.
                    if ui
                        .add(egui::Button::new(icon_text(ui, "Paper Settings", icons::GEAR)))
                        .on_hover_text(
                            "Open the paper settings window:\n\
                             page size, grid spacing, line color & thickness, apply to all.",
                        )
                        .clicked()
                    {
                        self.paper_settings_open = true;
                    }
                    });
                });
    }

    /// Row 3: 도구 피커(드래그 재정렬) + 도구별 설정.
    pub(crate) fn row_tools(&mut self, ui: &mut egui::Ui) {
                // Row 3: drawing tools (drag to reorder) + settings
                toolbar_row(ui, "row3", |ui| {
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
                            // 세부 설정 전용 창 트리거 — 툴바는 필수만 남깁니다.
                            if ui
                                .add(egui::Button::new(icon_text(ui, "Pen Settings", icons::GEAR)))
                                .on_hover_text(
                                    "Open the ballpen settings window:\n\
                                     physics model, ink soak, ink grain, cursor, smoothing.",
                                )
                                .clicked()
                            {
                                self.tool_settings_open = true;
                            }
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
                            // 세부 설정 전용 창 트리거 — 툴바는 필수만 남깁니다.
                            if ui
                                .add(egui::Button::new(icon_text(ui, "Fountain Settings", icons::GEAR)))
                                .on_hover_text(
                                    "Open the fountain pen settings window:\n\
                                     physics model, ink soak, ink grain, italic nib, dwell.",
                                )
                                .clicked()
                            {
                                self.tool_settings_open = true;
                            }
                        }
                        ToolType::Highlighter => {
                            // GoodNotes 풍 파스텔 프리셋.
                            let swatches = Palette::highlighter_swatches();
                            for (i, swatch) in swatches.iter().enumerate() {
                                let color = Color32::from_rgba_unmultiplied(
                                    swatch[0],
                                    swatch[1],
                                    swatch[2],
                                    swatch[3],
                                );
                                let selected = *swatch == self.hi_color;
                                if color_circle_swatch(ui, ("hi_swatch", i), color, selected)
                                    .on_hover_text("Highlighter color")
                                    .clicked()
                                {
                                    self.hi_color = *swatch;
                                    self.save_default_session();
                                    self.save_session();
                                }
                            }
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
    }

    /// Row 4: 검색 (Ctrl+F일 때만).
    pub(crate) fn search_row(&mut self, ui: &mut egui::Ui) {
        if !self.show_search {
            return;
        }
                if self.show_search {
                    toolbar_row(ui, "search", |ui| {
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
        ui.add_space(4.0);
    }
}
