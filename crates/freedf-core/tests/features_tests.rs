//! 신규 기능(노트 CRUD, 페이지 CRUD, 아웃라인, 검색, 펜, 로깅) 통합 테스트.
//! (FreeDF v2: 영속화는 PostgreSQL 담당 — 이 테스트는 순수 인메모리 로직만 검증)

use freedf_core::logging::{AppEvent, LogEntry, Logger};
use freedf_core::model::{StrokePoint, ToolType};
use freedf_core::notes::{NoteMeta, NotesManager};
use freedf_core::outline::{find_by_title, flatten, OutlineNode};
use freedf_core::pen::{BallPenProfile, ColorFamily, Palette};
use freedf_core::search::{find_matches, TextRun};
use freedf_core::store::AnnotationStore;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "freedf-features-{tag}-{}-{n}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// 노트 CRUD 전체 시나리오 + 페이지 수 유지 (인메모리).
#[test]
fn notes_crud_end_to_end() {
    let mut notes = NotesManager::new();

    let a = notes.create_note("Math").unwrap();
    let b = notes.create_note("Physics").unwrap();
    assert_eq!(notes.len(), 2);

    notes.rename_note(a.id, "Advanced Math").unwrap();
    assert_eq!(notes.get(a.id).unwrap().title, "Advanced Math");

    notes.set_page_count(b.id, 12).unwrap();
    assert_eq!(notes.get(b.id).unwrap().page_count, 12);

    // 메타 목록으로 캐시 재구성 (DB 로드 경로와 동일한 경로).
    let metas: Vec<NoteMeta> = notes.list().iter().map(|m| (*m).clone()).collect();
    let reloaded = NotesManager::from_metas(metas);
    assert_eq!(reloaded.len(), 2);
    assert_eq!(reloaded.get(a.id).unwrap().title, "Advanced Math");
}

/// 페이지 CRUD와 주석 인덱스 정리가 함께 동작.
#[test]
fn page_crud_keeps_annotations_aligned() {
    let mut store = AnnotationStore::new();
    for page in 0..4 {
        store.add_stroke(
            page,
            ToolType::Pen,
            [0, 0, 0, 255],
            2.0,
            vec![StrokePoint::new(page as f32, 0.0, 0.5)],
        );
    }
    // 1페이지 삭제
    store.remove_page(1);
    assert_eq!(store.stroke_count_on(0), 1);
    assert_eq!(store.stroke_count_on(1), 1); // 옛 2
    assert_eq!(store.stroke_count_on(2), 1); // 옛 3
    assert_eq!(store.stroke_count_on(3), 0);

    // 1페이지에 빈 페이지 삽입
    store.insert_page(1);
    assert_eq!(store.stroke_count_on(1), 0);
    assert_eq!(store.stroke_count_on(2), 1);
    assert_eq!(store.stroke_count_on(3), 1);
}

/// 아웃라인 트리 탐색/검색.
#[test]
fn outline_tree_navigation() {
    let tree = vec![
        OutlineNode::new(
            "Chapter 1",
            Some(0),
            vec![
                OutlineNode::new("Section 1.1", Some(0), vec![]),
                OutlineNode::new("Section 1.2", Some(2), vec![]),
            ],
        ),
        OutlineNode::new("Chapter 2", Some(9), vec![]),
    ];
    let flat = flatten(&tree);
    assert_eq!(flat.len(), 4);
    assert_eq!(flat[2].node.title, "Section 1.2");
    assert_eq!(flat[2].node.page_index, Some(2));

    assert_eq!(find_by_title(&tree, "chapter 2").unwrap().page_index, Some(9));
}

/// 페이지 내 단어 검색 (여러 런 + 문자 좌표).
#[test]
fn page_word_search_with_highlights() {
    let runs = vec![
        TextRun::new(
            "FreeDF is a PDF viewer.",
            [0.0, 0.0, 200.0, 20.0],
            vec![],
        ),
        TextRun::new(
            "You can search PDF words.",
            [0.0, 20.0, 200.0, 40.0],
            vec![],
        ),
    ];
    let matches = find_matches(&runs, "pdf");
    assert_eq!(matches.len(), 2);
    for m in &matches {
        assert_eq!(m.matched.to_lowercase(), "pdf");
        // 하이라이트 사각형은 런 경계 안에 있어야 함
        assert!(m.rect[0] >= 0.0 && m.rect[2] <= 200.0 + 1e-3);
    }
}

/// 펜: 색상 팔레트 + 일반 펜(볼펜) 물리 모델.
#[test]
fn pen_palette_and_pressure() {
    for family in [ColorFamily::Red, ColorFamily::Blue, ColorFamily::Black] {
        assert!(Palette::swatches(family).len() >= 3);
    }
    let pen = BallPenProfile::default();
    let light = pen.width_at(1.0, 0.1, 0.0, 200.0);
    let hard = pen.width_at(1.0, 0.9, 0.0, 200.0);
    assert!(light < hard);
    assert!(light > 0.0);
}

/// 로그 파일 구조 검증: JSON Lines, 순번, 이벤트 종류.
#[test]
fn log_file_structure_for_analysis() {
    let dir = temp_dir("log");
    let path = dir.join("app.log");
    let mut logger = Logger::to_file(&path).unwrap();
    logger.log(AppEvent::AppStart { version: "0.2.0".into() });
    logger.log(AppEvent::PageChanged { page: 1, total: 20 });
    logger.log(AppEvent::Search { query: "rust".into(), results: 2 });
    logger.flush();

    let text = std::fs::read_to_string(&path).unwrap();
    let entries: Vec<LogEntry> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].seq, 1);
    assert_eq!(entries[2].seq, 3);
    assert!(matches!(
        entries[1].event,
        AppEvent::PageChanged { page: 1, total: 20 }
    ));
    let _ = std::fs::remove_dir_all(dir);
}
