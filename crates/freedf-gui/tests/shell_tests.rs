//! `shell` 모듈 테스트 — `src/shell.rs`의 `#[cfg(test)]` 모듈에서 이동했습니다.

use freedf_core::model::StrokePoint;
use freedf_core::model::ToolType;
use freedf_gui::canvas::{with, Canvas};
use freedf_gui::shell::*;

#[test]
fn shell_renders_chrome() {
    let app = elm_magic::mount!(Shell);
    app.assert_text("FreeDF");
    app.assert_text("Untitled");
    app.assert_text("Library");
    app.assert_text("Notes");
    app.assert_text("Ready");
}

#[test]
fn new_tab_via_modal() {
    let mut app = elm_magic::mount!(Shell);
    app.click("New Tab");
    app.assert_text("Tab name:");
    app.type_("Sketch 2");
    app.press_enter();
    app.assert_text("Sketch 2");
    // 모달이 닫혔는지 — 다이얼로그 문구가 사라졌는지로 판정
    app.assert_hidden("Tab name:");
}

#[test]
fn tab_click_selects() {
    // 탭은 캔버스 엔진이 소유 — 엔진에 문서를 만들고 셸이 읽어 렌더한다.
    with(|c| {
        *c = Canvas::default();
    });
    freedf_gui::canvas::add_tab("alpha".to_string());
    freedf_gui::canvas::add_tab("beta".to_string());
    let alpha_id = freedf_gui::canvas::tab_names()
        .first()
        .map(|(id, _)| *id)
        .expect("alpha");
    freedf_gui::canvas::select_tab(alpha_id);
    let mut app = elm_magic::mount!(Shell);
    app.click("beta");
    let tree = app.render_tree();
    assert!(tree.contains("Tab \"beta\" active"), "tree:\n{tree}");
    assert!(!tree.contains("Tab \"alpha\" active"), "tree:\n{tree}");
}

#[test]
fn close_tab_confirm_flow() {
    with(|c| *c = Canvas::default());
    freedf_gui::canvas::add_tab("alpha".to_string());
    freedf_gui::canvas::add_tab("beta".to_string());
    let alpha_id = freedf_gui::canvas::tab_names()
        .first()
        .map(|(id, _)| *id)
        .expect("alpha");
    freedf_gui::canvas::select_tab(alpha_id);
    let mut app = elm_magic::mount!(Shell);
    app.click("beta"); // beta 선택
    app.click("Close Tab");
    app.assert_text("Close this tab?");
    app.click("Delete");
    app.assert_hidden("beta");
    app.assert_text("alpha");
}

#[test]
fn ribbon_updates_canvas_engine() {
    // 리본 클릭 → 캔버스 엔진의 도구/색/굵기가 실제로 바뀐다 (다음 획에 반영).
    with(|c| *c = Canvas::default());
    let mut app = elm_magic::mount!(Shell);
    app.click("Fountain");
    app.click("Red");
    app.click("Thick");
    with(|c| {
        assert_eq!(c.tool, ToolType::Fountain);
        assert_eq!(c.color, [255, 71, 66, 255]);
        assert_eq!(c.width, 4.0);
    });
}

#[test]
fn settings_modal_opens_and_closes() {
    with(|c| *c = Canvas::default());
    let mut app = elm_magic::mount!(Shell);
    app.click("Settings");
    app.assert_text("잉크 기본값");
    app.assert_text("도구 Pen · 색상 Black · 굵기 Medium");
    app.click("Close");
    app.assert_hidden("Save as default");
}

#[test]
fn bookmarks_panel_flow() {
    let mut app = elm_magic::mount!(Shell);
    app.click("Bookmarks");
    app.assert_text("북마크 없음 — Bookmark 버튼으로 추가");
    app.click("Bookmark"); // 현재 페이지(0) 북마크
    app.expect_text("페이지 0");
    // 상태바는 3초 토스트("북마크 추가")로 대체된다 — 만료는 엔진 소유라
    // headless 테스트에서 시간이 지나지 않으므로 토스트 문구로 검증한다.
    app.expect_text("북마크 추가: 0페이지");
    app.click("Bookmark"); // 다시 토글 → 해제
    app.assert_text("북마크 없음 — Bookmark 버튼으로 추가");
}

#[test]
fn close_tab_keeps_ink_isolated() {
    // 문서별 저장소 분리: 탭을 닫아도 다른 탭의 잉크는 남는다.
    with(|c| {
        *c = Canvas::default();
        let pts = vec![
            StrokePoint {
                x: 1.0,
                y: 1.0,
                pressure: 1.0,
                t_ms: 0,
                width: 2.0,
            },
            StrokePoint {
                x: 9.0,
                y: 9.0,
                pressure: 1.0,
                t_ms: 0,
                width: 2.0,
            },
        ];
        c.doc()
            .store
            .add_stroke(0, ToolType::Pen, [0, 0, 0, 255], 2.0, pts);
    });
    // 주의: with() 안에서 add_tab을 부르지 않는다 (RefCell 이중 대여).
    freedf_gui::canvas::add_tab("Second".to_string());
    let mut app = elm_magic::mount!(Shell);
    app.assert_text("획 0"); // 새 문서는 빈 페이지
    app.click("Close Tab");
    app.click("Delete");
    with(|c| {
        assert_eq!(c.docs.len(), 1);
        assert_eq!(c.docs[0].name, "Untitled"); // 리셋 — 잉크는 사라진다
    });
}

#[test]
fn sidebar_toggle_and_status() {
    let mut app = elm_magic::mount!(Shell);
    app.click("Notes");
    app.expect_text("Notes panel (placeholder)");
    app.click("Sidebar");
    app.assert_hidden("Library");
    app.click("Sidebar");
    app.assert_text("Library");
}

#[test]
fn about_modal() {
    let mut app = elm_magic::mount!(Shell);
    app.click("About");
    app.assert_text("every widget above is a view! element");
    app.click("OK");
    app.assert_hidden("every widget above is a view! element");
}

#[test]
fn zoom_buttons_drive_canvas_status() {
    let mut app = elm_magic::mount!(Shell);
    app.assert_text("줌 100%");
    app.click("Zoom In");
    app.expect_text("줌 125%");
    app.click("Zoom Out");
    app.expect_text("줌 100%");
    app.click("Fit");
    app.expect_text("줌 100%");
}

#[test]
fn open_pdf_modal_calls_canvas_and_closes() {
    let mut app = elm_magic::mount!(Shell);
    app.click("Open PDF");
    app.assert_text("PDF file path:");
    app.type_("/nonexistent/no-such-file.pdf");
    app.press_enter();
    // 모달이 닫히고, 캔버스의 open_pdf가 상태바에 PDF 오류를 남긴다
    // (pdfium 부재/파일 오류 어느 쪽이든 "PDF"를 포함).
    app.assert_hidden("PDF file path:");
    app.expect_text("PDF");
}

#[test]
fn clear_ink_confirm_flow() {
    let mut app = elm_magic::mount!(Shell);
    app.click("Clear Ink");
    app.assert_text("Remove all ink on this page?");
    app.click("Delete");
    app.assert_hidden("Remove all ink on this page?");
}

/// 계약: 편집 행의 Undo/Redo가 캔버스 커맨드로 이어진다 (이력이 비면 토스트).
///
/// 저장/불러오기(`Save Edits`/`Load Edits`)는 실제 `app_data_dir`에 쓰므로 여기서
/// 누르지 않는다 — 그 경로는 `canvas_tests.rs`가 임시 경로로 검증한다.
#[test]
fn edit_row_reaches_canvas_commands() {
    with(|c| *c = Canvas::default());
    let mut app = elm_magic::mount!(Shell);
    app.click("Undo");
    app.expect_text("되돌릴 작업이 없습니다");
    app.click("Redo");
    app.expect_text("다시 실행할 작업이 없습니다");
}

/// 계약: 설정 창의 스무딩 프리셋 선택이 캔버스 상태를 바꾼다.
#[test]
fn settings_modal_selects_smoothing() {
    with(|c| *c = Canvas::default());
    let mut app = elm_magic::mount!(Shell);
    app.click("Settings");
    app.assert_text("스무딩 Off");
    app.click("Strong");
    app.expect_text("스무딩 Strong");
    app.click("Off");
    app.expect_text("스무딩 Off");
}
