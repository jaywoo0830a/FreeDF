//! freedf-gui 스타일 — **elm-magic CSS 속성만**으로 정의한다 (단일 스타일 출처).
//!
//! 마크업(`shell.rs`/`ui/*`)은 BEM 클래스 이름만 붙이고, 폭·여백·색·글자 크기는
//! 전부 이 파일이 정한다.
//!
//! ## 컨셉 — Quiet chrome, loud canvas
//!
//! 1. **크롬은 한 판**(`.chrome`)이다. 바마다 라운드 카드를 쌓으면 같은 판이
//!    3~4장 겹쳐 보인다. 안쪽 구획은 헤어라인(`.bar__rule`)이 만든다.
//! 2. **버튼은 바탕이 없다**(`.btn`). 버튼 20개가 각각 상자를 그리면 격자처럼
//!    보인다 — 호버에서 떠오르고, 눌림에서만 액센트가 스친다.
//! 3. **액센트는 의미가 있을 때만** 나타난다: 켜진 도구(`.btn--on`) · 주 동작
//!    (`.btn--primary`) · 위험 동작(`.btn--danger`) · 토스트.
//! 4. **선택은 두 단계**다 — 도구처럼 하나만 켜지는 것은 채움(`--on`), 굵기/필압/
//!    패널처럼 여럿이 켜질 수 있는 것은 떠오른 바탕 + 액센트 헤어라인(`--sel`).
//!
//! ## 색 — 토큰 14슬롯이 상한
//!
//! elm-magic CSS의 색 값은 **토큰 이름만** 받는다(`bg: #fff`는 컴파일 에러).
//! 그래서 팔레트는 [`Token`] 14슬롯에 역할을 배정하는 방식이고, 새 색이 필요하면
//! 슬롯을 재배치해야 한다. 액센트를 **글자**로 쓸 때는 `info`(surface 위 6.08:1)를
//! 쓰고, `primary`는 채움과 헤어라인(UI 구성요소 기준 3.0)에만 쓴다.
//!
//! ## BEM
//!
//! 셀렉터는 **BEM 클래스뿐**이다(태그 셀렉터 금지 — `tests/style_tests.rs`가 강제).
//! 블록: `app` `chrome` `topbar` `tabs` `bar` `panel` `statusbar` `modal`
//! `btn` `swatch`. 수정자는 기본 클래스와 **함께** 붙여 차이만 덮는다
//! (`tabs__item--active`, `btn--danger`).
//!
//! ## 간격 사다리 — 4 · 6 · 8 · 10 · 12 · 16 · 20
//!
//! 여백은 **한 사다리**만 쓴다. 단계가 올라갈수록 큰 덩어리를 나눈다:
//!
//! | 값 | 쓰는 곳 |
//! |---|---|
//! | 4 | 한 묶음 안의 칩 (`bar__group` · `topbar__nav` · `topbar__end`) |
//! | 6 | 탭 사이(`tabs`) · 묶음 사이(`bar`) · 패널 행 사이(`panel`) · 모달 묶음 안(`modal__group`) |
//! | 8 | 컨트롤 좌우 패딩(`btn`·`swatch`·`panel__row`) · 판 **안**에서 행 사이(`chrome`) |
//! | 10 | 모달 액션 사이(`modal__actions`) |
//! | 12 | 판 **사이**(`app`·`app__body`) · 상단 바 그룹 사이(`topbar`) · 판 안쪽 세로 패딩 |
//! | 16 | 정보 스트립의 값 사이(`statusbar`) · 모달 블록 사이 |
//! | 20 | 모달 안쪽 패딩 |
//!
//! 사다리에 없는 값(5·7·9 등)은 쓰지 않는다 — 단계가 하나 늘면 계층이 흐려진다.
//!
//! 크기 사다리도 하나다: **컨트롤 높이 30**(`btn`·`swatch`·`tabs__item`·`panel__row`)
//! 이고 도구 줄의 행 높이도 30이다(감사 `small_targets` 24pt 기준을 크게 넘긴다). 세로
//! 헤어라인은 22 + 마진 4×2 = 30이라 컨트롤과 **정확히 같은 높이**로 중심이 맞는다.
//! 예외는 정보 스트립(26) 하나다 — 글자 12짜리 띠라 컨트롤 높이를 따르지 않는다.
//!
//! ## 왼쪽 인셋 — 글자는 모두 24
//!
//! 크롬(6 + 2) · 패널(8) · 정보 스트립(16)의 **글자**가 모두 x=24에서 시작한다. 크롬과
//! 패널은 항목 상자(16) + 항목 패딩(8)이 만들고, 정보 스트립은 항목 상자가 없어
//! 좌우 패딩이 그 8을 대신한다(글자가 곧 항목이다). 판마다 시작선이 다르면 세로로
//! 나란한 목록(상태 문자열 ↔ 패널 행 ↔ 캔버스)이 어긋나 보인다.
//!
//! 실측 확인: 패널 행의 첫 잉크 x=24(`sample_pixels`), 크롬 버튼 상자 x=16 + 패딩 8.
//!
//! ## 세로 예산
//!
//! 크롬은 도구 줄 수만큼 늘어난다(`.bar` 줄은 명시적으로 나눈다 — 아래 `wrap` 항목).
//! 지금 크롬은 **187px**다: 행 30×4 + 헤어라인 1×3 + 행 사이 8×6 + 판 패딩 8×2.
//! 캔버스가 남은 높이를 전부 받는다(720px 창에서 471). 간격을 키울 때는 감사
//! (`layout_issues` + 캔버스 높이)를 다시 확인한다.
//!
//! ## `justify`는 쓰고, `wrap`은 쓰지 않는다 (실측)
//!
//! `justify: end`는 먹는다 — `.topbar__end { flex-grow: 1; justify: end }`로 앱 명령
//! (Settings/About)이 상단 바 오른쪽 끝에 붙는다. 0.8.1에서는 `width: fill` 대신
//! `flex-grow: 1`을 쓴다(어댑터가 둘을 같은 배분으로 처리한다).
//!
//! `wrap: true`는 **우리 트리에서는 동작하지 않는다**(실측): 도구 줄의 항목이 부모
//! 폭을 넘어도 줄바꿈하지 않고 그대로 뻗어, 루트가 창 밖으로 팽창하고(1100px 창에서
//! root w=1171) 캔버스가 0크기·offscreen이 된다. 그래서 도구 줄은 **명시적으로
//! 나누고**, 각 줄을 좁은 창에서도 넘지 않는 폭(≤730px)으로 유지한다.
//!
//! 캔버스가 남은 세로를 전부 먹으므로(`<Raw>`가 `available_size()`를 소비) 정보
//! 스트립은 캔버스 **위**에 온다 — 순서를 바꾸면 스트립 높이가 0이 된다.
//!
//! ## 0.8.1 확장 속성 — 쓰는 것과 못 쓰는 것 (실측)
//!
//! elm-magic 0.8.1은 CSS 속성을 62개로 늘렸다. 이 파일이 실제로 쓰는 것:
//!
//! | 속성 | 쓰는 곳 | 이유 |
//! |---|---|---|
//! | `flex-grow: 1` | `.topbar__end` · `.statusbar__meta` | 남는 폭을 먹는다(`width: fill`과 같은 배분) |
//! | `white-space: nowrap` + `text-overflow: ellipsis` + `max-lines: 1` | 상태 문장 · 패널 항목 | 긴 문장을 **자른다** — 넘치면 창을 민다 |
//! | `overflow: hidden` | `.chrome` · `.panel` · `.panel__list` | 라운드 모서리 밖으로 새지 않게 |
//! | `pointer-events: none` | 헤어라인 · 섹션 제목 | 장식은 클릭 대상이 아니다 |
//! | `margin-top`/`margin-bottom` | `.bar__sep` | 세로 헤어라인을 30(= 컨트롤 높이)에 맞춘다 |
//! | `border-width`/`border-color` + `:focus` | `.modal__input` | 테두리를 항상 두고 색만 바꾼다(크기 고정) |
//!
//! 못 쓰는 것과 그 이유:
//!
//! - `align-self: center` — 교차축이 아직 무한한 줄에서는 **가용 높이를 통째로 먹는다**.
//!   실측: `.bar__sep`에 걸었더니 잉크 줄이 300px로 부풀어 편집 줄이 y=402로 밀렸다
//!   (정상 y=105). 세로 정렬이 필요하면 마진으로 맞춘다.
//! - `overflow: auto/scroll` — 어댑터가 `ScrollArea`를 id salt 없이 만든다. 패널을 둘
//!   이상 열면 egui가 **ID 충돌 경고를 화면에 그리고** 스크롤 상태를 공유한다(실측:
//!   Library + Bookmarks 동시 열림). 그래서 목록은 `overflow: hidden`으로 자른다.
//! - `border-*-width`(개별) — 어댑터가 **최댓값 하나**로 그린다(사방 동일). 한쪽만
//!   선을 긋는 용도로는 못 쓴다.
//! - `rotate` — egui `TSTransform`에 회전이 없어 반영되지 않는다(어댑터 문서).
//! - `width`/`height`는 여전히 **내용 상자**다(0.7과 같다) — `.panel { width: 216;
//!   padding: 12 8 }`의 바깥 폭은 232다.
//!
//! ## 예외 — egui 네이티브 위젯
//!
//! `<Raw>`(캔버스 painter)·`<Input>`·창 크롬·스크롤바는 egui가 직접 그린다.
//! [`install_egui_visuals`]가 그 최소 설정만 담당하고 색은 같은 팔레트에서 온다.
//! 캔버스 스테이지/종이 색은 CSS 밖이라 **리터럴**을 쓴다([`stage_color`] 등).

use eframe::egui;
use elm_magic::style::{Color, Palette, Token};

elm_magic::css! {
    // ── app — 창 루트 ───────────────────────────────────────────
    // 배경/글자색/기본 크기가 여기서 정해지고 **상속**된다.
    // `padding`은 창 방어 여백(Windows 최대화 시 좌우 밀림) 겸용이다.
    // 세로 12 / 가로 8: 가로는 줄 폭 예산(900px 창)에 직접 들어가므로 세로보다 작다.
    .app { bg: background; color: text; font-size: 14; padding: 12 8; gap: 12; }
    // 본문 행(패널 + 캔버스) — 남은 세로를 전부 받는다. 캔버스가 이 행의
    // 마지막 자식이라 `height: fill`이 없으면 행 높이가 콘텐츠에 끌려간다.
    // 주의: 행에 `align: center`를 걸면 egui 교차축 정렬이 "가용 높이 전체" 기준이
    // 되어 각 행이 남은 세로를 다 먹는다(실측: 캔버스 높이 0). 그래서 쓰지 않는다.
    .app__body { gap: 12; height: fill; }
    // 빈 자리표시를 **그리지 않게** 한다 — display:none은 공간도 차지하지 않는다.
    .app__hidden { display: none; }

    // ── chrome — 상단 크롬 전체를 **한 판**으로 묶는다 ──────────
    // 바마다 라운드 카드를 쌓으면 같은 판이 3~4장 겹쳐 보인다(투박함의 주원인).
    // 크롬은 이 한 판이고, 안쪽 구획은 헤어라인(`.bar__rule`)이 만든다.
    // `width: fill`은 **루트 바로 아래**에서만 쓴다: 여기서 폭이 확정되어야 안쪽의
    // 가로 헤어라인(`.bar__rule`)이 가용 폭을 물고 판을 창 밖으로 늘리지 않는다.
    // 패딩은 세로 8 / 가로 6 — 가로는 줄 폭 예산에 들어가므로 6으로 눌러 둔다.
    // `overflow: hidden` — 라운드 모서리(10) 밖으로 자식 바탕이 새지 않게 자른다.
    .chrome { width: fill; bg: surface; radius: 10; padding: 8 6; gap: 8; overflow: hidden; }

    // ── topbar — 브랜드 · 탭 · 문서/앱 명령 ─────────────────────
    // `min-height`는 컨트롤 높이(30)와 **같다** — 다르면 30짜리 버튼이 행 위쪽에 붙어
    // 아래 헤어라인까지의 여백만 커진다(실측: 행 사이 8인데 여기만 12).
    .topbar { gap: 12; padding: 0 2; min-height: 30; }
    // 브랜드는 `info`(surface 위 6.08:1) — `primary`는 14px 본문 대비 3.37이라
    // 글자로 쓰지 않는다(액센트 "텍스트"의 슬롯이 `info`인 이유).
    .topbar__brand { color: info; font-size: 17; weight: bold; letter-spacing: 0.4; }
    .topbar__nav { gap: 4; }
    // 우측 그룹 — 남는 폭을 받아(`flex-grow: 1`) 오른쪽으로 민다(`justify: end`).
    // 0.8 `flex-grow`는 어댑터에서 `width: fill`과 같은 배분을 한다(교차축도 함께
    // 채운다). 의도가 "남는 폭을 먹는다"이므로 `fill` 대신 `flex-grow`로 쓴다.
    .topbar__end { flex-grow: 1; justify: end; gap: 4; }

    // ── tabs — 문서 탭 칩 ───────────────────────────────────────
    // 스트립에 배경을 깔지 않는다 — 크롬 위에 크롬을 겹치지 않는다.
    // 활성 탭은 **떠오른 바탕 + 액센트 헤어라인**이다. 파란 채움은 "지금 켜진
    // 도구" 하나에만 남긴다(활성 표시가 동시에 5개씩 파랗던 문제).
    .tabs { gap: 6; }
    .tabs__item { color: text_dim; radius: 6; padding: 6 8; min-height: 30; cursor: pointer; }
    .tabs__item:hover { bg: surface_alt; color: text; }
    .tabs__item--active { bg: surface_alt; color: text; weight: bold; border-width: 1; border-color: primary; }

    // ── bar — 도구 줄 ───────────────────────────────────────────
    // 줄은 **명시적으로 나눈다**. 실측: `wrap: true`는 우리 트리에서 줄바꿈을
    // 만들지 못했다 — 노드가 부모 폭을 넘어 루트가 창 밖으로 팽창하고(1100px 창에서
    // root w=1171) 뒤 항목(Redo/Clear Ink)과 캔버스가 offscreen/0크기가 된다.
    // 그래서 각 줄을 **좁은 창(900px)에서도 넘지 않는 폭**으로 유지한다(각 줄 ≤ 730).
    .bar { gap: 6; padding: 0 2; }
    .bar__group { gap: 4; }
    // 그룹 구분 헤어라인 — 높이 22 + 개별 마진 4/4 = 30(컨트롤 높이와 같다).
    // 0.8 `margin-top`/`margin-bottom`을 쓴다. `align-self: center`는 **쓰지 않는다**:
    // 이 줄의 교차축(세로)이 아직 무한이라 가용 높이를 통째로 먹어 행이 300px로
    // 부풀었다(실측: 잉크 줄 y=67 → 편집 줄 y=402). 마진은 행 높이를 고정한다.
    // `pointer-events: none` — 장식이라 클릭/커서 대상이 아니다.
    .bar__sep { width: 1; height: 22; margin-top: 4; margin-bottom: 4; bg: border; pointer-events: none; }
    // 크롬 안의 가로 헤어라인 — 판을 늘리지 않고 구획만 만든다.
    .bar__rule { width: fill; height: 1; bg: border; pointer-events: none; }

    // ── btn — 버튼 하나 + 상태 + 변형 ───────────────────────────
    // 기본 버튼은 **바탕이 없다**: 크롬에 버튼 20개가 각각 상자를 그리면
    // 격자처럼 보인다. 호버에서 떠오르고, 눌림에서만 액센트가 스친다.
    // 크기: `padding: 6 8` + `min-height: 30`(감사 small_targets 24pt 기준을 크게 넘긴다).
    .btn { color: text; radius: 6; padding: 6 8; min-height: 30; cursor: pointer; }
    .btn:hover { bg: surface_alt; color: text; }
    .btn:active { bg: primary; color: on_primary; }
    // 주 동작 — 화면에 하나뿐인 액션(모달 OK / 기본값 저장).
    .btn--primary { bg: primary; color: on_primary; weight: bold; }
    .btn--primary:hover { bg: primary; color: on_primary; }
    // 켜짐(도구) — 액센트 채움. 한 줄에 하나만 켜지는 것에 쓴다.
    .btn--on { bg: primary; color: on_primary; weight: bold; }
    .btn--on:hover { bg: primary; color: on_primary; }
    // 켜짐(토글: 굵기/필압/패널) — **떠오른 바탕 + 액센트 헤어라인**.
    // 채움보다 한 단계 조용해서 "도구 켜짐"과 위계가 갈린다.
    .btn--sel { bg: surface_alt; color: text; weight: bold; border-width: 1; border-color: primary; }
    .btn--sel:hover { bg: surface_alt; color: text; }
    // 파괴적 동작 — **채움**으로 표현한다(`error`는 본문 텍스트 대비가 3.22라
    // 텍스트 색으로 쓰지 않는다).
    .btn--danger { bg: error; color: on_primary; }
    .btn--danger:hover { bg: error; color: on_primary; }
    // 보조 동작(설정/정보/취소) — 글자만 남긴다.
    .btn--ghost { color: text_dim; }
    .btn--ghost:hover { bg: surface_alt; color: text; }

    // ── swatch — 즐겨찾기 색 칩 ─────────────────────────────────
    // 글자색은 `text`다 — `text_dim`은 surface_alt 위 이론값 5.83이지만 원형
    // 글리프의 안티에일리어싱 때문에 실측이 3.81로 떨어진다.
    // 켜짐은 `.btn--sel`과 같은 문법(떠오른 바탕 + 액센트 헤어라인)이다.
    .swatch { color: text; radius: 6; padding: 6 8; min-height: 30; cursor: pointer; }
    .swatch:hover { bg: surface_alt; color: text; }
    .swatch--on { bg: surface_alt; color: text; weight: bold; border-width: 1; border-color: primary; }
    .swatch--on:hover { bg: surface_alt; color: text; }

    // ── 텍스트 ──────────────────────────────────────────────────
    // 섹션 라벨은 작고 흐린 대문자 라벨 (Bootstrap form-label 결).
    // 장식이라 클릭 대상이 아니다 — `pointer-events: none`. 오른쪽 값(개수 배지)은
    // 같은 줄의 스페이서가 밀어낸다(텍스트 노드는 늘어나지 않는다 — `.panel__spacer`).
    .section__title { color: text_dim; font-size: 11; weight: bold; letter-spacing: 0.8; text-transform: uppercase; pointer-events: none; }
    .modal__title { color: text; font-size: 16; weight: bold; }
    .text { color: text_dim; }

    // ── panel — 사이드바(라이브러리/북마크/목차) ─────────────────
    // 폭은 **고정**이다(216). 내용 크기로 두면 안쪽 가로 헤어라인(`.panel__rule`)이
    // 가용 폭을 다 먹어 패널이 화면 폭을 전부 차지한다(실측: 캔버스 폭 0).
    // `height: fill` — 캔버스와 같은 높이를 갖는다. 캔버스가 `.app__body` 안에
    // 있으므로 Row 높이가 확정되어 이 값이 안전하다.
    // 패딩은 세로 12 / 가로 8 — 가로 8은 크롬(6 + 2)과 같은 인셋이라 패널 행과 크롬
    // 버튼의 시작선이 맞는다.
    // 주의: elm-magic CSS의 `width`/`height`는 **내용 상자**다 — 216 + 패딩 8×2 = 바깥
    // 232다(실측: 패널 상자가 x=8..240). 폭을 바꾸면 캔버스 폭이 그만큼 따라 움직인다.
    .panel { width: 216; bg: surface; radius: 10; padding: 12 8; gap: 6; height: fill; overflow: hidden; }
    // 패널 머리 — 제목(남는 폭) + 개수 배지, 그 아래 헤어라인.
    // 제목이 `flex-grow: 1`이라 배지가 오른쪽 끝에 붙는다(실측: 0.8.1 `natural_size`가
    // `flex-grow` 있는 노드를 가변으로 본다 — 텍스트 노드에도 먹는다).
    .panel__head { gap: 6; padding: 0 8; }
    .panel__head-row { gap: 8; }
    // 개수 배지 — 목록이 몇 개인지 제목 옆에서 바로 읽힌다.
    .panel__badge { color: text_dim; font-size: 11; bg: surface_alt; radius: 4; padding: 1 6; }
    // 빈 스페이서 — 남는 폭을 먹어 배지/값을 줄 끝으로 민다. **텍스트 노드는
    // `flex-grow`로 늘어나지 않는다**(실측: 제목에 걸었더니 배지가 글자 바로 뒤에 붙었다).
    // 늘어나는 쪽은 컨테이너여야 한다.
    .panel__spacer { flex-grow: 1; }
    .panel__rule { width: fill; height: 1; bg: border; pointer-events: none; }
    // 목록은 판 안에서 **잘린다**(`overflow: hidden`). 스크롤(`overflow: auto`)을 쓰면
    // 어댑터가 egui `ScrollArea`를 id salt 없이 만들어(0.8.1 `render_el`: `ScrollArea::
    // vertical()`만 호출) 패널을 둘 이상 열 때 **ID가 충돌**한다 — 화면에 egui 디버그
    // 경고("Second use of ScrollArea ID …")가 뜨고 스크롤 상태를 공유한다(실측:
    // Library + Bookmarks 동시 열림). 스크롤이 필요해지면 패널을 하나만 열거나
    // 어댑터에 id salt를 넣은 뒤 되돌린다.
    .panel__list { height: fill; gap: 6; overflow: hidden; }
    // 행 높이는 컨트롤과 같다(6+18+6 = 30). 다르면 패널만 커져 크롬과 어긋난다.
    .panel__row { radius: 6; padding: 6 8; min-height: 30; }
    .panel__row:hover { bg: surface_alt; }
    // 긴 제목은 **자른다** — 줄바꿈하면 행 높이가 흔들리고 목록 리듬이 깨진다.
    // 색은 `text`다: 행 전체가 클릭 영역이라 흐린 회색보다 읽히는 편이 낫다.
    .panel__item { color: text; font-size: 13; cursor: pointer; white-space: nowrap; text-overflow: ellipsis; max-lines: 1; }
    // 값이 있는 행은 라벨 폭을 묶어 둔다 — 그러지 않으면 긴 제목이 값을 밀어낸다.
    .panel__row--meta .panel__item { max-width: 150; }
    // 행 끝의 보조 값 — 목차의 페이지 번호. 라벨보다 한 단계 작고 흐리다.
    .panel__meta { color: text_dim; font-size: 11; }
    .panel__empty { color: text_dim; font-size: 12; padding: 6 8; width: fill; text-align: center; }

    // ── statusbar — 캔버스 위 정보 스트립 ───────────────────────
    // 캔버스가 남은 공간을 전부 먹으므로 이 스트립은 캔버스 **위**에 온다.
    // 바탕을 깔지 않는다 — 앱 바탕(`background`)이 곧 스트립이고, 판이 하나
    // 줄어든다. 상태 문자열은 값 목록으로 읽히게 `gap`으로만 나눈다.
    // 좌우 패딩 16 = 인셋 8 + 항목 패딩 8 — 이 스트립만 **항목 상자가 없어서**
    // (글자가 곧 항목이다) 크롬·패널의 글자 시작선(x=24)에 맞추려면 16이 필요하다.
    // `width: fill` — 이 스트립은 창 폭을 전부 쓴다. 그래야 오른쪽 메타를 끝으로 밀 수
    // 있다(`.statusbar__meta`의 `flex-grow: 1` + `justify: end`).
    .statusbar { width: fill; padding: 0 16; min-height: 26; gap: 16; }
    // 상태 문장은 길이를 우리가 정하지 못한다(파일 이름/오류). **한 줄로 자른다** —
    // 자르지 않으면 긴 문장이 스트립을 넘어 창을 밀어낸다(0.7의 fill 팽창과 같은 결과).
    .statusbar__text { color: text_dim; font-size: 12; max-width: 520; white-space: nowrap; text-overflow: ellipsis; max-lines: 1; }
    .statusbar__meta { color: text_dim; font-size: 12; flex-grow: 1; justify: end; white-space: nowrap; text-overflow: ellipsis; max-lines: 1; }
    .statusbar__toast { color: warn; font-size: 12; max-width: 520; white-space: nowrap; text-overflow: ellipsis; max-lines: 1; }

    // ── modal ──────────────────────────────────────────────────
    // 그림자는 모달에만 쓴다 — 나머지 구획은 배경 단차로 만든다.
    // 모달의 `gap` 16은 **블록 사이**다(제목 / 필드 / 프리셋 / 액션). 라벨과 그 필드처럼
    // 붙어 있어야 하는 것은 `.modal__group`(gap 6)으로 묶는다 — 그러지 않으면 라벨이
    // 자기 필드에서 16px 떨어져 다음 블록처럼 읽힌다.
    .modal { bg: surface; radius: 12; padding: 20; gap: 16; shadow: 0 8 24; shadow-color: shadow; }
    // 블록 안의 묶음 — 라벨↔필드, 확인 모달의 질문↔액션 (사다리 6).
    .modal__group { gap: 6; }
    // 사실 표 — 라벨/값 행 (설정 창). 행 사이 4, 라벨 폭 고정으로 값이 정렬된다.
    .modal__facts { gap: 4; }
    .modal__fact { gap: 8; }
    .modal__fact-label { color: text_dim; font-size: 12; width: 68; }
    .modal__fact-value { color: text; font-size: 13; }
    // 푸터 구분선 — 본문과 액션을 가른다. 헤어라인 하나로 "여기서부터 조작부"를
    // 알린다(모달 안에서 액션이 본문에 섞여 보이던 문제).
    .modal__rule { width: fill; height: 1; bg: border; pointer-events: none; }
    // 되돌릴 수 없는 동작의 질문 줄 — `warn` 색.
    .modal__warn { color: warn; }
    // `<Input>`은 egui가 직접 그린다 — CSS는 커서/패딩/**테두리**만 지정한다.
    // 어댑터가 egui의 `interact_size`를 0으로 리셋하므로(실측: egui 스타일로는 높이가
    // 안 변한다) 필드 높이는 CSS가 만든다: 글자 18 + 5×2 + 테두리 1×2 = 30 = 컨트롤 높이.
    // 테두리는 **항상** 두고 색만 바꾼다(`:focus`) — 나타났다 사라지면 필드가 2px
    // 커졌다 작아지며 아래 액션 행이 흔들린다.
    .modal__input { cursor: text; padding: 5 8; border-width: 1; border-color: border; }
    .modal__input:focus { border-color: primary; }
    // 액션 행은 **오른쪽 정렬**이다. 실측: elm-magic 모달 창은 내용보다 넓어서 밀
    // 공간이 있다(액션 행의 첫 버튼이 plain 행보다 223px 오른쪽으로 밀린다) —
    // `justify: end`는 밀 공간이 있는 컨테이너에서 정상 동작한다.
    .modal__actions { gap: 10; }
    .modal__actions--end { justify: end; }
}

/// 팔레트 — CSS 색 토큰(`bg: surface` 등)의 값. **14슬롯이 전부**다.
///
/// 중성 슬레이트 4단(`background` < `surface` < `surface_alt` < `border`) + 단일
/// 액센트(`primary` = `#2563EB`)이고, 구획은 보더가 아니라 이 단차가 만든다.
/// 색을 늘릴 수 없으므로(토큰 14슬롯이 상한) 새 역할이 필요하면 슬롯을 재배치한다.
///
/// 사용 규칙(대비는 실측값):
/// - `primary`를 **글자**로 쓰지 않는다 — on `surface` 3.37:1이라 큰 글자만 허용된다.
///   액센트 글자가 필요하면 `info`(6.08)를 쓴다. `primary`는 채움과 헤어라인에만.
/// - `error`도 글자로 쓰지 않는다(3.22) — 위험 동작은 **채움**으로 표현한다
///   (`bg: error` + `color: on_primary` = 5.40).
/// - `text_dim`은 `border` 바탕 위에 쓰지 않는다(4.47) — 호버 바탕 위 글자는 `text`.
pub fn palette() -> Palette {
    Palette::dark()
        .with(Token::Primary, Color::rgb(0x25, 0x63, 0xeb))
        .with(Token::OnPrimary, Color::rgb(0xff, 0xff, 0xff))
        .with(Token::Background, Color::rgb(0x0f, 0x11, 0x15))
        .with(Token::Surface, Color::rgb(0x17, 0x1a, 0x21))
        .with(Token::SurfaceAlt, Color::rgb(0x22, 0x26, 0x2f))
        .with(Token::Border, Color::rgb(0x33, 0x39, 0x44))
        .with(Token::Text, Color::rgb(0xe6, 0xe8, 0xec))
        .with(Token::TextDim, Color::rgb(0x9b, 0xa1, 0xac))
        .with(Token::Error, Color::rgb(0xc4, 0x31, 0x4b))
        .with(Token::Warn, Color::rgb(0xff, 0xb2, 0x24))
        .with(Token::Success, Color::rgb(0x30, 0xa4, 0x6c))
        .with(Token::Info, Color::rgb(0x3e, 0x9b, 0xff))
        .with(Token::Shadow, Color::rgba(0x00, 0x00, 0x00, 0x96))
        .with(Token::Overlay, Color::rgba(0x0f, 0x11, 0x15, 0xbe))
}

/// CSS 색 토큰 → egui 색 (어댑터가 쓰는 변환과 같은 규칙).
fn to_color32(color: Color) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a)
}

/// 토큰 하나를 egui 색으로 (캔버스 같은 egui 네이티브 코드가 쓴다).
pub fn token_color(token: Token) -> egui::Color32 {
    to_color32(palette().get(token))
}

/// 창 클리어 색 — `.app`의 `bg: background`와 **같은 토큰**이라 리사이즈
/// 중에도 이음새가 없다 (eframe 기본값은 반투명 근사 검정).
pub fn clear_color() -> [f32; 4] {
    token_color(Token::Background).to_normalized_gamma_f32()
}

/// 캔버스 스테이지 — 페이지 뒤 영역. `background`(#0F1115)보다 **한 단계 아래**
/// 리터럴이라 캔버스 영역이 창 여백과 구분된다 (CSS 밖 painter 색).
pub fn stage_color() -> egui::Color32 {
    egui::Color32::from_rgb(0x0b, 0x0d, 0x11)
}

/// 페이지(흰 종이) 테두리 — 종이 윤곽을 또렷하게 만드는 어두운 헤어라인.
pub fn page_border_color() -> egui::Color32 {
    egui::Color32::from_rgb(0x0a, 0x0b, 0x0e)
}

/// 페이지 그림자 — `draw_paper`가 겹으로 근사한다(egui painter에는 blur가 없다).
pub fn paper_shadow_color() -> egui::Color32 {
    egui::Color32::from_rgb(0x00, 0x00, 0x00)
}

/// elm-magic CSS가 닿지 않는 **egui 네이티브 위젯**의 최소 설정.
///
/// 대상: `<Raw>` 캔버스 painter, `<Input>`/`<TextArea>` 텍스트 편집, `Divider`의
/// 구분선 색, 스크롤바, 창 크롬. 여기서 정하지 않은 것은 egui 기본값을 쓴다 —
/// 셸의 나머지 전부는 위 `css!`가 담당한다.
pub fn install_egui_visuals(ctx: &egui::Context) {
    // PDF 리더는 장시간 읽기 — 다크 고정 (시스템 라이트 테마에서도).
    ctx.set_theme(egui::ThemePreference::Dark);
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        ctx.style_mut_of(theme, |style| {
            style.visuals = visuals();
            // egui 네이티브 위젯(`<Input>`, 스크롤바, 창 크롬)의 기본 글자 크기 —
            // CSS `.app`(14)과 같은 값이라 마크업 텍스트와 섞여도 튀지 않는다.
            style.text_styles = [
                (egui::TextStyle::Heading, egui::FontId::proportional(18.0)),
                (egui::TextStyle::Body, egui::FontId::proportional(14.0)),
                (egui::TextStyle::Button, egui::FontId::proportional(14.0)),
                (egui::TextStyle::Small, egui::FontId::proportional(12.0)),
                (egui::TextStyle::Monospace, egui::FontId::monospace(14.0)),
            ]
            .into();
        });
    }
}

/// 네이티브 위젯용 `Visuals` — 색은 전부 팔레트 토큰에서.
fn visuals() -> egui::Visuals {
    let p = palette();
    let background = to_color32(p.get(Token::Background));
    let surface = to_color32(p.get(Token::Surface));
    let surface_alt = to_color32(p.get(Token::SurfaceAlt));
    let border = to_color32(p.get(Token::Border));
    let text = to_color32(p.get(Token::Text));
    let primary = to_color32(p.get(Token::Primary));

    let mut v = egui::Visuals::dark();
    v.dark_mode = true;
    // 창/패널/입력 바탕 — `.app`(background)과 어긋나지 않게.
    // `window_fill`은 모달의 **제목 띠**에 쓰인다(아래 주석) — 한 단계 밝은
    // `surface_alt`로 두어 "헤더 밴드 + 본문"이라는 의도된 2단이 된다.
    v.window_fill = surface_alt;
    v.panel_fill = background;
    v.extreme_bg_color = surface_alt;
    v.faint_bg_color = surface_alt;
    // 모달의 **제목 띠**도 egui가 그린다: 어댑터는 제목 프레임을 따로 주지 않아
    // `Frame::window(&style)`이 쓰인다(0.8.1 `window.rs`: `title_frame.unwrap_or_else`).
    // 그래서 여기 값이 곧 그 띠의 바탕/테두리/모서리다 — 우리 CSS 판(surface,
    // radius 12, 그림자)과 겹치지 않게 테두리·그림자를 끄고 모서리만 맞춘다.
    v.window_stroke = egui::Stroke::NONE;
    v.window_shadow = egui::Shadow::NONE;
    v.window_corner_radius = egui::CornerRadius::same(12);
    // Divider(`ui.separator()`)가 쓰는 색.
    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, border);
    v.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, text);
    v.widgets.inactive.bg_fill = surface_alt;
    v.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, border);
    v.widgets.hovered.bg_fill = surface;
    // 텍스트 편집 커서/선택.
    v.text_cursor.stroke = egui::Stroke::new(2.0, primary);
    v.selection.bg_fill = primary.gamma_multiply(0.35);
    v
}
