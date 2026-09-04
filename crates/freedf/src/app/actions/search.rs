//! search — 액션 실행 로직 (자세한 내용은 mod.rs 참조).

use super::*;

impl FreeDfApp {
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
}
