//! layout — 영역(블록) 컴포넌트. 자식은 `{children}`으로 그대로 통과시키고,
//! 마크업에 붙는 것은 **블록 하나의 BEM 클래스**뿐이다.
//!
//! 하이브리드 규칙: 블록 클래스는 컨테이너에만 붙고, 문맥 차이는 CSS의 하위
//! 셀렉터(`.navbar .btn`, `.ribbon .btn`)가 가져간다. 그래서 호출부는 "무엇을
//! 어디에 두는가"만 표현하고 치수/색은 `style.rs` 한 곳에 모인다.
//!
//! 컴포넌트마다 `view!`를 나누지 않는다 — 0.7.2부터 한 블록에 여러 `fn`을 쓸 수
//! 있고, 문서 주석도 블록 안에서 생성 항목의 rustdoc이 된다 (`ui::atoms` 참고).
//!
//! 동적 값과 `{children}`은 재렌더마다 갱신된다 — 값 prop도 0.7.4부터 살아 있는
//! prop이라(부모가 넘긴 새 값이 반영된다) 영역 컴포넌트에 우회가 필요 없다.
//! 자세한 계약은 `ui::atoms` 모듈 문서 참고.

elm_magic::view! {
    /// 창 루트 — 배경/여백/간격은 `.app`이 갖는다 (Bootstrap container 결).
    pub fn App() {
        <Col class="app">{children}</Col>
    }

    /// 상단 내비게이션 바 — 브랜드 + 명령 그룹 (`.navbar`).
    pub fn Navbar() {
        <Row class="navbar">{children}</Row>
    }

    /// 브랜드 (아이콘 + 이름) — `.navbar__brand`.
    pub fn Brand(text: String = String::new()) {
        let l = crate::ui::label(&text);
        <Strong class="navbar__brand">"{l}"</Strong>
    }

    /// 내비게이션 버튼 그룹 (왼쪽 정렬, `.navbar__nav`).
    pub fn Nav() {
        <Row class="navbar__nav">{children}</Row>
    }

    /// 내비게이션 보조 그룹 (오른쪽 정렬 — 남는 폭을 밀어낸다, `.navbar__end`).
    pub fn NavEnd() {
        <Row class="navbar__end">{children}</Row>
    }

    /// 도구 행 — 보기/이동 명령 (`.toolbar`).
    pub fn Toolbar() {
        <Row class="toolbar">{children}</Row>
    }

    /// 도구 행의 명령 그룹 — 관련 버튼을 한 덩어리로 묶는다 (`.toolbar__group`,
    /// Bootstrap `btn-group` 결).
    pub fn ToolbarGroup() {
        <Row class="toolbar__group">{children}</Row>
    }

    /// 잉크 리본 — 도구/색/굵기 (`.ribbon`).
    pub fn Ribbon() {
        <Row class="ribbon">{children}</Row>
    }

    /// 리본의 도구 그룹 — 그룹 경계는 CSS가 만든다 (`.ribbon__group`).
    pub fn RibbonGroup() {
        <Row class="ribbon__group">{children}</Row>
    }

    /// 사이드 패널 (`.panel`) — 제목 + 행들.
    pub fn Panel() {
        <Col class="panel">{children}</Col>
    }

    /// 탭 스트립 (`.tabs`, Bootstrap `nav-tabs` 결).
    pub fn TabStrip() {
        <Row class="tabs">{children}</Row>
    }

    /// 상태바 (`.statusbar`).
    pub fn Statusbar() {
        <Row class="statusbar">{children}</Row>
    }

    /// 모달 하나 — 제목/닫기 핸들러는 elm-magic `Modal` 태그의 계약 그대로.
    ///
    /// 본문 안의 제목은 `ui::atoms::Heading`을 쓴다(`.modal__title`).
    pub fn Dialog(title: String = String::new(), on_close: fn()) {
        <Modal class="modal" title={title} on_close={on_close()}>{children}</Modal>
    }

    /// 모달의 액션 행 — 오른쪽 정렬 (Bootstrap `modal-footer` 결).
    pub fn Actions() {
        <Row class="modal__actions modal__actions--end">{children}</Row>
    }

    /// 모달의 프리셋 행 — 왼쪽 정렬 (설정 창의 선택지 나열).
    pub fn Presets() {
        <Row class="modal__actions">{children}</Row>
    }
}
