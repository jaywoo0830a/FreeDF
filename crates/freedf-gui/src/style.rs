//! freedf-gui 스타일 — **elm-magic 0.7 CSS 속성만**으로 정의한다.
//!
//! 이 파일이 freedf-gui의 **유일한 스타일 출처**다. freedf-theme(egui
//! `Style`/`Visuals` 설치기) 의존은 0 — 셸의 모든 시각적 결정은 아래 `css!`
//! 규칙과 [`palette`]에 있다 (BEM 이름은 `tests/style_tests.rs`가 강제한다).
//!
//! ## 구성
//!
//! - [`palette`] — 의미 토큰 14종의 값(`background` `surface` `border` …).
//!   어댑터가 `render_with_palette`로 받아 CSS의 색 토큰(`bg: surface`)을
//!   실제 색으로 바꾼다. 토큰 값을 바꾸려면 여기 한 곳만 고치면 된다.
//! - `css!` 규칙 — **BEM 클래스 셀렉터만** 쓴다. 태그 셀렉터(`Button` `Text` …)
//!   는 금지: 모든 요소가 마크업에서 자기 클래스를 갖고(`shell.rs`), 스타일은
//!   그 클래스 하나로만 결정된다. 상태는 `:hover` `:active`로 잇는다.
//!   **컴파일타임에 검증된다**: 모르는 속성/값, 깨진 셀렉터는 컴파일 에러.
//!
//! ## BEM 규칙
//!
//! - 블록: `app` `toolbar` `ribbon` `panel` `tabs` `statusbar` `modal` —
//!   화면의 독립 영역. 단독 클래스로도 완결되는 스타일이다.
//! - 요소: `블록__이름` (`toolbar__button`, `panel__title`) — 블록에만 의미가
//!   있는 부분. 요소를 중첩해 `블록__a__b`처럼 쓰지 않는다.
//! - 수정자: `블록__요소--이름` (`tabs__item--active`, `modal__actions--end`)
//!   — 기본 클래스와 **함께** 붙여 차이만 덮는다.
//!
//! 아래 `tests/style_tests.rs`의 `no_tag_selectors_registered` /
//! `selectors_are_bem` / `shell_markup_classes_are_registered`가 이 규칙을
//! 회귀 방지한다 (테스트는 `tests/`에 있다 — 크레이트 관례).
//!
//! ## 예외 하나 — egui 네이티브 위젯
//!
//! elm-magic CSS는 `Col` `Row` `Text` `Strong` `Button` `Tab` `Divider` `Modal`
//! 등 **어휘 태그**에만 닿는다. `<Raw>`(캔버스 painter)·`<Input>`(텍스트 편집)·
//! 창 크롬·스크롤바는 egui가 직접 그리므로 [`install_egui_visuals`]가 최소한만
//! 설정한다 — 색은 팔레트 토큰에서 가져와 출처는 여전히 하나다.

use eframe::egui;
use elm_magic::style::{Color, Palette, Token};

// 셸의 시각 언어 — 전부 CSS 속성이다 (elm-magic 0.7이 실제로 렌더한다).
//
// 셀렉터는 **BEM 클래스뿐**이다: `블록`(app/toolbar/…), `블록__요소`,
// `블록__요소--수정자`, 상태는 `:hover`/`:active`. 태그 셀렉터는 쓰지 않는다 —
// 어느 요소에 어떤 스타일이 붙는지는 `shell.rs`의 `class="…"`가 결정하고,
// 여기서는 그 클래스의 속성만 정의한다.
elm_magic::css! {
    // ── 전역: app — 창 루트 ─────────────────────────────────────
    // 배경/글자색/기본 글자 크기가 여기서 정해지고 **상속**된다 (elm-magic 상속).
    // `.app`의 `padding`은 창 방어 여백(Windows 최대화 시 좌우 밀림) 겸용이다 —
    // 예전 egui Frame 래퍼(ROOT_INNER_MARGIN)를 CSS가 대체했다.
    .app { bg: background; color: text; font-size: 14; padding: 8; gap: 8; }
    // 본문 행(패널들 + 탭 스트립) — 루트의 가로 분할.
    // 주의: 행에 `align: center`를 걸면 egui 교차축 정렬이 "가용 높이 전체"
    // 기준이 되어 각 행이 남은 세로를 다 먹는다(실측: 캔버스 높이 0).
    .app__body { gap: 8; }
    // 빈 자리표시를 **그리지 않게** 한다 — display:none은 공간도 차지하지 않는다.
    .app__hidden { display: none; }

    // ── navbar — 상단 바 (Bootstrap navbar) ─────────────────────
    // 브랜드 + 명령 그룹. 좁은 창에서는 **줄바꿈**한다 (`wrap`) — 예전에는
    // 넘친 오른쪽 명령(Settings/About)이 화면 밖으로 잘려 눌리지 않았다.
    .navbar { bg: surface; border-width: 1; border-color: border; radius: 8; padding: 8 12; gap: 12; wrap: true; }
    .navbar__brand { color: primary; font-size: 18; weight: bold; letter-spacing: 0.3; }
    .navbar__nav { gap: 6; wrap: true; }
    .navbar__end { gap: 6; justify: end; wrap: true; }

    // ── toolbar — 보기/이동 명령 행 ─────────────────────────────
    .toolbar { bg: background; border-width: 1; border-color: border; radius: 8; padding: 6 8; gap: 8; wrap: true; }
    // 명령 그룹 — 바탕을 한 단계 밝게 해서 "한 덩어리"로 읽히게 한다.
    .toolbar__group { bg: surface; radius: 6; padding: 3; gap: 3; }

    // ── ribbon — 잉크 도구/색/굵기 (Bootstrap btn-group 느낌) ───
    .ribbon { bg: surface_alt; radius: 8; padding: 6 8; gap: 8; wrap: true; }
    // 도구 그룹 — 리본 바탕(surface_alt)보다 어두운 판을 깔아 그룹 경계를 만든다.
    .ribbon__group { bg: background; radius: 6; padding: 3; gap: 3; }

    // ── 버튼: 기본형 하나 + 상태 ────────────────────────────────
    // 색/호버/눌림은 `.btn` 혼자 소유한다. 문맥(블록)은 **크기만** 조정하므로
    // 상태 규칙이 블록별로 중복되지 않는다 (하이브리드 CSS의 핵심 규칙).
    .btn { bg: surface_alt; color: text; border-width: 1; border-color: border; radius: 6; padding: 6 12; cursor: pointer; }
    .btn:hover { bg: primary; color: on_primary; border-color: primary; }
    .btn:active { bg: surface; color: text; }
    .btn--on { bg: primary; color: on_primary; border-color: primary; weight: bold; }
    .navbar .btn { padding: 5 10; }
    .toolbar .btn { padding: 5 10; }
    .ribbon .btn { padding: 5 8; }
    .modal .btn { padding: 6 14; }

    // ── swatch — 즐겨찾기 색 칩 ────────────────────────────────
    .swatch { bg: surface_alt; color: text; border-width: 1; border-color: border; radius: 6; padding: 4 8; cursor: pointer; }
    .swatch:hover { bg: primary; color: on_primary; border-color: primary; }
    .swatch--on { bg: primary; color: on_primary; border-color: primary; }

    // ── 제목/본문 텍스트 ───────────────────────────────────────
    // 섹션 제목은 작고 흐린 대문자 라벨 — Bootstrap의 form-label/card-header 결.
    .section__title { color: text_dim; font-size: 12; weight: bold; letter-spacing: 0.6; text-transform: uppercase; }
    .text { color: text_dim; }

    // ── panel — 사이드바/북마크/목차 ────────────────────────────
    // 주의: `height: fill` 금지 — 수평 Row 안의 Col에 가용 높이를 강제하면
    // Row가 남은 세로를 다 먹어 캔버스 높이가 0이 된다 (실측: 캔버스에 획이
    // 기록되지 않았다). 패널은 내용 높이만 차지한다.
    .panel { bg: surface; border-width: 1; border-color: border; radius: 8; padding: 8; gap: 6; min-width: 180; }
    .panel__row { radius: 6; padding: 5 8; }
    .panel__row:hover { bg: surface_alt; }
    .panel__item { color: text_dim; cursor: pointer; }
    .panel__empty { color: text_dim; font-size: 13; }

    // ── tabs — 문서 탭 스트립 (Bootstrap nav-tabs 결) ───────────
    // 활성 탭은 **버튼이 아니라 탭**처럼 보여야 한다: 파란 알약 대신 바탕 +
    // 파란 경계선 + 굵은 글자 (예전엔 활성 탭이 기본 버튼과 구별되지 않았다).
    .tabs { bg: background; border-width: 1; border-color: border; radius: 8; padding: 4; gap: 4; wrap: true; }
    .tabs__item { bg: surface; color: text_dim; border-width: 1; border-color: border; radius: 6; padding: 5 14; cursor: pointer; }
    .tabs__item:hover { color: text; border-color: primary; }
    .tabs__item--active { bg: surface_alt; color: text; border-color: primary; weight: bold; }

    // ── statusbar — 상태바/토스트 ──────────────────────────────
    .statusbar { bg: surface; border-width: 1; border-color: border; radius: 8; padding: 6 10; gap: 8; wrap: true; }
    .statusbar__text { color: text_dim; font-size: 13; }
    .statusbar__toast { color: warn; font-size: 13; }

    // ── modal ─────────────────────────────────────────────────
    .modal { bg: surface; border-width: 1; border-color: border; radius: 10; padding: 16; gap: 10; shadow: 0 8 24; }
    .modal Strong { color: text; font-size: 16; }
    // `<Input>`은 egui가 직접 그린다 — CSS는 커서만 지정(나머지는 Visuals).
    .modal__input { cursor: text; }
    .modal__actions { gap: 8; }
    .modal__actions--end { justify: end; }
}

/// 팔레트 — CSS 색 토큰(`bg: surface` 등)의 값.
///
/// Bootstrap 5 다크 테마의 색 계열에 맞춘 값이다 (`--bs-body-bg`/`--bs-body-color`/
/// `--bs-border-color`). 브랜드 색을 바꾸려면 `Token::Primary` 한 줄만 고치면 된다 —
/// 앱의 모든 파랑(버튼 호버, 활성 탭, 캔버스 커서)이 그 줄에서 따라온다.
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
        .with(Token::Error, Color::rgb(0xdc, 0x35, 0x45))
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
    v.widgets.hovered.bg_fill = to_color32(p.get(Token::Surface));
    // 텍스트 편집 커서/선택.
    v.text_cursor.stroke = egui::Stroke::new(2.0, primary);
    v.selection.bg_fill = primary.gamma_multiply(0.35);
    v
}
