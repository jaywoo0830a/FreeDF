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

/// 가변폭 폴리라인의 채움 지오메트리 — 벡터 드로잉의 관례적인
/// **라운드 캡/조인** 방식으로 분해한 결과입니다.
///
/// 이전 구현은 점별 **마이터(이등분) 법선**으로 세그먼트 quad를 만들어
/// 급격히 방향이 꺾이는 곳(거의 180°로 되접힐 때)에서 법선이 무한대로
/// 커져 **번개 모양 스파이크**가 튀었습니다. 이 방식은:
/// - quad는 **세그먼트 자체의 법선**만 사용 → 어떤 입력에도 스파이크가 생기지 않음.
/// - 방향이 일정 각도(약 15°) 이상 꺾이는 내부 점에는 **조인 원**을,
///   양끝에는 **캡 원**을 얹어 이음새/끝을 매끄럽게 이어줍니다.
///
/// 좌표는 입력과 같은 공간이며, 화면(egui)과 내보내기(래스터)가
/// **같은 함수**를 써서 동일하게 그려집니다.
#[derive(Debug, Clone, PartialEq)]
pub struct StrokeShape {
    /// 세그먼트별 사각형 (점 i → i+1, 세그먼트 법선 사용).
    pub quads: Vec<[[f32; 2]; 4]>,
    /// (중심, 반지름) 원 목록 — 양끝 캡 + 방향이 꺾인 내부 조인.
    pub circles: Vec<([f32; 2], f32)>,
}

/// 두 인접 세그먼트 방향의 코사인 유사도가 이 값보다 작으면(약 15° 이상
/// 꺾이면) 조인 원을 얹습니다. 완만한 곡선은 quad만으로 이음새가 안 보이므로
/// 원(오버드로)을 줄여 반투명 잉크의 얼룩을 막습니다.
const MIN_JOIN_COS: f32 = 0.966;

/// 가변폭 폴리라인 지오메트리를 계산합니다.
///
/// `points`와 `half_widths`의 길이는 같아야 합니다 (짧은 쪽 기준).
/// 점이 1개면 원 하나(점 찍기)를 반환합니다.
pub fn stroke_shape(points: &[[f32; 2]], half_widths: &[f32]) -> StrokeShape {
    let n = points.len().min(half_widths.len());
    if n == 0 {
        return StrokeShape {
            quads: Vec::new(),
            circles: Vec::new(),
        };
    }
    if n == 1 {
        return StrokeShape {
            quads: Vec::new(),
            circles: vec![(points[0], half_widths[0].max(0.0))],
        };
    }
    let mut shape = StrokeShape {
        quads: Vec::with_capacity(n - 1),
        circles: Vec::with_capacity(2 + n),
    };
    // 1) 세그먼트 quad — 법선은 세그먼트 자체에 수직 (마이터 없음 → 스파이크 없음).
    for i in 0..n - 1 {
        let (a, b) = (points[i], points[i + 1]);
        let dx = b[0] - a[0];
        let dy = b[1] - a[1];
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-6 {
            continue;
        }
        let (nx, ny) = (-dy / len, dx / len);
        let h0 = half_widths[i];
        let h1 = half_widths[i + 1];
        shape.quads.push([
            [a[0] + nx * h0, a[1] + ny * h0],
            [a[0] - nx * h0, a[1] - ny * h0],
            [b[0] - nx * h1, b[1] - ny * h1],
            [b[0] + nx * h1, b[1] + ny * h1],
        ]);
    }
    // 2) 양끝 캡 (라운드 캡이 관례).
    shape.circles.push((points[0], half_widths[0].max(0.0)));
    shape.circles.push((points[n - 1], half_widths[n - 1].max(0.0)));
    // 3) 방향이 꺾이는 내부 점에 조인 원 (이웃 quad 사이 쐐기 틈을 메움).
    for i in 1..n - 1 {
        let (dx0, dy0) = (points[i][0] - points[i - 1][0], points[i][1] - points[i - 1][1]);
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
        if dot < MIN_JOIN_COS {
            let r = half_widths[i]
                .max(half_widths[i - 1])
                .max(half_widths[i + 1]);
            shape.circles.push((points[i], r));
        }
    }
    shape
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

    // ---------- stroke_shape (관례적 라운드 조인/캡) ----------

    #[test]
    fn stroke_shape_single_point_is_one_circle() {
        let s = stroke_shape(&[[5.0, 6.0]], &[2.0]);
        assert!(s.quads.is_empty());
        assert_eq!(s.circles, vec![([5.0, 6.0], 2.0)]);
    }

    #[test]
    fn stroke_shape_straight_line_has_one_quad_and_two_caps() {
        let s = stroke_shape(&[[0.0, 0.0], [10.0, 0.0]], &[2.0, 2.0]);
        assert_eq!(s.quads.len(), 1);
        // quad는 선에서 정확히 ±half 떨어져 있음.
        let q = s.quads[0];
        for p in q {
            assert!((p[1].abs() - 2.0).abs() < 1e-6, "수직 오프셋: {p:?}");
            assert!(p[0] >= -1e-6 && p[0] <= 10.0 + 1e-6);
        }
        // 캡 2개, 조인 없음 (방향 변화 없음).
        assert_eq!(s.circles.len(), 2);
        assert_eq!(s.circles[0], ([0.0, 0.0], 2.0));
        assert_eq!(s.circles[1], ([10.0, 0.0], 2.0));
    }

    #[test]
    fn stroke_shape_no_spikes_on_sharp_reversal() {
        // 180° 되접힘 — 마이터 법선이면 무한대 스파이크가 튀는 입력.
        let pts = [[0.0f32, 0.0], [10.0, 0.0], [0.0, 0.0]];
        let s = stroke_shape(&pts, &[2.0, 2.0, 2.0]);
        // 모든 quad 정점이 폴리라인에서 max_half(+여유) 이내에 있어야 함.
        for p in s.quads.iter().flatten() {
            let d = min_dist_to_polyline(*p, &pts);
            assert!(d <= 2.0 + 1e-3, "스파이크: {p:?} 거리 {d}");
        }
        // 되접힘 점에 조인 원.
        assert!(s.circles.iter().any(|(c, r)| *c == [10.0, 0.0] && *r >= 2.0));
    }

    #[test]
    fn stroke_shape_jagged_input_stays_bounded() {
        // 번개 지그재그 — 모든 quad가 폴리라인에서 half 이내.
        let mut pts = vec![[0.0f32, 0.0]];
        let mut x = 0.0f32;
        let mut y = 0.0f32;
        for i in 1..40 {
            x += 8.0;
            y = if i % 2 == 0 { -4.0 } else { 4.0 };
            pts.push([x, y]);
        }
        let halves: Vec<f32> = (0..40).map(|i| 1.0 + 0.05 * i as f32).collect();
        let s = stroke_shape(&pts, &halves);
        let max_h = halves.iter().cloned().fold(0.0f32, f32::max);
        for p in s.quads.iter().flatten() {
            let d = min_dist_to_polyline(*p, &pts);
            assert!(d <= max_h + 1e-3, "스파이크: {p:?} 거리 {d} (max {max_h})");
        }
        // 캡 2 + 꺾인 내부 점마다 조인.
        assert_eq!(s.circles.len(), 2 + 38);
    }

    #[test]
    fn stroke_shape_variable_width_quads_follow_half_widths() {
        let s = stroke_shape(&[[0.0, 0.0], [10.0, 0.0]], &[1.0, 4.0]);
        let q = s.quads[0];
        // 시작점은 ±1, 끝점은 ±4 오프셋.
        assert!((q[0][1] - 1.0).abs() < 1e-6 && (q[1][1] + 1.0).abs() < 1e-6);
        assert!((q[2][1] + 4.0).abs() < 1e-6 && (q[3][1] - 4.0).abs() < 1e-6);
    }

    fn min_dist_to_polyline(p: [f32; 2], poly: &[[f32; 2]]) -> f32 {
        let mut best = f32::MAX;
        for i in 0..poly.len() {
            let d = ((p[0] - poly[i][0]).powi(2) + (p[1] - poly[i][1]).powi(2)).sqrt();
            best = best.min(d);
        }
        best
    }
}
