//! `core_mesh` 모듈 단위 테스트 — `src/core_mesh.rs`의 `#[cfg(test)]` 모듈에서 이동했습니다.

use freedf_canvas::core_mesh::*;
use freedf_canvas::geom::PagePoint;
use freedf_canvas::ink::Mesh;
use freedf_canvas::scene::{LayerKind, StrokeId};
use freedf_canvas::scene::{Stroke, StrokePoint};
use freedf_core::ink::InkGrain;
use freedf_core::model::ToolType;
use freedf_core::pen::{BallPenProfile, FountainProfile};
use freedf_core::pen::{InkSoak, Materials};

fn point(x: f32, y: f32, pressure: f32, t_ms: u64, width: f32) -> StrokePoint {
    StrokePoint {
        position: PagePoint::new(x, y),
        pressure,
        t_ms,
        width,
    }
}

fn stroke(tool: ToolType, pts: Vec<StrokePoint>) -> Stroke {
    Stroke {
        id: StrokeId(1),
        kind: LayerKind::Ink,
        tool,
        color: [0, 0, 0, 255],
        base_width: 2.0,
        points: pts,
        created_ms: 1_000,
    }
}

fn mesher() -> CoreRibbonMesher {
    CoreRibbonMesher {
        materials: Materials::new(BallPenProfile::default(), FountainProfile::default()),
        pen_soak: InkSoak::ballpoint_default(),
        fountain_soak: InkSoak::fountain_default(),
        pen_grain: InkGrain::default(),
        fountain_grain: InkGrain::default(),
        tilt_magnitude: 0.0,
        feather_pt: 1.0,
    }
}

/// 계약: 잠금된 폭이 있으면 프로파일을 무시하고 그 폭을 씁니다.
#[test]
fn halves_prefer_locked_widths() {
    let pts = vec![point(0.0, 0.0, 0.5, 0, 3.0), point(10.0, 0.0, 0.5, 10, 3.0)];
    let halves = halves_for_stroke(ToolType::Pen, 2.0, &pts, &Materials::default(), 0.0);
    assert!(halves.iter().all(|h| (*h - 1.5).abs() < 1e-4), "{halves:?}");
}

/// 계약: 알파는 좌우 쌍으로 0..1 안에 머물고, 펜/만년필이 아니면 None.
#[test]
fn alphas_bounded_and_tool_gated() {
    let m = mesher();
    let pen = stroke(ToolType::Pen, vec![point(0.0, 0.0, 0.5, 900, 0.0)]);
    let pairs = alphas_for_stroke(
        pen.tool,
        &pen.points,
        pen.created_ms,
        pen.id.0,
        &m.pen_soak,
        &m.fountain_soak,
        &m.pen_grain,
        &m.fountain_grain,
        1_000,
    )
    .expect("펜은 Some");
    assert!(pairs.iter().flatten().all(|a| (0.0..=1.0).contains(a)));

    let hi = stroke(ToolType::Highlighter, vec![point(0.0, 0.0, 0.5, 0, 0.0)]);
    assert!(
        alphas_for_stroke(
            hi.tool,
            &hi.points,
            hi.created_ms,
            hi.id.0,
            &m.pen_soak,
            &m.fountain_soak,
            &m.pen_grain,
            &m.fountain_grain,
            1_000,
        )
        .is_none(),
        "하이라이터는 알파 변조 없음"
    );
}

/// 계약: 증분 append — 새 획 리본만 붙여도 유한한 정점으로 커버.
#[test]
fn append_stroke_grows_mesh_with_bounded_geometry() {
    let m = mesher();
    let mut mesh = Mesh::default();
    let s1 = stroke(
        ToolType::Pen,
        vec![
            point(0.0, 0.0, 0.5, 1_000, 2.0),
            point(10.0, 0.0, 0.5, 1_010, 2.0),
        ],
    );
    let s2 = stroke(
        ToolType::Pen,
        vec![
            point(100.0, 0.0, 0.5, 1_000, 2.0),
            point(110.0, 0.0, 0.5, 1_010, 2.0),
        ],
    );
    m.append_stroke(&mut mesh, &s1, 1_100);
    let n0 = mesh.vertices.len();
    m.append_stroke(&mut mesh, &s2, 1_100);
    assert!(mesh.vertices.len() > n0, "append가 정점 추가");
    assert!(mesh.is_well_formed());
    let max_x = mesh.vertices.iter().map(|v| v[0]).fold(f32::MIN, f32::max);
    assert!(max_x > 100.0, "새 획 위치까지 커버: {max_x}");
    for v in &mesh.vertices {
        assert!(v[0].is_finite() && v[1].is_finite(), "NaN: {v:?}");
    }
}

/// 계약: 같은 입력은 같은 메시 (결정성 — 굽기 캐시의 근거).
#[test]
fn mesher_is_deterministic() {
    use freedf_canvas::ink::Mesher;
    let m = mesher();
    let s = stroke(
        ToolType::Pen,
        vec![
            point(0.0, 0.0, 0.5, 1_000, 2.0),
            point(10.0, 0.0, 0.5, 1_010, 2.0),
        ],
    );
    assert_eq!(m.mesh(&s, 1_100), m.mesh(&s, 1_100));
}

/// P0 벤치 (기본 실행 제외 — `cargo test -- --ignored --nocapture`).
/// live 렌더 재구성의 점당 비용을 측정해 `docs/OPTIMIZATION.md` §4 baseline을
/// 실측 갱신하는 데 씁니다. 벤치는 타이밍이라 CI에선 실행하지 않습니다.
#[test]
#[ignore]
fn bench_live_render_cost_per_point() {
    use std::time::Instant;
    let m = mesher();
    for &n in &[1_000usize, 10_000] {
        let pts: Vec<StrokePoint> = (0..n)
            .map(|i| {
                point(
                    (i as f32 % 200.0) * 0.5,
                    (i as f32 / 200.0) * 0.3,
                    0.4 + 0.5 * ((i as f32 * 0.017).sin()).abs(),
                    i as u64 * 1000,
                    2.0, // 잠금 폭 → halves fast path
                )
            })
            .collect();
        let s = stroke(ToolType::Pen, pts);
        // 예열 (할당/캐시 워밍).
        for _ in 0..3 {
            let mut me = Mesh::default();
            m.append_stroke(&mut me, &s, 1_100);
        }
        let mut best = f64::MAX;
        for _ in 0..7 {
            let mut me = Mesh::default();
            let t0 = Instant::now();
            m.append_stroke(&mut me, &s, 1_100);
            let dt = t0.elapsed().as_secs_f64();
            best = best.min(dt);
        }
        let ns_per_pt = best * 1e9 / n as f64;
        eprintln!(
            "[P0-baseline] n={n} append_stroke: {:.0} µs total, {:.2} ns/pt",
            best * 1e6,
            ns_per_pt
        );
    }
}
