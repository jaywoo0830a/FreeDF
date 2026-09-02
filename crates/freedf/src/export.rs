//! 주석(스트로크)을 `image` 크레이트 위에 래스터라이즈하고 PNG/JPG/PDF로 저장.
//!
//! 화면 렌더링(egui)과 별개로, 실제 픽셀 이미지에 선을 그려
//! "메모한 페이지 내보내기" 기능을 제공합니다.

use freedf_core::model::{Stroke, StrokePoint, ToolType};
use freedf_core::paper::{
    clamp_line_width, clamp_spacing, paper_dots, paper_lines, PaperStyle,
};
use freedf_core::pen::{taper_factors, uses_taper, PressureCurve, TAPER_LEN_PTS};
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
            // 마커: 일정한 두께.
            stroke.width * scale / 2.0
        } else {
            curve.apply(stroke.width, pts[0].pressure) * scale / 2.0
        };
        draw_disk(img, p, r, color);
        return;
    }
    // 화면과 동일: 펜은 양끝 테이퍼 (마커는 일정).
    let tapers: Vec<f32> = if uses_taper(stroke.tool) {
        taper_factors(pts, TAPER_LEN_PTS)
    } else {
        vec![1.0; pts.len()]
    };
    for (i, w) in pts.windows(2).enumerate() {
        let a = scale_point(&w[0], scale);
        let b = scale_point(&w[1], scale);
        let pressure = (w[0].pressure + w[1].pressure) * 0.5;
        let width = if stroke.tool == ToolType::Highlighter {
            // 마커: 기본 동작 — 필압 없이 일정한 두께.
            stroke.width * scale
        } else {
            curve.apply(stroke.width, pressure) * scale
        } * 0.5
            * (tapers[i] + tapers[i + 1]);
        draw_segment(img, a, b, width, color);
    }
}

/// RGBA 이미지를 JPEG 바이트로 인코딩합니다 (품질 1..100).
pub fn encode_jpeg(img: &RgbaImage, quality: u8) -> Result<Vec<u8>, String> {
    let mut rgb = image::RgbImage::new(img.width(), img.height());
    for (x, y, px) in img.enumerate_pixels() {
        rgb.put_pixel(x, y, image::Rgb([px.0[0], px.0[1], px.0[2]]));
    }
    let mut out: Vec<u8> = Vec::new();
    let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality);
    enc.encode_image(&rgb)
        .map_err(|e| format!("JPEG encode failed: {e}"))?;
    Ok(out)
}

/// JPEG 페이지 이미지를 **단일 페이지 PDF**로 감쌉니다 (의존성 없는 최소 PDF).
///
/// - 이미지는 DCTDecode(JPEG) 그대로 임베드되어 재인코딩이 없습니다.
/// - PDF 좌표계는 하단 원점이라 이미지를 세로로 뒤집어 배치합니다.
pub fn jpeg_to_pdf(
    jpeg: &[u8],
    w_px: u32,
    h_px: u32,
    w_pts: f32,
    h_pts: f32,
) -> Vec<u8> {
    let mut pdf: Vec<u8> = Vec::new();
    pdf.extend_from_slice(b"%PDF-1.4\n");
    // 1-based xref offsets (0번은 더미).
    let mut offsets: Vec<usize> = vec![0];

    fn add_obj(
        pdf: &mut Vec<u8>,
        offsets: &mut Vec<usize>,
        header: &[u8],
        stream: Option<&[u8]>,
    ) {
        offsets.push(pdf.len());
        pdf.extend_from_slice(header);
        if let Some(s) = stream {
            pdf.extend_from_slice(b"\nstream\n");
            pdf.extend_from_slice(s);
            pdf.extend_from_slice(b"\nendstream");
        }
        pdf.extend_from_slice(b"\nendobj\n");
    }

    add_obj(
        &mut pdf,
        &mut offsets,
        b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>",
        None,
    );
    add_obj(
        &mut pdf,
        &mut offsets,
        b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        None,
    );
    let page_obj = format!(
        "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w_pts:.2} {h_pts:.2}] \
         /Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>"
    );
    add_obj(&mut pdf, &mut offsets, page_obj.as_bytes(), None);
    let img_obj = format!(
        "4 0 obj\n<< /Type /XObject /Subtype /Image /Width {w_px} /Height {h_px} \
         /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>",
        jpeg.len()
    );
    add_obj(&mut pdf, &mut offsets, img_obj.as_bytes(), Some(jpeg));
    // 세로 뒤집기: 이미지가 top-down이므로 `-H`로 뒤집고 `H`만큼 평행이동.
    let content = format!("q {w_pts:.2} 0 0 -{h_pts:.2} 0 {h_pts:.2} cm /Im0 Do Q");
    let content_obj = format!(
        "5 0 obj\n<< /Length {} >>",
        content.as_bytes().len()
    );
    add_obj(&mut pdf, &mut offsets, content_obj.as_bytes(), Some(content.as_bytes()));

    let xref_pos = pdf.len();
    pdf.extend_from_slice(b"xref\n0 6\n");
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for off in &offsets[1..] {
        pdf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!("trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref_pos}\n%%EOF\n").as_bytes(),
    );
    pdf
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 최소 PDF 래퍼의 구조 검증: xref 오프셋이 실제 객체 위치와 일치해야 합니다.
    #[test]
    fn jpeg_to_pdf_has_valid_structure() {
        fn find_bytes(hay: &[u8], needle: &[u8]) -> Option<usize> {
            hay.windows(needle.len()).position(|w| w == needle)
        }
        let jpeg = b"\xff\xd8\xff\xe0 fake jpeg \xff\xd9";
        let pdf = jpeg_to_pdf(jpeg, 100, 50, 595.0, 842.0);
        assert!(pdf.starts_with(b"%PDF-1.4"));
        assert!(pdf.ends_with(b"%%EOF\n"));
        // xref 오프셋 표의 10자리 숫자들이 실제 객체 위치를 가리키는지.
        let startxref_at = find_bytes(&pdf, b"startxref\n").unwrap();
        let xref_pos = startxref_at + b"startxref\n".len();
        let declared: usize = std::str::from_utf8(&pdf[xref_pos..])
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert_eq!(&pdf[declared..declared + 4], b"xref");
        let table = &pdf[..startxref_at];
        // 각 객체 헤더가 오프셋과 일치.
        for i in 1..=5 {
            let needle = format!("{i} 0 obj\n<<");
            let pos = find_bytes(&pdf, needle.as_bytes()).unwrap();
            assert_eq!(pdf[pos - 1], b'\n', "객체 {i}가 줄 시작에 있어야 함");
            let entry = format!("{pos:010}").into_bytes();
            assert!(
                table.windows(10).any(|w| w == entry),
                "오프셋 표에 {i}번 객체 위치 {pos}가 있어야 함"
            );
        }
        // 이미지와 콘텐츠 스트림 포함 확인.
        assert!(find_bytes(&pdf, b"/Filter /DCTDecode").is_some());
        assert!(find_bytes(&pdf, b"/Im0 Do Q").is_some());
    }

    #[test]
    fn encode_jpeg_produces_jfif() {
        let mut img = image::RgbaImage::new(16, 16);
        for px in img.pixels_mut() {
            *px = image::Rgba([200, 30, 30, 255]);
        }
        let out = encode_jpeg(&img, 85).unwrap();
        assert!(out.starts_with(&[0xFF, 0xD8]), "JPEG SOI");
        assert!(out.ends_with(&[0xFF, 0xD9]), "JPEG EOI");
    }
}
