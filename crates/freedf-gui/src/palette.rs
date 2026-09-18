//! 즐겨찾기 색 팔레트 — 색 목록/이름/문자열 해석 (순수 함수만).
//!
//! 색의 **출처는 `freedf-services::settings`** 하나다: `PanelsState::default()`가
//! GoodNotes 기본 3색(블랙/레드/블루)을 주고 `MAX_FAVORITE_COLORS`(8)가 상한이다.
//! freedf-gui는 그 목록을 읽어 스와치를 그리고, 이름 해석만 여기서 한다 —
//! 리본 버튼 라벨/설정 저장(`InkDefaults`)이 **같은 이름**을 쓰게 하려는 것.
//!
//! 색 이름은 **저장 형식**이다: `InkDefaults.color`에 문자열로 들어가므로
//! 기본 3색 이름(Black/Red/Blue)은 freedf 설정과 같게 유지한다 (바꾸면 이전
//! 설정 파일이 기본값으로 떨어진다). 그 외 색은 `#RRGGBB`로 읽고 쓴다.

use freedf_services::settings::{PanelsState, MAX_FAVORITE_COLORS};

/// 스와치 최대 개수 — settings 상한을 그대로 쓴다 (출처 하나).
pub const SWATCHES: usize = MAX_FAVORITE_COLORS;

/// 기본 즐겨찾기 색 — settings 기본값을 그대로 (복사본).
pub fn defaults() -> Vec<[u8; 4]> {
    PanelsState::default().favorite_colors
}

/// 목록을 상한까지 자르고, 비면 기본값으로 채운다 (settings 정규화와 같은 규칙).
pub fn normalize(colors: Vec<[u8; 4]>) -> Vec<[u8; 4]> {
    let mut colors = colors;
    colors.truncate(SWATCHES);
    if colors.is_empty() {
        colors = defaults();
    }
    colors
}

/// 스와치 라벨 = 계약 id의 근거 (`Swatch 1` → `gui.swatch_1`).
///
/// `&'static str` 표로 둔다: 셸(`view!`)의 클로저가 **복사만** 하게 하려는 것
/// (문자열을 빌리면 클로저가 `'static`을 못 만족해 매크로 생성 코드가 깨진다).
pub const LABELS: [&str; SWATCHES] = [
    "Swatch 1",
    "Swatch 2",
    "Swatch 3",
    "Swatch 4",
    "Swatch 5",
    "Swatch 6",
    "Swatch 7",
    "Swatch 8",
];

/// 인덱스 → 스와치 라벨 (범위 밖이면 빈 문자열).
pub fn label_str(index: usize) -> &'static str {
    LABELS.get(index).copied().unwrap_or("")
}

/// 인덱스 → 스와치 라벨 (소유 문자열 — 커맨드/테스트용).
pub fn label(index: usize) -> String {
    String::from(label_str(index))
}

/// 라벨("Swatch N") → 0-기반 인덱스 (모르면 `None`).
pub fn label_index(text: &str) -> Option<usize> {
    let n: usize = text.trim().strip_prefix("Swatch ")?.trim().parse().ok()?;
    if n == 0 {
        None
    } else {
        Some(n - 1)
    }
}

/// 색 이름 — 기본 3색은 이름, 그 외는 `#RRGGBB`.
pub fn name(color: [u8; 4]) -> String {
    match color {
        [26, 26, 28, 255] => String::from("Black"),
        [255, 71, 66, 255] => String::from("Red"),
        [72, 166, 235, 255] => String::from("Blue"),
        _ => hex(color),
    }
}

/// `#RRGGBB` 문자열 (알파는 버린다 — 팔레트 색은 불투명하다).
pub fn hex(color: [u8; 4]) -> String {
    format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2])
}

/// 문자열 → 색: 이름(대소문자 무시) 또는 `#RRGGBB`/`RRGGBB`. 모르면 `None`.
pub fn parse(text: &str) -> Option<[u8; 4]> {
    match text.trim().to_ascii_lowercase().as_str() {
        "black" => return Some([26, 26, 28, 255]),
        "red" => return Some([255, 71, 66, 255]),
        "blue" => return Some([72, 166, 235, 255]),
        _ => {}
    }
    let digits = text.trim().trim_start_matches('#');
    if digits.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(digits, 16).ok()?;
    Some([
        ((v >> 16) & 0xFF) as u8,
        ((v >> 8) & 0xFF) as u8,
        (v & 0xFF) as u8,
        255,
    ])
}

/// 목록에서 그 색의 위치 (없으면 `None` — 활성 표시에 쓴다).
pub fn index_of(colors: &[[u8; 4]], color: [u8; 4]) -> Option<usize> {
    colors.iter().position(|c| *c == color)
}
