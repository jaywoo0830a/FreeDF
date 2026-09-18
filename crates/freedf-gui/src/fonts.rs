//! 폰트 — **Inter(라틴) + Asta Sans(한글) + Phosphor(아이콘)** 만 등록한다.
//!
//! egui 기본 폰트(Hack/Ubuntu/NotoEmoji…)는 등록하지 않는다. freedf-gui 셸은
//! 이 세 폰트로만 그린다 — 라틴은 Inter, 한글/CJK는 Asta Sans, 아이콘은
//! Phosphor 글리프(레거시 freedf와 같은 `egui_phosphor_icons`).
//!
//! 아이콘을 **named family로 고르지 않는 이유**: elm-magic CSS가 노출하는
//! `font-family` 값은 `proportional`/`monospace` 둘뿐이라 셸 마크업에서
//! "phosphor-regular"를 직접 지정할 길이 없다. 대신 비례/고정 패밀리의
//! **폴백 끝**에 넣어 둔다 — egui는 글리프를 패밀리 순서대로 찾으므로
//! 아이콘 글리프만 Phosphor에서 채워지고 라틴/한글은 앞의 두 폰트가 담당한다.

use eframe::egui;
use std::sync::Arc;

/// 라틴 문자 — Inter.
const INTER_REGULAR: &[u8] = include_bytes!("../../freedf/assets/fonts/Inter-Regular.ttf");
/// 한글/CJK — Asta Sans.
const ASTA_SANS_REGULAR: &[u8] = include_bytes!("../../freedf/assets/fonts/AstaSans-Regular.ttf");

/// 폰트 이름 (패밀리 순서의 근거).
pub const INTER: &str = "inter";
pub const ASTA_SANS: &str = "asta_sans";
/// Phosphor 아이콘 폰트의 데이터 키 (`egui_phosphor_icons::add_fonts`가 넣는 이름).
const PHOSPHOR: &str = "phosphor-icons";

/// 패밀리 순서 — Inter → Asta Sans → Phosphor(아이콘 폴백).
pub fn family() -> Vec<String> {
    vec![INTER.to_owned(), ASTA_SANS.to_owned(), PHOSPHOR.to_owned()]
}

/// 폰트 설치 — 비례/고정 패밀리 둘 다 같은 3단 스택을 쓴다.
///
/// (등록하는 폰트가 셋뿐이라 "고정폭"용 별도 폰트가 없다: `monospace`는
/// 비례와 같은 스택을 쓴다.)
pub fn install(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    // egui 기본 폰트를 **전부 비운다** — 셸은 Inter/Asta Sans/Phosphor만 쓴다.
    fonts.font_data.clear();
    fonts.font_data.insert(
        INTER.to_owned(),
        Arc::new(egui::FontData::from_static(INTER_REGULAR)),
    );
    fonts.font_data.insert(
        ASTA_SANS.to_owned(),
        Arc::new(egui::FontData::from_static(ASTA_SANS_REGULAR)),
    );
    // Phosphor 아이콘 폰트 (+ named family들 — 셸은 폴백으로만 쓴다).
    egui_phosphor_icons::add_fonts(&mut fonts);

    let stack = family();
    fonts
        .families
        .insert(egui::FontFamily::Proportional, stack.clone());
    fonts.families.insert(egui::FontFamily::Monospace, stack);

    ctx.set_fonts(fonts);
}
