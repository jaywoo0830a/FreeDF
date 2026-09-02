//! Document & note actions: open/save/export, page CRUD/rotate, search, bookmarks, undo/redo, shortcuts-free logic.
//!
//! Extracted from `app.rs` during the layered refactor — these are
//! `FreeDfApp` methods living in a submodule (`use super::*` re-exports the
//! app state types and shared helpers).

use super::*;

impl FreeDfApp {
    pub(crate) fn toggle_bookmark(&mut self, page: PageIndex) {
        self.store.toggle_bookmark(page);
        self.persist_bookmarks();
    }

    /// 모든 북마크 제거.
    pub(crate) fn clear_bookmarks(&mut self) {
        self.store.clear_bookmarks();
        self.persist_bookmarks();
    }

    /// 북마크를 디스크에 반영합니다 (노트는 자동저장, 일반 PDF는 사이드카).
    pub(crate) fn persist_bookmarks(&mut self) {
        if self.current_note.is_some() {
            self.autosave();
        } else if let Some(path) = self.file_path.clone() {
            let ann_path = annotation_path_for(&path);
            let json = self.store.to_json();
            let _ = std::fs::write(&ann_path, json);
        }
        self.save_session();
    }

    // ---------- Notes ----------

    /// Shows an error both in the status bar and as a popup alert.
    pub(crate) fn show_error(&mut self, msg: String) {
        self.status = Some(msg.clone());
        self.modal = Some(ModalState::alert("Error", &msg));
    }

    /// Creates a note's blank PDF using the PDFium instance cached at startup.
    pub(crate) fn create_blank_pdf_for_note(&self, path: &Path) -> Result<(), String> {
        match &self.pdfium {
            Ok(p) => DocumentView::create_blank_pdf_with(p, path, self.paper_size.size_pts()),
            Err(e) => Err(e.clone()),
        }
    }

    /// Returns a reference to the PDFium instance cached at startup.
    /// pdfium-render only allows one initialization per process, so everything
    /// must reuse this single instance (never call `load_pdfium` again).
    pub(crate) fn pdfium(&self) -> Result<&Pdfium, String> {
        self.pdfium.as_ref().map(|b| b.as_ref()).map_err(|e| e.clone())
    }

    pub(crate) fn create_note_action(&mut self, title: &str) {
        match self.notes.create_note(title) {
            Ok(meta) => {
                let pdf_path = self.notes.pdf_path(meta.id);
                if let Err(e) = self.create_blank_pdf_for_note(&pdf_path) {
                    self.show_error(e);
                    return;
                }
                let _ = self.notes.set_page_count(meta.id, 1);
                self.logger.log(AppEvent::NoteCreated {
                    note_id: meta.id,
                    title: meta.title.clone(),
                });
                self.open_note(meta.id);
            }
            Err(e) => self.status = Some(format!("Could not create note: {e}")),
        }
    }

    pub(crate) fn rename_note_action(&mut self, id: u64, title: &str) {
        let old = self
            .notes
            .get(id)
            .map(|m| m.title.clone())
            .unwrap_or_default();
        match self.notes.rename_note(id, title) {
            Ok(()) => {
                self.logger.log(AppEvent::NoteRenamed {
                    note_id: id,
                    from: old,
                    to: title.to_string(),
                });
                if self.current_note == Some(id) {
                    self.file_name = title.to_string();
                }
                let _ = self.notes.save();
            }
            Err(e) => self.status = Some(format!("Rename failed: {e}")),
        }
    }

    pub(crate) fn delete_note_action(&mut self, id: u64) {
        let title = self
            .notes
            .get(id)
            .map(|m| m.title.clone())
            .unwrap_or_default();
        match self.notes.delete_note(id) {
            Ok(()) => {
                self.logger.log(AppEvent::NoteDeleted { note_id: id, title });
                // 열려 있는 탭이면 닫고, 아니면 문서 상태 정리.
                if let Some(idx) = self.find_tab(&TabKind::Note(id)) {
                    self.close_tab(idx);
                } else if self.current_note == Some(id) {
                    self.close_document();
                }
                // 최근 목록에서도 제거.
                self.recents
                    .items
                    .retain(|r| !(r.kind == RecentKind::Note && r.note_id == Some(id)));
                self.recents.save(&self.recent_path);
                let _ = self.notes.save();
            }
            Err(e) => self.status = Some(format!("Delete failed: {e}")),
        }
    }

    pub(crate) fn close_document(&mut self) {
        self.document = None;
        self.current_note = None;
        self.texture = None;
        self.store = AnnotationStore::new();
        self.history = History::new(256);
        self.active_stroke = None;
        self.search_matches = Vec::new();
        self.search_current = None;
        self.outline = Vec::new();
        self.outline_loaded = false;
        self.file_name = String::new();
        self.file_path = None;
        self.scroll_vel = Vec2::ZERO;
        self.page_anim = None;
        self.prev_texture = None;
        self.transition_last_page = 0;
        self.status = None;
    }

    pub(crate) fn open_note(&mut self, id: u64) {
        let Some(meta) = self.notes.get(id).cloned() else {
            return;
        };
        // 이미 열려 있으면 해당 탭으로 전환만 합니다.
        if let Some(idx) = self.find_tab(&TabKind::Note(id)) {
            self.switch_tab(idx);
            return;
        }
        // 현재 활성 문서 상태를 탭에 보존하고 새 문서를 엽니다.
        self.save_session();
        if self.document.is_some() {
            self.capture_into(self.active);
        }
        let pdf_path = self.notes.pdf_path(id);
        if !pdf_path.exists() {
            if let Err(e) = self.create_blank_pdf_for_note(&pdf_path) {
                self.show_error(e);
                return;
            }
        }
        let ann_path = self.notes.annotations_path(id);
        let store = if ann_path.exists() {
            std::fs::read_to_string(&ann_path)
                .ok()
                .and_then(|t| AnnotationStore::from_json(&t).ok())
                .unwrap_or_default()
        } else {
            AnnotationStore::new()
        };
        let opened = self.pdfium().and_then(|p| DocumentView::open(p, &pdf_path));
        match opened {
            Ok(doc) => {
                self.current_note = Some(id);
                self.current_page = 0;
                self.page_size_pts = doc.page_size_pts(0);
                self.file_name = meta.title.clone();
                self.file_path = Some(pdf_path);
                self.document = Some(doc);
                self.store = store;
                self.history = History::new(256);
                self.active_stroke = None;
                self.pan_last = None;
                self.middle_pan_last = None;
                self.render_dirty = true;
                self.pending_fit = Some(FitMode::Width);
                self.outline_loaded = false;
                self.outline = Vec::new();
                self.search_matches = Vec::new();
                self.search_current = None;
                self.scroll_vel = Vec2::ZERO;
                self.page_anim = None;
                self.prev_texture = None;
                self.transition_last_page = 0;
                self.status = None;
                let page_count = self.document.as_ref().map(|d| d.page_count()).unwrap_or(0);
                // 마지막 세션(페이지/도구/펜/줌 등)을 복원합니다.
                let session_path = self.notes.session_path(id);
                if session_path.exists() {
                    let session = crate::settings::SessionState::load(&session_path);
                    self.apply_session(&session, page_count);
                    self.pending_fit = None;
                }
                self.logger.log(AppEvent::NoteOpened {
                    note_id: id,
                    title: meta.title.clone(),
                    page_count,
                });
                self.load_outline_if_needed();
                self.add_current_as_tab(TabKind::Note(id));
                self.note_recent(RecentKind::Note, meta.title.clone(), Some(id), None);
            }
            Err(e) => self.show_error(e),
        }
    }

    // ---------- Standalone PDF (Open button) ----------

    pub(crate) fn open_file_dialog(&mut self) {
        #[cfg(target_os = "windows")]
        {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("PDF files", &["pdf"])
                .pick_file()
            {
                self.open_pdf(&path);
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            self.modal = Some(ModalState::ask_text(
                "Open PDF",
                "Enter the PDF file path (e.g. C:/Users/me/doc.pdf)",
                TextAction::OpenPdf,
            ));
        }
    }

    pub(crate) fn open_pdf(&mut self, path: &Path) {
        // 이미 열려 있으면 해당 탭으로 전환만 합니다.
        if let Some(idx) = self.find_tab(&TabKind::Pdf(path.to_path_buf())) {
            self.switch_tab(idx);
            return;
        }
        // 현재 활성 문서 상태를 탭에 보존하고 새 문서를 엽니다.
        self.save_session();
        if self.document.is_some() {
            self.capture_into(self.active);
        }
        let opened = self.pdfium().and_then(|p| DocumentView::open(p, path));
        match opened {
            Ok(doc) => {
                self.current_note = None;
                self.current_page = 0;
                self.page_size_pts = doc.page_size_pts(0);
                self.file_name = doc.file_name.clone();
                self.file_path = Some(path.to_path_buf());
                self.document = Some(doc);
                self.store = AnnotationStore::new();
                self.history = History::new(256);
                self.active_stroke = None;
                self.render_dirty = true;
                self.pending_fit = Some(FitMode::Width);
                self.outline_loaded = false;
                self.outline = Vec::new();
                self.search_matches = Vec::new();
                self.search_current = None;
                self.scroll_vel = Vec2::ZERO;
                self.page_anim = None;
                self.prev_texture = None;
                self.transition_last_page = 0;
                self.status = None;

                // Auto-load a sidecar annotation file if present
                let ann_path = annotation_path_for(path);
                if ann_path.exists() {
                    if let Ok(text) = std::fs::read_to_string(&ann_path) {
                        if let Ok(store) = AnnotationStore::from_json(&text) {
                            self.store = store;
                            self.status = Some(format!(
                                "Loaded sidecar annotations: {}",
                                ann_path.file_name().unwrap_or_default().to_string_lossy()
                            ));
                        }
                    }
                }
                // 마지막 세션(페이지/도구/펜/줌 등)을 복원합니다.
                let page_count = self.document.as_ref().map(|d| d.page_count()).unwrap_or(0);
                let session_path = session_path_for(path);
                if session_path.exists() {
                    let session = crate::settings::SessionState::load(&session_path);
                    self.apply_session(&session, page_count);
                    self.pending_fit = None;
                }
                self.load_outline_if_needed();
                self.add_current_as_tab(TabKind::Pdf(path.to_path_buf()));
                self.note_recent(
                    RecentKind::File,
                    self.file_name.clone(),
                    None,
                    Some(path.to_path_buf()),
                );
            }
            Err(e) => self.show_error(e),
        }
    }

    // ---------- Pages ----------

    pub(crate) fn next_page(&mut self) {
        if let Some(doc) = &self.document {
            if self.current_page + 1 < doc.page_count() {
                self.current_page += 1;
                self.on_page_changed();
            }
        }
    }

    /// 다음 페이지로 이동 (키보드 PgDn). FreeDF 노트에서 이미 마지막
    /// 페이지라면 현재 페이지와 같은 크기/용지의 빈 페이지를 자동으로
    /// 추가해 계속 이어 씁니다.
    pub(crate) fn next_page_auto(&mut self) {
        let at_end = self
            .document
            .as_ref()
            .map(|d| self.current_page + 1 >= d.page_count())
            .unwrap_or(false);
        if at_end && self.current_note.is_some() {
            // 현재(마지막) 페이지의 크기/용지를 복사해 바로 다음에 삽입.
            self.insert_page_action(InsertTarget::FromCurrent);
        } else {
            self.next_page();
        }
    }

    pub(crate) fn prev_page(&mut self) {
        if self.current_page > 0 {
            self.current_page -= 1;
            self.on_page_changed();
        }
    }

    /// 브라우저식 PgUp/PgDn 동작 (핵심 판정은 core의 `browser_page_step`).
    ///
    /// - 페이지가 캔버스보다 길면(세로 스크롤 여지) **한 뷰포트만** 이동하고,
    ///   페이지를 넘기지 않습니다.
    /// - 더 스크롤할 수 없을 때만 다음/이전 페이지로 넘어갑니다
    ///   (노트는 `next_page_auto`라서 마지막 페이지면 새 페이지가 자동 추가됨).
    pub(crate) fn page_key(&mut self, down: bool) {
        if self.document.is_none() {
            return;
        }
        let canvas_h = self.last_canvas[1].max(1.0);
        let step = freedf_core::transform::browser_page_step(
            self.page_size_pts[1],
            self.view.zoom,
            canvas_h,
            CANVAS_MARGIN,
            self.view.pan_y,
            down,
        );
        match step {
            freedf_core::transform::PageStep::ScrollTo { pan_y } => {
                self.view.pan_y = pan_y;
            }
            freedf_core::transform::PageStep::NextPage => self.next_page_auto(),
            freedf_core::transform::PageStep::PrevPage => self.prev_page(),
        }
    }

    pub(crate) fn goto_page(&mut self, index: PageIndex) {
        if let Some(doc) = &self.document {
            if index < doc.page_count() {
                self.current_page = index;
                self.on_page_changed();
            }
        }
    }

    pub(crate) fn on_page_changed(&mut self) {
        let from = self.transition_last_page;
        self.active_stroke = None;
        self.pan_last = None;
        self.middle_pan_last = None;
        self.scroll_vel = Vec2::ZERO;
        if let Some(doc) = &self.document {
            self.page_size_pts = doc.page_size_pts(self.current_page);
        }
        self.render_dirty = true;
        // Keep the current zoom across page changes; just re-align the new page
        // (instead of resetting the zoom to fit-width).
        self.view
            .align_page(self.page_size_pts, self.last_canvas, TOP_MARGIN, self.page_align);
        self.search_update();
        if let Some(doc) = &self.document {
            self.logger.log(AppEvent::PageChanged {
                page: self.current_page,
                total: doc.page_count(),
            });
        }
        self.start_page_anim(from, self.current_page);
        self.transition_last_page = self.current_page;
        // 현재 페이지/줌 상태를 세션에 기록합니다.
        self.save_session();
    }

    /// Captures the outgoing page texture and starts a slide transition.
    ///
    /// PgUp/PgDn 키는 `transition_vertical`을 세팅해 **세로**(위/아래로 넘기는
    /// 긴 페이지 목록처럼) 애니메이션을 만들고, 그 외(내비게이션 바/휠/화살표)는
    /// 가로 슬라이드를 유지합니다.
    pub(crate) fn start_page_anim(&mut self, from: usize, to: usize) {
        if from == to {
            return;
        }
        if self.texture.is_none() {
            return;
        }
        // The current texture still holds the old page; keep it for the outgoing
        // frame and force a fresh render for the new page.
        let vertical = std::mem::take(&mut self.transition_vertical);
        self.prev_texture = self.texture.take();
        self.render_dirty = true;
        self.page_anim = Some(PageAnim {
            progress: 0.0,
            direction: if to > from { 1.0 } else { -1.0 },
            vertical,
        });
    }

    /// 새 빈 페이지를 삽입합니다. `target`에 따라 위치/크기/용지가 달라집니다.
    pub(crate) fn insert_page_action(&mut self, target: InsertTarget) {
        let Some(doc) = &mut self.document else {
            return;
        };
        let total = doc.page_count();
        if total == 0 {
            return;
        }
        let default_paper = PagePaper {
            style: self.paper_style,
            color: self.paper_color,
            spacing: self.paper_spacing,
            line_color: self.paper_line_color,
            line_width: self.paper_line_width,
        };
        let default_size = self.paper_size.size_pts();
        let (idx, size, paper) = match target {
            // 현재 페이지의 크기/용지를 그대로 써서 바로 다음에 삽입.
            InsertTarget::FromCurrent => {
                let size = doc.page_size_pts(self.current_page);
                let paper = self
                    .store
                    .paper_on_or(self.current_page, default_paper);
                (self.current_page + 1, size, paper)
            }
            InsertTarget::AtVeryFront => (0, default_size, default_paper),
            InsertTarget::AtVeryBack => (total, default_size, default_paper),
            InsertTarget::BeforeCurrent => (self.current_page, default_size, default_paper),
            InsertTarget::AfterCurrent => (self.current_page + 1, default_size, default_paper),
        };
        if let Err(e) = doc.insert_page_at(idx, size) {
            self.status = Some(e);
            return;
        }
        self.store.insert_page(idx);
        self.store.set_paper(idx, paper);
        self.current_page = idx;
        let total = doc.page_count();
        self.logger.log(AppEvent::PageAdded { page: idx, total });
        self.on_page_changed();
        self.autosave();
        self.save_pdf_if_note();
        self.sync_note_meta();
    }

    /// 현재 페이지를 시계/반시계 90° 회전합니다 (주석도 함께 회전).
    pub(crate) fn rotate_page_action(&mut self, clockwise: bool) {
        let Some(doc) = &mut self.document else {
            return;
        };
        let idx = self.current_page;
        if idx >= doc.page_count() {
            return;
        }
        let [w, h] = doc.page_size_pts(idx); // 회전 전 표시 크기
        if let Err(e) = doc.rotate_page(idx, clockwise) {
            self.status = Some(e);
            return;
        }
        self.store.rotate_strokes_on(idx, w, h, clockwise);
        self.page_size_pts = doc.page_size_pts(idx);
        let total = doc.page_count();
        self.logger
            .log(AppEvent::PageRotated { page: idx, total, clockwise });
        self.on_page_changed();
        self.autosave();
        self.save_pdf_if_note();
        self.sync_note_meta();
    }

    /// 문서의 모든 페이지를 시계/반시계 90° 회전합니다 (주석도 함께).
    pub(crate) fn rotate_all_pages_action(&mut self, clockwise: bool) {
        let Some(doc) = &mut self.document else {
            return;
        };
        let count = doc.page_count();
        if count == 0 {
            return;
        }
        // 각 페이지의 회전 전 표시 크기 스냅샷.
        let sizes: Vec<[f32; 2]> = (0..count).map(|i| doc.page_size_pts(i)).collect();
        if let Err(e) = doc.rotate_all_pages(clockwise) {
            self.status = Some(e);
            return;
        }
        for i in 0..count {
            self.store.rotate_strokes_on(i, sizes[i][0], sizes[i][1], clockwise);
        }
        self.page_size_pts = doc.page_size_pts(self.current_page);
        self.logger.log(AppEvent::PageRotated {
            page: self.current_page,
            total: count,
            clockwise,
        });
        self.on_page_changed();
        self.autosave();
        self.save_pdf_if_note();
        self.sync_note_meta();
    }

    pub(crate) fn delete_page_action(&mut self) {
        let Some(doc) = &mut self.document else {
            return;
        };
        if doc.page_count() <= 1 {
            self.status = Some("Cannot delete the last remaining page.".to_string());
            return;
        }
        let idx = self.current_page;
        if let Err(e) = doc.delete_page(idx) {
            self.status = Some(e);
            return;
        }
        let total = doc.page_count();
        self.store.remove_page(idx);
        if self.current_page >= total {
            self.current_page = total.saturating_sub(1);
        }
        self.logger.log(AppEvent::PageDeleted {
            page: idx,
            total,
        });
        self.on_page_changed();
        self.autosave();
        self.save_pdf_if_note();
        self.sync_note_meta();
    }

    // ---------- Zoom / fit ----------

    pub(crate) fn zoom_by(&mut self, factor: f32) {
        let anchor = [self.last_canvas[0] * 0.5, self.last_canvas[1] * 0.5];
        self.view.zoom_at(anchor, factor, MIN_ZOOM, MAX_ZOOM);
        self.render_dirty = true;
        self.save_session();
    }

    pub(crate) fn fit_width(&mut self) {
        self.pending_fit = Some(FitMode::Width);
    }

    pub(crate) fn fit_height(&mut self) {
        self.pending_fit = Some(FitMode::Height);
    }

    /// Applies a pending fit once the canvas size is known.
    pub(crate) fn apply_pending_fit(&mut self, canvas: [f32; 2]) {
        let Some(mode) = self.pending_fit else {
            return;
        };
        self.pending_fit = None;
        if canvas[0] <= 1.0 || canvas[1] <= 1.0 {
            return;
        }
        match mode {
            FitMode::Width => {
                self.view.zoom =
                    ViewTransform::fit_width_zoom(self.page_size_pts[0], canvas[0], CANVAS_MARGIN);
            }
            FitMode::Height => {
                self.view.zoom =
                    ViewTransform::fit_height_zoom(self.page_size_pts[1], canvas[1], CANVAS_MARGIN);
            }
        }
        self.view
            .align_page(self.page_size_pts, canvas, TOP_MARGIN, self.page_align);
        self.render_dirty = true;
        self.save_session();
    }

    /// Re-applies the current horizontal alignment without changing the zoom.
    pub(crate) fn realign(&mut self) {
        if self.document.is_none() {
            return;
        }
        self.view
            .align_page(self.page_size_pts, self.last_canvas, TOP_MARGIN, self.page_align);
        self.save_session();
    }

    // ---------- Undo / redo / clear ----------

    pub(crate) fn undo(&mut self) {
        if let Some(edit) = self.history.undo() {
            self.store.apply_edit(&edit);
            self.logger.log(AppEvent::UndoRedo {
                kind: "undo".to_string(),
            });
            self.autosave();
        }
    }

    pub(crate) fn redo(&mut self) {
        if let Some(edit) = self.history.redo() {
            self.store.apply_edit(&edit);
            self.logger.log(AppEvent::UndoRedo {
                kind: "redo".to_string(),
            });
            self.autosave();
        }
    }

    pub(crate) fn clear_page(&mut self) {
        let removed = self.store.clear_page(self.current_page);
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

    // ---------- Drawing ----------

    pub(crate) fn search_update(&mut self) {
        let Some(doc) = &self.document else {
            return;
        };
        self.search_runs = doc.page_text_runs(self.current_page).unwrap_or_default();
        self.search_matches = find_matches(&self.search_runs, &self.search_query);
        self.search_current = if self.search_matches.is_empty() {
            None
        } else {
            Some(0)
        };
        if !self.search_query.trim().is_empty() {
            self.logger.log(AppEvent::Search {
                query: self.search_query.trim().to_string(),
                results: self.search_matches.len(),
            });
        }
    }

    pub(crate) fn search_find(&mut self, forward: bool) {
        if self.search_matches.is_empty() {
            return;
        }
        let n = self.search_matches.len() as isize;
        let cur = self.search_current.unwrap_or(0) as isize;
        let next = if forward { cur + 1 } else { cur - 1 };
        let idx = ((next % n) + n) % n;
        self.search_current = Some(idx as usize);
    }

    pub(crate) fn search_clear(&mut self) {
        self.search_query.clear();
        self.search_matches = Vec::new();
        self.search_current = None;
    }

    // ---------- Outline ----------

    pub(crate) fn load_outline_if_needed(&mut self) {
        if self.outline_loaded {
            return;
        }
        if let Some(doc) = &self.document {
            self.outline = doc.outline();
            self.outline_loaded = true;
        }
    }

    // ---------- Persistence ----------

    /// Writes the current note's annotations to its note folder.
    pub(crate) fn autosave(&mut self) {
        let Some(id) = self.current_note else {
            return;
        };
        let ann_path = self.notes.annotations_path(id);
        let json = self.store.to_json();
        if let Err(e) = std::fs::write(&ann_path, json) {
            self.status = Some(format!("Autosave failed: {e}"));
            return;
        }
        let _ = self.notes.save();
    }

    pub(crate) fn sync_note_meta(&mut self) {
        if let Some(id) = self.current_note {
            if let Some(doc) = &self.document {
                let _ = self.notes.set_page_count(id, doc.page_count());
            }
        }
    }

    /// Persists page CRUD changes back into the note's PDF file.
    pub(crate) fn save_pdf_if_note(&mut self) {
        if self.current_note.is_none() {
            return;
        }
        let Some(doc) = &self.document else {
            return;
        };
        let Some(path) = self.file_path.clone() else {
            return;
        };
        if let Err(e) = doc.save(&path) {
            self.status = Some(format!("Save PDF failed: {e}"));
        }
    }

    // ---------- File dialogs (annotations / export) ----------

    pub(crate) fn save_annotations(&mut self) {
        if self.document.is_none() {
            self.status = Some("Open a PDF or note first.".to_string());
            return;
        }
        let default = self
            .file_path
            .as_deref()
            .map(annotation_path_for)
            .unwrap_or_else(|| PathBuf::from("annotations.freedf.json"));
        #[cfg(target_os = "windows")]
        {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("FreeDF annotations", &["json"])
                .set_file_name(
                    default
                        .file_name()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "annotations.freedf.json".into()),
                )
                .save_file()
            {
                self.do_save_annotations(&path);
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = default;
            self.modal = Some(ModalState::ask_text(
                "Save Annotations",
                "Enter the JSON file path to save to",
                TextAction::SaveAnnotations,
            ));
        }
    }

    pub(crate) fn do_save_annotations(&mut self, path: &Path) {
        match std::fs::write(path, self.store.to_json()) {
            Ok(()) => self.status = Some(format!("Annotations saved: {}", path.display())),
            Err(e) => self.status = Some(format!("Save failed: {e}")),
        }
    }

    pub(crate) fn load_annotations(&mut self) {
        #[cfg(target_os = "windows")]
        {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("FreeDF annotations", &["json"])
                .pick_file()
            {
                self.do_load_annotations(&path);
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            self.modal = Some(ModalState::ask_text(
                "Load Annotations",
                "Enter the JSON file path to load",
                TextAction::LoadAnnotations,
            ));
        }
    }

    pub(crate) fn do_load_annotations(&mut self, path: &Path) {
        match std::fs::read_to_string(path) {
            Ok(text) => match AnnotationStore::from_json(&text) {
                Ok(store) => {
                    self.store = store;
                    self.status = Some(format!("Annotations loaded: {}", path.display()));
                }
                Err(e) => self.status = Some(format!("Invalid annotation file: {e}")),
            },
            Err(e) => self.status = Some(format!("Load failed: {e}")),
        }
    }

    pub(crate) fn export_png(&mut self) {
        if self.document.is_none() {
            self.status = Some("Open a PDF or note first.".to_string());
            return;
        }
        #[cfg(target_os = "windows")]
        {
            let default_name = self
                .file_path
                .as_deref()
                .and_then(|p| p.file_stem())
                .map(|s| format!("{}-p{}.png", s.to_string_lossy(), self.current_page + 1))
                .unwrap_or_else(|| format!("page-{}.png", self.current_page + 1));
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("PNG image", &["png"])
                .set_file_name(&default_name)
                .save_file()
            {
                self.do_export_png(&path);
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            self.modal = Some(ModalState::ask_text(
                "Export PNG",
                "Enter the PNG file path to save to",
                TextAction::ExportPng,
            ));
        }
    }

    pub(crate) fn do_export_png(&mut self, path: &Path) {
        let Some(doc) = &self.document else {
            return;
        };
        let page_pts = doc.page_size_pts(self.current_page);
        let dpi = 150.0;
        let target_w = page_pts[0] * dpi / 72.0;
        match doc.render_page(self.current_page, target_w, 20_000.0) {
            Ok(rendered) => {
                let mut img = match image::RgbaImage::from_raw(
                    rendered.width as u32,
                    rendered.height as u32,
                    rendered.rgba,
                ) {
                    Some(img) => img,
                    None => {
                        self.status =
                            Some("Could not convert render result to image.".to_string());
                        return;
                    }
                };
                let scale = rendered.width as f32 / page_pts[0];
                // Paper tint + grid for notes (per-page settings)
                if self.current_note.is_some() {
                    let paper = self.current_page_paper();
                    crate::export::draw_paper(
                        &mut img,
                        page_pts[0],
                        page_pts[1],
                        scale,
                        paper.style,
                        paper.color,
                        paper.spacing,
                        paper.line_color,
                        paper.line_width,
                    );
                }
                let strokes: Vec<_> = self.store.strokes_on(self.current_page).to_vec();
                draw_strokes_on_image(&mut img, &strokes, scale);
                match img.save(path) {
                    Ok(()) => {
                        self.logger.log(AppEvent::ExportPng {
                            page: self.current_page,
                        });
                        self.status = Some(format!("Exported: {}", path.display()));
                    }
                    Err(e) => self.status = Some(format!("PNG save failed: {e}")),
                }
            }
            Err(e) => self.status = Some(format!("Export failed: {e}")),
        }
    }

    pub(crate) fn run_text_action(&mut self, action: TextAction, text: String) {
        match action {
            TextAction::NewNote => self.create_note_action(text.trim()),
            TextAction::RenameNote => {
                if let Some(id) = self.current_note {
                    self.rename_note_action(id, text.trim());
                }
            }
            TextAction::OpenPdf => self.open_pdf(&PathBuf::from(text.trim())),
            TextAction::SaveAnnotations => self.do_save_annotations(&PathBuf::from(text.trim())),
            TextAction::LoadAnnotations => self.do_load_annotations(&PathBuf::from(text.trim())),
            TextAction::ExportPng => self.do_export_png(&PathBuf::from(text.trim())),
        }
    }

    pub(crate) fn run_confirm_action(&mut self, action: ConfirmAction, text: String) {
        match action {
            ConfirmAction::DeleteNote => {
                if let Ok(id) = text.trim().parse::<u64>() {
                    self.delete_note_action(id);
                }
            }
        }
    }
}
