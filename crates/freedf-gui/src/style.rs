//! freedf-gui 스타일 — **elm-magic 0.6 CSS 속성만**으로 정의한다.
//!
//! 이 파일이 freedf-gui의 **유일한 스타일 출처**다. freedf-theme(egui
//! `Style`/`Visuals` 설치기) 의존은 0 — 셸의 모든 시각적 결정은 아래 `css!`
//! 규칙과 [`palette`]에 있다 (docs/freedf-gui-migration.md Phase 3).
//!
//! ## 구성
//!
//! - [`palette`] — 의미 토큰 14종의 값(`background` `surface` `border` …).
//!   어댑터가 `render_with_palette`로 받아 CSS의 색 토큰(`bg: surface`)을
//!   실제 색으로 바꾼다. 토큰 값을 바꾸려면 여기 한 곳만 고치면 된다.
//! - `css!` 규칙 — 태그/클래스 셀렉터 + 상태(`:hover` `:active`) + 클래스 조합.
//!   **컴파일타임에 검증된다**: 모르는 속성/값, 깨진 셀렉터는 컴파일 에러.
//!
//! ## 예외 하나 — egui 네이티브 위젯
//!
//! elm-magic CSS는 `Col` `Row` `Text` `Strong` `Button` `Tab` `Divider` `Modal`
//! 등 **어휘 태그**에만 닿는다. `<Raw>`(캔버스 painter)·`<Input>`(텍스트 편집)·
//! 창 크롬·스크롤바는 egui가 직접 그리므로 [`install_egui_visuals`]가 최소한만
//! 설정한다 — 색은 팔레트 토큰에서 가져와 출처는 여전히 하나다.

use eframe::egui;
use elm_magic::style::{Color, Palette, Token};

// 셸의 시각 언어 — 전부 CSS 속성이다 (elm-magic 0.6이 실제로 렌더한다).
//
// 셀렉터 어휘: `Col`/`Row`/`Button`/`Text`/`Strong`/`Tab`/`Divider`/`Modal`(태그),
// `.app` `.<이름>`(클래스), `:hover` `:active` `:disabled`(상태).
elm_magic::css! {
    // ── 기본 리듬: 4px 그리드, 세로 8 / 가로 8 ──────────────────
    // 주의: `Row`에 `align: center`를 전역으로 걸면 egui 교차축 정렬이
    // "가용 높이 전체" 기준이 되어 각 행이 남은 세로를 다 먹는다(실측: 캔버스
    // 높이 0). 컴팩트 행에는 `align`을 두지 않는다.
    Col { gap: 8; }
    Row { gap: 8; }

    // ── 루트/툴바/리본 ────────────────────────────────────────
    // `.app`이 창 배경 + 방어 여백(Windows 최대화 시 좌우 밀림)을 담당한다 —
    // 예전 egui Frame 래퍼(ROOT_INNER_MARGIN)를 CSS `padding`이 대체한다.
    .app { bg: background; padding: 8; gap: 8; }
    .toolbar { bg: surface; padding: 8; radius: 8; border-width: 1; border-color: border; }
    .ribbon { bg: surface_alt; padding: 8; radius: 8; }

    // ── 패널(사이드바/북마크/목차) ────────────────────────────
    .panel { bg: surface; padding: 8; radius: 8; min-width: 200; }
    .panel_title { color: text; font-size: 16; weight: bold; }
    .panel_item { color: text_dim; padding: 2 4; radius: 4; cursor: pointer; }
    .panel_item:hover { bg: surface_alt; color: text; }

    // ── 버튼: 태그 기본 + 상태 (CSS가 상태를 안다) ─────────────
    Button { bg: surface_alt; color: text; padding: 8 12; radius: 6; cursor: pointer; }
    Button:hover { bg: primary; color: on_primary; }
    Button:active { bg: surface; border-width: 1; border-color: border; }

    // ── 텍스트 ───────────────────────────────────────────────
    Strong { color: text; weight: bold; }
    Text { color: text_dim; }
    .status { bg: surface; color: text_dim; padding: 4 8; radius: 4; }
    .muted { color: text_dim; }

    // ── 탭 스트립 ────────────────────────────────────────────
    .tabs { gap: 4; bg: surface; padding: 4; radius: 8; }

    // ── 모달 ─────────────────────────────────────────────────
    Modal { bg: surface; padding: 16; radius: 8; shadow: 0 4 16; }
    // 버튼을 오른쪽으로 — `justify: end`(주축 정렬).
    .modal_actions { justify: end; }
}

/// 팔레트 — CSS 색 토큰(`bg: surface` 등)의 값.
///
/// 기본은 elm-magic의 어두운 팔레트(egui 다크와 맞춘 값). 브랜드 색을 넣으려면
/// `.with(Token::Primary, Color::rgb(...))`를 한 줄 붙이면 된다 — 앱의 모든
/// 파랑이 그 한 줄에서 따라온다.
pub(crate) fn palette() -> Palette {
    Palette::dark()
}

/// CSS 색 토큰 → egui 색 (어댑터가 쓰는 변환과 같은 규칙).
fn to_color32(color: Color) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a)
}

/// 토큰 하나를 egui 색으로 (캔버스 같은 egui 네이티브 코드가 쓴다).
pub(crate) fn token_color(token: Token) -> egui::Color32 {
    to_color32(palette().get(token))
}

/// 창 클리어 색 — `.app`의 `bg: background`와 **같은 토큰**이라 리사이즈
/// 중에도 이음새가 없다 (eframe 기본값은 반투명 근사 검정).
pub(crate) fn clear_color() -> [f32; 4] {
    token_color(Token::Background).to_normalized_gamma_f32()
}

/// 캔버스 스테이지 바탕 — 페이지 뒤 영역 (`<Raw>`가 직접 칠한다).
pub(crate) fn stage_color() -> egui::Color32 {
    token_color(Token::SurfaceAlt)
}

/// 페이지(흰 종이) 테두리/그림자 — CSS 밖(painter)에서 쓰는 캔버스 전용 색.
pub(crate) fn page_border_color() -> egui::Color32 {
    token_color(Token::Border)
}

/// elm-magic CSS가 닿지 않는 **egui 네이티브 위젯**의 최소 설정.
///
/// 대상: `<Raw>` 캔버스 painter, `<Input>`/`<TextArea>` 텍스트 편집, `Divider`의
/// 구분선 색, 스크롤바, 창 크롬. 여기서 정하지 않은 것은 egui 기본값을 쓴다 —
/// 셸의 나머지 전부는 위 `css!`가 담당한다.
pub(crate) fn install_egui_visuals(ctx: &egui::Context) {
    // PDF 리더는 장시간 읽기 — 다크 고정 (시스템 라이트 테마에서도).
    ctx.set_theme(egui::ThemePreference::Dark);
    for theme in [egui::Theme::Dark, egui::Theme::Light] {
        ctx.style_mut_of(theme, |style| style.visuals = visuals());
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
    v.widgets.hovered.bg_fill = to_color32(p.get(Token::Surface));
    // 텍스트 편집 커서/선택.
    v.text_cursor.stroke = egui::Stroke::new(2.0, primary);
    v.selection.bg_fill = primary.gamma_multiply(0.35);
    v
}
