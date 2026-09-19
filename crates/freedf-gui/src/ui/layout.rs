//! layout — 영역(블록) 컴포넌트. 자식은 `{children}`으로 그대로 통과시키고,
//! 마크업에 붙는 것은 **블록 하나의 BEM 클래스**뿐이다.
//!
//! 크롬 계층:
//!
//! ```text
//! Chrome                       바탕 판 **하나** (surface, radius 10)
//! ├ TopBar                     브랜드 · 탭 · 문서 명령 · 앱 명령(오른쪽 끝)
//! ├ Rule                       헤어라인 — 판을 늘리지 않고 구획만 만든다
//! ├ ToolBar (잉크)             도구 · 색 · 굵기 · 필압 · 편집
//! ├ Rule
//! ├ ToolBar (보기/문서)         줌 · 페이지 · 저장 · 북마크 · 패널 토글
//! Statusbar                    캔버스 위 정보 스트립 (바탕 없음)
//! ```
//!
//! 폭·여백·색은 여기서 정하지 않는다 — `src/style.rs`의 CSS가 갖는다.
//!
//! ## `ToolBar` 줄은 명시적으로 나눈다
//!
//! `wrap: true`는 이 트리에서 줄바꿈을 만들지 못한다(실측 — `style.rs` 모듈 문서).
//! 그래서 [`ToolBar`] 한 줄이 한 줄이고, 각 줄은 **좁은 창에서도 넘지 않는 폭**으로
//! 유지한다. [`BarGroup`]은 "같은 뜻의 칩"을 붙여 두는 의미 묶음(안쪽 `gap` 2)이고,
//! 묶음 사이는 헤어라인 [`Sep`]이 나눈다.
//!
//! ## 컴포넌트마다 `view!`를 나누지 않는다
//!
//! 한 블록에 여러 `fn`을 쓸 수 있고, 문서 주석도 블록 안에서 생성 항목의 rustdoc이
//! 된다 (`ui::atoms` 참고).

elm_magic::view! {
    /// 창 루트 — 배경/여백/간격은 `.app`이 갖는다.
    pub fn App() {
        <Col class="app">{children}</Col>
    }

    /// 상단 크롬 전체 — **한 판**. 바를 나누지 않고 헤어라인으로 구획한다.
    pub fn Chrome() {
        <Col class="chrome">{children}</Col>
    }

    /// 상단 바 — 브랜드 · 탭 · 문서/앱 명령 (`.topbar`).
    pub fn TopBar() {
        <Row class="topbar">{children}</Row>
    }

    /// 브랜드 (아이콘 + 이름) — `.topbar__brand`.
    pub fn Brand(text: String = String::new()) {
        let l = crate::ui::label(&text);
        <Strong class="topbar__brand">"{l}"</Strong>
    }

    /// 상단 바의 문서 명령 그룹 (`.topbar__nav`) — New/Close/Open.
    pub fn Nav() {
        <Row class="topbar__nav">{children}</Row>
    }

    /// 상단 바의 앱 명령 그룹 (`.topbar__end`) — 오른쪽 끝에 붙는다.
    ///
    /// `.topbar__end { width: fill; justify: end }` — 남는 폭을 받아 오른쪽으로 민다.
    pub fn TopEnd() {
        <Row class="topbar__end">{children}</Row>
    }

    /// 문서 탭 스트립 (`.tabs`) — TopBar **안**에 인라인으로 들어간다.
    pub fn TabStrip() {
        <Row class="tabs">{children}</Row>
    }

    /// 도구 줄 (`.bar`) — 한 줄이 한 줄이다(`wrap`은 이 트리에서 동작하지 않는다).
    ///
    /// 잉크 줄(도구·색·굵기·편집)과 보기 줄(줌·페이지·문서·패널)이 같은 컴포넌트를
    /// 쓴다 — 둘은 내용만 다르고 생김새는 같다.
    pub fn ToolBar() {
        <Row class="bar">{children}</Row>
    }

    /// 도구 줄의 의미 묶음 (`.bar__group`) — 안쪽 `gap` 2로 붙여 둔다.
    pub fn BarGroup() {
        <Row class="bar__group">{children}</Row>
    }

    /// 그룹 구분 세로 헤어라인 (`.bar__sep`) — 1×18px, 상하 마진 4 (= 컨트롤 높이 26).
    pub fn Sep() {
        <Col class="bar__sep" />
    }

    /// 크롬 안의 가로 헤어라인 (`.bar__rule`) — 판을 늘리지 않고 구획한다.
    pub fn Rule() {
        <Col class="bar__rule" />
    }

    /// 사이드바 패널 (`.panel`) — 캔버스와 같은 높이를 갖는다(`height: fill`).
    pub fn Panel() {
        <Col class="panel">{children}</Col>
    }

    /// 패널 머리 (`.panel__head`) — 제목 + 헤어라인.
    ///
    /// 제목만 떠 있으면 첫 행과 구분되지 않는다 — 헤어라인이 목록의 시작을 만든다.
    pub fn PanelHead(text: String = String::new()) {
        let l = crate::ui::label(&text);
        <Col class="panel__head">
            <Strong class="section__title">"{l}"</Strong>
            <Col class="panel__rule" />
        </Col>
    }

    /// 정보 스트립 (`.statusbar`) — 캔버스 **위**에 온다.
    ///
    /// 캔버스(`<Raw>`)가 `available_size()`를 전부 먹으므로 스트립이 뒤에 오면
    /// 0px가 된다.
    pub fn Statusbar() {
        <Row class="statusbar">{children}</Row>
    }

    /// 정보 스트립의 우측 메타 그룹 (`.statusbar__meta`).
    pub fn StatusMeta() {
        <Row class="statusbar__meta">{children}</Row>
    }

    /// 모달 하나 — 제목/닫기 핸들러는 elm-magic `Modal` 태그의 계약 그대로.
    ///
    /// 본문 안의 제목은 `ui::atoms::Heading`을 쓴다(`.modal__title`).
    pub fn Dialog(title: String = String::new(), on_close: fn()) {
        <Modal class="modal" title={title} on_close={on_close()}>{children}</Modal>
    }

    /// 모달의 액션 행 — **오른쪽 정렬**(`.modal__actions--end`).
    ///
    /// 순서는 닫는 동작(`BtnGhost`)이 먼저, 주 동작(`BtnPrimary`)이 **마지막**이다 —
    /// 오른쪽 끝이 주 동작이 된다. (`justify: end`는 밀 공간이 있는 컨테이너에서
    /// 동작한다: 모달 창은 내용보다 넓다 — 실측은 `tests/elm_magic_bugs.rs`.)
    pub fn Actions() {
        <Row class="modal__actions modal__actions--end">{children}</Row>
    }

    /// 모달의 프리셋 행 — 왼쪽 정렬 (설정 창의 선택지 나열).
    pub fn Presets() {
        <Row class="modal__actions">{children}</Row>
    }

    /// 모달 안에서 **붙어 있어야 하는 것들의 묶음** (`.modal__group`, `gap` 4).
    ///
    /// 모달의 `gap` 12는 **블록 사이** 값이라, 라벨과 그 필드(또는 한 덩어리인 사실
    /// 목록)를 그냥 나란히 두면 서로 다른 블록처럼 12px씩 떨어진다 — 붙어 있어야 하는
    /// 것은 이 컴포넌트로 묶는다. 안쪽은 4(사다리), 블록 사이는 12가 유지된다.
    pub fn Group() {
        <Col class="modal__group">{children}</Col>
    }
}
