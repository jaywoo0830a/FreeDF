//! 잉크 질감(입체적 불균일) 모델 — 결정적 노이즈 + 도구별 잉크 물리.
//!
//! 실제 잉크는 색이 완벽히 일정하지 않습니다. 이 모듈은 그 불균일을
//! **획 공간 좌표의 결정적 함수**로 모델링합니다:
//!
//! - `u`: 획을 따라 진행한 거리 비율 (0..1, 호 길이 기준)
//! - `v`: 단면 위치 (-1 = 왼쪽 가장자리, 0 = 중심, +1 = 오른쪽 가장자리)
//! - `seed`: 획별 시드 — 스트로크 id를 넣으면 필적마다 다른 질감
//!
//! 네 가지 성분:
//! 1. **저주파 흐름 물결** (`flow_amp`) — 잉크 공급이 들쭉날쭉해 생기는
//!    수 cm 규모의 밀도 변화.
//! 2. **고주파 위킹** (`wick_amp`) — 종이 섬유 사이로 잉크가 스며드는
//!    미세 얼룩 (만년필에서 더 큼).
//! 3. **물리 형태** (`pooling`, `starvation`) — 볼펜 시작 뭉침/끝 축적,
//!    만년필 시작·끝 고임, 빨리 쓰면 옅어지는 결핍.
//! 4. **단면 효과** — 가장자리가 중심보다 진함 (잉크 메니스커스의
//!    레일로드 효과, 볼펜에서 두드러짐).
//!
//! 시간을 키로 쓰지 않으므로 **같은 획은 항상 같은 밀도** — 렌더링 중
//! 깜빡임/물결이 없습니다.

use crate::model::{StrokePoint, ToolType};
use serde::{Deserialize, Serialize};

/// 잉크 질감 설정. `Default`가 자연스러운 미묘한 값입니다.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InkGrain {
    /// 잉크 불균일 효과 활성화 (끄면 모든 밀도가 1.0).
    pub enabled: bool,
    /// 획별 시드 — 스트로크 id(해시)를 넣으면 필적마다 다른 질감.
    pub seed: u64,
    /// **시각 근사**: 켜면 고주파 위킹 옥타브를 생략해 질감 계산을 절반으로
    /// 줄입니다 (**기본 true** — 분석적 2옥타브보다 부드러움). Debug HUD의
    /// "Fast ink noise" 토글로 정확 2옥타브와 라이브 비교합니다.
    pub fast_noise: bool,
    /// 저주파(잉크 흐름 물결) 진폭. 0이면 흐름 변화 없음.
    pub flow_amp: f32,
    /// 고주파(종이 섬유 위킹) 진폭.
    pub wick_amp: f32,
    /// 시작/끝 잉크 고임 강도 (볼펜 뭉침·만년필 풀링).
    pub pooling: f32,
    /// 속도 결핍 강도 — 빨리 쓰면 옅어짐 (만년필 중심).
    pub starvation: f32,
}

impl Default for InkGrain {
    fn default() -> Self {
        Self {
            enabled: true,
            seed: 0x5EED_5EED,
            fast_noise: true,
            // 미묘한 수준을 넘어 눈에 보이는 질감으로 조정 (2026-09):
            // 팬 노이즈 ±~19%, 만년필 ±~31% 밀도 요동.
            flow_amp: 0.18,
            wick_amp: 0.13,
            pooling: 0.34,
            starvation: 0.42,
        }
    }
}

/// fast_noise 전용 — 고주파(위킹) 옥타브를 생략한 저비용 필드 (흐름 옥타브만).
/// 값 노이즈 보간을 유지해 점프(popping)가 없고, 결정적입니다.
fn ink_field_fast(u: f32, v: f32, seed: u64) -> f32 {
    value_noise(u * 3.0, v * 1.2, seed)
}

/// fast_noise 전용 단면(좌/우) 필드 쌍 — x 공통 부분을 공유하고 위킹은 생략.
fn ink_field_pair_fast(u: f32, seed: u64) -> [f32; 2] {
    value_noise_pair(u * 3.0, -1.2, 1.2, seed)
}

/// 속도 정규화 기준 (pt/s) — 만년필 모델의 speed_ref 기본값과 동일.
const INK_SPEED_REF: f32 = 150.0;

/// 2D 정수 격자 해시 — (ix, iy, seed) → 균일 분포 0..1 값.
/// splitmix64 계열 파이널라이저로 음수 좌표도 안전하게 섞입니다.
pub(crate) fn hash2(ix: i32, iy: i32, seed: u64) -> f32 {
    let mut h = seed;
    h ^= (ix as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h = h.wrapping_mul(0xD1B5_4A32_D192_ED03);
    h ^= (iy as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h = h.wrapping_mul(0xD1B5_4A32_D192_ED03);
    h ^= h >> 29;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 32;
    (h & 0x00FF_FFFF) as f32 / 16_777_216.0
}

/// 2D 값 노이즈 (0..1) — 격자점 해시를 스무스스텝으로 보간.
/// 격자 경계에서 기울기가 0이므로 연속적이고, **항상 결정적**입니다.
pub fn value_noise(x: f32, y: f32, seed: u64) -> f32 {
    let ix = x.floor();
    let iy = y.floor();
    let fx = x - ix;
    let fy = y - iy;
    // 스무스스텝 — 경계에서 도함수 0 (이음매 없음).
    let sx = fx * fx * (3.0 - 2.0 * fx);
    let sy = fy * fy * (3.0 - 2.0 * fy);
    let a = hash2(ix as i32, iy as i32, seed);
    let b = hash2(ix as i32 + 1, iy as i32, seed);
    let c = hash2(ix as i32, iy as i32 + 1, seed);
    let d = hash2(ix as i32 + 1, iy as i32 + 1, seed);
    let lo = a + (b - a) * sx;
    let hi = c + (d - c) * sx;
    lo + (hi - lo) * sy
}

/// 두 세로(side) 값에 대해 **x 공통 부분(정수 x셀·`fx`·`sx`)을 1번만** 계산하는 쌍 버전.
/// 단면(l/r) 밀도는 같은 `u`를 쓰므로 x 응답을 재사용해 산술을 절반으로 줄입니다.
/// 결과는 `value_noise(x, y0)` / `value_noise(x, y1)`와 **완전히 동일**합니다.
fn value_noise_pair(x: f32, y0: f32, y1: f32, seed: u64) -> [f32; 2] {
    let ix = x.floor();
    let fx = x - ix;
    let sx = fx * fx * (3.0 - 2.0 * fx);
    let eval = |y: f32| -> f32 {
        let iy = y.floor();
        let fy = y - iy;
        let sy = fy * fy * (3.0 - 2.0 * fy);
        let a = hash2(ix as i32, iy as i32, seed);
        let b = hash2(ix as i32 + 1, iy as i32, seed);
        let c = hash2(ix as i32, iy as i32 + 1, seed);
        let d = hash2(ix as i32 + 1, iy as i32 + 1, seed);
        let lo = a + (b - a) * sx;
        let hi = c + (d - c) * sx;
        lo + (hi - lo) * sy
    };
    [eval(y0), eval(y1)]
}

/// 획 공간 잉크 필드 (0..1) — 저주파 흐름 + 고주파 위킹 옥타브 합성.
/// `u`는 진행 방향(호 길이 비율), `v`는 단면 위치(-1..1).
pub fn ink_field(u: f32, v: f32, seed: u64) -> f32 {
    // 저주파: 잉크 흐름 물결 (진행 방향으로 길게 늘어진 세포).
    let flow = value_noise(u * 3.0, v * 1.2, seed);
    // 고주파: 종이 섬유 위킹 (미세 얼룩) — 시드 뒤섞기로 옥타브 독립.
    let wick = value_noise(
        u * 13.0 + 5.3,
        v * 4.0 + 2.9,
        seed.wrapping_mul(0x9E37_79B9_7F4A_7C15),
    );
    0.72 * flow + 0.28 * wick
}

/// 단면(왼/오) 전체 잉크 필드 쌍 — `v=-1`(왼)·`v=+1`(오)를 두 옥타브의
/// x 공통 부분을 공유해 병렬 계산합니다. 결과는 `[ink_field(u,-1), ink_field(u,+1)]`과 동일.
fn ink_field_pair(u: f32, seed: u64) -> [f32; 2] {
    let wick_seed = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let flow = value_noise_pair(u * 3.0, -1.2, 1.2, seed);
    let wick = value_noise_pair(
        u * 13.0 + 5.3,
        -1.0 * 4.0 + 2.9,
        1.0 * 4.0 + 2.9,
        wick_seed,
    );
    [
        0.72 * flow[0] + 0.28 * wick[0],
        0.72 * flow[1] + 0.28 * wick[1],
    ]
}

/// 볼펜(일반 펜)의 진행 방향 밀도 형태: 시작 뭉침 + 회복 딥 + 끝 축적,
/// 속도가 빠르면 아주 살짝 옅어집니다.
fn ballpoint_shape(u: f32, speed: f32, g: InkGrain) -> f32 {
    // 볼이 처음 회전할 때 잉크가 뭉쳐 나오는 시작 얼룩.
    let blob = g.pooling * 1.4 * (-u / 0.018).exp();
    // 시작 직후 잉크 흐름이 회복되기 전의 옅은 구간.
    let dip = -0.12 * (-(u - 0.05).powi(2) / 0.0009).exp();
    // 펜을 멈추기 직전 잉크가 볼에 축적되는 끝 구슬.
    let bead = g.pooling * 0.8 * (-(1.0 - u) / 0.012).exp();
    let starve = 1.0 - 0.12 * speed;
    (1.0 + blob + dip + bead) * starve
}

/// 만년필의 진행 방향 밀도 형태: 양 끝 고임(풀링) + 속도 결핍.
fn fountain_shape(u: f32, speed: f32, g: InkGrain) -> f32 {
    // 닙이 종이에 닿는 순간과 떨어지는 순간 잉크가 고입니다.
    let pool = g.pooling
        * (0.5 * (-u / 0.035).exp() + 0.5 * (-(1.0 - u) / 0.035).exp());
    // 빨리 쓰면 공급이 따라가지 못해 옅어집니다 (폭과 별개로 색도).
    let starve = 1.0 - g.starvation * speed;
    (1.0 + pool) * starve
}

impl InkGrain {
    /// 점 하나의 잉크 밀도 배율 (0.3 ~ 1.6, 1.0 = 완전 균일).
    ///
    /// - `tool`: Pen = 볼펜, Fountain = 만년필 (나머지 도구는 볼펜 취급).
    /// - `u`: 진행 방향 위치 0..1 (호 길이 비율).
    /// - `v`: 단면 위치 -1..1 (-1/1 = 가장자리, 0 = 중심).
    /// - `speed_norm`: 속도 0..1 (150pt/s 기준 정규화).
    pub fn density(self, tool: ToolType, u: f32, v: f32, speed_norm: f32) -> f32 {
        let u = u.clamp(0.0, 1.0);
        let v = v.clamp(-1.0, 1.0);
        let s = speed_norm.clamp(0.0, 1.0);
        // 잡음: 0 중심 ±1 흔들림 (fast_noise면 고주파 위킹 생략).
        let noise = if self.fast_noise {
            (ink_field_fast(u, v, self.seed) - 0.5) * 2.0
        } else {
            (ink_field(u, v, self.seed) - 0.5) * 2.0
        };
        // 도구별 형태와 진폭/모서리 강도.
        let (shape, edge_k, noise_amp) = match tool {
            ToolType::Fountain => (
                fountain_shape(u, s, self),
                0.10,
                self.flow_amp + self.wick_amp,
            ),
            _ => (
                ballpoint_shape(u, s, self),
                0.06,
                // 볼펜은 잉크 점도가 높아 만년필보다 노이즈가 덜 보이지만,
                // 기본값 수준에서도 체감되도록 0.6 → 0.85로 키웁니다.
                (self.flow_amp + self.wick_amp) * 0.85,
            ),
        };
        // 단면: 가장자리가 중심보다 진함 (레일로드 효과).
        let edge = 1.0 + edge_k * (2.0 * v.abs() - 1.0);
        (shape * edge * (1.0 + noise_amp * noise)).clamp(0.30, 1.60)
    }

    /// 단면(왼/오) 밀도 배율 쌍 — 같은 `u`·속도에서 v만 다르므로 각 옥타브의
    /// x 공통 부분을 1번만 계산합니다. 결과는 `[density(...-1), density(...+1)]`과
    /// **완전히 동일**합니다 (`|v|=1`이라 edge 항도 좌우 동일).
    pub fn density_lr(self, tool: ToolType, u: f32, speed_norm: f32) -> [f32; 2] {
        let u = u.clamp(0.0, 1.0);
        let s = speed_norm.clamp(0.0, 1.0);
        let [nl, nr] = if self.fast_noise {
            ink_field_pair_fast(u, self.seed)
        } else {
            ink_field_pair(u, self.seed)
        };
        let nl = (nl - 0.5) * 2.0;
        let nr = (nr - 0.5) * 2.0;
        let (shape, edge_k, noise_amp) = match tool {
            ToolType::Fountain => (
                fountain_shape(u, s, self),
                0.10,
                self.flow_amp + self.wick_amp,
            ),
            _ => (
                ballpoint_shape(u, s, self),
                0.06,
                (self.flow_amp + self.wick_amp) * 0.85,
            ),
        };
        // |v| = 1 (왼/오 가장자리) → edge 동일.
        let edge = 1.0 + edge_k;
        [
            (shape * edge * (1.0 + noise_amp * nl)).clamp(0.30, 1.60),
            (shape * edge * (1.0 + noise_amp * nr)).clamp(0.30, 1.60),
        ]
    }
}

/// 스트로크 전체의 점별 잉크 밀도 배율을 계산합니다 (중심선 v=0).
///
/// 호 길이 누적으로 `u`를, 직전 세그먼트 속도(pt/s)로 `speed_norm`을
/// 만들고 중심선 밀도를 반환합니다 — 캔버스/내보내기 양쪽에서
/// **같은 입력이면 항상 같은 결과**입니다 (결정적, 시간 불변).
pub fn stroke_ink_factors(
    tool: ToolType,
    points: &[StrokePoint],
    grain: InkGrain,
) -> Vec<f32> {
    if points.is_empty() {
        return Vec::new();
    }
    if !grain.enabled {
        return vec![1.0; points.len()];
    }
    let (us, speeds) = stroke_space(points);
    let mut out = Vec::with_capacity(points.len());
    for i in 0..points.len() {
        out.push(grain.density(tool, us[i], 0.0, speeds[i]));
    }
    out
}

/// 스트로크 전체의 점별 **[왼쪽, 오른쪽] 단면 밀도** — 레일로드(가장자리가
/// 중심보다 진함) 효과를 리본 좌우 정점에 따로 적용하기 위한 API.
/// 비활성 상태면 전부 [1.0, 1.0]을 반환합니다.
pub fn stroke_ink_lr(
    tool: ToolType,
    points: &[StrokePoint],
    grain: InkGrain,
) -> Vec<[f32; 2]> {
    let n = points.len();
    if n == 0 {
        return Vec::new();
    }
    if !grain.enabled {
        return vec![[1.0, 1.0]; n];
    }
    let (us, speeds) = stroke_space(points);
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        out.push(grain.density_lr(tool, us[i], speeds[i]));
    }
    out
}

/// 점별 진행 위치(u 0..1)와 속도 정규화(0..1) — 공용 전처리.
fn stroke_space(points: &[StrokePoint]) -> (Vec<f32>, Vec<f32>) {
    let n = points.len();
    let dist = |a: &StrokePoint, b: &StrokePoint| -> f32 {
        ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt()
    };
    // 호 길이 누적 (u).
    let mut lens: Vec<f32> = Vec::with_capacity(n);
    let mut total = 0.0f32;
    for i in 0..n {
        if i > 0 {
            total += dist(&points[i - 1], &points[i]);
        }
        lens.push(total);
    }
    // 점별 속도 정규화 (직전 세그먼트).
    let mut speeds: Vec<f32> = Vec::with_capacity(n);
    for i in 0..n {
        let v = if i == 0 {
            0.0
        } else {
            let a = &points[i - 1];
            let b = &points[i];
            let dt = (b.t_ms.saturating_sub(a.t_ms)) as f32 / 1000.0;
            if dt > 1e-4 {
                dist(a, b) / dt
            } else {
                0.0
            }
        };
        speeds.push((v / INK_SPEED_REF).clamp(0.0, 1.0));
    }
    let us = lens
        .iter()
        .map(|l| if total > 1e-6 { l / total } else { 0.0 })
        .collect();
    (us, speeds)
}

/// 캔버스 통합용: 포화 램프(`sat` 0..1 — 잉크가 스며들며 진해지는 정도)와
/// 잉크 밀도(`density`)를 합성한 최종 정점 알파.
///
/// 밀도가 1보다 큰 곳(끝 고임/가장자리)은 더 빨리 원색에 도달하고,
/// 옅은 곳은 천천히 진해집니다. 결과는 항상 0..1.
pub fn combine_saturation(sat: f32, density: f32) -> f32 {
    (sat.clamp(0.0, 1.0) * density.clamp(0.0, f32::MAX)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn density_lr_matches_two_density_calls() {
        // 성능 최적화 가드: density_lr(공유 x-파트)의 결과는 density를 좌/우로
        // 각각 호출한 것과 완전히 동일해야 합니다 (행동 보존). 정확 2옥타브와
        // fast_noise(고주파 생략) 두 모드 모두 검증합니다.
        let g_base = InkGrain { seed: 11, ..InkGrain::default() };
        for &fast in &[false, true] {
            for &seed in &[11u64, 99, 55555] {
                let g = InkGrain { seed, fast_noise: fast, ..g_base };
                for &(u, s) in &[(0.0f32, 0.0f32), (0.12, 0.5), (0.5, 1.0), (1.0, 0.9)] {
                    for tool in [ToolType::Pen, ToolType::Fountain] {
                        let lr = g.density_lr(tool, u, s);
                        let l = g.density(tool, u, -1.0, s);
                        let r = g.density(tool, u, 1.0, s);
                        assert!((lr[0] - l).abs() < 1e-6, "L mismatch fast={fast} tool={tool:?} u={u} s={s}: {lr:?} vs {l}");
                        assert!((lr[1] - r).abs() < 1e-6, "R mismatch fast={fast} tool={tool:?} u={u} s={s}: {lr:?} vs {r}");
                    }
                }
            }
        }
    }

    #[test]
    fn fast_noise_is_deterministic_bounded_and_non_popping() {
        let g = InkGrain { fast_noise: true, ..InkGrain::default() };
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
        assert!(edges > center + 0.02, "레일로드 효과 없음: {edges} vs {center}");
    }

    #[test]
    fn stroke_ink_factors_follow_path_and_are_stable() {
        // 200점 직선 스트로크 — 계수 벡터가 점 수와 같고, 전부 유한/범위
        // 안이며, 시작이 중간보다 진하고(뭉침), 반복 호출에도 동일합니다.
        let pts: Vec<StrokePoint> = (0..200)
            .map(|i| StrokePoint::with_time(i as f32 * 4.0, 100.0, 0.5, i as u64 * 5))
            .collect();
        let g = InkGrain { seed: 1234, ..InkGrain::default() };
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
        let off = stroke_ink_factors(ToolType::Pen, &pts, InkGrain { enabled: false, ..g });
        assert!(off.iter().all(|v| (*v - 1.0).abs() < 1e-6));
    }

    #[test]
    fn stroke_ink_lr_edges_denser_than_center_and_bounded() {
        // 좌우 단면 밀도 — 레일로드(가장자리 진함)가 구간 평균으로 확인되고,
        // 값 전부가 범위 안이며, 결정적이고, 비활성화 시 [1,1]입니다.
        let pts: Vec<StrokePoint> = (0..300)
            .map(|i| StrokePoint::with_time(i as f32 * 4.0, 100.0, 0.5, i as u64 * 5))
            .collect();
        let g = InkGrain { seed: 77, ..InkGrain::default() };
        let lr = stroke_ink_lr(ToolType::Pen, &pts, g);
        assert_eq!(lr.len(), 300);
        let center = stroke_ink_factors(ToolType::Pen, &pts, g);
        let mid_avg: f32 = center[60..240].iter().sum::<f32>() / 180.0;
        let edge_avg: f32 =
            lr[60..240].iter().map(|p| (p[0] + p[1]) * 0.5).sum::<f32>() / 180.0;
        assert!(edge_avg > mid_avg + 0.02, "가장자리가 중심보다 진해야 함: {edge_avg} vs {mid_avg}");
        assert!(lr
            .iter()
            .flatten()
            .all(|v| v.is_finite() && (0.30..=1.60).contains(v)));
        // 비활성 → 전부 [1,1].
        let off = stroke_ink_lr(ToolType::Pen, &pts, InkGrain { enabled: false, ..g });
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
}
