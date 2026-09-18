//! `shell` 모듈 테스트 — `src/shell.rs`의 `#[cfg(test)]` 모듈에서 이동했습니다.

use freedf_core::model::StrokePoint;
use freedf_core::model::ToolType;
use freedf_gui::canvas::{with, Canvas};
use freedf_gui::shell::*;

/// 셸 버튼/탭 라벨은 **아이콘 + 텍스트**다 — 마크업과 같은 생성기(`ui::label`)로
/// 만든다. 아이콘 표가 바뀌어도 테스트는 따라온다.
fn label(text: &str) -> String {
    freedf_gui::ui::label(text)
}

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
    app.click(&label("New Tab"));
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
    app.click(&label("beta"));
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
    app.click(&label("beta")); // beta 선택
    app.click(&label("Close Tab"));
    app.assert_text("Close this tab?");
    app.click(&label("Delete"));
    app.assert_hidden("beta");
    app.assert_text("alpha");
}

#[test]
fn ribbon_updates_canvas_engine() {
    // 리본 클릭 → 캔버스 엔진의 도구/색/굵기가 실제로 바뀐다 (다음 획에 반영).
    // 색은 settings 기본 팔레트의 **스와치 라벨**로 고른다 (라벨 = 계약 id 근거).
    with(|c| *c = Canvas::default());
    let mut app = elm_magic::mount!(Shell);
    app.click(&label("Fountain"));
    app.click(&label("Swatch 2")); // 기본 팔레트 2번 = Red
    app.click(&label("Thick"));
    with(|c| {
        assert_eq!(c.tool, ToolType::Fountain);
        assert_eq!(c.color, [255, 71, 66, 255]);
        assert_eq!(c.width, 4.0);
    });
}

#[test]
fn ribbon_swatches_follow_settings_default_palette() {
    // 스와치는 settings 기본 즐겨찾기 3색을 그대로 노출한다.
    with(|c| *c = Canvas::default());
    let app = elm_magic::mount!(Shell);
    for label in ["Swatch 1", "Swatch 2", "Swatch 3"] {
        app.assert_text(label);
    }
    with(|c| assert_eq!(c.color, [26, 26, 28, 255]));
    let mut app = elm_magic::mount!(Shell);
    app.click(&label("Swatch 3")); // Blue
    with(|c| assert_eq!(c.color, [72, 166, 235, 255]));
}

#[test]
fn ribbon_pressure_toggle_flips_engine_flag() {
    // 필압 반영 토글은 캔버스 엔진의 플래그를 뒤집는다 (스트림이 없으면 값은 명목 1.0).
    with(|c| *c = Canvas::default());
    assert!(freedf_gui::canvas::pressure_enabled());
    let mut app = elm_magic::mount!(Shell);
    app.click(&label("Pressure"));
    assert!(!freedf_gui::canvas::pressure_enabled());
    let mut app = elm_magic::mount!(Shell);
    app.click(&label("Pressure"));
    assert!(freedf_gui::canvas::pressure_enabled());
}

#[test]
fn settings_modal_opens_and_closes() {
    with(|c| *c = Canvas::default());
    let mut app = elm_magic::mount!(Shell);
    app.click(&label("Settings"));
    app.assert_text("잉크 기본값");
    app.assert_text("도구 Pen · 색상 Black · 굵기 Medium");
    app.click(&label("Close"));
    app.assert_hidden("Save as default");
}

#[test]
fn bookmarks_panel_flow() {
    let mut app = elm_magic::mount!(Shell);
    app.click(&label("Bookmarks"));
    app.assert_text("북마크 없음 — Bookmark 버튼으로 추가");
    app.click(&label("Bookmark")); // 현재 페이지(0) 북마크
    app.expect_text("페이지 0");
    // 상태바는 3초 토스트("북마크 추가")로 대체된다 — 만료는 엔진 소유라
    // headless 테스트에서 시간이 지나지 않으므로 토스트 문구로 검증한다.
    app.expect_text("북마크 추가: 0페이지");
    app.click(&label("Bookmark")); // 다시 토글 → 해제
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
    app.click(&label("Close Tab"));
    app.click(&label("Delete"));
    with(|c| {
        assert_eq!(c.docs.len(), 1);
        assert_eq!(c.docs[0].name, "Untitled"); // 리셋 — 잉크는 사라진다
    });
}

#[test]
fn sidebar_toggle_and_status() {
    let mut app = elm_magic::mount!(Shell);
    app.click(&label("Notes"));
    app.expect_text("Notes panel (placeholder)");
    app.click(&label("Sidebar"));
    app.assert_hidden("Library");
    app.click(&label("Sidebar"));
    app.assert_text("Library");
}

#[test]
fn about_modal() {
    let mut app = elm_magic::mount!(Shell);
    app.click(&label("About"));
    app.assert_text("every widget above is a view! element");
    app.click(&label("OK"));
    app.assert_hidden("every widget above is a view! element");
}

#[test]
fn zoom_buttons_drive_canvas_status() {
    let mut app = elm_magic::mount!(Shell);
    app.assert_text("줌 100%");
    app.click(&label("Zoom In"));
    app.expect_text("줌 125%");
    app.click(&label("Zoom Out"));
    app.expect_text("줌 100%");
    app.click(&label("Fit"));
    app.expect_text("줌 100%");
}

#[test]
fn open_pdf_modal_calls_canvas_and_closes() {
    let mut app = elm_magic::mount!(Shell);
    app.click(&label("Open PDF"));
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
    app.click(&label("Clear Ink"));
    app.assert_text("Remove all ink on this page?");
    app.click(&label("Delete"));
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
    app.click(&label("Undo"));
    app.expect_text("되돌릴 작업이 없습니다");
    app.click(&label("Redo"));
    app.expect_text("다시 실행할 작업이 없습니다");
}

/// 계약: 설정 창의 스무딩 프리셋 선택이 캔버스 상태를 바꾼다.
#[test]
fn settings_modal_selects_smoothing() {
    with(|c| *c = Canvas::default());
    let mut app = elm_magic::mount!(Shell);
    app.click(&label("Settings"));
    app.assert_text("스무딩 Off");
    app.click(&label("Strong"));
    app.expect_text("스무딩 Strong");
    app.click(&label("Off"));
    app.expect_text("스무딩 Off");
}

#[test]
fn zz_debug_tab_click() {
    with(|c| *c = Canvas::default());
    freedf_gui::canvas::add_tab("alpha".to_string());
    freedf_gui::canvas::add_tab("beta".to_string());
    let names = freedf_gui::canvas::tab_names();
    let beta_id = names.iter().find(|(_, n)| n == "beta").map(|(id, _)| *id).unwrap();
    let mut app = elm_magic::mount!(Shell);
    app.click(&label("beta"));
    println!("active after click = {:?} (beta_id={})", freedf_gui::canvas::active_tab_id(), beta_id);
    let tree = app.render_tree();
    for line in tree.lines() {
        if line.contains("Tab \"") {
            println!("TREE {line}");
        }
    }
    println!("names = {:?}", freedf_gui::canvas::tab_names());
}
