//! 페이지 CRUD에 맞춘 주석 저장소의 페이지 연산.
//!
//! PDF 페이지를 삽입/삭제하면 주석(스트로크)과 용지 설정의
//! 페이지 인덱스가 함께 이동해야 합니다.

use crate::model::{PageIndex, Stroke};
use crate::store::AnnotationStore;
use std::collections::BTreeMap;

impl AnnotationStore {
    /// 페이지 삭제: 해당 페이지의 주석을 반환하고, 이후 페이지 인덱스를 -1 이동.
    /// 용지 설정과 북마크도 함께 이동/제거합니다.
    pub fn remove_page(&mut self, page_index: PageIndex) -> Vec<Stroke> {
        let removed = self
            .pages
            .remove(&page_index)
            .map(|p| p.strokes)
            .unwrap_or_default();
        self.paper.remove(&page_index);
        // 북마크: 삭제된 페이지는 제거, 이후 페이지는 -1 이동.
        self.bookmarks.retain(|b| *b != page_index);
        for b in self.bookmarks.iter_mut() {
            if *b > page_index {
                *b -= 1;
            }
        }
        self.shift_pages(page_index + 1, -1);
        removed
    }

    /// 빈 페이지 삽입: `at` 위치부터 이후 페이지 인덱스를 +1 이동.
    pub fn insert_page(&mut self, at: PageIndex) {
        for b in self.bookmarks.iter_mut() {
            if *b >= at {
                *b += 1;
            }
        }
        self.shift_pages(at, 1);
        self.ensure_page(at);
    }

    /// `from` 이상의 페이지 인덱스를 `delta`만큼 이동.
    /// 충돌을 피하기 위해 증가는 내림차순, 감소는 오름차순으로 처리합니다.
    pub fn shift_pages(&mut self, from: PageIndex, delta: i32) {
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
