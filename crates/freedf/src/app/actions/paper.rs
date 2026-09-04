//! paper — 액션 실행 로직 (자세한 내용은 mod.rs 참조).

use super::*;

impl FreeDfApp {
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
}
