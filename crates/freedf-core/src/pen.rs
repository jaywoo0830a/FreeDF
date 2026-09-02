//! 펜 설정: 색상 팔레트(빨강/파랑/검정 계열), 필압 → 두께 곡선, 스무딩/테이퍼.
//!
//! FreeDF v3: 필기구는 펜 하나로 단순화했습니다 (볼펜/만년필 제거).

use serde::{Deserialize, Serialize};

use crate::model::{StrokePoint, ToolType};

/// 시작/끝을 뾰족하게(테이퍼) 만드는 도구인지 — 실제 펜처럼 획 양끝이 얇아집니다.
/// 펜만 해당하고, 하이라이터/지우개는 해당 없습니다.
pub fn uses_taper(tool: ToolType) -> bool {
    matches!(tool, ToolType::Pen)
}

/// 획 양끝 테이퍼 길이 (페이지 포인트). 이 거리 안에서 두께가 0에서 서서히 커집니다.
pub const TAPER_LEN_PTS: f32 = 14.0;

/// 스트로크의 점별 **테이퍼 배율**(0..=1)을 계산합니다.
///
/// 실제 펜은 종이에 닿는 순간/떼는 순간 잉크가 얇게 시작/끝납니다. 점 i의 배율은
/// 시작점·끝점까지의 거리를 `taper_len`으로 나눈 값에 smoothstep을 씌운 것으로,
/// 양끝에서 0으로 시작해 중앙에서 1에 도달합니다. 점이 1개(점 찍기)면 1.0입니다.
///
/// 렌더/내보내기 양쪽에서 같은 함수를 써야 화면과 PNG가 일치합니다.
pub fn taper_factors(points: &[StrokePoint], taper_len: f32) -> Vec<f32> {
    let n = points.len();
    if n <= 1 {
        return vec![1.0; n];
    }
    // 획 총 길이 (세그먼트 거리 합).
    let total: f32 = points
        .windows(2)
        .map(|w| ((w[1].x - w[0].x).powi(2) + (w[1].y - w[0].y).powi(2)).sqrt())
        .sum();
    // 테이퍼 길이 = 요청값과 총 길이의 40% 중 작은 쪽 — 짧은 획도
    // 중앙은 완전한 두께에 도달하고 양끝만 뾰족해집니다.
    let len = if taper_len.is_finite() && taper_len > 0.0 {
        taper_len.min(total * 0.4).max(0.5)
    } else {
        TAPER_LEN_PTS.min(total * 0.4).max(0.5)
    };
    let first = points[0];
    let last = points[n - 1];
    points
        .iter()
        .map(|p| {
            let d0 = ((p.x - first.x).powi(2) + (p.y - first.y).powi(2)).sqrt();
            let d1 = ((p.x - last.x).powi(2) + (p.y - last.y).powi(2)).sqrt();
            let t = (d0.min(d1) / len).clamp(0.0, 1.0);
            // smoothstep: 자연스러운 붓 시작/끝 느낌
            t * t * (3.0 - 2.0 * t)
        })
        .collect()
}

// ── 스트로크 외곽선 + 삼각분할 (관례적 렌더링 지오메트리) ─────────────────────

/// 마이터 조인의 최소 방향 합 길이. 이보다 작으면(거의 180° 되접힘) 마이터
/// 법선이 무한대로 커져 스파이크가 튀므로 **베벨(한쪽 세그먼트 방향)** 으로
/// 폴백합니다 — SVG `stroke-linejoin: miter`의 miter-limit에 해당하는 관례.
const MITER_LIMIT_DIR_LEN: f32 = 0.35;

/// 가변폭 폴리라인의 **외곽선 다각형**을 만듭니다.
///
/// - 각 점의 오프셋 법선 = 인접 두 세그먼트 방향의 평균(마이터), 되접힘 시 베벨 폴백.
/// - `round_caps=true`면 양끝에 반원 캡(펜 관례), false면 직선(butt) 끝(마커 관례).
/// - 좌표는 입력과 같은 공간이며, 화면(egui)과 내보내기(래스터)가
///   **같은 함수**를 써서 동일하게 그려집니다.
///
/// 반환 다각형은 단순 다각형(구멍 없음)이므로, `triangulate_polygon`으로
/// **겹침 없는 삼각형들**로 나눌 수 있습니다 — 반투명 잉크도 얼룩 없이
/// 균일하게 칠해집니다.
pub fn stroke_outline(
    points: &[[f32; 2]],
    half_widths: &[f32],
    round_caps: bool,
) -> Vec<[f32; 2]> {
    let n = points.len().min(half_widths.len());
    if n == 0 {
        return Vec::new();
    }
    if n == 1 {
        return circle_polygon(points[0], half_widths[0].max(0.0), 12);
    }
    // 점별 오프셋 법선 (마이터 + 베벨 폴백).
    let mut norms: Vec<[f32; 2]> = Vec::with_capacity(n);
    for i in 0..n {
        let a = if i > 0 {
            sub(points[i], points[i - 1])
        } else {
            sub(points[1], points[0])
        };
        let b = if i + 1 < n {
            sub(points[i + 1], points[i])
        } else {
            sub(points[n - 1], points[n - 2])
        };
        let ua = unit2(a);
        let ub = unit2(b);
        let d = [ua[0] + ub[0], ua[1] + ub[1]];
        let miter_dir = if d[0] * d[0] + d[1] * d[1] < MITER_LIMIT_DIR_LEN * MITER_LIMIT_DIR_LEN {
            ua // 베벨 폴백
        } else {
            unit2(d)
        };
        // 오프셋 방향 = 마이터 방향에 수직.
        norms.push([-miter_dir[1], miter_dir[0]]);
    }
    let mut poly: Vec<[f32; 2]> = Vec::with_capacity(n * 2 + 26);
    let t_start = unit2(sub(points[1], points[0]));
    let t_end = unit2(sub(points[n - 1], points[n - 2]));
    // 바깥 체인 (p0+n·h → p_last+n·h).
    for i in 0..n {
        poly.push(add_mul(points[i], norms[i], half_widths[i]));
    }
    // 끝 캡 (반원): +n_last → −n_last, 바깥쪽(+t_end)으로.
    if round_caps {
        push_cap(&mut poly, points[n - 1], norms[n - 1], half_widths[n - 1], t_end);
    }
    // 안쪽 체인 (역순).
    for i in (0..n).rev() {
        poly.push(sub_mul(points[i], norms[i], half_widths[i]));
    }
    // 시작 캡 (반원): −n0 → +n0, 바깥쪽(−t_start)으로 (첫 정점으로 닫힘).
    if round_caps {
        push_cap(&mut poly, points[0], neg(norms[0]), half_widths[0], neg(t_start));
    }
    poly
}

/// 단순 다각형을 **겹침 없는 삼각형들**로 분할합니다 (귀 자르기/ear clipping).
///
/// - 입력은 `stroke_outline`처럼 자기 자신을 교차하지 않는 단순 다각형이어야 합니다.
/// - 반환 삼각형들은 다각형을 정확히 한 번씩 덮습니다 → 반투명 색으로 각각
///   칠해도 겹침으로 인한 진해짐(얼룩)이 없습니다.
/// - 퇴화(면적 0/볼록 귀 없음)는 빈 결과 또는 부분 결과로 안전하게 처리됩니다.
pub fn triangulate_polygon(poly: &[[f32; 2]]) -> Vec<[u32; 3]> {
    let n = poly.len();
    if n < 3 {
        return Vec::new();
    }
    let area2 = signed_area2(poly);
    if area2.abs() < 1e-6 {
        return Vec::new();
    }
    let ccw = area2 > 0.0;
    let mut idx: Vec<u32> = (0..n as u32).collect();
    let mut out: Vec<[u32; 3]> = Vec::with_capacity(n.saturating_sub(2));
    let mut guard = 0usize;
    let mut i = 0usize;
    while idx.len() > 3 && guard <= n * n + 16 {
        guard += 1;
        let m = idx.len();
        let a = idx[i % m];
        let b = idx[(i + 1) % m];
        let c = idx[(i + 2) % m];
        let (pa, pb, pc) = (poly[a as usize], poly[b as usize], poly[c as usize]);
        let cross = (pb[0] - pa[0]) * (pc[1] - pa[1]) - (pb[1] - pa[1]) * (pc[0] - pa[0]);
        let convex = if ccw { cross > 1e-6 } else { cross < -1e-6 };
        let mut is_ear = convex;
        if is_ear {
            for &v in &idx {
                if v == a || v == b || v == c {
                    continue;
                }
                if point_in_triangle(poly[v as usize], pa, pb, pc, ccw) {
                    is_ear = false;
                    break;
                }
            }
        }
        if is_ear {
            out.push([a, b, c]);
            idx.remove((i + 1) % m);
        } else {
            i += 1;
        }
    }
    if idx.len() == 3 {
        out.push([idx[0], idx[1], idx[2]]);
    }
    out
}

// ── 잉크 번짐(블리드) ────────────────────────────────────────────────────────

/// 잉크 번짐(블리드) 모델 — 그어진 지 얼마나 지났는지(나이)와 획 위 위치에
/// 따라 잉크가 종이로 퍼지는 **번짐 반경(pt)**을 계산합니다.
///
/// 실제 잉크처럼: 갓 그은 잉크는 빨리 번지고, 시간이 지날수록 느려지며,
/// 어느 시점에 수렴합니다. 번짐 속도는 획의 **시작/중간/끝 구간**별로
/// 다르게 지정할 수 있습니다 (`*_rate`, pt/초 — 0이면 그 구간은 안 번짐).
///
/// 순수 계산이라 GUI 없이 단위 테스트로 검증합니다.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct InkBleed {
    pub enabled: bool,
    /// 번짐 반경 상한 (pt) — 아무리 오래돼도 이보다 퍼지지 않습니다.
    pub max_spread_pt: f32,
    /// 시작 구간(획 앞 30%) 번짐 속도 (pt/초).
    pub start_rate: f32,
    /// 중간 구간(30~70%) 번짐 속도 (pt/초).
    pub mid_rate: f32,
    /// 끝 구간(뒤 30%) 번짐 속도 (pt/초).
    pub end_rate: f32,
}

impl Default for InkBleed {
    fn default() -> Self {
        Self {
            enabled: false,
            max_spread_pt: 5.0,
            start_rate: 0.6,
            mid_rate: 0.25,
            end_rate: 0.45,
        }
    }
}

impl InkBleed {
    /// 획 위 한 점의 번짐 반경(pt): `phase_rate × 나이`, 상한 클램프.
    ///
    /// `d0`/`d1` = 시작점·끝점까지의 획 길이(pt), `len` = 획 총 길이,
    /// `age_sec` = 그어진 뒤 경과 시간(초).
    pub fn radius(&self, d0: f32, d1: f32, len: f32, age_sec: f32) -> f32 {
        if !self.enabled {
            return 0.0;
        }
        let rate = self.phase_rate(d0, d1, len);
        let spread = rate * age_sec.max(0.0);
        spread.min(self.max_spread_pt.max(0.0))
    }

    /// 획 위 위치(시작 0 → 끝 1)에서의 번짐 속도(pt/초).
    ///
    /// 시작 구간(앞 30%)은 `start_rate`, 중간(30~70%)은 `mid_rate`,
    /// 끝 구간(뒤 30%)은 `end_rate`이며, 구간 경계는 smoothstep으로
    /// 자연스럽게 보간됩니다.
    pub fn phase_rate(&self, d0: f32, d1: f32, len: f32) -> f32 {
        if len <= 1e-6 {
            return self.start_rate.max(0.0);
        }
        let t = (d0 / len).clamp(0.0, 1.0);
        let s = (d0 / (0.3 * len)).min(1.0); // 시작 구간 진행도
        let e = (d1 / (0.3 * len)).min(1.0); // 끝 구간 진행도
        let ss = |x: f32| x * x * (3.0 - 2.0 * x);
        let rate = if t < 0.3 {
            self.start_rate + (self.mid_rate - self.start_rate) * ss(s)
        } else if t > 0.7 {
            self.end_rate + (self.mid_rate - self.end_rate) * ss(e)
        } else {
            self.mid_rate
        };
        rate.max(0.0)
    }

    /// 번짐이 최대(`max_spread_pt`)에 도달하는 시간(초) — 가장 느린 구간 기준.
    pub fn settle_sec(&self) -> f32 {
        let slowest = self
            .start_rate
            .max(self.mid_rate)
            .max(self.end_rate)
            .max(1e-3);
        self.max_spread_pt.max(0.0) / slowest
    }
}

// ── 2차원 헬퍼 ───────────────────────────────────────────────────────────────

fn sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn add_mul(p: [f32; 2], n: [f32; 2], k: f32) -> [f32; 2] {
    [p[0] + n[0] * k, p[1] + n[1] * k]
}

fn sub_mul(p: [f32; 2], n: [f32; 2], k: f32) -> [f32; 2] {
    [p[0] - n[0] * k, p[1] - n[1] * k]
}

fn neg(v: [f32; 2]) -> [f32; 2] {
    [-v[0], -v[1]]
}

fn unit2(v: [f32; 2]) -> [f32; 2] {
    let l = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if l < 1e-6 {
        [1.0, 0.0]
    } else {
        [v[0] / l, v[1] / l]
    }
}

/// 중심/반지름의 정다각형 (점 찍기·캡 근사용).
fn circle_polygon(center: [f32; 2], r: f32, steps: usize) -> Vec<[f32; 2]> {
    let mut out = Vec::with_capacity(steps);
    for k in 0..steps {
        let a = std::f32::consts::TAU * (k as f32 / steps as f32);
        out.push([center[0] + r * a.cos(), center[1] + r * a.sin()]);
    }
    out
}

/// 반원 캡: 법선 `n` 쪽(+n)에서 시작해 바깥쪽(`away`)을 지나 −n으로 이어지는 호.
fn push_cap(
    out: &mut Vec<[f32; 2]>,
    center: [f32; 2],
    n: [f32; 2],
    half: f32,
    away: [f32; 2],
) {
    let steps = 10usize;
    let base = n[1].atan2(n[0]);
    let mid = base + std::f32::consts::FRAC_PI_2;
    let mid_dir = [mid.cos(), mid.sin()];
    let dir = if mid_dir[0] * away[0] + mid_dir[1] * away[1] > 0.0 {
        1.0
    } else {
        -1.0
    };
    for k in 0..=steps {
        let a = base + dir * std::f32::consts::PI * (k as f32 / steps as f32);
        out.push([center[0] + half * a.cos(), center[1] + half * a.sin()]);
    }
}

/// 다각형 부호 면적의 2배 (CCW 양수).
fn signed_area2(poly: &[[f32; 2]]) -> f32 {
    let n = poly.len();
    let mut s = 0.0f32;
    for i in 0..n {
        let a = poly[i];
        let b = poly[(i + 1) % n];
        s += a[0] * b[1] - b[0] * a[1];
    }
    s
}

/// 점 `p`가 삼각형 (a,b,c) **내부**에 있는지 (경계는 외부로 취급 — 겹침 방지).
fn point_in_triangle(p: [f32; 2], a: [f32; 2], b: [f32; 2], c: [f32; 2], ccw: bool) -> bool {
    let cross = |u: [f32; 2], v: [f32; 2], w: [f32; 2]| {
        (v[0] - u[0]) * (w[1] - u[1]) - (v[1] - u[1]) * (w[0] - u[0])
    };
    let (c1, c2, c3) = (cross(a, b, p), cross(b, c, p), cross(c, a, p));
    if ccw {
        c1 > 1e-6 && c2 > 1e-6 && c3 > 1e-6
    } else {
        c1 < -1e-6 && c2 < -1e-6 && c3 < -1e-6
    }
}

/// OneEuroFilter (1€ 저역 통과 필터) — 손떨림은 제거하고 빠른 움직임은 그대로 따라갑니다
/// (Casiez, Roussel, Vogel 2012). 필압/좌표 스무딩에 사용합니다.
///
/// 속도에 적응적: 입력이 느리면(필기) 강하게 스무딩하고, 빠르면(획 긋기)
/// 지연을 줄입니다. 순수 수학만 담고 있어 단위 테스트로 검증합니다.
#[derive(Debug, Clone, Copy)]
pub struct OneEuroFilter {
    min_cutoff: f32,
    beta: f32,
    d_cutoff: f32,
    prev_x: f32,
    prev_dx: f32,
    prev_t: Option<f64>,
}

impl OneEuroFilter {
    /// `smoothing`(0..1)에서 필터 파라미터를 만듭니다.
    /// 0 = 거의 원본, 1 = 최대 스무딩(안정화). 값이 클수록 저역 컷오프를
    /// 낮춰(더 부드럽게) 잡고, 빠른 움직임에는 `beta`가 컷오프를 다시
    /// 올려 필기 지연을 억제합니다.
    pub fn from_smoothing(smoothing: f32) -> Self {
        let s = smoothing.clamp(0.0, 1.0);
        Self {
            min_cutoff: 4.5 - 3.5 * s,
            beta: 0.06 * s,
            d_cutoff: 1.0,
            prev_x: 0.0,
            prev_dx: 0.0,
            prev_t: None,
        }
    }

    /// 이전 상태를 버리고 새 입력부터 다시 시작합니다 (새 스트로크마다 호출).
    pub fn reset(&mut self) {
        self.prev_x = 0.0;
        self.prev_dx = 0.0;
        self.prev_t = None;
    }

    /// 현재 입력 `x`(시간 `t`, 초)를 필터링해 반환합니다.
    pub fn filter(&mut self, x: f32, t: f64) -> f32 {
        let Some(prev_t) = self.prev_t else {
            self.prev_x = x;
            self.prev_t = Some(t);
            return x;
        };
        let dt = (t - prev_t) as f32;
        if dt <= 1e-6 {
            return self.prev_x;
        }
        let alpha = |cutoff: f32| 1.0 / (1.0 + 1.0 / (std::f32::consts::TAU * cutoff * dt));
        // 1차 미분(속도)도 저역 통과로 부드럽게 해 cutoff를 추정합니다.
        let dx = (x - self.prev_x) / dt;
        let dx_hat = self.prev_dx + alpha(self.d_cutoff) * (dx - self.prev_dx);
        let cutoff = self.min_cutoff + self.beta * dx_hat.abs();
        let x_hat = self.prev_x + alpha(cutoff) * (x - self.prev_x);
        self.prev_x = x_hat;
        self.prev_dx = dx_hat;
        self.prev_t = Some(t);
        x_hat
    }
}

/// 색상 계열.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColorFamily {
    Red,
    Blue,
    Black,
    Green,
    Orange,
    Purple,
    Custom,
}

impl ColorFamily {
    pub fn label(self) -> &'static str {
        match self {
            ColorFamily::Red => "Red",
            ColorFamily::Blue => "Blue",
            ColorFamily::Black => "Black",
            ColorFamily::Green => "Green",
            ColorFamily::Orange => "Orange",
            ColorFamily::Purple => "Purple",
            ColorFamily::Custom => "Custom",
        }
    }

    pub fn all() -> [ColorFamily; 7] {
        [
            ColorFamily::Red,
            ColorFamily::Blue,
            ColorFamily::Black,
            ColorFamily::Green,
            ColorFamily::Orange,
            ColorFamily::Purple,
            ColorFamily::Custom,
        ]
    }
}

/// 색상 팔레트. 요구사항대로 빨강/파랑/검정 계열을 반드시 포함합니다.
pub struct Palette;

impl Palette {
    /// 계열별 스와치(불투명 RGBA).
    pub fn swatches(family: ColorFamily) -> Vec<[u8; 4]> {
        let base: &[[u8; 3]] = match family {
            ColorFamily::Red => &[
                [198, 40, 40],  // deep red
                [229, 57, 53],  // red
                [239, 83, 80],  // light red
                [255, 138, 128], // lighter red
            ],
            ColorFamily::Blue => &[
                [21, 101, 192], // deep blue
                [30, 136, 229], // blue
                [66, 165, 245], // light blue
                [144, 202, 249], // lighter blue
            ],
            ColorFamily::Black => &[
                [0, 0, 0],       // black
                [33, 33, 33],    // dark gray
                [66, 66, 66],    // gray
                [117, 117, 117], // light gray
            ],
            ColorFamily::Green => &[
                [27, 94, 32], [46, 125, 50], [102, 187, 106], [165, 214, 167],
            ],
            ColorFamily::Orange => &[
                [230, 81, 0], [245, 124, 0], [255, 167, 38], [255, 202, 40],
            ],
            ColorFamily::Purple => &[
                [74, 20, 140], [106, 27, 154], [156, 39, 176], [206, 147, 216],
            ],
            ColorFamily::Custom => &[[20, 20, 20]],
        };
        base.iter().map(|c| [c[0], c[1], c[2], 255]).collect()
    }

    /// 기본 펜 색(진한 검정).
    pub fn default_pen() -> [u8; 4] {
        [20, 20, 20, 255]
    }

    /// 기본 형광펜 색(반투명 노랑).
    pub fn default_highlighter() -> [u8; 4] {
        [255, 235, 59, 90]
    }
}

/// 필압 → 두께 곡선.
/// `width = base * (min_ratio + (max_ratio - min_ratio) * pressure)`
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PressureCurve {
    /// 가장 가벼운 필압(0)일 때의 두께 비율
    pub min_ratio: f32,
    /// 가장 센 필압(1)일 때의 두께 비율
    pub max_ratio: f32,
}

impl Default for PressureCurve {
    fn default() -> Self {
        Self {
            min_ratio: 0.4,
            max_ratio: 1.4,
        }
    }
}

impl PressureCurve {
    pub fn new(min_ratio: f32, max_ratio: f32) -> Self {
        Self {
            min_ratio: min_ratio.clamp(0.05, 1.0),
            max_ratio: max_ratio.clamp(1.0, 4.0).max(min_ratio),
        }
    }

    /// 필압(0..1)을 두께(포인트)로 변환합니다. 범위 밖/NaN은 안전하게 처리합니다.
    pub fn apply(&self, base_width: f32, pressure: f32) -> f32 {
        let p = if pressure.is_nan() {
            1.0
        } else {
            pressure.clamp(0.0, 1.0)
        };
        let ratio = self.min_ratio + (self.max_ratio - self.min_ratio) * p;
        (base_width * ratio).max(0.1)
    }

    /// 두께에서 필압 역산(UI 표시용).
    pub fn pressure_of(&self, base_width: f32, width: f32) -> f32 {
        if base_width <= 0.0 {
            return 0.0;
        }
        let ratio = (width / base_width).clamp(self.min_ratio, self.max_ratio);
        if (self.max_ratio - self.min_ratio).abs() < 1e-6 {
            return 0.0;
        }
        ((ratio - self.min_ratio) / (self.max_ratio - self.min_ratio)).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_families_have_swatches() {
        for family in [
            ColorFamily::Red,
            ColorFamily::Blue,
            ColorFamily::Black,
        ] {
            let s = Palette::swatches(family);
            assert!(s.len() >= 3, "{} 계열은 3개 이상 스와치 필요", family.label());
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
            assert!(c[0] < 130 && c[1] < 130 && c[2] < 130, "검정 계열은 어두워야 함");
        }
    }

    #[test]
    fn pressure_curve_is_monotonic() {
        let curve = PressureCurve::default();
        let w0 = curve.apply(2.0, 0.0);
        let w1 = curve.apply(2.0, 0.5);
        let w2 = curve.apply(2.0, 1.0);
        assert!(w0 < w1 && w1 < w2);
        assert!((w0 - 2.0 * curve.min_ratio).abs() < 1e-4);
        assert!((w2 - 2.0 * curve.max_ratio).abs() < 1e-4);
    }

    #[test]
    fn pressure_clamps_out_of_range_and_nan() {
        let curve = PressureCurve::default();
        assert_eq!(curve.apply(2.0, -5.0), curve.apply(2.0, 0.0));
        assert_eq!(curve.apply(2.0, 99.0), curve.apply(2.0, 1.0));
        assert_eq!(curve.apply(2.0, f32::NAN), curve.apply(2.0, 1.0));
    }

    #[test]
    fn pressure_curve_never_zero() {
        let curve = PressureCurve { min_ratio: 0.05, max_ratio: 1.0 };
        let w = curve.apply(1.0, 0.0);
        assert!(w >= 0.1);
    }

    #[test]
    fn pressure_of_inverts_apply() {
        let curve = PressureCurve::default();
        for p in [0.0f32, 0.25, 0.5, 0.75, 1.0] {
            let w = curve.apply(2.0, p);
            let back = curve.pressure_of(2.0, w);
            assert!((back - p).abs() < 1e-3, "역산: {p} vs {back}");
        }
    }

    #[test]
    fn curve_new_sanitizes_ratios() {
        let c = PressureCurve::new(-1.0, 100.0);
        assert!(c.min_ratio >= 0.05 && c.max_ratio <= 4.0 && c.max_ratio >= c.min_ratio);
    }

    // ---------- taper_factors ----------

    fn line_points(n: usize) -> Vec<StrokePoint> {
        (0..n)
            .map(|i| StrokePoint::new(i as f32 * 5.0, 0.0, 0.5))
            .collect()
    }

    #[test]
    fn taper_single_point_is_full_width() {
        let pts = vec![StrokePoint::new(10.0, 10.0, 0.5)];
        assert_eq!(taper_factors(&pts, TAPER_LEN_PTS), vec![1.0]);
    }

    #[test]
    fn taper_starts_and_ends_thin_middle_full() {
        let pts = line_points(40); // 40점, 5pt 간격 → 총 195pt 길이
        let f = taper_factors(&pts, TAPER_LEN_PTS);
        assert!(f[0] < 0.05, "시작점은 거의 0: {}", f[0]);
        assert!(f[39] < 0.05, "끝점도 거의 0: {}", f[39]);
        assert!((f[20] - 1.0).abs() < 1e-4, "중앙은 완전 두께: {}", f[20]);
        // 시작→중앙으로 단조 증가 (부드럽게 붓이 열림)
        for w in f[..20].windows(2) {
            assert!(w[0] <= w[1]);
        }
    }

    #[test]
    fn taper_scales_with_length() {
        // 같은 테이퍼 길이에서 짧은 획일수록 더 얇아진다 (퀵 플릭 느낌).
        let short = line_points(2); // 총 5pt
        let f = taper_factors(&short, TAPER_LEN_PTS);
        assert!(f.iter().all(|&v| v < 0.5), "{f:?}");
        let long = line_points(100);
        let g = taper_factors(&long, TAPER_LEN_PTS);
        assert!(g[50] > 0.99);
    }

    #[test]
    fn short_slow_strokes_keep_full_middle() {
        // 천천히 쓰는 짧은 획(6점, 총 5pt)도 중앙은 완전한 두께에 도달하고
        // 양끝만 얇아야 실제 펜 느낌이 납니다.
        let pts = (0..6)
            .map(|i| StrokePoint::new(i as f32 * 1.0, 0.0, 0.5))
            .collect::<Vec<_>>(); // 총 5pt
        let f = taper_factors(&pts, TAPER_LEN_PTS);
        assert!(f[0] < 0.1 && f[5] < 0.1, "양끝 얇게: {f:?}");
        assert!(f[2] > 0.9 || f[3] > 0.9, "중앙은 두껍게: {f:?}");
    }

    #[test]
    fn only_pen_tapers() {
        use crate::model::ToolType;
        assert!(uses_taper(ToolType::Pen));
        assert!(!uses_taper(ToolType::Highlighter));
        assert!(!uses_taper(ToolType::Eraser));
        assert!(!uses_taper(ToolType::Pan));
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
        assert!((area - poly_area).abs() < 1e-3, "겹침 없이 정확히 한 번 덮음");
        assert!(poly_area > 60.0, "L자 면적 확인");
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
        };
        let len = 100.0;
        assert!((b.phase_rate(0.0, len, len) - 3.0).abs() < 1e-4, "시작 = start_rate");
        assert!((b.phase_rate(50.0, 50.0, len) - 1.0).abs() < 1e-4, "중간 = mid_rate");
        assert!((b.phase_rate(len, 0.0, len) - 2.0).abs() < 1e-4, "끝 = end_rate");
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
        };
        assert_eq!(b.radius(0.0, 10.0, 10.0, 10.0), 0.0, "시작 구간 속도 0");
        assert!(b.radius(5.0, 5.0, 10.0, 10.0) > 0.0, "중간은 번짐");
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
        let (a, b, c) = (poly[t[0] as usize], poly[t[1] as usize], poly[t[2] as usize]);
        ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs() * 0.5
    }

    fn polygon_area(poly: &[[f32; 2]]) -> f32 {
        signed_area2(poly).abs() * 0.5
    }
}
