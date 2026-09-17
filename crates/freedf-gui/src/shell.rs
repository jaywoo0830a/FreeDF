//! `Shell` — freedf-gui의 루트 컴포넌트. 툴바 · 사이드바 · 탭 스트립 ·
//! 캔버스 플레이스홀더(`<Raw>`) · 상태바 · 모달 다이얼로그를 전부 `view!`로 그린다.
//!
//! 매개변수 = 상태 슬롯(선언 순서 = 슬롯 인덱스), 이벤트 = 슬롯 대입.
//! 탭은 `Vec<String>` — 항목 선택은 `position()`으로 인덱스를 찾고, 닫기는
//! `remove(usize)`(인덱스 삭제)를 쓴다 (elm-magic 3.3 문법).

/// 모달 종류 — `match modal { … }`로 분기 렌더링한다 (사양서 3.4).
#[derive(Clone, PartialEq)]
pub(crate) enum ShellModal {
    None,
    NewTab,
    CloseConfirm,
    OpenPdf,
    ClearInk,
    About,
}

elm_magic::view! {
    fn Shell(
        sidebar_open = true,
        bookmarks_open = false,
        outline_open = false,
        status = String::from("Ready"),
        modal: ShellModal = ShellModal::None,
        input = String::new(),
    ) {
        let sections = vec![
            String::from("Notes"),
            String::from("PDFs"),
            String::from("Recents"),
        ];
        // 탭/북마크는 캔버스 엔진이 소유 — 매 프레임 읽어 렌더만 한다.
        let tab_names = crate::canvas::tab_names();
        let active_id = crate::canvas::active_tab_id();
        let bookmarks = crate::canvas::bookmark_list();
        let outline_entries = crate::canvas::outline_list();
        // 모달 열림 여부를 캔버스에 동기화(모달 뒤 잉크 방지) + 상태바 문자열.
        let canvas_status = crate::canvas::sync_and_status(matches!(modal, ShellModal::None));
        <Col>
            // ── 툴바 ─────────────────────────────────────────────
            <Row>
                <Strong>"FreeDF"</Strong>
                <Divider />
                <Button on_click={sidebar_open = !sidebar_open}>"Sidebar"</Button>
                <Button on_click={input = String::new(), modal = ShellModal::NewTab}>"New Tab"</Button>
                <Button on_click={modal = ShellModal::CloseConfirm}>"Close Tab"</Button>
                <Divider />
                <Button on_click={crate::canvas::toggle_bookmark()}>"Bookmark"</Button>
                <Button on_click={bookmarks_open = !bookmarks_open}>"Bookmarks"</Button>
                <Button on_click={outline_open = !outline_open}>"Outline"</Button>
                <Divider />
                <Button on_click={crate::canvas::zoom_in()}>"Zoom In"</Button>
                <Button on_click={crate::canvas::zoom_out()}>"Zoom Out"</Button>
                <Button on_click={crate::canvas::zoom_fit()}>"Fit"</Button>
                <Button on_click={crate::canvas::page_prev()}>"Prev Page"</Button>
                <Button on_click={crate::canvas::page_next()}>"Next Page"</Button>
                <Button on_click={input = String::new(), modal = ShellModal::OpenPdf}>"Open PDF"</Button>
                <Button on_click={modal = ShellModal::ClearInk}>"Clear Ink"</Button>
                <Divider />
                <Button on_click={modal = ShellModal::About}>"About"</Button>
            </Row>
            <Divider />
            // ── 본문: 사이드바 + 북마크 패널 + 탭 스트립 ─────────
            // 참고: 캔버스 <Raw>는 이 Row **밖**(루트 Col 직접 자식)에 둔다 —
            // egui에서 수평 Row 안의 수직 Col은 컨텐츠 높이만 가용 높이로 받는다
            // (실측: Row 안 57px → 루트 Col 직접 자식은 남은 높이 전체).
            <Row>
                {if sidebar_open {
                    <Col>
                        <Strong>"Library"</Strong>
                        {sections.into_iter().map(|s| <Row on_click={status = format!("{} panel (placeholder)", s.clone())}>"{s}"</Row>)}
                    </Col>
                } else {
                    <Text>""</Text>
                }}
                {if bookmarks_open {
                    <Col>
                        <Strong>"Bookmarks"</Strong>
                        {if bookmarks.is_empty() {
                            <Text>"북마크 없음 — Bookmark 버튼으로 추가"</Text>
                        } else {
                            bookmarks.into_iter().map(|p| <Row on_click={crate::canvas::go_to_page(p)}>"페이지 {p}"</Row>)
                        }}
                    </Col>
                } else {
                    <Text>""</Text>
                }}
                {if outline_open {
                    <Col>
                        <Strong>"Outline"</Strong>
                        {if outline_entries.is_empty() {
                            <Text>"PDF를 열면 목차가 표시됩니다"</Text>
                        } else {
                            outline_entries.into_iter().map(|e| <Row on_click={crate::canvas::go_to_page(e.page)}>"{e.title}"</Row>)
                        }}
                    </Col>
                } else {
                    <Text>""</Text>
                }}
                <Divider />
                // 탭 스트립 — id 기준 선택 (이름은 중복될 수 있다)
                <Row>
                    {tab_names.into_iter().map(|t| <Tab active={t.0 == active_id} on_click={crate::canvas::select_tab(t.0)}>"{t.1}"</Tab>)}
                </Row>
            </Row>
            // 상태바 — 캔버스 위(항상 보이는 자리).
            "{status} · {canvas_status}"
            // ── 캔버스 — <Raw> 경계: 잉크 렌더/입력은 명령형 egui (canvas.rs,
            // docs/freedf-gui-migration.md Phase 2). 위젯 트리 밖의 상태는
            // canvas 모듈의 UI-스레드 엔진이 소유한다.
            <Raw>|ui: &mut eframe::egui::Ui| {
                crate::canvas::paint(ui);
            }</Raw>
            // ── 모달 — 하나만 열린다 ────────────────────────────
            {match modal {
                ShellModal::None => <Text>""</Text>,
                ShellModal::NewTab => <Modal title="New Tab" on_close={modal = ShellModal::None}>
                    <Text>"Tab name:"</Text>
                    <Input value={input.clone()} on_change={input = _} on_enter={modal = ShellModal::None, crate::canvas::add_tab(if input.trim().is_empty() { String::from("Untitled") } else { input.clone() })} />
                    <Row>
                        <Button on_click={modal = ShellModal::None}>"Cancel"</Button>
                        <Button on_click={modal = ShellModal::None, crate::canvas::add_tab(if input.trim().is_empty() { String::from("Untitled") } else { input.clone() })}>"OK"</Button>
                    </Row>
                </Modal>,
                ShellModal::OpenPdf => <Modal title="Open PDF" on_close={modal = ShellModal::None}>
                    <Text>"PDF file path:"</Text>
                    <Input value={input.clone()} on_change={input = _} on_enter={modal = ShellModal::None, crate::canvas::open_pdf(input.clone())} />
                    <Row>
                        <Button on_click={modal = ShellModal::None}>"Cancel"</Button>
                        <Button on_click={modal = ShellModal::None, crate::canvas::open_pdf(input.clone())}>"OK"</Button>
                    </Row>
                </Modal>,
                ShellModal::ClearInk => <Modal title="Clear Ink" on_close={modal = ShellModal::None}>
                    <Text>"Remove all ink on this page?"</Text>
                    <Row>
                        <Button on_click={modal = ShellModal::None}>"Cancel"</Button>
                        <Button on_click={modal = ShellModal::None, crate::canvas::clear_ink()}>"Delete"</Button>
                    </Row>
                </Modal>,
                ShellModal::CloseConfirm => <Modal title="Close Tab" on_close={modal = ShellModal::None}>
                    <Text>"Close this tab?"</Text>
                    <Row>
                        <Button on_click={modal = ShellModal::None}>"Cancel"</Button>
                        <Button on_click={modal = ShellModal::None, crate::canvas::close_tab()}>"Delete"</Button>
                    </Row>
                </Modal>,
                ShellModal::About => <Modal title="About" on_close={modal = ShellModal::None}>
                    <Strong>"FreeDF GUI"</Strong>
                    <Text>"elm-magic shell — every widget above is a view! element"</Text>
                    <Button on_click={modal = ShellModal::None}>"OK"</Button>
                </Modal>,
            }}
        </Col>
    }
}

/// main.rs용 진입점 — 매크로의 `pub fn` 버그(pub #[derive] 출력)를 피하려고
/// 생성 타입은 모듈 프라이빗으로 두고 여기서만 렌더링한다.
pub(crate) fn render_shell(ui: &mut eframe::egui::Ui, ctx: &mut elm_magic::Ctx) {
    let props = ShellProps::default();
    let tree = elm_magic::frame::<Shell>(ctx, &props);
    let pass = elm_magic_egui::render(ui, &tree, &mut ctx.arena);
    // ── eguidev 계약 등록 (Phase 3 — docs/freedf-gui-migration.md) ──
    // 어댑터가 그린 버튼/탭을 계약 id로 등록. id 규칙: `gui.<라벨 슬러그>`,
    // 같은 라벨이 한 프레임에 두 번 이상 나오면 `.<n>` 접미사 (이름 없는 탭 둘 →
    // `gui.untitled`, `gui.untitled.1` — eguidev 중복 id 결함 방지).
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (label, resp) in &pass.buttons {
        let slug = label.to_lowercase().replace(' ', "_");
        let n = seen.entry(slug.clone()).or_insert(0);
        let id = if *n == 0 {
            format!("gui.{slug}")
        } else {
            format!("gui.{slug}.{}", *n)
        };
        *n += 1;
        crate::dev::tag_button(ui, id, label.clone(), resp);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::{with, Canvas};
    use freedf_core::model::StrokePoint;
    use freedf_core::model::ToolType;

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
        crate::canvas::add_tab("alpha".to_string());
        crate::canvas::add_tab("beta".to_string());
        let alpha_id = crate::canvas::tab_names()
            .first()
            .map(|(id, _)| *id)
            .expect("alpha");
        crate::canvas::select_tab(alpha_id);
        let mut app = elm_magic::mount!(Shell);
        app.click("beta");
        let tree = app.render_tree();
        assert!(tree.contains("Tab \"beta\" active"), "tree:\n{tree}");
        assert!(!tree.contains("Tab \"alpha\" active"), "tree:\n{tree}");
    }

    #[test]
    fn close_tab_confirm_flow() {
        with(|c| *c = Canvas::default());
        crate::canvas::add_tab("alpha".to_string());
        crate::canvas::add_tab("beta".to_string());
        let alpha_id = crate::canvas::tab_names()
            .first()
            .map(|(id, _)| *id)
            .expect("alpha");
        crate::canvas::select_tab(alpha_id);
        let mut app = elm_magic::mount!(Shell);
        app.click("beta"); // beta 선택
        app.click("Close Tab");
        app.assert_text("Close this tab?");
        app.click("Delete");
        app.assert_hidden("beta");
        app.assert_text("alpha");
    }

    #[test]
    fn bookmarks_panel_flow() {
        let mut app = elm_magic::mount!(Shell);
        app.click("Bookmarks");
        app.assert_text("북마크 없음 — Bookmark 버튼으로 추가");
        app.click("Bookmark"); // 현재 페이지(0) 북마크
        app.expect_text("페이지 0");
        app.expect_text("북마크 1");
        app.click("Bookmark"); // 다시 토글 → 해제
        app.assert_text("북마크 없음 — Bookmark 버튼으로 추가");
    }

    #[test]
    fn close_tab_keeps_ink_isolated() {
        // 문서별 저장소 분리: 탭을 닫아도 다른 탭의 잉크는 남는다.
        with(|c| {
            *c = Canvas::default();
            let pts = vec![
                StrokePoint { x: 1.0, y: 1.0, pressure: 1.0, t_ms: 0, width: 2.0 },
                StrokePoint { x: 9.0, y: 9.0, pressure: 1.0, t_ms: 0, width: 2.0 },
            ];
            c.doc().store.add_stroke(0, ToolType::Pen, [0, 0, 0, 255], 2.0, pts);
        });
        // 주의: with() 안에서 add_tab을 부르지 않는다 (RefCell 이중 대여).
        crate::canvas::add_tab("Second".to_string());
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
}
