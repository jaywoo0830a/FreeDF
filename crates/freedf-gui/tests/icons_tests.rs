//! 아이콘 계약 — **어휘 전량이 Heroicons Outline 글리프로 해석되는가**.
//!
//! 이 파일이 "레거시 아이콘 라이브러리 미참조"의 증거다: 폰트 스택에 Phosphor가
//! 없고, 표(`ui::icons::ICONS`)의 모든 이름이 iconflow Heroicons에서 해석된다.
//!
//! 없는 이름은 조용히 "아이콘 없음"(`'\0'`)이 되므로(런타임 오류 없음) 여기서
//! 전량을 훑는 것이 유일한 안전망이다 — 아이콘을 바꾸면 이 테스트가 먼저 깨진다.

use eframe::egui;
use freedf_gui::fonts;
use freedf_gui::ui::icons::{self, ICONS};
use iconflow::{try_icon, Pack, Size, Style};

/// Heroicons 글리프는 사설 영역(PUA)에 있다 — 라틴/한글 폰트와 겹치지 않는다.
fn is_pua(c: char) -> bool {
    (0xE000..=0xF8FF).contains(&(c as u32))
}

#[test]
fn every_vocabulary_label_resolves_to_an_outline_glyph() {
    assert!(!ICONS.is_empty(), "어휘 표가 비었다");
    for (label, name) in ICONS {
        let glyph = icons::icon(label);
        assert_ne!(glyph, '\0', "{label} → {name}: 팩에 없는 이름이다");
        assert!(is_pua(glyph), "{label} → {name}: PUA 글리프가 아니다 ({glyph:?})");

        // iconflow에 직접 물어본 값과 같아야 한다(스타일/크기 고정 확인).
        let expected = try_icon(Pack::Heroicons, name, Style::Outline, Size::Regular)
            .unwrap_or_else(|err| panic!("{label} → {name}: {err:?}"));
        assert_eq!(
            glyph as u32, expected.codepoint,
            "{label} → {name}: Outline/Regular 코드포인트와 다르다"
        );
    }
}

#[test]
fn vocabulary_labels_are_unique() {
    let mut seen = std::collections::BTreeSet::new();
    for (label, _) in ICONS {
        assert!(seen.insert(*label), "라벨 중복: {label} (뒤 항목이 앞을 가린다)");
    }
}

#[test]
fn unknown_labels_have_no_icon_and_keep_their_text() {
    // 동적 라벨(탭 이름·스무딩 프리셋)은 아이콘 없이 텍스트만 그린다.
    for label in ["alpha", "Light", "Normal", "Strong", "Off", "FreeDF GUI", ""] {
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
    // "Ink"(잉크 영역 라벨)과 같은 스와치 글리프를 쓴다.
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
    ] {
        assert_eq!(icons::slug(&icons::label(label)), slug, "{label} id가 바뀌었다");
    }
}

#[test]
fn font_stack_uses_heroicons_and_has_no_phosphor() {
    let definitions = fonts::definitions();
    let icon_family = fonts::icon_family();
    assert!(!icon_family.is_empty(), "아이콘 폰트 패밀리가 비었다");

    // 폰트 데이터: 아이콘 TTF가 등록돼 있어야 한다.
    assert!(
        definitions.font_data.contains_key(icon_family),
        "아이콘 폰트({icon_family})가 등록되지 않았다: {:?}",
        definitions.font_data.keys().collect::<Vec<_>>()
    );

    // 비례/고정 스택 둘 다 같은 3단 폴백을 쓴다(아이콘은 마지막).
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        let stack = definitions
            .families
            .get(&family)
            .unwrap_or_else(|| panic!("{family:?} 스택이 없다"));
        assert_eq!(
            stack,
            &vec![fonts::INTER.to_owned(), fonts::ASTA_SANS.to_owned(), icon_family.to_owned()],
            "{family:?} 스택이 Inter → Asta Sans → Heroicons 순서가 아니다"
        );
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
