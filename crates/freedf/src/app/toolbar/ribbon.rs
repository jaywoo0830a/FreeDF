//! 툴바 리본 — 도메인별 3행 레이아웃 (Workspace · Page · Ink).
//!
//! 재설계 원칙 (`docs/DESIGN-TOOLBAR.md`, `docs/UI-COMPONENTS.md`):
//! - **하나의 액션 = 한 곳**: 각 액션은 정확히 한 그룹에만 상주하고,
//!   자주 안 쓰는 것은 `More` 오버플로 또는 `Settings` 창으로 보냅니다.
//! - **도메인 그룹핑**: 행마다 하나의 주제만 —
//!   Row1 = 워크스페이스(뷰/편집/파일), Row2 = 페이지(페이지/종이/캔버스),
//!   Row3 = 잉크(도구/색/굵기).
//! - **설정 진입 2곳**: 전역 `Settings` 기어 + 도구별 `Draw` 기어로 수렴합니다.
//! - **프레젠테이션 ↔ 상태 분리**: 표시는 `crate::ui` 컴포넌트가, 상태 연결은
//!   이 파일(컨테이너)이 담당합니다.

use super::*;
use crate::ui::{icon_button, icon_label, icon_select, icon_toggle, IconButton};

/// 메뉴 안 섹션 제목 — 같은 주제의 항목을 묶는 작은 강조 라벨.
/// 도구 선택 버튼의 자동화용 안정 id — 표시 라벨(`ToolType::label`)과 분리된 계약입니다.
/// (라벨을 바꿔도 스크립트가 깨지지 않도록 명시적으로 고정합니다.)
fn dev_tool_id(tool: ToolType) -> &'static str {
    match tool {
        ToolType::Pen => "toolbar.tool.pen",
        ToolType::Fountain => "toolbar.tool.fountain",
        ToolType::Highlighter => "toolbar.tool.highlighter",
        ToolType::Eraser => "toolbar.tool.eraser",
        ToolType::Pan => "toolbar.tool.pan",
    }
}

fn menu_section(ui: &mut egui::Ui, label: &str) {
    ui.add_space(2.0);
    ui.label(egui::RichText::new(label).small().strong());
}

impl FreeDfApp {
    /// Row 1 — Workspace: 크롬 · 뷰 패널 · 편집 이력 · 파일 · 전역 설정/오버플로.
    pub(crate) fn row_workspace(&mut self, ui: &mut egui::Ui) {
        toolbar_row(ui, "workspace", |ui| {
            ui.horizontal(|ui| {
                // ── Chrome ──────────────────────────────────────────────
                // 숨기면 캔버스+팔레트만 남고, 복귀는 우상단 플로팅 pill(☰)
                // 또는 Ctrl+Shift+M.
                let hide_ui = icon_button(
                    ui,
                    IconButton::new(icons::CORNERS_OUT, "Hide UI").hint(
                        "Hide toolbars & panels — canvas + palette only.\n\
                         Bring them back with the floating Show UI button (top-right)\n\
                         or Ctrl+Shift+M.",
                    ),
                );
                crate::app::dev::tag_button(ui, "toolbar.hide_ui", "Hide UI", &hide_ui);
                if hide_ui.clicked() {
                    self.manual_minimal = true;
                    self.narrow_chrome_expanded = false;
                    self.show_palette = true;
                    self.save_default_session();
                }
                crate::ui::layout::vdivider(ui);

                // ── View: 패널 토글 (Library/Outline/Bookmarks/Palette) ──
                crate::ui::layout::group(ui, crate::ui::layout::SP_2, |ui| {
                    self.toolbar_panel_group(ui);
                });
                crate::ui::layout::vdivider(ui);

                // ── Edit: 실행취소 · 재실행 · 페이지 비우기 ─────────────
                crate::ui::layout::group(ui, crate::ui::layout::SP_2, |ui| {
                    let undo = icon_button(
                        ui,
                        IconButton::new(icons::ARROW_COUNTER_CLOCKWISE, "Undo")
                            .enabled(self.history.can_undo())
                            .hint("Undo (Ctrl+Z)"),
                    );
                    crate::app::dev::tag_button(ui, "toolbar.undo", "Undo", &undo);
                    if undo.clicked() {
                        self.undo();
                    }
                    let redo = icon_button(
                        ui,
                        IconButton::new(icons::ARROW_CLOCKWISE, "Redo")
                            .enabled(self.history.can_redo())
                            .hint("Redo (Ctrl+Y)"),
                    );
                    crate::app::dev::tag_button(ui, "toolbar.redo", "Redo", &redo);
                    if redo.clicked() {
                        self.redo();
                    }
                    let clear_page = icon_button(
                        ui,
                        IconButton::new(icons::X_CIRCLE, "Clear Page")
                            .hint("Clear all ink on this page"),
                    );
                    crate::app::dev::tag_button(
                        ui,
                        "toolbar.clear_page",
                        "Clear Page",
                        &clear_page,
                    );
                    if clear_page.clicked() {
                        self.clear_page();
                    }
                });
                crate::ui::layout::vdivider(ui);

                // ── File: 주석 저장 · 불러오기 ─────────────────────────
                crate::ui::layout::group(ui, crate::ui::layout::SP_2, |ui| {
                    let save_edits = icon_button(
                        ui,
                        IconButton::new(icons::FLOPPY_DISK, "Save Edits")
                            .hint("Save annotations (Ctrl+S)"),
                    );
                    crate::app::dev::tag_button(
                        ui,
                        "toolbar.save_edits",
                        "Save Edits",
                        &save_edits,
                    );
                    if save_edits.clicked() {
                        self.save_annotations();
                    }
                    let load_edits = icon_button(
                        ui,
                        IconButton::new(icons::FOLDER_SIMPLE, "Load Edits")
                            .hint("Load annotations"),
                    );
                    crate::app::dev::tag_button(
                        ui,
                        "toolbar.load_edits",
                        "Load Edits",
                        &load_edits,
                    );
                    if load_edits.clicked() {
                        self.load_annotations();
                    }
                });
                crate::ui::layout::vdivider(ui);

                // ── App: 전역 설정(단일 홈) + 오버플로 ─────────────────
                let settings = icon_button(
                    ui,
                    IconButton::new(icons::GEAR, "Settings").hint(
                        "Open Settings — every preference in one tabbed window\n\
                         (draw, cursor, paper, canvas, color wheel, page,\n\
                         edge scroll, window focus, server, macros, gamepad).",
                    ),
                );
                crate::app::dev::tag_button(ui, "toolbar.settings", "Settings", &settings);
                if settings.clicked() {
                    self.settings_open = true;
                }
                crate::ui::layout::group(ui, crate::ui::layout::SP_2, |ui| {
                    self.overflow_menu(ui);
                });
            });
        });
    }

    /// 컴포넌트(그룹 1-패널): Library / Outline / Bookmarks / Palette 토글 묶음.
    /// Library·Outline·Bookmarks는 상호 배타적(하나를 켜면 나머지 자동 해제)이고,
    /// Palette는 독립 토글입니다. (React의 <PanelToggles>에 해당.)
    fn toolbar_panel_group(&mut self, ui: &mut egui::Ui) {
        let library_toggle = icon_toggle(
            ui,
            &mut self.show_library,
            icons::NOTEBOOK,
            "Library",
            "Library (notes, PDFs, recents) — exclusive",
        );
        crate::app::dev::tag_toggle(
            ui,
            "toolbar.panel.library",
            "Library",
            &library_toggle,
            self.show_library,
        );
        if library_toggle.changed() {
            if self.show_library {
                [self.show_library, self.show_outline, self.show_bookmarks] =
                    exclusive_panel_on(PanelKind::Library);
            }
            self.save_session();
        }
        let outline_toggle = icon_toggle(
            ui,
            &mut self.show_outline,
            icons::LIST_BULLETS,
            "Outline",
            "Outline — exclusive",
        );
        crate::app::dev::tag_toggle(
            ui,
            "toolbar.panel.outline",
            "Outline",
            &outline_toggle,
            self.show_outline,
        );
        if outline_toggle.changed() {
            if self.show_outline {
                [self.show_library, self.show_outline, self.show_bookmarks] =
                    exclusive_panel_on(PanelKind::Outline);
            }
            self.save_session();
        }
        let bookmarks_toggle = icon_toggle(
            ui,
            &mut self.show_bookmarks,
            icons::BOOKMARKS_SIMPLE,
            "Bookmarks",
            "Bookmarked pages — exclusive",
        );
        crate::app::dev::tag_toggle(
            ui,
            "toolbar.panel.bookmarks",
            "Bookmarks",
            &bookmarks_toggle,
            self.show_bookmarks,
        );
        if bookmarks_toggle.changed() {
            if self.show_bookmarks {
                [self.show_library, self.show_outline, self.show_bookmarks] =
                    exclusive_panel_on(PanelKind::Bookmarks);
            }
        }
        let palette_toggle = icon_toggle(
            ui,
            &mut self.show_palette,
            icons::PALETTE,
            "Palette",
            "Writing-tool color palette (right side of canvas)",
        );
        crate::app::dev::tag_toggle(
            ui,
            "toolbar.panel.palette",
            "Palette",
            &palette_toggle,
            self.show_palette,
        );
        if palette_toggle.changed() {
            self.save_default_session();
        }
    }

    /// 컴포넌트(그룹 6): More 오버플로 — 자주 안 쓰는 액션을 **주제별 섹션**으로.
    /// (React의 <OverflowMenu>에 해당하는 컨테이너 컴포넌트.)
    fn overflow_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button(icon_text(ui, "More", icons::DOTS_THREE), |ui| {
            ui.set_min_width(crate::ui::scale::rem(17)); // 272px

            // ── Page: 페이지 정렬 ───────────────────────────────────
            menu_section(ui, "Page");
            let aligns = [
                (PageAlign::Left, icons::TEXT_ALIGN_LEFT, "Align left"),
                (PageAlign::Center, icons::TEXT_ALIGN_CENTER, "Align center"),
                (PageAlign::Right, icons::TEXT_ALIGN_RIGHT, "Align right"),
            ];
            ui.horizontal(|ui| {
                for (a, ic, hint) in aligns {
                    if icon_select(ui, self.page_align == a, ic, "", hint).clicked() {
                        self.page_align = a;
                        self.realign();
                        self.save_session();
                    }
                }
            });

            // ── View: 미디어 패널 ───────────────────────────────────
            menu_section(ui, "View");
            if ui
                .checkbox(&mut self.show_media, "Media panel")
                .on_hover_text("Show the audio recordings panel.")
                .changed()
            {
                self.media_refresh();
            }

            // ── Lookup: 사전 오버레이 ───────────────────────────────
            menu_section(ui, "Lookup");
            if ui
                .checkbox(&mut self.dictionary.enabled, "Dictionary")
                .on_hover_text("Look up a tapped word (needs internet once per word).")
                .changed()
            {
                self.save_default_session();
            }

            // ── Input: 창 포커스 · 매크로 · 게임패드 ─────────────────
            menu_section(ui, "Input");
            ui.horizontal(|ui| {
                if ui
                    .checkbox(&mut self.window_focus_on_move, "Focus on cursor dwell")
                    .on_hover_text(
                        "Focus this window when the cursor stays over it for the dwell time.\n\
                         Turn off for windows that should not grab focus in split view.",
                    )
                    .changed()
                {
                    self.save_default_session();
                }
                if icon_button(
                    ui,
                    IconButton::new(icons::GEAR, "")
                        .frame(false)
                        .hint("Window Focus settings — enable + dwell time"),
                )
                .clicked()
                {
                    self.window_focus_settings_open = true;
                }
            });
            if crate::ui::buttons::Button::secondary("Macros…")
                .show(ui)
                .clicked()
            {
                self.macro_capture = None;
                self.macro_settings_open = true;
            }
            if crate::ui::buttons::Button::secondary("Gamepad…")
                .show(ui)
                .clicked()
            {
                self.gamepad_settings_open = true;
            }

            // ── Server: 미디어 서버 ─────────────────────────────────
            menu_section(ui, "Server");
            if crate::ui::buttons::Button::secondary("Media server settings…")
                .show(ui)
                .clicked()
            {
                self.server_msg = None;
                self.server_settings_open = true;
            }

            // ── Maintenance: 캐시 정리 ──────────────────────────────
            menu_section(ui, "Maintenance");
            ui.menu_button("Cache…", |ui| {
                // 등록된 모든 캐시 순회 — actions/cache.rs의 all_caches()에
                // 등록만 하면 여기에 자동으로 나타납니다.
                for (i, cache) in all_caches().iter().enumerate() {
                    if i > 0 {
                        ui.separator();
                    }
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

            // ── Diagnostics: 디버그 HUD (단일 홈) ───────────────────
            menu_section(ui, "Diagnostics");
            if ui
                .checkbox(&mut self.debug_hud, "Debug HUD")
                .on_hover_text("Live input overlay & diagnostics (pressure, tilt, speed, system).")
                .changed()
            {
                self.save_default_session();
            }
        });
    }

    /// Row 2 — Page: 페이지 조작 · 종이(스타일/색) · 캔버스(배경/엣지).
    /// 세 그룹 모두 "현재 페이지의 모양"을 다루므로 한 행에 모읍니다.
    pub(crate) fn row_page(&mut self, ui: &mut egui::Ui) {
        toolbar_row(ui, "page", |ui| {
            ui.horizontal(|ui| {
                self.page_group(ui);
                crate::ui::layout::vdivider(ui);
                self.paper_group(ui);
                crate::ui::layout::vdivider(ui);
                self.canvas_group(ui);
            });
        });
    }

    /// 그룹(2-1) Page: 삽입 · 삭제 · 회전.
    fn page_group(&mut self, ui: &mut egui::Ui) {
        crate::ui::layout::group(ui, crate::ui::layout::SP_2, |ui| {
            icon_label(ui, icons::FILES, "Page");
            let page_count = self.document.as_ref().map(|d| d.page_count()).unwrap_or(0);
            // 메뉴 안에서는 숫자 타이핑이 닫히므로 **전용 창**을 엽니다.
            let insert_page = icon_button(
                ui,
                IconButton::new(icons::PLUS_SQUARE, "Insert Page")
                    .hint("Insert blank pages — opens a small window"),
            );
            crate::app::dev::tag_button(ui, "toolbar.insert_page", "Insert Page", &insert_page);
            if insert_page.clicked() {
                self.insert_page_open = true;
            }
            let delete_page = icon_button(
                ui,
                IconButton::new(icons::TRASH_SIMPLE, "Delete Page")
                    .enabled(page_count > 1)
                    .hint("Delete this page"),
            );
            crate::app::dev::tag_button(ui, "toolbar.delete_page", "Delete Page", &delete_page);
            if delete_page.clicked() {
                self.delete_page_action();
            }
            ui.menu_button(icon_text(ui, "Rotate", icons::REPEAT), |ui| {
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
        });
    }

    /// 그룹(2-2) Paper: 스타일 · 배경색 · 세부 설정(창).
    fn paper_group(&mut self, ui: &mut egui::Ui) {
        crate::ui::layout::group(ui, crate::ui::layout::SP_2, |ui| {
            // 현재 페이지에 즉시 적용 + 새 페이지/노트의 기본값.
            icon_label(ui, icons::RULER, "Paper");
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
            // 프리셋 색 스와치 (클릭 적용 / 더블클릭 편집) + 커스텀 색.
            crate::ui::layout::hstack(ui, crate::ui::layout::SP_1, |ui| {
                for (i, paper) in PAPER_COLORS.iter().enumerate() {
                    let mut color =
                        Color32::from_rgba_unmultiplied(paper[0], paper[1], paper[2], paper[3]);
                    let selected = self.paper_color == *paper;
                    let (resp, changed) =
                        swatch_with_picker(ui, ("paper_swatch", i), &mut color, selected);
                    let resp = resp.on_hover_text(
                        "Paper color — click to apply, double-click to edit (current page)",
                    );
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
                // 프리셋 외 원하는 배경색을 직접 고릅니다.
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
            });
            // 세부 설정(크기/간격/줄 색·두께/전체 적용) 전용 창.
            if icon_button(
                ui,
                IconButton::new(icons::GEAR, "").frame(false).hint(
                    "Paper settings — page size, grid spacing,\n\
                     line color & thickness, apply to all.",
                ),
            )
            .clicked()
            {
                self.paper_settings_open = true;
            }
        });
    }

    /// 그룹(2-3) Canvas: 페이지 뒤 배경색 · 엣지 자동 스크롤.
    fn canvas_group(&mut self, ui: &mut egui::Ui) {
        crate::ui::layout::group(ui, crate::ui::layout::SP_2, |ui| {
            icon_label(ui, icons::SQUARES_FOUR, "Canvas");
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
        });
    }

    /// Row 3 — Ink: 도구 피커(드래그 재정렬) · 도구별 색/굵기 · 표시 페이싱.
    pub(crate) fn row_ink(&mut self, ui: &mut egui::Ui) {
        toolbar_row(ui, "ink", |ui| {
            ui.horizontal(|ui| {
                self.ink_tool_picker(ui);
                crate::ui::layout::vdivider(ui);
                self.ink_tool_options(ui);
                crate::ui::layout::vdivider(ui);
                self.ink_pacing_group(ui);
            });
        });
    }

    /// 그룹(3-1) Tools: 도구 선택기 (드래그 앤 드롭 재정렬 — 특수 로직 유지).
    fn ink_tool_picker(&mut self, ui: &mut egui::Ui) {
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
                crate::app::dev::tag_selected_button(ui, dev_tool_id(tool), label, &resp, selected);
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
        });
    }

    /// 그룹(3-2) Tool options: 현재 도구의 색/굵기/토글 + 세부 설정 기어.
    fn ink_tool_options(&mut self, ui: &mut egui::Ui) {
        crate::ui::layout::group(ui, crate::ui::layout::SP_2, |ui| {
            match self.tool {
                ToolType::Pen => self.ink_pen_options(ui),
                ToolType::Fountain => self.ink_fountain_options(ui),
                ToolType::Highlighter => self.ink_highlighter_options(ui),
                ToolType::Eraser => self.ink_eraser_options(ui),
                ToolType::Pan => {
                    icon_label(ui, icons::HAND, "Drag to pan the page");
                }
            }
            // 도구별 세부 설정 — Draw 탭으로 라우팅되는 단일 기어.
            // (설정 진입은 Row1의 전역 Settings와 이 기어, 총 2곳.)
            let hint = match self.tool {
                ToolType::Pen => Some(
                    "Pen settings — physics model, ink soak,\n\
                     ink grain, cursor, smoothing.",
                ),
                ToolType::Fountain => Some(
                    "Fountain settings — physics model, ink soak,\n\
                     ink grain, italic nib, dwell.",
                ),
                _ => None,
            };
            if let Some(hint) = hint {
                if icon_button(ui, IconButton::new(icons::GEAR, "").frame(false).hint(hint))
                    .clicked()
                {
                    self.tool_settings_open = true;
                }
            }
        });
    }

    /// 펜(볼펜) 옵션 — 색 계열/스와치/커스텀 색/폭/필압·왼손 토글.
    fn ink_pen_options(&mut self, ui: &mut egui::Ui) {
        egui::ComboBox::from_id_salt("pen_family")
            .selected_text(self.color_family.label())
            .show_ui(ui, |ui| {
                for family in ColorFamily::all() {
                    if ui
                        .selectable_value(&mut self.color_family, family, family.label())
                        .changed()
                    {
                        self.save_session();
                    }
                }
            })
            .response
            .on_hover_text("Pen color family");
        let swatches = Palette::swatches(self.color_family);
        crate::ui::layout::hstack(ui, crate::ui::layout::SP_1, |ui| {
            for (i, swatch) in swatches.iter().enumerate() {
                let mut color =
                    Color32::from_rgba_unmultiplied(swatch[0], swatch[1], swatch[2], swatch[3]);
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
        });
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
        if crate::ui::slider(
            ui,
            &mut self.pen_width,
            0.5..=12.0,
            "Width",
            "Base line width (pt). The ballpen model varies it only \
             a little (±30%) by pressure & speed.",
        )
        .changed()
        {
            self.save_session();
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


    /// 만년필 옵션 — 색 스와치/커스텀 색/닙 굵기.
    fn ink_fountain_options(&mut self, ui: &mut egui::Ui) {
        let swatches = Palette::swatches(self.color_family);
        crate::ui::layout::hstack(ui, crate::ui::layout::SP_1, |ui| {
            for (i, swatch) in swatches.iter().enumerate() {
                let mut color =
                    Color32::from_rgba_unmultiplied(swatch[0], swatch[1], swatch[2], swatch[3]);
                let selected = *swatch == self.fountain_color;
                let (resp, changed) =
                    swatch_with_picker(ui, ("fountain_swatch", i), &mut color, selected);
                let resp =
                    resp.on_hover_text("Ink color — click to apply, double-click to edit");
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
        });
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
        if crate::ui::slider(
            ui,
            &mut self.fountain_width,
            0.5..=12.0,
            "Nib",
            "Nib width = maximum line width (pt).\n\
             The model varies it by pressure, speed and tilt.",
        )
        .changed()
        {
            self.save_session();
        }
    }

    /// 하이라이터 옵션 — 파스텔 스와치/커스텀 색/굵기/텍스트 스냅.
    fn ink_highlighter_options(&mut self, ui: &mut egui::Ui) {
        let swatches = Palette::highlighter_swatches();
        crate::ui::layout::hstack(ui, crate::ui::layout::SP_1, |ui| {
            for (i, swatch) in swatches.iter().enumerate() {
                let mut color =
                    Color32::from_rgba_unmultiplied(swatch[0], swatch[1], swatch[2], swatch[3]);
                let selected = *swatch == self.hi_color;
                let (resp, changed) =
                    swatch_with_picker(ui, ("hi_swatch", i), &mut color, selected);
                let resp = resp
                    .on_hover_text("Highlighter color — click to apply, double-click to edit");
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
        });
        let mut color = Color32::from_rgba_unmultiplied(
            self.hi_color[0],
            self.hi_color[1],
            self.hi_color[2],
            self.hi_color[3],
        );
        if ui
            .color_edit_button_srgba(&mut color)
            .on_hover_text("Custom highlighter color")
            .changed()
        {
            self.hi_color = color.to_array();
            self.save_session();
        }
        if crate::ui::slider(
            ui,
            &mut self.hi_width,
            4.0..=40.0,
            "Width",
            "Highlighter stroke width (pt).",
        )
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

    /// 지우개 옵션 — 반경.
    fn ink_eraser_options(&mut self, ui: &mut egui::Ui) {
        if crate::ui::slider(
            ui,
            &mut self.eraser_radius,
            4.0..=60.0,
            "Radius",
            "Eraser radius (px).",
        )
        .changed()
        {
            self.save_session();
        }
    }


    /// 그룹(3-3) Pacing: 모니터 주사율 프리셋 — 필기 페이싱을 한 번에 조정.
    fn ink_pacing_group(&mut self, ui: &mut egui::Ui) {
        crate::ui::layout::group(ui, crate::ui::layout::SP_2, |ui| {
            icon_label(ui, icons::LIGHTNING, "Pacing");
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