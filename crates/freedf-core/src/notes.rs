//! 노트 CRUD — 노트 라이브러리(인덱스)와 파일 배치를 관리합니다.
//!
//! 노트 하나 = `notes/<id>/document.pdf` + `notes/<id>/annotations.json` + 메타데이터.
//! 파일 이름은 ID 기반이라 제목(제목 변경)과 무관하게 안정적입니다.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

/// 노트 라이브러리 인덱스 파일명.
pub const LIBRARY_FILE: &str = "notes.json";
/// 노트 내 PDF 파일명.
pub const PDF_FILE: &str = "document.pdf";
/// 노트 내 주석 파일명.
pub const ANNOTATIONS_FILE: &str = "annotations.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoteError {
    NotFound(u64),
    InvalidTitle,
    DuplicateTitle,
    Io(String),
}

impl std::fmt::Display for NoteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NoteError::NotFound(id) => write!(f, "note not found: {id}"),
            NoteError::InvalidTitle => write!(f, "invalid title"),
            NoteError::DuplicateTitle => write!(f, "a note with this title already exists"),
            NoteError::Io(e) => write!(f, "io error: {e}"),
        }
    }
}

impl From<io::Error> for NoteError {
    fn from(e: io::Error) -> Self {
        NoteError::Io(e.to_string())
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

/// 노트 라이브러리 관리자.
pub struct NotesManager {
    library: Library,
    dir: PathBuf,
    /// 단조 증가 시계 — 같은 밀리초에 연속 호출이 겹쳐도 정렬 순서를 보장.
    last_ts: u128,
}

impl NotesManager {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            library: Library::default(),
            dir,
            last_ts: 0,
        }
    }

    /// 디렉터리에서 인덱스를 로드하거나, 없으면 새로 시작합니다.
    pub fn load_or_create(dir: PathBuf) -> Self {
        let mut manager = Self::new(dir);
        let _ = manager.load();
        manager
    }

    pub fn load(&mut self) -> Result<(), NoteError> {
        let path = self.dir.join(LIBRARY_FILE);
        if path.exists() {
            let text = std::fs::read_to_string(&path)?;
            self.library =
                serde_json::from_str(&text).map_err(|e| NoteError::Io(e.to_string()))?;
        }
        Ok(())
    }

    pub fn save(&mut self) -> Result<(), NoteError> {
        std::fs::create_dir_all(&self.dir)?;
        let json = serde_json::to_string_pretty(&self.library)
            .map_err(|e| NoteError::Io(e.to_string()))?;
        std::fs::write(self.dir.join(LIBRARY_FILE), json)?;
        Ok(())
    }

    pub fn dir(&self) -> &Path {
        &self.dir
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
        std::fs::create_dir_all(self.note_dir(id))?;
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

    /// 노트 삭제: 인덱스에서 제거하고 노트 디렉터리(파일 포함)를 삭제합니다.
    pub fn delete_note(&mut self, id: u64) -> Result<(), NoteError> {
        let meta = self
            .library
            .notes
            .remove(&id)
            .ok_or(NoteError::NotFound(id))?;
        let dir = self.note_dir(id);
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        let _ = meta;
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

    pub fn note_dir(&self, id: u64) -> PathBuf {
        self.dir.join(id.to_string())
    }

    pub fn pdf_path(&self, id: u64) -> PathBuf {
        self.note_dir(id).join(PDF_FILE)
    }

    pub fn annotations_path(&self, id: u64) -> PathBuf {
        self.note_dir(id).join(ANNOTATIONS_FILE)
    }
}

fn validate_title(title: &str) -> Result<String, NoteError> {
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
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_dir(tag: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("freedf-notes-{tag}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn create_assigns_unique_ids() {
        let dir = temp_dir("ids");
        let mut m = NotesManager::new(dir.clone());
        let a = m.create_note("Alpha").unwrap();
        let b = m.create_note("Beta").unwrap();
        assert_ne!(a.id, b.id);
        assert_eq!(m.len(), 2);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn title_validation() {
        let dir = temp_dir("title");
        let mut m = NotesManager::new(dir.clone());
        assert!(m.create_note("").is_err());
        assert!(m.create_note("   ").is_err());
        assert!(m.create_note("line\nbreak").is_err());
        assert!(m.create_note("OK Title").is_ok());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn duplicate_title_rejected_case_insensitive() {
        let dir = temp_dir("dup");
        let mut m = NotesManager::new(dir.clone());
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
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn rename_keeps_id_and_updates_title() {
        let dir = temp_dir("rename");
        let mut m = NotesManager::new(dir.clone());
        let note = m.create_note("Old").unwrap();
        m.rename_note(note.id, "New").unwrap();
        let renamed = m.get(note.id).unwrap();
        assert_eq!(renamed.id, note.id);
        assert_eq!(renamed.title, "New");
        assert!(renamed.updated_at_ms >= note.updated_at_ms);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn delete_removes_index_and_files() {
        let dir = temp_dir("del");
        let mut m = NotesManager::new(dir.clone());
        let note = m.create_note("Temp").unwrap();
        // 파일 흉내: 노트 디렉터리에 파일 생성
        let pdf = m.pdf_path(note.id);
        std::fs::write(&pdf, b"%PDF-1.4").unwrap();
        assert!(pdf.exists());

        m.delete_note(note.id).unwrap();
        assert!(m.get(note.id).is_none());
        assert!(!m.note_dir(note.id).exists());
        assert!(m.delete_note(note.id).is_err()); // 이미 없음
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn persistence_round_trip() {
        let dir = temp_dir("persist");
        {
            let mut m = NotesManager::new(dir.clone());
            m.create_note("First").unwrap();
            m.create_note("Second").unwrap();
            m.set_page_count(0, 3).unwrap();
            m.save().unwrap();
        }
        {
            let mut m = NotesManager::new(dir.clone());
            m.load().unwrap();
            assert_eq!(m.len(), 2);
            assert_eq!(m.get(0).unwrap().title, "First");
            assert_eq!(m.get(0).unwrap().page_count, 3);
            assert_eq!(m.get(1).unwrap().title, "Second");
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn list_sorted_by_recent_update() {
        let dir = temp_dir("order");
        let mut m = NotesManager::new(dir.clone());
        m.create_note("Oldest").unwrap(); // id 0
        m.create_note("Middle").unwrap(); // id 1
        m.create_note("Newest").unwrap(); // id 2
        // Middle을 최근으로 터치
        m.touch(1).unwrap();
        let titles: Vec<&str> = m.list().iter().map(|n| n.title.as_str()).collect();
        assert_eq!(titles, vec!["Middle", "Newest", "Oldest"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn load_or_create_handles_missing() {
        let dir = temp_dir("migrate");
        let m = NotesManager::load_or_create(dir.clone());
        assert!(m.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
