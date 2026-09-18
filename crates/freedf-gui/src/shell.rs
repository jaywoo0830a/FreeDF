/// 모달 종류 — `match modal { … }`로 분기 렌더링한다 (사양서 3.4).
#[derive(Clone, PartialEq)]
pub enum ShellModal {
    None,
    NewTab,
    CloseConfirm,
    OpenPdf,
    ClearInk,
    About,
    Settings,
}

/// 리본 스와치 한 칸 — 라벨(계약 id 근거)/활성 여부/클릭 인덱스.
///
/// `Copy`인 이유: `view!` 클로저가 **값을 복사**하게 하려는 것 (참조를 캡처하면
/// 매크로가 만드는 `'static` 핸들러 계약을 못 맞춘다 — 실측 E0716).
#[derive(Clone, Copy)]
pub struct SwatchItem {
    pub index: usize,
    /// `&'static str` — `view!` 클로저가 복사만 하도록 (`palette::label_str`).
    pub label: &'static str,
    /// 지금 선택된 색인가 (활성 표시 — `ribbon__button--on`).
    pub on: bool,
}

/// 셸이 매 프레임 읽는 캔버스 상태 스냅샷 — 팔레트/필압/펜 진단.
///
/// `view!` 본문에서 제네릭 타입 표기나 `if` 식을 쓰면 매크로 파서가 태그로
/// 오인하므로(실측), 계산은 전부 여기서 끝내고 본문에는 `let st = …` 하나만 둔다.
pub struct ShellState {
    pub swatch_items: Vec<SwatchItem>,
    /// 설정 창용 팔레트 요약 (이름/HEX — 저장 형식과 같은 문자열).
    pub swatch_list: String,
    pub pressure: bool,
    pub pressure_text: &'static str,
    pub pen_source: String,
    pub pen_tilt: &'static str,
}

/// 현재 캔버스 상태 스냅샷 — 색 목록/이름 해석은 `canvas`+`palette`가 소유한다.
pub fn shell_state() -> ShellState {
    let swatches = crate::canvas::palette_colors();
    let active = crate::canvas::color_swatch_index();
    let pressure = crate::canvas::pressure_enabled();
    ShellState {
        swatch_items: swatches
            .iter()
            .enumerate()
            .map(|(i, _)| SwatchItem {
                index: i,
                label: crate::palette::label_str(i),
                on: active == Some(i),
            })
            .collect(),
        swatch_list: swatches
            .iter()
            .map(|c| crate::palette::name(*c))
            .collect::<Vec<String>>()
            .join(", "),
        pressure,
        pressure_text: if pressure { "켜짐" } else { "꺼짐" },
        pen_source: crate::canvas::pen_source(),
        pen_tilt: if crate::canvas::pen_tilt_supported() {
            "지원"
        } else {
            "미지원"
        },
    }
}

elm_magic::view! {
    pub fn Shell(
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
        //
        // 주의: `sync_and_status`의 인자는 **모달이 열렸는지**다. 여기서
        // `matches!(modal, ShellModal::None)`(= 모달 없음)을 그대로 넘기면
        // 캔버스 입력이 **항상 꺼진다** — 실제로 그 반전 때문에 펜이 전혀
        // 그려지지 않았다 (`ink_stroke_lands_through_shell_raw` 회귀 테스트).
        let modal_open = !matches!(modal, ShellModal::None);
        let canvas_status = crate::canvas::sync_and_status(modal_open);
        // 리본/토스트 — 캔버스 엔진이 소유한 상태를 매 프레임 읽어 렌더만 한다.
        let tool = crate::canvas::tool_name();
        let color = crate::canvas::color_name();
        let width = crate::canvas::width_name();
        // 즐겨찾기 팔레트/필압/펜 진단 스냅샷 — 계산은 [`shell_state`]가 소유한다.
        // (view! 본문에는 **단순 문장**만 두는 게 안전하다: 제네릭 표기(`Vec<(A,B)>`)
        //  나 `if` 식이 섞이면 매크로 파서가 태그로 오인해 파싱이 깨진다 — 실측.)
        let st = shell_state();
        // 스무딩 프리셋 — 설정 창 표시/선택용 (코어 `InkPipeline` 강도).
        let smoothing = crate::canvas::smoothing_name();
        let toast = crate::canvas::toast().unwrap_or_default();
        // 루트 클래스 `.app` — 창 배경(`bg: background`) + 방어 여백(`padding: 8`,
        // Windows 최대화 시 창을 좌우로 밀어내는 문제) + 기본 간격(`gap`).
        // 예전 egui Frame 래퍼(shell::render_root의 ROOT_INNER_MARGIN)를 CSS가 대체한다.
        <Col class="app">
            // ── 툴바 ─────────────────────────────────────────────
            // 그룹 구분은 CSS `gap`으로 한다. (`<Divider/>`는 수평 행 높이를 가용
            // 높이로 부풀리는 elm-magic-egui 결함 때문에 제외 — 어댑터의
            // `Element::Divider` 경로는 0.7.0에서도 `ui.separator()` 그대로다.)
            <Row class="toolbar">
                <Strong class="toolbar__title">"FreeDF"</Strong>
                <Button class="toolbar__button" on_click={sidebar_open = !sidebar_open}>"Sidebar"</Button>
                <Button class="toolbar__button" on_click={input = String::new(), modal = ShellModal::NewTab}>"New Tab"</Button>
                <Button class="toolbar__button" on_click={modal = ShellModal::CloseConfirm}>"Close Tab"</Button>
                <Button class="toolbar__button" on_click={crate::canvas::toggle_bookmark()}>"Bookmark"</Button>
                <Button class="toolbar__button" on_click={bookmarks_open = !bookmarks_open}>"Bookmarks"</Button>
                <Button class="toolbar__button" on_click={outline_open = !outline_open}>"Outline"</Button>
                <Button class="toolbar__button" on_click={crate::canvas::zoom_in()}>"Zoom In"</Button>
                <Button class="toolbar__button" on_click={crate::canvas::zoom_out()}>"Zoom Out"</Button>
                <Button class="toolbar__button" on_click={crate::canvas::zoom_fit()}>"Fit"</Button>
                <Button class="toolbar__button" on_click={crate::canvas::page_prev()}>"Prev Page"</Button>
                <Button class="toolbar__button" on_click={crate::canvas::page_next()}>"Next Page"</Button>
                <Button class="toolbar__button" on_click={input = String::new(), modal = ShellModal::OpenPdf}>"Open PDF"</Button>
                <Button class="toolbar__button" on_click={modal = ShellModal::ClearInk}>"Clear Ink"</Button>
                <Button class="toolbar__button" on_click={modal = ShellModal::Settings}>"Settings"</Button>
                <Button class="toolbar__button" on_click={modal = ShellModal::About}>"About"</Button>
            </Row>
            // ── 편집 행: 이력(실행취소/다시실행) + 문서 저장/불러오기 ──
            // 전부 `canvas`의 커맨드가 코어 API(`History`/`AnnotationStore`)로 배선된다.
            <Row class="toolbar">
                <Strong class="toolbar__title">"Edit"</Strong>
                <Button class="toolbar__button" on_click={crate::canvas::undo()}>"Undo"</Button>
                <Button class="toolbar__button" on_click={crate::canvas::redo()}>"Redo"</Button>
                <Button class="toolbar__button" on_click={crate::canvas::save_edits()}>"Save Edits"</Button>
                <Button class="toolbar__button" on_click={crate::canvas::load_edits()}>"Load Edits"</Button>
            </Row>
            // ── 잉크 리본: 도구/색상/굵기 — 활성 항목은 Strong(비활성은 Button) ──
            // 색상 팔레트는 settings 서비스 기본 즐겨찾기(블랙/레드/블루)와 동일.
            // (구분선은 툴바와 같은 이유로 CSS `gap`이 대신한다.)
            <Row class="ribbon">
                <Strong class="ribbon__title">"Ink"</Strong>
                {if tool == "Pen" {
                    <Strong class="ribbon__active">"[Pen]"</Strong>
                } else {
                    <Button class="ribbon__button" on_click={crate::canvas::select_tool("Pen")}>"Pen"</Button>
                }}
                {if tool == "Fountain" {
                    <Strong class="ribbon__active">"[Fountain]"</Strong>
                } else {
                    <Button class="ribbon__button" on_click={crate::canvas::select_tool("Fountain")}>"Fountain"</Button>
                }}
                {if tool == "Highlighter" {
                    <Strong class="ribbon__active">"[Highlighter]"</Strong>
                } else {
                    <Button class="ribbon__button" on_click={crate::canvas::select_tool("Highlighter")}>"Highlighter"</Button>
                }}
                {if tool == "Eraser" {
                    <Strong class="ribbon__active">"[Eraser]"</Strong>
                } else {
                    <Button class="ribbon__button" on_click={crate::canvas::select_tool("Eraser")}>"Eraser"</Button>
                }}
                // 즐겨찾기 색 스와치 — 목록/이름은 `canvas`+`palette`가 소유하고
                // 여기서는 렌더만 한다. 라벨("Swatch N")이 계약 id의 근거다
                // (`gui.swatch_1` … `gui.swatch_8` — docs/eguidev-automation.md).
                {st.swatch_items.clone().into_iter().map(|item| <Button class={if item.on { "ribbon__button ribbon__button--on" } else { "ribbon__button" }} on_click={crate::canvas::select_swatch(item.index)}>"{item.label}"</Button>)}
                {if width == "Thin" {
                    <Strong class="ribbon__active">"[Thin]"</Strong>
                } else {
                    <Button class="ribbon__button" on_click={crate::canvas::select_width("Thin")}>"Thin"</Button>
                }}
                {if width == "Medium" {
                    <Strong class="ribbon__active">"[Medium]"</Strong>
                } else {
                    <Button class="ribbon__button" on_click={crate::canvas::select_width("Medium")}>"Medium"</Button>
                }}
                {if width == "Thick" {
                    <Strong class="ribbon__active">"[Thick]"</Strong>
                } else {
                    <Button class="ribbon__button" on_click={crate::canvas::select_width("Thick")}>"Thick"</Button>
                }}
                // 필압 반영 토글 — 펜 장치가 압력을 보고할 때만 실제로 달라진다
                // (스트림이 없으면 명목 1.0이라 표시만 바뀐다).
                <Button class={if st.pressure { "ribbon__button ribbon__button--on" } else { "ribbon__button" }} on_click={crate::canvas::toggle_pressure()}>"Pressure"</Button>
            </Row>
            // ── 본문: 사이드바 + 북마크 패널 + 탭 스트립 ─────────
            // 참고: 캔버스 <Raw>는 이 Row **밖**(루트 Col 직접 자식)에 둔다 —
            // egui에서 수평 Row 안의 수직 Col은 컨텐츠 높이만 가용 높이로 받는다
            // (실측: Row 안 57px → 루트 Col 직접 자식은 남은 높이 전체).
            <Row class="app__body">
                {if sidebar_open {
                    <Col class="panel">
                        <Strong class="panel__title">"Library"</Strong>
                        {sections.iter().map(|s| <Row class="panel__row" on_click={status = format!("{} panel (placeholder)", s)}><Text class="panel__item">"{s}"</Text></Row>)}
                    </Col>
                } else {
                    <Text class="app__hidden">""</Text>
                }}
                {if bookmarks_open {
                    <Col class="panel">
                        <Strong class="panel__title">"Bookmarks"</Strong>
                        {if bookmarks.is_empty() {
                            <Text class="panel__empty">"북마크 없음 — Bookmark 버튼으로 추가"</Text>
                        } else {
                            bookmarks.iter().map(|p| <Row class="panel__row" on_click={crate::canvas::go_to_page(p)}><Text class="panel__item">"페이지 {p}"</Text></Row>)
                        }}
                    </Col>
                } else {
                    <Text class="app__hidden">""</Text>
                }}
                {if outline_open {
                    <Col class="panel">
                        <Strong class="panel__title">"Outline"</Strong>
                        {if outline_entries.is_empty() {
                            <Text class="panel__empty">"PDF를 열면 목차가 표시됩니다"</Text>
                        } else {
                            outline_entries.iter().map(|e| <Row class="panel__row" on_click={crate::canvas::go_to_page(e.page)}><Text class="panel__item">"{e.title}"</Text></Row>)
                        }}
                    </Col>
                } else {
                    <Text class="app__hidden">""</Text>
                }}
                // 탭 스트립 — id 기준 선택 (이름은 중복될 수 있다)
                // `class`가 상태를 안다: 활성 탭은 `.tabs__item--active`(수정자).
                <Row class="tabs">
                    {tab_names.iter().map(|t| <Tab class={if t.0 == active_id { "tabs__item tabs__item--active" } else { "tabs__item" }} active={t.0 == active_id} on_click={crate::canvas::select_tab(t.0)}>"{t.1}"</Tab>)}
                </Row>
            </Row>
            // 상태바 — 캔버스 위(항상 보이는 자리). `.statusbar`가 배경/여백을,
            // 안쪽 `.statusbar__text`/`.statusbar__toast`가 색을 담당한다 (Text에는 bg/padding이 적용되지 않는다).
            // 토스트가 있으면 대신 표시하고 TOAST_SECS(3초) 뒤 자동 복귀.
            <Row class="statusbar">
                {if toast.is_empty() {
                    <Text class="statusbar__text">"{status} · {canvas_status}"</Text>
                } else {
                    <Text class="statusbar__toast">"{toast}"</Text>
                }}
            </Row>
            // ── 캔버스 — <Raw> 경계: 잉크 렌더/입력은 명령형 egui (canvas.rs Phase 2).
            // 위젯 트리 밖의 상태는 canvas 모듈의 UI-스레드 엔진이 소유한다.
            // 주의: `<Raw>`는 class를 `vec![]`로 고정해 BEM 클래스를 줄 수 없다 — 캔버스 색은 `canvas.rs`의 팔레트 토큰이 담당한다.
            <Raw>|ui: &mut eframe::egui::Ui| {
                crate::canvas::paint(ui);
            }</Raw>
            // ── 모달 — 하나만 열린다 ────────────────────────────
            {match modal {
                ShellModal::None => <Text class="app__hidden">""</Text>,
                ShellModal::NewTab => <Modal class="modal" title="New Tab" on_close={modal = ShellModal::None}>
                    <Text class="modal__text">"Tab name:"</Text>
                    <Input class="modal__input" value={input.clone()} on_change={input = _} on_enter={modal = ShellModal::None, crate::canvas::add_tab(if input.trim().is_empty() { String::from("Untitled") } else { input.clone() })} />
                    <Row class="modal__actions modal__actions--end">
                        <Button class="modal__button" on_click={modal = ShellModal::None}>"Cancel"</Button>
                        <Button class="modal__button" on_click={modal = ShellModal::None, crate::canvas::add_tab(if input.trim().is_empty() { String::from("Untitled") } else { input.clone() })}>"OK"</Button>
                    </Row>
                </Modal>,
                ShellModal::OpenPdf => <Modal class="modal" title="Open PDF" on_close={modal = ShellModal::None}>
                    <Text class="modal__text">"PDF file path:"</Text>
                    <Input class="modal__input" value={input.clone()} on_change={input = _} on_enter={modal = ShellModal::None, crate::canvas::open_pdf(input.clone())} />
                    <Row class="modal__actions modal__actions--end">
                        <Button class="modal__button" on_click={modal = ShellModal::None}>"Cancel"</Button>
                        <Button class="modal__button" on_click={modal = ShellModal::None, crate::canvas::open_pdf(input.clone())}>"OK"</Button>
                    </Row>
                </Modal>,
                ShellModal::ClearInk => <Modal class="modal" title="Clear Ink" on_close={modal = ShellModal::None}>
                    <Text class="modal__text">"Remove all ink on this page?"</Text>
                    <Row class="modal__actions modal__actions--end">
                        <Button class="modal__button" on_click={modal = ShellModal::None}>"Cancel"</Button>
                        <Button class="modal__button" on_click={modal = ShellModal::None, crate::canvas::clear_ink()}>"Delete"</Button>
                    </Row>
                </Modal>,
                ShellModal::CloseConfirm => <Modal class="modal" title="Close Tab" on_close={modal = ShellModal::None}>
                    <Text class="modal__text">"Close this tab?"</Text>
                    <Row class="modal__actions modal__actions--end">
                        <Button class="modal__button" on_click={modal = ShellModal::None}>"Cancel"</Button>
                        <Button class="modal__button" on_click={modal = ShellModal::None, crate::canvas::close_tab()}>"Delete"</Button>
                    </Row>
                </Modal>,
                ShellModal::About => <Modal class="modal" title="About" on_close={modal = ShellModal::None}>
                    <Strong class="modal__title">"FreeDF GUI"</Strong>
                    <Text class="modal__text">"elm-magic shell — every widget above is a view! element"</Text>
                    <Button class="modal__button" on_click={modal = ShellModal::None}>"OK"</Button>
                </Modal>,
                ShellModal::Settings => <Modal class="modal" title="Settings" on_close={modal = ShellModal::None}>
                    <Strong class="modal__title">"잉크 기본값"</Strong>
                    <Text class="modal__text">"도구 {tool} · 색상 {color} · 굵기 {width} · 스무딩 {smoothing}"</Text>
                    <Text class="modal__text">"스무딩은 코어 `InkPipeline`의 1€ 필터 강도입니다 (Off = 원본 좌표)."</Text>
                    <Text class="modal__text">"펜 입력 {st.pen_source} · 틸트 {st.pen_tilt} · 필압 {st.pressure_text}"</Text>
                    <Text class="modal__text">"팔레트 {st.swatch_list}"</Text>
                    <Row class="modal__actions">
                        {if smoothing == "Off" {
                            <Strong class="modal__preset modal__preset--active">"Off"</Strong>
                        } else {
                            <Button class="modal__preset" on_click={crate::canvas::select_smoothing("Off")}>"Off"</Button>
                        }}
                        {if smoothing == "Light" {
                            <Strong class="modal__preset modal__preset--active">"Light"</Strong>
                        } else {
                            <Button class="modal__preset" on_click={crate::canvas::select_smoothing("Light")}>"Light"</Button>
                        }}
                        {if smoothing == "Normal" {
                            <Strong class="modal__preset modal__preset--active">"Normal"</Strong>
                        } else {
                            <Button class="modal__preset" on_click={crate::canvas::select_smoothing("Normal")}>"Normal"</Button>
                        }}
                        {if smoothing == "Strong" {
                            <Strong class="modal__preset modal__preset--active">"Strong"</Strong>
                        } else {
                            <Button class="modal__preset" on_click={crate::canvas::select_smoothing("Strong")}>"Strong"</Button>
                        }}
                    </Row>
                    <Text class="modal__text">"현재 리본 상태를 기본값으로 저장합니다 — 다음 실행 때 자동 복원."</Text>
                    <Row class="modal__actions">
                        <Button class="modal__button" on_click={crate::canvas::save_defaults()}>"Save as default"</Button>
                        <Button class="modal__button" on_click={modal = ShellModal::None}>"Close"</Button>
                    </Row>
                </Modal>,
            }}
        </Col>
    }
}

/// 셸 렌더 진입점 — 트리를 그린 뒤 어댑터 패스에 eguidev 계약 id를 붙인다.
///
/// 계측 태깅은 어댑터 `Response`가 필요해 셸 모듈이 소유하는 게 맞으므로
/// 여기서 한 번에 처리한다 — 호스트(`src/main.rs`)는 `render_root`만 알면 되고,
/// 테스트(`tests/shell_tests.rs`)는 같은 경로를 그대로 그려 검증한다.
pub fn render_shell(ui: &mut eframe::egui::Ui, ctx: &mut elm_magic::Ctx) {
    let props = ShellProps::default();
    let tree = elm_magic::frame::<Shell>(ctx, &props);
    // 팔레트를 넘겨 CSS 색 토큰(`bg: surface` …)을 실제 색으로 해석시킨다.
    // 스타일 해석 자체는 elm-magic 코어의 몫이라 freedf-gui는 값을 옮기기만 한다.
    let pass =
        elm_magic_egui::render_with_palette(ui, &tree, &mut ctx.arena, &crate::style::palette());
    // ── eguidev 계약 등록 (Phase 3 — docs/eguidev-automation.md) ──
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
/// 안쪽 여백)를 elm-magic 0.7 CSS 속성이 대체했다. 창 클리어 색은 main.rs의
/// `clear_color`가 같은 팔레트 토큰으로 채운다 (elm-magic CSS 밖 영역).
pub fn render_root(ui: &mut eframe::egui::Ui, ctx: &mut elm_magic::Ctx) {
    render_shell(ui, ctx);
}
