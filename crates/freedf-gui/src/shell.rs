//! `Shell` — freedf-gui의 루트 컴포넌트. 툴바 · 사이드바 · 탭 스트립 ·
//! 캔버스 플레이스홀더(`<Raw>`) · 상태바 · 모달 다이얼로그를 전부 `view!`로 그린다.
//!
//! 매개변수 = 상태 슬롯(선언 순서 = 슬롯 인덱스), 이벤트 = 슬롯 대입.
//! 탭은 `Vec<String>` — 항목 선택은 `position()`으로 인덱스를 찾고, 닫기는
//! `remove(usize)`(인덱스 삭제)를 쓴다 (elm-magic 3.3 문법).

/// 모달 종류 — `match modal { … }`로 분기 렌더링한다 (사양서 3.4).
#[derive(Clone)]
pub(crate) enum ShellModal {
    None,
    NewTab,
    CloseConfirm,
    About,
}

elm_magic::view! {
    fn Shell(
        tabs: Vec<String> = vec![String::from("Untitled")],
        active: usize = 0,
        sidebar_open = true,
        status = String::from("Ready"),
        modal: ShellModal = ShellModal::None,
        input = String::new(),
    ) {
        let sections = vec![
            String::from("Notes"),
            String::from("PDFs"),
            String::from("Recents"),
        ];
        let current = tabs.get(active).cloned().unwrap_or_default();
        <Col>
            // ── 툴바 ─────────────────────────────────────────────
            <Row>
                <Strong>"FreeDF"</Strong>
                <Divider />
                <Button on_click={sidebar_open = !sidebar_open}>"Sidebar"</Button>
                <Button on_click={input = String::new(), modal = ShellModal::NewTab}>"New Tab"</Button>
                <Button on_click={modal = ShellModal::CloseConfirm}>"Close Tab"</Button>
                <Divider />
                <Button on_click={modal = ShellModal::About}>"About"</Button>
            </Row>
            <Divider />
            // ── 본문: 사이드바 + 콘텐츠 ─────────────────────────
            <Row>
                {if sidebar_open {
                    <Col>
                        <Strong>"Library"</Strong>
                        {sections.into_iter().map(|s| <Row on_click={status = format!("{} panel (placeholder)", s.clone())}>"{s}"</Row>)}
                    </Col>
                } else {
                    <Text>""</Text>
                }}
                <Divider />
                <Col>
                    // 탭 스트립 — 항목 선택은 값으로 인덱스를 찾아 대입
                    <Row>
                        {tabs.map(|t| <Tab active={tabs.get(active) == Some(&t)} on_click={active = tabs.iter().position(|x| *x == t.clone()).unwrap_or(0)}>"{t}"</Tab>)}
                    </Row>
                    // 캔버스 자리 — 어댑터 어휘 밖이라 <Raw> 플레이스홀더 (v1에서 실제 페인팅 연결)
                    <Raw>|ui: &mut eframe::egui::Ui| {
                        let rect = ui.allocate_exact_size(ui.available_size(), eframe::egui::Sense::hover()).0;
                        ui.painter().rect_stroke(rect, 8.0, (2.0, eframe::egui::Color32::GRAY), eframe::egui::StrokeKind::Inside);
                        ui.painter().text(
                            rect.center(),
                            eframe::egui::Align2::CENTER_CENTER,
                            "canvas — <Raw> placeholder (PDF / ink lands here)",
                            eframe::egui::FontId::proportional(16.0),
                            eframe::egui::Color32::GRAY,
                        );
                    }</Raw>
                    <Divider />
                    "{status}"
                </Col>
            </Row>
            // ── 모달 — 하나만 열린다 ────────────────────────────
            {match modal {
                ShellModal::None => <Text>""</Text>,
                ShellModal::NewTab => <Modal title="New Tab" on_close={modal = ShellModal::None}>
                    <Text>"Tab name:"</Text>
                    <Input value={input.clone()} on_change={input = _} on_enter={tabs.push(input.clone()), active = tabs.len() - 1, modal = ShellModal::None} />
                    <Row>
                        <Button on_click={modal = ShellModal::None}>"Cancel"</Button>
                        <Button on_click={tabs.push(input.clone()), active = tabs.len() - 1, modal = ShellModal::None}>"OK"</Button>
                    </Row>
                </Modal>,
                ShellModal::CloseConfirm => <Modal title="Close Tab" on_close={modal = ShellModal::None}>
                    <Text>"Close this tab?"</Text>
                    <Row>
                        <Button on_click={modal = ShellModal::None}>"Cancel"</Button>
                        <Button on_click={active = 0, modal = ShellModal::None, tabs.remove(current.clone())}>"Delete"</Button>
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
    elm_magic_egui::render(ui, &tree, &mut ctx.arena);
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let mut app = elm_magic::mount_with::<Shell>(ShellProps {
            tabs: Some(vec!["alpha".to_string(), "beta".to_string()]),
            ..Default::default()
        });
        app.click("beta");
        let tree = app.render_tree();
        assert!(tree.contains("Tab \"beta\" active"), "tree:\n{tree}");
        assert!(!tree.contains("Tab \"alpha\" active"), "tree:\n{tree}");
    }

    #[test]
    fn close_tab_confirm_flow() {
        let mut app = elm_magic::mount_with::<Shell>(ShellProps {
            tabs: Some(vec!["alpha".to_string(), "beta".to_string()]),
            ..Default::default()
        });
        app.click("beta"); // beta 선택
        app.click("Close Tab");
        app.assert_text("Close this tab?");
        app.click("Delete");
        app.assert_hidden("beta");
        app.assert_text("alpha");
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
}
