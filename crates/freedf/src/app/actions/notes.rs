//! notes — 액션 실행 로직 (자세한 내용은 mod.rs 참조).

use super::*;

impl FreeDfApp {
    pub(crate) fn create_note_action(&mut self, title: &str, page_count: usize) {
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
        let pages = page_count.clamp(1, 2000);
        // 2) 빈 PDF(페이지 N장)를 메모리에 생성 → 바이트.
        let bytes = match self
            .pdfium()
            .and_then(|p| DocumentView::create_blank_view(p, self.new_page_size_pts(), &title, pages))
            .and_then(|view| view.save_to_bytes())
        {
            Ok(b) => b,
            Err(e) => {
                self.show_error(e);
                return;
            }
        };
        // 3) documents 행 삽입 + 캐시 반영.
        match self
            .db
            .insert_document("note", &title, None, pages as i32, &bytes)
        {
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
                    page_count: pages,
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
        // 로컬 캐시/목록 제거 (즉시).
        let _ = self.notes.delete_note(id);
        self.recents.remove(RecentKind::Note, doc_id);
        // DB 제거(strokes/pages/sessions/recents는 ON DELETE CASCADE)는
        // 백그라운드 — 원격 왕복이 UI 스레드를 막지 않습니다.
        self.spawn_library_delete(vec![LibraryJob::DeleteNote { doc_id, title }]);
    }

    /// DB의 문서(외부 PDF)를 삭제합니다. 원본 파일은 건드리지 않습니다
    /// (파일을 지우려면 탐색기에서 직접 삭제하세요).
    pub(crate) fn delete_pdf_action(&mut self, doc_id: i64) {
        // 이름은 로컬 캐시(활성 문서/탭/최근 목록)에서만 — 원격 왕복을 피합니다.
        let name = if self.doc_id == Some(doc_id) {
            self.file_name.clone()
        } else {
            self.tabs
                .iter()
                .find(|t| matches!(&t.kind, TabKind::Pdf(id) if *id == doc_id))
                .map(|t| t.label.clone())
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    self.recents
                        .items
                        .iter()
                        .find(|r| r.doc_id == Some(doc_id))
                        .map(|r| r.title.clone())
                })
                .unwrap_or_else(|| doc_id.to_string())
        };
        // 열린 탭 닫기 (활성 탭이면 인접 탭으로 전환됨).
        if let Some(idx) = self.find_tab(&TabKind::Pdf(doc_id)) {
            self.close_tab(idx);
        }
        self.recents.remove(RecentKind::File, doc_id);
        self.spawn_library_delete(vec![LibraryJob::DeletePdf { doc_id, name }]);
    }
}
