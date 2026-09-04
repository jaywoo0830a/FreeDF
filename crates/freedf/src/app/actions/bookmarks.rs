//! bookmarks — 액션 실행 로직 (자세한 내용은 mod.rs 참조).

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
}
