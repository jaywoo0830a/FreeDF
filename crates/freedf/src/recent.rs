//! 최근에 열었던 항목(노트 + 일반 PDF) 목록.
//!
//! `<data>/recent.json`에 저장되며, 앱 데이터 폴더의 settings.json 옆에
//! 둡니다. 노트와 PDF를 구분해 각각 저장하고, 최근 순으로 정렬해 보여줍니다.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// 항목 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecentKind {
    /// FreeDF 노트 (notes 라이브러리에 저장).
    Note,
    /// 외부 PDF 파일 (경로).
    File,
}

/// 최근 항목 하나.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecentItem {
    pub kind: RecentKind,
    pub note_id: Option<u64>,
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
    /// 파일에서 로드. 없거나 깨졌으면 빈 목록.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// JSON으로 저장 (부모 폴더 자동 생성).
    pub fn save(&self, path: &Path) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(path, json);
        }
    }

    /// 항목을 맨 앞에 추가/갱신하고 중복을 제거합니다.
    pub fn touch(&mut self, item: RecentItem) {
        // 같은 항목(노트 id 또는 파일 경로) 제거 후 맨 앞에 삽입.
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
}

/// 두 항목이 같은 대상을 가리키는지 (노트 id 또는 파일 경로).
fn same_target(a: &RecentItem, b: &RecentItem) -> bool {
    match (a.kind, b.kind) {
        (RecentKind::Note, RecentKind::Note) => a.note_id == b.note_id,
        (RecentKind::File, RecentKind::File) => a.path == b.path,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(kind: RecentKind, id: u64, path: Option<&str>, t: u128) -> RecentItem {
        RecentItem {
            kind,
            note_id: (kind == RecentKind::Note).then_some(id),
            path: path.map(PathBuf::from),
            title: format!("item {id}"),
            opened_at_ms: t,
        }
    }

    #[test]
    fn touch_dedupes_by_target_and_keeps_newest_first() {
        let mut list = RecentList::default();
        list.touch(item(RecentKind::Note, 1, None, 100));
        list.touch(item(RecentKind::Note, 2, None, 200));
        // 같은 노트 1을 다시 → 맨 앞으로, 중복 제거
        list.touch(item(RecentKind::Note, 1, None, 300));
        assert_eq!(list.items.len(), 2);
        assert_eq!(list.items[0].note_id, Some(1));
        assert_eq!(list.items[1].note_id, Some(2));
    }

    #[test]
    fn notes_and_files_do_not_collide() {
        let mut list = RecentList::default();
        list.touch(item(RecentKind::Note, 1, None, 100));
        list.touch(item(RecentKind::File, 0, Some("a.pdf"), 200));
        assert_eq!(list.items.len(), 2);
    }

    #[test]
    fn sorted_is_newest_first() {
        let mut list = RecentList::default();
        list.touch(item(RecentKind::File, 0, Some("a.pdf"), 100));
        list.touch(item(RecentKind::File, 0, Some("b.pdf"), 300));
        list.touch(item(RecentKind::File, 0, Some("c.pdf"), 200));
        let sorted = list.sorted();
        assert_eq!(sorted[0].path.as_deref(), Some(Path::new("b.pdf")));
        assert_eq!(sorted[2].path.as_deref(), Some(Path::new("a.pdf")));
    }

    #[test]
    fn json_round_trip() {
        let mut list = RecentList::default();
        list.touch(item(RecentKind::Note, 7, None, 42));
        let json = serde_json::to_string(&list).unwrap();
        let restored: RecentList = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, list);
    }
}
