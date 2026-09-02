//! 주석(스트로크)을 `image` 크레이트 위에 래스터라이즈하고 PNG로 저장.
//!
//! 화면 렌더링(egui)과 별개로, 실제 픽셀 이미지에 선을 그려
//! "메모한 페이지 내보내기" 기능을 제공합니다.

use freedf_core::model::{Stroke, StrokePoint};
use freedf_core::paper::{clamp_spacing, paper_dots, paper_lines, PaperStyle};
use freedf_core::pen::PressureCurve;
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
    // 그리드/줄 선
    let spacing = clamp_spacing(spacing);
    let line = [120, 120, 140, 100];
    for [x0, y0, x1, y1] in paper_lines(w_pts, h_pts, style, spacing) {
        draw_segment(
            img,
            [x0 * scale, y0 * scale],
            [x1 * scale, y1 * scale],
            2.0,
            line,
        );
    }
    // 점선
    for [x, y] in paper_dots(w_pts, h_pts, style, spacing) {
        draw_disk(img, [x * scale, y * scale], 2.0, line);
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
        let r = curve.apply(stroke.width, pts[0].pressure) * scale / 2.0;
        draw_disk(img, p, r, color);
        return;
    }
    for w in pts.windows(2) {
        let a = scale_point(&w[0], scale);
        let b = scale_point(&w[1], scale);
        let pressure = (w[0].pressure + w[1].pressure) * 0.5;
        let width = curve.apply(stroke.width, pressure) * scale;
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
