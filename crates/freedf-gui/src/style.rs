//! freedf-gui 스타일 — **elm-magic CSS 속성만**으로 정의한다 (단일 스타일 출처).
//!
//! 설계 사양은 `docs/DESIGN-SYSTEM.md`다. 이 파일은 그 사양의 구현이고, 사양과
//! 어긋나면 **문서를 먼저** 고친다(문서 = 단일 진실).
//!
//! ## 컨셉 — Quiet chrome, loud canvas
//!
//! 크롬(바·패널)은 중성 슬레이트 4단의 **배경 단차**로 구획하고 보더를 쓰지 않는다.
//! 액센트는 하나(`primary`)이며 선택/활성 상태에만 나타난다. 캔버스가 주인공이다.
//!
//! ## 색 — 토큰 14슬롯이 상한
//!
//! elm-magic CSS의 색 값은 **토큰 이름만** 받는다(`bg: #fff`는 컴파일 에러).
//! 그래서 팔레트는 [`Token`] 14슬롯에 역할을 배정하는 방식이고, 새 색이 필요하면
//! 슬롯을 재배치해야 한다. 대비는 전부 계산값이다(`docs/DESIGN-SYSTEM.md` §2).
//!
//! ## BEM
//!
//! 셀렉터는 **BEM 클래스뿐**이다(태그 셀렉터 금지 — `tests/style_tests.rs`가 강제).
//! 블록: `app` `topbar` `tabs` `inkbar` `viewbar` `panel` `statusbar` `modal`
//! `btn` `swatch`. 수정자는 기본 클래스와 **함께** 붙여 차이만 덮는다
//! (`tabs__item--active`, `btn--danger`).
//!
//! ## 간격 스케일 — 4px 베이스
//!
//! 4 · 8 · 12 · 16 · 24px만 쓴다(`docs/DESIGN-SYSTEM.md` §3). 그룹 내부는 3–4,
//! 그룹 사이는 12, 바 좌우 패딩은 8, 모달 패딩은 16이다. 임의 값(5, 7, 10 …)은
//! 넣지 않는다 — 리듬이 흐트러진다.
//!
//! ## 세로 예산
//!
//! 크롬 1줄 합계는 184px(`docs/DESIGN-SYSTEM.md` §6.2)이고 캔버스가 남은 높이를
//! 전부 받는다. 컨트롤 `min-height: 28`은 감사 `small_targets`(24pt) 기준을 넘긴다.
//! 간격을 키울 때는 감사(`layout_issues` + 캔버스 높이)를 다시 확인한다.
//!
//! ## 우측 정렬과 `wrap`은 쓰지 않는다 (실측)
//!
//! 두 가지가 elm-magic 0.7.4에서 **동작하지 않는다**:
//!
//! 1. `.…__end { justify: end }` — 콘텐츠 크기 자식 Row에서는 효과가 없다
//!    (before 캡처에서 About이 x≈760에 멈춤 — 1100px 창의 우측 끝이 아님).
//! 2. `width: fill` 스페이서 — `available_width()`가 행의 남은 폭이 아니라
//!    커서 기준으로 `max_rect`를 재설정해 **부모 max_rect를 창 밖으로 팽창**시킨다
//!    (실측: 캔버스 폭 1754 > 창 1100 → 캔버스가 화면 밖까지 커짐).
//!
//! `wrap: true`는 어댑터가 **읽지 않는다**(`ResolvedStyle.wrap`은 파싱만 되고
//! 사용처가 없다) — 행은 항상 단일 줄이다. 그래서 한 행의 항목이 창 폭을 넘으면
//! 조용히 화면 밖으로 나간다: 감사 `layout_issues`의 `offscreen`이 그 감지기다.
//! 그룹 사이 헤어라인(`.bar__sep`)과 `gap`만으로 위계를 만든다.
//!
//! 위 3건의 **최소 재현**은 `crates/freedf-gui/tests/elm_magic_bugs.rs`에 있다
//! (어댑터만 직접 호출, 창 800×600 고정, 실측값이 주석에 있다).
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
    .app { bg: background; color: text; font-size: 14; padding: 8; gap: 8; }
    // 본문 행(패널 + 캔버스) — 남은 세로를 전부 받는다. 캔버스가 이 행의
    // 마지막 자식이라 `height: fill`이 없으면 행 높이가 콘텐츠에 끌려간다.
    // 주의: 행에 `align: center`를 걸면 egui 교차축 정렬이 "가용 높이 전체" 기준이
    // 되어 각 행이 남은 세로를 다 먹는다(실측: 캔버스 높이 0). 그래서 쓰지 않는다.
    .app__body { gap: 12; height: fill; }
    // 빈 자리표시를 **그리지 않게** 한다 — display:none은 공간도 차지하지 않는다.
    .app__hidden { display: none; }

    // ── topbar — 브랜드 · 탭 · 문서/앱 명령 ─────────────────────
    // 브랜드 + 탭 + 문서 명령 + 앱 명령. `wrap`은 무효라(모듈 문서) 한 줄 고정이다 —
    // 항목이 넘치면 감사 `offscreen`이 잡으므로 항목 수를 예산 안에 유지한다.
    .topbar { bg: surface; radius: 8; padding: 6 8; gap: 12; }
    .topbar__brand { color: primary; font-size: 20; weight: bold; letter-spacing: 0.2; }
    .topbar__nav { gap: 4; }
    .topbar__end { gap: 4; }

    // ── tabs — 문서 탭 스트립 (TopBar 안) ───────────────────────
    // 활성 탭은 **액센트 채움** — 어느 문서에 있는지 한눈에 보이게 한다.
    .tabs { bg: background; radius: 6; padding: 3; gap: 4; }
    .tabs__item { bg: surface_alt; color: text_dim; radius: 6; padding: 4 10; min-height: 28; cursor: pointer; }
    .tabs__item:hover { bg: border; color: text; }
    .tabs__item--active { bg: primary; color: on_primary; weight: bold; }

    // ── inkbar — 도구·색(1줄) + 편집·굵기(2줄) ──────────────────
    // **2줄 고정**: elm-magic 행은 줄바꿈하지 않으므로(모듈 문서) 한 줄에 몰면
    // 좁은 창에서 화면 밖으로 나간다(실측: 900px에서 thick/pressure offscreen).
    // 줄을 나눠 두면 900px에서도 각 줄이 폭 안에 들어간다.
    .inkbar { bg: surface; radius: 8; padding: 4 8; gap: 3; }
    .inkbar__line { gap: 12; }
    .inkbar__group { gap: 3; }

    // ── viewbar — 보기/이동(1줄) + 문서 동작(2줄) ───────────────
    .viewbar { bg: surface; radius: 8; padding: 3 8; gap: 3; }
    .viewbar__line { gap: 12; }
    .viewbar__group { gap: 3; }

    // ── bar 요소 — 그룹 구분 헤어라인 ──────────────────────────
    .bar__sep { width: 1; height: 20; bg: border; }

    // ── btn — 버튼 하나 + 상태 + 변형 ───────────────────────────
    // 색/호버/눌림은 `.btn` 혼자 소유하고, 변형은 차이만 덮는다.
    // 크기: `padding: 4 10` + `min-height: 28`(감사 small_targets 24pt 기준).
    .btn { bg: surface_alt; color: text; radius: 6; padding: 4 10; min-height: 28; cursor: pointer; }
    .btn:hover { bg: border; color: text; }
    .btn:active { bg: primary; color: on_primary; }
    // 선택 상태 (Bootstrap `.active` 결).
    .btn--on { bg: primary; color: on_primary; weight: bold; }
    // 파괴적 동작 — **채움**으로 표현한다(`error`는 본문 텍스트 대비가 3.22라
    // 텍스트 색으로 쓰지 않는다: docs/DESIGN-SYSTEM.md §2.1).
    .btn--danger { bg: error; color: on_primary; }
    .btn--danger:hover { bg: error; color: on_primary; }
    // 보조 동작(취소/닫기/설정/정보) — 바탕을 죽이고 글자만 남긴다.
    .btn--ghost { bg: background; color: text_dim; }
    .btn--ghost:hover { bg: surface_alt; color: text; }

    // ── swatch — 즐겨찾기 색 칩 ─────────────────────────────────
    // 글자색은 `text`다(`text_dim`이면 surface_alt 위에서 5.83 이론값이지만
    // 원형 글리프의 안티에일리어싱 때문에 감사 실측이 3.81로 떨어진다 — 실측 기록:
    // docs/DESIGN-SYSTEM.md §9.3).
    .swatch { bg: surface_alt; color: text; radius: 6; padding: 4 10; min-height: 28; cursor: pointer; }
    .swatch:hover { bg: border; color: text; }
    .swatch--on { bg: primary; color: on_primary; weight: bold; }

    // ── 텍스트 ──────────────────────────────────────────────────
    // 섹션 라벨은 작고 흐린 대문자 라벨 (Bootstrap form-label 결).
    .section__title { color: text_dim; font-size: 12; weight: bold; letter-spacing: 0.6; text-transform: uppercase; }
    .modal__title { color: text; font-size: 16; weight: bold; }
    .text { color: text_dim; }

    // ── panel — 사이드바(라이브러리/북마크/목차) ─────────────────
    // `height: fill` — 캔버스와 같은 높이를 갖는다. 캔버스가 `.app__body` 안에
    // 있으므로 Row 높이가 확정되어 이 값이 안전하다(폴백 배치에서는 금지).
    .panel { bg: surface; radius: 8; padding: 8; gap: 4; min-width: 200; height: fill; }
    .panel__row { radius: 6; padding: 6 8; }
    .panel__row:hover { bg: surface_alt; }
    .panel__item { color: text_dim; font-size: 13; cursor: pointer; }
    .panel__empty { color: text_dim; font-size: 12; }

    // ── statusbar — 캔버스 위 정보 스트립 ───────────────────────
    // 캔버스가 남은 공간을 전부 먹으므로(C4) 이 스트립은 캔버스 **위**에 온다.
    .statusbar { bg: background; radius: 6; padding: 2 8; gap: 12; }
    .statusbar__text { color: text_dim; font-size: 12; }
    .statusbar__meta { color: text_dim; font-size: 12; }
    .statusbar__toast { color: warn; font-size: 12; }

    // ── modal ──────────────────────────────────────────────────
    // 그림자는 모달에만 쓴다 — 나머지 구획은 배경 단차로 만든다.
    .modal { bg: surface; radius: 12; padding: 16; gap: 12; shadow: 0 8 24; shadow-color: shadow; }
    // `<Input>`은 egui가 직접 그린다 — CSS는 커서만 지정(나머지는 Visuals).
    .modal__input { cursor: text; }
    .modal__actions { gap: 8; }
    .modal__actions--end { justify: end; }
}

/// 팔레트 — CSS 색 토큰(`bg: surface` 등)의 값. **14슬롯이 전부**다.
///
/// 값·대비·역할의 근거는 `docs/DESIGN-SYSTEM.md` §2다. 요약하면 중성 슬레이트
/// 4단(`background` < `surface` < `surface_alt` < `border`) + 단일 액센트
/// (`primary` = `#2563EB`, 흰 글자 대비 5.17)이고, 구획은 보더가 아니라 이 단차가
/// 만든다. 색을 늘릴 수 없으므로(토큰 14슬롯이 상한) 새 역할이 필요하면 슬롯을
/// 재배치하고 **문서를 함께** 고친다.
///
/// 사용 규칙(§2.1): `primary`는 본문 크기 텍스트로 쓰지 않고(on `surface` 3.37),
/// `error`도 텍스트로 쓰지 않는다(3.22) — 위험 동작은 채움으로 표현한다.
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
    v.window_fill = surface;
    v.panel_fill = background;
    v.extreme_bg_color = surface_alt;
    v.faint_bg_color = surface_alt;
    v.window_stroke = egui::Stroke::new(1.0, border);
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
