//! 주석(스트로크)을 `image` 크레이트 위에 래스터라이즈하고 PNG로 저장.
//!
//! 화면 렌더링(egui)과 별개로, 실제 픽셀 이미지에 선을 그려
//! "메모한 페이지 내보내기" 기능을 제공합니다.

use freedf_core::model::{Stroke, StrokePoint, ToolType};
use freedf_core::paper::{
    clamp_line_width, clamp_spacing, paper_dots, paper_lines, PaperStyle,
};
use freedf_core::pen::{
    base_width_factor, ink_modifier, taper_factors, uses_own_profile, uses_taper, PressureCurve,
    TAPER_LEN_PTS,
};
use image::{Rgba, RgbaImage};

/// 스트로크 목록을 `scale`(픽셀/포인트)로 확대해 이미지에 그립니다.
pub fn draw_strokes_on_image(img: &mut RgbaImage, strokes: &[Stroke], scale: f32) {
    for stroke in strokes {
        draw_one_stroke(img, stroke, scale);
    }
}

/// 용지 배경 색(틴트)과 그리드/줄/점선을 렌더링된 이미지에 적용합니다.
pub fn draw_paper(
    img: &mut RgbaImage,
    w_pts: f32,
    h_pts: f32,
    scale: f32,
    style: PaperStyle,
    color: [u8; 4],
    spacing: f32,
    line_color: [u8; 4],
    line_width: f32,
) {
    // 배경 색 틴트 (흰색이 아닐 때만)
    if color != [255, 255, 255, 255] {
        let (w, h) = (img.width(), img.height());
        let t = [
            color[0] as f32 / 255.0,
            color[1] as f32 / 255.0,
            color[2] as f32 / 255.0,
        ];
        for y in 0..h {
            for x in 0..w {
                let px = img.get_pixel_mut(x, y);
                px.0[0] = (px.0[0] as f32 * t[0]) as u8;
                px.0[1] = (px.0[1] as f32 * t[1]) as u8;
                px.0[2] = (px.0[2] as f32 * t[2]) as u8;
            }
        }
    }
    // 그리드/줄 선 (색/두께는 페이지별 설정, 두께는 포인트 단위 → scale 곱)
    let spacing = clamp_spacing(spacing);
    let line_w = (clamp_line_width(line_width) * scale).clamp(0.5, 64.0);
    for [x0, y0, x1, y1] in paper_lines(w_pts, h_pts, style, spacing) {
        draw_segment(
            img,
            [x0 * scale, y0 * scale],
            [x1 * scale, y1 * scale],
            line_w,
            line_color,
        );
    }
    // 점선
    let dot_r = (line_w * 0.4).max(0.6);
    for [x, y] in paper_dots(w_pts, h_pts, style, spacing) {
        draw_disk(img, [x * scale, y * scale], dot_r, line_color);
    }
}

fn draw_one_stroke(img: &mut RgbaImage, stroke: &Stroke, scale: f32) {
    let color = stroke.color;
    let curve = PressureCurve::default();
    let pts = &stroke.points;
    if pts.is_empty() {
        return;
    }
    if pts.len() == 1 {
        let p = scale_point(&pts[0], scale);
        let r = if stroke.tool == ToolType::Highlighter {
            // 마커: 일정한 두께
            stroke.width * scale / 2.0
        } else if uses_own_profile(stroke.tool) {
            stroke.width * scale * base_width_factor(stroke.tool)
                * ink_modifier(stroke.tool, pts[0].pressure, 0.0)
                / 2.0
        } else {
            curve.apply(stroke.width, pts[0].pressure)
                * base_width_factor(stroke.tool)
                * scale
                / 2.0
        };
        draw_disk(img, p, r, color);
        return;
    }
    let wfactor = base_width_factor(stroke.tool);
    // 화면과 동일: 펜/만년필은 양끝 테이퍼 (볼펜/마커는 일정).
    let tapers: Vec<f32> = if uses_taper(stroke.tool) {
        taper_factors(pts, TAPER_LEN_PTS)
    } else {
        vec![1.0; pts.len()]
    };
    // 만년필 시작점 잉크 방울 (화면과 동일).
    if stroke.tool == ToolType::Fountain {
        let blob = stroke.width * scale * wfactor * ink_modifier(ToolType::Fountain, 1.0, 0.0) / 2.0;
        draw_disk(img, scale_point(&pts[0], scale), blob, color);
    }
    for (i, w) in pts.windows(2).enumerate() {
        let a = scale_point(&w[0], scale);
        let b = scale_point(&w[1], scale);
        let pressure = (w[0].pressure + w[1].pressure) * 0.5;
        let width = if stroke.tool == ToolType::Highlighter {
            // 마커: 기본 동작 — 필압 없이 일정한 두께.
            stroke.width * scale
        } else if uses_own_profile(stroke.tool) {
            let speed = ((w[1].x - w[0].x).powi(2) + (w[1].y - w[0].y).powi(2)).sqrt();
            stroke.width * wfactor * ink_modifier(stroke.tool, pressure, speed) * scale
        } else {
            curve.apply(stroke.width, pressure) * wfactor * scale
        } * 0.5
            * (tapers[i] + tapers[i + 1]);
        draw_segment(img, a, b, width, color);
    }
}

fn scale_point(p: &StrokePoint, scale: f32) -> [f32; 2] {
    [p.x * scale, p.y * scale]
}

/// 두 점 사이를 두께 `width` 픽셀의 선으로 그립니다.
fn draw_segment(img: &mut RgbaImage, a: [f32; 2], b: [f32; 2], width: f32, color: [u8; 4]) {
    let r = (width * 0.5).max(0.5);
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let dist = (dx * dx + dy * dy).sqrt();
    if dist < 1e-4 {
        draw_disk(img, a, r, color);
        return;
    }
    // 지름의 1/4 간격으로 디스크를 찍어 연결된 굵은 선을 만듭니다.
    let step = (r * 0.5).max(0.75);
    let steps = (dist / step).ceil() as usize;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let x = a[0] + dx * t;
        let y = a[1] + dy * t;
        draw_disk(img, [x, y], r, color);
    }
}

/// 중심 `c`, 반지름 `r` 원을 색으로 채웁니다.
fn draw_disk(img: &mut RgbaImage, c: [f32; 2], r: f32, color: [u8; 4]) {
    let (w, h) = (img.width() as i32, img.height() as i32);
    let x0 = (c[0] - r).floor() as i32;
    let x1 = (c[0] + r).ceil() as i32;
    let y0 = (c[1] - r).floor() as i32;
    let y1 = (c[1] + r).ceil() as i32;
    let r2 = r * r;
    for y in y0..=y1 {
        for x in x0..=x1 {
            if x < 0 || y < 0 || x >= w || y >= h {
                continue;
            }
            let dx = x as f32 - c[0];
            let dy = y as f32 - c[1];
            if dx * dx + dy * dy <= r2 {
                blend_pixel(img, x as u32, y as u32, color);
            }
        }
    }
}

/// 알파 블렌딩으로 픽셀을 덮습니다.
fn blend_pixel(img: &mut RgbaImage, x: u32, y: u32, color: [u8; 4]) {
    let dst = img.get_pixel_mut(x, y);
    let a = color[3] as f32 / 255.0;
    if a >= 1.0 {
        *dst = Rgba(color);
        return;
    }
    let inv = 1.0 - a;
    let src = color;
    dst.0[0] = (src[0] as f32 * a + dst.0[0] as f32 * inv) as u8;
    dst.0[1] = (src[1] as f32 * a + dst.0[1] as f32 * inv) as u8;
    dst.0[2] = (src[2] as f32 * a + dst.0[2] as f32 * inv) as u8;
    dst.0[3] = 255;
}
