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
- **`wrap: true`는 무효다**(실측). `ResolvedStyle.wrap`은 파싱만 되고
  `elm-magic-egui` 어댑터에 사용처가 없다 — 행은 항상 **단일 줄**이고, 폭을 넘긴
  항목은 조용히 화면 밖으로 나간다(감사 `layout_issues`의 `offscreen`이 감지기).
  freedf-gui는 그래서 `wrap`을 쓰지 않고 **줄을 명시적으로 나눈다**(`.inkbar__line`).
- **우측 정렬 수단이 없다**(실측). `.…__end { justify: end }`는 콘텐츠 크기 자식
  Row에서 무효이고, `width: fill` 스페이서는 `set_max_width`가 커서 기준으로
  `max_rect`를 재설정해 **부모를 창 밖으로 팽창**시킨다(캔버스 폭 1754 > 창 1100,
  루트 rect 1241로 측정됨). 그룹은 왼쪽부터 차례로 흐르고 위계는 순서와
  `.bar__sep` 헤어라인으로 만든다.

### 1.1 최소 재현 (실측값 — `tests/elm_magic_bugs.rs`)

창 800×600 고정, 어댑터만 직접 호출(`frame` → `render_with_palette`).

| # | 재현 | 측정 | 고쳐지면 |
|---|---|---|---|
| 1 | 폭 **300** 컨테이너에 200px 버튼 3개 + `wrap: true` | 버튼 3개가 **모두 y=0**(한 줄), x = 0 / 204 / **408–608** | 2·3번이 다음 줄로 내려가고 608이 사라진다 |
| 2 | 폭 300 안의 **콘텐츠 크기** 자식 Row에 `justify: end` | 버튼이 `[0,0]–[100,20]` (x=0) | 자식이 부모 폭을 받아 x≈200에 붙는다 |
| 3 | 폭 **780**(창 800) 컨테이너에 `width: fill` + 100px 버튼 | 스페이서가 780을 전부 먹어 버튼이 **x=784**(컨테이너 밖), 루트 `max_rect` 폭 **884 > 800** | 스페이서가 680만 차지하고 루트는 800을 유지한다 |

> 3번의 두 증상은 순서가 있다: ① fill이 뒤 형제 자리를 비우지 않아 형제가 밀려나고,
> ② 밀려난 형제가 **창을 넘을 때만** 조상 `max_rect`가 팽창한다(창 안이면 팽창 없음).
> freedf-gui에서 캔버스 폭이 1754까지 간 것은 ①(여러 행의 overflow)과 ②(fill 스페이서
> 3개)가 겹친 결과다.
- 시작 자기등록은 **플랫폼별 섹션**으로 돈다: MSVC `.CRT$XCU`,
  Apple `__DATA,__mod_init_func`, ELF `.init_array`. 여기에 더해 `main()`이
  `elm_magic::style::init_styles()`를 한 번 부른다(멱등 — 섹션이 안 도는 환경의 안전판).
- `Button`/`Tab`도 CSS `padding`/`height`/`min-height`를 그대로 반영한다.
  클릭 대상은 `min-height: 28`(감사 `small_targets` 기준 24를 넘긴다).
- 등록 진단: `FREEDF_GUI_STYLE_DIAG=1`로 실행하면 등록된 셀렉터 수를 찍는다
  (기대값 **45**, 0이면 CSS가 하나도 적용되지 않는다는 뜻).


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

- `cargo test -p freedf-gui` — **78개** (셸/캔버스/펜/커서/팔레트/스타일 + elm-magic 재현 3
  + 아이콘 계약 9).
  `shell_tests::tab_click_selects`와 `settings_modal_selects_smoothing`이 **값 prop
  회귀 지점**이다 (`key=` 없이 통과해야 한다).
- **버그 최소 재현**: `crates/freedf-gui/tests/elm_magic_bugs.rs` — 이 문서 §1에서
  실측으로 확인한 3건을 어댑터만 직접 호출해 재현한다(창 800×600 고정). 단언이
  "현재(버그 있는) 동작"을 잠그므로, elm-magic이 고치면 그 테스트가 깨진다.
- 시각/레이아웃 감사:
  `scripts/edev-run.sh --config .edev-gui.toml eval scripts/design-audit.luau --out-dir tmp/out`
  — `layout_issues` **0건**(1100×720 **및** `FREEDF_GUI_SIZE=900x600`), 계약 id
  (`gui.*`, `canvas.surface`) 전부 등록, `small_targets` 0건(컨트롤 `min-height: 28`),
  캔버스 856×475 / 656×355.
- 감사 수치 읽기 주의: `contrast`는 렌더된 픽셀 샘플이라 안티에일리어싱 때문에 이론값보다
  낮게 나온다. 흰 글자 on `#2563EB`(`.btn--on`/`.swatch--on`)는 이론 **5.17:1**이고,
  실측은 샘플 지점에 따라 1.75~5.17로 흔들린다(같은 스타일인데 `gui.medium`은 5.17,
  `gui.swatch_1`은 글리프 가장자리에 걸려 1.75) — 경계값은 이미지와 함께 판단한다
  (`docs/DESIGN-SYSTEM.md` §9.3).
