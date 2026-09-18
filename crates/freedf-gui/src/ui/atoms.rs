//! atoms — 셸의 최소 컴포넌트. **태그는 의미만** 갖고 스타일은 전부
//! `style.rs`(BEM 클래스 CSS)가 가져간다.
//!
//! 컴포넌트는 세 가지만 책임진다:
//! 1. 의미 태그(`Button` `Strong` `Text` `Row` `Tab` `Modal` …) 선택
//! 2. 아이콘 + 라벨 문자열 조합(`crate::ui::label`)과 상태 → BEM 수정자 매핑
//! 3. 콜백 prop을 태그의 핸들러로 연결
//!
//! 폭/여백/색/글자크기는 여기서 정하지 않는다 — 클래스 이름만 붙이고 CSS가 결정한다.
//!
//! ## elm-magic 0.7.2 계약 (실측)
//!
//! - 한 `view!` 블록에 컴포넌트를 **여러 개** 정의한다 (0.7.0의 \"두 번째 `fn` 소실\"
//!   결함은 0.7.2에서 수정). `///` 문서는 블록 **안**에서 생성 항목의 rustdoc이 된다
//!   (호출부에 단 `///`는 rustc가 매크로 호출 속성으로 두므로 전달되지 않는다).
//! - 콜백 prop은 `fn()`으로 선언하고 본문에서 `cb()`로 호출한다 (0.7.2에서 E0061 수정 —
//!   `fn(bool)` + 더미 인자 우회는 더 이상 필요 없다).
//! - `format!`은 자식/속성 위치 어디서나 그대로 쓴다 (0.7.2에서 포맷 리터럴 치환 수정).
//!   속성의 보간 리터럴(`text=\"값 {x}\"`)도 0.7.2부터 실제로 보간된다.
//!
//! ## elm-magic 버그 11 (미수정) — 값 prop은 재렌더에서 갱신되지 않는다
//!
//! 자식 컴포넌트의 파라미터는 **마운트 시점의 상태 슬롯**으로만 초기화되어, 부모가
//! 같은 자리에 새 값을 넘겨도 화면은 첫 렌더 값에 머문다 (`text`/`on`/`active` 같은
//! 모든 값 prop). 콜백 prop(`fn(..)`)과 `{children}`은 정상이라 두 경로로 우회할 수
//! 있고, 값 prop을 써야 하는 자리는 호출부에서 `key={..}`를 함께 넘겨 인스턴스를
//! 갱신시킨다 (`shell.rs`의 `key=` 주석 참고).
//!
//! 회귀 기록: `tests/shell_tests.rs::tab_click_selects` ·
//! `settings_modal_selects_smoothing` — 이 버그가 작용하는 지점이다.

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
}
