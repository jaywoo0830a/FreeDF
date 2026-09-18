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
//! 아래 `mod tests`의 `no_tag_selectors_registered` / `selectors_are_bem` /
//! `shell_markup_classes_are_registered`가 이 규칙을 회귀 방지한다.
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
// 셀렉터는 **BEM 클래스뿐**이다: `블록`(app/toolbar/…), `블록__요소`,
// `블록__요소--수정자`, 상태는 `:hover`/`:active`. 태그 셀렉터는 쓰지 않는다 —
// 어느 요소에 어떤 스타일이 붙는지는 `shell.rs`의 `class="…"`가 결정하고,
// 여기서는 그 클래스의 속성만 정의한다.
elm_magic::css! {
    // ── 블록: app — 창 루트 ─────────────────────────────────────
    // `.app`이 창 배경 + 방어 여백(Windows 최대화 시 좌우 밀림)을 담당한다 —
    // 예전 egui Frame 래퍼(ROOT_INNER_MARGIN)를 CSS `padding`이 대체한다.
    .app { bg: background; padding: 8; gap: 8; }
    // 본문 행(패널들 + 탭 스트립) — 루트의 가로 분할.
    // 주의: 행에 `align: center`를 걸면 egui 교차축 정렬이 "가용 높이 전체"
    // 기준이 되어 각 행이 남은 세로를 다 먹는다(실측: 캔버스 높이 0).
    .app__body { gap: 8; }
    // 빈 자리표시를 **그리지 않게** 한다 — display:none은 공간도 차지하지 않는다.
    .app__hidden { display: none; }

    // ── 블록: toolbar — 상단 명령 행 ────────────────────────────
    .toolbar { bg: surface; padding: 8; radius: 8; border-width: 1; border-color: border; gap: 8; }
    .toolbar__title { color: text; weight: bold; }
    .toolbar__button { bg: surface_alt; color: text; padding: 8 12; radius: 6; cursor: pointer; }
    .toolbar__button:hover { bg: primary; color: on_primary; }
    .toolbar__button:active { bg: surface; border-width: 1; border-color: border; }

    // ── 블록: ribbon — 잉크 도구/색/굵기 ────────────────────────
    .ribbon { bg: surface_alt; padding: 8; radius: 8; gap: 8; }
    .ribbon__title { color: text; weight: bold; }
    .ribbon__button { bg: surface_alt; color: text; padding: 8 12; radius: 6; cursor: pointer; }
    .ribbon__button:hover { bg: primary; color: on_primary; }
    .ribbon__button:active { bg: surface; border-width: 1; border-color: border; }
    // 활성 항목은 `<Strong>`으로 그린다(비활성은 `.ribbon__button`).
    .ribbon__active { color: text; weight: bold; }

    // ── 블록: panel — 사이드바/북마크/목차 ──────────────────────
    .panel { bg: surface; padding: 8; radius: 8; min-width: 200; gap: 8; }
    .panel__title { color: text; font-size: 16; weight: bold; }
    .panel__row { gap: 8; }
    .panel__item { color: text_dim; padding: 2 4; radius: 4; cursor: pointer; }
    .panel__item:hover { bg: surface_alt; color: text; }
    // 항목이 0개일 때의 안내 문구.
    .panel__empty { color: text_dim; }

    // ── 블록: tabs — 탭 스트립 ──────────────────────────────────
    .tabs { bg: surface; padding: 4; radius: 8; gap: 4; }
    .tabs__item { color: text_dim; }
    // BEM 수정자(`--`)는 **문자열 셀렉터**로 써야 한다 — 매크로가 `-`를
    // 별도 토큰으로 쪼개 `.tabs__item - - active`로 조인해 조용히
    // 미등록되기 때문이다 (elm-magic 0.6 한계, docs/elm-magic-bug-report.md 참고).
    ".tabs__item--active" { color: text; }

    // ── 블록: statusbar — 상태바/토스트 ─────────────────────────
    .statusbar { bg: surface; padding: 4 8; radius: 4; gap: 8; }
    .statusbar__text { color: text_dim; }
    .statusbar__toast { color: text_dim; }

    // ── 블록: modal ─────────────────────────────────────────────
    .modal { bg: surface; padding: 16; radius: 8; shadow: 0 4 16; }
    .modal__title { color: text; weight: bold; }
    .modal__text { color: text_dim; }
    // `<Input>`은 egui가 직접 그린다 — CSS는 커서만 지정(나머지는 Visuals).
    .modal__input { cursor: text; }
    .modal__actions { gap: 8; }
    // 버튼을 오른쪽으로 — `justify: end`(주축 정렬).
    ".modal__actions--end" { justify: end; }
    .modal__button { bg: surface_alt; color: text; padding: 8 12; radius: 6; cursor: pointer; }
    .modal__button:hover { bg: primary; color: on_primary; }
    .modal__button:active { bg: surface; border-width: 1; border-color: border; }
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

#[cfg(test)]
mod tests {
    use elm_magic::style::{lookup_class, selectors};

    /// `:hover` 같은 상태 접미사를 떼고 셀렉터 본체만.
    fn body(selector: &str) -> &str {
        selector.trim().split(':').next().unwrap_or("").trim()
    }

    /// BEM 이름 판정 — `block`, `block__element`, `block__element--modifier`.
    ///
    /// 소문자/숫자/하이픈만 쓰고, `__`·`--` 구분자 조각은 비어 있으면 안 된다
    /// (`.panel_title` 같은 밑줄 하나짜리 이름이 조용히 섞이는 걸 막는다).
    fn is_bem(name: &str) -> bool {
        let Some(name) = name.strip_prefix('.') else {
            return false;
        };
        let word = |s: &str| {
            !s.is_empty()
                && !s.starts_with('-')
                && !s.ends_with('-')
                && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        };
        let (rest, modifier) = match name.split_once("--") {
            Some((rest, m)) => (rest, Some(m)),
            None => (name, None),
        };
        let (block, element) = match rest.split_once("__") {
            Some((b, e)) => (b, Some(e)),
            None => (rest, None),
        };
        word(block)
            && element.map(word).unwrap_or(true)
            && modifier.map(word).unwrap_or(true)
    }

    /// `shell.rs`의 리터럴 `class="…"` 값들 (표현식 class는 별도 검증).
    fn literal_classes(src: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut rest = src;
        while let Some(i) = rest.find("class=\"") {
            rest = &rest[i + "class=\"".len()..];
            let Some(end) = rest.find('"') else { break };
            out.extend(rest[..end].split_whitespace().map(str::to_string));
            rest = &rest[end..];
        }
        out
    }

    /// 태그 셀렉터 금지 — 스타일은 클래스로만 붙인다 (셸 전체가 BEM).
    #[test]
    fn no_tag_selectors_registered() {
        let all = selectors();
        assert!(!all.is_empty(), "css! 규칙이 하나도 등록되지 않았다");
        for sel in &all {
            assert!(
                body(sel).starts_with('.'),
                "태그 셀렉터는 금지 — 클래스(BEM)만 쓴다: {sel}"
            );
        }
    }

    /// 등록된 셀렉터는 전부 BEM 이름이다.
    #[test]
    fn selectors_are_bem() {
        for sel in &selectors() {
            let base = body(sel);
            assert!(is_bem(base), "BEM 이름이 아니다: {sel}");
        }
        // 판정기 자체의 회귀 방지.
        assert!(is_bem(".app") && is_bem(".panel__item") && is_bem(".tabs__item--active"));
        assert!(!is_bem(".panel_title") && !is_bem("Button") && !is_bem(".a__") && !is_bem(".a--"));
    }

    /// 마크업의 모든 class가 CSS에 등록돼 있다 — 요소마다 자기 CSS 이름을 갖는다.
    #[test]
    fn shell_markup_classes_are_registered() {
        let classes = literal_classes(include_str!("shell.rs"));
        assert!(classes.len() > 30, "class 파싱이 이상하다: {classes:?}");
        for name in &classes {
            assert!(
                lookup_class(name).is_some(),
                "shell.rs가 쓰는 `{name}`이 style.rs에 등록되지 않았다"
            );
        }
        // 표현식 class(`class={if …}`)는 리터럴 스캔에 안 잡히므로 명시 검증.
        for name in ["tabs__item", "tabs__item--active"] {
            assert!(lookup_class(name).is_some(), "동적 class `{name}` 미등록");
        }
    }
}
