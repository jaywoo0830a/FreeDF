//! save — 액션 실행 로직 (자세한 내용은 mod.rs 참조).

use super::*;

impl FreeDfApp {
    /// 현재 문서를 DB로 플러시합니다 — **델타 프로토콜**:
    ///   1) write-behind 대기열 플러시 (획은 이미 증분 반영 중 — 재전송 없음)
    ///   2) 서버 구조 델타 (페이지 인덱스 이동/삭제/회전 — 선택)
    ///   3) 메타 동기화 (페이지/문서 정보/PDF — migration 0008)
    /// 전체 스트로크 재동기화는 하지 않습니다 (repair용 `document_sync`는 별도).
    ///
    /// PDF 직렬화(pdfium)만 UI 스레드에서 하고, **DB 반영은 백그라운드**로
    /// 단계별 진행 메시지(SaveMsg::Stage)를 보냅니다.
    pub(crate) fn flush_current_document(&mut self) {
        self.flush_current_document_with(None);
    }

    /// 구조 연산 델타와 함께 플러시합니다 (`None` = 일반 저장).
    pub(crate) fn flush_current_document_with(&mut self, op: Option<StructureOp>) {
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
        let db = self.db.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        self.save_rx = Some(rx);
        self.begin_loading("Preparing save…");
        std::thread::spawn(move || {
            let res = (|| -> Result<(), String> {
                // 1) 획 증분 반영 (대기열에 남은 것까지 지금 전송).
                let _ = tx.send(SaveMsg::Stage("Flushing pending strokes…".into()));
                db.flush_pending();
                // 2) 서버 구조 델타 — 재전송 없이 인덱스 이동/삭제/회전.
                match op {
                    Some(StructureOp::Shift { from, delta }) => {
                        let _ = tx.send(SaveMsg::Stage("Shifting page indices…".into()));
                        db.shift_strokes(doc_id, from, delta);
                    }
                    Some(StructureOp::DeletePage { page }) => {
                        let _ = tx.send(SaveMsg::Stage("Deleting page data…".into()));
                        db.delete_page_data(doc_id, page);
                    }
                    Some(StructureOp::RotatePage {
                        page,
                        clockwise,
                        w,
                        h,
                    }) => {
                        let _ = tx.send(SaveMsg::Stage("Rotating strokes on server…".into()));
                        db.rotate_page_data(doc_id, page, clockwise, w, h);
                    }
                    Some(StructureOp::RotateAll { clockwise, sizes }) => {
                        let _ = tx.send(SaveMsg::Stage("Rotating all strokes on server…".into()));
                        db.rotate_all_data(doc_id, clockwise, &sizes);
                    }
                    None => {}
                }
                // 3) 메타 동기화 (페이지/문서 정보/PDF — 획 불변).
                let _ = tx.send(SaveMsg::Stage(format!(
                    "Saving document info ({} pages / {} KB PDF)…",
                    entries.len(),
                    pdf_bytes.as_ref().map(|b| b.len() / 1024).unwrap_or(0)
                )));
                db.sync_meta(doc_id, page_count as i32, &entries, pdf_bytes.as_deref())
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
        let bundle = self.db.load_bundle(doc_id);
        self.set_store(bundle.store);
        self.restore_history_from_edits(&bundle.edits);
        self.render_dirty = true;
        self.status = Some("Annotations reloaded from database.".to_string());
    }

    // ---------- Media (audio recordings) ----------
}
