//! 아이콘 + 라벨 — 셸 마크업과 테스트가 **같은 문자열**을 쓰게 하는 유일한 곳.
//!
//! 아이콘은 이모지가 아니라 **iconflow의 Heroicons Outline 글리프**다
//! (`iconflow` 크레이트, `pack-heroicons` 피처 — 레거시 Phosphor는 참조하지 않는다).
//! 팩/스타일/크기 선택과 어휘 표가 이 파일에 모여 있고, 폰트 설치는 [`crate::fonts`]가 한다.
//!
//! 라벨은 `"글리프 텍스트"` 형태이므로 계약 id 슬러그(`gui.new_tab`)를 만들 때
//! 글리프를 걷어내야 한다 — 그 규칙은 [`slug`]가 갖는다(ASCII만 남긴다).
//!
//! ## 어휘를 표로 두는 이유
//!
//! Heroicons는 이름 기반 API(`try_icon(pack, name, style, size)`)라서 라벨→이름
//! 매핑이 필요하다. 표([`ICONS`])가 그 단일 출처다:
//!
//! - 마크업은 라벨만 안다(이름은 이 파일 안에만 있다).
//! - 이름 오타는 표를 훑는 테스트(`tests/icons_tests.rs`)가 잡는다 — 없는 이름은
//!   조용히 "아이콘 없음"이 되므로 테스트가 유일한 안전망이다.
//! - 스타일/팩을 바꿀 때 고칠 곳이 이 파일 하나다.

use std::collections::HashMap;
use std::sync::OnceLock;

use iconflow::{try_icon, IconRef, Pack, Size, Style};

/// 아이콘 팩 — freedf-gui는 Heroicons만 쓴다.
const PACK: Pack = Pack::Heroicons;
/// 아이콘 스타일 — 디자인 시스템이 아웃라인(헤어라인)이라 **Outline** 고정.
///
/// Heroicons의 `Filled`와 `Outline`은 **같은 코드포인트**를 쓴다(실측 318/324).
/// 그래서 폰트 폴백 스택에는 한 스타일만 넣을 수 있고, 여기서 Outline을 고른다.
const STYLE: Style = Style::Outline;
/// 아이콘 크기 변형 — Heroicons는 `Size::Regular`(24px 그리드)를 쓴다.
const SIZE: Size = Size::Regular;

/// freedf-gui의 **아이콘 어휘** — (라벨, Heroicons 이름).
///
/// 라벨은 계약 id의 출처라 **바꾸면 자동화 계약이 깨진다**(`docs/eguidev-automation.md`).
/// 아이콘을 바꾸려면 둘째 열만 고친다 — 라벨/슬러그/테스트는 그대로 따라온다.
///
/// Heroicons에 전용 글리프가 없는 것들은 뜻이 가까운 이름으로 잇는다(주석 참조).
pub const ICONS: &[(&str, &str)] = &[
    // 브랜드/내비게이션
    ("FreeDF", "sparkles"),
    ("New Tab", "document-plus"),
    ("Close Tab", "x-mark"),
    ("Open PDF", "folder-open"),
    ("Untitled", "document"),
    // 편집 — Heroicons의 저장/불러오기 관용 글리프는 트레이 화살표 쌍이다.
    ("Save Edits", "arrow-down-tray"),
    ("Load Edits", "arrow-up-tray"),
    ("Undo", "arrow-uturn-left"),
    ("Redo", "arrow-uturn-right"),
    ("Delete", "trash"),
    ("Cancel", "x-mark"),
    ("OK", "check"),
    ("Close", "x-mark"),
    ("Save as default", "arrow-down-tray"),
    // 보기/패널 — 사이드바·PDF·다중 북마크 전용 글리프가 없어 뜻이 가까운 것으로 잇는다.
    ("Sidebar", "view-columns"),
    ("Library", "book-open"),
    ("Bookmark", "bookmark"),
    ("Bookmarks", "rectangle-stack"),
    ("Outline", "list-bullet"),
    ("Notes", "pencil-square"),
    ("PDFs", "document-text"),
    ("Recents", "clock"),
    ("Zoom In", "magnifying-glass-plus"),
    ("Zoom Out", "magnifying-glass-minus"),
    ("Fit", "arrows-pointing-out"),
    ("Prev Page", "chevron-left"),
    ("Next Page", "chevron-right"),
    // 잉크 — 도구 4종은 **서로 구분되는 실루엣**이어야 한다(한 줄에 나란히 선다).
    // Heroicons에 만년필/형광펜/지우개가 없어 eye-dropper/paint-brush/backspace로 잇는다.
    ("Ink", "swatch"),
    ("Pen", "pencil"),
    ("Fountain", "eye-dropper"),
    ("Highlighter", "paint-brush"),
    ("Eraser", "backspace"),
    ("Thin", "minus"),
    ("Medium", "equals"),
    ("Thick", "plus"),
    ("Pressure", "adjustments-horizontal"),
    // 앱
    ("Clear Ink", "trash"),
    ("Settings", "cog"),
    ("About", "information-circle"),
];

/// 색 스와치 라벨의 **접두어** — `"Swatch 1"`처럼 뒤에 번호가 붙어 표에 넣을 수 없다.
const SWATCH_PREFIX: &str = "Swatch ";
/// 색 스와치 글리프의 Heroicons 이름.
const SWATCH_NAME: &str = "swatch";

/// 아이콘 이름 → 해석 결과. **팩에 없는 이름은 아예 담기지 않는다**(= `'\0'`).
///
/// 이 함수는 위젯마다 매 프레임 불리므로 해석은 첫 호출에 한 번만 한다
/// (`try_icon`은 이름 하나당 표를 선형으로 훑는다 — 318개).
fn resolved() -> &'static HashMap<&'static str, IconRef> {
    static CACHE: OnceLock<HashMap<&'static str, IconRef>> = OnceLock::new();
    CACHE.get_or_init(|| {
        ICONS
            .iter()
            .map(|(_, name)| *name)
            .chain(std::iter::once(SWATCH_NAME))
            .filter_map(|name| {
                try_icon(PACK, name, STYLE, SIZE)
                    .ok()
                    .map(|icon| (name, icon))
            })
            .collect()
    })
}

/// Heroicons 이름 → 글리프 문자 (`'\0'` = 팩에 없음).
fn glyph(name: &str) -> char {
    resolved()
        .get(name)
        .and_then(|icon| char::from_u32(icon.codepoint))
        .unwrap_or('\0')
}

/// 텍스트 → 아이콘 글리프 (`'\0'` = 아이콘 없음).
///
/// 표에 없으면 아이콘 없이 텍스트만 그린다 — 탭 이름·스무딩 프리셋처럼 동적인
/// 라벨이 그렇다([`label`]이 텍스트 그대로 돌려준다).
pub fn icon(text: &str) -> char {
    if text.starts_with(SWATCH_PREFIX) {
        return glyph(SWATCH_NAME);
    }
    ICONS
        .iter()
        .find(|(label, _)| *label == text)
        .map_or('\0', |(_, name)| glyph(name))
}

/// 아이콘 폰트 패밀리 이름 (TTF 내부 이름) — [`crate::fonts`]가 폴백 스택에 넣는다.
///
/// 파일명이 아니라 `IconRef.family`를 써야 한다(iconflow FAQ). 어휘의 아무 항목이나
/// 해석해 얻으므로 팩/스타일을 바꿔도 자동으로 따라온다. 어휘가 전부 깨졌으면 `""`이고,
/// 그때는 `tests/icons_tests.rs`가 잡는다.
pub fn family() -> &'static str {
    resolved()
        .values()
        .next()
        .map_or("", |icon| icon.family)
}

/// 라벨 = 아이콘 + 공백 + 텍스트. 아이콘이 없으면 텍스트 그대로.
pub fn label(text: &str) -> String {
    match icon(text) {
        '\0' => text.to_string(),
        glyph => format!("{glyph} {text}"),
    }
}

/// 계약 id 슬러그 — 라벨에서 아이콘/비ASCII를 걷어내고 ASCII만 남긴다.
///
/// `"글리프 New Tab"` → `"new_tab"` 이라 id는 아이콘 도입 전과 **같다**
/// (`docs/eguidev-automation.md`의 `gui.<라벨 슬러그>` 규칙).
pub fn slug(label: &str) -> String {
    let mut out = String::new();
    let mut last_space = true;
    for c in label.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            last_space = false;
        } else if !last_space {
            out.push('_');
            last_space = true;
        }
    }
    out.trim_matches('_').to_string()
}
