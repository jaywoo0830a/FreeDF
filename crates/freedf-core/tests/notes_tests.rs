//! `notes` 모듈 단위 테스트 — `src/notes.rs`의 `#[cfg(test)]` 모듈에서 이동했습니다.

use freedf_core::notes::*;

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
