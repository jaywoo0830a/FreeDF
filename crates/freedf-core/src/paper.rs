//! 노트/페이지의 용지 스타일(그리드·줄·점선)과 배경 색.
//!
//! 순수 데이터/수학만 담고 있어 GUI 없이 단위 테스트로 검증합니다.
//! 좌표는 페이지 좌표계(포인트)입니다.

use serde::{Deserialize, Serialize};

/// 그리드/줄 스타일.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaperStyle {
    /// 흰 종이
    Blank,
    /// 가로줄 노트
    Ruled,
    /// 모눈종이
    Grid,
    /// 점선 모눈
    Dotted,
}

impl PaperStyle {
    pub fn label(self) -> &'static str {
        match self {
            PaperStyle::Blank => "Blank",
            PaperStyle::Ruled => "Ruled",
            PaperStyle::Grid => "Grid",
            PaperStyle::Dotted => "Dotted",
        }
    }

    pub fn all() -> [PaperStyle; 4] {
        [PaperStyle::Blank, PaperStyle::Ruled, PaperStyle::Grid, PaperStyle::Dotted]
    }
}

/// 용지 배경색 팔레트 (RGBA) — GoodNotes 풍 종이 색.
pub const PAPER_COLORS: &[[u8; 4]] = &[
    [255, 255, 255, 255], // White
    [251, 243, 220, 255], // Cream
    [249, 237, 197, 255], // Warm yellow
    [234, 242, 248, 255], // Ice blue
    [240, 240, 240, 255], // Light gray
];

/// 기본 용지 색 (White).
pub const PAPER_WHITE: [u8; 4] = [255, 255, 255, 255];

/// 그리드 간격 (포인트, 약 6mm).
pub const GRID_SPACING_PTS: f32 = 24.0;

/// 기본 줄/격자/점 색 (GoodNotes 풍 연한 회청).
pub const PAPER_LINE: [u8; 4] = [180, 186, 198, 120];
/// 기본 줄/격자/점 두께 (포인트).
pub const PAPER_LINE_WIDTH_PT: f32 = 1.2;

fn default_line_color() -> [u8; 4] {
    PAPER_LINE
}

fn default_line_width() -> f32 {
    PAPER_LINE_WIDTH_PT
}

fn default_spacing() -> f32 {
    GRID_SPACING_PTS
}

/// 유효한 용지 라인 두께(포인트). 0.25~8로 제한.
pub fn clamp_line_width(w: f32) -> f32 {
    if !w.is_finite() || w <= 0.0 {
        PAPER_LINE_WIDTH_PT
    } else {
        w.clamp(0.25, 8.0)
    }
}

/// 표준 종이 크기.
///
/// 새 노트/페이지를 만들 때 물리적 PDF 페이지 크기로 사용합니다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PaperSize {
    A3,
    A4,
    A5,
    Letter,
    Legal,
    /// 사용자 정의 크기 — 실제 치수는 앱 상태(custom_paper_size)가 들고 있습니다.
    Custom,
}

impl PaperSize {
    pub fn label(self) -> &'static str {
        match self {
            PaperSize::A3 => "A3",
            PaperSize::A4 => "A4",
            PaperSize::A5 => "A5",
            PaperSize::Letter => "Letter",
            PaperSize::Legal => "Legal",
            PaperSize::Custom => "Custom",
        }
    }

    pub fn all() -> [PaperSize; 6] {
        [
            PaperSize::A3,
            PaperSize::A4,
            PaperSize::A5,
            PaperSize::Letter,
            PaperSize::Legal,
            PaperSize::Custom,
        ]
    }

    /// 페이지 크기(포인트). 세로 방향 기준 [width, height].
    ///
    /// `Custom`은 자체 치수가 없어 A4를 반환합니다 — 호출부에서
    /// 사용자 정의 치수를 대신 사용해야 합니다.
    pub fn size_pts(self) -> [f32; 2] {
        match self {
            PaperSize::A3 => [841.89, 1190.55],
            PaperSize::A4 => [595.28, 841.89],
            PaperSize::A5 => [419.53, 595.28],
            PaperSize::Letter => [612.0, 792.0],
            PaperSize::Legal => [612.0, 1008.0],
            PaperSize::Custom => [595.28, 841.89],
        }
    }

    /// 주어진 페이지 크기(포인트)에 가장 가까운 표준 크기.
    /// 문서를 열 때 실제 페이지 크기를 표시/기록하는 데 사용합니다.
    pub fn matching(w: f32, h: f32) -> PaperSize {
        Self::all()
            .into_iter()
            .min_by(|a, b| {
                let sa = a.size_pts();
                let sb = b.size_pts();
                let da = (sa[0] - w).abs() + (sa[1] - h).abs();
                let db = (sb[0] - w).abs() + (sb[1] - h).abs();
                da.total_cmp(&db)
            })
            .unwrap_or(PaperSize::A4)
    }
}

impl Default for PaperSize {
    fn default() -> Self {
        PaperSize::A4
    }
}

/// 한 페이지의 용지 설정 — **스타일 + 배경 색만** 페이지별로 저장합니다.
///
/// 줄/격자/점의 간격·색·두께는 **스타일별 프리셋**([`PaperStyleSettings`])을
/// 렌더 시점에 참조하므로 페이지마다 중복 저장하지 않습니다. 즉,
/// 프리셋을 바꾸면 그 스타일을 쓰는 **모든 페이지**가 즉시 함께 바뀝니다.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PagePaper {
    pub style: PaperStyle,
    pub color: [u8; 4],
}

impl Default for PagePaper {
    fn default() -> Self {
        Self {
            style: PaperStyle::Blank,
            color: PAPER_WHITE,
        }
    }
}

/// 줄/격자/점의 세부설정 (간격/색/두께) — 스타일 하나의 정의.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LineStyle {
    /// 간격 (포인트). 0 이하면 `GRID_SPACING_PTS`로 대체.
    #[serde(default = "default_spacing")]
    pub spacing: f32,
    /// 색 (RGBA).
    #[serde(default = "default_line_color")]
    pub color: [u8; 4],
    /// 두께 (포인트).
    #[serde(default = "default_line_width")]
    pub width: f32,
}

impl Default for LineStyle {
    fn default() -> Self {
        Self {
            spacing: GRID_SPACING_PTS,
            color: PAPER_LINE,
            width: PAPER_LINE_WIDTH_PT,
        }
    }
}

/// **스타일별 독립 세부설정** — Ruled / Grid / Dotted 각각 자기만의
/// 간격·색·두께를 가집니다. Blank는 줄이 없어 항목이 없습니다.
///
/// 앱의 Paper 설정 창에서 "현재 선택된 스타일"의 값을 편집하며,
/// 한 스타일의 값 변경은 그 스타일을 쓰는 모든 페이지에 반영됩니다.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PaperStyleSettings {
    pub ruled: LineStyle,
    pub grid: LineStyle,
    pub dotted: LineStyle,
}

impl Default for PaperStyleSettings {
    fn default() -> Self {
        Self {
            ruled: LineStyle::default(),
            grid: LineStyle::default(),
            dotted: LineStyle::default(),
        }
    }
}

impl PaperStyleSettings {
    /// 스타일에 해당하는 세부설정 (Blank는 줄이 없어 `None`).
    pub fn of(&self, style: PaperStyle) -> Option<LineStyle> {
        match style {
            PaperStyle::Blank => None,
            PaperStyle::Ruled => Some(self.ruled),
            PaperStyle::Grid => Some(self.grid),
            PaperStyle::Dotted => Some(self.dotted),
        }
    }

    /// 스타일의 세부설정을 통째로 교체합니다 (Blank는 무시).
    pub fn set(&mut self, style: PaperStyle, value: LineStyle) {
        match style {
            PaperStyle::Ruled => self.ruled = value,
            PaperStyle::Grid => self.grid = value,
            PaperStyle::Dotted => self.dotted = value,
            PaperStyle::Blank => {}
        }
    }
}

/// 유효한 용지 간격(포인트). 너무 작으면 12, 크면 120으로 제한.
pub fn clamp_spacing(spacing: f32) -> f32 {
    if !spacing.is_finite() || spacing <= 0.0 {
        GRID_SPACING_PTS
    } else {
        spacing.clamp(12.0, 120.0)
    }
}

/// 용지 스타일에 따라 그릴 선분 [x0, y0, x1, y1] (페이지 포인트)을 반환합니다.
/// Ruled는 가로줄, Grid는 가로+세로, Blank/Dotted는 빈 벡터.
pub fn paper_lines(w: f32, h: f32, style: PaperStyle, spacing: f32) -> Vec<[f32; 4]> {
    let gap = clamp_spacing(spacing);
    let mut out = Vec::new();
    match style {
        PaperStyle::Blank | PaperStyle::Dotted => {}
        PaperStyle::Ruled => {
            let mut y = gap;
            while y < h {
                out.push([0.0, y, w, y]);
                y += gap;
            }
        }
        PaperStyle::Grid => {
            let mut y = gap;
            while y < h {
                out.push([0.0, y, w, y]);
                y += gap;
            }
            let mut x = gap;
            while x < w {
                out.push([x, 0.0, x, h]);
                x += gap;
            }
        }
    }
    out
}

/// Dotted 스타일의 점 위치 [x, y] (페이지 포인트).
pub fn paper_dots(w: f32, h: f32, style: PaperStyle, spacing: f32) -> Vec<[f32; 2]> {
    if style != PaperStyle::Dotted {
        return Vec::new();
    }
    let gap = clamp_spacing(spacing);
    let mut out = Vec::new();
    let half = gap / 2.0;
    let mut y = half;
    while y < h {
        let mut x = half;
        while x < w {
            out.push([x, y]);
            x += gap;
        }
        y += gap;
    }
    out
}

/// 페이지 **표시 회전**을 반영한 줄을 반환합니다 (화면에 그리는 용).
///
/// 회전은 줄을 종이와 함께 돌립니다: Ruled는 90/270°에서 세로줄로,
/// 0/180°에서는 가로줄로 그립니다. Grid/점은 90° 회전에 불변이라 그대로입니다.
/// (균일 간격이라 위상 이동은 보이지 않음 — 스트로크는 앱이 같은 회전 변환으로
/// 회전하므로 줄과 정렬이 유지됩니다.)
pub fn paper_lines_rotated(
    w: f32,
    h: f32,
    style: PaperStyle,
    spacing: f32,
    rotation: crate::text::PageRotation,
) -> Vec<[f32; 4]> {
    let vertical = style == PaperStyle::Ruled
        && matches!(
            rotation,
            crate::text::PageRotation::Degrees90 | crate::text::PageRotation::Degrees270
        );
    if vertical {
        // 가로줄을 그린 뒤 선분을 90° 회전한 것과 같은 집합 — 세로줄.
        let gap = clamp_spacing(spacing);
        let mut out = Vec::new();
        let mut x = gap;
        while x < w {
            out.push([x, 0.0, x, h]);
            x += gap;
        }
        out
    } else {
        paper_lines(w, h, style, spacing)
    }
}

/// 타일 가능한 값 노이즈 — 격자 인덱스를 셀 수(cu, cv)로 **나머지 연산**해
/// 타일 경계(u=0 ↔ u=1, v=0 ↔ v=1)에서 정확히 이어지게 합니다.
/// 일반 `value_noise`는 정수 주기가 없어(격자 해시가 매 셀 다름) 이음새가 생깁니다.
fn tile_noise(u: f32, v: f32, seed: u64, cu: u32, cv: u32) -> f32 {
    let x = u * cu as f32;
    let y = v * cv as f32;
    let mut ix = x.floor() as i32;
    let mut iy = y.floor() as i32;
    let fx = x - ix as f32;
    let fy = y - iy as f32;
    let sx = fx * fx * (3.0 - 2.0 * fx);
    let sy = fy * fy * (3.0 - 2.0 * fy);
    let (cu, cv) = (cu as i32, cv as i32);
    let wrap = |i: i32, n: i32| ((i % n) + n) % n;
    ix = wrap(ix, cu);
    iy = wrap(iy, cv);
    let h = crate::ink::hash2;
    let a = h(ix, iy, seed);
    let b = h(wrap(ix + 1, cu), iy, seed);
    let c = h(ix, wrap(iy + 1, cv), seed);
    let d = h(wrap(ix + 1, cu), wrap(iy + 1, cv), seed);
    let lo = a + (b - a) * sx;
    let hi = c + (d - c) * sx;
    lo + (hi - lo) * sy
}

/// 종이 높이장 h(u,v) — 문서 §2.
///
/// 모틀링(지료 밀도, FBM 3옥타브) + 섬유(이방성 두 방향) + 스펙(필러 입자)의
/// 합입니다. 모든 성분이 **토러스 격자 노이즈**(`tile_noise`)라 타일 경계에서
/// 완전히 이음새 없이 이어집니다.
pub fn paper_field(u: f32, v: f32, seed: u64) -> f32 {
    // ① 모틀링 — 지료 분산 편차 (FBM 3옥타브).
    let m0 = tile_noise(u, v, seed, 5, 5);
    let m1 = tile_noise(
        u + 1.7 / 11.0,
        v + 4.2 / 11.0,
        seed ^ 0xA511_7095_B320_FCB3,
        11,
        11,
    );
    let m2 = tile_noise(
        u + 9.3 / 23.0,
        v + 2.6 / 23.0,
        seed.wrapping_mul(0xD1B5_4A32_D192_ED03),
        23,
        23,
    );
    let mottling = 0.5 * m0 + 0.3 * m1 + 0.2 * m2 - 0.5;
    // ② 섬유 — 가로/세로 이방성 셀 두 층 (토러스 격자라 이음새 없음).
    let fib_h = tile_noise(
        u + 5.1 / 18.0,
        v + 8.8 / 4.0,
        seed ^ 0x9E37_79B9_7F4A_7C15,
        18,
        4,
    ) - 0.5;
    let fib_v = tile_noise(
        u + 2.2 / 4.0,
        v + 6.4 / 18.0,
        seed ^ 0xBF58_476D_1CE4_E5B9,
        4,
        18,
    ) - 0.5;
    // ③ 스펙 — 필러 입자/미세 먼지 (고주파 점 얼룩).
    let speck = tile_noise(
        u + 11.2 / 29.0,
        v + 13.9 / 29.0,
        seed ^ 0x94D0_49BB_1331_11EB,
        29,
        29,
    ) - 0.5;
    (1.35 * mottling + 0.45 * fib_h + 0.45 * fib_v + 0.35 * speck).clamp(-1.0, 1.0)
}

// ═════════════════════════════════════════════════════════════════════
// 물리 기반 종이 표면 모델 — docs/paper-texture-model.md 구현
//
// 함수형 스타일: 모든 함수는 순수(입력 불변·부수효과 없음)하며,
// 최종 텍스처는 순수 함수들의 합성(iterator pipeline)으로 계산됩니다.
// ═════════════════════════════════════════════════════════════════════

/// 종이 표면의 조명·요철 파라미터 (문서 §6).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PaperSurfaceSettings {
    /// 요철(범프) 강도 β — 높이장 기울기가 법선에 미치는 배율.
    pub bump: f32,
    /// 휘도 요동 진폭 a_L — 섬유 밀도의 광흡수 편차 (채널 공통).
    pub albedo_l: f32,
    /// 색도 요동 진폭 a_C — 충전재·형광증백제의 미세 스펙트럼 편차 (채널 독립).
    pub albedo_c: f32,
    /// 광원 방위각 θ (도).
    pub light_azimuth_deg: f32,
    /// 광원 고도 φ (도).
    pub light_elevation_deg: f32,
    /// 주변광 강도 E_a.
    pub ambient: f32,
    /// 직사광 강도 E_d.
    pub direct: f32,
    /// 골 차폐 강도 k_ao — A(x) = 1 − k_ao·max(0, −h).
    pub ao_strength: f32,
    /// 시닝 강도 ρ_s (Blinn-Phong).
    pub sheen: f32,
    /// 광택 지수 α (클수록 좁은 하이라이트).
    pub gloss: f32,
}

impl Default for PaperSurfaceSettings {
    fn default() -> Self {
        Self {
            // 은은한 기본값 — 흰 종이에 미세한 결만 남는 수준.
            bump: 0.35,
            albedo_l: 0.04,
            albedo_c: 0.03,
            light_azimuth_deg: 45.0,
            light_elevation_deg: 55.0,
            ambient: 0.55,
            direct: 0.45,
            ao_strength: 0.25,
            sheen: 0.04,
            gloss: 8.0,
        }
    }
}

/// 높이장의 중앙 차분 기울기 ∇h (문서 §3).
/// `step`은 한 텍셀에 해당하는 타일 좌표 간격(1/size).
pub fn paper_gradient(u: f32, v: f32, seed: u64, step: f32) -> (f32, f32) {
    let dx =
        (paper_field(u + step, v, seed) - paper_field(u - step, v, seed)) / (2.0 * step);
    let dy =
        (paper_field(u, v + step, seed) - paper_field(u, v - step, seed)) / (2.0 * step);
    (dx, dy)
}

/// 단위 법선 n = normalize(−β∇h, 1) — 문서 §3.
pub fn paper_normal((gx, gy): (f32, f32), bump: f32) -> [f32; 3] {
    let nx = -bump * gx;
    let ny = -bump * gy;
    let inv = 1.0 / (nx * nx + ny * ny + 1.0).sqrt();
    [nx * inv, ny * inv, inv]
}

/// 방향광 벡터 l(θ, φ) — 문서 §5.
pub fn light_direction(azimuth_deg: f32, elevation_deg: f32) -> [f32; 3] {
    let az = azimuth_deg.to_radians();
    let el = elevation_deg.to_radians();
    [el.cos() * az.cos(), el.cos() * az.sin(), el.sin()]
}

/// 골짜기 차폐 A(h) = 1 − k_ao·max(0, −h) — 문서 §5.
pub fn ambient_occlusion(h: f32, ao_strength: f32) -> f32 {
    1.0 - ao_strength * (-h).max(0.0)
}

/// 채널 반사율 ρ_c(x) = ρ0_c·[1 + a_L·ξ_L + a_C·ξ_c] — 문서 §4.
/// `channel` ∈ {0,1,2} — ξ_c는 채널마다 다른 노이즈(씨앗·위상)를 씁니다.
pub fn albedo(
    rho0: f32,
    u: f32,
    v: f32,
    channel: u32,
    seed: u64,
    s: &PaperSurfaceSettings,
) -> f32 {
    // ξ_L: 채널 공통 휘도 노이즈 (섬유 밀도 → 광흡수).
    let lum = (tile_noise(u + 0.13, v + 0.27, seed ^ 0x5EED_00A1, 7, 7) - 0.5) * 2.0;
    // ξ_c: 채널 독립 색도 노이즈 (충전재 스펙트럼 편차).
    let chr = (tile_noise(
        u + 0.31 + channel as f32 * 0.17,
        v + 0.47 + channel as f32 * 0.23,
        seed ^ 0x00C1_C0DEu64.wrapping_mul(channel as u64 + 1),
        13,
        13,
    ) - 0.5) * 2.0;
    (rho0 * (1.0 + s.albedo_l * lum + s.albedo_c * chr)).clamp(0.0, 1.0)
}

/// 표면 복사휘도 L — 문서 §5. 확산(주변광·차폐 + 직사광) + Blinn-Phong 시닝.
///
/// `flat`은 평평한 표면(β=0, h=0)에서의 노출로, 평면에서 C = ρ가 되도록
/// 정규화합니다 — 설정한 종이 색이 그대로 보이게 하는 실용적 보정입니다.
pub fn radiance(
    rho: f32,
    n: [f32; 3],
    l: [f32; 3],
    view: [f32; 3],
    ao: f32,
    s: &PaperSurfaceSettings,
) -> f32 {
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let ndl = dot(n, l).max(0.0);
    // 하프 벡터 ĥ = normalize(l + v) — Blinn-Phong 시닝.
    let (hx, hy, hz) = (l[0] + view[0], l[1] + view[1], l[2] + view[2]);
    let hlen = (hx * hx + hy * hy + hz * hz).sqrt().max(1e-6);
    let ndh = dot(n, [hx / hlen, hy / hlen, hz / hlen]).max(0.0);
    let flat = s.ambient + s.direct * l[2].max(0.0);
    let exposure = if flat > 1e-4 { 1.0 / flat } else { 1.0 };
    rho * (s.ambient * ao + s.direct * ndl) * exposure + s.sheen * ndh.powf(s.gloss.max(1.0))
}

/// sRGB → 선형광 (문서 §5의 감마 디코딩).
pub fn srgb_to_linear(c: u8) -> f32 {
    let x = c as f32 / 255.0;
    if x <= 0.04045 {
        x / 12.92
    } else {
        ((x + 0.055) / 1.055).powf(2.4)
    }
}

/// 선형광 → sRGB (감마 인코딩).
pub fn linear_to_srgb(x: f32) -> u8 {
    let y = x.clamp(0.0, 1.0);
    let e = if y <= 0.003_130_8 {
        y * 12.92
    } else {
        1.055 * y.powf(1.0 / 2.4) - 0.055
    };
    (e * 255.0 + 0.5) as u8
}

/// 초보자용 5단계 프리셋 (Lowest..Highest) — (전체 강도, 표면 설정).
/// 광원 각도·주변광·직사광·광택은 모든 단계에서 동일하고, 요철·반사율
/// 요동·차폐·시닝만 단계별로 커집니다. 순수 함수.
pub fn paper_texture_preset(level: u8) -> (f32, PaperSurfaceSettings) {
    let light = PaperSurfaceSettings::default();
    let (strength, bump, albedo_l, albedo_c, ao, sheen) = match level.clamp(0, 4) {
        0 => (0.08, 0.12, 0.012, 0.010, 0.08, 0.015),
        1 => (0.16, 0.22, 0.025, 0.018, 0.16, 0.025),
        2 => (0.25, 0.35, 0.040, 0.030, 0.25, 0.040),
        3 => (0.35, 0.60, 0.060, 0.040, 0.40, 0.060),
        _ => (0.50, 0.90, 0.090, 0.060, 0.55, 0.090),
    };
    (
        strength,
        PaperSurfaceSettings {
            bump,
            albedo_l,
            albedo_c,
            ao_strength: ao,
            sheen,
            ..light
        },
    )
}

/// 프리셋 단계 라벨.
pub fn paper_texture_preset_label(level: u8) -> &'static str {
    ["Lowest", "Low", "Medium", "High", "Highest"][level.clamp(0, 4) as usize]
}

/// 종이 표면을 **물리 모델**로 굽습니다 (size×size, RGBA 평탄화, 불투명).
///
/// 픽셀 색 = albedo(ρ0, 채널 노이즈) × 조명(법선·광원·차폐) + 시닝을
/// 선형광 공간에서 계산하고 감마 인코딩해 반환합니다.
/// `strength`(0..1)는 최종 결과를 평평한 ρ0 쪽으로 선형 보간합니다 —
/// 0이면 텍스처가 종이 색 그 자체가 됩니다.
/// **순수 함수**: 같은 입력은 항상 같은 출력 (깜빡임 없음).
pub fn paper_texture_rgba(
    size: usize,
    base_rgb: [u8; 3],
    strength: f32,
    settings: &PaperSurfaceSettings,
    seed: u64,
) -> Vec<u8> {
    let n = size.clamp(8, 512);
    let step = 1.0 / n as f32;
    let s = strength.clamp(0.0, 1.0);
    const VIEW: [f32; 3] = [0.0, 0.0, 1.0];
    let l = light_direction(settings.light_azimuth_deg, settings.light_elevation_deg);
    let rho0 = [
        srgb_to_linear(base_rgb[0]),
        srgb_to_linear(base_rgb[1]),
        srgb_to_linear(base_rgb[2]),
    ];
    (0..n)
        .flat_map(move |y| (0..n).map(move |x| (x, y)))
        .map(move |(x, y)| {
            let u = x as f32 / n as f32;
            let v = y as f32 / n as f32;
            // 순수 파이프라인: 높이 → 기울기 → 법선 → (채널별) 반사율 → 복사휘도.
            let h = paper_field(u, v, seed);
            let ao = ambient_occlusion(h, settings.ao_strength);
            let nrm = paper_normal(paper_gradient(u, v, seed, step), settings.bump);
            let rgb: [u8; 3] = std::array::from_fn(|c| {
                let rho = albedo(rho0[c], u, v, c as u32, seed, settings);
                let lit = radiance(rho, nrm, l, VIEW, ao, settings);
                let lin = rho0[c] + (lit - rho0[c]) * s;
                linear_to_srgb(lin)
            });
            [rgb[0], rgb[1], rgb[2], 255]
        })
        .flatten()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ruled_lines_follow_page_rotation() {
        use crate::text::PageRotation;
        // 0/180°: 가로줄 (y 고정), 90/270°: 세로줄 (x 고정).
        let horiz = paper_lines_rotated(100.0, 200.0, PaperStyle::Ruled, 50.0, PageRotation::None);
        assert_eq!(horiz.len(), 3, "h=200/50 → 3줄");
        assert!(horiz.iter().all(|l| l[1] == l[3] && l[0] == 0.0 && l[2] == 100.0));
        let vert = paper_lines_rotated(
            100.0,
            200.0,
            PaperStyle::Ruled,
            50.0,
            PageRotation::Degrees90,
        );
        assert_eq!(vert.len(), 1, "w=100/50 → 1줄");
        assert!(vert.iter().all(|l| l[0] == l[2] && l[1] == 0.0 && l[3] == 200.0));
        let vert2 = paper_lines_rotated(
            100.0,
            200.0,
            PaperStyle::Ruled,
            50.0,
            PageRotation::Degrees270,
        );
        assert_eq!(vert2, vert);
        // 180°는 가로줄 집합 그대로 (위상 무관).
        let horiz180 = paper_lines_rotated(
            100.0,
            200.0,
            PaperStyle::Ruled,
            50.0,
            PageRotation::Degrees180,
        );
        assert_eq!(horiz180, horiz);
        // Grid/Blank는 회전에 불변 (기존 함수와 동일).
        assert_eq!(
            paper_lines_rotated(100.0, 200.0, PaperStyle::Grid, 50.0, PageRotation::Degrees90),
            paper_lines(100.0, 200.0, PaperStyle::Grid, 50.0)
        );
    }

    #[test]
    fn style_settings_are_independent_per_style() {
        // Ruled/Grid/Dotted 각각 독립 — 한 스타일을 바꿔도 다른 스타일은 그대로.
        let mut s = PaperStyleSettings::default();
        let custom = LineStyle {
            spacing: 60.0,
            color: [200, 10, 10, 255],
            width: 3.0,
        };
        s.set(PaperStyle::Grid, custom);
        assert_eq!(s.of(PaperStyle::Grid), Some(custom));
        assert_eq!(s.of(PaperStyle::Ruled), Some(LineStyle::default()));
        assert_eq!(s.of(PaperStyle::Dotted), Some(LineStyle::default()));
        // Blank는 줄이 없음.
        assert_eq!(s.of(PaperStyle::Blank), None);
        // 직렬화 왕복.
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<PaperStyleSettings>(&json).unwrap(), s);
        // 빈 객체 → 기본값 (이전 세션 호환).
        let d: PaperStyleSettings = serde_json::from_str("{}").unwrap();
        assert_eq!(d, PaperStyleSettings::default());
    }

    #[test]
    fn blank_has_no_lines_or_dots() {
        assert!(paper_lines(595.0, 842.0, PaperStyle::Blank, 24.0).is_empty());
        assert!(paper_dots(595.0, 842.0, PaperStyle::Blank, 24.0).is_empty());
    }

    #[test]
    fn ruled_only_horizontal() {
        let lines = paper_lines(595.0, 100.0, PaperStyle::Ruled, 24.0);
        assert!(!lines.is_empty());
        for l in &lines {
            // 가로줄: y0 == y1, 페이지 폭을 가로지름
            assert!((l[1] - l[3]).abs() < 1e-3);
            assert!((l[0]).abs() < 1e-3 && (l[2] - 595.0).abs() < 1e-3);
        }
    }

    #[test]
    fn grid_has_horizontal_and_vertical() {
        let lines = paper_lines(595.0, 100.0, PaperStyle::Grid, 24.0);
        assert!(!lines.is_empty());
        let horiz = lines.iter().filter(|l| (l[1] - l[3]).abs() < 1e-3).count();
        let vert = lines.iter().filter(|l| (l[0] - l[2]).abs() < 1e-3).count();
        assert!(horiz > 0);
        assert!(vert > 0);
    }

    #[test]
    fn dotted_has_dots_not_lines() {
        assert!(paper_lines(100.0, 100.0, PaperStyle::Dotted, 24.0).is_empty());
        let dots = paper_dots(100.0, 100.0, PaperStyle::Dotted, 24.0);
        assert!(!dots.is_empty());
        for d in &dots {
            assert!(d[0] > 0.0 && d[1] > 0.0);
        }
    }

    #[test]
    fn paper_sizes_are_portrait_and_positive() {
        for size in PaperSize::all() {
            let [w, h] = size.size_pts();
            assert!(w > 0.0 && h > w, "{size:?} should be portrait");
        }
        assert!(PaperSize::A5.size_pts()[0] < PaperSize::A4.size_pts()[0]);
        assert!(PaperSize::A4.size_pts()[0] < PaperSize::A3.size_pts()[0]);
    }

    #[test]
    fn matching_finds_nearest_size() {
        assert_eq!(PaperSize::matching(595.0, 842.0), PaperSize::A4);
        assert_eq!(PaperSize::matching(419.0, 595.0), PaperSize::A5);
        assert_eq!(PaperSize::matching(612.0, 792.0), PaperSize::Letter);
        assert_eq!(PaperSize::matching(612.0, 1008.0), PaperSize::Legal);
    }

    #[test]
    fn page_paper_default_is_blank_white() {
        let p = PagePaper::default();
        assert_eq!(p.style, PaperStyle::Blank);
        assert_eq!(p.color, PAPER_WHITE);
    }

    /// 테스트용 베이크 헬퍼 (문서 §7의 CPU 베이킹과 동일 경로).
    fn bake(size: usize, base: [u8; 3], strength: f32, settings: PaperSurfaceSettings) -> Vec<u8> {
        paper_texture_rgba(size, base, strength, &settings, 7)
    }

    #[test]
    fn texture_is_deterministic() {
        let a = bake(64, [255, 255, 255], 0.35, PaperSurfaceSettings::default());
        let b = bake(64, [255, 255, 255], 0.35, PaperSurfaceSettings::default());
        assert_eq!(a, b, "같은 입력은 항상 같은 질감 (깜빡임 금지)");
        assert_eq!(a.len(), 64 * 64 * 4);
    }

    #[test]
    fn zero_strength_is_plain_paper() {
        // strength 0 → 텍스처가 종이 색 그 자체 (감마 왕복 ±1 이내).
        let out = bake(32, [240, 230, 210], 0.0, PaperSurfaceSettings::default());
        for px in out.chunks_exact(4) {
            assert!((px[0] as i32 - 240).abs() <= 1, "r={}", px[0]);
            assert!((px[1] as i32 - 230).abs() <= 1, "g={}", px[1]);
            assert!((px[2] as i32 - 210).abs() <= 1, "b={}", px[2]);
            assert_eq!(px[3], 255, "불투명");
        }
    }

    #[test]
    fn flat_surface_is_uniform() {
        // β=0, 반사율 요동 0, 차폐 0, 시닝 0 → 평면이므로 모든 픽셀이 ρ0.
        // (노출 정규화 덕분에 평면에서 C = ρ가 정확히 성립해야 함 — 문서 §5)
        let flat = PaperSurfaceSettings {
            bump: 0.0,
            albedo_l: 0.0,
            albedo_c: 0.0,
            ao_strength: 0.0,
            sheen: 0.0,
            ..PaperSurfaceSettings::default()
        };
        let out = bake(32, [250, 240, 230], 1.0, flat);
        for px in out.chunks_exact(4) {
            assert!((px[0] as i32 - 250).abs() <= 1, "r={}", px[0]);
            assert!((px[1] as i32 - 240).abs() <= 1, "g={}", px[1]);
            assert!((px[2] as i32 - 230).abs() <= 1, "b={}", px[2]);
        }
    }

    #[test]
    fn bump_creates_relief() {
        let flat = PaperSurfaceSettings {
            bump: 0.0,
            albedo_l: 0.0,
            albedo_c: 0.0,
            ao_strength: 0.0,
            sheen: 0.0,
            ..PaperSurfaceSettings::default()
        };
        let relief = PaperSurfaceSettings { bump: 1.0, ..flat };
        let a = bake(48, [255, 255, 255], 1.0, flat);
        let b = bake(48, [255, 255, 255], 1.0, relief);
        assert_ne!(a, b, "β>0이면 음영이 생겨 달라야 함");
        // 음영 분산이 실제로 존재해야 입체감이 있음.
        let mut min = 255u8;
        let mut max = 0u8;
        for px in b.chunks_exact(4) {
            min = min.min(px[0]);
            max = max.max(px[0]);
        }
        assert!(max - min > 8, "음영 범위가 너무 좁음: {min}..{max}");
    }

    #[test]
    fn light_rotation_changes_shading() {
        let a = PaperSurfaceSettings {
            light_azimuth_deg: 0.0,
            ..PaperSurfaceSettings::default()
        };
        let b = PaperSurfaceSettings {
            light_azimuth_deg: 180.0,
            ..PaperSurfaceSettings::default()
        };
        let ta = bake(48, [255, 255, 255], 1.0, a);
        let tb = bake(48, [255, 255, 255], 1.0, b);
        assert_ne!(ta, tb, "광원을 반대로 돌리면 음영이 달라져야 함");
    }

    #[test]
    fn texture_depends_on_base_color() {
        let white = bake(32, [255, 255, 255], 0.35, PaperSurfaceSettings::default());
        let cream = bake(32, [251, 243, 220], 0.35, PaperSurfaceSettings::default());
        assert_ne!(white, cream, "종이 배경색에 따라 텍스처가 달라야 함 (요구 ①)");
    }

    #[test]
    fn presets_are_monotonic_and_medium_is_default() {
        // 단계가 올라갈수록 강도·요철이 커져야 합니다.
        let mut prev = paper_texture_preset(0);
        assert_eq!("Lowest", paper_texture_preset_label(0));
        assert_eq!("Highest", paper_texture_preset_label(4));
        for level in 1..5u8 {
            let cur = paper_texture_preset(level);
            assert!(cur.0 > prev.0, "강도 단조 증가: {level}");
            assert!(
                cur.1.bump > prev.1.bump,
                "요철 단조 증가: {level}"
            );
            prev = cur;
        }
        // Medium(2)는 은은한 기본값과 정확히 일치해야 합니다.
        assert_eq!(
            paper_texture_preset(2),
            (0.25, PaperSurfaceSettings::default())
        );
    }

    #[test]
    fn paper_field_is_tileable() {
        // 모든 옥타브가 토러스 격자(셀 수 나머지 연산)를 쓰므로 타일
        // 경계(u/v = 0 ↔ 1)에서 값이 정확히 같아야 합니다 — 이음새 없음.
        let seed = 9;
        for i in 0..20 {
            let v = i as f32 / 19.0;
            let a = paper_field(0.0, v, seed);
            let b = paper_field(1.0, v, seed);
            assert!((a - b).abs() < 1e-5, "u 경계 이음새: {a} vs {b}");
            let u = i as f32 / 19.0;
            let a = paper_field(u, 0.0, seed);
            let b = paper_field(u, 1.0, seed);
            assert!((a - b).abs() < 1e-5, "v 경계 이음새: {a} vs {b}");
        }
    }
}
