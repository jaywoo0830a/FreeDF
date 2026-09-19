//! layout — 영역(블록) 컴포넌트. 자식은 `{children}`으로 그대로 통과시키고,
//! 마크업에 붙는 것은 **블록 하나의 BEM 클래스**뿐이다.
//!
//! 설계 사양: `docs/DESIGN-SYSTEM.md` §6(예상 트리). 이 파일이 그 트리의 컨테이너
//! 층이다 — [`TopBar`](브랜드·탭·문서/앱 명령) · [`InkBar`](잉크) ·
//! [`ViewBar`](보기·이동·패널) · [`Panel`](사이드바) · [`Statusbar`](캔버스 위 정보
//! 스트립) · [`Dialog`](모달).
//!
//! 폭·여백·색은 여기서 정하지 않는다 — `style.rs`의 CSS가 갖는다.
//!
//! ## 우측 정렬·`wrap`은 쓰지 않는다
//!
//! `justify: end`는 콘텐츠 크기 자식에서 무효이고, `width: fill` 스페이서는 부모
//! `max_rect`를 창 밖으로 팽창시킨다(실측: 캔버스 폭 1754 > 창 1100). `wrap`은
//! 어댑터가 읽지 않아 행이 항상 단일 줄이다 — 근거는 `style.rs` 모듈 문서.
//! 그래서 `__end` 그룹은 "의미상 묶음"일 뿐이고 배치는 왼쪽부터 차례로 흐른다.
//!
//! 컴포넌트마다 `view!`를 나누지 않는다 — 0.7.2부터 한 블록에 여러 `fn`을 쓸 수
//! 있고, 문서 주석도 블록 안에서 생성 항목의 rustdoc이 된다 (`ui::atoms` 참고).

elm_magic::view! {
    /// 창 루트 — 배경/여백/간격은 `.app`이 갖는다 (Bootstrap container 결).
    pub fn App() {
        <Col class="app">{children}</Col>
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

    /// 상단 바의 앱 명령 그룹 (`.topbar__end`) — 앞에 [`Spacer`]를 둬 우측 정렬.
    pub fn TopEnd() {
        <Row class="topbar__end">{children}</Row>
    }

    /// 문서 탭 스트립 (`.tabs`) — TopBar **안**에 인라인으로 들어간다.
    pub fn TabStrip() {
        <Row class="tabs">{children}</Row>
    }

    /// 잉크 바 — 도구/색/편집/굵기 (`.inkbar`, **2줄**).
    ///
    /// 자식은 [`InkLine`] 두 개다 — elm-magic 행은 줄바꿈하지 않으므로(모듈 문서)
    /// 줄을 명시적으로 나눠 좁은 창에서도 화면 밖으로 나가지 않게 한다.
    pub fn InkBar() {
        <Col class="inkbar">{children}</Col>
    }

    /// 잉크 바의 한 줄 (`.inkbar__line`).
    pub fn InkLine() {
        <Row class="inkbar__line">{children}</Row>
    }

    /// 잉크 바의 그룹 (`.inkbar__group`) — 경계는 그룹 사이 `gap`과 [`Sep`]이 만든다.
    pub fn InkGroup() {
        <Row class="inkbar__group">{children}</Row>
    }

    /// 보기/이동 바 (`.viewbar`, **2줄**).
    pub fn ViewBar() {
        <Col class="viewbar">{children}</Col>
    }

    /// 보기 바의 한 줄 (`.viewbar__line`).
    pub fn ViewLine() {
        <Row class="viewbar__line">{children}</Row>
    }

    /// 보기 바의 그룹 (`.viewbar__group`).
    pub fn ViewGroup() {
        <Row class="viewbar__group">{children}</Row>
    }

    /// 그룹 구분 헤어라인 (`.bar__sep`) — 1×20px 세로선.
    pub fn Sep() {
        <Col class="bar__sep" />
    }

    /// 사이드바 패널 (`.panel`) — 캔버스와 같은 높이를 갖는다(`height: fill`).
    pub fn Panel() {
        <Col class="panel">{children}</Col>
    }

    /// 정보 스트립 (`.statusbar`) — 캔버스 **위**에 온다.
    ///
    /// 캔버스(`<Raw>`)가 `available_size()`를 전부 먹으므로 스트립이 뒤에 오면
    /// 0px가 된다 (docs/DESIGN-SYSTEM.md §6.3).
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

    /// 모달의 액션 행 — 오른쪽 정렬 (Bootstrap `modal-footer` 결).
    pub fn Actions() {
        <Row class="modal__actions modal__actions--end">{children}</Row>
    }

    /// 모달의 프리셋 행 — 왼쪽 정렬 (설정 창의 선택지 나열).
    pub fn Presets() {
        <Row class="modal__actions">{children}</Row>
    }
}
