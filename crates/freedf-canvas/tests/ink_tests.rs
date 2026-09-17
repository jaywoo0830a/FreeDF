//! `ink` 모듈 단위 테스트 — `src/ink.rs`의 `#[cfg(test)]` 모듈에서 이동했습니다.

use freedf_canvas::ink::*;
use freedf_canvas::scene::{LayerKind, Stroke, StrokeId, StrokePoint};
use freedf_canvas::geom::PagePoint;
use freedf_core::model::ToolType;

fn stroke_at(id: u64, x0: f32, x1: f32) -> Stroke {
    Stroke {
        id: StrokeId(id),
        kind: LayerKind::Ink,
        tool: ToolType::Pen,
        color: [0, 0, 0, 255],
        base_width: 2.0,
        points: vec![
            StrokePoint {
                position: PagePoint::new(x0, 0.0),
                pressure: 1.0,
                t_ms: 1_000,
                width: 0.0,
            },
            StrokePoint {
                position: PagePoint::new(x1, 0.0),
                pressure: 1.0,
                t_ms: 1_010,
                width: 0.0,
            },
        ],
        created_ms: 1_000,
    }
}

/// 계약: 폭 모델은 필압이 0이면 절반, 1이면 기준 폭 (스켈레톤 모델).
#[test]
fn ball_width_reacts_to_pressure() {
    let model = BallWidth;
    let base = 2.0;
    assert!((model.width(&StrokeCtx { pressure: 0.0, speed_pt_per_s: 0.0, base_width: base }) - 1.0).abs() < 1e-6);
    assert!((model.width(&StrokeCtx { pressure: 1.0, speed_pt_per_s: 0.0, base_width: base }) - 2.0).abs() < 1e-6);
}

/// 계약: 번짐 모델은 0 → 0, 포화 시간 이후 → 1.
#[test]
fn soak_alpha_saturates_over_time() {
    let model = SoakAlpha { saturate_sec: 2.0 };
    assert_eq!(model.alpha_at(0), 0.0);
    assert_eq!(model.alpha_at(1_000), 0.5);
    assert_eq!(model.alpha_at(5_000), 1.0);
}

/// 계약: 질감 시드는 결정적 — 같은 입력은 항상 같은 밀도.
#[test]
fn grain_seed_is_deterministic() {
    let a = GrainSeed::of(StrokeId(7));
    let b = GrainSeed::of(StrokeId(7));
    for i in 0..64 {
        assert_eq!(a.density(i), b.density(i));
        assert!((0.0..=1.0).contains(&a.density(i)));
    }
}

/// 계약: 조합형 메셔 — 폭/알파 스텁으로 지오메트리를 결정적으로 검증.
#[test]
fn ribbon_mesher_composes_models() {
    struct StubWidth;
    impl WidthModel for StubWidth {
        fn width(&self, _ctx: &StrokeCtx) -> f32 {
            2.0 // half = 1.0
        }
    }
    struct StubAlpha;
    impl AlphaModel for StubAlpha {
        fn alpha_at(&self, _age_ms: u64) -> f32 {
            1.0
        }
    }
    let mesher = RibbonMesher::new(StubWidth, StubAlpha);
    let mesh = mesher.mesh(&stroke_at(1, 0.0, 10.0), 0);
    assert!(mesh.is_well_formed());
    assert_eq!(mesh.vertices.len(), 4, "세그먼트 1개 = 사각형 1개");
    assert_eq!(mesh.indices.len(), 6, "삼각형 2개");
    // 수평선 → 상하 1pt 벗어난 정점.
    for v in &mesh.vertices {
        assert!((v[1].abs() - 1.0).abs() < 1e-4, "y: {}", v[1]);
    }
}

/// 계약: append는 인덱스 오프셋을 보정해 이어 붙입니다 (증분의 근거).
#[test]
fn mesh_append_offsets_indices() {
    let mesher = RibbonMesher::new(BallWidth, SoakAlpha::default());
    let part1 = mesher.mesh(&stroke_at(1, 0.0, 10.0), 1_100);
    let part2 = mesher.mesh(&stroke_at(2, 20.0, 30.0), 1_100);
    let mut merged = Mesh::default();
    merged.append(&part1);
    merged.append(&part2);
    assert_eq!(merged.vertices.len(), part1.vertices.len() + part2.vertices.len());
    assert!(merged.is_well_formed());
}
