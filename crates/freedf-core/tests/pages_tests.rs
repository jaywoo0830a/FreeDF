//! `pages` 모듈 단위 테스트 — `src/pages.rs`의 `#[cfg(test)]` 모듈에서 이동했습니다.

use freedf_core::model::PageIndex;
use freedf_core::model::{StrokePoint, ToolType};
use freedf_core::store::AnnotationStore;

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
    let p = store
        .pages()
        .find(|p| p.page_index == 3)
        .expect("이동된 페이지");
    assert_eq!(p.strokes.len(), 1);
    assert_eq!(p.strokes[0].points[0].x, 99.0);
}

#[test]
fn paper_settings_shift_with_pages() {
    use freedf_core::paper::{PagePaper, PaperStyle, PAPER_WHITE};
    let mut store = AnnotationStore::new();
    store.set_paper(
        0,
        PagePaper {
            style: PaperStyle::Grid,
            color: PAPER_WHITE,
        },
    );
    store.set_paper(
        2,
        PagePaper {
            style: PaperStyle::Ruled,
            color: PAPER_WHITE,
        },
    );

    // 1번 페이지 삽입 → 기존 2번 용지는 3번으로 이동
    store.insert_page(1);
    assert_eq!(store.paper_on(0).map(|p| p.style), Some(PaperStyle::Grid));
    assert_eq!(store.paper_on(1), None);
    assert_eq!(store.paper_on(3).map(|p| p.style), Some(PaperStyle::Ruled));

    // 1번 삭제 → 3번 용지가 2번으로 복귀
    store.remove_page(1);
    assert_eq!(store.paper_on(2).map(|p| p.style), Some(PaperStyle::Ruled));
}

#[test]
fn bookmarks_shift_with_pages() {
    let mut store = AnnotationStore::new();
    assert!(store.toggle_bookmark(0));
    assert!(store.toggle_bookmark(2));
    assert!(store.is_bookmarked(0));
    assert!(store.is_bookmarked(2));

    // 1번 삽입 → 2번 북마크가 3번으로 이동
    store.insert_page(1);
    assert!(store.is_bookmarked(0));
    assert!(store.is_bookmarked(3));
    assert!(!store.is_bookmarked(2));

    // 1번 삭제 → 3번 북마크가 2번으로 복귀
    store.remove_page(1);
    assert!(store.is_bookmarked(0));
    assert!(store.is_bookmarked(2));

    // 삭제된 페이지의 북마크는 제거되고, 이후 페이지 북마크는 아래로 이동
    store.toggle_bookmark(1);
    store.remove_page(1);
    // 옛 2번 페이지의 북마크가 새 1번으로 이동
    assert!(store.is_bookmarked(1));
    assert!(!store.is_bookmarked(2));
}

#[test]
fn bookmark_toggle_and_clear() {
    let mut store = AnnotationStore::new();
    assert!(store.toggle_bookmark(3));
    assert!(!store.toggle_bookmark(3)); // 해제
    assert!(store.toggle_bookmark(1));
    assert!(store.toggle_bookmark(5));
    assert_eq!(store.bookmarks(), &[1, 5]);
    store.clear_bookmarks();
    assert!(store.bookmarks().is_empty());
}
