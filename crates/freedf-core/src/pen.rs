//! 펜 도구 모델과 색상 팔레트.
//!
//! - [`BallPenProfile`]: 일반 펜(볼펜/젤펜) — 필압·속도 영향이 작고 선폭 변동폭이
//!   좁은 물리 모델 (기울기 끊김·과속 잉크 부족 포함).
//! - [`FountainProfile`]: 만년필 — 필압 × 속도 × 기울기 모델.
//! - [`InkBleed`]: 잉크 번짐(블리드) — 구간별 속도 커스텀.
//! - [`OneEuroFilter`]: 손떨림 안정화(선택 기능).
//! - [`Palette`] / [`ColorFamily`]: 색상.

use serde::{Deserialize, Serialize};

use crate::model::{StrokePoint, ToolType};

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
    triangulate_polygon_checked(poly).0
}

/// [`triangulate_polygon`]의 완전성 검사 버전.
///
/// `complete == false`면 입력이 자기 교차하거나 귀가 더 없어 **조기 종료**된
/// 것이므로(다각형 일부가 안 채워짐), 호출자는 [`stroke_fallback_geometry`]
/// 같은 폴백으로 렌더링해야 합니다.
pub fn triangulate_polygon_checked(poly: &[[f32; 2]]) -> (Vec<[u32; 3]>, bool) {
    let n = poly.len();
    if n < 3 {
        return (Vec::new(), true);
    }
    let area2 = signed_area2(poly);
    if area2.abs() < 1e-6 {
        return (Vec::new(), true);
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
        if cross.abs() <= 1e-6 {
            // 일직선(collinear) 정점 — 면적 기여가 없으므로 제거만 합니다.
            // (일정 두께의 직선 획처럼 외곽선이 완전 직선인 경우 필수)
            idx.remove((i + 1) % m);
            continue;
        }
        let convex = if ccw { cross > 0.0 } else { cross < 0.0 };
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
        (out, true)
    } else {
        (out, false) // 조기 종료 — 부분 커버만 됨.
    }
}

/// 자기 교차 등으로 완전 삼각분할이 불가능한 입력을 위한 **폴백 지오메트리**:
/// 세그먼트별 quad(세그먼트 법선 — 항상 유한) + 급격히 꺾인 곳의 조인 원 +
/// 양끝 캡 원. 어떤 입력에도 경계가 항상 폴리라인 근처에 머뭅니다.
#[derive(Debug, Clone, PartialEq)]
pub struct FallbackGeometry {
    pub quads: Vec<[[f32; 2]; 4]>,
    /// (중심, 반지름) — 양끝 캡 + 방향이 꺾인 내부 조인.
    pub circles: Vec<([f32; 2], f32)>,
}

/// 폴백 지오메트리를 계산합니다. `points`/`half_widths`는 같은 공간(pt 또는 px).
pub fn stroke_fallback_geometry(
    points: &[[f32; 2]],
    half_widths: &[f32],
) -> FallbackGeometry {
    let n = points.len().min(half_widths.len());
    if n == 0 {
        return FallbackGeometry {
            quads: Vec::new(),
            circles: Vec::new(),
        };
    }
    if n == 1 {
        return FallbackGeometry {
            quads: Vec::new(),
            circles: vec![(points[0], half_widths[0].max(0.0))],
        };
    }
    let mut out = FallbackGeometry {
        quads: Vec::with_capacity(n - 1),
        circles: Vec::with_capacity(2 + n),
    };
    // 세그먼트 quad — 법선은 세그먼트 자체에 수직 (마이터 없음 → 스파이크 없음).
    for i in 0..n - 1 {
        let (a, b) = (points[i], points[i + 1]);
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-6 {
            continue;
        }
        let (nx, ny) = (-dy / len, dx / len);
        let (h0, h1) = (half_widths[i], half_widths[i + 1]);
        out.quads.push([
            [a[0] + nx * h0, a[1] + ny * h0],
            [a[0] - nx * h0, a[1] - ny * h0],
            [b[0] - nx * h1, b[1] - ny * h1],
            [b[0] + nx * h1, b[1] + ny * h1],
        ]);
    }
    // 양끝 캡 원.
    out.circles.push((points[0], half_widths[0].max(0.0)));
    out.circles.push((points[n - 1], half_widths[n - 1].max(0.0)));
    // 방향이 약 15° 이상 꺾이는 내부 점에 조인 원 (이웃 quad 사이 틈 메움).
    for i in 1..n - 1 {
        let (dx0, dy0) = (
            points[i][0] - points[i - 1][0],
            points[i][1] - points[i - 1][1],
        );
        let (dx1, dy1) = (
            points[i + 1][0] - points[i][0],
            points[i + 1][1] - points[i][1],
        );
        let l0 = (dx0 * dx0 + dy0 * dy0).sqrt();
        let l1 = (dx1 * dx1 + dy1 * dy1).sqrt();
        if l0 < 1e-6 || l1 < 1e-6 {
            continue;
        }
        let dot = (dx0 * dx1 + dy0 * dy1) / (l0 * l1);
        if dot < 0.966 {
            let r = half_widths[i]
                .max(half_widths[i - 1])
                .max(half_widths[i + 1]);
            out.circles.push((points[i], r));
        }
    }
    out
}

// ── 통합 스트로크 지오메트리 (캔버스/내보내기 공용 진입점) ───────────────────

/// 완전 삼각분할된 스트로크 지오메트리 (pt 공간).
///
/// `aa_edges`는 **경계 가장자리만** (a, b, 바깥 단위 방향) — 내부 공유
/// 가장자리를 제외해 반투명 잉크의 이음새 얼룩을 막고, 안티앨리어싱 페더
/// 스트립을 만들 때 씁니다. `bbox` = [min_x, min_y, max_x, max_y].
#[derive(Debug, Clone, PartialEq)]
pub struct StrokeTris {
    pub poly: Vec<[f32; 2]>,
    pub tris: Vec<[u32; 3]>,
    pub aa_edges: Vec<(u32, u32, [f32; 2])>,
    pub bbox: [f32; 4],
}

/// 스트로크 렌더 지오메트리 — 완전 분할이면 [`StrokeTris`], 자기 교차
/// 등으로 불가능하면 [`FallbackGeometry`]. 캔버스와 내보내기가 **같은
/// 진입점**을 씁니다.
#[derive(Debug, Clone, PartialEq)]
pub enum StrokeFill {
    Tris(StrokeTris),
    Fallback(FallbackGeometry),
}

/// 외곽선 → (확인된) 삼각분할 → 경계 가장자리/bbox까지 한 번에 계산합니다.
pub fn stroke_geometry(
    points: &[[f32; 2]],
    half_widths: &[f32],
    round_caps: bool,
) -> StrokeFill {
    let poly = stroke_outline(points, half_widths, round_caps);
    let (tris, complete) = triangulate_polygon_checked(&poly);
    if !complete || tris.is_empty() {
        return StrokeFill::Fallback(stroke_fallback_geometry(points, half_widths));
    }
    // 경계 가장자리: 삼각형들 사이에서 1번만 공유된 가장자리.
    let mut counts: std::collections::HashMap<(u32, u32), u32> =
        std::collections::HashMap::new();
    for t in &tris {
        for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
            *counts.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    let mut aa_edges = Vec::new();
    for t in &tris {
        for (ia, ib) in [(0usize, 1usize), (1, 2), (2, 0)] {
            let (a, b) = (t[ia], t[ib]);
            if counts.get(&(a.min(b), a.max(b))) != Some(&1) {
                continue;
            }
            let ic = 3 - ia - ib; // 남은 정점 = 안쪽.
            let (pa, pb, pc) = (
                poly[a as usize],
                poly[b as usize],
                poly[t[ic] as usize],
            );
            let (dx, dy) = (pb[0] - pa[0], pb[1] - pa[1]);
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1e-4 {
                continue;
            }
            let perp = [-dy / len, dx / len];
            // 안쪽 정점(pc)의 반대 방향이 바깥.
            let side = (pc[0] - pa[0]) * perp[0] + (pc[1] - pa[1]) * perp[1];
            let dir = if side < 0.0 { perp } else { [-perp[0], -perp[1]] };
            aa_edges.push((a, b, dir));
        }
    }
    let mut bbox = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
    for p in &poly {
        bbox[0] = bbox[0].min(p[0]);
        bbox[1] = bbox[1].min(p[1]);
        bbox[2] = bbox[2].max(p[0]);
        bbox[3] = bbox[3].max(p[1]);
    }
    StrokeFill::Tris(StrokeTris {
        poly,
        tris,
        aa_edges,
        bbox,
    })
}

// ── 인과적(온라인) 선폭 확정기 ───────────────────────────────────────────────

/// 스트로크 진행 중 점별 선폭을 **입력 즉시 확정**하는 계산기.
///
/// "그리는 동안 보이던 굵기"와 "펜을 뗀 뒤의 굵기"가 **정확히 같도록**
/// 배치 함수([`BallPenProfile::widths`], [`FountainProfile::widths`])와 같은
/// 공식을 순차(인과)적으로 적용합니다:
/// - 속도는 **직전 점과 현재 점**만 사용 (미래 점 금지).
/// - 속도 평활은 EMA(과거 값만 참조).
/// - 이탤릭 닙 계수는 "다음 세그먼트 방향"을 쓰므로, 다음 점이 도착하는
///   순간 이전 점의 폭을 확정합니다 (마지막 점은 이전 세그먼트 — 배치와 동일).
#[derive(Debug, Clone, Copy)]
enum LockerProfile {
    /// 하이라이터 — 모든 점 동일 폭.
    Constant,
    BallPen(BallPenProfile),
    Fountain(FountainProfile),
}

#[derive(Debug, Clone)]
pub struct WidthLocker {
    profile: LockerProfile,
    max_width_pt: f32,
    tilt_mag: f32,
    alpha: f32,
    /// 마지막 스무딩 속도 (EMA 상태).
    prev_speed: f32,
    /// 마지막 점 (아직 폭 확정 전) — 다음 점 도착 시 폭 확정.
    prev_point: Option<StrokePoint>,
    /// 첫 점 (속도가 첫 세그먼트에서 결정되므로 두 번째 점까지 보류).
    first_point: Option<StrokePoint>,
    /// 두 번째 점이 도착해 첫 점이 확정된 이후인지.
    started: bool,
}

impl WidthLocker {
    pub fn new(
        tool: ToolType,
        max_width_pt: f32,
        ball: BallPenProfile,
        fountain: FountainProfile,
        tilt_mag: f32,
    ) -> Self {
        let (profile, alpha) = match tool {
            ToolType::Highlighter => (LockerProfile::Constant, 0.0),
            ToolType::Fountain => (LockerProfile::Fountain(fountain), fountain.speed_smooth),
            _ => (LockerProfile::BallPen(ball), ball.speed_smooth),
        };
        Self {
            profile,
            max_width_pt,
            tilt_mag: tilt_mag.clamp(0.0, 1.0),
            alpha: alpha.clamp(0.0, 1.0),
            prev_speed: 0.0,
            prev_point: None,
            first_point: None,
            started: false,
        }
    }

    fn lock_width(&self, pressure: f32, speed: f32, dir: [f32; 2]) -> f32 {
        match self.profile {
            LockerProfile::Constant => self.max_width_pt,
            LockerProfile::BallPen(b) => b.width_at(self.max_width_pt, pressure, self.tilt_mag, speed),
            LockerProfile::Fountain(f) => {
                let w = f.width_at(self.max_width_pt, pressure, self.tilt_mag, speed);
                let lo = f.min_width_pt.max(0.05).min(self.max_width_pt.max(0.05));
                let hi = self.max_width_pt.max(0.05);
                (w * f.italic_factor(dir[0], dir[1])).clamp(lo, hi)
            }
        }
    }

    /// 새 점을 추가합니다.
    ///
    /// 반환: `.0` = 이전 점의 **확정본**(첫 점이면 None — canvas가 이 값으로
    /// 마지막 점의 폭을 교체), `.1` = 새 점(임시 폭 — 렌더에 바로 사용).
    pub fn push(&mut self, p: StrokePoint) -> (Option<StrokePoint>, StrokePoint) {
        if let Some(first) = self.first_point.take() {
            // 두 번째 점: 첫 점 확정(첫 세그먼트 속도/방향 — 배치와 동일) + 새 점.
            let (v, dir) = seg_speed_dir(&first, &p);
            let s0 = self.alpha * v; // EMA 초기값 0 (배치 ema_speeds와 동일)
            let mut locked_first = first;
            locked_first.width = self.lock_width(first.pressure, s0, dir);
            // 두 번째 점의 배치 속도: 같은 세그먼트로 EMA 한 번 더.
            let s1 = self.alpha * v + (1.0 - self.alpha) * s0;
            self.prev_speed = s1;
            self.started = true;
            let mut tip = p;
            tip.width = self.lock_width(p.pressure, s1, dir);
            self.prev_point = Some(tip);
            (Some(locked_first), tip)
        } else if self.started {
            // 세 번째 이후: 이전 점 확정(이탤릭 "다음 세그먼트" 규칙) + 새 점.
            let prev = self.prev_point.expect("started이면 이전 점 존재");
            let (v, dir) = seg_speed_dir(&prev, &p);
            let s = self.alpha * v + (1.0 - self.alpha) * self.prev_speed;
            let mut locked_prev = prev;
            locked_prev.width = self.lock_width(prev.pressure, self.prev_speed, dir);
            self.prev_speed = s;
            let mut tip = p;
            tip.width = self.lock_width(p.pressure, s, dir);
            self.prev_point = Some(tip);
            (Some(locked_prev), tip)
        } else {
            // 첫 점: 속도는 두 번째 점 도착 시 확정 — 임시 폭(배치 n=1 규칙)으로
            // 바로 그려지도록 합니다.
            let mut tip = p;
            tip.width = self.lock_width(p.pressure, 0.0, [1.0, 0.0]);
            self.first_point = Some(p);
            (None, tip)
        }
    }

    /// 펜을 뗄 때: 마지막 점의 확정 폭을 반환합니다.
    /// (배치 함수의 마지막 점 규칙 — 이탤릭은 이전 세그먼트 방향 — 과 동일)
    pub fn finish(&mut self) -> Option<StrokePoint> {
        if let Some(mut first) = self.first_point.take() {
            first.width = self.lock_width(first.pressure, 0.0, [1.0, 0.0]);
            return Some(first);
        }
        self.prev_point.take()
    }
}

/// 직전 점→현재 점 세그먼트의 (속도, 단위 방향) — 배치 `ema_speeds`와
/// 같은 속도 규칙 (시각 0 또는 dt ≤ 0.1ms면 속도 0).
fn seg_speed_dir(a: &StrokePoint, b: &StrokePoint) -> (f32, [f32; 2]) {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len = (dx * dx + dy * dy).sqrt();
    let dir = if len < 1e-6 { [1.0, 0.0] } else { [dx / len, dy / len] };
    let v = if a.t_ms == 0 || b.t_ms == 0 {
        0.0
    } else {
        let dt = (b.t_ms.saturating_sub(a.t_ms)) as f32 / 1000.0;
        if dt <= 1e-4 {
            0.0
        } else {
            len / dt
        }
    };
    (v, dir)
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

// ── 만년필 물리 모델 (필압 × 속도 × 기울기) ─────────────────────────────────

/// 만년필 프로파일 — 스타일러스의 **필압·속도·기울기**로 실제 만년필의
/// 물리적 특성을 흉내 내는 가변 선폭 모델의 파라미터입니다.
///
/// 핵심 공식 (점 i에서):
/// ```text
/// P_eff = pressure · (1 + k_tilt · T)          // 기울기 → 유효 필압 (곱 구조)
/// v     = EMA(거리/dt)                          // 저역 통과 속도
/// w = w_min + (w_max − w_min) · P_eff^α · 1/(1 + (v/v_ref)^β)
/// if italic: w *= 1 + k_italic · cos(2(δ − φ)) // 스텁 닙 방향 대비
/// if v < v_dwell: w += k_dwell · (v_dwell − v) // 정지 시 잉크 고임
/// w = clamp(w, w_min, w_max)
/// ```
/// 순수 계산이라 GUI 없이 단위 테스트로 검증합니다.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FountainProfile {
    /// 가장 가는 선폭 (pt)
    pub min_width_pt: f32,
    /// 필압 민감도 α (0.3 ~ 2.0)
    pub pressure_alpha: f32,
    /// 속도 민감도 β (0.3 ~ 3.0)
    pub speed_beta: f32,
    /// 기준 속도 (pt/초) — 이 속도에서 속도 계수가 0.5가 됨
    pub speed_ref: f32,
    /// 기울기 영향 계수 k_tilt (0이면 기울기 무시)
    pub tilt_k: f32,
    /// 속도 저역 통과(EMA) 계수 α_smooth (0.05 ~ 1.0)
    pub speed_smooth: f32,
    /// 정지 판정 속도 (pt/초) — 이보다 느리면 잉크가 고임
    pub v_dwell: f32,
    /// 정지 시 잉크 고임 계수 (pt 단위 가산량 배율)
    pub dwell_k: f32,
    /// 이탤릭/스텁 닙 효과 사용 여부
    pub italic: bool,
    /// 닙 축 각도 (도) — 진행 방향과의 차이로 굵기가 달라짐
    pub nib_angle_deg: f32,
    /// 이탤릭 방향 대비 강도 (0이면 무시)
    pub italic_k: f32,
}

impl Default for FountainProfile {
    fn default() -> Self {
        Self {
            min_width_pt: 0.3,
            pressure_alpha: 0.8,
            speed_beta: 1.2,
            speed_ref: 60.0,
            tilt_k: 0.4,
            speed_smooth: 0.3,
            v_dwell: 5.0,
            dwell_k: 0.05,
            italic: false,
            nib_angle_deg: 45.0,
            italic_k: 0.3,
        }
    }
}

impl FountainProfile {
    /// 기울기가 반영된 유효 필압. 곱 구조라 필압 0이면 기울기만으로는
    /// 선이 생기지 않습니다.
    pub fn effective_pressure(&self, pressure: f32, tilt_mag: f32) -> f32 {
        (pressure.clamp(0.0, 1.0) * (1.0 + self.tilt_k.max(0.0) * tilt_mag.clamp(0.0, 1.0)))
            .clamp(0.0, 2.0)
    }

    /// 속도 계수: 정지=1, `speed_ref`=0.5, 빠를수록 0에 수렴.
    pub fn speed_factor(&self, v: f32) -> f32 {
        let v = v.max(0.0);
        let ratio = (v / self.speed_ref.max(1e-3)).powf(self.speed_beta.max(0.0));
        1.0 / (1.0 + ratio)
    }

    /// 이탤릭/스텁 닙 계수: 획 진행 방향(δ)이 닙 축(φ)과 일치하면 굵고,
    /// 수직이면 가늘어집니다.
    pub fn italic_factor(&self, dx: f32, dy: f32) -> f32 {
        if !self.italic {
            return 1.0;
        }
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-6 {
            return 1.0;
        }
        let delta = dy.atan2(dx);
        let phi = self.nib_angle_deg.to_radians();
        1.0 + self.italic_k.max(0.0) * (2.0 * (delta - phi)).cos()
    }

    /// 정지(느린 속도)에서의 잉크 고임 가산량 (pt).
    pub fn dwell_extra(&self, v: f32) -> f32 {
        self.dwell_k.max(0.0) * (self.v_dwell - v.max(0.0)).max(0.0)
    }

    /// 점 하나의 최종 선폭(pt). `max_width_pt`는 툴바 Width(펜촉 최대 폭),
    /// `tilt_mag`은 0..1 기울기 크기, `v`는 스무딩된 속도(pt/초).
    pub fn width_at(&self, max_width_pt: f32, pressure: f32, tilt_mag: f32, v: f32) -> f32 {
        let w_min = self.min_width_pt.max(0.05).min(max_width_pt.max(0.05));
        let w_max = max_width_pt.max(w_min);
        let p = self.effective_pressure(pressure, tilt_mag);
        let mut w = w_min + (w_max - w_min) * p.powf(self.pressure_alpha.max(0.1)) * self.speed_factor(v);
        w += self.dwell_extra(v);
        w.clamp(w_min, w_max)
    }

    /// 획 전체의 점별 **스무딩된 속도**(pt/초). 공용 EMA 계산 사용.
    pub fn speeds(&self, pts: &[StrokePoint]) -> Vec<f32> {
        ema_speeds(pts, self.speed_smooth)
    }

    /// 획 전체의 점별 **최종 선폭(pt)** (스무딩 속도 → 공식 → 클램프).
    /// `tilt_mag`는 획 전체에 일정한 기울기 크기(0..1)입니다.
    pub fn widths(&self, max_width_pt: f32, pts: &[StrokePoint], tilt_mag: f32) -> Vec<f32> {
        let vs = self.speeds(pts);
        pts.iter()
            .enumerate()
            .map(|(i, p)| {
                let w = self.width_at(max_width_pt, p.pressure, tilt_mag, vs[i]);
                // 이탤릭 방향 계수 (다음 세그먼트 방향 기준, 마지막은 이전 세그먼트).
                let (dx, dy) = if i + 1 < pts.len() {
                    (pts[i + 1].x - p.x, pts[i + 1].y - p.y)
                } else if i > 0 {
                    (p.x - pts[i - 1].x, p.y - pts[i - 1].y)
                } else {
                    (1.0, 0.0)
                };
                let w = w * self.italic_factor(dx, dy);
                w.clamp(
                    self.min_width_pt.max(0.05).min(max_width_pt.max(0.05)),
                    max_width_pt.max(0.05),
                )
            })
            .collect()
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

/// 일반 펜(볼펜/젤펜) 물리 모델.
///
/// 일반 펜은 닙이 벌어지지 않고 볼이 회전하며 잉크를 전달하므로,
/// **필압·속도의 영향을 작게** 하고 **선폭을 좁은 범위**로 제한하는 것이 핵심입니다.
///
/// ```text
/// speed_norm = clamp(v / v_max, 0, 1)
/// w = w_base · (1 + k_p·(p − 0.5)) · (1 − k_v · speed_norm)
/// 기울기 끊김(볼펜): elevation < cut이면 w·α에 clamp((elev−cut)/falloff) 곱
/// 과속 잉크 부족: v > v_starve이면 w·α에 1 − clamp((v−v_starve)/falloff) 곱
/// w = clamp(w, base·min_ratio, base·max_ratio)
/// ```
/// 순수 계산이라 GUI 없이 단위 테스트로 검증합니다.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BallPenProfile {
    /// 필압 계수 k_p — 작을수록 필압 영향이 적음 (0.1~0.3 권장).
    pub pressure_k: f32,
    /// 속도 계수 k_v — 작을수록 속도 영향이 적음 (0.05~0.15 권장).
    pub speed_k: f32,
    /// 속도 정규화 상수 v_max (pt/초) — 이 속도 이상이면 속도항이 1.
    pub speed_max: f32,
    /// 속도 저역 통과(EMA) 계수.
    pub speed_smooth: f32,
    /// 최소 선폭 비율 (base 대비).
    pub min_ratio: f32,
    /// 최대 선폭 비율 (base 대비).
    pub max_ratio: f32,
    /// 기울기 끊김(볼펜 눕힘) 사용 여부 — 기울기 센서가 없으면 꺼둠.
    pub tilt_cut_enabled: bool,
    /// 끊김이 시작되는 고도각(도). 90=수직, 0=수평.
    pub tilt_cut_deg: f32,
    /// 끊김 전환 폭(도).
    pub tilt_falloff_deg: f32,
    /// 잉크 부족 시작 속도 (pt/초).
    pub starve_v: f32,
    /// 완전히 끊기는 속도 범위 (pt/초).
    pub starve_falloff: f32,
}

impl Default for BallPenProfile {
    fn default() -> Self {
        Self {
            pressure_k: 0.15,
            speed_k: 0.08,
            speed_max: 600.0,
            speed_smooth: 0.3,
            min_ratio: 0.65,
            max_ratio: 1.35,
            tilt_cut_enabled: false,
            tilt_cut_deg: 30.0,
            tilt_falloff_deg: 15.0,
            starve_v: 900.0,
            starve_falloff: 300.0,
        }
    }
}

impl BallPenProfile {
    /// 기울기 끊김 계수 (0..1): 펜을 너무 눕히면(고도각이 낮아지면) 0으로 수렴.
    /// `tilt_mag`는 0(수직)..1(완전 수평) 기울기 크기.
    pub fn tilt_cut_factor(&self, tilt_mag: f32) -> f32 {
        if !self.tilt_cut_enabled {
            return 1.0;
        }
        let elev_deg = 90.0 * (1.0 - tilt_mag.clamp(0.0, 1.0));
        ((elev_deg - self.tilt_cut_deg) / self.tilt_falloff_deg.max(1.0)).clamp(0.0, 1.0)
    }

    /// 과속 잉크 부족 계수 (0..1): `starve_v`를 넘으면 서서히 끊김.
    pub fn starve_factor(&self, v: f32) -> f32 {
        if v <= self.starve_v {
            1.0
        } else {
            (1.0 - ((v - self.starve_v) / self.starve_falloff.max(1.0)).min(1.0)).max(0.0)
        }
    }

    /// 점 하나의 선폭(pt). `base_pt`는 툴바 Width(기본 선폭), `tilt_mag`는 0..1.
    pub fn width_at(&self, base_pt: f32, pressure: f32, tilt_mag: f32, v: f32) -> f32 {
        let speed_norm = (v.max(0.0) / self.speed_max.max(1.0)).clamp(0.0, 1.0);
        let mut w = base_pt.max(0.05)
            * (1.0 + self.pressure_k * (pressure.clamp(0.0, 1.0) - 0.5))
            * (1.0 - self.speed_k.max(0.0) * speed_norm);
        w *= self.tilt_cut_factor(tilt_mag) * self.starve_factor(v);
        let lo = base_pt.max(0.05) * self.min_ratio.min(self.max_ratio);
        let hi = base_pt.max(0.05) * self.max_ratio;
        w.clamp(lo.min(hi), hi.max(lo))
    }

    /// 획 전체의 점별 스무딩 속도 (공용 EMA).
    pub fn speeds(&self, pts: &[StrokePoint]) -> Vec<f32> {
        ema_speeds(pts, self.speed_smooth)
    }

    /// 획 전체의 점별 최종 선폭(pt).
    pub fn widths(&self, base_pt: f32, pts: &[StrokePoint], tilt_mag: f32) -> Vec<f32> {
        let vs = self.speeds(pts);
        pts.iter()
            .enumerate()
            .map(|(i, p)| self.width_at(base_pt, p.pressure, tilt_mag, vs[i]))
            .collect()
    }
}

/// 점별 **스무딩된 속도**(pt/초) — 공용 저역 통과(EMA) 계산.
/// 시각이 0(미기록)이거나 dt ≤ 0이면 속도 0으로 처리하고,
/// 첫 점은 다음 세그먼트의 속도를 사용합니다.
pub(crate) fn ema_speeds(pts: &[StrokePoint], smooth: f32) -> Vec<f32> {
    let n = pts.len();
    let mut out = Vec::with_capacity(n);
    if n == 0 {
        return out;
    }
    let alpha = smooth.clamp(0.0, 1.0);
    let mut prev_v = 0.0f32;
    for i in 0..n {
        let (j, k) = if i == 0 {
            if n > 1 {
                (0, 1)
            } else {
                (0, 0)
            }
        } else {
            (i - 1, i)
        };
        let v = if j == k || pts[k].t_ms == 0 || pts[j].t_ms == 0 {
            0.0
        } else {
            let dt = (pts[k].t_ms.saturating_sub(pts[j].t_ms)) as f32 / 1000.0;
            if dt <= 1e-4 {
                0.0
            } else {
                let dx = pts[k].x - pts[j].x;
                let dy = pts[k].y - pts[j].y;
                (dx * dx + dy * dy).sqrt() / dt
            }
        };
        let v_smooth = alpha * v + (1.0 - alpha) * prev_v;
        out.push(v_smooth);
        prev_v = v_smooth;
    }
    out
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
    fn ballpen_width_variation_is_small_and_bounded() {
        let p = BallPenProfile::default(); // base 1.0, ratio 0.65..1.35
        for (pr, v) in [(0.0f32, 0.0f32), (1.0, 0.0), (0.5, 600.0), (1.0, 6000.0)] {
            let w = p.width_at(1.0, pr, 0.0, v);
            assert!(w >= 0.65 - 1e-4 && w <= 1.35 + 1e-4, "좁은 범위: {w}");
        }
    }

    #[test]
    fn ballpen_pressure_and_speed_effects_are_gentle() {
        let p = BallPenProfile::default();
        let light = p.width_at(1.0, 0.0, 0.0, 300.0);
        let full = p.width_at(1.0, 1.0, 0.0, 300.0);
        let slow = p.width_at(1.0, 1.0, 0.0, 0.0);
        let fast = p.width_at(1.0, 1.0, 0.0, 600.0);
        // 영향은 존재하되 작음 (±20% 안팎).
        assert!(full > light, "필압↑ → 약간 굵어짐");
        assert!(slow > fast, "속도↑ → 약간 가늘어짐");
        assert!((full - light) < 0.25, "필압 영향이 너무 큼: {} vs {}", light, full);
        assert!((slow - fast) < 0.25, "속도 영향이 너무 큼: {} vs {}", fast, slow);
    }

    #[test]
    fn ballpen_tilt_cut_skips_ink_when_laid_down() {
        let mut p = BallPenProfile::default();
        p.tilt_cut_enabled = true;
        p.tilt_cut_deg = 30.0;
        p.tilt_falloff_deg = 15.0;
        assert!((p.tilt_cut_factor(0.0) - 1.0).abs() < 1e-5, "수직 = 끊김 없음");
        assert!((p.tilt_cut_factor(0.5) - 1.0).abs() < 1e-5, "45° 고도 = 아직 끊김 없음");
        assert!(p.tilt_cut_factor(0.75) < 1e-5, "낮게 누움 = 완전 끊김");
        assert!(p.tilt_cut_factor(1.0) < 1e-5, "완전 수평 = 끊김");
        // 끄면 항상 1.
        p.tilt_cut_enabled = false;
        assert!((p.tilt_cut_factor(1.0) - 1.0).abs() < 1e-5);
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
        assert!((area - expected).abs() < 1.0, "면적 일치: {area} vs {expected}");
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
        assert!((area - poly_area).abs() < 1e-3, "겹침 없이 정확히 한 번 덮음");
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
        let f = FountainProfile::default(); // v_ref=60, beta=1.2
        assert!((f.speed_factor(0.0) - 1.0).abs() < 1e-5, "정지 = 최대");
        assert!((f.speed_factor(60.0) - 0.5).abs() < 1e-5, "v_ref = 0.5");
        assert!(f.speed_factor(600.0) < 0.1, "빠르면 얇아짐");
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
        let (a, b, c) = (poly[t[0] as usize], poly[t[1] as usize], poly[t[2] as usize]);
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
        let mut locker =
            WidthLocker::new(ToolType::Fountain, 2.5, BallPenProfile::default(), f, 0.0);
        for i in 0..40 {
            let x = i as f32 * 6.0 + (i as f32 * 0.7).sin() * 8.0;
            let y = 100.0 + (i as f32 * 0.35).cos() * 30.0 + i as f32 * 1.5;
            let p = StrokePoint::with_time(
                x,
                y,
                0.4 + 0.5 * (i % 7) as f32 / 7.0,
                t0 + i as u64 * 8,
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
        let mut locker =
            WidthLocker::new(ToolType::Pen, 2.0, b, FountainProfile::default(), 0.0);
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
        let mut locker =
            WidthLocker::new(ToolType::Fountain, 2.5, BallPenProfile::default(), f, 0.0);
        let (_, tip) = locker.push(p);
        let done = locker.finish().unwrap();
        let batch = f.widths(2.5, &[p], 0.0);
        assert!((tip.width - batch[0]).abs() < 1e-4, "임시 폭도 일치");
        assert!((done.width - batch[0]).abs() < 1e-4, "확정 폭 일치");
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
}
