//! freedf-gui 스타일 — **elm-magic 0.7 CSS 속성만**으로 정의한다.
//!
//! 이 파일이 freedf-gui의 **유일한 스타일 출처**다. freedf-theme(egui
//! `Style`/`Visuals` 설치기) 의존은 0 — 셸의 모든 시각적 결정은 아래 `css!`
//! 규칙과 [`palette`]에 있다.
//!
//! ## 구성
//!
//! - [`palette`] — Bootstrap 5 **다크** 테마의 색 계열(`--bs-body-bg` /
//!   `--bs-secondary-bg` / `--bs-border-color` …)을 elm-magic 토큰 14종에 매핑한다.
//!   어댑터가 `render_with_palette`로 받아 CSS의 색 토큰(`bg: surface`)을 실제
//!   색으로 바꾼다. 브랜드 색을 바꾸려면 `Token::Primary` 한 줄만 고치면 된다.
//! - `css!` 규칙 — **BEM 클래스 셀렉터만** 쓴다. 태그 셀렉터(`Button { … }`,
//!   `.modal Strong`)는 **금지**다: 어느 요소에 어떤 스타일이 붙는지는 셸의
//!   `class="…"`가 정하고, CSS는 그 클래스만 본다. 상태는 `:hover` `:active`로
//!   잇는다. **컴파일타임에 검증된다** — 모르는 속성/값, 깨진 셀렉터는 컴파일 에러.
//!
//! ## BEM 규칙 (하이브리드 CSS)
//!
//! - 블록: `app` `navbar` `toolbar` `ribbon` `panel` `tabs` `statusbar` `modal` —
//!   화면의 독립 영역. 단독 클래스로도 완결된다.
//! - 요소: `블록__이름` (`navbar__brand`, `panel__item`) — 블록에만 의미가 있는
//!   부분. 요소를 중첩해 `블록__a__b`로 쓰지 않는다.
//! - 수정자: `블록__요소--이름` (`tabs__item--active`, `btn--danger`) — 기본
//!   클래스와 **함께** 붙여 차이만 덮는다. 그래서 버튼 색/호버는 `.btn` 혼자
//!   소유한다. 크기는 `height` 하나로만 조정한다 — `Button`/`Tab`은 `padding`을
//!   무시한다(실측).
//!
//! ## 간격 스케일 (Bootstrap `$spacer` 결)
//!
//! 2 · 4 · 6 · 8 · 10 · 12 · 16 · 24px만 쓴다. 영역 간격은 12(루트 `gap`),
//! 그룹 내부는 3–6, 버튼 안쪽은 `6 12`(가로가 세로의 2배)를 기본으로 한다.
//! 임의 값(7, 13 …)은 넣지 않는다 — 리듬이 흐트러진다.
//!
//! `tests/style_tests.rs`의 `selectors_use_bem_classes_only` /
//! `shell_markup_classes_are_registered`가 이 규칙을 회귀 방지한다.
//!
//! ## 예외 하나 — egui 네이티브 위젯
//!
//! elm-magic CSS는 `Col` `Row` `Text` `Strong` `Button` `Tab` `Modal` 등
//! **어휘 태그**에만 닿는다. `<Raw>`(캔버스 painter)·`<Input>`(텍스트 편집)·
//! 창 크롬·스크롤바는 egui가 직접 그리므로 [`install_egui_visuals`]가 최소한만
//! 설정한다 — 색은 팔레트 토큰에서 가져와 출처는 여전히 하나다.

use eframe::egui;
use elm_magic::style::{Color, Palette, Token};

// 셸의 시각 언어 — 전부 CSS 속성이다 (elm-magic 0.7이 실제로 렌더한다).
//
// 위 모듈 문서의 BEM/간격 규칙을 따른다: 셀렉터는 BEM 클래스뿐이고, 태그
// 셀렉터는 한 줄도 없다. 치수는 2·4·6·8·10·12·16·24 스케일만 쓴다.
elm_magic::css! {
    // ── app — 창 루트 ───────────────────────────────────────────
    // 배경/글자색/기본 글자 크기가 여기서 정해지고 **상속**된다.
    // `padding`은 창 방어 여백(Windows 최대화 시 좌우 밀림) 겸용이다.
    .app { bg: background; color: text; font-size: 14; padding: 12; gap: 12; }
    // 본문 행(패널들 + 탭 스트립) — 루트의 가로 분할.
    // 주의: 행에 `align: center`를 걸면 egui 교차축 정렬이 "가용 높이 전체" 기준이
    // 되어 각 행이 남은 세로를 다 먹는다(실측: 캔버스 높이 0). 그래서 쓰지 않는다.
    .app__body { gap: 12; }
    // 빈 자리표시를 **그리지 않게** 한다 — display:none은 공간도 차지하지 않는다.
    .app__hidden { display: none; }

    // ── navbar — 상단 바 ────────────────────────────────────────
    // 브랜드 + 명령 그룹. 좁은 창에서는 **줄바꿈**한다(`wrap`) — 넘친 오른쪽
    // 명령(Settings/About)이 화면 밖으로 잘리던 문제를 막는다.
    .navbar { bg: surface; border-width: 1; border-color: border; radius: 10; padding: 8 12; gap: 12; wrap: true; }
    .navbar__brand { color: primary; font-size: 18; weight: bold; letter-spacing: 0.4; }
    .navbar__nav { gap: 6; wrap: true; }
    .navbar__end { gap: 6; justify: end; wrap: true; }

    // ── toolbar — 보기/이동 명령 행 ─────────────────────────────
    .toolbar { bg: background; border-width: 1; border-color: border; radius: 10; padding: 6 8; gap: 8; wrap: true; }
    // 명령 그룹 — 바탕을 한 단계 밝게 해 "한 덩어리"로 읽히게 한다.
    .toolbar__group { bg: surface; radius: 8; padding: 3; gap: 3; }

    // ── ribbon — 잉크 도구/색/굵기 ──────────────────────────────
    .ribbon { bg: surface_alt; radius: 10; padding: 6 8; gap: 8; wrap: true; }
    // 도구 그룹 — 리본 바탕(surface_alt)보다 어두운 판을 깔아 경계를 만든다.
    .ribbon__group { bg: background; radius: 8; padding: 3; gap: 3; }

    // ── btn — 버튼 하나 + 상태 + 변형 ───────────────────────────
    // 색/호버/눌림은 `.btn` 혼자 소유하고, 변형은 차이만 덮는다.
    .btn { bg: surface_alt; color: text; border-width: 1; border-color: border; radius: 6; height: 24; cursor: pointer; }
    .btn:hover { bg: primary; color: on_primary; border-color: primary; }
    .btn:active { bg: surface; color: text; }
    // 선택 상태 (Bootstrap `.active`).
    .btn--on { bg: primary; color: on_primary; border-color: primary; weight: bold; }
    // 파괴적 동작 (Bootstrap `.btn-danger`) — 호버도 따로 잡아야
    // `.btn:hover`(명시도 높음)에 덮이지 않는다.
    .btn--danger { bg: error; color: on_primary; border-color: error; }
    .btn--danger:hover { bg: error; color: on_primary; border-color: error; }
    // 보조 동작 (취소/닫기) — 바탕을 죽이고 글자만 남긴다.
    .btn--ghost { bg: background; color: text_dim; border-color: border; }
    .btn--ghost:hover { bg: surface_alt; color: text; border-color: border; }
    // 크기 주의(실측): elm-magic `Button`은 **`padding`을 무시하고 `height`만 먹는다**.
    // 그래서 버튼 크기는 `height: 24`(감사 `small_targets`가 보는 최소 클릭 대상)로만
    // 정하고, 적용되지 않는 `padding`은 두지 않는다. `Tab`은 `padding`도 `height`도
    // 무시해 클릭 영역이 라벨 높이(약 19px)로 남는다 — 유일하게 남는 small_target이고,
    // elm-magic이 고쳐야 하는 지점이다(`docs/elm-magic-bug-report.md` §3).

    // ── swatch — 즐겨찾기 색 칩 ─────────────────────────────────
    .swatch { bg: surface_alt; color: text; border-width: 1; border-color: border; radius: 6; height: 24; cursor: pointer; }
    .swatch:hover { bg: primary; color: on_primary; border-color: primary; }
    .swatch--on { bg: primary; color: on_primary; border-color: primary; weight: bold; }

    // ── 제목/본문 텍스트 ────────────────────────────────────────
    // 섹션 제목은 작고 흐린 대문자 라벨 (Bootstrap form-label 결).
    .section__title { color: text_dim; font-size: 12; weight: bold; letter-spacing: 0.6; text-transform: uppercase; }
    // 모달 본문 제목 (Bootstrap `modal-title` 결) — 태그 셀렉터 대신 클래스.
    .modal__title { color: text; font-size: 16; weight: bold; }
    .text { color: text_dim; }

    // ── panel — 사이드바/북마크/목차 ────────────────────────────
    // 주의: `height: fill` 금지 — 수평 Row 안의 Col에 가용 높이를 강제하면 Row가
    // 남은 세로를 다 먹어 캔버스 높이가 0이 된다(실측: 캔버스에 획이 기록되지 않음).
    .panel { bg: surface; border-width: 1; border-color: border; radius: 10; padding: 10; gap: 6; min-width: 200; }
    .panel__row { radius: 6; padding: 8 8; }
    .panel__row:hover { bg: surface_alt; }
    .panel__item { color: text_dim; cursor: pointer; }
    .panel__empty { color: text_dim; font-size: 13; }

    // ── tabs — 문서 탭 스트립 ───────────────────────────────────
    // 활성 탭은 **탭**처럼 보여야 한다: 파란 알약 대신 바탕 + 파란 경계선 + 굵은
    // 글자 (Bootstrap `nav-tabs`의 활성 결).
    .tabs { bg: background; border-width: 1; border-color: border; radius: 10; padding: 4; gap: 4; wrap: true; }
    .tabs__item { bg: surface; color: text_dim; border-width: 1; border-color: border; radius: 6; cursor: pointer; }
    .tabs__item:hover { color: text; border-color: primary; }
    .tabs__item--active { bg: surface_alt; color: text; border-color: primary; weight: bold; }

    // ── statusbar — 상태줄/토스트 ───────────────────────────────
    .statusbar { bg: surface; border-width: 1; border-color: border; radius: 10; padding: 6 12; gap: 10; wrap: true; }
    .statusbar__text { color: text_dim; font-size: 13; }
    .statusbar__toast { color: warn; font-size: 13; }

    // ── modal ──────────────────────────────────────────────────
    .modal { bg: surface; border-width: 1; border-color: border; radius: 12; padding: 16; gap: 10; shadow: 0 8 24; }
    // `<Input>`은 egui가 직접 그린다 — CSS는 커서만 지정(나머지는 Visuals).
    .modal__input { cursor: text; }
    .modal__actions { gap: 8; }
    .modal__actions--end { justify: end; }
}

/// 팔레트 — CSS 색 토큰(`bg: surface` 등)의 값.
///
/// Bootstrap 5 다크 테마의 색 계열에 맞춘 값이다 (`--bs-body-bg` `#212529` /
/// `--bs-tertiary-bg` `#2b3035` / `--bs-secondary-bg` `#343a40` /
/// `--bs-border-color` `#495057` / `--bs-body-color` `#dee2e6` /
/// `--bs-primary` `#0d6efd`). 브랜드 색을 바꾸려면 `Token::Primary` 한 줄만
/// 고치면 된다 — 앱의 모든 파랑(버튼 호버, 활성 탭, 캔버스 커서)이 따라온다.
pub fn palette() -> Palette {
    Palette::dark()
        .with(Token::Primary, Color::rgb(0x0d, 0x6e, 0xfd))
        .with(Token::OnPrimary, Color::rgb(0xff, 0xff, 0xff))
        .with(Token::Background, Color::rgb(0x21, 0x25, 0x29))
        .with(Token::Surface, Color::rgb(0x2b, 0x30, 0x35))
        .with(Token::SurfaceAlt, Color::rgb(0x34, 0x3a, 0x40))
        .with(Token::Border, Color::rgb(0x49, 0x50, 0x57))
        .with(Token::Text, Color::rgb(0xde, 0xe2, 0xe6))
        .with(Token::TextDim, Color::rgb(0xad, 0xb5, 0xbd))
        .with(Token::Info, Color::rgb(0x0d, 0xca, 0xf0))
        .with(Token::Success, Color::rgb(0x19, 0x87, 0x54))
        .with(Token::Warn, Color::rgb(0xff, 0xc1, 0x07))
        // 흰 글자와의 대비: #dc3545는 4.2:1(#dc3545)로 AA에 못 미친다 — 한 단계 어두운
        // Bootstrap danger 강조색(#b02a37)을 쓴다 (5.9:1).
        .with(Token::Error, Color::rgb(0xb0, 0x2a, 0x37))
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

/// 캔버스 스테이지 바탕 — 페이지 뒤 영역 (`<Raw>`가 직접 칠한다).
pub fn stage_color() -> egui::Color32 {
    token_color(Token::SurfaceAlt)
}

/// 페이지(흰 종이) 테두리/그림자 — CSS 밖(painter)에서 쓰는 캔버스 전용 색.
pub fn page_border_color() -> egui::Color32 {
    token_color(Token::Border)
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
