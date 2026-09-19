//! atoms — 셸의 최소 컴포넌트. **태그는 의미만** 갖고 스타일은 전부
//! `style.rs`(BEM 클래스 CSS)가 가져간다.
//!
//! ## 라벨은 계약이다 (건드리지 말 것)
//!
//! 버튼 `text`는 eguidev 계약 id의 근거다(`gui.<라벨 슬러그>` — `shell::render_shell`).
//! `ButtonEl`에는 `id`/`tooltip` prop이 없어(elm-magic 0.7.4) **아이콘 전용 버튼이
//! 불가능**하다. 라벨 문구를 바꾸면 id가 바뀌고 스모크·문서가 깨진다 —
//! `docs/DESIGN-SYSTEM.md` §1 C1을 먼저 읽어라.
//!
//! 컴포넌트는 세 가지만 책임진다:
//! 1. 의미 태그(`Button` `Strong` `Text` `Row` `Tab` `Modal` …) 선택
//! 2. 아이콘 + 라벨 문자열 조합(`crate::ui::label`)과 상태 → BEM 수정자 매핑
//! 3. 콜백 prop을 태그의 핸들러로 연결
//!
//! 폭/여백/색/글자크기는 여기서 정하지 않는다 — 클래스 이름만 붙이고 CSS가 결정한다.
//!
//! ## elm-magic 0.7.4 계약 (실측)
//!
//! - 한 `view!` 블록에 컴포넌트를 **여러 개** 정의한다 (0.7.0의 "두 번째 `fn` 소실"
//!   결함은 0.7.2에서 수정). `///` 문서는 블록 **안**에서 생성 항목의 rustdoc이 된다
//!   (호출부에 단 `///`는 rustc가 매크로 호출 속성으로 두므로 전달되지 않는다).
//! - 콜백 prop은 `fn()`으로 선언하고 본문에서 `cb()`로 호출한다 (0.7.2에서 E0061 수정).
//! - `format!`은 자식/속성 위치 어디서나 그대로 쓴다. 속성의 보간 리터럴
//!   (`text="값 {x}"`)도 실제로 보간된다.
//! - **값 prop은 살아 있는 prop이다** — 부모가 새 값을 넘기면 재렌더에서 반영된다
//!   (0.7.4에서 수정; 그 전에는 마운트 시점 값에 고정됐다). 자식이 그 매개변수를
//!   직접 쓰면 그때부터 자식의 상태가 된다 (`Arena::slot_dirty`). 이 파일의
//!   컴포넌트는 값을 표시만 하므로 키 같은 우회가 필요 없다.
//! - CSS `padding`/`height`/`min-height`가 `Button`/`Tab`에도 반영된다 (0.7.4에서 수정).
//!   크기는 `style.rs`가 정한다 — 여기서는 클래스 이름만 붙인다.
//!
elm_magic::view! {
    /// 기본 버튼 — 아이콘 + 라벨(`.btn`, Bootstrap `btn-secondary` 결).
    ///
    /// 상태가 없는 명령 전용. 눌림/선택 표시가 필요하면 [`BtnOn`]을 쓴다.
    pub fn Btn(text: String = String::new(), on_click: fn()) {
        let l = crate::ui::label(&text);
        <Button class="btn" on_click={on_click()}>"{l}"</Button>
    }

    /// 선택 상태 버튼 — 켜짐은 BEM 수정자 `btn--on` (Bootstrap `active` 결).
    ///
    /// 선택 상태에서도 같은 `Button` 태그를 유지한다: 계약 id(`gui.pen` 등)가 항상
    /// 등록되고 클릭 대상도 사라지지 않는다.
    pub fn BtnOn(text: String = String::new(), on: bool = false, on_click: fn()) {
        let l = crate::ui::label(&text);
        <Button class={if on { "btn btn--on" } else { "btn" }} on_click={on_click()}>"{l}"</Button>
    }

    /// 파괴적 동작 버튼 — `btn--danger` (삭제/초기화 확인).
    pub fn BtnDanger(text: String = String::new(), on_click: fn()) {
        let l = crate::ui::label(&text);
        <Button class="btn btn--danger" on_click={on_click()}>"{l}"</Button>
    }

    /// 보조 버튼 — `btn--ghost` (취소/닫기처럼 주 동작이 아닌 것).
    pub fn BtnGhost(text: String = String::new(), on_click: fn()) {
        let l = crate::ui::label(&text);
        <Button class="btn btn--ghost" on_click={on_click()}>"{l}"</Button>
    }

    /// 색 스와치 — 팔레트 칩 (선택 상태는 `swatch--on`).
    ///
    /// 색 자체는 캔버스 엔진이 소유한다 — 여기서는 라벨/선택 여부만 그린다.
    pub fn Swatch(text: String = String::new(), on: bool = false, on_click: fn()) {
        let l = crate::ui::label(&text);
        <Button class={if on { "swatch swatch--on" } else { "swatch" }} on_click={on_click()}>"{l}"</Button>
    }

    /// 섹션 제목 — 작고 흐린 대문자 라벨 (`.section__title`).
    pub fn Section(text: String = String::new()) {
        let l = crate::ui::label(&text);
        <Strong class="section__title">"{l}"</Strong>
    }

    /// 모달 제목 — 본문보다 한 단계 큰 제목 (`.modal__title`).
    ///
    /// `Dialog`의 `title`은 모달 프레임의 제목 표시줄이고, 이 컴포넌트는 **본문 안의
    /// 제목**이다 (Bootstrap modal-title 결).
    pub fn Heading(text: String = String::new()) {
        <Strong class="modal__title">"{text}"</Strong>
    }

    /// 본문 한 줄 (`.text`).
    pub fn Note(text: String = String::new()) {
        <Text class="text">"{text}"</Text>
    }

    /// 패널의 클릭 가능한 행 — 행 전체가 클릭 영역 (`.panel__row`).
    pub fn PanelRow(text: String = String::new(), on_click: fn()) {
        let l = crate::ui::label(&text);
        <Row class="panel__row" on_click={on_click()}>
            <Text class="panel__item">"{l}"</Text>
        </Row>
    }

    /// 패널의 빈 상태 안내문 (`.panel__empty`).
    pub fn Empty(text: String = String::new()) {
        <Text class="panel__empty">"{text}"</Text>
    }

    /// 탭 하나 — 활성은 BEM 수정자(`tabs__item--active`)와 `active` 상태를 함께 쓴다.
    ///
    /// `active`는 접근성/자동화가 읽는 상태이고, `--active`는 시각 스타일이다.
    pub fn TabItem(text: String = String::new(), active: bool = false, on_click: fn()) {
        let l = crate::ui::label(&text);
        <Tab class={if active { "tabs__item tabs__item--active" } else { "tabs__item" }} active={active} on_click={on_click()}>"{l}"</Tab>
    }

    /// 정보 스트립의 상태 텍스트 (`.statusbar__text`) — 좌측/메타 공용.
    ///
    /// 캔버스가 남은 공간을 먹어 스트립이 캔버스 **위**에 오므로, 상태 문자열은
    /// painter가 아니라 이 트리 노드가 소유한다(자동화 `assert_text` 계약).
    pub fn StatusText(text: String = String::new()) {
        <Text class="statusbar__text">"{text}"</Text>
    }

    /// 정보 스트립의 토스트 (`.statusbar__toast`) — 3초짜리 알림 문구.
    pub fn StatusToast(text: String = String::new()) {
        <Text class="statusbar__toast">"{text}"</Text>
    }
}
