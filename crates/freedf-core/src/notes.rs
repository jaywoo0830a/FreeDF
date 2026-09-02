//! 노트 CRUD — 노트 라이브러리(인덱스)를 메모리 캐시로 관리합니다.
//!
//! FreeDF v2: 영속화는 PostgreSQL(`documents` 테이블)이 담당하며, 이 모듈은
//! 파일을 전혀 건드리지 않습니다. 제목 검증/중복 검사/정렬 같은 순수 로직만
//! 담고 있어 GUI 없이 단위 테스트됩니다.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoteError {
    NotFound(u64),
    InvalidTitle,
    DuplicateTitle,
}

impl std::fmt::Display for NoteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NoteError::NotFound(id) => write!(f, "note not found: {id}"),
            NoteError::InvalidTitle => write!(f, "invalid title"),
            NoteError::DuplicateTitle => write!(f, "a note with this title already exists"),
        }
    }
}

/// 노트 메타데이터.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NoteMeta {
    pub id: u64,
    pub title: String,
    pub created_at_ms: u128,
    pub updated_at_ms: u128,
    pub page_count: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
struct Library {
    notes: BTreeMap<u64, NoteMeta>,
    next_id: u64,
}

/// 노트 라이브러리 관리자 (인메모리 캐시).
pub struct NotesManager {
    library: Library,
    /// 단조 증가 시계 — 같은 밀리초에 연속 호출이 겹쳐도 정렬 순서를 보장.
    last_ts: u128,
}

impl NotesManager {
    pub fn new() -> Self {
        Self {
            library: Library::default(),
            last_ts: 0,
        }
    }

    /// DB에서 로드한 메타 목록으로 캐시를 초기화합니다.
    pub fn from_metas(metas: Vec<NoteMeta>) -> Self {
        let next_id = metas.iter().map(|m| m.id).max().map(|v| v + 1).unwrap_or(0);
        let mut library = Library {
            next_id,
            ..Default::default()
        };
        for meta in metas {
            library.notes.insert(meta.id, meta);
        }
        Self {
            library,
            last_ts: 0,
        }
    }

    pub fn get(&self, id: u64) -> Option<&NoteMeta> {
        self.library.notes.get(&id)
    }

    /// 최근 수정 순으로 정렬된 노트 목록.
    pub fn list(&self) -> Vec<&NoteMeta> {
        let mut v: Vec<&NoteMeta> = self.library.notes.values().collect();
        v.sort_by(|a, b| b.updated_at_ms.cmp(&a.updated_at_ms));
        v
    }

    pub fn len(&self) -> usize {
        self.library.notes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.library.notes.is_empty()
    }

    /// 수정 시각을 엄격히 증가시키는 시계. 같은 밀리초에 여러 호출이 겹쳐도
    /// 수정 순서가 보장됩니다.
    fn bump_ts(&mut self) -> u128 {
        let now = now_ms();
        self.last_ts = now.max(self.last_ts.saturating_add(1));
        self.last_ts
    }

    /// 새 노트 생성. (실제 빈 PDF 파일은 앱에서 `create_blank_pdf`로 만듭니다)
    pub fn create_note(&mut self, title: &str) -> Result<NoteMeta, NoteError> {
        let title = validate_title(title)?;
        if self
            .library
            .notes
            .values()
            .any(|n| n.title.eq_ignore_ascii_case(&title))
        {
            return Err(NoteError::DuplicateTitle);
        }
        let id = self.library.next_id;
        self.library.next_id += 1;
        let now = self.bump_ts();
        let meta = NoteMeta {
            id,
            title,
            created_at_ms: now,
            updated_at_ms: now,
            page_count: 0,
        };
        self.library.notes.insert(id, meta.clone());
        Ok(meta)
    }

    pub fn rename_note(&mut self, id: u64, new_title: &str) -> Result<(), NoteError> {
        let new_title = validate_title(new_title)?;
        // 중복 검사 (자기 자신 제외) — 가변 빌림 전에 수행
        if self
            .library
            .notes
            .values()
            .any(|n| n.id != id && n.title.eq_ignore_ascii_case(&new_title))
        {
            return Err(NoteError::DuplicateTitle);
        }
        // 가변 빌림 전에 시각 확보
        let new_ts = self.bump_ts();
        let meta = self
            .library
            .notes
            .get_mut(&id)
            .ok_or(NoteError::NotFound(id))?;
        meta.title = new_title;
        meta.updated_at_ms = new_ts;
        Ok(())
    }

    /// 노트 삭제: 인덱스에서만 제거합니다 (DB의 documents 행은 앱이 삭제).
    pub fn delete_note(&mut self, id: u64) -> Result<(), NoteError> {
        self.library
            .notes
            .remove(&id)
            .ok_or(NoteError::NotFound(id))?;
        Ok(())
    }

    /// 페이지 수 반영 + 수정 시각 갱신.
    pub fn set_page_count(&mut self, id: u64, page_count: usize) -> Result<(), NoteError> {
        let new_ts = self.bump_ts();
        let meta = self
            .library
            .notes
            .get_mut(&id)
            .ok_or(NoteError::NotFound(id))?;
        meta.page_count = page_count;
        meta.updated_at_ms = new_ts;
        Ok(())
    }

    pub fn touch(&mut self, id: u64) -> Result<(), NoteError> {
        let new_ts = self.bump_ts();
        let meta = self
            .library
            .notes
            .get_mut(&id)
            .ok_or(NoteError::NotFound(id))?;
        meta.updated_at_ms = new_ts;
        Ok(())
    }

    /// DB에서 생성된 메타를 캐시에 삽입합니다 (중복 제목 검사 포함).
    pub fn insert_meta(&mut self, meta: NoteMeta) -> Result<(), NoteError> {
        if self
            .library
            .notes
            .values()
            .any(|n| n.id != meta.id && n.title.eq_ignore_ascii_case(&meta.title))
        {
            return Err(NoteError::DuplicateTitle);
        }
        self.library.next_id = self.library.next_id.max(meta.id + 1);
        self.library.notes.insert(meta.id, meta);
        Ok(())
    }
}

/// 제목 검증 (빈/개행/과다 길이 금지).
pub fn validate_title(title: &str) -> Result<String, NoteError> {
    let title = title.trim();
    if title.is_empty() || title.chars().count() > 128 || title.contains('\n') || title.contains('\r') {
        return Err(NoteError::InvalidTitle);
    }
    Ok(title.to_string())
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_assigns_unique_ids() {
        let mut m = NotesManager::new();
        let a = m.create_note("Alpha").unwrap();
        let b = m.create_note("Beta").unwrap();
        assert_ne!(a.id, b.id);
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn title_validation() {
        let mut m = NotesManager::new();
        assert!(m.create_note("").is_err());
        assert!(m.create_note("   ").is_err());
        assert!(m.create_note("line\nbreak").is_err());
        assert!(m.create_note("OK Title").is_ok());
    }

    #[test]
    fn duplicate_title_rejected_case_insensitive() {
        let mut m = NotesManager::new();
        m.create_note("Lecture").unwrap(); // id 0
        m.create_note("Notes").unwrap(); // id 1
        // 대소문자만 다른 제목으로 생성 거부
        assert!(matches!(
            m.create_note("lecture"),
            Err(NoteError::DuplicateTitle)
        ));
        // 다른 노트와 대소문자만 다른 rename도 거부
        assert!(m.rename_note(0, "NOTES").is_err());
        // 자기 자신으로의 rename(대소문자 변경)은 허용
        m.rename_note(0, "LECTURE").unwrap();
        assert_eq!(m.get(0).unwrap().title, "LECTURE");
    }

    #[test]
    fn rename_keeps_id_and_updates_title() {
        let mut m = NotesManager::new();
        let note = m.create_note("Old").unwrap();
        m.rename_note(note.id, "New").unwrap();
        let renamed = m.get(note.id).unwrap();
        assert_eq!(renamed.id, note.id);
        assert_eq!(renamed.title, "New");
        assert!(renamed.updated_at_ms >= note.updated_at_ms);
    }

    #[test]
    fn delete_removes_from_index_only() {
        let mut m = NotesManager::new();
        let note = m.create_note("Temp").unwrap();
        m.delete_note(note.id).unwrap();
        assert!(m.get(note.id).is_none());
        assert!(m.delete_note(note.id).is_err()); // 이미 없음
    }

    #[test]
    fn from_metas_seeds_library() {
        let metas = vec![
            NoteMeta {
                id: 7,
                title: "First".into(),
                created_at_ms: 100,
                updated_at_ms: 100,
                page_count: 3,
            },
            NoteMeta {
                id: 9,
                title: "Second".into(),
                created_at_ms: 200,
                updated_at_ms: 200,
                page_count: 0,
            },
        ];
        let mut m = NotesManager::from_metas(metas);
        assert_eq!(m.len(), 2);
        assert_eq!(m.get(7).unwrap().title, "First");
        assert_eq!(m.get(7).unwrap().page_count, 3);
        // 새 노트는 기존 최대 id 다음부터
        let next = m.create_note("Third").unwrap();
        assert!(next.id > 9);
    }

    #[test]
    fn list_sorted_by_recent_update() {
        let mut m = NotesManager::new();
        m.create_note("Oldest").unwrap(); // id 0
        m.create_note("Middle").unwrap(); // id 1
        m.create_note("Newest").unwrap(); // id 2
        // Middle을 최근으로 터치
        m.touch(1).unwrap();
        let titles: Vec<&str> = m.list().iter().map(|n| n.title.as_str()).collect();
        assert_eq!(titles, vec!["Middle", "Newest", "Oldest"]);
    }
}
