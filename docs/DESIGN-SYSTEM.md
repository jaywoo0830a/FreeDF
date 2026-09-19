# FreeDF 디자인 시스템 — 예상 트리 · 토큰 · 규칙

> **지위**: `crates/freedf-gui` UI 전면 재설계의 **사양 초안**이다. 코드는 아직 바뀌지
> 않았고, 이 문서가 확정된 뒤 `src/style.rs`(CSS + 팔레트)와
> `src/shell.rs`/`src/ui/*`(마크업)가 이 문서를 따른다.
> 구현이 이 문서와 어긋나면 **문서를 먼저 고친다**(문서 = 단일 진실).

## 0. 근거 (추측 금지 — 실측만)

| 근거 | 값 |
|---|---|
| 캡처 | `tmp/eguidev-screenshots7/design-audit-img_0.jpg` (1100×720) |
| 감사 JSON | `tmp/audit7.log` — `layout_issues {}`, `small_targets {}` |
| **캔버스 (before)** | `canvas.surface` = **x10 y390 w1080 h320** → 크롬이 창 높이의 **54%** |
| **AA 실패 (before)** | `aa_body:false` **4건** — 활성 버튼 `gui.sidebar`가 `bg #0d6efd / fg #fefeff ratio 4.47` |
| 좁은 창 | `tmp/audit6.summary` — 캔버스 **450×258**(300px 미만), `gui.pressure` offscreen |
| 스타일 출처 | `crates/freedf-gui/src/style.rs` (셀렉터 41개 + `palette()` 14토큰) |
| 계약 | `docs/eguidev-automation.md`, `smoketest-gui/10_launch_gui.luau` |
| 가드 | `tests/style_tests.rs`, `tests/shell_tests.rs`, `tests/canvas_tests.rs` |

### 0.1 진단 (D1~D8)

- **D1 위계 부재** — 3개 바 30개 버튼이 전부 같은 크기·같은 배경·같은 무게.
- **D2 카드 중첩** — 바 3개 + 패널 + 상태바가 각각 `border-width:1 + radius:10` → 둥근 보더 박스 6개.
- **D3 세로 예산 초과** — 캔버스 320px(창의 45%), 좁은 창에서 258px.
- **D4 사이드바가 "옆"이 아니라 "위"** — 캡처에서 LIBRARY 카드가 캔버스 **위**에 있고(≈x10..228, y207..343), 캔버스는 그 **아래** 전폭(1080). 이름과 배치가 불일치.
- **D5 정보 구조 혼재** — navbar에 문서/편집/앱 명령이 뒤섞임, 탭 조작 버튼은 탭과 다른 행.
- **D6 상태바 노이즈** — 한영 혼용 6지표를 13px 한 줄에.
- **D7 빈 화면** — 캔버스에 다음 행동 힌트 0.
- **D8 라벨 폭** — 계약 id가 라벨에서 나오므로 축약 불가(아래 C1).

## 1. 하드 제약 (elm-magic 0.7.4 소스 실측)

- **C1 — 계약 id = 버튼 `text`의 슬러그.** `ButtonEl { text, class, disabled, on_click }`
  에 `id`/`tooltip`/`aria`가 **없고**(`elm-magic-0.7.4/src/widget.rs:318`), 어댑터는
  `(text, response)`만 넘긴다(`elm-magic-egui-0.7.4/src/lib.rs:543`).
  → **아이콘 전용 버튼 불가.** `gui.new_tab`을 유지하려면 "New Tab"이 화면에 있어야 한다.
- **C2 — 스모크가 요구하는 21개 위젯은 항상 `present`.** 접힌 메뉴/오버플로로 숨길 수 없다.
- **C3 — CSS 값은 14토큰만.** `bg`/`color`/`fill`/`border-color`/`shadow-color`는
  토큰 이름만 받고 **hex 리터럴은 컴파일 에러**(`elm-magic-macros-0.7.4/src/css.rs:491`).
  → 팔레트는 **14슬롯**이 상한. 색을 늘리려면 토큰을 재배치해야 한다.
- **C4 — 캔버스는 자기 컨테이너의 마지막 자식.** `canvas::paint_ui`가
  `ui.available_size()`를 전부 소비하므로 그 뒤에 오는 **형제**는 0px가 된다.
  모달은 `egui::Window`(플로팅, `elm-magic-egui-0.7.4/src/lib.rs:673`)이라 순서 무관.
  → 정보 스트립은 캔버스 **위**에 온다(§6.3).
- **C5 — 캔버스 높이 ≥300px** (`canvas_tests.rs`, `smoketest-gui/10_launch_gui.luau:36`).
- **C6 — 셀렉터는 BEM 클래스뿐**, 마크업의 모든 클래스는 `style.rs`에 등록(`style_tests.rs`).
- **C7 — `dev-automation` 기능 뒤.** 기본 빌드 동작/성능 영향 금지.
- **C8 — `.edev-instances/` 접근 금지, `Cargo.lock` 변경 금지(`--locked`).**
- **C9 — 문자열이 고정된 지점**(`shell_tests.rs`가 `assert_text`): `FreeDF`, `Untitled`,
  `Library`, `Notes`, `Ready`, `Tab name:`, `Close this tab?`, `PDF file path:`,
  `Remove all ink on this page?`, `Notes panel (placeholder)`, `줌 100%`, `스무딩 Off/Strong`,
  `되돌릴 작업이 없습니다`. 문구를 바꾸려면 **테스트를 함께** 고친다.
- **C10 — `wrap: true`는 무효다.** `ResolvedStyle.wrap`은 파싱만 되고
  `elm-magic-egui` 어댑터가 **읽지 않는다**(0.7.4 소스 확인). 행은 항상 단일 줄이고,
  넘친 항목은 조용히 화면 밖으로 나간다 — 감사 `layout_issues`의 `offscreen`이 감지기다.
  → 행의 항목 수를 **폭 예산 안에** 유지해야 하고, 넘치면 **줄을 나눠**야 한다(§6).
  최소 재현: `crates/freedf-gui/tests/elm_magic_bugs.rs` → `wrap_true_is_ignored`.
- **C11 — 우측 정렬 수단이 없다.** `.…__end { justify: end }`는 콘텐츠 크기 자식
  Row에서 무효(before 캡처: About이 x≈760에서 멈춤)이고, `width: fill` 스페이서는
  ① **뒤 형제 자리를 비우지 않아** 형제를 컨테이너 밖으로 밀고 ② 밀린 형제가 **창을
  넘을 때만** 조상 `max_rect`를 창 밖까지 팽창시킨다(실측: 캔버스 폭 1754 > 창 1100,
  루트 rect 1241). → 그룹은 왼쪽부터 차례로 흐르고, 위계는 **순서와
  헤어라인**(`.bar__sep`)으로 만든다.
  최소 재현: `crates/freedf-gui/tests/elm_magic_bugs.rs` →
  `justify_end_on_content_sized_child_does_nothing`,
  `width_fill_pushes_siblings_out_and_inflates_parent`.

## 2. 브랜드 팔레트 (14슬롯 — C3)

컨셉: **Quiet chrome, loud canvas.** 크롬은 중성 슬레이트 4단, 액센트는 **하나**(블루).
색 대비는 전부 계산값이며(아래 표), 감사 `contrast`의 AA 기준은 본문 4.5 / UI 3.0이다.

| 토큰 | 역할 | 값 | 실측 대비 | 근거 |
|---|---|---|---|---|
| `background` | 창 바탕 · 크롬 바 | `#0F1115` | text 15.4 · text_dim 7.28 | 가장 어두운 단 |
| `surface` | 패널 · 팝업 | `#171A21` | text 14.19 · text_dim 6.71 | 배경 단차 +1.09 (보더 없이 구분) |
| `surface_alt` | 컨트롤 바탕 | `#22262F` | text 12.35 · text_dim 5.83 | 배경 단차 +1.15 |
| `border` | 헤어라인 · 호버 바탕 | `#333944` | text 9.46 · text_dim 4.47 | hover 바탕으로 쓸 때 `text`만 허용 |
| `text` | 본문 · 버튼 라벨 | `#E6E8EC` | 12.35~15.4 | |
| `text_dim` | 보조 · 메타 · 비활성 | `#9BA1AC` | 5.83~7.28 | **AA 본문 통과**(가장 낮은 조합 5.83) |
| `primary` | 브랜드 · 선택/활성 채움 | `#2563EB` | **on_primary 5.17** | 기존 `#0d6efd`(4.5)는 감사에서 AA 실패 4건 → 교체 |
| `on_primary` | 액센트 위 글자 | `#FFFFFF` | 5.17 | |
| `error` | 파괴 동작 채움 | `#C4314B` | on_primary 5.40 | 기존 `#b02a37`(6.5)보다 현대적, AA 여유 유지 |
| `warn` | 토스트 · 경고 텍스트 | `#FFB224` | surface 9.66 | |
| `success` | 성공 | `#30A46C` | surface 5.52 | |
| `info` | 정보 · 보조 액센트 텍스트 | `#3E9BFF` | surface 6.08 | 액센트가 **본문 크기 텍스트**로 필요할 때 |
| `shadow` | 모달 그림자 | `rgba(0,0,0,150)` | — | `Color::rgba` |
| `overlay` | 모달 스크림 | `rgba(15,17,21,190)` | — | `Color::rgba` |

### 2.1 색 사용 규칙 (반드시 지킬 것)

1. **`primary`를 본문 크기 텍스트로 쓰지 않는다.** `primary` on `surface` = **3.37** →
   큰 글자(≥18.66px bold / ≥24px)만 허용. 브랜드는 20px bold라 3.0을 넘긴다.
   액센트 텍스트가 본문 크기로 필요하면 **`info`(6.08)** 를 쓴다.
2. **`text_dim`을 `border` 바탕 위에 쓰지 않는다.** 4.47로 경계선이다 —
   hover 바탕 위 글자는 `text`(9.46)로 올린다.
3. **`error`를 텍스트 색으로 쓰지 않는다.** on `surface` 3.22 = UI(아이콘/보더) 전용.
   위험 동작은 **채움**(`bg: error` + `color: on_primary` = 5.40)으로 표현한다.
4. **보더를 구획에 쓰지 않는다.** 구획은 배경 단차(`background`→`surface`→`surface_alt`)로.
   `border`는 헤어라인 구분자와 hover 바탕에만.
5. 캔버스는 토큰 밖 **리터럴**(egui painter — C3 예외, CSS 아님):
   스테이지 `#0B0D11`(앱 바탕보다 한 단계 아래), 종이 `#FFFFFF`, 종이 그림자 `rgba(0,0,0,90)`.

## 3. 간격 · 여백 · 마진

**베이스 4px.** 아래 5개 값만 쓴다(임의 값 금지 — 리듬이 흐트러진다).

| 이름 | 값 | 용도 |
|---|---|---|
| `space-1` | **4** | 그룹 내부 `gap`, 칩/버튼 좌우 padding, 아이콘↔텍스트 |
| `space-2` | **8** | 앱 padding, 바 좌우 padding, 패널 padding, 바↔바 `gap` |
| `space-3` | **12** | 같은 바 안의 **그룹 사이** `gap`, 모달 gap |
| `space-4` | **16** | 모달 padding |
| `space-5` | **24** | 모달 섹션, 캔버스↔패널 `gap` |

### 3.1 마진 규칙

- **기본 `margin: 0`.** 형제 간격은 `gap`, 안쪽 여백은 `padding`으로 표현한다.
- `margin`은 `gap`으로 표현할 수 없을 때만(예: 모달 안의 단일 문단 앞 간격) 쓰고,
  값은 4px 배수. **허용 형태는 1값(`margin: 8`) 또는 2값(`margin: 8 12`)뿐**이다.
- 금지: 음수 마진, 3값(예: `margin: 4 8 4`), `auto`.
- `padding`도 같은 스케일을 쓴다. 버튼 안쪽은 `padding: 4 10`(space-1/space-2 사이
  이지만 컨트롤 높이 28을 만족시키는 최소값 — 아래 §5의 예외로 명시).

### 3.2 세로 치수 (예산의 핵심)

| 영역 | 높이 | 비고 |
|---|---|---|
| `topbar` | **44** | 브랜드 20px + 탭 + 문서/앱 명령 |
| `inkbar` | **67** (2줄 고정) | 1줄 = 도구·색, 2줄 = 편집·굵기 |
| `viewbar` | **65** (2줄 고정) | 1줄 = 보기/이동, 2줄 = 문서 동작·패널 |
| `statusbar` | **22** | 캔버스 **위**(C4) |
| 컨트롤 | **min-height 28** | 감사 `small_targets`(24pt) 기준을 넘김 |
| 탭 | **min-height 28** | |
| 모달 | max **960×640** | 기존 값 유지 |

> **2줄 고정의 근거(C10)**: 행은 줄바꿈하지 않으므로 1줄에 몰면 좁은 창에서
> 항목이 화면 밖으로 나간다(실측 900×600: `thick`·`pressure`·`settings`·`about`·
> `bookmark`·`clear_ink` offscreen). 줄을 나누면 900px에서도 각 줄이 폭 안에 들어간다.
> 대가는 캔버스 높이 약 26px(500 → 475)이고, 그 대가로 **모든 크기에서 offscreen 0**이 된다.

## 4. 타이포 스케일

폰트는 이미 고정: Inter(라틴) → Asta Sans(한글) → Phosphor(아이콘).

| 역할 | size | weight | line-height | color | 클래스 |
|---|---|---|---|---|---|
| 브랜드 | **20** | bold | 1.2 | `primary` | `.topbar__brand` |
| 모달 제목 | 16 | bold | 1.3 | `text` | `.modal__title` |
| 본문 · 버튼 | **14** | normal | 1.4 | `text` | `.btn`, `.text` |
| 패널 항목 | 13 | normal | 1.4 | `text_dim` (hover `text`) | `.panel__item` |
| 섹션 라벨 | 12 | bold | 1.2 | `text_dim` + uppercase + ls 0.6 | `.section__title` |
| 메타 · 상태 | **12** | normal | 1.3 | `text_dim` | `.statusbar__text` |

## 5. 형태 (radius · border · shadow · opacity)

| 대상 | radius | border | shadow |
|---|---|---|---|
| 칩 · 탭 | 6 | **0** | — |
| 컨트롤(버튼/스와치) | 6 | **0** | — |
| 바(크롬) · 패널 | 8 | **0** (패널만 선택적으로 1) | — |
| 모달 | 12 | 0 | `shadow: 0 8 24` + `shadow-color: shadow` |

- `opacity`는 **비활성 표현에만** 쓴다(0.5). 장식에 쓰지 않는다.
- 호버 단차는 색 한 단계(`surface_alt` → `border`)로만. 이동/확대 애니메이션 없음.

## 6. 트리 (구현 완료 — 실측값 포함)

`App`의 자식 순서 = **배치 순서**다(egui는 위→아래로 공간을 나눠 준다). 캔버스는
`<Raw>`라서 `available_size()`를 전부 먹으므로 **Body의 마지막 자식**이어야 한다(C4).
그래서 정보 스트립은 캔버스 **위**에 온다(§6.3).

```
App .app                                  padding 8 · gap 8 · bg background · 14/text
│
├─ TopBar .topbar                          h 44 · padding 6 8 · gap 12 · bg surface · radius 8
│  ├─ Brand      .topbar__brand            "FreeDF" · 20/bold · primary      → 오른쪽 끝 126
│  ├─ TabStrip   .tabs                     padding 3 · gap 4 · bg background · radius 6
│  │  └─ TabItem .tabs__item--active       bg primary · on_primary · bold     (gui.untitled)
│  ├─ Nav        .topbar__nav  gap 4       [New Tab · Close Tab · Open PDF]
│  └─ TopEnd     .topbar__end  gap 4       [Settings · About]  (btn--ghost)
│                                            → 줄 오른쪽 끝 **693** (사용 폭 1084)
│
├─ InkBar .inkbar                          padding 4 8 · gap 3 · bg surface · radius 8
│  ├─ InkLine    .inkbar__line  gap 12     [Pen · Fountain · Highlighter · Eraser] │ [Swatch 1..3]
│  │                                          → 오른쪽 끝 **700**
│  └─ InkLine    .inkbar__line  gap 12     [Undo · Redo] │ [Thin · Medium · Thick · Pressure]
│                                             → 오른쪽 끝 **516**
│
├─ ViewBar .viewbar                        padding 3 8 · gap 3 · bg surface · radius 8
│  ├─ ViewLine   .viewbar__line gap 12     [Zoom In · Zoom Out · Fit · Prev Page · Next Page]
│  │                                          → 오른쪽 끝 **500**
│  └─ ViewLine   .viewbar__line gap 12     [Save Edits · Load Edits · Bookmark · Clear Ink(danger)]
│                                          │ [Sidebar · Bookmarks · Outline]  → 오른쪽 끝 **740**
│
├─ Statusbar .statusbar                    h 22 · padding 2 8 · gap 12 · bg background · radius 6
│  ├─ Text/Toast .statusbar__text / __toast   좌: "Ready" (또는 3초 토스트)
│  └─ Meta       .statusbar__meta          이어서 문서 메타 ("빈 페이지 … 줌 100% …")
│
├─ Body .app__body                         gap 12 · height fill      ← 남은 세로 전부
│  ├─ Panel .panel                         min-w 200(실측 폭 224) · height fill · bg surface
│  │  ├─ Title .section__title             "LIBRARY" · 12/bold/text_dim/uppercase
│  │  ├─ Row   .panel__row > .panel__item  padding 6 8 · hover bg surface_alt
│  │  └─ Empty .panel__empty
│  └─ Raw  (canvas)                        남은 폭/높이 전부 · stage #0B0D11 · paper #FFFFFF
│                                            실측 1100×720 → **856×475 @(236,237)**
│                                                900×600   → **656×355 @(236,237)**
└─ Modals  .modal                          padding 16 · gap 12 · radius 12 · shadow 0 8 24
```

### 6.1 계약 id → 실제 위치 (21개 전부 상시 노출 — C2)

| id | 라벨(동결) | 위치 | 스타일 |
|---|---|---|---|
| `gui.new_tab` | New Tab | TopBar › `topbar__nav` | `.btn` |
| `gui.close_tab` | Close Tab | TopBar › `topbar__nav` | `.btn` |
| `gui.open_pdf` | Open PDF | TopBar › `topbar__nav` | `.btn` |
| `gui.settings` | Settings | TopBar › `topbar__end` | `.btn--ghost` |
| `gui.about` | About | TopBar › `topbar__end` | `.btn--ghost` |
| `gui.untitled` | Untitled | TopBar › `tabs__item` | `.tabs__item--active` |
| `gui.pen` `fountain` `highlighter` `eraser` | | InkBar 1줄 › group 1 | `.btn` / `.btn--on` |
| `gui.swatch_1` … `_3` | Swatch N | InkBar 1줄 › group 2 | `.swatch` / `.swatch--on` |
| `gui.undo` · `gui.redo` | Undo · Redo | InkBar 2줄 › group 1 | `.btn` |
| `gui.thin` `medium` `thick` `pressure` | | InkBar 2줄 › group 2 | `.btn` / `.btn--on` |
| `gui.zoom_in` `zoom_out` `fit` `prev_page` `next_page` | | ViewBar 1줄 › group 1 | `.btn` |
| `gui.save_edits` `load_edits` `bookmark` | | ViewBar 2줄 › group 1 | `.btn` |
| `gui.clear_ink` | Clear Ink | ViewBar 2줄 › group 1 끝 | `.btn--danger` |
| `gui.sidebar` `bookmarks` `outline` | | ViewBar 2줄 › group 2 | `.btn` / `.btn--on` |
| `canvas.surface` | — | Body › Raw | painter |

> `gui.off/light/normal/strong`(스무딩 프리셋)은 Settings 모달이 열릴 때만 존재 — 기존 계약 그대로.
>
> **활성 항목도 id가 있다**: `BtnOn`은 `<Strong>`이 아니라 `Button` + `btn--on` 수정자라
> `gui.medium`·`gui.sidebar`·`gui.pressure`·`gui.swatch_N`이 그대로 등록된다(감사 실측 확인).

### 6.2 예산 — 예측 vs 실측

`app padding 8×2 = 16`, 바 사이 `gap 8`(자식 5개 → 4개 간격 = 32) 기준.

**예측**: 크롬 = 16 + 32 + 44 + 67 + 65 + 22 = **246** → 1100×720 캔버스 474,
900×600 캔버스 354.

**실측 (감사)**:

| 창 | 캔버스 | 높이 | `layout_issues` | `small_targets` | 캔버스 면적 (before → after) |
|---|---|---|---|---|---|
| 1100×720 | 856×**475** @(236,237) | ✓ ≥300 | **{}** | {} | 345,600 → **406,600** (**+18%**) |
| 900×600 | 656×**355** @(236,237) | ✓ ≥300 | **{}** | {} | 116,100 → **232,880** (**+101%**) |

예측 474/354 vs 실측 475/355 — 오차 1px(반올림). 세로 예산은 **예측대로** 나왔다.

**가로**: 캔버스 폭 = 창폭 − 16 − 224(패널 실측 폭) − 12(gap). 1100 → 848(실측 856),
900 → 648(실측 656). before는 패널이 캔버스 **위**에 있어 캔버스가 전폭(1080/450)이었지만
세로가 320/258이었다 — 이 재설계는 **패널 폭 224를 내주고 세로 155~97을 얻는** 교환이다.

**각 줄의 오른쪽 끝(1100×720, 사용 폭 1084)**: TopBar 693 · InkBar 700/516 ·
ViewBar 500/740 → 전부 안에 들어간다(여유 344~568px).

### 6.3 왜 정보 스트립이 캔버스 위인가

1. **C4**: 캔버스가 `available_size()`를 전부 먹으므로 뒤에 오는 위젯은 0px.
2. **`shell_tests`**: 상태 문자열(`Ready`, `줌 100%`)은 **elm-magic 트리의 텍스트**여야
   `assert_text`/`expect_text`가 잡는다. 캔버스 painter로 옮기면 계약 테스트가 깨진다.
   → 스트립은 트리에 남고, 위치만 캔버스 **위**로 온다.

### 6.4 P3 실험 결과 — `height: fill`은 성공했다

`.app__body { height: fill }` + 캔버스를 Row 안에 넣는 방식(진짜 사이드바)은
**성공**했다. 실측으로 확인한 것:

- 캔버스가 Row 안에서 `available_size()`를 정상적으로 받는다(856×475 @1100×720).
- `.panel { height: fill }`이 캔버스와 같은 높이를 만든다(패널 폭 224, 캔버스 옆).
- `style.rs`의 옛 금지 주석("Row 안의 Col에 `height: fill` → 캔버스 높이 0")은
  **캔버스가 Row 밖에 있던 시절의 것**이라 폐기했다.

폴백(패널을 캔버스 **위**에 두는 구조)은 쓰지 않는다 — 그 구조는 세로를 ~140px 더
먹어 900×600에서 캔버스가 300px 미만이 된다.

## 7. `style.rs` CSS (구현 완료 — 셀렉터 45개)

BEM 클래스만 쓴다(태그 셀렉터 금지 — `style_tests::selectors_use_bem_classes_only`).
값은 전부 §2~§5의 토큰·스케일이며, elm-magic이 컴파일타임에 검증한다.
아래는 `crates/freedf-gui/src/style.rs`의 `css!` 블록과 **같은 내용**이다.

```rust
elm_magic::css! {
    // ── app ──────────────────────────────────────────────────────
    .app { bg: background; color: text; font-size: 14; padding: 8; gap: 8; }
    .app__body { gap: 12; height: fill; }
    .app__hidden { display: none; }

    // ── topbar — 브랜드 · 탭 · 문서/앱 명령 ───────────────────────
    .topbar { bg: surface; radius: 8; padding: 6 8; gap: 12; }
    .topbar__brand { color: primary; font-size: 20; weight: bold; letter-spacing: 0.2; }
    .topbar__nav { gap: 4; }
    .topbar__end { gap: 4; }

    // ── tabs — 문서 탭 ───────────────────────────────────────────
    .tabs { bg: background; radius: 6; padding: 3; gap: 4; }
    .tabs__item { bg: surface_alt; color: text_dim; radius: 6; padding: 4 10; min-height: 28; cursor: pointer; }
    .tabs__item:hover { bg: border; color: text; }
    .tabs__item--active { bg: primary; color: on_primary; weight: bold; }

    // ── inkbar — 도구·색(1줄) + 편집·굵기(2줄) ───────────────────
    .inkbar { bg: surface; radius: 8; padding: 4 8; gap: 3; }
    .inkbar__line { gap: 12; }
    .inkbar__group { gap: 3; }

    // ── viewbar — 보기/이동(1줄) + 문서 동작(2줄) ────────────────
    .viewbar { bg: surface; radius: 8; padding: 3 8; gap: 3; }
    .viewbar__line { gap: 12; }
    .viewbar__group { gap: 3; }

    // ── bar 요소 — 그룹 구분 헤어라인 ────────────────────────────
    .bar__sep { width: 1; height: 20; bg: border; }

    // ── btn — 버튼 하나 + 상태 + 변형 ────────────────────────────
    .btn { bg: surface_alt; color: text; radius: 6; padding: 4 10; min-height: 28; cursor: pointer; }
    .btn:hover { bg: border; color: text; }
    .btn:active { bg: primary; color: on_primary; }
    .btn--on { bg: primary; color: on_primary; weight: bold; }
    .btn--danger { bg: error; color: on_primary; }
    .btn--danger:hover { bg: error; color: on_primary; }
    .btn--ghost { bg: background; color: text_dim; }
    .btn--ghost:hover { bg: surface_alt; color: text; }

    // ── swatch — 즐겨찾기 색 칩 (글자색은 text — §9.3) ───────────
    .swatch { bg: surface_alt; color: text; radius: 6; padding: 4 10; min-height: 28; cursor: pointer; }
    .swatch:hover { bg: border; color: text; }
    .swatch--on { bg: primary; color: on_primary; weight: bold; }

    // ── 텍스트 ──────────────────────────────────────────────────
    .section__title { color: text_dim; font-size: 12; weight: bold; letter-spacing: 0.6; text-transform: uppercase; }
    .modal__title { color: text; font-size: 16; weight: bold; }
    .text { color: text_dim; }

    // ── panel — 사이드바(라이브러리/북마크/목차) ─────────────────
    .panel { bg: surface; radius: 8; padding: 8; gap: 4; min-width: 200; height: fill; }
    .panel__row { radius: 6; padding: 6 8; }
    .panel__row:hover { bg: surface_alt; }
    .panel__item { color: text_dim; font-size: 13; cursor: pointer; }
    .panel__empty { color: text_dim; font-size: 12; }

    // ── statusbar — 캔버스 위 정보 스트립(§6.3) ──────────────────
    .statusbar { bg: background; radius: 6; padding: 2 8; gap: 12; }
    .statusbar__text { color: text_dim; font-size: 12; }
    .statusbar__meta { color: text_dim; font-size: 12; }
    .statusbar__toast { color: warn; font-size: 12; }

    // ── modal ──────────────────────────────────────────────────
    .modal { bg: surface; radius: 12; padding: 16; gap: 12; shadow: 0 8 24; shadow-color: shadow; }
    .modal__input { cursor: text; }
    .modal__actions { gap: 8; }
    .modal__actions--end { justify: end; }
}
```

### 7.1 구현 노트 (실측으로 확정된 것)

- **셀렉터 수 = 45**(기존 41). `docs/elm-magic-notes.md`의 `FREEDF_GUI_STYLE_DIAG`
  기대값을 **45**로 갱신했다.
- `palette()`는 §2의 14슬롯이고 `shadow`/`overlay`는 `Color::rgba`를 쓴다.
- `install_egui_visuals`는 유지하되 색 출처만 새 팔레트로 바꿨다
  (`window_fill = surface`, `panel_fill = background`, `extreme_bg_color = surface_alt`).
- `stage_color()`는 **리터럴 `#0B0D11`**, `page_border_color()`는 `#0A0B0E`,
  `paper_shadow_color()`는 `#000000`이다(캔버스는 CSS 밖 painter).
- **`draw_paper`가 그림자를 3겹으로 근사**한다(`rect.expand(7/5/3)` × alpha 18/30/48,
  `gamma_multiply`) — egui painter에는 blur가 없다.
- `wrap: true`는 **전부 제거**했다(C10 — 무효라서 남겨두면 오해를 만든다).
- `.panel { height: fill }`을 켰다(§6.4 실험 성공).

## 8. 컴포넌트 어휘 (구현 완료)

테스트가 참조하지 않으므로 이름 변경은 안전했다(확인: `tests/*.rs`에
`Navbar`/`Toolbar`/`Ribbon`/`TabStrip` 참조 0건).

**`ui/layout.rs` — 영역(블록)**

| 컴포넌트 | 클래스 | 비고 |
|---|---|---|
| `App` | `.app` | 루트 |
| `TopBar` / `Brand` / `Nav` / `TopEnd` | `.topbar` / `__brand` / `__nav` / `__end` | 상단 바 |
| `TabStrip` | `.tabs` | TopBar 안에 인라인 |
| `InkBar` / `InkLine` / `InkGroup` | `.inkbar` / `__line` / `__group` | 2줄 |
| `ViewBar` / `ViewLine` / `ViewGroup` | `.viewbar` / `__line` / `__group` | 2줄 |
| `Sep` | `.bar__sep` | 그룹 구분 헤어라인 |
| `Panel` | `.panel` | 사이드바(`height: fill`) |
| `Statusbar` / `StatusMeta` | `.statusbar` / `__meta` | 캔버스 위 스트립 |
| `Dialog` / `Actions` / `Presets` | `.modal` / `.modal__actions(--end)` | 모달 |

**`ui/atoms.rs` — 항목**

| 컴포넌트 | 클래스 | 계약 id 근거 |
|---|---|---|
| `Btn` `BtnOn` `BtnDanger` `BtnGhost` | `.btn` / `--on` / `--danger` / `--ghost` | ✓ (라벨이 id) |
| `Swatch` | `.swatch` / `--on` | ✓ (`gui.swatch_N`) |
| `TabItem` | `.tabs__item(--active)` | ✓ (`gui.untitled`) |
| `Section` `Heading` `Note` `PanelRow` `Empty` | `.section__title` `.___` `.text` `.panel__row/__item/__empty` | ✗ (Button 아님) |
| `StatusText` `StatusToast` | `.statusbar__text` / `__toast` | ✗ (자동화 `assert_text` 대상) |

- **`ui/icons.rs`는 라벨 문자열을 바꾸지 않는다**(C1). 아이콘 글리프만 교체했다:
  스와치 글리프를 `CIRCLE`(윤곽선) → **`DOT`(채워진 점)** 으로 바꿔 감사의 중심 픽셀
  샘플링이 원 안쪽(바탕색 혼합)을 잡던 오측정을 없앴다(§9.3).
- 상태 문자열은 `shell.rs`에서 **분리**했다: 좌측 `status`(또는 토스트),
  이어서 `StatusMeta`의 `canvas_status`(§6 트리).

## 9. 검증 결과 (before → after, 실측)

### 9.1 지표

| 지표 | before (실측) | after (실측) | 판정 |
|---|---|---|---|
| 캔버스 @1100×720 | 1080×**320** @(10,390) | 856×**475** @(236,237) | 높이 **+155** (면적 +18%) |
| 캔버스 @900×600 | 450×**258** (✗ C5) | 656×**355** | ✓ (+97, ≥300) |
| 크롬이 먹는 세로 | 390px (창의 54%) | **246px** (34%) | −144 |
| 바 개수 | 5 (navbar/toolbar/ribbon/tabs/statusbar) | **4** (TopBar/InkBar/ViewBar/Statusbar) | tabs 인라인 |
| 보더 박스 | 6 | **0** (그림자는 모달만) | 배경 단차로 구획 |
| `contrast` `aa_body:false` | **4건** (`gui.sidebar` 4.47 …) | **2건**(§9.3 아티팩트) | 이론값은 전부 통과 |
| `layout_issues` | `{}` @1100 / offscreen @900 | **`{}` 양쪽** | ✓ |
| `small_targets` | `{}` | **`{}`** | ✓ (28px) |
| CSS 셀렉터 수 | 41 | **45** | 문서 갱신 |
| 사이드바 | 캔버스 **위**(전폭 캔버스) | 캔버스 **옆**(폭 224, full height) | 진짜 사이드바 |
| 회귀 | — | `cargo test` **69 통과**(+재현 3), 스모크 **`[PASS]`** | ✓ |

### 9.2 재현 명령

```bash
# before 기준선(구 코드) — tmp/audit7.log / tmp/audit6.summary
scripts/edev-run.sh --config .edev-gui.toml eval scripts/design-audit.luau --out-dir tmp/design-before

# after — 1100×720 과 900×600 두 크기
scripts/edev-run.sh --config .edev-gui.toml eval scripts/design-audit.luau --out-dir tmp/design-1100
FREEDF_GUI_SIZE=900x600 scripts/edev-run.sh --config .edev-gui.toml eval scripts/design-audit.luau --out-dir tmp/design-900

# 계약 회귀 / 테스트
scripts/edev-run.sh --config .edev-gui.toml smoke     # [PASS]
cargo test -p freedf-gui --locked                     # 69 통과
cargo test -p freedf-gui --test elm_magic_bugs        # elm-magic 버그 최소 재현 3건
cargo check -p freedf-gui --features dev-automation
cargo check -p freedf                                 # 레거시 기본 빌드 유지
```

- **`FREEDF_GUI_SIZE=WxH`** 를 `main.rs`에 추가했다(기존 `FREEDF_RENDERER`/
  `FREEDF_GUI_PPP`와 같은 방식, 기본 1100×720 — 동작 변화 없음). eguidev에는 창
  리사이즈 API가 없어 이 오버라이드가 좁은 창 검증의 유일한 수단이다.
- `Cargo.lock` 변경 0줄(`--locked` 유지).

### 9.3 남은 `aa_body:false` 2건은 측정 아티팩트다

| id | 실측 | 원인 | 이론값 |
|---|---|---|---|
| `gui.swatch_1` | fg `#6994f1` / bg `#2563eb` = **1.75** | 활성 칩의 중심 픽셀이 **글자/글리프의 안티에일리어싱 가장자리**에 걸려 흰색과 액센트가 섞였다 | 흰 글자 on `#2563EB` = **5.17** ✓ |
| `gui.pressure` | fg `#cddbfa` / bg `#2563eb` = **3.72** | 같은 이유(게이지 글리프 가장자리) | 5.17 ✓ |

같은 행의 같은 스타일인 `gui.medium`(5.17), `gui.sidebar`(5.12), `gui.untitled`(4.91)은
**순수 흰 픽셀**이 잡혀 통과한다 — 즉 스타일이 아니라 **샘플 지점**의 문제다.
`docs/eguidev-automation.md`가 명시하듯 `contrast`는 렌더 픽셀에서 나오므로 경계값은
이미지와 함께 판단한다(캡처: `tmp/design-1100/design-audit-img_0.jpg`).
`900×600`의 `canvas.surface` 항목은 `distinct:false`(흰 종이가 캔버스를 채워 단일 색)라
감사가 수치를 못 만든 경우다.

> 개선 여지: 스와치 글리프를 `DOT`으로 바꿔 *비활성* 칩의 오측정(3.81)은 없앴다.
> 활성 칩의 잔여 아티팩트는 어댑터가 `id`/`title` prop을 갖게 되면(§11.2-3)
> 아이콘 없는 라벨로 정리할 수 있다.

## 10. 단계별 실행 결과 (P0~P4 완료)

| Phase | 내용 | 결과 |
|---|---|---|
| **P0** | 기준선 고정 | before 실측 = 캔버스 1080×320, 크롬 390px, AA 실패 4건 |
| **P1** | **토큰 교체** — 팔레트 14슬롯 + 간격/타이포/radius | `style.rs` 전면 재작성, 셀렉터 41→45 |
| **P2** | **컴포넌트 재정의** — 보더 제거, 버튼 4변형, 칩, `Sep` 헤어라인 | 보더 박스 6→0, 컨트롤 24→28px |
| **P3** | **IA 재배치** — 탭 인라인, `TopBar/InkBar/ViewBar`, 캔버스를 `.app__body` 안으로, 상태 스트립 분리 | 캔버스 320→475 (**+155**), 사이드바가 캔버스 옆 |
| **P3.1** | **측정 후 재배분**(추가) | 1줄 툴바가 900px에서 6개 버튼 offscreen → **2줄 툴바**로 재구성, offscreen 0 |
| **P4** | **캔버스 마감** — 스테이지 리터럴, 종이 그림자 3겹, 종이 테두리 | `#0B0D11` 스테이지 + 그림자 |
| **P5** (남음) | 라이트 테마 / 밀도 프리셋 / 진짜 "Fit"(페이지 맞춤) | 토큰 14슬롯 한계 → 팔레트 2벌 필요 |

### 10.1 구현 중 발견해서 **문서를 먼저 고친** 것

| 발견 | 처리 |
|---|---|
| `wrap: true`가 무효였다(C10) | CSS에서 전부 제거하고 2줄 구성으로 대체 |
| `justify: end`·`width: fill` 스페이서가 레이아웃을 창 밖으로 팽창시켰다(C11) | 스페이서 컴포넌트·CSS 삭제, 우측 정렬 포기 |
| 1줄 툴바가 900px에서 6개 버튼을 화면 밖으로 밀어냈다 | 2줄 툴바(§3.2) — 대가는 캔버스 26px |
| 스와치 글리프 `CIRCLE`이 대비를 오측정시켰다 | `DOT`으로 교체(§9.3) |
| `save_edits`/`load_edits`를 TopBar에 두면 900px에서 넘친다 | ViewBar 2줄의 문서 그룹으로 이동 |
| `draw_paper`가 단색 1px 경계만 그렸다 | 3겹 그림자로 근사(blur 없음) |

## 11. 금지 사항 · 열린 결정

### 11.1 금지 (하드 제약에서 유도)

- 계약 id를 없애거나 이름을 바꾸는 것(C1/C2). 라벨 문구 변경 = id 변경.
- 아이콘 전용 버튼으로 21개 위젯을 숨기는 것(C1/C2).
- 명령을 접힌 메뉴/오버플로로 옮기는 것(C2).
- hex 리터럴을 `css!`에 쓰는 것(C3) — 캔버스 painter만 예외.
- 캔버스와 같은 컨테이너에서 캔버스 **뒤에 형제를 두는 것**(C4) — 그 형제는 0px가 된다.
- 태그 셀렉터, 미등록 클래스(C6), 임의 간격값(§3), 음수 마진(§3.1).
- 기본 빌드에 계측/자동화 코드를 넣는 것(C7), `.edev-instances/` 접근(C8).
- `primary`를 본문 크기 텍스트로, `text_dim`을 `border` 바탕 위에, `error`를 텍스트로
  쓰는 것(§2.1).
- **`wrap: true`를 다시 넣는 것**(C10) — 무효라서 "줄바꿈된다"는 착각만 만든다.
- **`justify: end`/`width: fill` 스페이서로 우측 정렬하는 것**(C11) — 레이아웃을
  창 밖으로 팽창시킨다.
- **한 줄에 항목을 몰아넣는 것** — 행 폭 예산(1100px에서 사용 폭 1084)을 넘기면
  조용히 offscreen이 된다. 넘칠 것 같으면 **줄을 나눈다**.

### 11.2 결정 사항

| # | 결정 | 결론 | 근거 |
|---|---|---|---|
| 1 | 사이드바 기본 상태 | **열림 유지** | `shell_tests` 2개가 "Library/Notes" 노출을 요구. 캔버스 옆 full-height로 바뀌어 세로 손해가 없다 |
| 2 | 툴바 줄 수 | **2줄 고정** | 1줄이면 900px에서 6개 버튼 offscreen. 대가는 캔버스 26px(475, 여전히 +155) |
| 3 | 아이콘 전용 버튼 | **불가 유지**(C1) | `ButtonEl`에 `id`/`title`이 없다. 필요하면 elm-magic 어댑터 작업이 선행돼야 한다 |
| 4 | 테마 | **다크 고정** | 장시간 PDF 읽기 정책. 라이트는 토큰 14슬롯 때문에 팔레트 2벌이 필요(P5) |
| 5 | 상태 스트립 위치 | **캔버스 위 유지** | C4 + `assert_text` 계약(§6.3). 우측 정렬은 C11로 불가 → 좌측 흐름으로 확정 |
| 6 | 우측 정렬 | **포기** | C11. 그룹 순서 + 헤어라인으로 위계를 만든다 |

### 11.3 남은 작업 (P5 후보)

| 항목 | 내용 |
|---|---|
| 진짜 "Fit" | 지금 `zoom_fit()`은 `ViewTransform::default()`(줌 100%)다. A4(842pt) > 캔버스(475)라 페이지 하단이 잘린다 — 캔버스 크기에서 맞춤 배율을 계산하는 기능이 필요(`shell_tests`의 "줌 100%" 기대값도 함께 수정) |
| 스와치 색 노출 | 칩이 자기 **잉크 색**을 보여주지 않는다(글리프는 글자색). 인스턴스별 색은 CSS로 불가 → 어댑터 기능 필요 |
| 좁은 창(<900px) | 2줄로도 900px 미만은 넘칠 수 있다. 3줄 전환 또는 스크롤/오버플로가 필요 |
| 테마 | 라이트 팔레트 2벌 |

### 11.4 다음 문서

- `docs/elm-magic-notes.md` — 셀렉터 수 기대값 **45** + `wrap` 무효/`width: fill` 팽창 실측 기록.
- `docs/eguidev-automation.md` — 바 이름(topbar/inkbar/viewbar)과 활성 항목 id 설명 갱신.
- `docs/DESIGN-SYSTEM.md`(이 문서) — 구현이 어긋나면 **여기를 먼저** 고친다.




