//! Document & note actions: open/save, page CRUD/rotate, search, bookmarks, undo/redo, shortcuts-free logic.
//!
//! Extracted from `app.rs` during the layered refactor — these are
//! `FreeDfApp` methods living in a submodule (`use super::*` re-exports the
//! app state types and shared helpers).

use super::*;

/// 문서 열기의 DB 부분 — **백그라운드 스레드 전용** (UI를 절대 막지 않음).
/// 단계마다 `LoaderMsg::Stage`를 보내 진행 바에 무엇을 가져오는지 표시합니다.
fn load_document_bundle(
    db: &dyn StorageBackend,
    doc_id: i64,
    tx: &std::sync::mpsc::Sender<LoaderMsg>,
) -> Result<LoaderBundle, String> {
    let _ = tx.send(LoaderMsg::Stage("Loading: document info…".into()));
    let row = db
        .get_document(doc_id)
        .ok_or_else(|| format!("Document {doc_id} not found in the database."))?;
    let is_note = row.is_note();
    let _ = tx.send(LoaderMsg::Stage("Loading: PDF bytes…".into()));
    let pdf_bytes = db.load_pdf(doc_id).ok_or_else(|| {
        format!("{} has no PDF content in the database.", row.title)
    })?;
    let _ = tx.send(LoaderMsg::Stage("Loading: annotations…".into()));
    let store = db.load_store(doc_id);
    let _ = tx.send(LoaderMsg::Stage("Loading: history & session…".into()));
    let edits = db.load_edits(doc_id);
    let session = db.load_session(doc_id);
    Ok(LoaderBundle {
        doc_id,
        is_note,
        row,
        pdf_bytes,
        store,
        edits,
        session,
    })
}

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

    /// 북마크를 DB에 반영합니다 (현재 페이지의 pages 행 갱신).
    pub(crate) fn persist_bookmarks(&mut self) {
        if let Some(doc_id) = self.doc_id {
            self.db.upsert_page(
                doc_id,
                self.current_page as i32,
                &self.current_page_paper(),
                self.store.is_bookmarked(self.current_page),
            );
        }
        self.save_session();
    }

    // ---------- Notes ----------

    /// Shows an error both in the status bar and as a popup alert.
    pub(crate) fn show_error(&mut self, msg: String) {
        self.status = Some(msg.clone());
        self.modal = Some(ModalState::alert("Error", &msg));
    }

    /// Returns a reference to the PDFium instance cached at startup.
    /// pdfium-render only allows one initialization per process, so everything
    /// must reuse this single instance (never call `load_pdfium` again).
    pub(crate) fn pdfium(&self) -> Result<&Pdfium, String> {
        self.pdfium.as_ref().map(|b| b.as_ref()).map_err(|e| e.clone())
    }

    pub(crate) fn create_note_action(&mut self, title: &str) {
        // 1) 제목 검증 + 중복 검사 (캐시 기준).
        let title = match freedf_core::notes::validate_title(title) {
            Ok(t) => t,
            Err(e) => {
                self.status = Some(format!("Could not create note: {e}"));
                return;
            }
        };
        if self
            .notes
            .list()
            .iter()
            .any(|n| n.title.eq_ignore_ascii_case(&title))
        {
            self.status = Some("A note with this title already exists.".to_string());
            return;
        }
        // 2) 빈 PDF를 메모리에 생성 → 바이트.
        let bytes = match self
            .pdfium()
            .and_then(|p| DocumentView::create_blank_view(p, self.new_page_size_pts(), &title))
            .and_then(|view| view.save_to_bytes())
        {
            Ok(b) => b,
            Err(e) => {
                self.show_error(e);
                return;
            }
        };
        // 3) documents 행 삽입 + 캐시 반영.
        match self.db.insert_document("note", &title, None, &bytes) {
            Ok(doc_id) => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis())
                    .unwrap_or(0);
                let meta = freedf_core::notes::NoteMeta {
                    id: doc_id as u64,
                    title: title.clone(),
                    created_at_ms: now,
                    updated_at_ms: now,
                    page_count: 1,
                };
                let _ = self.notes.insert_meta(meta);
                self.logger.log(AppEvent::NoteCreated {
                    note_id: doc_id as u64,
                    title,
                });
                self.open_document(doc_id);
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
                let _ = self.db.update_title(id as i64, title);
                self.logger.log(AppEvent::NoteRenamed {
                    note_id: id,
                    from: old,
                    to: title.to_string(),
                });
                if self.current_note == Some(id as i64) {
                    self.file_name = title.to_string();
                }
            }
            Err(e) => self.status = Some(format!("Rename failed: {e}")),
        }
    }

    pub(crate) fn delete_note_action(&mut self, id: u64) {
        let doc_id = id as i64;
        let title = self
            .notes
            .get(id)
            .map(|m| m.title.clone())
            .unwrap_or_default();
        // 열려 있는 탭이면 닫고, 아니면 문서 상태 정리.
        if let Some(idx) = self.find_tab(&TabKind::Note(doc_id)) {
            self.close_tab(idx);
        } else if self.current_note == Some(doc_id) {
            self.close_document();
        }
        // 캐시 제거.
        let _ = self.notes.delete_note(id);
        self.recents.remove(RecentKind::Note, doc_id);
        // DB 제거 (strokes/pages/sessions/recents는 ON DELETE CASCADE).
        if let Err(e) = self.db.delete_document(doc_id) {
            self.status = Some(format!("Delete failed: {e}"));
            return;
        }
        self.logger.log(AppEvent::NoteDeleted { note_id: id, title });
    }

    /// DB의 문서(외부 PDF)를 삭제합니다. 원본 파일은 건드리지 않습니다
    /// (파일을 지우려면 탐색기에서 직접 삭제하세요).
    pub(crate) fn delete_pdf_action(&mut self, doc_id: i64) {
        let name = self
            .db
            .get_document(doc_id)
            .map(|d| d.title)
            .unwrap_or_else(|| doc_id.to_string());
        // 열린 탭 닫기 (활성 탭이면 인접 탭으로 전환됨).
        if let Some(idx) = self.find_tab(&TabKind::Pdf(doc_id)) {
            self.close_tab(idx);
        }
        self.recents.remove(RecentKind::File, doc_id);
        if let Err(e) = self.db.delete_document(doc_id) {
            self.status = Some(format!("Delete failed: {e}"));
            return;
        }
        self.logger.log(AppEvent::PdfDeleted {
            path: name.clone(),
        });
        self.status = Some(format!("Deleted {name} from the library"));
    }

    /// 현재 선택된 용지(스타일+색)를 **모든 페이지**에 적용합니다.
    /// (줄/점 세부설정은 스타일별 프리셋을 렌더 시 참조하므로 별도 적용 불필요)
    pub(crate) fn apply_paper_to_all_pages(&mut self) {
        let Some(doc) = &self.document else {
            return;
        };
        let count = doc.page_count();
        if count == 0 {
            return;
        }
        let paper = PagePaper {
            style: self.paper_style,
            color: self.paper_color,
        };
        for i in 0..count {
            self.store.set_paper(i, paper);
        }
        self.render_dirty = true;
        if let Some(doc_id) = self.doc_id {
            // write-behind — 페이지 수만큼 왕복하지 않고 큐에 쌓아 백그라운드 반영.
            for i in 0..count {
                self.db
                    .upsert_page(doc_id, i as i32, &paper, self.store.is_bookmarked(i));
            }
        }
        self.save_session();
        self.status = Some(format!("Applied paper to all {count} pages"));
    }

    /// 현재 선택된 용지(스타일+색)를 **페이지 범위**에 적용합니다.
    /// `from`/`to`는 0-based 페이지 인덱스 (포함, 범위를 벗어나면 클램프).
    pub(crate) fn apply_paper_to_range(&mut self, from: usize, to: usize) {
        let Some(doc) = &self.document else {
            return;
        };
        let count = doc.page_count();
        if count == 0 {
            return;
        }
        let (a, b) = (from.min(count - 1), to.min(count - 1));
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        let paper = PagePaper {
            style: self.paper_style,
            color: self.paper_color,
        };
        for i in lo..=hi {
            self.store.set_paper(i, paper);
        }
        self.render_dirty = true;
        if let Some(doc_id) = self.doc_id {
            // 범위는 upsert_page(write-behind)로 행 단위 갱신.
            for i in lo..=hi {
                self.db.upsert_page(
                    doc_id,
                    i as i32,
                    &self.store.paper_on(i).unwrap_or(paper),
                    self.store.is_bookmarked(i),
                );
            }
        }
        self.save_session();
        self.status = Some(format!(
            "Applied paper to pages {}–{}",
            lo + 1,
            hi + 1
        ));
    }

    pub(crate) fn close_document(&mut self) {
        self.document = None;
        self.current_note = None;
        self.doc_id = None;
        self.texture = None;
        self.prefetch = None;
        self.prefetch_pending = false;
        self.set_store(AnnotationStore::new());
        self.history = History::new(256);
        self.active_stroke = None;
        self.search_matches = Vec::new();
        self.search_current = None;
        self.outline = Vec::new();
        self.outline_loaded = false;
        self.file_name = String::new();
        self.scroll_vel = Vec2::ZERO;
        self.page_anim = None;
        self.prev_texture = None;
        self.transition_last_page = 0;
        self.status = None;
    }

    /// DB의 문서 id로 문서를 엽니다 (노트/외부 PDF 공통).
    /// DB 조회는 백그라운드 스레드에서 진행되고, 완료되면
    /// `poll_loader → finish_document_open`이 로컬에서 문서를 엽니다.
    pub(crate) fn open_document(&mut self, doc_id: i64) {
        if self.loading.is_some() || self.loader_rx.is_some() {
            return;
        }
        // 이미 열려 있는 탭이면 DB 조회 없이 전환만 합니다.
        for kind in [TabKind::Note(doc_id), TabKind::Pdf(doc_id)] {
            if let Some(idx) = self.find_tab(&kind) {
                self.switch_tab(idx);
                return;
            }
        }
        let db = self.db.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        self.loader_rx = Some(rx);
        self.begin_loading(format!("Loading document {doc_id}…"));
        std::thread::spawn(move || {
            let result = load_document_bundle(db.as_ref(), doc_id, &tx);
            let _ = tx.send(LoaderMsg::Done(result));
        });
    }

    /// 로더가 가져온 번들로 문서를 **로컬에서** 엽니다 (네트워크 없음).
    pub(crate) fn finish_document_open(&mut self, bundle: LoaderBundle) {
        let LoaderBundle {
            doc_id,
            is_note,
            row,
            pdf_bytes,
            store,
            edits,
            session,
        } = bundle;
        // 로딩 중 다른 경로로 열렸으면 전환만 합니다.
        for kind in [TabKind::Note(doc_id), TabKind::Pdf(doc_id)] {
            if let Some(idx) = self.find_tab(&kind) {
                self.switch_tab(idx);
                return;
            }
        }
        self.save_session();
        if self.document.is_some() {
            self.capture_into(self.active);
        }
        let opened = self
            .pdfium()
            .and_then(|p| DocumentView::open_bytes(p, &pdf_bytes, &row.title));
        match opened {
            Ok(doc) => {
                self.doc_id = Some(doc_id);
                self.current_note = if is_note { Some(doc_id) } else { None };
                self.current_page = 0;
                self.page_size_pts = doc.page_size_pts(0);
                self.file_name = row.title.clone();
                self.document = Some(doc);
                self.set_store(store);
                // 스트로크 id 풀을 백그라운드로 미리 채워 첫 획부터
                // UI 왕복 없이 그립니다 (도착 전이면 로컬 id 폴백).
                self.request_stroke_pool_refill();
                // 편집 저널 재생 → 재시작 전의 undo/redo 가능.
                self.restore_history_from_edits(&edits);
                self.active_stroke = None;
                self.pan_last = None;
                self.middle_pan_last = None;
                self.prefetch = None;
                self.prefetch_pending = true;
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
                if let Some(value) = session {
                    let session = crate::settings::SessionState::from_json_value(value);
                    self.apply_session(&session, page_count);
                    self.pending_fit = None;
                }
                if is_note {
                    self.logger.log(AppEvent::NoteOpened {
                        note_id: doc_id as u64,
                        title: row.title.clone(),
                        page_count,
                    });
                }
                self.load_outline_if_needed();
                let kind = if is_note {
                    TabKind::Note(doc_id)
                } else {
                    TabKind::Pdf(doc_id)
                };
                self.add_current_as_tab(kind);
                self.note_recent(
                    if is_note { RecentKind::Note } else { RecentKind::File },
                    row.title.clone(),
                    doc_id,
                    row.origin_path.map(PathBuf::from),
                );
            }
            Err(e) => self.show_error(e),
        }
    }

    /// 노트 열기 (라이브러리 패널용 래퍼).
    pub(crate) fn open_note(&mut self, id: u64) {
        self.open_document(id as i64);
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
                "Enter the PDF file path (e.g. /home/me/doc.pdf)",
                TextAction::OpenPdf,
            ));
        }
    }

    /// 외부 PDF를 **DB에 import**하고 엽니다. 같은 경로가 이미 있으면 재사용합니다.
    pub(crate) fn open_pdf(&mut self, path: &Path) {
        let key = path.to_string_lossy().into_owned();
        if let Some(doc_id) = self.db.find_document_by_path(&key) {
            self.open_document(doc_id);
            return;
        }
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => {
                self.show_error(format!("Could not read PDF file: {e}"));
                return;
            }
        };
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| key.clone());
        match self.db.insert_document("pdf", &name, Some(&key), &bytes) {
            Ok(doc_id) => self.open_document(doc_id),
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
        // 다음 페이지 프리페치 예약 (현재 페이지의 줌/크기 기준).
        self.prefetch_pending = true;
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
        let vertical = std::mem::take(&mut self.transition_vertical);
        // 프리페치된 새 페이지 텍스처가 있으면 즉시 사용 → 렌더 대기 없는 전환.
        let hit = self
            .prefetch
            .as_ref()
            .is_some_and(|(p, z, _)| *p == to && (*z - self.view.zoom).abs() < 1e-3);
        if hit {
            let (_, _, tex) = self.prefetch.take().expect("hit");
            self.prev_texture = self.texture.take();
            self.texture = Some(tex);
            self.render_dirty = false;
        } else {
            if self.texture.is_none() {
                return;
            }
            // The current texture still holds the old page; keep it for the
            // outgoing frame and force a fresh render for the new page.
            self.prev_texture = self.texture.take();
            self.render_dirty = true;
        }
        self.page_anim = Some(PageAnim {
            progress: 0.0,
            direction: if to > from { 1.0 } else { -1.0 },
            vertical,
        });
    }

    /// 새 빈 페이지를 삽입합니다. `target`에 따라 위치/크기/용지가 달라집니다.
    pub(crate) fn insert_page_action(&mut self, target: InsertTarget) {
        // (self.document를 가변 빌리기 전에 self 값을 미리 계산)
        let default_paper = PagePaper {
            style: self.paper_style,
            color: self.paper_color,
        };
        let default_size = self.new_page_size_pts();
        let Some(doc) = &mut self.document else {
            return;
        };
        let total = doc.page_count();
        if total == 0 {
            return;
        }
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
        self.flush_current_document();
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
        // 회전은 좌표계를 바꾸는 구조 연산 — 회전 전 좌표가 담긴 undo 히스토리와
        // DB 편집 저널은 더 이상 유효하지 않으므로 초기화합니다.
        self.history.clear();
        if let Some(doc_id) = self.doc_id {
            self.db.clear_edits(doc_id);
        }
        self.page_size_pts = doc.page_size_pts(idx);
        let total = doc.page_count();
        self.logger
            .log(AppEvent::PageRotated { page: idx, total, clockwise });
        self.status = Some(format!(
            "Rotated page {} 90° {}",
            idx + 1,
            if clockwise { "clockwise" } else { "counter-clockwise" }
        ));
        self.on_page_changed();
        self.flush_current_document();
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
        // 좌표계가 바뀌었으므로 undo 히스토리/저널 초기화.
        self.history.clear();
        if let Some(doc_id) = self.doc_id {
            self.db.clear_edits(doc_id);
        }
        self.page_size_pts = doc.page_size_pts(self.current_page);
        self.logger.log(AppEvent::PageRotated {
            page: self.current_page,
            total: count,
            clockwise,
        });
        self.status = Some(format!(
            "Rotated all {count} pages 90° {}",
            if clockwise { "clockwise" } else { "counter-clockwise" }
        ));
        self.on_page_changed();
        self.flush_current_document();
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
        self.flush_current_document();
    }

    // ---------- Zoom / fit ----------

    pub(crate) fn zoom_by(&mut self, factor: f32) {
        if self.zoom_lock {
            return;
        }
        let anchor = [self.last_canvas[0] * 0.5, self.last_canvas[1] * 0.5];
        self.view.zoom_at(anchor, factor, MIN_ZOOM, MAX_ZOOM);
        self.render_dirty = true;
        self.save_session();
    }

    pub(crate) fn fit_width(&mut self) {
        if self.zoom_lock {
            return;
        }
        self.pending_fit = Some(FitMode::Width);
    }

    pub(crate) fn fit_height(&mut self) {
        if self.zoom_lock {
            return;
        }
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
            self.persist_edit(&edit);
            self.logger.log(AppEvent::UndoRedo {
                kind: "undo".to_string(),
            });
        }
    }

    pub(crate) fn redo(&mut self) {
        if let Some(edit) = self.history.redo() {
            self.store.apply_edit(&edit);
            self.persist_edit(&edit);
            self.logger.log(AppEvent::UndoRedo {
                kind: "redo".to_string(),
            });
        }
    }

    /// undo/redo로 바뀐 스트로크만 DB에 반영합니다 (행 단위 증분).
    fn persist_edit(&mut self, edit: &Edit) {
        let Some(doc_id) = self.doc_id else {
            return;
        };
        match edit {
            Edit::AddStrokes { page, strokes } => {
                self.db.insert_strokes(doc_id, *page as i32, strokes);
            }
            Edit::RemoveStrokes { strokes, .. } => {
                let ids: Vec<i64> = strokes.iter().map(|s| s.id as i64).collect();
                self.db.delete_strokes(doc_id, &ids);
            }
        }
    }

    pub(crate) fn clear_page(&mut self) {
        let removed = self.store.clear_page(self.current_page);
        if !removed.is_empty() {
            let ids: Vec<i64> = removed.iter().map(|s| s.id as i64).collect();
            if let Some(doc_id) = self.doc_id {
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

    // ---------- Persistence (PostgreSQL) ----------

    /// 현재 문서를 전부 DB로 플러시합니다: 스트로크 전체 재동기화 + pages 테이블
    /// (용지/북마크) + 페이지 수 + PDF 본문 바이트.
    /// (페이지 CRUD/회전 등 구조 연산과 저장/종료 시 호출)
    ///
    /// PDF 직렬화(pdfium)만 UI 스레드에서 하고, **DB 반영은 백그라운드**로
    /// 단계별 진행 메시지(SaveMsg::Stage)를 보냅니다.
    pub(crate) fn flush_current_document(&mut self) {
        let Some(doc_id) = self.doc_id else {
            return;
        };
        if self.save_rx.is_some() {
            return; // 이미 저장 진행 중 (완료 시 최종 상태 반영).
        }
        let page_count = self.document.as_ref().map(|d| d.page_count()).unwrap_or(0);
        let default_paper = PagePaper {
            style: self.paper_style,
            color: self.paper_color,
        };
        let entries: Vec<(i32, PagePaper, bool)> = (0..page_count)
            .map(|i| {
                let paper = self.store.paper_on(i).unwrap_or(default_paper);
                (i as i32, paper, self.store.is_bookmarked(i))
            })
            .collect();
        if let Some(note_id) = self.current_note {
            let _ = self.notes.set_page_count(note_id as u64, page_count);
        }
        // PDF 직렬화(pdfium)는 UI 스레드에서만 가능 — 로컬 작업.
        let pdf_bytes = match &self.document {
            Some(doc) => match doc.save_to_bytes() {
                Ok(bytes) => Some(bytes),
                Err(e) => {
                    self.status = Some(format!("Save PDF failed: {e}"));
                    None
                }
            },
            None => None,
        };
        let store = self.store.clone();
        let db = self.db.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        self.save_rx = Some(rx);
        self.begin_loading("Preparing save…");
        std::thread::spawn(move || {
            let res = (|| -> Result<(), String> {
                // 문서 전체(획+페이지+문서 정보+PDF)를 **한 번의 왕복**으로
                // 서버 함수(document_sync, migration 0006)가 원자 반영 —
                // 함수가 없는 구형 스키마면 내부에서 단계별 경로로 폴백합니다.
                let _ = tx.send(SaveMsg::Stage(format!(
                    "Saving document ({} strokes / {} pages / {} KB PDF)…",
                    store.total_stroke_count(),
                    entries.len(),
                    pdf_bytes.as_ref().map(|b| b.len() / 1024).unwrap_or(0)
                )));
                db.sync_document(
                    doc_id,
                    page_count as i32,
                    &store,
                    &entries,
                    pdf_bytes.as_deref(),
                )
            })();
            let _ = tx.send(SaveMsg::Done(res));
        });
    }

    // ---------- Save / Load buttons ----------

    pub(crate) fn save_annotations(&mut self) {
        if self.document.is_none() {
            self.status = Some("Open a PDF or note first.".to_string());
            return;
        }
        // 비동기 저장 — 완료 메시지는 poll_save가 "Saved to database."로 표시.
        self.flush_current_document();
    }

    pub(crate) fn load_annotations(&mut self) {
        let Some(doc_id) = self.doc_id else {
            self.status = Some("Open a PDF or note first.".to_string());
            return;
        };
        // DB에 저장된 최신 상태로 재로드 (메모리 변경 폐기).
        // 로컬 캐시가 있으면 먼저 무효화해 강제로 원격에서 다시 읽습니다.
        self.db.invalidate_document(doc_id);
        self.set_store(self.db.load_store(doc_id));
        self.restore_history_from_db();
        self.render_dirty = true;
        self.status = Some("Annotations reloaded from database.".to_string());
    }

    // ---------- Media (audio recordings) ----------

    /// 목록 조회 작업을 백그라운드 스레드로 실행 (로딩 오버레이 표시).
    pub(crate) fn media_list_job(&mut self) {
        if self.media_rx.is_some() || self.loading.is_some() {
            return;
        }
        let Some(doc_id) = self.doc_id else {
            return;
        };
        let config = self.media_config.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        self.media_rx = Some(rx);
        // 실패해도 재시도 루프가 생기지 않게 문서 id를 먼저 기록.
        self.media_loaded_for = Some(doc_id);
        self.begin_loading("Loading recordings…");
        std::thread::spawn(move || {
            let res = match MediaClient::new_enabled(&config) {
                None => Err("Media server is not enabled — open Server settings.".to_string()),
                Some(client) => client
                    .list(Some(doc_id), 100, 0)
                    .map(MediaOutcome::Listed),
            };
            let _ = tx.send(res);
        });
    }

    /// 현재 문서의 미디어 목록을 서버에서 다시 불러옵니다 (비동기).
    pub(crate) fn media_refresh(&mut self) {
        self.media_list_job();
    }

    /// 업로드 파일 선택 — Windows는 네이티브 대화상자, 그 외엔 경로 입력.
    pub(crate) fn upload_media_dialog(&mut self) {
        #[cfg(target_os = "windows")]
        {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter(
                    "Audio files",
                    &["m4a", "mp3", "wav", "webm", "ogg", "aac", "flac", "m4b", "opus"],
                )
                .pick_file()
            {
                self.upload_media_path(&path);
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            self.modal = Some(ModalState::ask_text(
                "Upload recording",
                "Enter the audio file path (e.g. /home/me/rec.m4a)",
                TextAction::UploadMedia,
            ));
        }
    }

    /// 파일을 현재 문서의 녹음으로 업로드합니다 (비동기 — UI를 막지 않음).
    pub(crate) fn upload_media_path(&mut self, path: &Path) {
        if self.media_rx.is_some() || self.loading.is_some() {
            return;
        }
        let Some(doc_id) = self.doc_id else {
            self.media_status = Some("Open a document first.".into());
            return;
        };
        let Some(client) = MediaClient::new_enabled(&self.media_config) else {
            self.media_status = Some("Media server is not enabled — open Server settings.".into());
            return;
        };
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "upload.bin".into());
        let path = path.to_path_buf();
        let (tx, rx) = std::sync::mpsc::channel();
        self.media_rx = Some(rx);
        self.begin_loading(format!("Uploading {name}…"));
        std::thread::spawn(move || {
            let res = (|| -> Result<MediaOutcome, String> {
                let bytes =
                    std::fs::read(&path).map_err(|e| format!("Could not read file: {e}"))?;
                if bytes.len() > 200 * 1024 * 1024 {
                    return Err("File is larger than 200 MB (server limit).".into());
                }
                let mime = crate::server::mime_for_ext(&name);
                client
                    .upload(Some(doc_id), "audio", &name, mime, &bytes)
                    .map(MediaOutcome::Uploaded)
            })();
            let _ = tx.send(res);
        });
    }

    /// 녹음 하나를 서버에서 삭제합니다 (비동기 — 파일 + 메타데이터).
    pub(crate) fn delete_media_item(&mut self, id: i64) {
        if self.media_rx.is_some() || self.loading.is_some() {
            return;
        }
        let Some(client) = MediaClient::new_enabled(&self.media_config) else {
            self.media_status = Some("Media server is not enabled.".into());
            return;
        };
        let (tx, rx) = std::sync::mpsc::channel();
        self.media_rx = Some(rx);
        self.begin_loading("Deleting recording…");
        std::thread::spawn(move || {
            let res = client.delete(id).map(|()| MediaOutcome::Deleted);
            let _ = tx.send(res);
        });
    }

    /// 녹음 URL을 OS 기본 미디어 플레이어로 엽니다 (nginx가 스트리밍).
    pub(crate) fn play_media_item(&mut self, url: String) {
        if let Err(e) = open_in_system_player(&url) {
            self.media_status = Some(format!("Could not open player: {e}"));
        }
    }

    pub(crate) fn run_text_action(&mut self, action: TextAction, text: String) {
        match action {
            TextAction::NewNote => self.create_note_action(text.trim()),
            TextAction::RenameNote => {
                if let Some(id) = self.current_note {
                    self.rename_note_action(id as u64, text.trim());
                }
            }
            TextAction::OpenPdf => self.open_pdf(&PathBuf::from(text.trim())),
            TextAction::UploadMedia => self.upload_media_path(&PathBuf::from(text.trim())),
        }
    }

    pub(crate) fn run_confirm_action(&mut self, action: ConfirmAction, text: String) {
        match action {
            ConfirmAction::DeleteNote => {
                if let Ok(id) = text.trim().parse::<i64>() {
                    self.delete_note_action(id as u64);
                }
            }
            ConfirmAction::DeleteLibrary { notes, pdfs } => {
                let n_notes = notes.len();
                let n_pdfs = pdfs.len();
                for id in &notes {
                    self.delete_note_action(*id as u64);
                }
                for p in pdfs {
                    self.delete_pdf_action(p);
                }
                self.sel_notes.clear();
                self.sel_pdfs.clear();
                self.status = Some(format!(
                    "Deleted {n_notes} note(s) and {n_pdfs} PDF(s)"
                ));
            }
        }
    }
}

/// URL을 OS 기본 플레이어로 엽니다 (미디어 스트리밍은 nginx가 직접 서빙).
#[cfg(target_os = "windows")]
fn open_in_system_player(url: &str) -> std::io::Result<()> {
    std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn()
        .map(|_| ())
}

#[cfg(target_os = "macos")]
fn open_in_system_player(url: &str) -> std::io::Result<()> {
    std::process::Command::new("open").arg(url).spawn().map(|_| ())
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn open_in_system_player(url: &str) -> std::io::Result<()> {
    std::process::Command::new("xdg-open")
        .arg(url)
        .spawn()
        .map(|_| ())
}
