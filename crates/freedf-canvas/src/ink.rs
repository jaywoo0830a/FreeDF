//! 잉크 모델 — 폭/번짐/질감과 리본 메셔.
//!
//! 컴포저빌리티의 핵심: [`RibbonMesher`]는 [`WidthModel`]과 [`AlphaModel`]을
//! **조합**해서 만듭니다. 폭 모델만 교체하면 만년필, 알파 모델만 교체하면
//! 다른 번짐 — 테스트는 스텁 모델로 지오메트리를 결정적으로 검증합니다.
//!
//! 스켈레톤의 리본은 점 2개마다 사각형 1개(삼각형 2개)인 단순 지오메트리.
//! 실제 앱의 폭 리본(원형 캡/조인트)은 이 인터페이스 뒤에서 교체됩니다.

use crate::scene::{Stroke, StrokeId};

/// 굽기 시점의 점 문맥 — 폭 모델의 순수 입력.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrokeCtx {
    /// 필압 0..1.
    pub pressure: f32,
    /// 이동 속도 (pt/s).
    pub speed_pt_per_s: f32,
    /// 기준 두께 (pt).
    pub base_width: f32,
}

/// 폭 모델 — 문맥 → 실제 폭 (pt). 결정적이어야 합니다.
pub trait WidthModel: Send + Sync {
    fn width(&self, ctx: &StrokeCtx) -> f32;
}

/// 스켈레톤 볼펜 폭 모델 — 필압에 선형 반응 (계수는 골격값).
#[derive(Debug, Clone, Copy, Default)]
pub struct BallWidth;

impl WidthModel for BallWidth {
    fn width(&self, ctx: &StrokeCtx) -> f32 {
        ctx.base_width * (0.5 + 0.5 * ctx.pressure.clamp(0.0, 1.0))
    }
}

/// 알파(번짐) 모델 — 점이 쓰인 지 `age_ms` 후의 알파 0..1.
pub trait AlphaModel: Send + Sync {
    fn alpha_at(&self, age_ms: u64) -> f32;
}

/// 스켈레톤 번짐 모델 — `saturate_sec` 동안 선형 포화.
#[derive(Debug, Clone, Copy)]
pub struct SoakAlpha {
    pub saturate_sec: f32,
}

impl Default for SoakAlpha {
    fn default() -> Self {
        Self { saturate_sec: 2.0 }
    }
}

impl AlphaModel for SoakAlpha {
    fn alpha_at(&self, age_ms: u64) -> f32 {
        (age_ms as f32 / 1000.0 / self.saturate_sec.max(1e-3)).clamp(0.0, 1.0)
    }
}

/// 질감 시드 — **같은 획은 항상 같은 밀도** (id 기반 결정적 해시).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GrainSeed(pub u64);

impl GrainSeed {
    pub fn of(stroke_id: StrokeId) -> Self {
        Self(stroke_id.0.wrapping_mul(0x9E37_79B9_7F4A_7C15))
    }

    /// 점 인덱스의 밀도 0..1 — 결정적, 무상태.
    pub fn density(&self, point_index: usize) -> f32 {
        let x = self.0 ^ (point_index as u64).wrapping_mul(0xD6E8_FEB8_6659_FD93);
        let h = x.wrapping_mul(x ^ (x >> 32));
        ((h >> 40) as u32 as f32) / ((1u32 << 24) as f32)
    }
}

/// CPU 쪽 메시 — GPU/래스터 업로드 직전 형태. 페이지 좌표(pt)로 구워집니다.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mesh {
    pub vertices: Vec<[f32; 2]>,
    /// 정점 색 (r, g, b, a) — 0..1.
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
}

impl Mesh {
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    /// 다른 메시를 뒤에 이어 붙입니다 (증분 append의 기본 연산).
    pub fn append(&mut self, other: &Mesh) {
        let base = self.vertices.len() as u32;
        self.vertices.extend_from_slice(&other.vertices);
        self.colors.extend_from_slice(&other.colors);
        self.indices
            .extend(other.indices.iter().map(|i| *i + base));
    }

    /// 정점 수와 색 수가 일치하고 인덱스가 범위 안인지 (굽기 결과 검증용).
    pub fn is_well_formed(&self) -> bool {
        if self.vertices.len() != self.colors.len() {
            return false;
        }
        let n = self.vertices.len() as u32;
        self.indices.iter().all(|i| *i < n)
    }
}

/// 스트로크 → 메시 변환. `now_ms`는 번짐 나이 계산용 (시간은 인자로).
pub trait Mesher: Send + Sync {
    fn mesh(&self, stroke: &Stroke, now_ms: u64) -> Mesh;
}

/// 조합형 리본 메셔 — `Width × Alpha`를 생성자에서 조립.
pub struct RibbonMesher<W: WidthModel, A: AlphaModel> {
    width_model: W,
    alpha_model: A,
}

impl<W: WidthModel, A: AlphaModel> RibbonMesher<W, A> {
    pub fn new(width_model: W, alpha_model: A) -> Self {
        Self {
            width_model,
            alpha_model,
        }
    }
}

impl<W: WidthModel, A: AlphaModel> Mesher for RibbonMesher<W, A> {
    fn mesh(&self, stroke: &Stroke, now_ms: u64) -> Mesh {
        let mut mesh = Mesh::default();
        let color = [
            stroke.color[0] as f32 / 255.0,
            stroke.color[1] as f32 / 255.0,
            stroke.color[2] as f32 / 255.0,
            stroke.color[3] as f32 / 255.0,
        ];
        let seed = GrainSeed::of(stroke.id);
        for (i, pair) in stroke.points.windows(2).enumerate() {
            let (a, b) = (pair[0], pair[1]);
            let dx = b.position.x - a.position.x;
            let dy = b.position.y - a.position.y;
            let len = (dx * dx + dy * dy).sqrt();
            // 스켈레톤: 퇴화 세그먼트는 건너뜀 (실 리본은 캡으로 처리).
            if len < 1e-4 {
                continue;
            }
            let speed = len / ((b.t_ms.saturating_sub(a.t_ms) as f32 / 1000.0).max(1e-3));
            let ctx = StrokeCtx {
                pressure: 0.5 * (a.pressure + b.pressure),
                speed_pt_per_s: speed,
                base_width: stroke.base_width,
            };
            let half = self.width_model.width(&ctx) * 0.5;
            // 질감 밀도가 좌우 알파를 변조 (스켈레톤: 좌우 동일).
            let grain = 0.5 + 0.5 * seed.density(i);
            let age = now_ms.saturating_sub(b.t_ms.min(now_ms));
            let alpha = self.alpha_model.alpha_at(age) * grain;
            let nx = -dy / len;
            let ny = dx / len;
            let base = mesh.vertices.len() as u32;
            mesh.vertices.extend_from_slice(&[
                [a.position.x + nx * half, a.position.y + ny * half],
                [a.position.x - nx * half, a.position.y - ny * half],
                [b.position.x + nx * half, b.position.y + ny * half],
                [b.position.x - nx * half, b.position.y - ny * half],
            ]);
            for _ in 0..4 {
                mesh.colors.push([color[0], color[1], color[2], color[3] * alpha]);
            }
            mesh.indices
                .extend_from_slice(&[base, base + 1, base + 2, base + 1, base + 3, base + 2]);
        }
        mesh
    }
}

/// 편의 함수 — 스냅샷의 모든 스트로크를 `now_ms` 시각으로 굽습니다.
pub fn bake_strokes<M: Mesher>(mesher: &M, strokes: &[Stroke], now_ms: u64) -> Mesh {
    let mut out = Mesh::default();
    for s in strokes {
        out.append(&mesher.mesh(s, now_ms));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::PagePoint;
    use crate::scene::{LayerKind, StrokePoint, StrokeId};

    fn stroke_at(id: u64, x0: f32, x1: f32) -> Stroke {
        Stroke {
            id: StrokeId(id),
            kind: LayerKind::Ink,
            color: [0, 0, 0, 255],
            base_width: 2.0,
            points: vec![
                StrokePoint {
                    position: PagePoint::new(x0, 0.0),
                    pressure: 1.0,
                    t_ms: 1_000,
                },
                StrokePoint {
                    position: PagePoint::new(x1, 0.0),
                    pressure: 1.0,
                    t_ms: 1_010,
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
}
