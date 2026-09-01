//! 펜 설정: 색상 팔레트(빨강/파랑/검정 계열)와 필압 → 두께 곡선.

use serde::{Deserialize, Serialize};

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
}
