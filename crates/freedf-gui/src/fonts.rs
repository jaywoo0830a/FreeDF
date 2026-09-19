//! 폰트 — **Inter(라틴) + Asta Sans(한글) + Heroicons Outline(아이콘)** 만 등록한다.
//!
//! egui 기본 폰트(Hack/Ubuntu/NotoEmoji…)는 등록하지 않는다. freedf-gui 셸은
//! 이 세 폰트로만 그린다 — 라틴은 Inter, 한글/CJK는 Asta Sans, 아이콘은
//! iconflow의 **Heroicons Outline** 글리프([`crate::ui::icons`]가 어휘를 갖는다).
//!
//! ## iconflow 공식 egui 예제를 따른다
//!
//! 문서(`docs/quickstart.md`)의 egui 통합은 `fonts()`가 주는 폰트를
//! `font_data`에 넣고 **`FontFamily::Name(<TTF 내부 이름>)` 패밀리**를 만든 뒤,
//! 글리프를 `FontId::new(size, FontFamily::Name(...))`로 그린다. 여기서도 그대로
//! 등록한다(변형별 named family까지) — 셸이 나중에 named family를 고를 수 있게.
//!
//! ## 우리가 하나 더 하는 일(elm-magic 제약)
//!
//! elm-magic CSS가 노출하는 `font-family` 값은 `proportional`/`monospace` 둘뿐이라
//! 셸 마크업에서 named family를 지정할 길이 없다(아이콘은 **라벨 문자열 안**에 있다).
//! 그래서 폴백 스택으로 아이콘 글리프를 채운다.
//!
//! **아이콘을 스택의 맨 앞에 둔다** — 뒤에 두면 Inter가 가로챈다. 실측: Inter는
//! PUA에 **745개**(U+E000..U+F6C3) 글리프를 갖고 있어 Heroicons의 324개 중 **238개**를
//! 덮는다(예: `pencil` U+E0F5). 그러면 아이콘이 Heroicons가 아니라 Inter의 PUA 글리프로
//! 그려져 "연필이 얇은 막대"처럼 보인다(크기 불균일·아이콘 누락처럼 보이던 원인).
//! Heroicons 폰트는 **PUA 324개만** 덮으므로(라틴·한글·공백 없음) 맨 앞에 둬도
//! 라틴은 Inter, 한글은 Asta Sans가 그대로 담당한다.
//!
//! **Outline 하나만** 넣는 이유: `Filled`와 `Outline`은 코드포인트가 **같다**
//! (실측 318/324). 둘 다 넣으면 앞에 온 쪽만 그려지므로 스타일은 하나로 고정한다.

use eframe::egui;
use std::sync::Arc;

/// 라틴 문자 — Inter.
const INTER_REGULAR: &[u8] = include_bytes!("../../freedf/assets/fonts/Inter-Regular.ttf");
/// 한글/CJK — Asta Sans.
const ASTA_SANS_REGULAR: &[u8] = include_bytes!("../../freedf/assets/fonts/AstaSans-Regular.ttf");

/// 폰트 이름 (패밀리 순서의 근거).
pub const INTER: &str = "inter";
pub const ASTA_SANS: &str = "asta_sans";

/// 아이콘 폰트 패밀리 (TTF 내부 이름 — `IconRef.family`, iconflow FAQ).
pub fn icon_family() -> &'static str {
    crate::ui::icons::family()
}

/// 폴백 스택 — **Heroicons(아이콘) → Inter → Asta Sans**.
///
/// 아이콘이 맨 앞인 이유는 모듈 문서 참조(Inter의 PUA 745개가 아이콘을 가로챈다).
/// Heroicons는 PUA만 덮으므로 라틴/한글 순서는 그대로다.
pub fn family() -> Vec<String> {
    vec![
        icon_family().to_owned(),
        INTER.to_owned(),
        ASTA_SANS.to_owned(),
    ]
}

/// 폰트 정의 — [`install`]이 ctx에 넣는 것과 **같은** 값(테스트가 검사한다).
pub fn definitions() -> egui::FontDefinitions {
    let mut fonts = egui::FontDefinitions::default();
    // egui 기본 폰트를 **전부 비운다** — 셸은 Inter/Asta Sans/Heroicons만 쓴다.
    fonts.font_data.clear();
    fonts.font_data.insert(
        INTER.to_owned(),
        Arc::new(egui::FontData::from_static(INTER_REGULAR)),
    );
    fonts.font_data.insert(
        ASTA_SANS.to_owned(),
        Arc::new(egui::FontData::from_static(ASTA_SANS_REGULAR)),
    );

    // ── 공식 예제 그대로: 팩의 모든 폰트 + named family ──
    for font in iconflow::fonts() {
        fonts.font_data.insert(
            font.family.to_owned(),
            Arc::new(egui::FontData::from_static(font.bytes)),
        );
        fonts
            .families
            .entry(egui::FontFamily::Name(font.family.into()))
            .or_default()
            .insert(0, font.family.to_owned());
    }

    // ── 셸용 폴백 스택(위 모듈 문서의 이유) ──
    let icon = icon_family();
    debug_assert!(
        !icon.is_empty(),
        "iconflow Heroicons 폰트 패밀리를 못 찾았다 — ui/icons.rs의 어휘를 확인할 것"
    );
    let stack = family();
    fonts
        .families
        .insert(egui::FontFamily::Proportional, stack.clone());
    fonts.families.insert(egui::FontFamily::Monospace, stack);

    fonts
}

/// 폰트 설치 — 비례/고정 패밀리 둘 다 같은 3단 스택을 쓴다.
///
/// (등록하는 폰트가 셋뿐이라 "고정폭"용 별도 폰트가 없다: `monospace`는
/// 비례와 같은 스택을 쓴다.)
pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(definitions());
}
