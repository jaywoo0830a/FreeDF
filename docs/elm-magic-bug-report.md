# elm-magic 채택 기록 — 0.7.2 반영과 남은 이슈

대상: `elm-magic 0.7.2` / `elm-magic-macros 0.7.2`.

이 문서는 **두 부분**만 남긴다:
1. 0.7.2에서 고쳐져 freedf-gui가 정식 사용법으로 되돌린 것 (우회 삭제)
2. 아직 남은 버그와 freedf-gui의 우회 (`key=`)

elm-magic 쪽 회귀 테스트는 <https://github.com/jaywoo0830a/elm-magic/tree/dev/tests>
(`tests/bug_report.rs`)에 있다 — 리포트가 수정되면 리포트 파일은 지우고 테스트만
남기는 방식이다. freedf-gui 쪽 회귀는 `crates/freedf-gui/tests/`가 갖는다.

---

## 1. 0.7.2에서 해결 — 우회 코드 삭제

| # | 0.7.0/0.7.1 증상 | 0.7.2 이후 freedf-gui |
|---|---|---|
| 1 | 한 `view!`의 두 번째 `fn`이 경고 없이 사라짐 | `ui/atoms.rs`·`ui/layout.rs`가 **한 블록에 전 컴포넌트**를 정의 |
| 2 | 첫 `fn` 앞 `///` → proc macro 패닉 | 컴포넌트마다 `///` 문서를 블록 안에 작성 |
| 3 | `view!` 호출부에 단 `///` → `unused doc comment` | 문서를 **블록 안**으로 (호출부에는 쓰지 않음) → `lib.rs`의 `#![allow(unused_doc_comments)]` 삭제 |
| 4 | `fn()` 콜백 prop → E0061 | 모든 콜백 prop이 `fn()` + 본문 `cb()` 호출 (더미 인자 없음) |
| 5 | `format!`의 포맷 리터럴이 `Text`로 치환 | `format!`을 자식/속성 위치에서 그대로 사용 |
| 6 | prop/속성의 보간 리터럴이 조용히 무시 | `text="… {x}"`를 그대로 씀 (호출부에서 문자열을 완성하지 않음) |

주의 — 0.7.2에서도 **호출부에 단 `///`는 여전히 전달되지 않는다** (함수형 매크로의
한계). 문서는 반드시 `view!` 블록 **안**에 쓴다.

## 2. 남은 버그 — 값 prop은 재렌더에서 갱신되지 않는다 ("버그 11")

자식 컴포넌트의 **값 prop**(`text`/`on`/`active`)은 마운트 시점의 상태 슬롯으로만
초기화되어, 부모가 같은 자리에 새 값을 넘겨도 화면은 첫 값에 머문다. 콜백
prop(`fn(..)`)과 `{children}`은 정상이라 그 두 경로는 영향이 없다.

최소 재현 (freedf-gui에서 실측):

```rust
elm_magic::view! {
    fn Child(on: bool = false) {
        <Button class={if on { "btn btn--on" } else { "btn" }}>"child"</Button>
    }
    fn Parent(flag = false) {
        <Col>
            <Child on={flag} />
            <Button on_click={flag = !flag}>"toggle"</Button>
        </Col>
    }
}
// toggle을 눌러 flag=true가 되어도 `child`는 `.btn` 그대로 — `--on`이 붙지 않는다.
```

### freedf-gui의 우회 — `key=`

키가 바뀌면 슬롯이 새로 만들어져 값이 따라온다 (keyed 슬롯의 원래 용도).

- 키는 **값과 함께 바뀌어야** 하고, 그 프레임에서 **인스턴스마다 유일**해야 한다.
  같은 키가 둘이면 elm-magic이 슬롯 경로를 공유해 **다른 위젯을 덮어쓴다**
  (실측: 사이드바 인스턴스가 여러 개로 늘고 `Fountain` 버튼이 사라졌다).
- `key={format!("Pen-{}", tool == "Pen")}` — 이름(라벨)을 접두로 붙여 유일성을 만든다.
- **상태 슬롯**은 키 식 안에서 직접 읽을 수 없다:
  `key={format!("…{sidebar_open}")}` → E0425. 본문에서 `ui::state_key(...)`로
  지역값을 만들어 `key={지역값}`으로 넘긴다.
- `format!`의 **인라인 캡처**(예: `format!("outline-{i}-{}", e.title)`)를 매크로 위치에서
  쓰면 E0716이 난다 — 위치 인자(`{}`)로 쓴다.

회귀 기록 (이 버그가 작용하는 테스트):

- `crates/freedf-gui/tests/shell_tests.rs::tab_click_selects`
- `crates/freedf-gui/tests/shell_tests.rs::settings_modal_selects_smoothing`

elm-magic이 고쳐지면 두 테스트에서 `key=`를 지워도 통과해야 한다.

## 3. 버그 12 — `Button`/`Tab`이 CSS `padding`·`height`를 무시한다

`design-audit.luau`(eguidev 캡처)의 `widgets[].rect`로 실측했다:

- `Button`: 세로 패딩을 `5` → `8`로, 가로를 `10` → `12`로 바꿔도 rect의 `w`/`h`가
  **전혀 변하지 않는다**. `height: 24`는 먹는다 (`h: 19 → 24`).
- `Tab`: `padding`도 `height`도 먹지 않는다 (`h: 19` 고정). 그래서 탭 클릭 영역만
  24px 미만으로 남고, 감사 `small_targets`가 탭 하나를 계속 보고한다.
- `Row`(패널 행)·`Col`·`Tab` 스트립 같은 컨테이너는 `padding`이 정상 적용된다.

freedf-gui의 대응: 적용되지 않는 `padding`을 CSS에 두지 않고(죽은 규칙 금지),
버튼 크기는 `height: 24`로 고정한다. 탭의 19px 클릭 영역은 이 버그가 고쳐질 때까지
남는다 — 고쳐지면 `.tabs__item { height: 26 }`을 되살리면 된다.

덧붙임: 감사 `contrast`는 **렌더된 픽셀**을 샘플링하므로 안티에일리어싱 때문에
이론값보다 낮게 나온다. `.btn--on`(흰 글자 on `#0d6efd`)은 4.46:1로 AA 경계선이고,
`.btn--danger`는 팔레트 `Token::Error`를 Bootstrap danger 강조색 `#b02a37`(≈5.9:1)로
어둡게 해 통과시켰다.
