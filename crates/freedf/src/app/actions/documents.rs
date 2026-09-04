//! documents — 액션 실행 로직 (자세한 내용은 mod.rs 참조).

use super::*;

impl FreeDfApp {
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
        self.color_wheel_open = false;
        self.wheel_swallow_click = false;
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
                // 이전 문서의 줌/팬/텍스처가 새 문서에 남지 않도록 초기화 —
                // fit-width가 다음 프레임에 적용됩니다 (새 문서 = 새 상태).
                self.view = ViewTransform::default();
                self.texture = None;
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
    /// 파일 읽기(디스크)와 DB 업로드(원격 왕복, MB 단위일 수 있음)는
    /// 백그라운드 스레드에서 진행 — UI 스레드를 막지 않습니다.
    /// 완료되면 `poll_pdf_import → open_document`가 문서를 엽니다.
    pub(crate) fn open_pdf(&mut self, path: &Path) {
        if self.loading.is_some() || self.pdf_import_rx.is_some() {
            return;
        }
        let path = path.to_path_buf();
        let db = self.db.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        self.pdf_import_rx = Some(rx);
        let label = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        self.begin_loading(format!("Importing {label}…"));
        std::thread::spawn(move || {
            let result = (|| -> Result<i64, String> {
                let key = path.to_string_lossy().into_owned();
                if let Some(doc_id) = db.find_document_by_path(&key) {
                    return Ok(doc_id);
                }
                let bytes = std::fs::read(&path)
                    .map_err(|e| format!("Could not read PDF file: {e}"))?;
                let name = path
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_else(|| key.clone());
                db.insert_document("pdf", &name, Some(&key), 1, &bytes)
            })();
            let _ = tx.send(PdfImportMsg::Done(result));
        });
    }

    // ---------- Pages ----------
}
