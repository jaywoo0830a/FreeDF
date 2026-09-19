//! 아이콘 계약 — **어휘 전량이 Heroicons Outline 글리프로 해석되는가**.
//!
//! 이 파일이 "레거시 아이콘 라이브러리 미참조 + iconflow 공식 API 사용"의 증거다:
//! 폰트 스택에 Phosphor가 없고, 표(`ui::icons::ICONS`)의 모든 이름이 iconflow
//! Heroicons에서 해석되며, 공식 권장 경로(`list` + `resolve_all` + 인덱스)가 성립한다.
//!
//! 없는 이름은 조용히 "아이콘 없음"(`'\0'`)이 되므로(런타임 오류 없음) 여기서
//! 전량을 훑는 것이 유일한 안전망이다 — 아이콘을 바꾸면 이 테스트가 먼저 깨진다.

use eframe::egui;
use freedf_gui::fonts;
use freedf_gui::ui::icons::{self, ICONS};
use iconflow::{list, resolve_all, try_icon, Pack, Size, Style};

/// Heroicons 글리프는 사설 영역(PUA)에 있다 — 라틴/한글 폰트와 겹치지 않는다.
fn is_pua(c: char) -> bool {
    (0xE000..=0xF8FF).contains(&(c as u32))
}

#[test]
fn resolve_all_matches_list_order_and_resolves_the_pack() {
    // iconflow 권장 콜드 패스: `resolve_all`은 `list`와 1:1, 순서도 같다.
    let names = list(Pack::Heroicons);
    let resolved = resolve_all(Pack::Heroicons, Style::Outline, Size::Regular);
    assert_eq!(
        names.len(),
        resolved.len(),
        "resolve_all은 list와 길이가 같아야 한다"
    );
    for (name, icon) in names.iter().zip(&resolved) {
        let icon = icon
            .as_ref()
            .unwrap_or_else(|err| panic!("{name}: Outline/Regular 해석 실패: {err:?}"));
        assert_eq!(icon.family, "Heroicons Outline", "{name}: 패밀리가 다르다");
    }
}

#[test]
fn every_vocabulary_label_resolves_to_an_outline_glyph() {
    assert!(!ICONS.is_empty(), "어휘 표가 비었다");
    for (label, name) in ICONS {
        let glyph = icons::icon(label);
        assert_ne!(glyph, '\0', "{label} → {name}: 팩에 없는 이름이다");
        assert!(
            is_pua(glyph),
            "{label} → {name}: PUA 글리프가 아니다 ({glyph:?})"
        );

        // iconflow에 직접 물어본 값과 같아야 한다(스타일/크기 고정 확인).
        let expected = try_icon(Pack::Heroicons, name, Style::Outline, Size::Regular)
            .unwrap_or_else(|err| panic!("{label} → {name}: {err:?}"));
        assert_eq!(
            glyph as u32, expected.codepoint,
            "{label} → {name}: Outline/Regular 코드포인트와 다르다"
        );
        // 어휘의 모든 이름은 팩 목록에 있어야 한다(공식 콜드 패스의 전제).
        assert!(
            list(Pack::Heroicons).contains(name),
            "{label} → {name}: list()에 없다"
        );
    }
}

#[test]
fn vocabulary_labels_are_unique() {
    let mut seen = std::collections::BTreeSet::new();
    for (label, _) in ICONS {
        assert!(
            seen.insert(*label),
            "라벨 중복: {label} (뒤 항목이 앞을 가린다)"
        );
    }
}

#[test]
fn unknown_labels_have_no_icon_and_keep_their_text() {
    // 동적 라벨(탭 이름·목차 제목·문장)은 아이콘 없이 텍스트만 그린다.
    for label in [
        "alpha",
        "FreeDF GUI",
        "페이지 3",
        "PDF file path:",
        "Tab name:",
        "Close this tab?",
        "",
    ] {
        assert_eq!(icons::icon(label), '\0', "{label}에 아이콘이 붙었다");
        assert_eq!(icons::label(label), label, "{label} 라벨이 변형됐다");
    }
}

#[test]
fn swatch_labels_share_one_glyph() {
    let first = icons::icon("Swatch 1");
    assert_ne!(first, '\0');
    assert!(is_pua(first));
    for n in 1..=8 {
        assert_eq!(icons::icon(&format!("Swatch {n}")), first);
    }
    // 접두어 규칙은 "Swatch "로 시작하는 라벨만 잡는다.
    assert_eq!(icons::icon("swatch 1"), '\0');
    assert_eq!(icons::icon("Swatch"), '\0');
    // "Ink"(잉크 영역 라벨)와 같은 스와치 글리프를 쓴다.
    assert_eq!(icons::icon("Ink"), first);
}

#[test]
fn ink_row_glyphs_are_pairwise_distinct() {
    // 잉크 줄에 나란히 서는 컨트롤들은 실루엣이 겹치면 안 된다.
    let row = [
        "Pen",
        "Fountain",
        "Highlighter",
        "Eraser",
        "Thin",
        "Medium",
        "Thick",
        "Pressure",
        "Undo",
        "Redo",
    ];
    let mut seen = std::collections::BTreeMap::new();
    for label in row {
        let glyph = icons::icon(label);
        assert_ne!(glyph, '\0', "{label} 아이콘이 없다");
        if let Some(prev) = seen.insert(glyph, label) {
            panic!("{label}와 {prev}가 같은 글리프({glyph:?})를 쓴다 — 한 줄에서 구분되지 않는다");
        }
    }
    let swatch = icons::icon("Swatch 1");
    assert!(
        !seen.contains_key(&swatch),
        "스와치 칩 글리프가 도구 글리프와 겹친다"
    );
}

#[test]
fn settings_preset_glyphs_are_distinct_and_present() {
    // 설정 창의 스무딩 프리셋 — 아이콘 없이 나오던 4개(회귀 방지).
    let mut seen = std::collections::BTreeMap::new();
    for label in ["Off", "Light", "Normal", "Strong"] {
        let glyph = icons::icon(label);
        assert_ne!(glyph, '\0', "{label} 아이콘이 없다");
        if let Some(prev) = seen.insert(glyph, label) {
            panic!("{label}와 {prev}가 같은 글리프를 쓴다");
        }
    }
}

#[test]
fn labels_keep_their_contract_slugs() {
    // 계약 id는 라벨 슬러그에서 나온다 — 아이콘을 바꿔도 id는 그대로여야 한다.
    for (label, slug) in [
        ("New Tab", "new_tab"),
        ("Close Tab", "close_tab"),
        ("Open PDF", "open_pdf"),
        ("Swatch 2", "swatch_2"),
        ("Clear Ink", "clear_ink"),
        ("Save as default", "save_as_default"),
        ("Prev Page", "prev_page"),
        ("Thin", "thin"),
        ("Strong", "strong"),
    ] {
        assert_eq!(
            icons::slug(&icons::label(label)),
            slug,
            "{label} id가 바뀌었다"
        );
    }
}

#[test]
fn fonts_register_official_named_families_and_heroicons_fallback() {
    let definitions = fonts::definitions();

    // 공식 egui 예제: `fonts()`의 **모든** 폰트가 font_data + named family로 등록된다.
    for font in iconflow::fonts() {
        assert!(
            definitions.font_data.contains_key(font.family),
            "{} 폰트가 등록되지 않았다",
            font.family
        );
        assert!(
            definitions
                .families
                .contains_key(&egui::FontFamily::Name(font.family.into())),
            "{} named family가 등록되지 않았다",
            font.family
        );
    }

    // 셸용 폴백 스택은 Heroicons(아이콘) → Inter → Asta Sans.
    // 아이콘이 **맨 앞**인 이유: Inter가 PUA 745개를 덮어 Heroicons의 238개를
    // 가로채기 때문이다(뒤에 두면 아이콘이 Inter 글리프로 그려진다).
    let expected = vec![
        fonts::icon_family().to_owned(),
        fonts::INTER.to_owned(),
        fonts::ASTA_SANS.to_owned(),
    ];
    assert!(!fonts::icon_family().is_empty(), "아이콘 패밀리가 비었다");
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        let stack = definitions
            .families
            .get(&family)
            .unwrap_or_else(|| panic!("{family:?} 스택이 없다"));
        assert_eq!(stack, &expected, "{family:?} 폴백 스택이 다르다");
    }

    // 레거시 Phosphor가 어디에도 남아 있지 않아야 한다.
    for key in definitions.font_data.keys() {
        assert!(
            !key.to_ascii_lowercase().contains("phosphor"),
            "레거시 아이콘 폰트가 등록돼 있다: {key}"
        );
    }
    for (family, stack) in &definitions.families {
        for name in stack {
            assert!(
                !name.to_ascii_lowercase().contains("phosphor"),
                "{family:?} 스택에 레거시 아이콘 폰트가 있다: {name}"
            );
        }
    }
}
