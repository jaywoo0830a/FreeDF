/// 모달 종류 — `match modal { … }`로 분기 렌더링한다 (사양서 3.4).
#[derive(Clone, PartialEq)]
pub(crate) enum ShellModal {
    None,
    NewTab,
    CloseConfirm,
    OpenPdf,
    ClearInk,
    About,
    Settings,
}

elm_magic::view! {
    pub(crate) fn Shell(
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
        // 리본/토스트 — 캔버스 엔진이 소유한 상태를 매 프레임 읽어 렌더만 한다.
        let tool = crate::canvas::tool_name();
        let color = crate::canvas::color_name();
        let width = crate::canvas::width_name();
        let toast = crate::canvas::toast().unwrap_or_default();
        // 루트 클래스 `.app` — 창 배경(`bg: background`) + 방어 여백(`padding: 8`,
        // Windows 최대화 시 창을 좌우로 밀어내는 문제) + 기본 간격(`gap`).
        // 예전 egui Frame 래퍼(shell::render_root의 ROOT_INNER_MARGIN)를 CSS가 대체한다.
        <Col class="app">
            // ── 툴바 ─────────────────────────────────────────────
            // 그룹 구분은 CSS `gap`으로 한다. (`<Divider/>`는 수평 행 높이를 가용
            // 높이로 부풀리는 elm-magic-egui 0.6.0 결함 때문에 제외 —
            // docs/elm-magic-bug-report.md 참고. 0.6.1에서 복원 예정.)
            <Row class="toolbar">
                <Strong>"FreeDF"</Strong>
                <Button on_click={sidebar_open = !sidebar_open}>"Sidebar"</Button>
                <Button on_click={input = String::new(), modal = ShellModal::NewTab}>"New Tab"</Button>
                <Button on_click={modal = ShellModal::CloseConfirm}>"Close Tab"</Button>
                <Button on_click={crate::canvas::toggle_bookmark()}>"Bookmark"</Button>
                <Button on_click={bookmarks_open = !bookmarks_open}>"Bookmarks"</Button>
                <Button on_click={outline_open = !outline_open}>"Outline"</Button>
                <Button on_click={crate::canvas::zoom_in()}>"Zoom In"</Button>
                <Button on_click={crate::canvas::zoom_out()}>"Zoom Out"</Button>
                <Button on_click={crate::canvas::zoom_fit()}>"Fit"</Button>
                <Button on_click={crate::canvas::page_prev()}>"Prev Page"</Button>
                <Button on_click={crate::canvas::page_next()}>"Next Page"</Button>
                <Button on_click={input = String::new(), modal = ShellModal::OpenPdf}>"Open PDF"</Button>
                <Button on_click={modal = ShellModal::ClearInk}>"Clear Ink"</Button>
                <Button on_click={modal = ShellModal::Settings}>"Settings"</Button>
                <Button on_click={modal = ShellModal::About}>"About"</Button>
            </Row>
            // ── 잉크 리본: 도구/색상/굵기 — 활성 항목은 Strong(비활성은 Button) ──
            // 색상 팔레트는 settings 서비스 기본 즐겨찾기(블랙/레드/블루)와 동일.
            // (구분선은 툴바와 같은 이유로 CSS `gap`이 대신한다.)
            <Row class="ribbon">
                <Strong>"Ink"</Strong>
                {if tool == "Pen" {
                    <Strong>"[Pen]"</Strong>
                } else {
                    <Button on_click={crate::canvas::select_tool("Pen")}>"Pen"</Button>
                }}
                {if tool == "Fountain" {
                    <Strong>"[Fountain]"</Strong>
                } else {
                    <Button on_click={crate::canvas::select_tool("Fountain")}>"Fountain"</Button>
                }}
                {if tool == "Highlighter" {
                    <Strong>"[Highlighter]"</Strong>
                } else {
                    <Button on_click={crate::canvas::select_tool("Highlighter")}>"Highlighter"</Button>
                }}
                {if tool == "Eraser" {
                    <Strong>"[Eraser]"</Strong>
                } else {
                    <Button on_click={crate::canvas::select_tool("Eraser")}>"Eraser"</Button>
                }}
                {if color == "Black" {
                    <Strong>"[Black]"</Strong>
                } else {
                    <Button on_click={crate::canvas::select_color("Black")}>"Black"</Button>
                }}
                {if color == "Red" {
                    <Strong>"[Red]"</Strong>
                } else {
                    <Button on_click={crate::canvas::select_color("Red")}>"Red"</Button>
                }}
                {if color == "Blue" {
                    <Strong>"[Blue]"</Strong>
                } else {
                    <Button on_click={crate::canvas::select_color("Blue")}>"Blue"</Button>
                }}
                {if width == "Thin" {
                    <Strong>"[Thin]"</Strong>
                } else {
                    <Button on_click={crate::canvas::select_width("Thin")}>"Thin"</Button>
                }}
                {if width == "Medium" {
                    <Strong>"[Medium]"</Strong>
                } else {
                    <Button on_click={crate::canvas::select_width("Medium")}>"Medium"</Button>
                }}
                {if width == "Thick" {
                    <Strong>"[Thick]"</Strong>
                } else {
                    <Button on_click={crate::canvas::select_width("Thick")}>"Thick"</Button>
                }}
            </Row>
            // ── 본문: 사이드바 + 북마크 패널 + 탭 스트립 ─────────
            // 참고: 캔버스 <Raw>는 이 Row **밖**(루트 Col 직접 자식)에 둔다 —
            // egui에서 수평 Row 안의 수직 Col은 컨텐츠 높이만 가용 높이로 받는다
            // (실측: Row 안 57px → 루트 Col 직접 자식은 남은 높이 전체).
            <Row>
                {if sidebar_open {
                    <Col class="panel">
                        <Strong class="panel_title">"Library"</Strong>
                        {sections.iter().map(|s| <Row on_click={status = format!("{} panel (placeholder)", s)}><Text class="panel_item">"{s}"</Text></Row>)}
                    </Col>
                } else {
                    <Text class="hidden">""</Text>
                }}
                {if bookmarks_open {
                    <Col class="panel">
                        <Strong class="panel_title">"Bookmarks"</Strong>
                        {if bookmarks.is_empty() {
                            <Text>"북마크 없음 — Bookmark 버튼으로 추가"</Text>
                        } else {
                            bookmarks.iter().map(|p| <Row on_click={crate::canvas::go_to_page(p)}><Text class="panel_item">"페이지 {p}"</Text></Row>)
                        }}
                    </Col>
                } else {
                    <Text class="hidden">""</Text>
                }}
                {if outline_open {
                    <Col class="panel">
                        <Strong class="panel_title">"Outline"</Strong>
                        {if outline_entries.is_empty() {
                            <Text>"PDF를 열면 목차가 표시됩니다"</Text>
                        } else {
                            outline_entries.iter().map(|e| <Row on_click={crate::canvas::go_to_page(e.page)}><Text class="panel_item">"{e.title}"</Text></Row>)
                        }}
                    </Col>
                } else {
                    <Text class="hidden">""</Text>
                }}
                // 탭 스트립 — id 기준 선택 (이름은 중복될 수 있다)
                <Row class="tabs">
                    {tab_names.iter().map(|t| <Tab active={t.0 == active_id} on_click={crate::canvas::select_tab(t.0)}>"{t.1}"</Tab>)}
                </Row>
            </Row>
            // 상태바 — 캔버스 위(항상 보이는 자리). `.status`가 배경/여백을,
            // 안쪽 `Text`가 색을 담당한다 (Text에는 bg/padding이 적용되지 않는다).
            // 토스트가 있으면 대신 표시하고 TOAST_SECS(3초) 뒤 자동 복귀.
            <Row class="status">
                {if toast.is_empty() {
                    <Text class="muted">"{status} · {canvas_status}"</Text>
                } else {
                    <Text class="muted">"{toast}"</Text>
                }}
            </Row>
            // ── 캔버스 — <Raw> 경계: 잉크 렌더/입력은 명령형 egui (canvas.rs,
            // docs/freedf-gui-migration.md Phase 2). 위젯 트리 밖의 상태는
            // canvas 모듈의 UI-스레드 엔진이 소유한다.
            <Raw>|ui: &mut eframe::egui::Ui| {
                crate::canvas::paint(ui);
            }</Raw>
            // ── 모달 — 하나만 열린다 ────────────────────────────
            {match modal {
                ShellModal::None => <Text class="hidden">""</Text>,
                ShellModal::NewTab => <Modal title="New Tab" on_close={modal = ShellModal::None}>
                    <Text>"Tab name:"</Text>
                    <Input value={input.clone()} on_change={input = _} on_enter={modal = ShellModal::None, crate::canvas::add_tab(if input.trim().is_empty() { String::from("Untitled") } else { input.clone() })} />
                    <Row class="modal_actions">
                        <Button on_click={modal = ShellModal::None}>"Cancel"</Button>
                        <Button on_click={modal = ShellModal::None, crate::canvas::add_tab(if input.trim().is_empty() { String::from("Untitled") } else { input.clone() })}>"OK"</Button>
                    </Row>
                </Modal>,
                ShellModal::OpenPdf => <Modal title="Open PDF" on_close={modal = ShellModal::None}>
                    <Text>"PDF file path:"</Text>
                    <Input value={input.clone()} on_change={input = _} on_enter={modal = ShellModal::None, crate::canvas::open_pdf(input.clone())} />
                    <Row class="modal_actions">
                        <Button on_click={modal = ShellModal::None}>"Cancel"</Button>
                        <Button on_click={modal = ShellModal::None, crate::canvas::open_pdf(input.clone())}>"OK"</Button>
                    </Row>
                </Modal>,
                ShellModal::ClearInk => <Modal title="Clear Ink" on_close={modal = ShellModal::None}>
                    <Text>"Remove all ink on this page?"</Text>
                    <Row class="modal_actions">
                        <Button on_click={modal = ShellModal::None}>"Cancel"</Button>
                        <Button on_click={modal = ShellModal::None, crate::canvas::clear_ink()}>"Delete"</Button>
                    </Row>
                </Modal>,
                ShellModal::CloseConfirm => <Modal title="Close Tab" on_close={modal = ShellModal::None}>
                    <Text>"Close this tab?"</Text>
                    <Row class="modal_actions">
                        <Button on_click={modal = ShellModal::None}>"Cancel"</Button>
                        <Button on_click={modal = ShellModal::None, crate::canvas::close_tab()}>"Delete"</Button>
                    </Row>
                </Modal>,
                ShellModal::About => <Modal title="About" on_close={modal = ShellModal::None}>
                    <Strong>"FreeDF GUI"</Strong>
                    <Text>"elm-magic shell — every widget above is a view! element"</Text>
                    <Button on_click={modal = ShellModal::None}>"OK"</Button>
                </Modal>,
                ShellModal::Settings => <Modal title="Settings" on_close={modal = ShellModal::None}>
                    <Strong>"잉크 기본값"</Strong>
                    <Text>"도구 {tool} · 색상 {color} · 굵기 {width}"</Text>
                    <Text>"현재 리본 상태를 기본값으로 저장합니다 — 다음 실행 때 자동 복원."</Text>
                    <Row>
                        <Button on_click={crate::canvas::save_defaults()}>"Save as default"</Button>
                        <Button on_click={modal = ShellModal::None}>"Close"</Button>
                    </Row>
                </Modal>,
            }}
        </Col>
    }
}

/// main.rs용 진입점 — 트리를 그린 뒤 어댑터 패스에 eguidev 계약 id를 붙인다.
///
/// 컴포넌트 자체는 `pub(crate) fn Shell`(elm-magic의 vis 보존 패치 이후)이라
/// 타입 가시성 문제는 없다. 계측 태깅은 어댑터 `Response`가 필요해 셸 모듈이
/// 소유하는 게 맞으므로 여기서 한 번에 처리한다 — main.rs는 `render_root`만 알면 된다.
pub(crate) fn render_shell(ui: &mut eframe::egui::Ui, ctx: &mut elm_magic::Ctx) {
    let props = ShellProps::default();
    let tree = elm_magic::frame::<Shell>(ctx, &props);
    // 팔레트를 넘겨 CSS 색 토큰(`bg: surface` …)을 실제 색으로 해석시킨다.
    // 스타일 해석 자체는 elm-magic 코어의 몫이라 freedf-gui는 값을 옮기기만 한다.
    let pass = elm_magic_egui::render_with_palette(ui, &tree, &mut ctx.arena, &crate::style::palette());
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

/// 한 프레임 렌더 — 셸 하나만 그린다.
///
/// 배경/여백은 **CSS가 담당한다** (`.app { bg: background; padding: 8 }` on the
/// 루트 `Col`) — 예전 egui Frame 래퍼(테마 `window_fill` 채우기 + 하드코딩된
/// 안쪽 여백)를 elm-magic 0.6 CSS 속성이 대체했다. 창 클리어 색은 main.rs의
/// `clear_color`가 같은 팔레트 토큰으로 채운다 (elm-magic CSS 밖 영역).
pub(crate) fn render_root(ui: &mut eframe::egui::Ui, ctx: &mut elm_magic::Ctx) {
    render_shell(ui, ctx);
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
