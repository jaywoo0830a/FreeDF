//! 주석(스트로크)을 `image` 크레이트 위에 래스터라이즈하고 PNG/JPG/PDF로 저장.
//!
//! 화면 렌더링(egui)과 별개로, 실제 픽셀 이미지에 선을 그려
//! "메모한 페이지 내보내기" 기능을 제공합니다.

use freedf_core::model::{Stroke, StrokePoint, ToolType};
use freedf_core::paper::{
    clamp_line_width, clamp_spacing, paper_dots, paper_lines, PaperStyle,
};
use freedf_core::pen::{BallPenProfile, FountainProfile};
use image::{Rgba, RgbaImage};

/// 스트로크 목록을 `scale`(픽셀/포인트)로 확대해 이미지에 그립니다.
/// `fountain`은 만년필, `pen`은 일반 펜(볼펜) 모델입니다.
/// 내보내기는 **완전히 포화된(진해진) 최종 상태**를 쓴 굵기 그대로 저장합니다.
pub fn draw_strokes_on_image(
    img: &mut RgbaImage,
    strokes: &[Stroke],
    scale: f32,
    fountain: FountainProfile,
    pen: BallPenProfile,
) {
    for stroke in strokes {
        draw_one_stroke(img, stroke, scale, fountain, pen);
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

fn draw_one_stroke(
    img: &mut RgbaImage,
    stroke: &Stroke,
    scale: f32,
    fountain: FountainProfile,
    pen: BallPenProfile,
) {
    let color = stroke.color;
    let pts = &stroke.points;
    if pts.is_empty() {
        return;
    }
    let n = pts.len();
    // 픽셀 공간 점 + 점별 절반 두께 (화면과 동일한 규칙).
    let pts_xy: Vec<[f32; 2]> = pts.iter().map(|p| scale_point(p, scale)).collect();
    let mut halves: Vec<f32> = Vec::with_capacity(n);
    let round_caps = matches!(stroke.tool, ToolType::Pen | ToolType::Fountain);
    if stroke.has_locked_widths() {
        // 입력 시점에 잠금된 폭 그대로 (화면과 동일 — 바닥값 0.05는 퇴화 방지).
        for p in pts {
            halves.push((p.width * scale * 0.5).max(0.05));
        }
    } else if stroke.tool == ToolType::Highlighter {
        // 마커: 필압/테이퍼 없이 일정한 두께.
        halves.resize(n, (stroke.width * scale * 0.5).max(0.5));
    } else if stroke.tool == ToolType::Fountain {
        // 만년필: 필압 × 속도 × 기울기 모델 (기울기는 내보내기에서 0 —
        // 저장된 스트로크에는 기울기 센서 값이 없으므로).
        let widths = fountain.widths(stroke.width, pts, 0.0);
        for w in widths {
            halves.push((w * scale * 0.5).max(0.05));
        }
    } else {
        // 일반 펜: 필압·속도 영향이 작은 물리 모델.
        let widths = pen.widths(stroke.width, pts, 0.0);
        for w in widths {
            halves.push((w * scale * 0.5).max(0.05));
        }
    }
    // 본체: 펜/만년필은 둥근 캡, 마커는 직선(butt) 끝. 내보내기는 완전히
    // 포화된(진해진) 최종 상태를 **쓴 굵기 그대로** 저장합니다 — 잉크 스밈은
    // 색만 진해지고 선 두께는 변하지 않으므로 추가 채움이 필요 없습니다.
    fill_stroke_outline(img, &pts_xy, &halves, round_caps, color);
}

/// 화면과 **동일한** 외곽선→삼각분할로 스트로크를 래스터라이즈합니다.
/// (겹침 없는 삼각형들이라 반투명 색도 균일 — 완전 분할이 안 되는
/// 자기 교차 입력은 세그먼트 quad 폴백으로 커버)
fn fill_stroke_outline(
    img: &mut RgbaImage,
    pts_xy: &[[f32; 2]],
    half_widths: &[f32],
    round_caps: bool,
    color: [u8; 4],
) {
    match freedf_core::pen::stroke_geometry(pts_xy, half_widths, round_caps) {
        freedf_core::pen::StrokeFill::Tris(t) => {
            for tri in &t.tris {
                let a = t.poly[tri[0] as usize];
                let b = t.poly[tri[1] as usize];
                let c = t.poly[tri[2] as usize];
                fill_triangle(img, a, b, c, color);
            }
        }
        freedf_core::pen::StrokeFill::Fallback(fb) => {
            for q in &fb.quads {
                fill_triangle(img, q[0], q[1], q[2], color);
                fill_triangle(img, q[0], q[2], q[3], color);
            }
            for (c, r) in &fb.circles {
                draw_disk(img, *c, r.max(0.5), color);
            }
        }
    }
}

/// 삼각형을 무게중심 좌표 테스트로 채웁니다 (경계 포함, 알파 블렌드).
fn fill_triangle(img: &mut RgbaImage, a: [f32; 2], b: [f32; 2], c: [f32; 2], color: [u8; 4]) {
    let (w, h) = (img.width() as i32, img.height() as i32);
    let x0 = a[0].min(b[0]).min(c[0]).floor() as i32;
    let x1 = a[0].max(b[0]).max(c[0]).ceil() as i32;
    let y0 = a[1].min(b[1]).min(c[1]).floor() as i32;
    let y1 = a[1].max(b[1]).max(c[1]).ceil() as i32;
    let area = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
    if area.abs() < 1e-9 {
        return;
    }
    for y in y0..=y1 {
        for x in x0..=x1 {
            if x < 0 || y < 0 || x >= w || y >= h {
                continue;
            }
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let s = ((b[0] - a[0]) * (py - a[1]) - (b[1] - a[1]) * (px - a[0])) / area;
            let t = ((c[0] - b[0]) * (py - b[1]) - (c[1] - b[1]) * (px - b[0])) / area;
            let u = 1.0 - s - t;
            if s >= -1e-3 && t >= -1e-3 && u >= -1e-3 {
                blend_pixel(img, x as u32, y as u32, color);
            }
        }
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

    /// 직선 펜 스트로크가 "빛살(starburst)처럼 퍼지는" 깨짐 없이
    /// 자기 영역 안에만 그려지는지 검증합니다 (회귀 테스트).
    #[test]
    fn straight_pen_stroke_stays_inside_its_bounds() {
        let mut img = RgbaImage::from_pixel(240, 160, Rgba([255, 255, 255, 255]));
        let pts: Vec<StrokePoint> = (0..24)
            .map(|i| StrokePoint::new(20.0 + i as f32 * 8.0, 80.0, 0.5))
            .collect();
        let stroke = Stroke {
            id: 1,
            tool: ToolType::Pen,
            color: [200, 0, 0, 255],
            width: 3.0,
            points: pts,
            created_ms: 0,
        };
        draw_strokes_on_image(
            &mut img,
            &[stroke],
            1.0,
            FountainProfile::default(),
            BallPenProfile::default(),
        );
        // 스트로크 영역: x 20..204, y 80±1.5(캡 포함) → 여유 8px.
        for y in 0..160u32 {
            for x in 0..240u32 {
                let inside = x >= 12 && x <= 212 && y >= 70 && y <= 90;
                let px = img.get_pixel(x, y);
                if !inside {
                    assert_eq!(
                        *px,
                        Rgba([255, 255, 255, 255]),
                        "스트로크 바깥 픽셀 오염: ({x},{y}) = {px:?}"
                    );
                }
            }
        }
        // 본체는 실제로 그려져야 함.
        assert_ne!(
            *img.get_pixel(100, 80),
            Rgba([255, 255, 255, 255]),
            "스트로크 본체가 그려져야 함"
        );
    }

    /// 하이라이터 직선 밴드도 같은 검증 (반투명 색 포함).
    #[test]
    fn straight_highlighter_stroke_stays_inside_its_bounds() {
        let mut img = RgbaImage::from_pixel(240, 160, Rgba([255, 255, 255, 255]));
        let stroke = Stroke {
            id: 2,
            tool: ToolType::Highlighter,
            color: [250, 200, 0, 90],
            width: 14.0,
            points: vec![StrokePoint::new(20.0, 80.0, 1.0), StrokePoint::new(200.0, 80.0, 1.0)],
            created_ms: 0,
        };
        draw_strokes_on_image(
            &mut img,
            &[stroke],
            1.0,
            FountainProfile::default(),
            BallPenProfile::default(),
        );
        for y in 0..160u32 {
            for x in 0..240u32 {
                let inside = x >= 10 && x <= 210 && y >= 68 && y <= 92;
                let px = img.get_pixel(x, y);
                if !inside {
                    assert_eq!(
                        *px,
                        Rgba([255, 255, 255, 255]),
                        "밴드 바깥 픽셀 오염: ({x},{y}) = {px:?}"
                    );
                }
            }
        }
        assert_ne!(
            *img.get_pixel(100, 80),
            Rgba([255, 255, 255, 255]),
            "밴드 본체가 그려져야 함"
        );
    }

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
