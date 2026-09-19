//! 아이콘 + 라벨 — 셸 마크업과 테스트가 **같은 문자열**을 쓰게 하는 유일한 곳.
//!
//! 아이콘은 **iconflow의 Heroicons Outline 글리프**다(`pack-heroicons` 피처 —
//! 레거시 Phosphor는 참조하지 않는다). 팩/스타일/크기 선택과 어휘 표가 이 파일에
//! 모여 있고, 폰트 설치는 [`crate::fonts`]가 한다.
//!
//! ## iconflow 공식 API 사용법
//!
//! - **콜드 패스**: [`resolve_all`]로 팩 전체를 한 번에 해석해 `list(PACK)` 순서의
//!   `Vec`을 만들고, **워밍 프레임은 그 `Vec`을 인덱스로** 읽는다. FAQ가 명시적으로
//!   권장하는 경로다(이름 키 `HashMap` 메모는 권장 경로가 아니다).
//! - 팩/스타일/크기 조합은 Heroicons에 맞는 짝을 쓴다: FAQ대로 `Style::Regular`는
//!   **없고** `Filled`/`Outline` + `Size::Regular`가 정답이다.
//! - 폰트 패밀리는 파일명이 아니라 `IconRef.family`(TTF 내부 이름)를 쓴다.
//!
//! ## 어휘를 늘릴 때
//!
//! 1. 라벨은 **화면에 보이는 버튼 문구와 정확히 같아야** 한다 — 라벨이 곧 계약 id
//!    (`gui.<슬러그>`)의 근거이고, [`label`]은 완전 일치로만 글리프를 찾는다.
//! 2. 글리프는 **광학 크기가 비슷한 실루엣**으로 고른다(아래 "고르는 기준").
//! 3. 한 줄에 나란히 서는 컨트롤끼리는 글리프가 **겹치면 안 된다** — 회귀 테스트가
//!    `tests/icons_tests.rs`에 있다(`ink_row_glyphs_are_pairwise_distinct`).
//!
//! ## 어휘를 표로 두는 이유
//!
//! Heroicons는 이름 기반 API라서 라벨→이름 매핑이 필요하다. 표([`ICONS`])가 그 단일
//! 출처다: 마크업은 라벨만 알고, 이름은 이 파일 안에만 있으며, 이름 오타는
//! `IconError::IconNotFound`로 **조용히** "아이콘 없음"이 되므로 표를 훑는 테스트
//! (`tests/icons_tests.rs`)가 유일한 안전망이다.
//!
//! 라벨은 `"글리프 텍스트"` 형태이므로 계약 id 슬러그(`gui.new_tab`)를 만들 때
//! 글리프를 걷어내야 한다 — 그 규칙은 [`slug`]가 갖는다(ASCII만 남긴다).

use std::sync::OnceLock;

use iconflow::{list, resolve_all, IconError, IconRef, Pack, Size, Style};

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
/// ## 고르는 기준: **광학 크기**(실측)
///
/// Heroicons는 24px 그리드 안에서 *의미상 작은 도형을 정말 작게* 그린다. 그대로 쓰면
/// 같은 14px에서 잉크 높이가 0.9px(`minus`)~12.4px(`paint-brush`)까지 벌어진다
/// (표준편차 2.25). 그래서 어휘는 **잉크 박스가 비슷한 실루엣**으로 고른다 —
/// 예: `minus`(9.0×0.9) 대신 `minus-circle`(11.4×11.4), `chevron-left`(5.3×9.6)
/// 대신 `arrow-left`(11.4×9.6). 이 규칙으로 현재 어휘의 잉크 높이 표준편차는 0.85다.
///
/// (아이콘이 "없어 보이거나 제각각"이던 1차 원인은 어휘가 아니라 **폰트 폴백**이었다:
/// Inter가 PUA 745개로 Heroicons 324개 중 238개를 가로챘다 — [`crate::fonts`] 참조.)
///
/// 측정/검증: `scripts/icon-audit.luau`(렌더 픽셀) + `tests/icons_tests.rs`(해석·중복).
pub const ICONS: &[(&str, &str)] = &[
    // 브랜드/내비게이션
    ("FreeDF", "sparkles"),
    ("New Tab", "document-plus"),
    ("Close Tab", "x-circle"),
    ("Open PDF", "folder-open"),
    ("Untitled", "document"),
    // 편집 — 저장/불러오기는 Heroicons의 트레이 화살표 관용 쌍.
    ("Save Edits", "arrow-down-tray"),
    ("Load Edits", "arrow-up-tray"),
    ("Undo", "arrow-uturn-left"),
    ("Redo", "arrow-uturn-right"),
    ("Delete", "trash"),
    ("Cancel", "x-circle"),
    ("OK", "check-circle"),
    ("Close", "x-circle"),
    ("Save as default", "arrow-down-tray"),
    // 보기/패널 — 사이드바·PDF·다중 북마크 전용 글리프가 없어 뜻이 가까운 것으로 잇는다.
    ("Sidebar", "view-columns"),
    ("Library", "book-open"),
    ("Bookmark", "bookmark"),
    ("Bookmarks", "rectangle-stack"),
    ("Outline", "queue-list"),
    ("Notes", "pencil-square"),
    ("PDFs", "document-text"),
    ("Recents", "clock"),
    ("Zoom In", "magnifying-glass-plus"),
    ("Zoom Out", "magnifying-glass-minus"),
    ("Fit", "arrows-pointing-out"),
    ("Prev Page", "arrow-left"),
    ("Next Page", "arrow-right"),
    // 잉크 — 도구 4종은 **서로 구분되는 실루엣**이어야 한다(한 줄에 나란히 선다).
    // 굵기 3종은 배지 계열(−⊖ / ■⊡ / +⊕)로 통일해 `minus`(0.9px)·`equals`(5.2px)처럼
    // 보이지 않는 글리프를 피한다.
    ("Ink", "swatch"),
    ("Pen", "pencil"),
    ("Fountain", "eye-dropper"),
    ("Highlighter", "paint-brush"),
    ("Eraser", "backspace"),
    ("Thin", "minus-circle"),
    ("Medium", "stop-circle"),
    ("Thick", "plus-circle"),
    ("Pressure", "adjustments-horizontal"),
    // 앱 — 스무딩 프리셋(설정 창)도 같은 규칙으로 고른다.
    ("Off", "no-symbol"),
    ("Light", "sun"),
    ("Normal", "scale"),
    ("Strong", "bolt"),
    ("Clear Ink", "trash"),
    ("Settings", "cog"),
    ("About", "information-circle"),
];

/// 색 스와치 라벨의 **접두어** — `"Swatch 1"`처럼 뒤에 번호가 붙어 표에 넣을 수 없다.
const SWATCH_PREFIX: &str = "Swatch ";
/// 색 스와치 글리프의 Heroicons 이름.
const SWATCH_NAME: &str = "swatch";

/// 콜드 패스 — 팩 전체 해석 결과(`list(PACK)`와 1:1, 순서도 같다).
///
/// iconflow 권장 경로: [`resolve_all`]을 **한 번** 부르고, 워밍 프레임은 이 `Vec`을
/// 인덱스로 읽는다(이름 키 `HashMap` 메모를 만들지 않는다).
fn resolved() -> &'static [Result<IconRef, IconError>] {
    static CACHE: OnceLock<Vec<Result<IconRef, IconError>>> = OnceLock::new();
    CACHE.get_or_init(|| resolve_all(PACK, STYLE, SIZE))
}

/// 워밍 조회용 인덱스 — (라벨, `resolved()` 인덱스). 첫 호출에 `list(PACK)`로 찾아 둔다.
///
/// 팩에 없는 이름은 표에 남지 않는다(런타임에는 `'\0'` = 아이콘 없음).
fn table() -> &'static [(&'static str, usize)] {
    static CACHE: OnceLock<Vec<(&'static str, usize)>> = OnceLock::new();
    CACHE.get_or_init(|| {
        let names = list(PACK);
        ICONS
            .iter()
            .filter_map(|(label, name)| {
                names
                    .iter()
                    .position(|candidate| candidate == name)
                    .map(|index| (*label, index))
            })
            .collect()
    })
}

/// 스와치 접두어가 가리키는 인덱스 (팩에 없으면 `None`).
fn swatch_index() -> Option<usize> {
    static CACHE: OnceLock<Option<usize>> = OnceLock::new();
    *CACHE.get_or_init(|| list(PACK).iter().position(|name| *name == SWATCH_NAME))
}

/// 팩 인덱스 → 글리프 문자 (`'\0'` = 해석 실패).
fn glyph(index: usize) -> char {
    resolved()
        .get(index)
        .and_then(|icon| icon.as_ref().ok())
        .and_then(|icon| char::from_u32(icon.codepoint))
        .unwrap_or('\0')
}

/// 텍스트 → 아이콘 글리프 (`'\0'` = 아이콘 없음).
///
/// 표에 없으면 아이콘 없이 텍스트만 그린다 — 탭 이름·목차 항목처럼 동적인 라벨이
/// 그렇다([`label`]이 텍스트 그대로 돌려준다).
pub fn icon(text: &str) -> char {
    if text.starts_with(SWATCH_PREFIX) {
        return swatch_index().map_or('\0', glyph);
    }
    table()
        .iter()
        .find(|(label, _)| *label == text)
        .map_or('\0', |(_, index)| glyph(*index))
}

/// 아이콘 폰트 패밀리 이름 (TTF 내부 이름) — [`crate::fonts`]가 폰트 등록에 쓴다.
///
/// 파일명이 아니라 `IconRef.family`를 써야 한다(iconflow FAQ). 팩 전체 해석 결과에서
/// 얻으므로 팩/스타일을 바꿔도 자동으로 따라온다. 어휘가 전부 깨졌으면 `""`이고,
/// 그때는 `tests/icons_tests.rs`가 잡는다.
pub fn family() -> &'static str {
    resolved()
        .iter()
        .find_map(|icon| icon.as_ref().ok())
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
