//! 툴바 Row1~Row4 — 공용 컴포넌트(`crate::ui`)로 조립합니다.
//!
//! React 스타일 분리: 이 파일은 **상태(FreeDfApp) ↔ props(컴포넌트)** 연결만
//! 담당하고, 버튼/토글/선택의 실제 렌더링은 `crate::ui` 컴포넌트가 합니다.
//! 도구 피커의 드래그 재정렬은 특수 로직이라 원본 egui 코드를 유지합니다.

use super::*;
use crate::ui::{icon_button, icon_label, IconButton};
impl FreeDfApp {
    /// Row 1: 패널 토글, 북마크, 정렬, Undo/Redo/Clear, Save/Load, Hide UI.
    pub(crate) fn row_top(&mut self, ui: &mut egui::Ui) {
        toolbar_row(ui, "row1", |ui| {
            ui.horizontal(|ui| {
                // Show UI / Hide UI 토글 — 항상 툴바 **가장 왼쪽**에 상주합니다.
                // 숨기면 캔버스+팔레트만 남고, 복귀는 우상단 플로팅 pill(☰)
                // 또는 Ctrl+Shift+M.
                let hide_ui = 0;
                let win_focus = 1;
                let hit = crate::ui::actionbar::ActionBar::new()
                    .add(hide_ui, icons::CORNERS_OUT, "Hide UI")
                    .hint(
                        "Hide toolbars & panels — canvas + palette only.\n\
                         Bring them back with the floating Show UI button (top-right)\n\
                         or Ctrl+Shift+M.",
                    )
                    .add_select(
                        win_focus,
                        icons::CROSSHAIR,
                        "Window Focus",
                        self.window_focus_on_move,
                    )
                    .hint(
                        "Focus this window when the cursor stays over it for the dwell time.\n\
                         Click to open its settings (enable + dwell time).\n\
                         Turn off for windows that should not grab focus in split view.",
                    )
                    .show(ui);
                if hit == Some(hide_ui) {
                    self.manual_minimal = true;
                    self.narrow_chrome_expanded = false;
                    self.show_palette = true;
                    self.save_default_session();
                } else if hit == Some(win_focus) {
                    self.window_focus_settings_open = true;
                }
                crate::ui::layout::vdivider(ui);
                crate::ui::layout::group(ui, crate::ui::layout::SP_2, |ui| {
                    self.toolbar_panel_group(ui);
                });
                crate::ui::layout::vdivider(ui);

                // "하나의 액션 = 한 곳": Undo/Redo/Clear는 선언형 액션 바의 스펙 한 칸.
                let undo = 0;
                let redo = 1;
                let clear = 2;
                let hit = crate::ui::actionbar::ActionBar::new()
                    .add(undo, icons::ARROW_COUNTER_CLOCKWISE, "Undo")
                    .hint("Undo (Ctrl+Z)")
                    .enabled(self.history.can_undo())
                    .add(redo, icons::ARROW_CLOCKWISE, "Redo")
                    .hint("Redo (Ctrl+Y)")
                    .enabled(self.history.can_redo())
                    .add(clear, icons::X_CIRCLE, "Clear Page")
                    .hint("Clear all ink on this page")
                    .show(ui);
                if hit == Some(undo) {
                    self.undo();
                } else if hit == Some(redo) {
                    self.redo();
                } else if hit == Some(clear) {
                    self.clear_page();
                }
                crate::ui::layout::vdivider(ui);

                let save = 0;
                let load = 1;
                let hit = crate::ui::actionbar::ActionBar::new()
                    .add(save, icons::FLOPPY_DISK, "Save Edits")
                    .hint("Save annotations (Ctrl+S)")
                    .add(load, icons::FOLDER_SIMPLE, "Load Edits")
                    .hint("Load annotations")
                    .show(ui);
                if hit == Some(save) {
                    self.save_annotations();
                } else if hit == Some(load) {
                    self.load_annotations();
                }
                crate::ui::layout::vdivider(ui);

                crate::ui::layout::group(ui, crate::ui::layout::SP_2, |ui| {
                    self.toolbar_overflow_menu(ui);
                });
            });
        });
    }

/// 컴포넌트(그룹 1-패널): Library / Outline / Bookmarks / Palette 토글 묶음.
    /// 세 패널은 상호 배타적(어느 하나 켜면 나머지 자동 해제)이고, Palette는
    /// 독립 토글입니다. (React의 <PanelToggles>에 해당하는 컨테이너 컴포넌트.)
    fn toolbar_panel_group(&mut self, ui: &mut egui::Ui) {
        let lib = 0;
        let out = 1;
        let bm = 2;
        let pal = 3;
        // 상태 바인딩 토글 — egui가 값을 직접 갱신하므로 여기선 부수효과만.
        let hit = crate::ui::actionbar::ActionBar::new()
            .add_toggle(lib, icons::NOTEBOOK, "Library", &mut self.show_library)
            .hint("Library (notes, PDFs, recents) — exclusive")
            .add_toggle(out, icons::LIST_BULLETS, "Outline", &mut self.show_outline)
            .hint("Outline — exclusive")
            .add_toggle(bm, icons::BOOKMARKS_SIMPLE, "Bookmarks", &mut self.show_bookmarks)
            .hint("Bookmarked pages — exclusive")
            .add_toggle(pal, icons::PALETTE, "Palette", &mut self.show_palette)
            .hint("Writing-tool color palette (right side of canvas)")
            .show(ui);
        if hit == Some(lib) {
            // Library / Outline / Bookmarks는 어디서든 상호 베타적.
            if self.show_library {
                [self.show_library, self.show_outline, self.show_bookmarks] =
                    exclusive_panel_on(PanelKind::Library);
            }
            self.save_session();
        } else if hit == Some(out) {
            if self.show_outline {
                [self.show_library, self.show_outline, self.show_bookmarks] =
                    exclusive_panel_on(PanelKind::Outline);
            }
            self.save_session();
        } else if hit == Some(bm) {
            if self.show_bookmarks {
                [self.show_library, self.show_outline, self.show_bookmarks] =
                    exclusive_panel_on(PanelKind::Bookmarks);
            }
        } else if hit == Some(pal) {
            self.save_default_session();
        }
    }

    /// 컴포넌트(그룹 3): ⋯ 오버플로 메뉴 — 자주 안 쓰는 도구/설정을 한 곳으로.
    /// (React의 <OverflowMenu>에 해당하는 컨테이너 컴포넌트.)
    fn toolbar_overflow_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("More", |ui| {
            ui.set_min_width(300.0);
            // 정렬 — 상단 상주에서 메뉴로 이동 (패널 상태와 무관하게 동작).
            let left = 0;
            let center = 1;
            let right = 2;
            let hit = crate::ui::actionbar::ActionBar::new()
                .add_select(left, icons::TEXT_ALIGN_LEFT, "", self.page_align == PageAlign::Left)
                .hint("Align left")
                .add_select(center, icons::TEXT_ALIGN_CENTER, "", self.page_align == PageAlign::Center)
                .hint("Align center")
                .add_select(right, icons::TEXT_ALIGN_RIGHT, "", self.page_align == PageAlign::Right)
                .hint("Align right")
                .show(ui);
            if hit == Some(left) {
                self.page_align = PageAlign::Left;
                self.realign();
                self.save_session();
            } else if hit == Some(center) {
                self.page_align = PageAlign::Center;
                self.realign();
                self.save_session();
            } else if hit == Some(right) {
                self.page_align = PageAlign::Right;
                self.realign();
                self.save_session();
            }
            ui.separator();
            if ui
                .checkbox(&mut self.dictionary.enabled, "Dictionary")
                .on_hover_text("Look up a tapped word (needs internet once per word).")
                .changed()
            {
                self.save_default_session();
            }
            if ui.checkbox(&mut self.show_media, "Media").changed() {
                self.media_refresh();
            }
            if crate::ui::buttons::Button::secondary("Media Server...")
                .show(ui)
                .clicked()
            {
                self.server_msg = None;
                self.server_settings_open = true;
            }
            ui.separator();
            if crate::ui::buttons::Button::secondary("Macro...")
                .show(ui)
                .clicked()
            {
                self.macro_capture = None;
                self.macro_settings_open = true;
            }
            if crate::ui::buttons::Button::secondary("Gamepad...")
                .show(ui)
                .clicked()
            {
                self.gamepad_settings_open = true;
            }
            ui.separator();
            ui.menu_button("Cache actions", |ui| {
                // 등록된 모든 캐시 순회 — actions/cache.rs의 all_caches()에
                // 등록만 하면 여기에 자동으로 나타납니다.
                for (i, cache) in all_caches().iter().enumerate() {
                    if i > 0 { ui.separator(); }
                    ui.label(egui::RichText::new(cache.label()).strong());
                    ui.label(egui::RichText::new(cache.description()).weak().small());
                    if ui
                        .button(format!("Clear {}", cache.label().to_lowercase()))
                        .clicked()
                    {
                        cache.clear(self);
                    }
                }
            });
            ui.separator();
            ui.checkbox(&mut self.debug_hud, "Debug HUD").on_hover_text(
                "Live input overlay & diagnostics (pressure, tilt, speed, system).",
            );
        });
    }

    /// Row 2: Page(Insert/Rotate/Delete), Canvas, Wheel, Paper.
    pub(crate) fn row_pages(&mut self, ui: &mut egui::Ui) {
        toolbar_row(ui, "row2", |ui| {
            ui.horizontal(|ui| {
                // G1: Page group
                crate::ui::layout::group(ui, crate::ui::layout::SP_2, |ui| {
                icon_label(ui, icons::FILES, "Page");
                let page_count = self.document.as_ref().map(|d| d.page_count()).unwrap_or(0);
                // 메뉴 대신 **전용 플로팅 창**을 엽니다 — 메뉴 안에서는
                // 숫자를 타이핑하는 순간 닫히는 문제가 있어 창으로 분리.
                let insert = 0;
                let delete = 1;
                let hit = crate::ui::actionbar::ActionBar::new()
                    .add(insert, icons::PLUS_SQUARE, "Insert Page")
                    .hint("Insert blank pages — opens a small window")
                    .add(delete, icons::TRASH_SIMPLE, "Delete Page")
                    .enabled(page_count > 1)
                    .hint("Delete this page")
                    .show(ui);
                if hit == Some(insert) {
                    self.insert_page_open = true;
                } else if hit == Some(delete) {
                    self.delete_page_action();
                }
                ui.menu_button(icon_text(ui, "Rotate Page", icons::REPEAT), |ui| {
                    if crate::ui::buttons::Button::secondary("Rotate current page CW")
                        .enabled(page_count > 0)
                        .show(ui)
                        .clicked()
                    {
                        ui.close();
                        self.rotate_page_action(true);
                    }
                    if crate::ui::buttons::Button::secondary("Rotate current page CCW")
                        .enabled(page_count > 0)
                        .show(ui)
                        .clicked()
                    {
                        ui.close();
                        self.rotate_page_action(false);
                    }
                    if crate::ui::buttons::Button::secondary("Rotate all pages CW")
                        .enabled(page_count > 0)
                        .show(ui)
                        .clicked()
                    {
                        ui.close();
                        self.rotate_all_pages_action(true);
                    }
                    if crate::ui::buttons::Button::secondary("Rotate all pages CCW")
                        .enabled(page_count > 0)
                        .show(ui)
                        .clicked()
                    {
                        ui.close();
                        self.rotate_all_pages_action(false);
                    }
                })
                .response
                .on_hover_text("Rotate pages (CW = clockwise)");
                }); // end G1 (Page) group
                crate::ui::layout::vdivider(ui);

                // G2: Canvas + Edge scroll group
                crate::ui::layout::group(ui, crate::ui::layout::SP_2, |ui| {
                icon_label(ui, icons::IMAGE, "Canvas Color");
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
                if icon_button(
                    ui,
                    IconButton::new(icons::ARROWS_OUT_CARDINAL, "Edge Auto Scroll")
                        .selected(self.edge_autoscroll)
                        .hint(
                            "Edge auto-scroll: the pen (by default) near the canvas edge pans the view.\n\
                             Ignored over the palette/bottom bar. Click to open its settings.",
                        ),
                )
                .clicked()
                {
                    self.edge_scroll_settings_open = true;
                }
                }); // end G2 (Canvas) group
                crate::ui::layout::vdivider(ui);

                // G3: Color wheel group
                crate::ui::layout::group(ui, crate::ui::layout::SP_2, |ui| {
                // Color wheel (원형 팔레트 색 지정) — 전용 설정 창.
                if icon_button(
                    ui,
                    IconButton::new(icons::PALETTE, "Color Wheel")
                        .hint("Color wheel palette colors — click to open settings"),
                )
                .clicked()
                {
                    self.wheel_settings_open = true;
                }
                }); // end G3 (Color wheel) group
                crate::ui::layout::vdivider(ui);

                // G4: Paper group
                crate::ui::layout::group(ui, crate::ui::layout::SP_2, |ui| {
                // Paper (grid / ruling / color) — applied to the **current
                // page**; new pages use these values as their defaults.
                // "Apply to all" pushes the current values onto every page.
                icon_label(ui, icons::NOTEBOOK, "Paper");
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
                // Paper color swatches as a tightly-packed flex row.
                crate::ui::layout::hstack(ui, crate::ui::layout::SP_1, |ui| {
                for (i, paper) in PAPER_COLORS.iter().enumerate() {
                    let mut color =
                        Color32::from_rgba_unmultiplied(paper[0], paper[1], paper[2], paper[3]);
                    let selected = self.paper_color == *paper;
                    let (resp, changed) =
                        swatch_with_picker(ui, ("paper_swatch", i), &mut color, selected);
                    let resp = resp.on_hover_text("Paper color — click to apply, double-click to edit (current page)");
                    if resp.clicked() {
                        self.paper_color = *paper;
                        self.apply_paper_to_current_page();
                        self.save_default_session();
                        self.save_session();
                    } else if changed {
                        self.paper_color = color.to_array();
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
                }); // end paper swatch flex row
                // 세부 설정 전용 창 트리거 — 크기/간격/줄 색·두께/전체 적용.
                if icon_button(
                    ui,
                    IconButton::new(icons::GEAR, "Paper Settings").hint(
                        "Open the paper settings window:\n\
                         page size, grid spacing, line color & thickness, apply to all.",
                    ),
                )
                .clicked()
                {
                    self.paper_settings_open = true;
                }
                }); // end G4 (Paper) group
            });
        });
    }

    /// Row 3: 도구 피커(드래그 재정렬) + 도구별 설정.
    pub(crate) fn row_tools(&mut self, ui: &mut egui::Ui) {
        toolbar_row(ui, "row3", |ui| {
            ui.horizontal(|ui| {
                // ── 도구 선택기 (드래그 앤 드롭 재정렬) — 특수 로직 유지 ──
                crate::ui::layout::group(ui, crate::ui::layout::SP_2, |ui| {
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
                }); // end G1 (tool picker)
                crate::ui::layout::vdivider(ui);

                // G2: per-tool settings + refresh
                crate::ui::layout::group(ui, crate::ui::layout::SP_2, |ui| {
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
                        // Round color swatches forming a neat color bar (flex row).
                        crate::ui::layout::hstack(ui, crate::ui::layout::SP_1, |ui| {
                        for (i, swatch) in swatches.iter().enumerate() {
                            let mut color = Color32::from_rgba_unmultiplied(
                                swatch[0],
                                swatch[1],
                                swatch[2],
                                swatch[3],
                            );
                            let selected = *swatch == self.pen_color;
                            let (resp, changed) =
                                swatch_with_picker(ui, ("pen_swatch", i), &mut color, selected);
                            let resp = resp.on_hover_text("Pen color — click to apply, double-click to edit");
                            if resp.clicked() {
                                self.pen_color = *swatch;
                                self.save_default_session();
                                self.save_session();
                            } else if changed {
                                self.pen_color = color.to_array();
                                self.save_default_session();
                                self.save_session();
                            }
                        }
                        }); // end pen swatch flex row
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
                        let width_resp = crate::ui::slider(
                            ui,
                            &mut self.pen_width,
                            0.5..=12.0,
                            "Width",
                            "Base line width (pt). The ballpen model varies it only \
                             a little (±30%) by pressure & speed.",
                        );
                        if width_resp.changed() {
                            self.save_session();
                        }
                        // 세부 설정 전용 창 트리거 — 툴바는 필수만 남깁니다.
                        if icon_button(
                            ui,
                            IconButton::new(icons::GEAR, "Pen Settings").hint(
                                "Open the ballpen settings window:\n\
                                 physics model, ink soak, ink grain, cursor, smoothing.",
                            ),
                        )
                        .clicked()
                        {
                            self.tool_settings_open = true;
                        }
                        // 커서(펜 닙) 크기 전용 설정 창 트리거.
                        if icon_button(
                            ui,
                            IconButton::new(icons::CURSOR_CLICK, "Cursor Size").hint(
                                "Open the cursor settings window:\n\
                                 scale the ballpen/fountain pen nib cursor.",
                            ),
                        )
                        .clicked()
                        {
                            self.cursor_settings_open = true;
                        }
                        if crate::ui::check(
                            ui,
                            &mut self.pressure_enabled,
                            "Pressure",
                            "Use pen/tablet pressure. Off = always full pressure.",
                        )
                        .changed()
                        {
                            self.save_session();
                        }
                        if crate::ui::check(
                            ui,
                            &mut self.left_handed,
                            "Left-handed",
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
                            let mut color = Color32::from_rgba_unmultiplied(
                                swatch[0],
                                swatch[1],
                                swatch[2],
                                swatch[3],
                            );
                            let selected = *swatch == self.fountain_color;
                            let (resp, changed) =
                                swatch_with_picker(ui, ("fountain_swatch", i), &mut color, selected);
                            let resp = resp.on_hover_text("Ink color — click to apply, double-click to edit");
                            if resp.clicked() {
                                self.fountain_color = *swatch;
                                self.save_default_session();
                                self.save_session();
                            } else if changed {
                                self.fountain_color = color.to_array();
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
                        let nib_resp = crate::ui::slider(
                            ui,
                            &mut self.fountain_width,
                            0.5..=12.0,
                            "Nib",
                            "Nib width = maximum line width (pt).\n\
                             The model varies it by pressure, speed and tilt.",
                        );
                        if nib_resp.changed() {
                            self.save_session();
                        }
                        // 세부 설정 전용 창 트리거 — 툴바는 필수만 남깁니다.
                        if icon_button(
                            ui,
                            IconButton::new(icons::GEAR, "Fountain Settings").hint(
                                "Open the fountain pen settings window:\n\
                                 physics model, ink soak, ink grain, italic nib, dwell.",
                            ),
                        )
                        .clicked()
                        {
                            self.tool_settings_open = true;
                        }
                    }
                    ToolType::Highlighter => {
                        // GoodNotes 풍 파스텔 프리셋.
                        let swatches = Palette::highlighter_swatches();
                        // GoodNotes-palette pastels (flex row).
                        crate::ui::layout::hstack(ui, crate::ui::layout::SP_1, |ui| {
                        for (i, swatch) in swatches.iter().enumerate() {
                            let mut color = Color32::from_rgba_unmultiplied(
                                swatch[0],
                                swatch[1],
                                swatch[2],
                                swatch[3],
                            );
                            let selected = *swatch == self.hi_color;
                            let (resp, changed) =
                                swatch_with_picker(ui, ("hi_swatch", i), &mut color, selected);
                            let resp = resp.on_hover_text("Highlighter color — click to apply, double-click to edit");
                            if resp.clicked() {
                                self.hi_color = *swatch;
                                self.save_default_session();
                                self.save_session();
                            } else if changed {
                                self.hi_color = color.to_array();
                                self.save_default_session();
                                self.save_session();
                            }
                        }
                        }); // end highlighter swatch flex row
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
                        if crate::ui::slider(ui, &mut self.hi_width, 4.0..=40.0, "Width", "")
                            .changed()
                        {
                            self.save_session();
                        }
                        if crate::ui::check(
                            ui,
                            &mut self.text_highlight_snap,
                            "Snap to text",
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
                        if crate::ui::slider(ui, &mut self.eraser_radius, 4.0..=60.0, "Radius", "")
                            .changed()
                        {
                            self.save_session();
                        }
                    }
                    ToolType::Pan => {}
                }
                crate::ui::layout::vdivider(ui);
                // 모니터 주사율 프리셋 — 필기 관련 페이싱(진행 획 재구성
                // 주기·잉크 스밈 그라데이션)을 한 번에 바꿉니다.
                let mut hz = self.refresh_hz;
                let combo = egui::ComboBox::from_id_salt("refresh_hz")
                    .selected_text(format!("{hz}Hz"))
                    .show_ui(ui, |ui| {
                        for preset in freedf_canvas::REFRESH_PRESETS {
                            let desc = freedf_canvas::ink_pacing_for(preset);
                            ui.selectable_value(
                                &mut hz,
                                preset,
                                format!(
                                    "{preset}Hz — re-bake every {:.0}ms, soak ×{:.2}",
                                    desc.active_geom_ms, desc.soak_scale
                                ),
                            );
                        }
                    });
                combo.response.on_hover_text(
                    "Monitor refresh rate — tunes all ink pacing:\n\
                     higher = smoother strokes & finer soak gradient\n\
                     (more computation). Match your display's Hz.",
                );
                if hz != self.refresh_hz {
                    self.refresh_hz = hz;
                    self.save_default_session();
                    self.save_session();
                }
                }); // end G2 (settings) group
            });
        });
    }

    /// Row 4: 검색 (Ctrl+F일 때만).
    pub(crate) fn search_row(&mut self, ui: &mut egui::Ui) {
        if !self.show_search {
            return;
        }
        toolbar_row(ui, "search", |ui| {
            ui.horizontal(|ui| {
                icon_label(ui, icons::MAGNIFYING_GLASS, "Find");
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
                if crate::ui::buttons::Button::primary("Find").show(ui).clicked() || submitted {
                    self.search_update();
                }
                let can = !self.search_matches.is_empty();
                if icon_button(
                    ui,
                    IconButton::new(icons::CARET_UP, "")
                        .enabled(can)
                        .hint("Previous match"),
                )
                .clicked()
                {
                    self.search_find(false);
                }
                if icon_button(
                    ui,
                    IconButton::new(icons::CARET_DOWN, "")
                        .enabled(can)
                        .hint("Next match"),
                )
                .clicked()
                {
                    self.search_find(true);
                }
                if !self.search_matches.is_empty() {
                    let cur = self.search_current.map(|c| c + 1).unwrap_or(0);
                    ui.label(format!("{cur}/{}", self.search_matches.len()));
                }
                if icon_button(
                    ui,
                    IconButton::new(icons::X, "")
                        .frame(false)
                        .hint("Close search (Ctrl+F)"),
                )
                .clicked()
                {
                    self.show_search = false;
                    self.search_clear();
                }
            });
        });
        ui.add_space(4.0);
        ui.add_space(4.0);
    }
}
