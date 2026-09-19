//! 폰트 — **Inter(라틴) + Asta Sans(한글) + Heroicons Outline(아이콘)** 만 등록한다.
//!
//! egui 기본 폰트(Hack/Ubuntu/NotoEmoji…)는 등록하지 않는다. freedf-gui 셸은
//! 이 세 폰트로만 그린다 — 라틴은 Inter, 한글/CJK는 Asta Sans, 아이콘은
//! iconflow의 **Heroicons Outline** 글리프([`crate::ui::icons`]가 어휘를 갖는다).
//!
//! 아이콘을 **named family로 고르지 않는 이유**: elm-magic CSS가 노출하는
//! `font-family` 값은 `proportional`/`monospace` 둘뿐이라 셸 마크업에서
//! "Heroicons Outline"을 직접 지정할 길이 없다. 대신 비례/고정 패밀리의
//! **폴백 끝**에 넣어 둔다 — egui는 글리프를 패밀리 순서대로 찾으므로
//! 아이콘 글리프(U+E000 대역)만 Heroicons에서 채워지고 라틴/한글은 앞의 두 폰트가 담당한다.
//!
//! **Heroicons Outline 하나만** 넣는 이유: 같은 이름의 `Filled`와 `Outline`은
//! 코드포인트가 **같다**(실측 318/324 — `tests/icons_tests.rs`가 어휘를 검증).
//! 폴백 스택에 둘 다 넣으면 앞에 온 쪽만 그려지므로 스타일은 하나로 고정한다.

use eframe::egui;
use std::sync::Arc;

/// 라틴 문자 — Inter.
const INTER_REGULAR: &[u8] = include_bytes!("../../freedf/assets/fonts/Inter-Regular.ttf");
/// 한글/CJK — Asta Sans.
const ASTA_SANS_REGULAR: &[u8] = include_bytes!("../../freedf/assets/fonts/AstaSans-Regular.ttf");

/// 폰트 이름 (패밀리 순서의 근거).
pub const INTER: &str = "inter";
pub const ASTA_SANS: &str = "asta_sans";

/// 아이콘 폰트 패밀리 (TTF 내부 이름 — iconflow가 알려준다).
pub fn icon_family() -> &'static str {
    crate::ui::icons::family()
}

/// 패밀리 순서 — Inter → Asta Sans → Heroicons(아이콘 폴백).
pub fn family() -> Vec<String> {
    vec![
        INTER.to_owned(),
        ASTA_SANS.to_owned(),
        icon_family().to_owned(),
    ]
}

/// 폰트 정의 — [`install`]이 ctx에 넣는 것과 **같은** 값(테스트가 검사한다).
///
/// 아이콘 폰트는 `iconflow::fonts()`가 주는 TTF 중 **Outline 변형만** 고른다
/// (피처를 `pack-heroicons`로 좁혀도 Filled/Outline 두 개가 온다).
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
    // 아이콘 폰트 — 패밀리 이름은 파일명이 아니라 TTF 내부 이름이다(iconflow FAQ).
    let icon = icon_family();
    debug_assert!(
        !icon.is_empty(),
        "iconflow Heroicons 폰트 패밀리를 못 찾았다 — ui/icons.rs의 어휘를 확인할 것"
    );
    for font in iconflow::fonts() {
        if font.family == icon {
            fonts.font_data.insert(
                font.family.to_owned(),
                Arc::new(egui::FontData::from_static(font.bytes)),
            );
        }
    }

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
