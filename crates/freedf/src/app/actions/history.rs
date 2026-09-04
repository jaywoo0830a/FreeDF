//! history — 액션 실행 로직 (자세한 내용은 mod.rs 참조).

use super::*;

impl FreeDfApp {
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
    pub(crate) fn persist_edit(&mut self, edit: &Edit) {
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
}
