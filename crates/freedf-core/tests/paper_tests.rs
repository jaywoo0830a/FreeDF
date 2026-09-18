//! `paper` 모듈 단위 테스트 — `src/paper.rs`의 `#[cfg(test)]` 모듈에서 이동했습니다.

use freedf_core::paper::*;

#[test]
fn ruled_lines_follow_page_rotation() {
    use freedf_core::text::PageRotation;
    // 0/180°: 가로줄 (y 고정), 90/270°: 세로줄 (x 고정).
    let horiz = paper_lines_rotated(100.0, 200.0, PaperStyle::Ruled, 50.0, PageRotation::None);
    assert_eq!(horiz.len(), 3, "h=200/50 → 3줄");
    assert!(horiz
        .iter()
        .all(|l| l[1] == l[3] && l[0] == 0.0 && l[2] == 100.0));
    let vert = paper_lines_rotated(
        100.0,
        200.0,
        PaperStyle::Ruled,
        50.0,
        PageRotation::Degrees90,
    );
    assert_eq!(vert.len(), 1, "w=100/50 → 1줄");
    assert!(vert
        .iter()
        .all(|l| l[0] == l[2] && l[1] == 0.0 && l[3] == 200.0));
    let vert2 = paper_lines_rotated(
        100.0,
        200.0,
        PaperStyle::Ruled,
        50.0,
        PageRotation::Degrees270,
    );
    assert_eq!(vert2, vert);
    // 180°는 가로줄 집합 그대로 (위상 무관).
    let horiz180 = paper_lines_rotated(
        100.0,
        200.0,
        PaperStyle::Ruled,
        50.0,
        PageRotation::Degrees180,
    );
    assert_eq!(horiz180, horiz);
    // Grid/Blank는 회전에 불변 (기존 함수와 동일).
    assert_eq!(
        paper_lines_rotated(
            100.0,
            200.0,
            PaperStyle::Grid,
            50.0,
            PageRotation::Degrees90
        ),
        paper_lines(100.0, 200.0, PaperStyle::Grid, 50.0)
    );
}

#[test]
fn style_settings_are_independent_per_style() {
    // Ruled/Grid/Dotted 각각 독립 — 한 스타일을 바꿔도 다른 스타일은 그대로.
    let mut s = PaperStyleSettings::default();
    let custom = LineStyle {
        spacing: 60.0,
        color: [200, 10, 10, 255],
        width: 3.0,
    };
    s.set(PaperStyle::Grid, custom);
    assert_eq!(s.of(PaperStyle::Grid), Some(custom));
    assert_eq!(s.of(PaperStyle::Ruled), Some(LineStyle::default()));
    assert_eq!(s.of(PaperStyle::Dotted), Some(LineStyle::default()));
    // Blank는 줄이 없음.
    assert_eq!(s.of(PaperStyle::Blank), None);
    // 직렬화 왕복.
    let json = serde_json::to_string(&s).unwrap();
    assert_eq!(
        serde_json::from_str::<PaperStyleSettings>(&json).unwrap(),
        s
    );
    // 빈 객체 → 기본값 (이전 세션 호환).
    let d: PaperStyleSettings = serde_json::from_str("{}").unwrap();
    assert_eq!(d, PaperStyleSettings::default());
}

#[test]
fn blank_has_no_lines_or_dots() {
    assert!(paper_lines(595.0, 842.0, PaperStyle::Blank, 24.0).is_empty());
    assert!(paper_dots(595.0, 842.0, PaperStyle::Blank, 24.0).is_empty());
}

#[test]
fn ruled_only_horizontal() {
    let lines = paper_lines(595.0, 100.0, PaperStyle::Ruled, 24.0);
    assert!(!lines.is_empty());
    for l in &lines {
        // 가로줄: y0 == y1, 페이지 폭을 가로지름
        assert!((l[1] - l[3]).abs() < 1e-3);
        assert!((l[0]).abs() < 1e-3 && (l[2] - 595.0).abs() < 1e-3);
    }
}

#[test]
fn grid_has_horizontal_and_vertical() {
    let lines = paper_lines(595.0, 100.0, PaperStyle::Grid, 24.0);
    assert!(!lines.is_empty());
    let horiz = lines.iter().filter(|l| (l[1] - l[3]).abs() < 1e-3).count();
    let vert = lines.iter().filter(|l| (l[0] - l[2]).abs() < 1e-3).count();
    assert!(horiz > 0);
    assert!(vert > 0);
}

#[test]
fn dotted_has_dots_not_lines() {
    assert!(paper_lines(100.0, 100.0, PaperStyle::Dotted, 24.0).is_empty());
    let dots = paper_dots(100.0, 100.0, PaperStyle::Dotted, 24.0);
    assert!(!dots.is_empty());
    for d in &dots {
        assert!(d[0] > 0.0 && d[1] > 0.0);
    }
}

#[test]
fn paper_sizes_are_portrait_and_positive() {
    for size in PaperSize::all() {
        let [w, h] = size.size_pts();
        assert!(w > 0.0 && h > w, "{size:?} should be portrait");
    }
    assert!(PaperSize::A5.size_pts()[0] < PaperSize::A4.size_pts()[0]);
    assert!(PaperSize::A4.size_pts()[0] < PaperSize::A3.size_pts()[0]);
}

#[test]
fn matching_finds_nearest_size() {
    assert_eq!(PaperSize::matching(595.0, 842.0), PaperSize::A4);
    assert_eq!(PaperSize::matching(419.0, 595.0), PaperSize::A5);
    assert_eq!(PaperSize::matching(612.0, 792.0), PaperSize::Letter);
    assert_eq!(PaperSize::matching(612.0, 1008.0), PaperSize::Legal);
}

#[test]
fn page_paper_default_is_blank_white() {
    let p = PagePaper::default();
    assert_eq!(p.style, PaperStyle::Blank);
    assert_eq!(p.color, PAPER_WHITE);
}

/// 테스트용 베이크 헬퍼 (문서 §7의 CPU 베이킹과 동일 경로).
fn bake(size: usize, base: [u8; 3], strength: f32, settings: PaperSurfaceSettings) -> Vec<u8> {
    paper_texture_rgba(size, base, strength, &settings, 7)
}

#[test]
fn texture_is_deterministic() {
    let a = bake(64, [255, 255, 255], 0.35, PaperSurfaceSettings::default());
    let b = bake(64, [255, 255, 255], 0.35, PaperSurfaceSettings::default());
    assert_eq!(a, b, "같은 입력은 항상 같은 질감 (깜빡임 금지)");
    assert_eq!(a.len(), 64 * 64 * 4);
}

#[test]
fn zero_strength_is_plain_paper() {
    // strength 0 → 텍스처가 종이 색 그 자체 (감마 왕복 ±1 이내).
    let out = bake(32, [240, 230, 210], 0.0, PaperSurfaceSettings::default());
    for px in out.chunks_exact(4) {
        assert!((px[0] as i32 - 240).abs() <= 1, "r={}", px[0]);
        assert!((px[1] as i32 - 230).abs() <= 1, "g={}", px[1]);
        assert!((px[2] as i32 - 210).abs() <= 1, "b={}", px[2]);
        assert_eq!(px[3], 255, "불투명");
    }
}

#[test]
fn flat_surface_is_uniform() {
    // β=0, 반사율 요동 0, 차폐 0, 시닝 0 → 평면이므로 모든 픽셀이 ρ0.
    // (노출 정규화 덕분에 평면에서 C = ρ가 정확히 성립해야 함 — 문서 §5)
    let flat = PaperSurfaceSettings {
        bump: 0.0,
        albedo_l: 0.0,
        albedo_c: 0.0,
        ao_strength: 0.0,
        sheen: 0.0,
        ..PaperSurfaceSettings::default()
    };
    let out = bake(32, [250, 240, 230], 1.0, flat);
    for px in out.chunks_exact(4) {
        assert!((px[0] as i32 - 250).abs() <= 1, "r={}", px[0]);
        assert!((px[1] as i32 - 240).abs() <= 1, "g={}", px[1]);
        assert!((px[2] as i32 - 230).abs() <= 1, "b={}", px[2]);
    }
}

#[test]
fn bump_creates_relief() {
    let flat = PaperSurfaceSettings {
        bump: 0.0,
        albedo_l: 0.0,
        albedo_c: 0.0,
        ao_strength: 0.0,
        sheen: 0.0,
        ..PaperSurfaceSettings::default()
    };
    let relief = PaperSurfaceSettings { bump: 1.0, ..flat };
    let a = bake(48, [255, 255, 255], 1.0, flat);
    let b = bake(48, [255, 255, 255], 1.0, relief);
    assert_ne!(a, b, "β>0이면 음영이 생겨 달라야 함");
    // 음영 분산이 실제로 존재해야 입체감이 있음.
    let mut min = 255u8;
    let mut max = 0u8;
    for px in b.chunks_exact(4) {
        min = min.min(px[0]);
        max = max.max(px[0]);
    }
    assert!(max - min > 8, "음영 범위가 너무 좁음: {min}..{max}");
}

#[test]
fn light_rotation_changes_shading() {
    let a = PaperSurfaceSettings {
        light_azimuth_deg: 0.0,
        ..PaperSurfaceSettings::default()
    };
    let b = PaperSurfaceSettings {
        light_azimuth_deg: 180.0,
        ..PaperSurfaceSettings::default()
    };
    let ta = bake(48, [255, 255, 255], 1.0, a);
    let tb = bake(48, [255, 255, 255], 1.0, b);
    assert_ne!(ta, tb, "광원을 반대로 돌리면 음영이 달라져야 함");
}

#[test]
fn texture_depends_on_base_color() {
    let white = bake(32, [255, 255, 255], 0.35, PaperSurfaceSettings::default());
    let cream = bake(32, [251, 243, 220], 0.35, PaperSurfaceSettings::default());
    assert_ne!(
        white, cream,
        "종이 배경색에 따라 텍스처가 달라야 함 (요구 ①)"
    );
}

#[test]
fn presets_are_monotonic_and_medium_is_default() {
    // 단계가 올라갈수록 강도·요철이 커져야 합니다.
    let mut prev = paper_texture_preset(0);
    assert_eq!("Lowest", paper_texture_preset_label(0));
    assert_eq!("Highest", paper_texture_preset_label(4));
    for level in 1..5u8 {
        let cur = paper_texture_preset(level);
        assert!(cur.0 > prev.0, "강도 단조 증가: {level}");
        assert!(cur.1.bump > prev.1.bump, "요철 단조 증가: {level}");
        prev = cur;
    }
    // Medium(2)는 은은한 기본값과 정확히 일치해야 합니다.
    assert_eq!(
        paper_texture_preset(2),
        (0.25, PaperSurfaceSettings::default())
    );
}

#[test]
fn paper_field_is_tileable() {
    // 모든 옥타브가 토러스 격자(셀 수 나머지 연산)를 쓰므로 타일
    // 경계(u/v = 0 ↔ 1)에서 값이 정확히 같아야 합니다 — 이음새 없음.
    let seed = 9;
    for i in 0..20 {
        let v = i as f32 / 19.0;
        let a = paper_field(0.0, v, seed);
        let b = paper_field(1.0, v, seed);
        assert!((a - b).abs() < 1e-5, "u 경계 이음새: {a} vs {b}");
        let u = i as f32 / 19.0;
        let a = paper_field(u, 0.0, seed);
        let b = paper_field(u, 1.0, seed);
        assert!((a - b).abs() < 1e-5, "v 경계 이음새: {a} vs {b}");
    }
}
