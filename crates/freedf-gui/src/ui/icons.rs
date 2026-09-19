//! 아이콘 + 라벨 — 셸 마크업과 테스트가 **같은 문자열**을 쓰게 하는 유일한 곳.
//!
//! 아이콘은 이모지가 아니라 Phosphor 글리프(`egui_phosphor_icons`)다. 라벨은
//! `"글리프 텍스트"` 형태이므로 계약 id 슬러그(`gui.new_tab`)를 만들 때 글리프를
//! 걷어내야 한다 — 그 규칙은 `shell`이 갖는다(ASCII만 남긴다).

use egui_phosphor_icons::icons as ph;

/// 텍스트 → Phosphor 글리프 (표에 없으면 빈 문자열 = 아이콘 없음).
///
/// 표가 곧 freedf-gui의 아이콘 어휘다 — 아이콘을 바꾸려면 여기 한 줄만 고치면
/// 되고, 마크업·테스트·계약 id는 그대로 따라온다.
pub fn icon(text: &str) -> &'static str {
    match text {
        // 브랜드/내비게이션
        "FreeDF" => ph::FEATHER.0,
        "New Tab" => ph::FILE_PLUS.0,
        "Close Tab" => ph::X.0,
        "Open PDF" => ph::FOLDER_OPEN.0,
        "Untitled" => ph::FILE_TEXT.0,
        // 편집
        "Save Edits" => ph::FLOPPY_DISK.0,
        "Load Edits" => ph::FOLDER_SIMPLE.0,
        "Undo" => ph::ARROW_COUNTER_CLOCKWISE.0,
        "Redo" => ph::ARROW_CLOCKWISE.0,
        "Delete" => ph::TRASH.0,
        "Cancel" => ph::X.0,
        "OK" => ph::CHECK.0,
        "Close" => ph::X.0,
        "Save as default" => ph::FLOPPY_DISK.0,
        // 보기/패널
        "Sidebar" => ph::SIDEBAR_SIMPLE.0,
        "Library" => ph::BOOKS.0,
        "Bookmark" => ph::BOOKMARK.0,
        "Bookmarks" => ph::BOOKMARKS.0,
        "Outline" => ph::LIST.0,
        "Notes" => ph::NOTE_PENCIL.0,
        "PDFs" => ph::FILE_PDF.0,
        "Recents" => ph::CLOCK.0,
        "Zoom In" => ph::MAGNIFYING_GLASS_PLUS.0,
        "Zoom Out" => ph::MAGNIFYING_GLASS_MINUS.0,
        "Fit" => ph::ARROW_SQUARE_OUT.0,
        "Prev Page" => ph::CARET_LEFT.0,
        "Next Page" => ph::CARET_RIGHT.0,
        // 잉크
        "Ink" => ph::PALETTE.0,
        "Pen" => ph::PEN_NIB.0,
        "Fountain" => ph::PEN.0,
        "Highlighter" => ph::HIGHLIGHTER.0,
        "Eraser" => ph::ERASER.0,
        "Thin" => ph::MINUS.0,
        "Medium" => ph::EQUALS.0,
        "Thick" => ph::PLUS.0,
        "Pressure" => ph::GAUGE.0,
        // 앱
        "Clear Ink" => ph::TRASH.0,
        "Settings" => ph::GEAR.0,
        "About" => ph::INFO.0,
        // 색 스와치 — 색마다 다른 글리프가 아니라 하나의 **채워진 점** (색은 CSS가 칠한다).
        // `CIRCLE`(윤곽선)을 쓰면 감사가 위젯 중심 픽셀에서 원 **안쪽**(= 바탕색 혼합)을
        // 샘플해 대비가 1.75로 오측정된다 — 채워진 점은 중심이 글자색이라 정확히 측정된다.
        _ if text.starts_with("Swatch ") => ph::DOT.0,
        _ => "",
    }
}

/// 라벨 = 아이콘 + 공백 + 텍스트. 아이콘이 없으면 텍스트 그대로.
pub fn label(text: &str) -> String {
    match icon(text) {
        "" => text.to_string(),
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
