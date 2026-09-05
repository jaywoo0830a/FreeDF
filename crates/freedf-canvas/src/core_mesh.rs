//! freedf-core 리본 지오메트리와의 접착 — **실제 렌더에 쓰는 굽기 코드**.
//!
//! `freedf` 앱에서 검증된 계산들을 이 크레이트로 옮긴 순수 구현입니다:
//! - [`halves_for_stroke`] — 도구별 점 단위 절반 두께 (입력 잠금 폭 우선,
//!   없으면 프로파일 배치 계산).
//! - [`alphas_for_stroke`] — 잉크 스밈 포화 × 질감 밀도의 좌우 알파 쌍.
//! - [`append_stroke_ribbon`] — freedf-core `stroke_ribbon_lr`을 [`Mesh`]에
//!   페이지 좌표로 덧붙임 (변환은 그리기 단계 `Transform`이 담당).
//! - [`CoreRibbonMesher`] — 위 세 함수를 묶은 [`Mesher`] 구현 (조합형).
//! - [`append_stroke`] — 증분 append의 단위: 리본 추가 + 다음 블리드 정착 시각.

use freedf_core::ink::{combine_saturation, stroke_ink_lr, InkGrain};
use freedf_core::model::ToolType;
use freedf_core::pen::{BallPenProfile, FountainProfile, InkSoak};

use crate::ink::Mesh;
use crate::scene::{Stroke, StrokePoint};

/// 잉크 질감 시드 — **같은 획은 항상 같은 질감** (획 시작 시각/ID 기반).
fn seeded_grain(grain: InkGrain, seed: u64) -> InkGrain {
    InkGrain {
        seed: grain.seed ^ seed.wrapping_mul(0x9E37_79B9_7F4A_7C15),
        ..grain
    }
}

/// 도구별 점 단위 절반 두께(pt).
///
/// 입력 시점에 잠금된 폭(`StrokePoint.width`)이 있으면 그대로 쓰고,
/// 없으면(이전 데이터) 프로파일 배치 계산으로 폴백합니다.
pub fn halves_for_stroke(
    tool: ToolType,
    base_width: f32,
    points: &[StrokePoint],
    ball: &BallPenProfile,
    fountain: &FountainProfile,
    tilt_mag: f32,
) -> Vec<f32> {
    let n = points.len();
    let locked = !points.is_empty() && points.iter().all(|p| p.width > 0.0);
    if tool == ToolType::Highlighter {
        // 마커: 필압/테이퍼 없이 일정한 두께 (잠금 폭도 동일 규칙).
        let mut halves = Vec::with_capacity(n);
        if locked {
            for p in points {
                halves.push((p.width * 0.5).max(0.5));
            }
        } else {
            halves.resize(n, (base_width * 0.5).max(0.5));
        }
        return halves;
    }
    let mut halves = Vec::with_capacity(n);
    if locked {
        for p in points {
            halves.push((p.width * 0.5).max(0.05));
        }
        return halves;
    }
    let core_pts: Vec<freedf_core::model::StrokePoint> = points
        .iter()
        .map(|p| freedf_core::model::StrokePoint {
            x: p.position.x,
            y: p.position.y,
            pressure: p.pressure,
            t_ms: p.t_ms,
            width: p.width,
        })
        .collect();
    if tool == ToolType::Fountain {
        for w in fountain.widths(base_width, &core_pts, tilt_mag) {
            halves.push((w * 0.5).max(0.05));
        }
    } else {
        for w in ball.widths(base_width, &core_pts, tilt_mag) {
            halves.push((w * 0.5).max(0.05));
        }
    }
    halves
}

/// 잉크 스밈 포화 × 질감 밀도의 **점별 좌우 알파 쌍** (0..1).
/// 펜/만년필이 아니면 `None` (알파 변조 없음).
pub fn alphas_for_stroke(
    tool: ToolType,
    points: &[StrokePoint],
    created_ms: u64,
    stroke_id: u64,
    pen_soak: &InkSoak,
    fountain_soak: &InkSoak,
    pen_grain: &InkGrain,
    fountain_grain: &InkGrain,
    now_ms: u64,
) -> Option<Vec<[f32; 2]>> {
    if !matches!(tool, ToolType::Pen | ToolType::Fountain) {
        return None;
    }
    let (soak, grain) = if tool == ToolType::Fountain {
        (fountain_soak, fountain_grain)
    } else {
        (pen_soak, pen_grain)
    };
    let created = if created_ms > 0 { created_ms } else { stroke_id };
    let grain = seeded_grain(*grain, created);
    let core_pts: Vec<freedf_core::model::StrokePoint> = points
        .iter()
        .map(|p| freedf_core::model::StrokePoint {
            x: p.position.x,
            y: p.position.y,
            pressure: p.pressure,
            t_ms: p.t_ms,
            width: p.width,
        })
        .collect();
    let dens = stroke_ink_lr(tool, &core_pts, grain);
    let soak_on = soak.enabled;
    Some(
        points
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let sat = if soak_on {
                    let age = if p.t_ms == 0 {
                        soak.saturate_sec
                    } else {
                        (now_ms.saturating_sub(p.t_ms)) as f32 / 1000.0
                    };
                    soak.sat_at(age)
                } else {
                    1.0
                };
                [
                    combine_saturation(sat, dens[i][0]),
                    combine_saturation(sat, dens[i][1]),
                ]
            })
            .collect(),
    )
}

/// freedf-core 리본 지오메트리를 [`Mesh`]에 **페이지 좌표**로 덧붙입니다.
/// (화면 변환은 그리기 단계 `Transform` — 팬/줌만 바뀌면 재굽기 불필요.)
pub fn append_stroke_ribbon(
    mesh: &mut Mesh,
    points: &[StrokePoint],
    halves: &[f32],
    feather_pt: f32,
    round_caps: bool,
    color: [u8; 4],
    alphas: Option<&[[f32; 2]]>,
) {
    let pts: Vec<[f32; 2]> = points
        .iter()
        .map(|p| [p.position.x, p.position.y])
        .collect();
    let ribbon =
        freedf_core::pen::stroke_ribbon_lr(&pts, halves, feather_pt, round_caps, alphas);
    let base = mesh.vertices.len() as u32;
    let cf = |v: u8| v as f32 / 255.0;
    for (p, a) in ribbon.verts.iter().zip(&ribbon.alphas) {
        mesh.vertices.push(*p);
        mesh.colors.push([
            cf(color[0]),
            cf(color[1]),
            cf(color[2]),
            cf(color[3]) * a,
        ]);
    }
    for t in &ribbon.tris {
        mesh.indices
            .extend_from_slice(&[base + t[0], base + t[1], base + t[2]]);
    }
}

/// 실제 렌더에 쓰는 조합형 메셔 — freedf-core 프로파일/스밈/질감을
/// [`Mesher`] 인터페이스 뒤에 묶습니다.
#[derive(Debug, Clone, Copy)]
pub struct CoreRibbonMesher {
    pub ball: BallPenProfile,
    pub fountain: FountainProfile,
    pub pen_soak: InkSoak,
    pub fountain_soak: InkSoak,
    pub pen_grain: InkGrain,
    pub fountain_grain: InkGrain,
    /// 펜 틸트 크기 0..1.
    pub tilt_magnitude: f32,
    /// 화면 1px에 해당하는 pt — 줌에서 계산 (1/zoom).
    pub feather_pt: f32,
}

impl CoreRibbonMesher {
    /// 스트로크의 다음 블리드 정착 시각 (u64::MAX = 없음).
    pub fn next_settle(&self, stroke: &Stroke, now_ms: u64) -> u64 {
        let soak = if stroke.tool == ToolType::Fountain {
            self.fountain_soak
        } else {
            self.pen_soak
        };
        if !matches!(stroke.tool, ToolType::Pen | ToolType::Fountain) || !soak.enabled {
            return u64::MAX;
        }
        let deadline = (soak.saturate_sec.max(1e-3) * 1000.0) as u64;
        let mut settle = u64::MAX;
        for p in &stroke.points {
            if p.t_ms > 0 {
                let s = p.t_ms.saturating_add(deadline);
                if now_ms < s {
                    settle = settle.min(s);
                }
            }
        }
        settle
    }

    /// 스트로크 하나를 `mesh`에 증분 추가하고, 다음 블리드 정착 시각
    /// (u64::MAX = 없음)을 반환합니다.
    pub fn append_stroke(&self, mesh: &mut Mesh, stroke: &Stroke, now_ms: u64) -> u64 {
        let settle = self.next_settle(stroke, now_ms);
        if stroke.points.is_empty() {
            return settle;
        }
        let halves = halves_for_stroke(
            stroke.tool,
            stroke.base_width,
            &stroke.points,
            &self.ball,
            &self.fountain,
            self.tilt_magnitude,
        );
        let alphas = alphas_for_stroke(
            stroke.tool,
            &stroke.points,
            stroke.created_ms,
            stroke.id.0,
            &self.pen_soak,
            &self.fountain_soak,
            &self.pen_grain,
            &self.fountain_grain,
            now_ms,
        );
        let round_caps = matches!(stroke.tool, ToolType::Pen | ToolType::Fountain);
        append_stroke_ribbon(
            mesh,
            &stroke.points,
            &halves,
            self.feather_pt,
            round_caps,
            stroke.color,
            alphas.as_deref(),
        );
        settle
    }
}

impl crate::ink::Mesher for CoreRibbonMesher {
    fn mesh(&self, stroke: &Stroke, now_ms: u64) -> Mesh {
        let mut mesh = Mesh::default();
        self.append_stroke(&mut mesh, stroke, now_ms);
        mesh
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::PagePoint;
    use crate::scene::{LayerKind, StrokeId};

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
            ball: BallPenProfile::default(),
            fountain: FountainProfile::default(),
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
        let halves = halves_for_stroke(
            ToolType::Pen,
            2.0,
            &pts,
            &BallPenProfile::default(),
            &FountainProfile::default(),
            0.0,
        );
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
            vec![point(0.0, 0.0, 0.5, 1_000, 2.0), point(10.0, 0.0, 0.5, 1_010, 2.0)],
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
        use crate::ink::Mesher;
        let m = mesher();
        let s = stroke(
            ToolType::Pen,
            vec![point(0.0, 0.0, 0.5, 1_000, 2.0), point(10.0, 0.0, 0.5, 1_010, 2.0)],
        );
        assert_eq!(m.mesh(&s, 1_100), m.mesh(&s, 1_100));
    }
}
