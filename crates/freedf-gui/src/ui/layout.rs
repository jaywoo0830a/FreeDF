//! layout — 영역(블록) 컴포넌트. 자식은 `{children}`으로 그대로 통과시키고,
//! 마크업에 붙는 것은 **블록 하나의 BEM 클래스**뿐이다.
//!
//! 하이브리드 규칙: 블록 클래스는 컨테이너에만 붙고, 요소/문맥 스타일은 CSS의
//! 하위 셀렉터(`.navbar .btn`, `.modal Strong`)가 가져간다. 그래서 호출부는
//! "무엇을 어디에 두는가"만 표현하고 스타일은 `style.rs` 한 곳에 모인다.
//!
//! (컴포넌트마다 `view!` 호출을 나눈 이유는 `atoms.rs` 모듈 문서 참고.)

/// 창 루트 — 배경/여백/간격은 `.app`이 갖는다.
elm_magic::view! {
    pub fn App() {
        <Col class="app">{children}</Col>
    }
}

/// 상단 내비게이션 바 — 브랜드 + 명령 그룹.
elm_magic::view! {
    pub fn Navbar() {
        <Row class="navbar">{children}</Row>
    }
}

/// 브랜드 (아이콘 + 이름).
elm_magic::view! {
    pub fn Brand(text: String = String::new()) {
        let l = crate::ui::label(&text);
        <Strong class="navbar__brand">"{l}"</Strong>
    }
}

/// 내비게이션 버튼 그룹 (왼쪽 정렬).
elm_magic::view! {
    pub fn Nav() {
        <Row class="navbar__nav">{children}</Row>
    }
}

/// 내비게이션 보조 그룹 (오른쪽 정렬 — 남는 폭을 채운다).
elm_magic::view! {
    pub fn NavEnd() {
        <Row class="navbar__end">{children}</Row>
    }
}

/// 도구 행 — 보기/이동 명령 (`.toolbar`).
elm_magic::view! {
    pub fn Toolbar() {
        <Row class="toolbar">{children}</Row>
    }
}

/// 도구 행의 명령 그룹 — Bootstrap `btn-group`처럼 관련 버튼을 한 덩어리로 묶는다.
elm_magic::view! {
    pub fn ToolbarGroup() {
        <Row class="toolbar__group">{children}</Row>
    }
}

/// 잉크 리본 — 도구/색/굵기 (`.ribbon`).
elm_magic::view! {
    pub fn Ribbon() {
        <Row class="ribbon">{children}</Row>
    }
}

/// 리본의 도구 그룹 — 도구/색/굵기 묶음. 그룹 사이의 경계는 CSS가 만든다.
elm_magic::view! {
    pub fn RibbonGroup() {
        <Row class="ribbon__group">{children}</Row>
    }
}

/// 사이드 패널 (`.panel`) — 제목 + 행들.
elm_magic::view! {
    pub fn Panel() {
        <Col class="panel">{children}</Col>
    }
}

/// 탭 스트립 (`.tabs`).
elm_magic::view! {
    pub fn TabStrip() {
        <Row class="tabs">{children}</Row>
    }
}

/// 상태바 (`.statusbar`).
elm_magic::view! {
    pub fn Statusbar() {
        <Row class="statusbar">{children}</Row>
    }
}

/// 모달 하나 — 제목/닫기 핸들러는 elm-magic `Modal` 태그의 계약 그대로.
elm_magic::view! {
    pub fn Dialog(title: String = String::new(), on_close: fn(bool)) {
        <Modal class="modal" title={title} on_close={on_close(false)}>{children}</Modal>
    }
}

/// 모달의 액션 행 — 오른쪽 정렬.
elm_magic::view! {
    pub fn Actions() {
        <Row class="modal__actions modal__actions--end">{children}</Row>
    }
}

/// 모달의 도구 행 (왼쪽 정렬 — 프리셋 나열).
elm_magic::view! {
    pub fn Presets() {
        <Row class="modal__actions">{children}</Row>
    }
}
