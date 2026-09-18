//! `palette` 모듈 테스트 — 색 목록/이름/문자열 해석(순수 함수)만 검증한다.
//!
//! 색 값의 출처는 `freedf-services::settings`다 — 여기서는 gui가 그 목록을
//! 어떻게 읽고, 이름/HEX를 어떻게 왕복시키는지(저장 형식)만 본다.

use freedf_gui::palette;

#[test]
fn defaults_come_from_settings_service() {
    let colors = palette::defaults();
    assert_eq!(colors.len(), 3, "GoodNotes 기본 3색");
    assert_eq!(colors[0], [26, 26, 28, 255]);
    assert_eq!(colors[1], [255, 71, 66, 255]);
    assert_eq!(colors[2], [72, 166, 235, 255]);
    // 이름도 기본 3색의 저장 형식과 같다 (freedf 설정 호환).
    assert_eq!(palette::name(colors[0]), "Black");
    assert_eq!(palette::name(colors[1]), "Red");
    assert_eq!(palette::name(colors[2]), "Blue");
}

#[test]
fn normalize_truncates_to_settings_cap_and_fills_empty() {
    let many: Vec<[u8; 4]> = (0..12).map(|i| [i as u8, 0, 0, 255]).collect();
    assert_eq!(palette::normalize(many).len(), palette::SWATCHES);
    assert_eq!(
        palette::normalize(Vec::new()),
        palette::defaults(),
        "빈 목록은 기본 팔레트"
    );
}

#[test]
fn names_and_hex_round_trip() {
    for color in [[26, 26, 28, 255], [255, 71, 66, 255], [72, 166, 235, 255]] {
        let name = palette::name(color);
        assert_eq!(palette::parse(&name), Some(color), "이름 왕복: {name}");
        assert_eq!(palette::parse(&palette::hex(color)), Some(color));
    }
    // 기본 3색이 아닌 색은 `#RRGGBB`로 읽고 쓴다 (알파는 불투명으로 정규화).
    let custom = [1, 2, 3, 255];
    assert_eq!(palette::name(custom), "#010203");
    assert_eq!(palette::parse("#010203"), Some(custom));
    assert_eq!(palette::parse("010203"), Some(custom));
    // 대소문자 무시 + 알 수 없는 문자열은 None (호출자가 기본값으로 폴백).
    assert_eq!(palette::parse("black"), Some([26, 26, 28, 255]));
    assert_eq!(palette::parse("#0102"), None);
    assert_eq!(palette::parse("Nonsense"), None);
}

#[test]
fn labels_map_to_contract_ids() {
    // 라벨("Swatch N")이 계약 id(`gui.swatch_N`)의 근거다 — 1-기반.
    assert_eq!(palette::label(0), "Swatch 1");
    assert_eq!(palette::label_str(7), "Swatch 8");
    assert_eq!(palette::label_str(palette::SWATCHES), "");
    assert_eq!(palette::label_index("Swatch 1"), Some(0));
    assert_eq!(palette::label_index("Swatch 8"), Some(7));
    assert_eq!(palette::label_index("Swatch 0"), None);
    assert_eq!(palette::label_index("swatch 2"), None);
    assert_eq!(palette::label_index("Red"), None);
}

#[test]
fn index_of_finds_active_swatch() {
    let colors = palette::defaults();
    assert_eq!(palette::index_of(&colors, colors[2]), Some(2));
    assert_eq!(palette::index_of(&colors, [9, 9, 9, 255]), None);
}
