//! 셸 — `elm_magic::view!` 트리. **하이브리드 구조**: 시맨틱 태그 + 재사용 컴포넌트.
//!
//! 이 파일은 "무엇을 어디에 두는가"만 표현한다:
//!
//! - 영역은 [`crate::ui::layout`] 컴포넌트(`Chrome`/`TopBar`/`ToolBar`/`Panel`/…)
//! - 항목은 [`crate::ui::atoms`] 컴포넌트(`Btn`/`BtnOn`/`BtnSel`/`Swatch`/`StatusText`/…)
//! - 클래스 이름은 컴포넌트가 갖고, **스타일은 전부 `style.rs` CSS**가 갖는다
//!
//! ## 자식 순서 = 배치 순서 (캔버스가 마지막)
//!
//! `App`의 자식 순서가 곧 위→아래 배치다:
//! `Chrome`(TopBar → Rule → 잉크 줄 → Rule → 보기 줄) → `Statusbar` →
//! `Row.app__body`(fill).
//! 캔버스 `<Raw>`는 `.app__body`의 **마지막 자식**이다 — `canvas::paint_ui`가
//! `ui.available_size()`를 전부 소비하므로 같은 컨테이너에서 뒤에 형제를 두면
//! 그 형제는 0px가 된다. 그래서 정보 스트립은 캔버스 **위**에 온다.
//!
//! ## 위계는 두 단계다
//!
//! 하나만 켜지는 도구는 액센트 채움([`BtnOn`]), 여럿이 켜질 수 있는 토글(굵기/필압/
//! 패널)은 조용한 선택([`BtnSel`])이다. 파괴 동작만 `BtnDanger`, 모달의 주 동작은
//! `BtnPrimary`다 — 이 배분이 "무엇이 켜져 있는가"를 색 없이도 읽히게 한다.
//!
//! `view!` 본문 주의: 제네릭 타입 표기(`Vec<(A,B)>`)나 복잡한 `if` 식을 문장으로
//! 쓰면 매크로 파서가 태그로 오인한다(실측). 그래서 계산은 [`shell_state`]에서
//! 끝내고 본문에는 단순한 `let`만 둔다.
//!
//! ## 값 prop은 살아 있는 prop
//!
//! 자식 컴포넌트의 값 prop(`on`/`active`/`text`)은 부모가 새 값을 넘기면 화면이
//! 따라온다. 자식이 그 매개변수를 직접 쓰면 그때부터는 자식의 상태가 되지만
//! (`Arena::slot_dirty`), 셸의 컴포넌트는 값을 표시만 하므로 키 우회가 필요 없다.

use crate::ui;
use crate::ui::*;

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
#[derive(Clone)]
pub struct SwatchItem {
    pub index: usize,
    /// 아이콘 + 라벨 (`"◯ Swatch 1"`) — 계약 id는 `ui::slug`가 ASCII만 남겨 만든다.
    pub label: String,
    /// 지금 선택된 색인가 (활성 표시 — `swatch--on`).
    pub on: bool,
}

/// 셸이 매 프레임 읽는 캔버스 상태 스냅샷 — 팔레트/필압/펜 진단.
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
                label: ui::label(crate::palette::label_str(i)),
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

/// 설정 창의 사실 목록 — (라벨, 값) 쌍. 계산은 `view!` **밖**에서 끝낸다
/// (모듈 문서: 본문에는 단순한 `let`만 둔다). 0.8 `<For>`가 한 줄씩 그린다.
pub fn settings_facts(
    st: &ShellState,
    tool: &str,
    color: &str,
    width: &str,
    smoothing: &str,
) -> Vec<(String, String)> {
    vec![
        (String::from("도구"), String::from(tool)),
        (String::from("색상"), String::from(color)),
        (String::from("굵기"), String::from(width)),
        (String::from("스무딩"), String::from(smoothing)),
        (String::from("펜 입력"), st.pen_source.clone()),
        (String::from("틸트"), String::from(st.pen_tilt)),
        (String::from("필압"), String::from(st.pressure_text)),
        (String::from("팔레트"), st.swatch_list.clone()),
    ]
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
        // 주의: `sync_and_status`의 인자는 **모달이 열렸는지**다.
        let modal_open = !matches!(modal, ShellModal::None);
        let canvas_status = crate::canvas::sync_and_status(modal_open);
        // 리본/토스트 — 캔버스 엔진이 소유한 상태를 매 프레임 읽어 렌더만 한다.
        let tool = crate::canvas::tool_name();
        let color = crate::canvas::color_name();
        let width = crate::canvas::width_name();
        let st = shell_state();
        // 스무딩 프리셋 — 설정 창 표시/선택용 (코어 `InkPipeline` 강도).
        let smoothing = crate::canvas::smoothing_name();
        let toast = crate::canvas::toast().unwrap_or_default();
        // 설정 창 사실 목록 — 문장 조립은 `settings_facts`(view! 밖)에서 끝낸다.
        let facts = settings_facts(&st, &tool, &color, &width, &smoothing);
        // 패널 머리의 개수 배지 — 목록 길이를 문자열로 만들어 prop에 바로 넘긴다.
        let sections_count = sections.len().to_string();
        let bookmarks_count = bookmarks.len().to_string();
        let outline_count = outline_entries.len().to_string();
        <App>
            // ── chrome: 상단 크롬 **한 판** ────────────────────────
            // 바마다 라운드 카드를 쌓지 않는다 — 구획은 헤어라인(`Rule`)이 만든다.
            // 크롬 계층: TopBar → Rule → 도구 줄 → Rule → 보기 줄.
            <Chrome>
                // ── topbar: 브랜드 · 탭 · 문서 명령 · 앱 명령(오른쪽 끝) ──
                // 앱 명령은 `.topbar__end`의 `justify: end`로 오른쪽 끝에 붙는다.
                <TopBar>
                    <Brand text="FreeDF" />
                    <TabStrip>
                        <For each={tab_names} as={t}>
                            <TabItem text={t.1.clone()} active={t.0 == active_id} on_click={crate::canvas::select_tab(t.0)} />
                        </For>
                    </TabStrip>
                    <Nav>
                        <Btn text="New Tab" on_click={input = String::new(), modal = ShellModal::NewTab} />
                        <Btn text="Close Tab" on_click={modal = ShellModal::CloseConfirm} />
                        <Btn text="Open PDF" on_click={input = String::new(), modal = ShellModal::OpenPdf} />
                    </Nav>
                    <TopEnd>
                        <BtnGhost text="Settings" on_click={modal = ShellModal::Settings} />
                        <BtnGhost text="About" on_click={modal = ShellModal::About} />
                    </TopEnd>
                </TopBar>
                <Rule />
                // ── 잉크 줄 (3밴드): 재료 / 굵기·편집 / 보기·패널·위험 ──
                // 줄은 명시적으로 나눈다(`wrap`은 이 트리에서 동작하지 않는다 —
                // style.rs 모듈 문서). 도구(하나만 켜짐)는 액센트 채움, 굵기/필압
                // (여럿이 켜질 수 있음)은 조용한 선택(`BtnSel`)이다.
                <ToolBar>
                    <BarGroup>
                        <BtnOn text="Pen" on={tool == "Pen"} on_click={crate::canvas::select_tool("Pen")} />
                        <BtnOn text="Fountain" on={tool == "Fountain"} on_click={crate::canvas::select_tool("Fountain")} />
                        <BtnOn text="Highlighter" on={tool == "Highlighter"} on_click={crate::canvas::select_tool("Highlighter")} />
                        <BtnOn text="Eraser" on={tool == "Eraser"} on_click={crate::canvas::select_tool("Eraser")} />
                    </BarGroup>
                    <Sep />
                    <BarGroup>
                        <For each={st.swatch_items.clone()} as={item}>
                            <Swatch text={item.label.clone()} on={item.on} on_click={crate::canvas::select_swatch(item.index)} />
                        </For>
                    </BarGroup>
                </ToolBar>
                <ToolBar>
                    <BarGroup>
                        <BtnSel text="Thin" on={width == "Thin"} on_click={crate::canvas::select_width("Thin")} />
                        <BtnSel text="Medium" on={width == "Medium"} on_click={crate::canvas::select_width("Medium")} />
                        <BtnSel text="Thick" on={width == "Thick"} on_click={crate::canvas::select_width("Thick")} />
                        <BtnSel text="Pressure" on={st.pressure} on_click={crate::canvas::toggle_pressure()} />
                    </BarGroup>
                    <Sep />
                    <BarGroup>
                        <Btn text="Undo" on_click={crate::canvas::undo()} />
                        <Btn text="Redo" on_click={crate::canvas::redo()} />
                    </BarGroup>
                    <Sep />
                    <BarGroup>
                        <Btn text="Save Edits" on_click={crate::canvas::save_edits()} />
                        <Btn text="Load Edits" on_click={crate::canvas::load_edits()} />
                        <Btn text="Bookmark" on_click={crate::canvas::toggle_bookmark()} />
                    </BarGroup>
                </ToolBar>
                <Rule />
                // ── 보기/패널 줄: 줌·페이지 / 패널 토글 ───────────────
                <ToolBar>
                    <BarGroup>
                        <Btn text="Zoom In" on_click={crate::canvas::zoom_in()} />
                        <Btn text="Zoom Out" on_click={crate::canvas::zoom_out()} />
                        <Btn text="Fit" on_click={crate::canvas::zoom_fit()} />
                    </BarGroup>
                    <Sep />
                    <BarGroup>
                        <Btn text="Prev Page" on_click={crate::canvas::page_prev()} />
                        <Btn text="Next Page" on_click={crate::canvas::page_next()} />
                    </BarGroup>
                    <Sep />
                    <BarGroup>
                        <BtnSel text="Sidebar" on={sidebar_open} on_click={sidebar_open = !sidebar_open} />
                        <BtnSel text="Bookmarks" on={bookmarks_open} on_click={bookmarks_open = !bookmarks_open} />
                        <BtnSel text="Outline" on={outline_open} on_click={outline_open = !outline_open} />
                    </BarGroup>
                    // 위험 동작은 **편집 묶음에서 떼어** 마지막 줄 끝에 둔다: 편집 버튼
                    // 바로 옆에 붙어 있으면 오클릭 위험이 크고, 편집 줄 폭이 900px 창에서
                    // 여유 8px밖에 없어(실측) 이 줄이 숨통을 틔운다.
                    <Sep />
                    <BarGroup>
                        <BtnDanger text="Clear Ink" on_click={modal = ShellModal::ClearInk} />
                    </BarGroup>
                </ToolBar>
            </Chrome>
            // ── body: 사이드바 + 캔버스 (남은 세로 전부) ────────────
            // 캔버스 `<Raw>`는 이 행의 **마지막 자식**이다 — `available_size()`를
            // 전부 먹으므로 같은 컨테이너에서 뒤에 형제를 두면 그 형제는 0px가 된다.
            // `.app__body { height: fill }`이 행 높이를 확정하므로 사이드바도
            // `height: fill`로 캔버스와 같은 높이를 갖는다(진짜 사이드바).
            <Row class="app__body">
                <If when={sidebar_open}>
                    <Panel>
                        <PanelHead text="Library" count={sections_count} />
                        <PanelList>
                            <For each={sections.clone()} as={s}>
                                <PanelRow text={s.clone()} on_click={status = format!("{} panel (placeholder)", s)} />
                            </For>
                        </PanelList>
                    </Panel>
                </If>
                <If when={bookmarks_open}>
                    <Panel>
                        <PanelHead text="Bookmarks" count={bookmarks_count} />
                        <If when={bookmarks.is_empty()}>
                            <Empty text="북마크 없음 — Bookmark 버튼으로 추가" />
                        <Else>
                            <PanelList>
                                <For each={bookmarks.clone()} as={p}>
                                    <PanelRow text="페이지 {p}" on_click={crate::canvas::go_to_page(p)} />
                                </For>
                            </PanelList>
                        </Else>
                        </If>
                    </Panel>
                </If>
                <If when={outline_open}>
                    <Panel>
                        <PanelHead text="Outline" count={outline_count} />
                        <If when={outline_entries.is_empty()}>
                            <Empty text="PDF를 열면 목차가 표시됩니다" />
                        <Else>
                            <PanelList>
                                <For each={outline_entries.clone()} as={e}>
                                    <PanelRow text={e.title.clone()} meta={e.page.to_string()} on_click={crate::canvas::go_to_page(e.page)} />
                                </For>
                            </PanelList>
                        </Else>
                        </If>
                    </Panel>
                </If>
                // ── 캔버스 — <Raw> 경계: 잉크 렌더/입력은 명령형 egui (canvas.rs) ──
                // 위젯 트리 밖의 상태는 canvas 모듈의 UI-스레드 엔진이 소유한다.
                // `<Raw>`는 class를 받지 않는다 — 캔버스 색은 canvas.rs의 리터럴.
                <Raw>|ui: &mut eframe::egui::Ui| {
                    crate::canvas::paint(ui);
                }</Raw>
            </Row>
            // ── statusbar: 상태/토스트(좌) + 문서 메타(우) — **창 하단** ─────
            // `.app__body { height: fill }`이 남은 세로를 받고, 이 스트립은 그 **뒤**
            // 형제라 자기 높이(26)만 갖는다: 0.8.1 어댑터의 `apply_size`는 `fill`
            // 예산에서 **뒤 형제의 몫을 먼저 뺀다**(0.7의 "뒤에 두면 0px" 문제는
            // 0.8.1에서 사라졌다 — 실측: 스트립 y=682, 캔버스 h=456).
            // 상태 문자열은 트리 노드가 소유한다(자동화 assert_text 계약).
            <Statusbar>
                <If when={toast.is_empty()}>
                    <StatusText text={status.clone()} />
                <Else>
                    <StatusToast text={toast.clone()} />
                </Else>
                </If>
                <StatusMeta>
                    <StatusText text={canvas_status.clone()} />
                </StatusMeta>
            </Statusbar>
            // ── 모달 — 하나만 열린다 ───────────────────────────────
            // 0.8 `<Switch>`: `match` 대신 태그로 분기한다(래퍼 노드를 만들지 않는다).
            // 어느 분기도 맞지 않으면 `<Default>` — `None`이 여기서 빈 자리표시가 된다.
            <Switch on={modal}>
                <Case when={ShellModal::NewTab}>
                    <Dialog title="New Tab" on_close={modal = ShellModal::None}>
                        <Group>
                            <Note text="Tab name:" />
                            <Input class="modal__input" value={input.clone()} on_change={input = _} on_enter={modal = ShellModal::None, crate::canvas::add_tab(if input.trim().is_empty() { String::from("Untitled") } else { input.clone() })} />
                        </Group>
                        <ModalRule />
                        <Actions>
                            <BtnGhost text="Cancel" on_click={modal = ShellModal::None} />
                            <BtnPrimary text="OK" on_click={modal = ShellModal::None, crate::canvas::add_tab(if input.trim().is_empty() { String::from("Untitled") } else { input.clone() })} />
                        </Actions>
                    </Dialog>
                </Case>
                <Case when={ShellModal::OpenPdf}>
                    <Dialog title="Open PDF" on_close={modal = ShellModal::None}>
                        <Group>
                            <Note text="PDF file path:" />
                            <Input class="modal__input" value={input.clone()} on_change={input = _} on_enter={modal = ShellModal::None, crate::canvas::open_pdf(input.clone())} />
                        </Group>
                        <ModalRule />
                        <Actions>
                            <BtnGhost text="Cancel" on_click={modal = ShellModal::None} />
                            <BtnPrimary text="OK" on_click={modal = ShellModal::None, crate::canvas::open_pdf(input.clone())} />
                        </Actions>
                    </Dialog>
                </Case>
                <Case when={ShellModal::ClearInk}>
                    <Dialog title="Clear Ink" on_close={modal = ShellModal::None}>
                        <Warn text="Remove all ink on this page?" />
                        <ModalRule />
                        <Actions>
                            <BtnGhost text="Cancel" on_click={modal = ShellModal::None} />
                            <BtnDanger text="Delete" on_click={modal = ShellModal::None, crate::canvas::clear_ink()} />
                        </Actions>
                    </Dialog>
                </Case>
                <Case when={ShellModal::CloseConfirm}>
                    <Dialog title="Close Tab" on_close={modal = ShellModal::None}>
                        <Warn text="Close this tab?" />
                        <ModalRule />
                        <Actions>
                            <BtnGhost text="Cancel" on_click={modal = ShellModal::None} />
                            <BtnDanger text="Delete" on_click={modal = ShellModal::None, crate::canvas::close_tab()} />
                        </Actions>
                    </Dialog>
                </Case>
                <Case when={ShellModal::About}>
                    <Dialog title="About" on_close={modal = ShellModal::None}>
                        <Heading text="FreeDF GUI" />
                        <Note text="elm-magic shell — every widget above is a view! element" />
                        <ModalRule />
                        <Actions>
                            <BtnPrimary text="OK" on_click={modal = ShellModal::None} />
                        </Actions>
                    </Dialog>
                </Case>
                <Case when={ShellModal::Settings}>
                    <Dialog title="Settings" on_close={modal = ShellModal::None}>
                        <Heading text="잉크 기본값" />
                        // 사실 목록은 **라벨/값 표**다 — 라벨 폭을 고정해 값이 세로로
                        // 정렬된다(산문 4줄보다 훑기 쉽다). 0.8 `<For>` + `<Fact>`.
                        <Facts>
                            <For each={facts} as={fact}>
                                <Fact label={fact.0.clone()} value={fact.1.clone()} />
                            </For>
                        </Facts>
                        <Note text="스무딩은 코어 `InkPipeline`의 1€ 필터 강도입니다 (Off = 원본 좌표)." />
                        <Presets>
                            <BtnOn text="Off" on={smoothing == "Off"} on_click={crate::canvas::select_smoothing("Off")} />
                            <BtnOn text="Light" on={smoothing == "Light"} on_click={crate::canvas::select_smoothing("Light")} />
                            <BtnOn text="Normal" on={smoothing == "Normal"} on_click={crate::canvas::select_smoothing("Normal")} />
                            <BtnOn text="Strong" on={smoothing == "Strong"} on_click={crate::canvas::select_smoothing("Strong")} />
                        </Presets>
                        <Note text="현재 리본 상태를 기본값으로 저장합니다 — 다음 실행 때 자동 복원." />
                        <ModalRule />
                        <Actions>
                            <BtnGhost text="Close" on_click={modal = ShellModal::None} />
                            <BtnPrimary text="Save as default" on_click={crate::canvas::save_defaults()} />
                        </Actions>
                    </Dialog>
                </Case>
                <Default>
                    <Text class="app__hidden">""</Text>
                </Default>
            </Switch>
        </App>
    }
}

/// 셸 렌더 진입점 — 트리를 그린 뒤 어댑터 패스에 eguidev 계약 id를 붙인다.
///
/// 계측 태깅은 어댑터 `Response`가 필요해 셸 모듈이 소유한다 — 호스트
/// (`src/main.rs`)는 `render_root`만 알면 되고, 테스트(`tests/shell_tests.rs`)는
/// 같은 경로를 그대로 그려 검증한다.
pub fn render_shell(ui: &mut eframe::egui::Ui, ctx: &mut elm_magic::Ctx) {
    let props = ShellProps::default();
    let tree = elm_magic::frame::<Shell>(ctx, &props);
    // 팔레트를 넘겨 CSS 색 토큰(`bg: surface` …)을 실제 색으로 해석시킨다.
    // 스타일 해석 자체는 elm-magic 코어의 몫이라 freedf-gui는 값을 옮기기만 한다.
    let pass =
        elm_magic_egui::render_with_palette(ui, &tree, &mut ctx.arena, &crate::style::palette());
    // ── eguidev 계약 등록 (docs/eguidev-automation.md) ──
    // id 규칙: `gui.<라벨 슬러그>`. 슬러그는 **ASCII만** 남기므로 아이콘 글리프가
    // 라벨에 섞여도 id는 아이콘 도입 전과 같다 (`gui.new_tab`, `gui.swatch_1`).
    // 같은 라벨이 한 프레임에 두 번 이상 나오면 `.<n>` 접미사 (eguidev 중복 id 방지).
    let mut seen: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (label, resp) in &pass.buttons {
        let slug = ui::slug(label);
        if slug.is_empty() {
            continue;
        }
        let n = seen.entry(slug.clone()).or_insert(0);
        let id = if *n == 0 {
            format!("gui.{slug}")
        } else {
            format!("gui.{slug}.{}", *n)
        };
        *n += 1;
        // 아이콘 글리프(사설 영역)를 걷어낸 텍스트 — 자동화가 라벨로도 찾을 수 있게.
        let text: String = label
            .chars()
            .skip_while(|c| ('\u{e000}'..='\u{f8ff}').contains(c))
            .collect();
        crate::dev::tag_button(ui, id, text.trim().to_string(), resp);
    }
}

/// 한 프레임 렌더 — 셸 하나만 그린다.
///
/// 배경/여백은 **CSS가 담당한다** (`.app { bg: background; padding: 8 }`) — 예전
/// egui Frame 래퍼(테마 `window_fill` 채우기 + 하드코딩된 안쪽 여백)를 elm-magic
/// CSS 속성이 대체했다. 창 클리어 색은 main.rs의 `clear_color`가 같은 팔레트
/// 토큰으로 채운다 (elm-magic CSS 밖 영역).
pub fn render_root(ui: &mut eframe::egui::Ui, ctx: &mut elm_magic::Ctx) {
    render_shell(ui, ctx);
}
