//! 최근에 열었던 항목(노트 + 일반 PDF) 목록 (인메모리 캐시).
//!
//! FreeDF v2: 영속화는 PostgreSQL(`recents` 테이블)이 담당합니다.
//! 항목의 정체성은 `(kind, doc_id)`입니다.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 항목 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecentKind {
    /// FreeDF 노트 (documents.kind = 'note').
    Note,
    /// 외부 PDF 파일 (documents.kind = 'pdf').
    File,
}

/// 최근 항목 하나.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecentItem {
    pub kind: RecentKind,
    /// DB의 documents.id (노트/PDF 공통).
    #[serde(default)]
    pub doc_id: Option<i64>,
    /// 레거시: 노트 id (doc_id와 동일 값).
    #[serde(default)]
    pub note_id: Option<u64>,
    /// 외부 PDF의 원래 경로.
    #[serde(default)]
    pub path: Option<PathBuf>,
    pub title: String,
    pub opened_at_ms: u128,
}

/// 최근 목록 (가장 최근 것이 앞).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RecentList {
    pub items: Vec<RecentItem>,
}

/// 유지할 최대 개수.
pub const MAX_RECENT: usize = 20;

impl RecentList {
    /// 항목을 맨 앞에 추가/갱신하고 중복을 제거합니다.
    pub fn touch(&mut self, item: RecentItem) {
        // 같은 항목(kind, doc_id) 제거 후 맨 앞에 삽입.
        self.items.retain(|old| !same_target(old, &item));
        self.items.insert(0, item);
        if self.items.len() > MAX_RECENT {
            self.items.truncate(MAX_RECENT);
        }
    }

    /// 최근 항목을 최근 순으로 반환 (가장 최근이 먼저).
    pub fn sorted(&self) -> Vec<&RecentItem> {
        let mut v: Vec<&RecentItem> = self.items.iter().collect();
        v.sort_by(|a, b| b.opened_at_ms.cmp(&a.opened_at_ms));
        v
    }

    /// 특정 문서에 대한 최근 항목 제거.
    pub fn remove(&mut self, kind: RecentKind, doc_id: i64) {
        self.items.retain(|r| {
            !(r.kind == kind && r.doc_id == Some(doc_id))
        });
    }
}

/// 두 항목이 같은 대상을 가리키는지 (기본: kind + doc_id).
fn same_target(a: &RecentItem, b: &RecentItem) -> bool {
    match (a.doc_id, b.doc_id) {
        (Some(x), Some(y)) => a.kind == b.kind && x == y,
        _ => match (a.kind, b.kind) {
            (RecentKind::Note, RecentKind::Note) => a.note_id == b.note_id,
            (RecentKind::File, RecentKind::File) => a.path == b.path,
            _ => false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(kind: RecentKind, id: i64, t: u128) -> RecentItem {
        RecentItem {
            kind,
            doc_id: Some(id),
            note_id: (kind == RecentKind::Note).then_some(id as u64),
            path: None,
            title: format!("item {id}"),
            opened_at_ms: t,
        }
    }

    #[test]
    fn touch_dedupes_by_target_and_keeps_newest_first() {
        let mut list = RecentList::default();
        list.touch(item(RecentKind::Note, 1, 100));
        list.touch(item(RecentKind::Note, 2, 200));
        // 같은 노트 1을 다시 → 맨 앞으로, 중복 제거
        list.touch(item(RecentKind::Note, 1, 300));
        assert_eq!(list.items.len(), 2);
        assert_eq!(list.items[0].doc_id, Some(1));
        assert_eq!(list.items[1].doc_id, Some(2));
    }

    #[test]
    fn notes_and_files_do_not_collide() {
        let mut list = RecentList::default();
        list.touch(item(RecentKind::Note, 1, 100));
        list.touch(item(RecentKind::File, 1, 200));
        assert_eq!(list.items.len(), 2);
    }

    #[test]
    fn sorted_is_newest_first() {
        let mut list = RecentList::default();
        list.touch(item(RecentKind::File, 10, 100));
        list.touch(item(RecentKind::File, 20, 300));
        list.touch(item(RecentKind::File, 30, 200));
        let sorted = list.sorted();
        assert_eq!(sorted[0].doc_id, Some(20));
        assert_eq!(sorted[2].doc_id, Some(10));
    }

    #[test]
    fn remove_drops_matching_doc() {
        let mut list = RecentList::default();
        list.touch(item(RecentKind::File, 10, 100));
        list.touch(item(RecentKind::Note, 5, 200));
        list.remove(RecentKind::File, 10);
        assert_eq!(list.items.len(), 1);
        assert_eq!(list.items[0].doc_id, Some(5));
    }

    #[test]
    fn json_round_trip() {
        let mut list = RecentList::default();
        list.touch(item(RecentKind::Note, 7, 42));
        let json = serde_json::to_string(&list).unwrap();
        let restored: RecentList = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, list);
    }
}
