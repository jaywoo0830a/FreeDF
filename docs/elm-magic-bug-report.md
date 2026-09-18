# elm-magic 버그 리포트

FreeDF가 elm-magic을 쓰면서 재현한 결함들. 각 항목은 **최소 재현 → 기대 → 실제 →
원인(추정) → 제안** 순서로 적어, 업스트림이 그대로 회귀 테스트로 옮길 수 있게 합니다.
수정이 나오면 이 파일에서 지웁니다.

---

## [0.6.0] `Row`가 자식 높이로 줄어들지 않는다 — `Divider` 하나가 행을 가용 높이로 부풀림

### 최소 재현

```rust
// elm-magic-egui 0.6.0 — 창 세로 720px, 루트는 세로 Col.
elm_magic::view! {
    fn Demo() {
        <Col>
            <Row>
                <Text>"hello"</Text>
                <Divider />          // ← 이 한 줄이 원인
            </Row>
            <Row><Text>"row 2"</Text></Row>
        </Col>
    }
}
```

실제 재현: freedf-gui 셸(`crates/freedf-gui/src/shell.rs`) — 툴바 `Row`에 `<Divider/>`가
3개 있는데, 그 행이 창 높이(720px)만큼 커져 아래 행들이 화면 밖으로 밀려납니다.

### 기대

`<Row>`는 자식들의 자연 높이로 줄어든다(flex row). `Divider`는 그 행 높이에 맞는
세로 구분선이 된다.

### 실제

- `Row` 높이 = **가용 높이 전체**(704px). 첫 행이 세로를 다 먹고, 리본 행은 y=742
  (= 창 밖), 캔버스(`<Raw>`)는 `available_size()`가 0이라 높이 0으로 그려진다.
  실측(`edev dump`, 창 1100x720):

  ```
  freedf-gui.root unknown [0,0 1125.5x969]
    canvas.surface unknown [8,937 1109.5x0] !visible
    gui.sidebar button "Sidebar" [80.5,17 53.2x18]
    gui.fountain button "Fountain" [84.9,742 58.4x18]   ← 창 밖
  ```
- 같은 셸에서 `<Divider/>`를 행에서 **전부 제거**하면 정상으로 돌아온다:

  ```
  freedf-gui.root unknown [0,0 1100x744]
    canvas.surface unknown [8,241 1084x471]              ← 정상 크기
    gui.sidebar button "Sidebar" [66.5,17 53.2x18]
    gui.fountain button "Fountain" [84.9,60 58.4x18]      ← 정상 위치
  ```

### 원인 (추정)

0.5.0은 컨테이너를 `ui.horizontal(..)` / `ui.vertical(..)`로 그렸습니다. egui의
`Ui::horizontal`은

```rust
let initial_size = vec2(self.available_size_before_wrap().x, self.spacing().interact_size.y);
self.allocate_ui_with_layout_dyn(initial_size, layout, add_contents)
```

로 **자식 Ui의 `max_rect` 높이를 `interact_size.y`로 제한**합니다 — 그래서 행이 자식
높이로 줄어들었습니다.

0.6.0의 `container()`는 배경/여백/라운드 스타일을 위해
`frame_of(style).show(ui, |ui| { … ui.with_layout(layout_of(style, vertical), …) })`
경로로 바뀌었는데, `Frame::show`의 자식 Ui는 **가용 높이 전체**를 `max_rect`로 받습니다.
`ui.separator()`는 수평 레이아웃에서

```rust
vec2(self.spacing().indent, self.available_size_before_wrap().y - extra.y)
```

높이로 할당되므로, 구분선 하나가 행을 가용 높이까지 부풀립니다
(`crates/elm-magic-egui/src/lib.rs`: `Element::Divider` → `ui.separator()`,
`container()`).

### 제안

`container()`가 자식 Ui를 그릴 때 높이를 0.5처럼 **명시적으로 제한**해 주세요:

- `height`/`min-height`/`max-height`가 선언되지 않은 자식 스코프는 교차축을
  `spacing().interact_size.y`로 시작하는 `allocate_ui_with_layout`로 만들거나,
- `height: auto`(= `Len` 미지정)일 때 "내용 크기" 스코프를 쓴다 — `Frame`의
  배경/여백/라운드는 그대로 유지하면서.

회귀 테스트 제안: `Divider`를 넣은 `Row`의 rect 높이가 창 높이보다 작고, 다음 행이
그 아래에 붙는지(어댑터 테스트는 이미 `Pass::styles`로 rect를 볼 수 있습니다).

### 영향

`Row` 안에 `Divider`를 쓰는 모든 화면이 세로 레이아웃을 잃습니다. freedf-gui는 현재
행에서 `<Divider/>`를 전부 제거하는 방식으로 우회하고 있습니다(그 외 CSS는 0.6 기능
그대로 사용).

### 재현 환경

elm-magic 0.6.0 / elm-magic-egui 0.6.0 (crates.io 발행본) · egui 0.36 ·
freedf-gui `--features dev-automation` 빌드 · `scripts/edev-run.sh --config .edev-gui.toml
dump`로 rect 실측 (2026-09-17).

---

## [0.6.0] BEM 수정자(`--`) 셀렉터가 조용히 미등록된다 — `-`가 별도 토큰으로 쪼개져 조인됨

### 최소 재현

```rust
elm_magic::css! {
    .tabs__item { color: text_dim; }
    .tabs__item--active { color: text; }   // ← 등록되지 않는다
}

#[test]
fn modifier_is_registered() {
    assert!(elm_magic::style::lookup_class("tabs__item--active").is_some());
}
```

실제 재현: freedf-gui가 BEM 네이밍으로 전환하면서 `.toolbar__button`,
`.panel__item` 같은 요소 셀렉터는 정상 등록됐지만, **수정자**
(`.tabs__item--active`, `.modal__actions--end`)만 `lookup_class`가 `None`을
돌려줬습니다. 가드 테스트
(`crates/freedf-gui/src/style.rs::shell_markup_classes_are_registered`)가 검출.

### 기대

`css!`의 다른 셀렉터처럼 `.tabs__item--active`도 등록되고 `lookup`/`resolve`가
찾는다.

### 실제

- `css!` 매크로의 `join_selector()`는 식별자(또는 `*`) **사이에만** 공백을
  넣는데, `word()`가 `-` 하나도 "단어"로 판정한다. 그래서
  `.tabs__item--active`가 `.tabs__item - - active`로 조인된다.
- 이 문자열은 코어 `Selector::parse`에서 실패한다(`-`는 클래스 이름도 태그도
  아니다). 등록 루틴은 파싱 실패 셀렉터를 **조용히 건너뛴다** — 컴파일 에러도
  경고도 없다. `validate_selector()`도 이 형태는 잡지 못한다.

### 제안

- `join_selector()`에서 `-`만으로 이루어진 조각을 단어로 보지 않게 한다
  (`-` 뒤에 식별자가 붙어 있으면 이어 붙인다).
- 또는 등록 시 파싱 실패를 **컴파일 에러**로 승격해 주세요 — 지금은 오타든
  문법 한계든 결과가 같습니다("스타일이 안 먹는다").

### 우회

freedf-gui는 그 두 규칙만 **문자열 셀렉터** 형태로 씁니다 — 문자열은 공백까지
그대로 쓰이므로 정상 등록됩니다.

```rust
".tabs__item--active" { color: text; }
```

### 재현 환경

elm-magic 0.6.0 / elm-magic-macros 0.6.0 (crates.io 발행본) · egui 0.36 ·
freedf-gui `src/style.rs` (2026-09-17).
