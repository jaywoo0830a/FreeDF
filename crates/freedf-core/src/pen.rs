//! 펜 설정: 색상 팔레트(빨강/파랑/검정 계열)와 필압 → 두께 곡선.

use serde::{Deserialize, Serialize};

use crate::model::ToolType;

/// 잉크 도구별 두께 배율을 계산합니다 (렌더/내보내기 공용).
///
/// - `Pen`: 1.0 — 앱은 전역 필압 곡선(`PressureCurve`)을 별도로 곱합니다.
/// - `Ballpoint`: 필압에 살짝만 반응 (거의 일정).
/// - `Fountain`: 필압(아래로 누르면 굵게) + 속도(빠르면 얇게)로 닙 느낌.
/// - 그 외(Highlighter 등): 1.0 (기존 경로 사용).
///
/// `speed`는 인접 두 점 사이 페이지 좌표 거리(포인트)입니다.
pub fn ink_modifier(tool: ToolType, pressure: f32, speed: f32) -> f32 {
    let p = if pressure.is_nan() {
        1.0
    } else {
        pressure.clamp(0.0, 1.0)
    };
    match tool {
        ToolType::Pen | ToolType::Highlighter | ToolType::Eraser | ToolType::Pan => 1.0,
        ToolType::Ballpoint => 0.82 + 0.24 * p,
        ToolType::Fountain => {
            let nib = 0.5 + 1.15 * p; // 누를수록 굵게 (최대 ~1.65x)
            let speed = if speed.is_finite() { speed.abs() } else { 0.0 };
            let thin = 1.0 / (1.0 + (speed * 0.012).min(1.6)); // 빠르면 얇게
            (nib * thin).clamp(0.4, 1.7)
        }
    }
}

/// 잉크 도구(만년필/볼펜)인지 — 전역 필압 곡선을 쓰지 않고 자체 프로파일을 쓰는 도구.
pub fn uses_own_profile(tool: ToolType) -> bool {
    matches!(tool, ToolType::Ballpoint | ToolType::Fountain)
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

    #[test]
    fn ink_modifier_profiles() {
        use crate::model::ToolType;
        // 펜/하이라이터는 1.0 (전역 곡선이 담당)
        assert!((ink_modifier(ToolType::Pen, 0.3, 5.0) - 1.0).abs() < 1e-6);
        assert!((ink_modifier(ToolType::Highlighter, 0.3, 5.0) - 1.0).abs() < 1e-6);
        // 볼펜: 필압에 조금만 반응, 일정에 가까움
        let b_light = ink_modifier(ToolType::Ballpoint, 0.1, 5.0);
        let b_press = ink_modifier(ToolType::Ballpoint, 1.0, 5.0);
        assert!(b_light > 0.8 && b_press < 1.1);
        assert!(b_press > b_light);
        // 만년필: 누르면 굵어지고 빠르면 얇아짐
        let f_slow_press = ink_modifier(ToolType::Fountain, 1.0, 0.5);
        let f_fast_light = ink_modifier(ToolType::Fountain, 0.1, 200.0);
        assert!(f_slow_press > 1.4, "천천히 강하게: {f_slow_press}");
        assert!(f_fast_light < f_slow_press, "빠르게 가볍게는 더 얇아야");
        assert!(uses_own_profile(ToolType::Fountain));
        assert!(!uses_own_profile(ToolType::Pen));
    }
}
