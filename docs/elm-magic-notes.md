# elm-magic 채택 노트 — 0.7.4

대상: `elm-magic 0.7.4` / `elm-magic-macros 0.7.4` / `elm-magic-egui 0.7.4`.

freedf-gui는 UI를 전부 `elm_magic::view!` + `elm_magic::css!`로 그린다. 이 문서는
**지금 유효한 사용 계약**과, 0.7.0 → 0.7.4에서 우리가 지운 우회의 이력만 남긴다
(현재 열린 elm-magic 버그 없음). elm-magic 쪽 회귀 테스트는
<https://github.com/jaywoo0830a/elm-magic/tree/dev/tests> (`tests/bug_report.rs`)에 있다.

---

## 1. 사용 계약 (0.7.4 기준, 실측)

### `view!`

- **한 블록에 컴포넌트 여러 개**를 정의한다 (`ui/atoms.rs`, `ui/layout.rs`).
- 문서 주석(`///`)은 **블록 안**에 쓴다 — 호출부에 단 `///`는 rustc가 매크로 호출
  속성으로 두어 전달되지 않는다(`unused doc comment`).
- 콜백 prop은 `fn()`으로 선언하고 본문에서 `cb()`로 호출한다.
- `format!`과 보간 리터럴(`text="… {x}"`)은 자식/속성 위치 어디서나 그대로 쓴다.
- **값 prop은 살아 있는 prop이다** — 부모가 넘긴 새 값이 재렌더에서 반영된다.
  자식이 그 매개변수를 직접 쓰면 그때부터는 자식의 상태다(`Arena::slot_dirty`).
  freedf-gui의 컴포넌트는 값을 표시만 하므로 `key=` 우회가 필요 없다.
- `key=`는 이제 **목록 재정렬/상태 보존** 용도로만 쓴다.
- 매크로 위치의 `format!` 주의(실측): 인라인 캡처(`format!("{i}…")`)는 E0716을 낼 수
  있으니 **위치 인자**(`format!("{}…", i)`)로 쓰고, `key` 식 안에서는 상태 슬롯을
  직접 읽지 않는다(`format!("…{slot}")` → E0425).

### `css!`

- 셀렉터는 **BEM 클래스뿐**이다(태그 셀렉터 금지) — `tests/style_tests.rs`의
  `selectors_use_bem_classes_only`가 강제한다.
- 시작 자기등록은 **플랫폼별 섹션**으로 돈다: MSVC `.CRT$XCU`,
  Apple `__DATA,__mod_init_func`, ELF `.init_array`. 여기에 더해 `main()`이
  `elm_magic::style::init_styles()`를 한 번 부른다(멱등 — 섹션이 안 도는 환경의 안전판).
- `Button`/`Tab`도 CSS `padding`/`height`/`min-height`를 그대로 반영한다.
  클릭 대상은 `min-height: 24`(감사 `small_targets` 기준).
- 등록 진단: `FREEDF_GUI_STYLE_DIAG=1`로 실행하면 등록된 셀렉터 수를 찍는다
  (기대값 **41**, 0이면 CSS가 하나도 적용되지 않는다는 뜻).


## 2. 이력 — 고쳐져서 **지운** 우회 (0.7.2 / 0.7.4)

| # | 증상 (0.7.0–0.7.1) | 고친 버전 | 지운 우회 |
|---|---|---|---|
| 1 | 한 `view!`의 두 번째 `fn`이 조용히 사라짐 | 0.7.2 | 컴포넌트마다 `view!` 분리 |
| 2 | 첫 `fn` 앞 `///` → proc macro 패닉 | 0.7.2 | 주석을 블록 밖으로 |
| 3 | `view!` 호출부 `///` → doc 사라짐 + 경고 | 0.7.2 | `#![allow(unused_doc_comments)]` |
| 4 | `fn()` 콜백 prop → E0061 | 0.7.2 | `fn(bool)` + 더미 인자 |
| 5 | `format!` 포맷 리터럴이 `Text`로 치환 | 0.7.2 | `format!`을 `view!` 밖으로 |
| 6 | prop 보간 리터럴이 **조용히** 무시 | 0.7.2 | 호출부에서 문자열 완성 |
| 11 | 자식 값 prop이 재렌더에서 갱신 안 됨 | 0.7.4 | `key={…}` (22곳) + `ui::state_key` |
| 12 | `Button`/`Tab`이 `padding`·`height` 무시 | 0.7.4 | `height: 24` 하드코딩 |
| 13 | `css!` 자기등록이 MSVC/Mach-O에서 안 돎 | 0.7.4 | (진단만 두고 `init_styles()` 안전판) |

버그 11·12·13의 재현·수정 과정은 elm-magic의 `tests/bug_report.rs`와
`crates/elm-magic-egui/tests/adapter.rs`에 회귀 테스트로 남아 있다.

## 3. 검증 (freedf-gui 쪽)

- `cargo test -p freedf-gui` — 66개 (셸/캔버스/펜/커서/팔레트/스타일).
  `shell_tests::tab_click_selects`와 `settings_modal_selects_smoothing`이 **값 prop
  회귀 지점**이다 (`key=` 없이 통과해야 한다).
- 시각/레이아웃 감사:
  `scripts/edev-run.sh --config .edev-gui.toml eval scripts/design-audit.luau --out-dir tmp/out`
  — `layout_issues` 0건, 계약 id(`gui.*`, `canvas.surface`) 전부 등록, `small_targets`
  0건(버튼/탭 `min-height`), `contrast` AA 통과.
- 감사 수치 읽기 주의: `contrast`는 렌더된 픽셀 샘플이라 안티에일리어싱 때문에 이론값보다
  낮게 나온다. 흰 글자 on `#0d6efd`(`.btn--on`/`.swatch--on`)는 이론 4.5:1, 실측
  4.3–4.47:1로 AA 경계선이다 — Bootstrap `btn-primary`와 같은 값이고, 더 어둡게 하려면
  `Token::Primary`를 낮춰야 하는데 그러면 navbar 브랜드 글자 대비가 함께 떨어진다.
