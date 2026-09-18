//! atoms — 셸의 최소 컴포넌트. **태그는 의미만** 갖고 스타일은 전부
//! `style.rs`(BEM 클래스 CSS)가 가져간다.
//!
//! 컴포넌트는 두 가지만 책임진다:
//! 1. 의미 태그(`Button` `Strong` `Text` `Row` `Tab` `Modal` …) 선택
//! 2. 아이콘 + 라벨 문자열 조합(`crate::ui::label`)과 상태 → 클래스 수정자 매핑
//!
//! 폭/여백/색/글자크기는 여기서 정하지 않는다 — 클래스 이름만 붙이고 CSS가 결정한다.
//!
//! ## elm-magic 매크로 주의 (실측)
//!
//! - `view!` 한 번에는 **컴포넌트 하나만** 정의된다 (여러 개를 넣으면 "expects
//!   `fn Name(params)`"로 패닉). 그래서 컴포넌트마다 호출을 나눴다.
//! - 콜백 prop은 `fn()`으로 선언하면 본문 호출이 `Callback<()>::call(arena)`로
//!   펼쳐져 E0061이 난다 → **인자 하나짜리**(`fn(bool)`)로 선언하고 더미 값을 넘긴다.
//! - 속성에 `format!(…)`을 직접 쓰면 문자열 리터럴이 한 번 더 감싸여
//!   "format argument must be a string literal"이 된다 → 보간 리터럴(`"{x}"`)을 쓴다.

/// 클릭 버튼 — 아이콘 + 라벨, 클래스는 `.btn` 하나.
elm_magic::view! {
    pub fn Btn(text: String = String::new(), on_click: fn(bool)) {
        let l = crate::ui::label(&text);
        <Button class="btn" on_click={on_click(false)}>"{l}"</Button>
    }
}

/// 눌림/선택 상태가 있는 버튼 — 켜짐은 `--on` 수정자.
///
/// 선택 상태에서도 같은 `Button` 태그를 유지한다: 계약 id(`gui.pen` 등)가 항상
/// 등록되고 클릭 대상도 사라지지 않는다 (예전엔 활성 항목만 `<Strong>`이었다).
elm_magic::view! {
    pub fn BtnOn(text: String = String::new(), on: bool = false, on_click: fn(bool)) {
        let l = crate::ui::label(&text);
        <Button class={if on { "btn btn--on" } else { "btn" }} on_click={on_click(false)}>"{l}"</Button>
    }
}

/// 색 스와치 — 팔레트 칩 (선택 상태는 `--on`). 색 자체는 캔버스가 소유한다.
elm_magic::view! {
    pub fn Swatch(text: String = String::new(), on: bool = false, on_click: fn(bool)) {
        let l = crate::ui::label(&text);
        <Button class={if on { "swatch swatch--on" } else { "swatch" }} on_click={on_click(false)}>"{l}"</Button>
    }
}

/// 섹션 제목 — 아이콘 + 제목 (`.section__title`).
elm_magic::view! {
    pub fn Section(text: String = String::new()) {
        let l = crate::ui::label(&text);
        <Strong class="section__title">"{l}"</Strong>
    }
}

/// 본문 한 줄 (`.text`).
elm_magic::view! {
    pub fn Note(text: String = String::new()) {
        <Text class="text">"{text}"</Text>
    }
}

/// 패널의 클릭 가능한 행 — 라벨(아이콘) + 행 전체가 클릭 영역.
elm_magic::view! {
    pub fn PanelRow(text: String = String::new(), on_click: fn(bool)) {
        let l = crate::ui::label(&text);
        <Row class="panel__row" on_click={on_click(false)}>
            <Text class="panel__item">"{l}"</Text>
        </Row>
    }
}

/// 패널의 빈 상태 안내문.
elm_magic::view! {
    pub fn Empty(text: String = String::new()) {
        <Text class="panel__empty">"{text}"</Text>
    }
}

/// 탭 하나 — 활성은 클래스 수정자 + `active` 상태 둘 다 반영.
elm_magic::view! {
    pub fn TabItem(text: String = String::new(), active: bool = false, on_click: fn(bool)) {
        let l = crate::ui::label(&text);
        <Tab class={if active { "tabs__item tabs__item--active" } else { "tabs__item" }} active={active} on_click={on_click(false)}>"{l}"</Tab>
    }
}
