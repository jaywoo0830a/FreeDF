//! `pen` 모듈 단위 테스트 — `src/pen.rs`의 `#[cfg(test)]` 모듈에서 이동했습니다.

use freedf_core::model::{StrokePoint, ToolType};
use freedf_core::pen::*;

#[test]
fn required_families_have_swatches() {
    for family in [ColorFamily::Red, ColorFamily::Blue, ColorFamily::Black] {
        let s = Palette::swatches(family);
        assert!(
            s.len() >= 3,
            "{} 계열은 3개 이상 스와치 필요",
            family.label()
        );
        for c in &s {
            assert_eq!(c.len(), 4);
            assert!(c[3] > 0, "알파는 0이 아니어야 함");
        }
    }
}

#[test]
fn red_is_reddish_and_black_is_dark() {
    for c in Palette::swatches(ColorFamily::Red) {
        assert!(c[0] > c[1] && c[0] > c[2], "빨강 계열: R이 지배적이어야 함");
    }
    for c in Palette::swatches(ColorFamily::Black) {
        assert!(
            c[0] < 130 && c[1] < 130 && c[2] < 130,
            "검정 계열은 어두워야 함"
        );
    }
}

// ---------- OneEuroFilter ----------

#[test]
fn one_euro_converges_on_constant_signal() {
    let mut f = OneEuroFilter::from_smoothing(0.5);
    let mut out = 10.0f32;
    for i in 0..200 {
        out = f.filter(10.0, i as f64 * 0.016); // 60Hz
    }
    assert!((out - 10.0).abs() < 0.05, "상수 입력에 수렴해야: {out}");
}

#[test]
fn one_euro_reduces_jitter() {
    // ~2.5Hz 손떨림 잡음 (세밀한 흔들림). smoothing을 높이면 진폭이 더 줄어야 합니다.
    let raw: Vec<f32> = (0..400)
        .map(|i| 50.0 + (i as f32 * 0.25).sin() * 2.0)
        .collect();
    let run = |s: f32| {
        let mut f = OneEuroFilter::from_smoothing(s);
        raw.iter()
            .enumerate()
            .map(|(i, &x)| f.filter(x, i as f64 * 0.016))
            .collect::<Vec<f32>>()
    };
    let var = |v: &[f32]| -> f32 {
        let m = v.iter().sum::<f32>() / v.len() as f32;
        v.iter().map(|x| (x - m).powi(2)).sum::<f32>() / v.len() as f32
    };
    let out0 = run(0.0);
    let out1 = run(1.0);
    assert!(var(&out1) < var(&raw), "스무딩이 잡음을 줄여야");
    assert!(
        var(&out1) < var(&out0) * 0.75,
        "smoothing 1이 smoothing 0보다 잡음을 더 줄여야: {} vs {}",
        var(&out1),
        var(&out0)
    );
}

#[test]
fn one_euro_tracks_fast_steps() {
    // 계단 입력(빠른 이동)은 smoothing이 커도 지연 없이 따라가야 합니다
    // (beta가 속도에 적응해 컷오프를 올려줌).
    for s in [0.0, 0.5, 1.0] {
        let mut f = OneEuroFilter::from_smoothing(s);
        for i in 0..100 {
            let _ = f.filter(0.0, i as f64 * 0.008);
        }
        let mut out = 0.0f32;
        for i in 0..120 {
            out = f.filter(100.0, (100 + i) as f64 * 0.008);
        }
        assert!(out > 95.0, "s={s}: 스텝 추적 실패: {out}");
    }
}

#[test]
fn one_euro_reset_restarts_tracking() {
    let mut f = OneEuroFilter::from_smoothing(0.6);
    f.filter(1000.0, 0.0);
    f.reset();
    let y = f.filter(10.0, 0.1);
    assert!((y - 10.0).abs() < 1e-4, "reset 후 첫 값은 원본 그대로: {y}");
}

// ---------- BallPenProfile (일반 펜 물리 모델) ----------

#[test]
fn ballpen_pressure_curve_is_strong_and_bounded() {
    let p = BallPenProfile::default(); // base 1.0, min_ratio 0.2, max_ratio 1.35
    let light = p.width_at(1.0, 0.0, 0.0, 0.0);
    let feather = p.width_at(1.0, 0.1, 0.0, 0.0);
    let mid = p.width_at(1.0, 0.5, 0.0, 0.0);
    let full = p.width_at(1.0, 1.0, 0.0, 0.0);
    assert!(
        light >= 0.2 - 1e-4 && full <= 1.35 + 1e-4,
        "범위: {light}..{full}"
    );
    assert!(feather < 0.4, "살짝 닿으면 가늘어야: {feather}");
    assert!(
        feather < mid && mid < full,
        "필압 커브 단조 증가: {feather} {mid} {full}"
    );
    // 어떤 입력에서도 유한·범위 내.
    for (pr, v) in [(0.0f32, 600.0), (0.5, 600.0), (1.0, 6000.0)] {
        let w = p.width_at(1.0, pr, 0.0, v);
        assert!(w >= 0.2 - 1e-4 && w <= 1.35 + 1e-4, "범위: {w}");
    }
}

#[test]
fn ballpen_light_pressure_is_thin_and_speed_reduces() {
    let p = BallPenProfile::default();
    let light = p.width_at(1.0, 0.0, 0.0, 300.0);
    let full = p.width_at(1.0, 1.0, 0.0, 300.0);
    let slow = p.width_at(1.0, 1.0, 0.0, 0.0);
    let fast = p.width_at(1.0, 1.0, 0.0, 600.0);
    assert!(full > light, "필압↑ → 굵어짐");
    assert!(slow > fast, "속도↑ → 가늘어짐");
    assert!(
        full - light > 0.5,
        "필압 효과가 커야 (살짝=가늘게, 꾹=굵게): {} vs {}",
        light,
        full
    );
}

#[test]
fn ballpen_tilt_widens_continuously() {
    let p = BallPenProfile::default(); // tilt_k = 0.35
    let flat = p.width_at(2.0, 0.5, 1.0, 0.0);
    let mid = p.width_at(2.0, 0.5, 0.5, 0.0);
    let vert = p.width_at(2.0, 0.5, 0.0, 0.0);
    assert!(
        flat > mid && mid > vert,
        "틸트가 클수록 굵어짐 (연속, 임계값 없음)"
    );
    assert!((flat / vert - 1.35).abs() < 1e-3, "tilt_k=0.35 → 1.35배");
}

#[test]
fn ballpen_starve_thins_at_extreme_speed() {
    let p = BallPenProfile::default(); // starve_v=900, falloff=300
    assert!((p.starve_factor(500.0) - 1.0).abs() < 1e-5);
    assert!((p.starve_factor(1050.0) - 0.5).abs() < 1e-5);
    assert!(p.starve_factor(2000.0) < 1e-5, "완전 끊김");
    // 끊기면 폭이 base 아래로 내려감.
    assert!(p.width_at(1.0, 1.0, 0.0, 2000.0) < 0.8);
}

#[test]
fn ballpen_widths_are_finite_and_smooth() {
    let p = BallPenProfile::default();
    let pts: Vec<StrokePoint> = (0..20)
        .map(|i| StrokePoint::with_time(i as f32, 0.0, 0.7, i * 10))
        .collect();
    let ws = p.widths(1.0, &pts, 0.0);
    assert!(ws.iter().all(|w| w.is_finite()));
    assert!(ws[0] > 0.6 && ws[19] > 0.6);
}

// ---------- stroke_outline / triangulate_polygon ----------

#[test]
fn outline_single_point_is_a_circle() {
    let poly = stroke_outline(&[[5.0, 6.0]], &[2.0], true);
    assert_eq!(poly.len(), 12);
    for p in &poly {
        let d = ((p[0] - 5.0).powi(2) + (p[1] - 6.0).powi(2)).sqrt();
        assert!((d - 2.0).abs() < 1e-4, "원 반지름: {p:?} → {d}");
    }
}

#[test]
fn outline_straight_line_butt_caps() {
    let poly = stroke_outline(&[[0.0, 0.0], [10.0, 0.0]], &[2.0, 2.0], false);
    // butt: 바깥 체인 2점 + 안쪽 체인 2점 = 4점 사각형.
    assert_eq!(poly.len(), 4);
    for p in &poly {
        assert!((p[1].abs() - 2.0).abs() < 1e-6);
        assert!(p[0] >= -1e-6 && p[0] <= 10.0 + 1e-6);
    }
}

#[test]
fn triangulate_straight_lens_is_complete() {
    // 일정 두께의 직선 획 — 외곽선이 완전 직선 렌즈(많은 collinear 정점)여도
    // 삼각분할이 중간을 빠뜨리지 않아야 합니다 (회귀: 스트로크 본체 미렌더).
    let pts: Vec<[f32; 2]> = (0..24).map(|i| [20.0 + i as f32 * 8.0, 80.0]).collect();
    let halves = vec![1.5; 24];
    let poly = stroke_outline(&pts, &halves, true);
    let tris = triangulate_polygon(&poly);
    // collinear 정점은 면적 없이 제거되므로 개수 대신 **면적 일치**로
    // 완전 커버를 검증합니다 (직사각형 184×3 + 반원 캡 2개).
    let area: f32 = tris.iter().map(|t| triangle_area(t, &poly)).sum();
    let expected = 184.0 * 3.0 + std::f32::consts::PI * 1.5 * 1.5;
    assert!(
        (area - expected).abs() < 1.0,
        "면적 일치: {area} vs {expected}"
    );
    assert!(area > 500.0, "본체가 채워져야 함: {area}");
}

#[test]
fn triangulate_square_covers_area_exactly() {
    let square = [[0.0f32, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
    let tris = triangulate_polygon(&square);
    assert_eq!(tris.len(), 2, "사각형 → 삼각형 2개");
    let area: f32 = tris.iter().map(|t| triangle_area(t, &square)).sum();
    assert!((area - 100.0).abs() < 1e-3, "넓이 합 = 사각형 넓이");
}

#[test]
fn triangulate_concave_polygon_no_overlap() {
    // L자(오목) 다각형 — 볼록 팬이면 밖으로 삐져나가는 모양.
    let l = [
        [0.0f32, 0.0],
        [10.0, 0.0],
        [10.0, 4.0],
        [4.0, 4.0],
        [4.0, 10.0],
        [0.0, 10.0],
    ];
    let poly_area = polygon_area(&l);
    let tris = triangulate_polygon(&l);
    assert_eq!(tris.len(), 4, "6각형 → 삼각형 4개");
    let area: f32 = tris.iter().map(|t| triangle_area(t, &l)).sum();
    assert!(
        (area - poly_area).abs() < 1e-3,
        "겹침 없이 정확히 한 번 덮음"
    );
    assert!(poly_area > 60.0, "L자 면적 확인");
}

#[test]
fn checked_triangulation_reports_incomplete_for_self_intersecting() {
    // 자기 교차가 생기는 스크리블 외곽선 — 완전 분할 불가를 감지해야 함.
    let mut pts: Vec<[f32; 2]> = Vec::new();
    let mut t = 0.0f32;
    while t < std::f32::consts::TAU * 3.0 {
        let r = 20.0 + 8.0 * (t * 3.0).sin();
        pts.push([100.0 + r * t.cos(), 100.0 + r * t.sin() * 0.7]);
        t += 0.08;
    }
    let halves: Vec<f32> = vec![1.0; pts.len()];
    let poly = stroke_outline(&pts, &halves, true);
    let (tris, complete) = triangulate_polygon_checked(&poly);
    // 완전하지 않으면 폴백이 반드시 존재해야 함.
    if !complete {
        let fb = stroke_fallback_geometry(&pts, &halves);
        assert!(!fb.quads.is_empty(), "폴백 quad 존재");
    }
    assert!(!tris.is_empty(), "부분이라도 삼각형은 있음");
}

#[test]
fn fallback_geometry_is_bounded_even_for_scribbles() {
    let mut pts: Vec<[f32; 2]> = Vec::new();
    let mut t = 0.0f32;
    while t < std::f32::consts::TAU * 3.0 {
        let r = 20.0 + 8.0 * (t * 3.0).sin();
        pts.push([100.0 + r * t.cos(), 100.0 + r * t.sin() * 0.7]);
        t += 0.08;
    }
    let halves: Vec<f32> = vec![1.5; pts.len()];
    let fb = stroke_fallback_geometry(&pts, &halves);
    for p in fb.quads.iter().flatten() {
        let d = min_dist_to_polyline(*p, &pts);
        assert!(d <= 1.5 + 1e-3, "폴백 quad 경계: {d}");
    }
    for (c, r) in &fb.circles {
        assert!(*r <= 1.5 + 1e-3);
        let d = min_dist_to_polyline(*c, &pts);
        assert!(d <= 1.5 + 1e-3);
    }
}

#[test]
fn outline_no_spikes_on_sharp_reversal() {
    // 180° 되접힘 — 마이터 법선이면 무한대 스파이크가 튀는 입력.
    let pts = [[0.0f32, 0.0], [10.0, 0.0], [0.0, 0.0]];
    let poly = stroke_outline(&pts, &[2.0, 2.0, 2.0], false);
    for p in &poly {
        let d = min_dist_to_polyline(*p, &pts);
        assert!(d <= 2.1, "스파이크: {p:?} 거리 {d}");
    }
    // 삼각분할도 가능해야 함 (단순 다각형).
    assert!(!triangulate_polygon(&poly).is_empty());
}

#[test]
fn outline_jagged_input_stays_bounded() {
    let mut pts = vec![[0.0f32, 0.0]];
    let mut x = 0.0f32;
    for i in 1..30 {
        x += 8.0;
        pts.push([x, if i % 2 == 0 { -4.0 } else { 4.0 }]);
    }
    let halves: Vec<f32> = (0..30).map(|i| 1.0 + 0.04 * i as f32).collect();
    let poly = stroke_outline(&pts, &halves, true);
    let max_h = halves.iter().cloned().fold(0.0f32, f32::max);
    for p in &poly {
        let d = min_dist_to_polyline(*p, &pts);
        assert!(d <= max_h + 1e-2, "경계 초과: {p:?} 거리 {d} (max {max_h})");
    }
    assert!(!triangulate_polygon(&poly).is_empty());
}

#[test]
fn outline_round_caps_add_cap_area() {
    // 둥근 캡: 직사각형(2h×L) + 양쪽 반원(π h²)만큼 넓어짐.
    let pts = [[0.0f32, 0.0], [10.0, 0.0]];
    let h = 2.0f32;
    let poly = stroke_outline(&pts, &[h, h], true);
    let expected = 10.0 * 4.0 + std::f32::consts::PI * h * h; // ≈ 40 + 12.57
    let area = polygon_area(&poly);
    assert!(
        (area - expected).abs() < 0.5,
        "캡 면적 포함: {area} vs {expected} (10각 근사)"
    );
}

// ---------- InkBleed ----------

#[test]
fn bleed_disabled_returns_zero() {
    let mut b = InkBleed::default();
    b.enabled = false;
    assert_eq!(b.radius(0.0, 10.0, 10.0, 5.0), 0.0);
}

#[test]
fn bleed_zero_age_returns_zero() {
    let b = InkBleed {
        enabled: true,
        ..InkBleed::default()
    };
    assert_eq!(b.radius(5.0, 5.0, 10.0, 0.0), 0.0);
}

#[test]
fn bleed_grows_with_age_and_clamps() {
    let b = InkBleed {
        enabled: true,
        max_spread_pt: 5.0,
        start_rate: 1.0,
        mid_rate: 1.0,
        end_rate: 1.0,
        ..InkBleed::default()
    };
    let mid = b.radius(5.0, 5.0, 10.0, 2.0);
    assert!((mid - 2.0).abs() < 1e-4, "속도 1 → 2초에 2pt");
    assert_eq!(b.radius(5.0, 5.0, 10.0, 99.0), 5.0, "상한 클램프");
    // 나이에 단조 증가.
    assert!(b.radius(5.0, 5.0, 10.0, 0.5) < b.radius(5.0, 5.0, 10.0, 1.5));
}

#[test]
fn bleed_phase_rates_follow_stroke_position() {
    let b = InkBleed {
        enabled: true,
        max_spread_pt: 50.0,
        start_rate: 3.0,
        mid_rate: 1.0,
        end_rate: 2.0,
        ..InkBleed::default()
    };
    let len = 100.0;
    assert!(
        (b.phase_rate(0.0, len, len) - 3.0).abs() < 1e-4,
        "시작 = start_rate"
    );
    assert!(
        (b.phase_rate(50.0, 50.0, len) - 1.0).abs() < 1e-4,
        "중간 = mid_rate"
    );
    assert!(
        (b.phase_rate(len, 0.0, len) - 2.0).abs() < 1e-4,
        "끝 = end_rate"
    );
    // 구간 경계는 단조 보간 (start 3 → mid 1로 감소).
    let r1 = b.phase_rate(10.0, 90.0, len);
    let r2 = b.phase_rate(20.0, 80.0, len);
    let r3 = b.phase_rate(30.0, 70.0, len);
    assert!(r1 >= r2, "시작→중간으로 감소: {r1} vs {r2}");
    assert!(r2 >= r3, "중간 수렴: {r2} vs {r3}");
    assert!((r3 - 1.0).abs() < 1e-4, "경계 끝 = mid_rate");
}

#[test]
fn bleed_zero_rate_phase_never_spreads() {
    let b = InkBleed {
        enabled: true,
        max_spread_pt: 5.0,
        start_rate: 0.0,
        mid_rate: 0.4,
        end_rate: 0.0,
        ..InkBleed::default()
    };
    assert_eq!(b.radius(0.0, 10.0, 10.0, 10.0), 0.0, "시작 구간 속도 0");
    assert!(b.radius(5.0, 5.0, 10.0, 10.0) > 0.0, "중간은 번짐");
}

// ---------- InkSoak (스며들며 진해짐, 도구별) ----------

#[test]
fn ink_soak_ramp_matches_tool_defaults() {
    let f = InkSoak::fountain_default();
    assert!((f.sat_at(0.0) - 0.35).abs() < 1e-6, "만년필 시작 = 옅게");
    assert!((f.sat_at(2.0) - 1.0).abs() < 1e-6, "2초 후 원색");
    assert!((f.sat_at(1.0) - 0.675).abs() < 1e-5, "선형 중간");
    assert!((f.sat_at(99.0) - 1.0).abs() < 1e-6, "상한");
    let b = InkSoak::ballpoint_default();
    assert!(b.enabled && f.enabled, "둘 다 기본 활성화");
    assert!(
        b.initial > f.initial,
        "볼펜은 더 은은하게(높은 초기값) 시작: {} vs {}",
        b.initial,
        f.initial
    );
    assert!(b.saturate_sec <= f.saturate_sec, "볼펜이 더 빨리 진해짐");
    // 볼펜 나이 0 → 0.6 (만년필보다 진한 상태에서 시작).
    assert!((b.sat_at(0.0) - 0.6).abs() < 1e-6);
    assert!((b.sat_at(b.saturate_sec) - 1.0).abs() < 1e-6);
}

#[test]
fn ink_soak_deserializes_with_defaults() {
    let s: InkSoak = serde_json::from_str("{}").unwrap();
    assert_eq!(s, InkSoak::default(), "이전 세션 호환");
    // initial 클램프 — 0..1 밖 값은 고정.
    let weird = InkSoak {
        enabled: true,
        saturate_sec: 2.0,
        initial: 1.5,
    };
    assert!(
        (weird.sat_at(0.0) - 1.0).abs() < 1e-6,
        "initial > 1 → 항상 원색"
    );
    assert!((weird.sat_at(0.0) - weird.sat_at(3.0)).abs() < 1e-6);
}

// ---------- FountainProfile (만년필 물리 모델) ----------

#[test]
fn fountain_effective_pressure_multiplies_tilt() {
    let f = FountainProfile::default(); // tilt_k = 0.4
    assert!((f.effective_pressure(1.0, 0.0) - 1.0).abs() < 1e-5);
    assert!((f.effective_pressure(1.0, 1.0) - 1.4).abs() < 1e-5);
    // 필압 0이면 기울기만으로는 선이 생기지 않음 (곱 구조).
    assert!(f.effective_pressure(0.0, 1.0) < 1e-5);
}

#[test]
fn fountain_speed_factor_halves_at_ref() {
    let f = FountainProfile::default(); // speed_ref = 150
    assert!((f.speed_factor(0.0) - 1.0).abs() < 1e-5, "정지 = 최대");
    assert!(
        (f.speed_factor(f.speed_ref) - 0.5).abs() < 1e-5,
        "v_ref = 0.5"
    );
    assert!(f.speed_factor(1200.0) < 0.1, "빠르면 얇아짐");
}

#[test]
fn fountain_width_grows_with_pressure_and_tilt() {
    let mut f = FountainProfile::default();
    f.italic = false;
    f.v_dwell = 0.0; // 정지 보정 제외
    let v = f.speed_ref; // 속도 계수 0.5 고정
    let w_weak = f.width_at(2.5, 0.2, 0.0, v);
    let w_full = f.width_at(2.5, 1.0, 0.0, v);
    assert!(w_full > w_weak, "필압↑ → 굵어짐");
    // 기울기↑ → 같은 필압에서 굵어짐.
    let w_tilt = f.width_at(2.5, 1.0, 1.0, v);
    assert!(w_tilt > w_full, "기울기↑ → 굵어짐");
}

#[test]
fn fountain_width_decreases_with_speed() {
    let mut f = FountainProfile::default();
    f.v_dwell = 0.0;
    let slow = f.width_at(2.5, 1.0, 0.0, 10.0);
    let fast = f.width_at(2.5, 1.0, 0.0, 600.0);
    assert!(fast < slow, "속도↑ → 가늘어짐");
}

#[test]
fn fountain_width_clamped_to_min_max() {
    let mut f = FountainProfile::default();
    f.min_width_pt = 0.5;
    f.v_dwell = 0.0;
    for (p, v) in [(0.0f32, 1e9f32), (1.0, 0.0), (0.5, 60.0)] {
        let w = f.width_at(2.5, p, 0.0, v);
        assert!(w >= 0.5 - 1e-4 && w <= 2.5 + 1e-4, "클램프: {w}");
    }
}

#[test]
fn fountain_dwell_blobs_when_stopped() {
    let mut f = FountainProfile::default(); // v_dwell=5, dwell_k=0.05
    f.italic = false;
    let moving = f.width_at(2.5, 1.0, 0.0, 60.0);
    let stopped = f.width_at(2.5, 1.0, 0.0, 0.0);
    assert!(stopped > moving, "정지 → 잉크 고임으로 굵어짐");
    // 가산량 함수 = k_dwell × (v_dwell − v), 임계에서 0.
    assert!((f.dwell_extra(0.0) - 0.25).abs() < 1e-5);
    assert!(f.dwell_extra(f.v_dwell) < 1e-5);
}

#[test]
fn fountain_italic_direction_contrast() {
    let mut f = FountainProfile::default();
    f.italic = true;
    f.nib_angle_deg = 0.0;
    f.italic_k = 0.3;
    // 닙 축(0°)과 나란한 방향 → 최대, 수직 → 최소.
    assert!((f.italic_factor(10.0, 0.0) - 1.3).abs() < 1e-5);
    assert!((f.italic_factor(0.0, 10.0) - 0.7).abs() < 1e-5);
    // 끄면 1.0.
    f.italic = false;
    assert!((f.italic_factor(10.0, 0.0) - 1.0).abs() < 1e-5);
}

#[test]
fn fountain_speeds_smoothed_and_missing_time_is_zero() {
    let f = FountainProfile::default(); // smooth α = 0.3
                                        // 일정 속도 100pt/s (매 10ms에 1pt 이동).
    let pts: Vec<StrokePoint> = (0..20)
        .map(|i| StrokePoint::with_time(i as f32, 0.0, 0.5, i * 10))
        .collect();
    let vs = f.speeds(&pts);
    // EMA가 일정 입력에 수렴.
    assert!(vs[19] > 95.0 && vs[19] <= 100.0, "EMA 수렴: {}", vs[19]);
    // 시각이 없는 점 → 속도 0.
    let no_time = vec![
        StrokePoint::new(0.0, 0.0, 0.5),
        StrokePoint::new(10.0, 0.0, 0.5),
    ];
    assert_eq!(f.speeds(&no_time), vec![0.0, 0.0]);
}

#[test]
fn fountain_widths_follow_speed_profile() {
    let mut f = FountainProfile::default();
    f.italic = false;
    // 느리게(굵게) 쓰다가 빠르게(가늘게) 쓰는 획.
    let pts: Vec<StrokePoint> = (0..20)
        .map(|i| StrokePoint::with_time(i as f32, 0.0, 0.9, i * 10)) // 100pt/s
        .chain((0..20).map(|i| StrokePoint::with_time(20.0 + i as f32, 0.0, 0.9, 200 + i))) // 1000pt/s
        .collect();
    let ws = f.widths(2.5, &pts, 0.0);
    assert!(ws[5] > ws[35], "느린 구간이 빠른 구간보다 굵어야 함");
}

// ---------- 헬퍼 ----------

fn min_dist_to_polyline(p: [f32; 2], poly: &[[f32; 2]]) -> f32 {
    let mut best = f32::MAX;
    for i in 0..poly.len() {
        let d = ((p[0] - poly[i][0]).powi(2) + (p[1] - poly[i][1]).powi(2)).sqrt();
        best = best.min(d);
    }
    best
}

fn triangle_area(t: &[u32; 3], poly: &[[f32; 2]]) -> f32 {
    let (a, b, c) = (
        poly[t[0] as usize],
        poly[t[1] as usize],
        poly[t[2] as usize],
    );
    ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() * 0.5
}

fn polygon_area(poly: &[[f32; 2]]) -> f32 {
    signed_area2(poly).abs() * 0.5
}

// ---------- 통합 지오메트리 + 인과적 선폭 확정 ----------

/// 만년필(이탤릭 켜짐) 곡선 스트로크 — 배치 `widths`와
/// `WidthLocker`(입력 즉시 확정)의 최종 폭이 정확히 일치해야 합니다.
/// 이 불변식이 "펜을 떼도 굵기가 변하지 않음"을 보장합니다.
#[test]
fn locker_matches_batch_fountain_widths_with_italic() {
    let mut f = FountainProfile::default();
    f.italic = true;
    f.italic_k = 0.35;
    f.nib_angle_deg = 45.0;
    let t0 = 1_700_000_000_000u64;
    let mut pts: Vec<StrokePoint> = Vec::new();
    let mut locker = WidthLocker::new(
        ToolType::Fountain,
        2.5,
        &Materials::new(BallPenProfile::default(), f),
        0.0,
    );
    for i in 0..40 {
        let x = i as f32 * 6.0 + (i as f32 * 0.7).sin() * 8.0;
        let y = 100.0 + (i as f32 * 0.35).cos() * 30.0 + i as f32 * 1.5;
        let p = StrokePoint::with_time(x, y, 0.4 + 0.5 * (i % 7) as f32 / 7.0, t0 + i as u64 * 8);
        let (locked_prev, tip) = locker.push(p);
        if let Some(prev) = locked_prev {
            if let Some(last) = pts.last_mut() {
                *last = prev;
            }
        }
        pts.push(tip);
    }
    if let Some(final_pt) = locker.finish() {
        if let Some(last) = pts.last_mut() {
            *last = final_pt;
        }
    }
    let batch = f.widths(2.5, &pts, 0.0);
    for (i, p) in pts.iter().enumerate() {
        assert!(
            (p.width - batch[i]).abs() < 1e-4,
            "점 {i}: 잠금 {:.4} vs 배치 {:.4}",
            p.width,
            batch[i]
        );
    }
}

/// 일반 펜(볼펜)도 배치와 일치 (첫 점 속도는 두 번째 점 도착 시 확정).
#[test]
fn locker_matches_batch_ballpen_widths() {
    let b = BallPenProfile::default();
    let t0 = 1_700_000_000_000u64;
    let mut pts: Vec<StrokePoint> = Vec::new();
    let mut locker = WidthLocker::new(
        ToolType::Pen,
        2.0,
        &Materials::new(b, FountainProfile::default()),
        0.0,
    );
    for i in 0..25 {
        let p = StrokePoint::with_time(
            i as f32 * 5.0,
            60.0 + (i as f32 * 0.9).sin() * 10.0,
            0.3 + 0.6 * (i % 5) as f32 / 5.0,
            t0 + i as u64 * 12,
        );
        let (locked_prev, tip) = locker.push(p);
        if let Some(prev) = locked_prev {
            if let Some(last) = pts.last_mut() {
                *last = prev;
            }
        }
        pts.push(tip);
    }
    if let Some(final_pt) = locker.finish() {
        if let Some(last) = pts.last_mut() {
            *last = final_pt;
        }
    }
    let batch = b.widths(2.0, &pts, 0.0);
    for (i, p) in pts.iter().enumerate() {
        assert!(
            (p.width - batch[i]).abs() < 1e-4,
            "점 {i}: 잠금 {:.4} vs 배치 {:.4}",
            p.width,
            batch[i]
        );
    }
}

/// 한 점(도트) 스트로크도 확정 폭이 배치와 같아야 합니다.
#[test]
fn locker_single_point_matches_batch() {
    let f = FountainProfile::default();
    let p = StrokePoint::with_time(10.0, 20.0, 0.8, 0);
    let mut locker = WidthLocker::new(
        ToolType::Fountain,
        2.5,
        &Materials::new(BallPenProfile::default(), f),
        0.0,
    );
    let (_, tip) = locker.push(p);
    let done = locker.finish().unwrap();
    let batch = f.widths(2.5, &[p], 0.0);
    assert!((tip.width - batch[0]).abs() < 1e-4, "임시 폭도 일치");
    assert!((done.width - batch[0]).abs() < 1e-4, "확정 폭 일치");
}

// ---- WritingMaterial: open/closed trait + a single factory branch ----

/// A tiny custom material used to prove `WidthLocker` accepts **any**
/// `WritingMaterial` (previously it branched on a private `LockerProfile`).
struct NudgeMaterial {
    pub k: f32,
}

impl WritingMaterial for NudgeMaterial {
    fn smoothing_alpha(&self) -> f32 {
        0.0
    }
    fn point_width(
        &self,
        max_width_pt: f32,
        _pressure: f32,
        _tilt_mag: f32,
        _speed: f32,
        _dir: [f32; 2],
    ) -> f32 {
        max_width_pt * self.k
    }
    fn widths(&self, max_width_pt: f32, pts: &[StrokePoint], _tilt_mag: f32) -> Vec<f32> {
        let mut v = Vec::with_capacity(pts.len());
        for _ in pts.iter() {
            v.push(max_width_pt * self.k);
        }
        v
    }
}

#[test]
fn locker_accepts_any_custom_writing_material() {
    let mut locker = WidthLocker::with_material(Box::new(NudgeMaterial { k: 0.5 }), 4.0, 0.0);
    let (_, tip) = locker.push(StrokePoint::with_time(0.0, 0.0, 0.6, 1));
    assert!(
        (tip.width - 2.0).abs() < 1e-5,
        "uses the custom material's width"
    );
}

#[test]
fn for_tool_highlighter_is_constant_uniform_width() {
    let mat = Materials::default()
        .for_tool(ToolType::Highlighter)
        .expect("highlighter has a material");
    assert!(
        (mat.smoothing_alpha() - 0.0).abs() < 1e-9,
        "constant has no EMA smoothing"
    );
    for i in 0..5 {
        let w = mat.point_width(3.0, 0.1 + i as f32 * 0.2, 0.4, 123.0, [1.0, 0.0]);
        assert!(
            (w - 3.0).abs() < 1e-5,
            "constant width is exactly max for any input"
        );
    }
}

#[test]
fn for_tool_ballpen_ignores_direction() {
    let b = BallPenProfile::default();
    let mat = Materials::new(b, FountainProfile::default())
        .for_tool(ToolType::Pen)
        .expect("pen material");
    let straight = mat.point_width(2.0, 0.5, 0.2, 30.0, [1.0, 0.0]);
    let sideways = mat.point_width(2.0, 0.5, 0.2, 30.0, [0.0, 1.0]);
    assert!((straight - b.width_at(2.0, 0.5, 0.2, 30.0)).abs() < 1e-6);
    assert!(
        (straight - sideways).abs() < 1e-6,
        "ball pen ignores stroke direction"
    );
    assert!((mat.smoothing_alpha() - b.speed_smooth).abs() < 1e-9);
}

#[test]
fn for_tool_fountain_applies_italic_and_clamp() {
    let mut f = FountainProfile::default();
    f.italic = true; // exercise the italic-nib direction contrast
    let mat = Materials::new(BallPenProfile::default(), f)
        .for_tool(ToolType::Fountain)
        .expect("fountain material");
    // Nib axis is nib_angle_deg=45°: aligned vs perpendicular must differ.
    let along = [
        std::f32::consts::FRAC_PI_4.cos(),
        std::f32::consts::FRAC_PI_4.sin(),
    ]; // 45°
    let perp = [
        std::f32::consts::FRAC_PI_4.cos(),
        -std::f32::consts::FRAC_PI_4.sin(),
    ]; // -45°
    let w_along = mat.point_width(2.5, 0.8, 0.0, 10.0, along);
    let w_perp = mat.point_width(2.5, 0.8, 0.0, 10.0, perp);
    assert!(
        (w_along - w_perp).abs() > 1e-3,
        "italic nib must vary with direction"
    );
    // Exactly the (former) locker Fountain arm: (w * italic).clamp(lo, hi).
    let w = f.width_at(2.5, 0.8, 0.0, 10.0);
    let lo = f.min_width_pt.max(0.05).min(2.5);
    let hi = 2.5;
    let exp = (w * f.italic_factor(along[0], along[1])).clamp(lo, hi);
    assert!(
        (w_along - exp).abs() < 1e-6,
        "matches the previous Fountain locker math"
    );
    assert!((mat.smoothing_alpha() - f.speed_smooth).abs() < 1e-9);
}

#[test]
fn for_tool_non_ink_tools_is_none() {
    let m = Materials::default();
    assert!(m.for_tool(ToolType::Eraser).is_none());
    assert!(m.for_tool(ToolType::Pan).is_none());
}

// Batch widths move into the trait so the bake path (`halves_for_stroke`)
// can consume any material through one `Materials::for_tool` resolution.

#[test]
fn writing_material_widths_survive_via_factory_box() {
    let f = FountainProfile::default();
    let m = Materials::new(BallPenProfile::default(), f)
        .for_tool(ToolType::Fountain)
        .expect("fountain material");
    let t0 = 1_000_000u64;
    let mut pts: Vec<StrokePoint> = Vec::with_capacity(20);
    for i in 0..20 {
        pts.push(StrokePoint::with_time(
            i as f32 * 3.0,
            50.0 + (i as f32).sin() * 5.0,
            0.4 + 0.5 * (i % 4) as f32 / 4.0,
            t0 + i as u64 * 12,
        ));
    }
    let ws = m.widths(2.5, &pts, 0.0);
    assert_eq!(ws.len(), pts.len());
    for w in ws {
        assert!(w > 0.0, "boxed material produces positive widths");
    }
}

#[test]
fn constant_material_widths_are_all_max() {
    let m = Materials::default()
        .for_tool(ToolType::Highlighter)
        .expect("highlighter material");
    let mut pts: Vec<StrokePoint> = Vec::new();
    pts.push(StrokePoint::new(0.0, 0.0, 0.5));
    pts.push(StrokePoint::new(1.0, 1.0, 0.5));
    pts.push(StrokePoint::new(2.0, 0.0, 0.9));
    let ws = m.widths(4.0, &pts, 0.3);
    assert_eq!(ws.len(), 3);
    for w in ws {
        assert!(
            (w - 4.0).abs() < 1e-6,
            "constant material fills exactly max width"
        );
    }
}

/// 실제 필기 속도 회귀 테스트: 느린 곡선 구간과 빠른 직선 구간이 같은
/// 스트로크 안에서 **눈에 보일 정도로 다른 굵기**를 가져야 합니다
/// (이전 기본값 speed_ref=60은 실제 속도에서 곡선이 포화되어 굵기가
/// 일정해 보이는 문제가 있었습니다).
#[test]
fn widths_vary_visibly_between_slow_and_fast_sections() {
    // 16ms 간격 점, 느린 구간 50 pt/s, 빠른 구간 400 pt/s, 필압 일정.
    let build = |speed: f32, count: usize, t0: &mut u64, x: &mut f32| {
        let mut pts = Vec::new();
        for _ in 0..count {
            *x += speed * 0.016;
            pts.push(StrokePoint::with_time(*x, 100.0, 0.7, *t0));
            *t0 += 16;
        }
        pts
    };
    let mut t0 = 1_700_000_000_000u64;
    let mut x = 0.0f32;
    let slow = build(50.0, 12, &mut t0, &mut x);
    let fast = build(400.0, 12, &mut t0, &mut x);
    let mut pts = slow;
    pts.extend(fast);

    let f = FountainProfile::default();
    let batch = f.widths(2.5, &pts, 0.0);
    let slow_w = batch[11];
    let fast_w = batch[23];
    assert!(
        slow_w / fast_w > 1.8,
        "만년필: 느린 곡선 {slow_w:.2} vs 빠른 직선 {fast_w:.2} — 굵기 차이가 보여야 함"
    );

    let b = BallPenProfile::default();
    let batch = b.widths(2.0, &pts, 0.0);
    let slow_w = batch[11];
    let fast_w = batch[23];
    assert!(
        slow_w / fast_w > 1.15,
        "볼펜: 느린 곡선 {slow_w:.2} vs 빠른 직선 {fast_w:.2} — 은은하지만 차이가 있어야 함"
    );
}

/// 통합 진입점 `stroke_geometry`: 직선 렌즈는 완전 분할 + 면적 일치.
#[test]
fn stroke_geometry_covers_straight_lens_exactly() {
    let pts: Vec<[f32; 2]> = (0..10).map(|i| [10.0 + i as f32 * 9.0, 50.0]).collect();
    let halves: Vec<f32> = vec![1.5; 10];
    match stroke_geometry(&pts, &halves, true) {
        StrokeFill::Tris(t) => {
            let area: f32 = t.tris.iter().map(|tr| triangle_area(tr, &t.poly)).sum();
            let expected = 81.0 * 3.0 + std::f32::consts::PI * 1.5 * 1.5;
            assert!((area - expected).abs() < 1.0, "면적 {area} vs {expected}");
            assert!(!t.aa_edges.is_empty(), "경계 AA 가장자리 존재");
        }
        StrokeFill::Fallback(_) => panic!("직선 렌즈는 완전 분할이어야 함"),
    }
}

// ---------- 격자 가속 귀 자르기 (n ≥ 96에서 활성화) ----------

/// 큰 별 모양(단순) 다각형 — 격자 경로로 완전 분할되고, 삼각형 면적
/// 합이 다각형 면적과 일치해야 합니다 (격자가 후보를 빠뜨리면 귀 판정이
/// 틀려 면적이 어긋납니다).
#[test]
fn grid_triangulation_covers_large_star_exactly() {
    let n = 300;
    let mut poly: Vec<[f32; 2]> = Vec::with_capacity(n);
    for k in 0..n {
        let t = k as f32 / n as f32 * std::f32::consts::TAU;
        let r = 100.0 + 12.0 * (7.0 * t).cos();
        poly.push([200.0 + r * t.cos(), 200.0 + r * t.sin()]);
    }
    let (tris, complete) = triangulate_polygon_checked(&poly);
    assert!(complete, "단순 다각형은 완전 분할");
    assert_eq!(tris.len(), n - 2);
    let tri_area: f32 = tris.iter().map(|t| triangle_area(t, &poly)).sum();
    let expected = polygon_area(&poly);
    assert!(
        (tri_area - expected).abs() < expected * 1e-3,
        "면적 {tri_area} vs {expected}"
    );
}

/// 큰 다각형에서 일직선(collinear) 정점이 섞여도 격자 경로가 올바르게
/// 완전 분할합니다.
#[test]
fn grid_triangulation_handles_collinear_vertices() {
    // 사각형 테두리에 점을 많이 찍음 (테두리 위 점 = 콜리니어).
    let mut poly: Vec<[f32; 2]> = Vec::new();
    for i in 0..60 {
        poly.push([10.0 + i as f32, 10.0]); // 아래
    }
    for i in 0..60 {
        poly.push([70.0, 10.0 + i as f32]); // 오른쪽
    }
    for i in 0..60 {
        poly.push([70.0 - i as f32, 70.0]); // 위 (역순)
    }
    for i in 0..60 {
        poly.push([10.0, 70.0 - i as f32]); // 왼쪽 (역순)
    }
    assert!(poly.len() >= 96, "격자 경로 활성화");
    let (tris, complete) = triangulate_polygon_checked(&poly);
    assert!(complete, "사각형은 완전 분할");
    let tri_area: f32 = tris.iter().map(|t| triangle_area(t, &poly)).sum();
    let expected = polygon_area(&poly);
    assert!(
        (tri_area - expected).abs() < 1e-3,
        "면적 {tri_area} vs {expected}"
    );
}

// ---------- stroke_ribbon (리본 근사 지오메트리) ----------

#[test]
fn ribbon_straight_line_has_expected_edges_and_area() {
    let pts: Vec<[f32; 2]> = (0..12).map(|i| [i as f32 * 10.0, 0.0]).collect();
    let halves: Vec<f32> = vec![2.0; 12];
    let rb = stroke_ribbon(&pts, &halves, 0.0, true, None);
    // 직선이면 법선은 위쪽 — L = +2, R = −2.
    assert_eq!(rb.verts[0], [0.0, 2.0]);
    assert_eq!(rb.verts[1], [0.0, -2.0]);
    // 본체 면적 = 직사각형(110×4) + 8분할 반원 캡 두 개.
    let area: f32 = rb.tris.iter().map(|t| triangle_area(t, &rb.verts)).sum();
    let expected = 110.0 * 4.0 + 32.0 * (std::f32::consts::FRAC_PI_8).sin();
    assert!((area - expected).abs() < 1e-2, "면적 {area} vs {expected}");
}

#[test]
fn ribbon_matches_outline_area_without_overlap() {
    // 완만한 곡선 + 다양한 폭 — 리본 본체(마이터 단면 공유)는 정확 외곽선
    // 다각형과 **같은 면적**이어야 함(겹침/틈 없음 = 반투명 얼룩 없음).
    let pts = [
        [0.0f32, 0.0],
        [12.0, 3.0],
        [24.0, -2.0],
        [38.0, 6.0],
        [50.0, 4.0],
    ];
    let halves = [1.0f32, 1.6, 0.8, 2.0, 1.2];
    let rb = stroke_ribbon(&pts, &halves, 0.0, false, None);
    let poly = stroke_outline(&pts, &halves, false);
    let rb_area: f32 = rb.tris.iter().map(|t| triangle_area(t, &rb.verts)).sum();
    let poly_area = polygon_area(&poly);
    assert!(
        (rb_area - poly_area).abs() < 1e-2,
        "리본 {rb_area} vs 외곽선 {poly_area}"
    );
}

#[test]
fn ribbon_feather_adds_alpha_ramp_outside() {
    let pts = [[0.0f32, 0.0], [10.0, 0.0]];
    let halves = [1.0f32, 1.0];
    let rb = stroke_ribbon(&pts, &halves, 0.5, true, None);
    assert!(rb.alphas.iter().any(|&a| a == 0.0), "바깥 알파 0 존재");
    assert!(rb.alphas.iter().any(|&a| a == 1.0), "본체 알파 1 존재");
    assert_eq!(rb.verts.len(), rb.alphas.len());
    // bbox: 두께 1 + 페더 0.5 → y ∈ [−1.5, 1.5].
    assert!((rb.bbox[1] - (-1.5)).abs() < 1e-4);
    assert!((rb.bbox[3] - 1.5).abs() < 1e-4);
}

#[test]
fn ribbon_applies_per_point_ink_saturation() {
    // 잉크가 스며들며 진해지는 효과 — 점별 알파가 본체 정점에 적용.
    let pts = [[0.0f32, 0.0], [10.0, 0.0]];
    let halves = [1.0f32, 1.0];
    let rb = stroke_ribbon(&pts, &halves, 0.0, false, Some(&[0.35, 1.0]));
    assert!((rb.alphas[0] - 0.35).abs() < 1e-4, "첫 점은 옅게");
    assert!((rb.alphas[2] - 1.0).abs() < 1e-4, "둘째 점은 완전 포화");
    assert!((rb.alphas[3] - 1.0).abs() < 1e-4, "둘째 점 R도 완전 포화");
    // 캡에도 끝점 알파 적용.
    let rb2 = stroke_ribbon(&pts, &halves, 0.0, true, Some(&[0.2, 1.0]));
    assert!(
        rb2.alphas.iter().any(|&a| (a - 0.2).abs() < 1e-4),
        "시작 캡은 첫 점 알파"
    );
}

#[test]
fn ribbon_lr_applies_per_side_alphas() {
    // 단면(레일로드) 효과: 왼쪽/오른쪽 정점에 서로 다른 알파.
    let pts = [[0.0f32, 0.0], [10.0, 0.0]];
    let halves = [1.0f32, 1.0];
    let rb = stroke_ribbon_lr(&pts, &halves, 0.0, false, Some(&[[0.4, 0.8], [1.0, 0.5]]));
    // feather=0 → per=2, 정점 순서 [L0, R0, L1, R1].
    assert!((rb.alphas[0] - 0.4).abs() < 1e-4, "L0");
    assert!((rb.alphas[1] - 0.8).abs() < 1e-4, "R0");
    assert!((rb.alphas[2] - 1.0).abs() < 1e-4, "L1");
    assert!((rb.alphas[3] - 0.5).abs() < 1e-4, "R1");
    // None → 전부 1.0 (기존 동작과 동일).
    let rb2 = stroke_ribbon_lr(&pts, &halves, 0.0, false, None);
    assert!(rb2.alphas.iter().all(|&a| a == 1.0));
    // 캡은 양쪽 평균.
    let rb3 = stroke_ribbon_lr(&pts, &halves, 0.0, true, Some(&[[0.2, 0.8], [0.0, 1.0]]));
    assert!(
        rb3.alphas.iter().any(|&a| (a - 0.5).abs() < 1e-4),
        "시작 캡 = (0.2+0.8)/2"
    );
}

#[test]
fn ribbon_single_point_and_empty_are_safe() {
    let rb = stroke_ribbon(&[[5.0f32, 5.0]], &[2.0], 0.5, true, None);
    assert!(!rb.tris.is_empty());
    for v in &rb.verts {
        assert!(v[0].is_finite() && v[1].is_finite());
    }
    let empty = stroke_ribbon(&[], &[], 1.0, true, None);
    assert!(empty.verts.is_empty() && empty.tris.is_empty());
}
