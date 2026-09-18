//! `ink` 모듈 단위 테스트 — `src/ink.rs`의 `#[cfg(test)]` 모듈에서 이동했습니다.

use freedf_core::ink::*;
use freedf_core::model::{StrokePoint, ToolType};

#[test]
fn density_lr_matches_two_density_calls() {
    // 성능 최적화 가드: density_lr(공유 x-파트)의 결과는 density를 좌/우로
    // 각각 호출한 것과 완전히 동일해야 합니다 (행동 보존). 정확 2옥타브와
    // fast_noise(고주파 생략) 두 모드 모두 검증합니다.
    let g_base = InkGrain {
        seed: 11,
        ..InkGrain::default()
    };
    for &fast in &[false, true] {
        for &seed in &[11u64, 99, 55555] {
            let g = InkGrain {
                seed,
                fast_noise: fast,
                ..g_base
            };
            for &(u, s) in &[(0.0f32, 0.0f32), (0.12, 0.5), (0.5, 1.0), (1.0, 0.9)] {
                for tool in [ToolType::Pen, ToolType::Fountain] {
                    let lr = g.density_lr(tool, u, s);
                    let l = g.density(tool, u, -1.0, s);
                    let r = g.density(tool, u, 1.0, s);
                    assert!(
                        (lr[0] - l).abs() < 1e-6,
                        "L mismatch fast={fast} tool={tool:?} u={u} s={s}: {lr:?} vs {l}"
                    );
                    assert!(
                        (lr[1] - r).abs() < 1e-6,
                        "R mismatch fast={fast} tool={tool:?} u={u} s={s}: {lr:?} vs {r}"
                    );
                }
            }
        }
    }
}

#[test]
fn fast_noise_is_deterministic_bounded_and_non_popping() {
    let g = InkGrain {
        fast_noise: true,
        ..InkGrain::default()
    };
    // 결정성 — 같은 (u,v)는 항상 같은 값.
    for &(u, v) in &[(0.3f32, -1.0), (0.5, 1.0), (0.12, 0.0)] {
        assert_eq!(
            g.density(ToolType::Pen, u, v, 0.5),
            g.density(ToolType::Pen, u, v, 0.5)
        );
    }
    // 밀도 범위 0.30..=1.60 유지.
    for i in 0..200 {
        let u = i as f32 / 200.0;
        for x in g.density_lr(ToolType::Pen, u, 0.5) {
            assert!((0.30..=1.60).contains(&x), "fast_noise 범위: {x}");
        }
    }
    // no-popping — 인접 u 사이 점프가 유계.
    let mut prev = g.density(ToolType::Pen, 0.0, -1.0, 0.5);
    let mut max_jump = 0.0f32;
    for i in 1..200 {
        let u = i as f32 / 200.0;
        let d = g.density(ToolType::Pen, u, -1.0, 0.5);
        max_jump = max_jump.max((d - prev).abs());
        prev = d;
    }
    assert!(max_jump < 0.5, "fast_noise 점프 과대: {max_jump}");
}

#[test]
fn value_noise_is_deterministic_and_bounded() {
    for &(x, y) in &[(0.3f32, 0.7), (1.0, 1.0), (12.4, -3.1), (0.0, 0.0)] {
        let a = value_noise(x, y, 42);
        let b = value_noise(x, y, 42);
        assert_eq!(a, b, "같은 좌표·시드는 항상 같은 값 (깜빡임 금지)");
        assert!((0.0..=1.0).contains(&a), "노이즈 범위: {a}");
    }
}

#[test]
fn value_noise_has_no_popping_between_adjacent_samples() {
    // 인접한 획 점(간격 ~0.005) 사이에서 값이 점프하면 질감이
    // 깜빡이며 보입니다 — 스무스스텝 보간은 기울기가 유계입니다.
    let seed = 7;
    for &x0 in &[0.0f32, 3.7, 10.0] {
        let mut prev = value_noise(x0, 0.5, seed);
        let mut max_jump = 0.0f32;
        let mut x = x0;
        while x < x0 + 2.0 {
            x += 0.005;
            let v = value_noise(x, 0.5, seed);
            max_jump = max_jump.max((v - prev).abs());
            prev = v;
        }
        assert!(max_jump < 0.05, "인접 샘플 점프가 너무 큼: {max_jump}");
    }
}

#[test]
fn noise_distribution_is_centered_and_wide() {
    // 평균이 0.5 근처고 양쪽으로 충분히 퍼져야 "불균일"이 보입니다.
    // (옥타브 가중합 0.72/0.28이 극값을 약간 압축하는 건 의도 —
    //  미묘한 질감이 목표라 [0.1, 0.9] 수준이면 충분합니다)
    let mut sum = 0.0f64;
    let (mut mn, mut mx) = (f32::MAX, f32::MIN);
    let mut n = 0usize;
    for i in 0..64 {
        for j in 0..64 {
            let v = ink_field(i as f32 * 0.37, j as f32 * 0.53, 99);
            sum += v as f64;
            mn = mn.min(v);
            mx = mx.max(v);
            n += 1;
        }
    }
    let mean = sum / n as f64;
    assert!(
        (0.45..=0.55).contains(&mean),
        "평균이 중심에서 벗어남: {mean}"
    );
    assert!(mn < 0.15 && mx > 0.85, "분포가 좁음: [{mn}, {mx}]");
}

#[test]
fn different_seeds_produce_different_grain() {
    // 시드가 다르면 같은 좌표라도 질감이 달라야 합니다 (획별 개성).
    let mut diff = 0usize;
    for i in 0..100 {
        let u = i as f32 / 100.0;
        if (ink_field(u, 0.0, 1) - ink_field(u, 0.0, 2)).abs() > 1e-3 {
            diff += 1;
        }
    }
    assert!(diff > 90, "시드가 질감을 못 바꿈: {diff}/100");
}

#[test]
fn density_stays_bounded_across_whole_stroke_space() {
    let g = InkGrain::default();
    for tool in [ToolType::Pen, ToolType::Fountain] {
        for i in 0..100 {
            let u = i as f32 / 99.0;
            for j in 0..7 {
                let v = -1.0 + j as f32 / 3.0;
                for k in 0..5 {
                    let s = k as f32 / 4.0;
                    let d = g.density(tool, u, v, s);
                    assert!(
                        (0.30..=1.60).contains(&d),
                        "밀도 범위 이탈: {tool:?} u={u} v={v} s={s} d={d}"
                    );
                }
            }
        }
    }
}

#[test]
fn ballpoint_has_start_blob_and_end_bead() {
    // 볼펜: 시작 뭉침과 끝 축적이 중간보다 진해야 함 (평균 비교 —
    // 잡음은 구간 평균에서 상쇄되므로 시드와 무관하게 견고).
    let g = InkGrain::default();
    let avg = |u0: f32, u1: f32| -> f32 {
        let mut s = 0.0f32;
        for i in 0..60 {
            let u = u0 + (u1 - u0) * i as f32 / 59.0;
            s += g.density(ToolType::Pen, u, 0.0, 0.2);
        }
        s / 60.0
    };
    let start = avg(0.0, 0.04);
    let mid = avg(0.35, 0.65);
    let end = avg(0.97, 1.0);
    assert!(start > mid + 0.05, "시작 뭉침 없음: {start} vs {mid}");
    assert!(end > mid + 0.05, "끝 축적 없음: {end} vs {mid}");
}

#[test]
fn fountain_pools_at_ends_and_starves_with_speed() {
    let g = InkGrain::default();
    // 속도 결핍: 같은 위치에서 느릴 때가 빠를 때보다 진함.
    let slow_mid = g.density(ToolType::Fountain, 0.5, 0.0, 0.0);
    let fast_mid = g.density(ToolType::Fountain, 0.5, 0.0, 1.0);
    assert!(
        slow_mid > fast_mid + 0.05,
        "속도 결핍 없음: {slow_mid} vs {fast_mid}"
    );
    // 끝 고임 (구간 평균).
    let avg = |u0: f32, u1: f32| -> f32 {
        let mut s = 0.0f32;
        for i in 0..60 {
            let u = u0 + (u1 - u0) * i as f32 / 59.0;
            s += g.density(ToolType::Fountain, u, 0.0, 0.1);
        }
        s / 60.0
    };
    let mid = avg(0.35, 0.65);
    let end = avg(0.98, 1.0);
    let start = avg(0.0, 0.02);
    assert!(end > mid + 0.03, "끝 고임 없음: {end} vs {mid}");
    assert!(start > mid + 0.03, "시작 고임 없음: {start} vs {mid}");
}

#[test]
fn edges_are_denser_than_center_railroad_effect() {
    // 단면 불균일: 가장자리가 중심보다 진함 — 노이즈 성분을 끄고
    // 순수 레일로드 인자만 비교합니다 (노이즈 진폭이 커지면 고정 시드
    // 평균이 노이즈에 지배되어 6% 가장자리 효과가 묻힙니다).
    let g = InkGrain {
        flow_amp: 0.0,
        wick_amp: 0.0,
        ..InkGrain::default()
    };
    let mut center = 0.0f32;
    let mut edges = 0.0f32;
    for i in 0..200 {
        let u = i as f32 / 200.0;
        center += g.density(ToolType::Pen, u, 0.0, 0.3);
        edges += g.density(ToolType::Pen, u, 1.0, 0.3);
    }
    center /= 200.0;
    edges /= 200.0;
    assert!(
        edges > center + 0.02,
        "레일로드 효과 없음: {edges} vs {center}"
    );
}

#[test]
fn stroke_ink_factors_follow_path_and_are_stable() {
    // 200점 직선 스트로크 — 계수 벡터가 점 수와 같고, 전부 유한/범위
    // 안이며, 시작이 중간보다 진하고(뭉침), 반복 호출에도 동일합니다.
    let pts: Vec<StrokePoint> = (0..200)
        .map(|i| StrokePoint::with_time(i as f32 * 4.0, 100.0, 0.5, i as u64 * 5))
        .collect();
    let g = InkGrain {
        seed: 1234,
        ..InkGrain::default()
    };
    let f = stroke_ink_factors(ToolType::Pen, &pts, g);
    assert_eq!(f.len(), 200);
    assert!(
        f.iter().all(|v| v.is_finite() && (0.30..=1.60).contains(v)),
        "계수 범위 이탈: {:?}",
        &f[..8]
    );
    let mid_avg: f32 = f[60..140].iter().sum::<f32>() / 80.0;
    assert!(
        f[0..8].iter().sum::<f32>() / 8.0 > mid_avg + 0.05,
        "시작 뭉침이 계수에 반영 안 됨"
    );
    // 결정성: 같은 입력 두 번 호출 = 완전 동일.
    let f2 = stroke_ink_factors(ToolType::Pen, &pts, g);
    assert_eq!(f, f2);
    // 비활성화 시 균일 1.0.
    let off = stroke_ink_factors(
        ToolType::Pen,
        &pts,
        InkGrain {
            enabled: false,
            ..g
        },
    );
    assert!(off.iter().all(|v| (*v - 1.0).abs() < 1e-6));
}

#[test]
fn stroke_ink_lr_edges_denser_than_center_and_bounded() {
    // 좌우 단면 밀도 — 레일로드(가장자리 진함)가 구간 평균으로 확인되고,
    // 값 전부가 범위 안이며, 결정적이고, 비활성화 시 [1,1]입니다.
    let pts: Vec<StrokePoint> = (0..300)
        .map(|i| StrokePoint::with_time(i as f32 * 4.0, 100.0, 0.5, i as u64 * 5))
        .collect();
    let g = InkGrain {
        seed: 77,
        ..InkGrain::default()
    };
    let lr = stroke_ink_lr(ToolType::Pen, &pts, g);
    assert_eq!(lr.len(), 300);
    let center = stroke_ink_factors(ToolType::Pen, &pts, g);
    let mid_avg: f32 = center[60..240].iter().sum::<f32>() / 180.0;
    let edge_avg: f32 = lr[60..240].iter().map(|p| (p[0] + p[1]) * 0.5).sum::<f32>() / 180.0;
    assert!(
        edge_avg > mid_avg + 0.02,
        "가장자리가 중심보다 진해야 함: {edge_avg} vs {mid_avg}"
    );
    assert!(lr
        .iter()
        .flatten()
        .all(|v| v.is_finite() && (0.30..=1.60).contains(v)));
    // 비활성 → 전부 [1,1].
    let off = stroke_ink_lr(
        ToolType::Pen,
        &pts,
        InkGrain {
            enabled: false,
            ..g
        },
    );
    assert!(off.iter().all(|p| p[0] == 1.0 && p[1] == 1.0));
    // 결정성.
    assert_eq!(lr, stroke_ink_lr(ToolType::Pen, &pts, g));
}

#[test]
fn combine_saturation_is_monotone_and_bounded() {
    // 밀도가 높을수록 같은 sat에서 더 진하고, 결과는 항상 0..1.
    let a = combine_saturation(0.5, 1.0);
    let b = combine_saturation(0.5, 1.4);
    assert!(b >= a, "밀도가 진함을 증가시켜야 함");
    assert_eq!(combine_saturation(1.0, 1.4), 1.0, "상한 1.0");
    assert_eq!(combine_saturation(0.2, 0.0), 0.0);
    assert!((0.0..=1.0).contains(&combine_saturation(0.35, 1.3)));
}

#[test]
fn grain_deserializes_with_defaults() {
    // 이전 세션 파일(빈 객체)과의 호환성 — 기본값이 채워집니다.
    let g: InkGrain = serde_json::from_str("{}").unwrap();
    assert!(g.enabled);
    assert!(g.flow_amp > 0.0 && g.wick_amp > 0.0);
    assert!(g.pooling > 0.0 && g.starvation > 0.0);
    // 왕복.
    let json = serde_json::to_string(&InkGrain::default()).unwrap();
    let g2: InkGrain = serde_json::from_str(&json).unwrap();
    assert_eq!(g2, InkGrain::default());
}
