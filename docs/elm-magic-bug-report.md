# elm-magic 버그 리포트 — FreeDF 채택 과정에서 발견한 3건

> **대상**: https://github.com/jaywoo0830a/elm-magic
> **리비전**: `14f11eb928913acfe7438ab87aa654f27d9d63df` (dev 브랜치, 2026-09-17 기준 최신)
> **환경**: rustc 1.98.1 (2026-09-01) / Linux x86_64 / egui 0.36.1 (`elm-magic-egui` 어댑터 사용)
> **발견 맥락**: PDF 뷰어 앱(FreeDF)의 UI를 `view!` 컴포넌트로 재작성하는 중 — 모달
> 다이얼로그 3종과 앱 셸(툴바·사이드바·탭 스트립·모달 3종)을 실제로 구현하며 발견.
> 3건 모두 우회법으로 동작 중이며, 아래 재현 코드는 실제 앱 코드에서 축약한 것입니다.

---

## 버그 1 — `view!`의 `pub fn`이 `pub #[derive(...)]`를 생성해 컴파일 실패

### 재현

```rust
elm_magic::view! {
    pub fn Badge(count = 0) {
        <Row>"count: {count}"</Row>
    }
}
```

### 실제 생성 코드 (ELM_MAGIC_DUMP=1)

```rust
pub #[derive(::core::clone::Clone)]      // ← vis가 derive **앞에** 찍힘
pub struct BadgeProps { ... }
```

rustc 에러:

```text
error: visibility `pub` is not followed by an item
error: macro expansion ignores `#` and any tokens following
```

### 원인 추정

`crates/elm-magic-macros/src/view.rs`의 head 생성부:

```rust
head.push_str(&format!(
    "{v}#[derive(::core::clone::Clone)]\n{v}struct {n}Props {{\n",  // v = "pub "
    ...
));
```

`vis`가 derive 속성과 `struct` 양쪽에 각각 찍히면서 첫 줄이 `pub #[derive(...)]`가 됩니다.

### 부가 정보

- `pub(crate) fn`도 동일합니다 — vis 파싱이 `pub` 다음의 `(crate)` 그룹을 건너뛰고
  `vis = "pub "`으로만 남기 때문에, **`pub(crate)` 의미가 보존되지도 않습니다**.
- 라이브러리 자체 테스트는 모두 비-`pub` fn이라 미발견으로 보입니다.

### 기대

```rust
#[derive(Clone)]
pub struct BadgeProps { ... }
```

### 우회법

컴포넌트는 모듈 프라이빗으로 두고, 같은 모듈에 진입 함수를 하나 노출:

```rust
elm_magic::view! { fn Badge(count = 0) { ... } }   // pub 없음

pub(crate) fn render_badge(ui: &mut egui::Ui, ctx: &mut elm_magic::Ctx) {
    let tree = elm_magic::frame::<Badge>(ctx, &BadgeProps::default());
    elm_magic_egui::render(ui, &tree, &mut ctx.arena);
}
```

---

## 버그 2 — `remove(x)` 특수 폼 뒤에 오는 문장이 `,`로 이어져 생성됨

### 재현

```rust
elm_magic::view! {
    fn T(items: Vec<String> = vec![], n = 0, done = false) {
        <Button on_click={items.remove(String::from("a")), n = 1, done = true}>"go"</Button>
    }
}
```

### 실제 생성 코드 (ELM_MAGIC_DUMP=1)

```rust
move | _elm_a : & mut :: elm_magic :: Arena | {
    { let __elm_item = (...); __elm_state_items . mutate (_elm_a , | __v |
        { __v . retain (| __x | * __x != __elm_item ) ; } ) ; }
    ,                                     // ← 문장 사이가 `,`
    { let __elm_rhs = (1 ) ; __elm_state_n . set (...) ; } ;
    { let __elm_rhs = (true ) ; ... }
}
```

rustc 에러:

```text
error[E0XXX]: expected expression, found `,`
```

블록 문장 다음의 `,`는 유효하지 않습니다.

### 관찰된 규칙 (같은 다중문이라도 결과가 다름)

| 핸들러 첫 문장 | 이어지는 구분자 |
|---|---|
| 대입 (`n = 1, done = true`) | `;` — 정상 |
| `items.push(arg)` (일반 메서드+인자) | `;` — 정상 |
| **`items.remove(x)` (retain 특수 폼)** | **`,` — 컴파일 실패** |

참고로 2문장이든 3문장이든 remove가 **첫 문장이면** 실패하고, 대입 문장들과의
조합/순서와 무관하게 remove **직후**의 구분자만 `,`로 나옵니다.

### 우회법

`remove(x)`를 핸들러의 **마지막** 문장으로 배치:

```rust
<Button on_click={n = 1, done = true, items.remove(String::from("a"))}>"go"</Button>
```

---

## 버그 3 — `{if ...}` 식 내부의 `.iter().map()`이 소유 반복으로 재작성되지 않음 (E0716)

### 재현

```rust
elm_magic::view! {
    fn S(open = true, status = String::new()) {
        let sections = ["Notes", "PDFs"];          // 렌더 본문의 지역 배열
        <Col>
            {if open {
                {sections.iter().map(|s| <Row on_click={status = format!("{}", s)}>"{s}"</Row>)}
            } else {
                <Text>""</Text>
            }}
        </Col>
    }
}
```

### 에러

```text
error[E0716]: temporary value dropped while borrowed
    = coercion requires that borrow lasts for `'static`
```

### 분석

- 핸들러 클로저는 `Rc<dyn Fn ... + 'static>`으로 승격되므로 캡처는 'static이어야
  하는데, 이 핸들러가 캡처하는 `s`가 **렌더 본문 지역 배열 `sections`를 빌리고**
  있습니다 (`sections.iter()`가 소유 반복으로 재작성되지 않은 채 통과).
- `tests/syntax.rs`의 `iter_sugar_yields_owned_items`(문장 레벨, 핸들러 없음)는
  통과하지만 그 테스트에는 핸들러 캡처가 없어 빌림 여부를 구분할 수 없습니다.
  실측으로는 **식 레벨**(`{if ...}` 분기 내부)에서는 빌림이 그대로 남습니다.

### 우회법

지역 컬렉션은 `.into_iter()`를 명시:

```rust
{if open {
    {sections.into_iter().map(|s| <Row on_click={status = format!("{}", s.clone())}>"{s}"</Row>)}
} else {
    <Text>""</Text>
}}
```

(또는 상태 슬롯의 `.map` 슈가를 쓰거나, 아예 `if` 밖으로 리스트 렌더를 꺼내 문장
레벨로 유지.)

---

## 함께 보면 좋은 정보

- 위 3건 모두 `ELM_MAGIC_DUMP=1`로 생성 코드를 확인해 근거를 남겨 두었습니다.
- FreeDF 쪽 우회법과 발견 맥락은 `FreeDF/CHANGELOG.md`의 "[Unreleased]" 섹션과
  `docs/freedf-gui-migration.md`에도 정리되어 있습니다.
- 우회 후에도 라이브러리는 실제 앱(PDF 뷰어 셸: 툴바/사이드바/탭/모달/리스트)에서
  잘 동작하고 있습니다. `pub fn` 지원과 문장 구분자 정리만 되면 크게 개선될 것 같습니다.

감사합니다 — 필요하면 재현 크레이트를 통째로 제공할 수 있습니다.
