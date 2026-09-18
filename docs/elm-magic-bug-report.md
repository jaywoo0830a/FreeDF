# elm-magic 버그 리포트

대상: `elm-magic 0.7.0` / `elm-magic-macros 0.7.0` (rustc 1.98.1).
아래 스니펫은 모두 실제 컴파일/실행으로 확인한 최소 재현입니다.
0.7.2에서 수정 예정.

---

## 1. `view!` 하나에 컴포넌트 하나 — 두 번째 `fn`은 **조용히 사라진다**

```rust
elm_magic::view! {
    pub fn Alpha(text: String = String::new()) {
        <Text class="a">"{text}"</Text>
    }

    pub fn Beta(text: String = String::new()) {
        <Text class="b">"{text}"</Text>
    }
}

#[test]
fn t() {
    let _ = AlphaProps { text: None, children: None }; // OK
    let _ = BetaProps  { text: None, children: None }; // ?
}
```

- 기대: `Alpha`, `Beta` 둘 다 생성. (여러 컴포넌트를 한 블록에 쓰면 자연스럽다.)
- 실제: **경고 없이 통과하는 것처럼 보이고**, `Beta`는 생성되지 않는다. 참조하는
  순간에야 원인과 무관한 지점에서 에러:

  ```
  error[E0422]: cannot find struct, variant or union type `BetaProps` in this scope
  ```

- 원인 지점: `view.rs`의 `expand()`가 첫 `fn` 4토큰만 읽고 **뒤 토큰을 버린다**
  (`expand_fn(fn_name, vis, params, body)` 반환 후 나머지 미처리).
- 이 저장소의 회피: 컴포넌트마다 `elm_magic::view! { ... }`를 따로 호출.

## 2. 첫 `fn` 앞의 `///` 문서 주석 → proc macro 패닉

```rust
elm_magic::view! {
    /// 문서 주석이 첫 `fn` 앞에 있으면?
    pub fn Gamma(text: String = String::new()) {
        <Text class="g">"{text}"</Text>
    }
}
```

- 기대: 주석은 무시되거나 생성된 항목의 doc이 된다.
- 실제:

  ```
  error: proc macro panicked
    = help: message: elm-magic: view! expects `fn Name(params) { ... }`
  ```

  (`view.rs`의 inner-attribute 스킵 루프가 문서 주석을 건너뛰지 못한다.)

## 3. `view!` 호출 자체에 단 `///` → 문서가 사라지고 경고

```rust
/// 클릭 버튼 — 아이콘 + 라벨.
elm_magic::view! {
    pub fn Btn(text: String = String::new()) {
        <Text class="btn">"{text}"</Text>
    }
}
```

- 기대: `Btn`/`BtnProps`의 rustdoc에 반영.
- 실제:

  ```
  warning: unused doc comment
    = help: rustdoc does not generate documentation for macro invocations
    = help: to document an item produced by a macro, the macro must produce
            the documentation as part of its expansion
  ```

  즉 `view!`을 쓰는 한 컴포넌트에 문서를 붙일 방법이 없다.

## 4. 콜백 prop `fn()` → E0061 (인자 누락)

```rust
elm_magic::view! {
    pub fn CbZero(on_click: fn()) {
        <Button class="b" on_click={on_click()}>"z"</Button>
    }
}
```

- 기대: 인자 없는 콜백이 그대로 호출된다.
- 실제:

  ```
  error[E0061]: this method takes 2 arguments but 1 argument was supplied
    = note: argument #2 of type `()` is missing
    note: method defined here
      elm-magic-0.7.0/src/element.rs:123
      pub fn call(&self, arena: &mut Arena, value: T)
  ```

  `fn()` prop은 `Callback<()>`로 선언되지만 본문 호출은 `cb.call(arena, )`로 펼쳐진다
  (`Callback::call`은 항상 `value`를 요구).
- 이 저장소의 회피: `fn(bool)`로 선언하고 `on_click(false)` 더미 값을 넘긴다.

## 5. `format!(...)`의 **포맷 문자열 리터럴**이 `Text` 요소로 치환된다

```rust
// 자식 위치
elm_magic::view! {
    pub fn E2(v: u32 = 0) {
        <Text class="t">{format!("v {}", v)}</Text>
    }
}

// prop 위치
elm_magic::view! {
    pub fn E4(tool: String = String::new()) {
        <P text={format!("도구 {}", tool)} />
    }
}
```

- 기대: 그냥 `format!`이 실행된다.
- 실제 (두 경우 모두):

  ```
  error: format argument must be a string literal
    = note: this error originates in the macro `elm_magic::view`
  help: you might be missing a string literal to format with
  ```

  힌트가 보여주는 인자 개수(`"{} {}"`)가 `format!`의 원래 인자 수와 일치한다 —
  즉 리터럴 `"v {}"`가 `Element::Text(...)`로 바뀌어 `format!(<Element>, v)`가 됐다.
- 원인 지점: `jsx.rs:909` `transform_render()` — **표현식 위치의 문자열 리터럴**에
  `{`가 있으면 무조건 `text_expr()`로 `Text` 요소를 만든다. 중첩된 `format!`의
  포맷 문자열도 예외가 아니다.
- 이 저장소의 회피: `format!`을 `view!` 밖으로 빼고 보간 리터럴(`"{x}"`)을 쓴다.

## 6. prop/속성 위치의 보간 리터럴은 **보간되지 않는다** (조용한 리터럴)

```rust
elm_magic::view! { pub fn P(text: String = String::new()) { <Text class="p">"{text}"</Text> } }

elm_magic::view! {
    pub fn E3(tool: String = String::new()) {
        <P text="도구 {tool} · 끝" />   // prop
    }
}

elm_magic::view! {
    pub fn E5(tool: String = String::new()) {
        <Col class="c"><Text class="t">"도구 {tool} · 끝"</Text></Col>  // 자식 텍스트
    }
}

#[test]
fn t() {
    let app = elm_magic::mount!(E3, E3Props { tool: Some("Pen".into()), children: None });
    println!("{}", app.render_tree());
    let app = elm_magic::mount!(E5, E5Props { tool: Some("Pen".into()), children: None });
    println!("{}", app.render_tree());
}
```

- 기대: 두 위치 모두 `도구 Pen · 끝`.
- 실제:

  ```
  --- E3 (prop) ---
  Text "도구 {tool} · 끝" .p      ← 보간 안 됨 (컴파일 경고도 없음)

  --- E5 (자식) ---
  Col .c
    Text "도구 Pen · 끝" .t       ← 보간 됨
  ```

- 원인 지점: 자식 텍스트는 `text_from_children()`(`jsx.rs:1879`)이 `{...}`를 분해하지만,
  prop/속성 값은 `attr_string()`(`jsx.rs:1816`) / 컴포넌트 prop(`jsx.rs:2166`)이
  `String::from("...")`으로 그대로 넘긴다.
- 이 저장소의 회피: prop에는 완성된 문자열(호출부에서 `format!`)을 넘긴다.

---

### 요약

| # | 증상 | 표면화 시점 | 회피 |
|---|---|---|---|
| 1 | 한 `view!`의 두 번째 `fn` 소실 | 사용 지점 E0422 | 컴포넌트마다 `view!` 분리 |
| 2 | 첫 `fn` 앞 `///` | 매크로 패닉 | 주석을 `view!` 밖으로 |
| 3 | `view!`에 단 `///` | 경고 + doc 없음 | 주석을 `view!` 밖으로 |
| 4 | `fn()` 콜백 prop | E0061 | `fn(bool)` + 더미 인자 |
| 5 | `format!` 포맷 리터럴 치환 | "format argument must be a string literal" | `format!`을 `view!` 밖으로 |
| 6 | prop 보간 리터럴 | **조용히** 리터럴 렌더 | 호출부에서 문자열 완성 |

