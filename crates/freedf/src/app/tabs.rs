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

    /// A tab can be launched in a separate OS window by re-running this
    /// executable with a document path. Standalone PDFs reopen faithfully
    /// (the sidecar annotations and per-file session are re-loaded by the new
    /// process). FreeDF notes share a single annotation JSON, so splitting one
    /// into a second window would race on autosave and lose ink — they are not
    /// offered here.
    pub(crate) fn tab_launch(&self, idx: usize) -> Option<PathBuf> {
        let tab = self.tabs.get(idx)?;
        match &tab.kind {
            TabKind::Pdf(path) if path.is_file() => Some(path.clone()),
            _ => None,
        }
    }

    /// Relaunches this executable as a separate OS window that opens `path`.
    /// eframe runs one window per process, so this is how "open in a new
    /// window" is realized (pdfium is also a single-binding-per-process lib).
    /// Returns `true` when the child process was spawned successfully.
    pub(crate) fn open_in_new_window(&mut self, path: &Path) -> bool {
        let exe = match std::env::current_exe() {
            Ok(e) => e,
            Err(e) => {
                self.status = Some(format!("Could not locate executable: {e}"));
                return false;
            }
        };
        match std::process::Command::new(&exe).arg(path).spawn() {
            Ok(_) => {
                self.status = Some(format!(
                    "Opened in a new window: {}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ));
                true
            }
            Err(e) => {
                self.status = Some(format!("Could not open a new window: {e}"));
                false
            }
        }
    }

    /// A standalone PDF tab is "moved" to a new window: its ink is flushed to
    /// the sidecar file (so nothing is lost) so the tab can be closed here.
    pub(crate) fn save_tab_sidecar(&mut self, idx: usize) {
        // 활성 탭이면 문서 상태를 탭 항목으로 먼저 옮깁니다.
        if idx == self.active && self.document.is_some() {
            self.capture_into(idx);
        }
        let Some(tab) = self.tabs.get(idx) else {
            return;
        };
        if tab.store.total_stroke_count() == 0 {
            return;
        }
        let Some(path) = &tab.file_path else {
            return;
        };
        let json = tab.store.to_json();
        if std::fs::write(annotation_path_for(path), json).is_err() {
            self.status =
                Some("Could not save annotations before moving the tab".to_string());
        }
    }

    /// 현재 활성 문서 상태를 `tabs[idx]`에 복사해 둡니다.
    pub(crate) fn capture_into(&mut self, idx: usize) {
        let Some(tab) = self.tabs.get_mut(idx) else {
            return;
        };
        tab.label = self.file_name.clone();
        tab.file_path = self.file_path.clone();
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
        tab.hi_color = self.hi_color;
        tab.hi_width = self.hi_width;
        tab.eraser_radius = self.eraser_radius;
        tab.pressure_enabled = self.pressure_enabled;
        tab.pressure_curve = self.pressure_curve;
        tab.paper_style = self.paper_style;
        tab.paper_color = self.paper_color;
        tab.paper_size = self.paper_size;
        tab.paper_spacing = self.paper_spacing;
        tab.paper_line_color = self.paper_line_color;
        tab.paper_line_width = self.paper_line_width;
    }

    /// `tabs[idx]`의 상태를 활성 문서로 복원합니다. (활성 탭의 document는 None이 됨)
    pub(crate) fn restore_from(&mut self, idx: usize) {
        let (
            label,
            file_path,
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
            hi_color,
            hi_width,
            eraser_radius,
            pressure_enabled,
            pressure_curve,
            paper_style,
            paper_color,
            paper_size,
            paper_spacing,
            paper_line_color,
            paper_line_width,
        ) = {
            let tab = self.tabs.get_mut(idx).expect("tab index in range");
            (
                std::mem::take(&mut tab.label),
                tab.file_path.clone(),
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
                tab.hi_color,
                tab.hi_width,
                tab.eraser_radius,
                tab.pressure_enabled,
                tab.pressure_curve,
                tab.paper_style,
                tab.paper_color,
                tab.paper_size,
                tab.paper_spacing,
                tab.paper_line_color,
                tab.paper_line_width,
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
        self.file_path = file_path;
        self.current_note = current_note;
        self.document = document;
        self.current_page = current_page;
        self.page_size_pts = page_size_pts;
        self.view = view;
        self.page_align = page_align;
        self.store = store;
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
        self.hi_color = hi_color;
        self.hi_width = hi_width;
        self.eraser_radius = eraser_radius;
        self.pressure_enabled = pressure_enabled;
        self.pressure_curve = pressure_curve;
        self.paper_style = paper_style;
        self.paper_color = paper_color;
        self.paper_size = paper_size;
        self.paper_spacing = paper_spacing;
        self.paper_line_color = paper_line_color;
        self.paper_line_width = paper_line_width;
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
            file_path: self.file_path.clone(),
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
            hi_color: self.hi_color,
            hi_width: self.hi_width,
            eraser_radius: self.eraser_radius,
            pressure_enabled: self.pressure_enabled,
            pressure_curve: self.pressure_curve,
            paper_style: self.paper_style,
            paper_color: self.paper_color,
            paper_size: self.paper_size,
            paper_spacing: self.paper_spacing,
            paper_line_color: self.paper_line_color,
            paper_line_width: self.paper_line_width,
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
                                            // per process, so we relaunch the exe with
                                            // the file path).
                                            let launch_path = self.tab_launch(i);
                                            let is_note =
                                                matches!(&tab.kind, TabKind::Note(_));
                                            tr.context_menu(|ui| {
                                                ui.set_min_width(190.0);
                                                ui.label(
                                                    egui::RichText::new("Tab")
                                                        .weak()
                                                        .small(),
                                                );
                                                ui.separator();
                                                match launch_path {
                                                    Some(_) => {
                                                        if ui
                                                            .button("Open in new window")
                                                            .clicked()
                                                        {
                                                            to_detach = Some(i);
                                                            ui.close();
                                                        }
                                                    }
                                                    None if is_note => {
                                                        ui.add_enabled_ui(false, |ui| {
                                                            let _ =
                                                                ui.button("Open in new window");
                                                        })
                                                        .response
                                                        .on_hover_text(
                                                            "FreeDF notes share one \
                                                             annotation file — opening \
                                                             the same note in two windows \
                                                             would race and lose ink. \
                                                             Standalone PDFs can be split \
                                                             into a new window.",
                                                        );
                                                    }
                                                    _ => {
                                                        ui.add_enabled_ui(false, |ui| {
                                                            let _ =
                                                                ui.button("Open in new window");
                                                        })
                                                        .response
                                                        .on_hover_text("File no longer exists");
                                                    }
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
                    if let Some(p) = self.tab_launch(i) {
                        // 새 창이 뜨면 이 탭은 "이동(detach)"합니다: 잉크를
                        // 사이드카에 저장하고 이 창에서는 탭을 닫아 같은
                        // 문서가 두 창에 겹쳐 보이지 않게 합니다.
                        if self.open_in_new_window(&p) {
                            self.save_tab_sidecar(i);
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
