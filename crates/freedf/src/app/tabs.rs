//! Tab strip UI + tab lifecycle (open/close/switch) + detach to a new window.
//!
//! Extracted from `app.rs` during the layered refactor — these are
//! `FreeDfApp` methods living in a submodule (`use super::*` re-exports the
//! app state types and shared helpers).

use super::*;

impl FreeDfApp {
    pub(crate) fn find_tab(&self, kind: &TabKind) -> Option<usize> {
        self.tabs.iter().position(|t| &t.kind == kind)
    }

    /// 모든 탭은 DB 문서이므로, 새 창 실행 인자는 documents.id입니다.
    pub(crate) fn tab_launch(&self, idx: usize) -> Option<i64> {
        let tab = self.tabs.get(idx)?;
        Some(match tab.kind {
            TabKind::Note(id) | TabKind::Pdf(id) => id,
        })
    }

    /// Relaunches this executable as a separate OS window that opens the DB
    /// document `doc_id`. eframe runs one window per process, so this is how
    /// "open in a new window" is realized (pdfium is also a single-binding-
    /// per-process lib). Returns `true` when the child process spawned.
    pub(crate) fn open_in_new_window(&mut self, doc_id: i64) -> bool {
        let exe = match std::env::current_exe() {
            Ok(e) => e,
            Err(e) => {
                self.status = Some(format!("Could not locate executable: {e}"));
                return false;
            }
        };
        match std::process::Command::new(&exe)
            .arg("--doc")
            .arg(doc_id.to_string())
            .spawn()
        {
            Ok(_) => {
                self.status = Some("Opened in a new window".to_string());
                true
            }
            Err(e) => {
                self.status = Some(format!("Could not open a new window: {e}"));
                false
            }
        }
    }

    /// 탭을 새 창으로 보내기 전에 해당 문서를 DB에 플러시합니다.
    /// (공유 DB라 두 창이 같은 데이터를 보므로, 최신 상태만 보장하면 됨)
    pub(crate) fn flush_tab_to_db(&mut self, idx: usize) {
        // 활성 탭이면 문서 상태를 탭 항목으로 먼저 옮깁니다.
        if idx == self.active && self.document.is_some() {
            self.capture_into(idx);
        }
        let doc_id = match self.tabs.get(idx).map(|t| t.kind) {
            Some(TabKind::Note(id)) | Some(TabKind::Pdf(id)) => id,
            None => return,
        };
        // 비활성 탭이면 탭에 실데이터가 있습니다.
        if let Some(tab) = self.tabs.get_mut(idx) {
            self.db.resync_strokes(doc_id, &tab.store);
            if let Some(doc) = &tab.document {
                if let Ok(bytes) = doc.save_to_bytes() {
                    let _ = self.db.save_pdf(doc_id, &bytes);
                }
            }
        }
        if idx == self.active {
            // 캡처로 옮겼으니 복원해 현재 상태 유지 (탭이 곧 닫혀도 안전).
            self.restore_from(idx);
        }
    }

    /// 현재 활성 문서 상태를 `tabs[idx]`에 복사해 둡니다.
    pub(crate) fn capture_into(&mut self, idx: usize) {
        let Some(tab) = self.tabs.get_mut(idx) else {
            return;
        };
        tab.label = self.file_name.clone();
        tab.current_note = self.current_note;
        tab.document = self.document.take();
        tab.current_page = self.current_page;
        tab.page_size_pts = self.page_size_pts;
        tab.view = self.view;
        tab.page_align = self.page_align;
        tab.store = std::mem::take(&mut self.store);
        tab.history = std::mem::take(&mut self.history);
        tab.search_query = std::mem::take(&mut self.search_query);
        tab.search_matches = std::mem::take(&mut self.search_matches);
        tab.search_current = self.search_current.take();
        tab.outline = std::mem::take(&mut self.outline);
        tab.outline_loaded = self.outline_loaded;
        // Per-tab UI state.
        tab.show_library = self.show_library;
        tab.show_outline = self.show_outline;
        tab.show_search = self.show_search;
        tab.library_width = self.library_width;
        tab.outline_width = self.outline_width;
        tab.tool = self.tool;
        tab.color_family = self.color_family;
        tab.pen_color = self.pen_color;
        tab.pen_width = self.pen_width;
        tab.fountain_color = self.fountain_color;
        tab.fountain_width = self.fountain_width;
        tab.hi_color = self.hi_color;
        tab.hi_width = self.hi_width;
        tab.eraser_radius = self.eraser_radius;
        tab.pressure_enabled = self.pressure_enabled;
        tab.pen_profile = self.pen_profile;
        tab.paper_style = self.paper_style;
        tab.paper_color = self.paper_color;
        tab.paper_size = self.paper_size;
        tab.paper_spacing = self.paper_spacing;
        tab.paper_line_color = self.paper_line_color;
        tab.paper_line_width = self.paper_line_width;
        tab.custom_paper_size = self.custom_paper_size;
        tab.smoothing = self.smoothing;
        tab.smoothing_enabled = self.smoothing_enabled;
        tab.ink_bleed = self.ink_bleed;
        tab.pen_grain = self.pen_grain;
        tab.fountain_grain = self.fountain_grain;
        tab.fountain_profile = self.fountain_profile;
        tab.zoom_lock = self.zoom_lock;
    }

    /// `tabs[idx]`의 상태를 활성 문서로 복원합니다. (활성 탭의 document는 None이 됨)
    pub(crate) fn restore_from(&mut self, idx: usize) {
        let (
            label,
            kind,
            current_note,
            document,
            current_page,
            page_size_pts,
            view,
            page_align,
            store,
            history,
            search_query,
            search_matches,
            search_current,
            outline,
            outline_loaded,
            show_library,
            show_outline,
            show_search,
            library_width,
            outline_width,
            tool,
            color_family,
            pen_color,
            pen_width,
            fountain_color,
            fountain_width,
            hi_color,
            hi_width,
            eraser_radius,
            pressure_enabled,
            pen_profile,
            paper_style,
            paper_color,
            paper_size,
            paper_spacing,
            paper_line_color,
            paper_line_width,
            custom_paper_size,
            smoothing,
            smoothing_enabled,
            ink_bleed,
            pen_grain,
            fountain_grain,
            fountain_profile,
            zoom_lock,
        ) = {
            let tab = self.tabs.get_mut(idx).expect("tab index in range");
            (
                std::mem::take(&mut tab.label),
                tab.kind,
                tab.current_note,
                tab.document.take(),
                tab.current_page,
                tab.page_size_pts,
                tab.view,
                tab.page_align,
                std::mem::take(&mut tab.store),
                std::mem::take(&mut tab.history),
                std::mem::take(&mut tab.search_query),
                std::mem::take(&mut tab.search_matches),
                tab.search_current.take(),
                std::mem::take(&mut tab.outline),
                tab.outline_loaded,
                tab.show_library,
                tab.show_outline,
                tab.show_search,
                tab.library_width,
                tab.outline_width,
                tab.tool,
                tab.color_family,
                tab.pen_color,
                tab.pen_width,
                tab.fountain_color,
                tab.fountain_width,
                tab.hi_color,
                tab.hi_width,
                tab.eraser_radius,
                tab.pressure_enabled,
                tab.pen_profile,
                tab.paper_style,
                tab.paper_color,
                tab.paper_size,
                tab.paper_spacing,
                tab.paper_line_color,
                tab.paper_line_width,
                tab.custom_paper_size,
                tab.smoothing,
                tab.smoothing_enabled,
                tab.ink_bleed,
                tab.pen_grain,
                tab.fountain_grain,
                tab.fountain_profile,
                tab.zoom_lock,
            )
        };
        // 일시적인 렌더/입력 상태 초기화.
        self.texture = None;
        self.render_dirty = true;
        self.pending_fit = None;
        self.page_anim = None;
        self.prev_texture = None;
        self.active_stroke = None;
        self.pan_last = None;
        self.middle_pan_last = None;
        self.scroll_vel = Vec2::ZERO;
        self.transition_last_page = current_page;
        self.file_name = label;
        self.doc_id = Some(match kind {
            TabKind::Note(id) | TabKind::Pdf(id) => id,
        });
        self.current_note = current_note;
        self.document = document;
        self.current_page = current_page;
        self.page_size_pts = page_size_pts;
        self.view = view;
        self.page_align = page_align;
        self.set_store(store);
        self.history = history;
        self.search_query = search_query;
        self.search_matches = search_matches;
        self.search_current = search_current;
        self.outline = outline;
        self.outline_loaded = outline_loaded;
        // Per-tab UI state (panels, tools, paper).
        self.show_library = show_library;
        self.show_outline = show_outline;
        self.show_search = show_search;
        self.library_width = library_width;
        self.outline_width = outline_width;
        self.tool = tool;
        self.color_family = color_family;
        self.pen_color = pen_color;
        self.pen_width = pen_width;
        self.fountain_color = fountain_color;
        self.fountain_width = fountain_width;
        self.hi_color = hi_color;
        self.hi_width = hi_width;
        self.eraser_radius = eraser_radius;
        self.pressure_enabled = pressure_enabled;
        self.pen_profile = pen_profile;
        self.paper_style = paper_style;
        self.paper_color = paper_color;
        self.paper_size = paper_size;
        self.paper_spacing = paper_spacing;
        self.paper_line_color = paper_line_color;
        self.paper_line_width = paper_line_width;
        self.custom_paper_size = custom_paper_size;
        self.smoothing = smoothing;
        self.smoothing_enabled = smoothing_enabled;
        self.ink_bleed = ink_bleed;
        self.pen_grain = pen_grain;
        self.fountain_grain = fountain_grain;
        self.fountain_profile = fountain_profile;
        self.zoom_lock = zoom_lock;
        self.search_runs = Vec::new();
        self.status = None;
        self.search_update();
    }

    /// 활성 탭을 `idx`로 전환합니다.
    pub(crate) fn switch_tab(&mut self, idx: usize) {
        if idx >= self.tabs.len() || idx == self.active {
            return;
        }
        self.save_session();
        self.capture_into(self.active);
        self.restore_from(idx);
        self.active = idx;
    }

    /// 탭을 닫습니다. 활성 탭이면 인접 탭으로 전환합니다.
    pub(crate) fn close_tab(&mut self, idx: usize) {
        if idx >= self.tabs.len() {
            return;
        }
        if idx == self.active {
            self.close_document();
            self.tabs.remove(idx);
            if self.tabs.is_empty() {
                return;
            }
            let new_active = idx.min(self.tabs.len() - 1);
            self.restore_from(new_active);
            self.active = new_active;
        } else {
            self.tabs.remove(idx);
            if idx < self.active {
                self.active -= 1;
            }
        }
    }

    /// 현재 활성 문서를 새 탭으로 추가합니다 (document는 self에 남아 활성 상태).
    pub(crate) fn add_current_as_tab(&mut self, kind: TabKind) {
        let label = self.file_name.clone();
        // 활성 탭의 실제 데이터는 self에 유지합니다 (document/store/…).
        // 탭 항목에는 전환 시 capture_into가 채워 넣으므로 빈 값으로 둡니다.
        let tab = TabEntry {
            kind,
            label,
            current_note: self.current_note,
            document: None,
            current_page: self.current_page,
            page_size_pts: self.page_size_pts,
            view: self.view,
            page_align: self.page_align,
            store: AnnotationStore::new(),
            history: History::new(256),
            search_query: String::new(),
            search_matches: Vec::new(),
            search_current: None,
            outline: Vec::new(),
            outline_loaded: false,
            show_library: self.show_library,
            show_outline: self.show_outline,
            show_search: self.show_search,
            library_width: self.library_width,
            outline_width: self.outline_width,
            tool: self.tool,
            color_family: self.color_family,
            pen_color: self.pen_color,
            pen_width: self.pen_width,
            fountain_color: self.fountain_color,
            fountain_width: self.fountain_width,
            hi_color: self.hi_color,
            hi_width: self.hi_width,
            eraser_radius: self.eraser_radius,
            pressure_enabled: self.pressure_enabled,
            pen_profile: self.pen_profile,
            paper_style: self.paper_style,
            paper_color: self.paper_color,
            paper_size: self.paper_size,
            paper_spacing: self.paper_spacing,
            paper_line_color: self.paper_line_color,
            paper_line_width: self.paper_line_width,
            custom_paper_size: self.custom_paper_size,
            smoothing: self.smoothing,
            smoothing_enabled: self.smoothing_enabled,
            ink_bleed: self.ink_bleed,
            pen_grain: self.pen_grain,
            fountain_grain: self.fountain_grain,
            fountain_profile: self.fountain_profile,
            zoom_lock: self.zoom_lock,
        };
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
    }

    // ---------- Recent files ----------

    pub(crate) fn tabs_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("tabs_bar").show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
            ui.add_space(3.0);
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button(icon_text(ui, "New", icons::PLUS))
                    .on_hover_text("New note (Ctrl+N)")
                    .clicked()
                {
                    self.modal = Some(ModalState::ask_text(
                        "New Note",
                        "Note title:",
                        TextAction::NewNote,
                    ));
                }
                if ui
                    .button(icon_text(ui, "Open", icons::FOLDER_OPEN))
                    .on_hover_text("Open PDF (Ctrl+O)")
                    .clicked()
                {
                    self.open_file_dialog();
                }
                ui.separator();

                if self.tabs.is_empty() {
                    ui.label(egui::RichText::new("No documents open").weak());
                    return;
                }
                let mut to_switch: Option<usize> = None;
                let mut to_close: Option<usize> = None;
                let mut to_detach: Option<usize> = None;
                let active_fill = crate::theme::nord::semantic::BG_SURFACE;
                let accent = crate::theme::nord::semantic::ACCENT_ACTIVE;
                let weak_border = crate::theme::nord::semantic::OVERLAY_BORDER;
                // Scrollable tab strip: many tabs or long titles scroll
                // instead of wrapping to a new line ("folding").
                egui::ScrollArea::horizontal()
                    .id_salt("tabs_scroll")
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(5.0, 4.0);
                            for (i, tab) in self.tabs.iter().enumerate() {
                                let selected = i == self.active;
                                // The active tab's title lives in `self.file_name`
                                // (its `tab.label` is emptied by restore_from);
                                // inactive tabs keep their own label.
                                let title: &str = if selected {
                                    &self.file_name
                                } else {
                                    &tab.label
                                };
                                // 제목 폭을 내용에 맞춰(최소 110px, 최대 190px) — 너무
                                // 좁아져 제목이 깨지지 않으면서, 짧은 제목이 큰
                                // 빈 여백을 만들지도 않습니다.
                                let title_w = ui
                                    .painter()
                                    .layout_no_wrap(
                                        title.to_string(),
                                        egui::FontId::proportional(14.0),
                                        egui::Color32::WHITE,
                                    )
                                    .rect
                                    .width()
                                    .clamp(110.0, 190.0);
                                egui::Frame::new()
                                    .fill(if selected {
                                        active_fill
                                    } else {
                                        egui::Color32::TRANSPARENT
                                    })
                                    .stroke(if selected {
                                        Stroke::new(1.0, accent)
                                    } else {
                                        Stroke::new(1.0, weak_border)
                                    })
                                    .corner_radius(5)
                                    .inner_margin(egui::Margin::symmetric(6, 3))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
                                            let tr = ui.add_sized(
                                                egui::vec2(title_w, 22.0),
                                                egui::Label::new(egui::RichText::new(title))
                                                    .truncate()
                                                    .sense(egui::Sense::click()),
                                            );
                                            let tr = tr.on_hover_text(title);
                                            if tr.clicked() {
                                                to_switch = Some(i);
                                            }
                                            // Right-click a tab: open this document in
                                            // a separate OS window (eframe = 1 window
                                            // per process). DB를 공유하므로 노트/PDF
                                            // 모두 분리 가능합니다.
                                            tr.context_menu(|ui| {
                                                ui.set_min_width(190.0);
                                                ui.label(
                                                    egui::RichText::new("Tab")
                                                        .weak()
                                                        .small(),
                                                );
                                                ui.separator();
                                                if ui.button("Open in new window").clicked() {
                                                    to_detach = Some(i);
                                                    ui.close();
                                                }
                                            });
                                            let close = ui.add(
                                                egui::Button::new(icon_text(ui, "", icons::X))
                                                    .frame(false)
                                                    .small(),
                                            );
                                            if close.on_hover_text("Close document").clicked() {
                                                to_close = Some(i);
                                            }
                                        });
                                    });
                            }
                        });
                    });
                if let Some(i) = to_close {
                    self.close_tab(i);
                }
                if let Some(i) = to_switch {
                    self.switch_tab(i);
                }
                if let Some(i) = to_detach {
                    if let Some(doc_id) = self.tab_launch(i) {
                        // 새 창이 뜨면 이 탭은 "이동(detach)"합니다: 공유 DB에
                        // 최신 상태를 플러시하고 이 창에서는 탭을 닫아 같은
                        // 문서가 두 창에 겹쳐 보이지 않게 합니다.
                        if self.open_in_new_window(doc_id) {
                            self.flush_tab_to_db(i);
                            self.close_tab(i);
                            if self.tabs.is_empty() {
                                self.close_document();
                            }
                        }
                    }
                }
            });
        });
    }
}
