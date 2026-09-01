//! 페이지 CRUD에 맞춘 주석 저장소의 페이지 연산.
//!
//! PDF 페이지를 삽입/삭제하면 주석(스트로크)과 용지 설정의
//! 페이지 인덱스가 함께 이동해야 합니다.

use crate::model::{PageIndex, Stroke};
use crate::store::AnnotationStore;
use std::collections::BTreeMap;

impl AnnotationStore {
    /// 페이지 삭제: 해당 페이지의 주석을 반환하고, 이후 페이지 인덱스를 -1 이동.
    /// 용지 설정도 함께 이동/제거합니다.
    pub fn remove_page(&mut self, page_index: PageIndex) -> Vec<Stroke> {
        let removed = self
            .pages
            .remove(&page_index)
            .map(|p| p.strokes)
            .unwrap_or_default();
        self.paper.remove(&page_index);
        self.shift_pages(page_index + 1, -1);
        removed
    }

    /// 빈 페이지 삽입: `at` 위치부터 이후 페이지 인덱스를 +1 이동.
    pub fn insert_page(&mut self, at: PageIndex) {
        self.shift_pages(at, 1);
        self.ensure_page(at);
    }

    /// `from` 이상의 페이지 인덱스를 `delta`만큼 이동.
    /// 충돌을 피하기 위해 증가는 내림차순, 감소는 오름차순으로 처리합니다.
    fn shift_pages(&mut self, from: PageIndex, delta: i32) {
        shift_map(&mut self.pages, from, delta, |k, page| page.page_index = k);
        shift_map(&mut self.paper, from, delta, |_k, _paper| {});
    }
}

/// 맵의 `from` 이상 키를 `delta`만큼 이동합니다.
fn shift_map<V>(
    map: &mut BTreeMap<PageIndex, V>,
    from: PageIndex,
    delta: i32,
    mut on_move: impl FnMut(PageIndex, &mut V),
) {
    let mut keys: Vec<PageIndex> = map.keys().copied().filter(|k| *k >= from).collect();
    if delta >= 0 {
        keys.sort_unstable_by(|a, b| b.cmp(a));
    } else {
        keys.sort_unstable();
    }
    for k in keys {
        if let Some(mut v) = map.remove(&k) {
            let new_key = if delta >= 0 {
                k.checked_add(delta as usize)
            } else {
                k.checked_sub(delta.unsigned_abs() as usize)
            };
            if let Some(new_key) = new_key {
                on_move(new_key, &mut v);
                map.insert(new_key, v);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{StrokePoint, ToolType};

    fn add_on_page(store: &mut AnnotationStore, page: PageIndex, label: u64) {
        store.add_stroke(
            page,
            ToolType::Pen,
            [0, 0, 0, 255],
            2.0,
            vec![StrokePoint::new(label as f32, 0.0, 0.5)],
        );
    }

    #[test]
    fn remove_page_shifts_following_annotations() {
        let mut store = AnnotationStore::new();
        add_on_page(&mut store, 0, 10);
        add_on_page(&mut store, 1, 11);
        add_on_page(&mut store, 2, 12);
        add_on_page(&mut store, 5, 15);

        let removed = store.remove_page(1);
        assert_eq!(removed.len(), 1);
        assert_eq!(store.stroke_count_on(0), 1);
        assert_eq!(store.stroke_count_on(1), 1); // 옛 2번 페이지가 1번으로
        assert_eq!(store.stroke_count_on(4), 1); // 옛 5번 페이지가 4번으로
        assert_eq!(store.stroke_count_on(5), 0);
        assert_eq!(store.stroke_count_on(2), 0);
    }

    #[test]
    fn remove_page_missing_is_noop() {
        let mut store = AnnotationStore::new();
        add_on_page(&mut store, 0, 1);
        add_on_page(&mut store, 2, 3);
        let removed = store.remove_page(7);
        assert!(removed.is_empty());
        assert_eq!(store.stroke_count_on(0), 1);
        assert_eq!(store.stroke_count_on(2), 1);
    }

    #[test]
    fn insert_page_shifts_up() {
        let mut store = AnnotationStore::new();
        add_on_page(&mut store, 0, 1);
        add_on_page(&mut store, 2, 3);

        store.insert_page(1);
        assert_eq!(store.stroke_count_on(0), 1);
        assert_eq!(store.stroke_count_on(1), 0); // 새 빈 페이지
        assert_eq!(store.stroke_count_on(2), 0);
        assert_eq!(store.stroke_count_on(3), 1); // 옛 2번 페이지
    }

    #[test]
    fn insert_and_delete_round_trip() {
        let mut store = AnnotationStore::new();
        add_on_page(&mut store, 1, 10);
        // 중간에 0 삽입 → 기존 1번은 2번으로
        store.insert_page(0);
        assert_eq!(store.stroke_count_on(2), 1);
        // 다시 0 삭제 → 복귀
        store.remove_page(0);
        assert_eq!(store.stroke_count_on(1), 1);
        assert_eq!(store.stroke_count_on(2), 0);
    }

    #[test]
    fn page_index_metadata_stays_consistent() {
        let mut store = AnnotationStore::new();
        add_on_page(&mut store, 2, 99);
        store.insert_page(1);
        let p = store.pages().find(|p| p.page_index == 3).expect("이동된 페이지");
        assert_eq!(p.strokes.len(), 1);
        assert_eq!(p.strokes[0].points[0].x, 99.0);
    }

    #[test]
    fn paper_settings_shift_with_pages() {
        use crate::paper::{PagePaper, PaperStyle, PAPER_WHITE};
        let mut store = AnnotationStore::new();
        store.set_paper(0, PagePaper { style: PaperStyle::Grid, color: PAPER_WHITE });
        store.set_paper(2, PagePaper { style: PaperStyle::Ruled, color: PAPER_WHITE });

        // 1번 페이지 삽입 → 기존 2번 용지는 3번으로 이동
        store.insert_page(1);
        assert_eq!(store.paper_on(0).map(|p| p.style), Some(PaperStyle::Grid));
        assert_eq!(store.paper_on(1), None);
        assert_eq!(store.paper_on(3).map(|p| p.style), Some(PaperStyle::Ruled));

        // 1번 삭제 → 3번 용지가 2번으로 복귀
        store.remove_page(1);
        assert_eq!(store.paper_on(2).map(|p| p.style), Some(PaperStyle::Ruled));
    }
}
