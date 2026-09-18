//! 도구별 **커스텀 펜 커서** — 캔버스가 시스템 커서를 숨기고 직접 그리는 스프라이트.
//!
//! freedf `app/canvas/paint.rs::paint_custom_cursor` 이식이다. freedf-gui에는
//! 자체 커서 엔진이 없으므로 여기서 소유한다 (egui 의존이 있는 코드라
//! `freedf-canvas`에 둘 수 없다 — 그 크레이트는 egui를 모른다).
//!
//! | 도구 | 스프라이트 |
//! |---|---|
//! | Pen | 은색 금속 볼펜 닙 (볼이 좌표에 고정, 흐르는 반짝임) |
//! | Fountain | 금색 금속 닙 + 뾰족 팁/숨구멍 |
//! | Highlighter | 실제 두께와 같은 반투명 사각형 (왼쪽 모서리 = 좌표) |
//! | Eraser | 가운데가 뚫린 흰 도넛 링 (구멍으로 지워질 내용이 보인다) |
//! | Pan | 작은 이동 십자선 (OS grab 손보다 작다) |
//!
//! 커서 **각도**는 장치가 보고한 틸트 방향이다 — 능력 질의(틸트 미지원
//! 장치)면 손잡이 기반 기본 방위각([`DEFAULT_PEN_AZ`])으로 떨어진다.
//! 표시 여부는 [`hysteresis`]로 3프레임 안정될 때만 바뀐다 (깜빡임 방지).

use eframe::egui::{self, Color32, Pos2, Stroke, Vec2};
use freedf_core::model::ToolType;

/// 틸트 소스가 없을 때의 펜 커서 기본 방위각 (rad) — 오른손잡이 관례(위-오른쪽).
pub const DEFAULT_PEN_AZ: f32 = -0.6;

/// 커서 표시가 뒤집히는 데 필요한 연속 프레임 수 (freedf와 동일한 3프레임).
pub const CURSOR_STABLE_FRAMES: u32 = 3;

/// 손잡이에 따라 방위각을 유효 반평면으로 되돌린다 — 오른손잡이는 배럴이
/// 오른쪽(1·4사분면), 왼손잡이는 왼쪽(2·3사분면)에만 머문다. 반대편이면
/// 세로축 대칭으로 접어 같은 상하 방향의 반대쪽으로 보낸다.
pub fn clamp_azimuth_hand(az: f32, left_handed: bool) -> f32 {
    let ok = if left_handed {
        az.cos() <= 0.0
    } else {
        az.cos() >= 0.0
    };
    if ok {
        az
    } else if az >= 0.0 {
        std::f32::consts::PI - az
    } else {
        -std::f32::consts::PI - az
    }
}

/// 커서 표시 히스테리시스 1프레임 — (새 카운터, 새 표시 상태) 반환.
///
/// 같은 `want`가 `stable_frames` 연속일 때만 표시 상태가 `want`를 따라간다
/// (상태가 프레임마다 뒤집혀도 깜빡임/시스템 커서와의 겹침이 없다).
pub fn hysteresis(
    prev_want: bool,
    want: bool,
    counter: u32,
    shown: bool,
    stable_frames: u32,
) -> (u32, bool) {
    let counter = if want == prev_want {
        (counter + 1).min(stable_frames)
    } else {
        1
    };
    let shown = if counter >= stable_frames { want } else { shown };
    (counter, shown)
}

/// 배럴 기하 — (길이, 절반폭). 눕힐수록 길어지고 원근으로 좁아진다.
pub fn pen_barrel(cos_pitch: f32, scale: f32) -> (f32, f32) {
    let pitch = cos_pitch.clamp(-1.0, 1.0).acos();
    let len = (24.0 + 30.0 * pitch.sin()) * scale;
    let w = ((5.5 * cos_pitch).max(1.8)) * scale;
    (len, w)
}

/// 배럴 방위각 (rad, cos_pitch) — 틸트 지원 장치는 **장치가 보고한 방향**,
/// 미지원/무틸트면 손잡이 기본값. 결과는 손잡이 반평면으로 제한된다.
pub fn pen_azimuth(tilt: Option<(f32, f32)>, left_handed: bool) -> (f32, f32) {
    let (az_raw, cos_pitch) = match tilt {
        Some(t) => t,
        None if left_handed => (-std::f32::consts::PI - DEFAULT_PEN_AZ, 1.0),
        None => (DEFAULT_PEN_AZ, 1.0),
    };
    (clamp_azimuth_hand(az_raw, left_handed), cos_pitch)
}

/// 한 프레임의 커서 입력 — 도구/색/기하/틸트/근접감.
///
/// `tilt`는 `None`이면 "장치가 틸트를 보고하지 않는다"(능력 질의 결과)이고,
/// `Some((az, cos_pitch))`면 장치가 보고한 방향이다.
pub struct CursorSpec {
    pub tool: ToolType,
    /// 하이라이터 미리보기 색 / 펜 잉크 색 (도구별로 해석이 다르다).
    pub color: [u8; 4],
    /// 하이라이터 두께 (pt) — 화면 폭 = `width_pt * zoom`.
    pub width_pt: f32,
    pub zoom: f32,
    /// 지우개 반경 (화면 px).
    pub eraser_radius_px: f32,
    /// 장치 틸트 (방위각 rad, 기울기 코사인) — 미지원 장치는 `None`.
    pub tilt: Option<(f32, f32)>,
    pub left_handed: bool,
    /// 커서 크기 배율 (0.5..2.0) — freedf 설정 `global.cursor_scale` 기본 1.0.
    pub scale: f32,
    /// 근접감 0..1 (`PenInput::proximity`) — 그림자 깊이/거리.
    pub proximity: f32,
}

/// 커서 스프라이트를 그린다 — 도구별 분기는 **여기 하나뿐**이다.
pub fn paint(painter: &egui::Painter, pos: Pos2, time: f32, spec: &CursorSpec) {
    match spec.tool {
        ToolType::Pen => paint_nib(painter, pos, time, spec, false),
        ToolType::Fountain => paint_nib(painter, pos, time, spec, true),
        ToolType::Highlighter => paint_highlighter(painter, pos, spec),
        ToolType::Eraser => paint_eraser(painter, pos, spec),
        ToolType::Pan => paint_pan(painter, pos),
    }
}

/// 금속 닙(볼펜/만년필) — 배럴 정점색 메시 + 2겹 그림자 + 닙 끝.
/// 닙 끝(볼 중심)이 실제 좌표 `pos`에 고정된다.
fn paint_nib(painter: &egui::Painter, pos: Pos2, time: f32, spec: &CursorSpec, fountain: bool) {
    // 금속 기본색 (금/은, f32 채널).
    let (br, bg, bb) = if fountain {
        (0.82f32, 0.60, 0.08)
    } else {
        (0.74, 0.77, 0.80)
    };
    let dark = if fountain {
        Color32::from_rgb(74, 52, 6)
    } else {
        Color32::from_rgb(58, 62, 68)
    };
    let bright = if fountain {
        Color32::from_rgb(255, 240, 160)
    } else {
        Color32::from_rgb(250, 252, 255)
    };
    // 방위각은 장치가 보고한 틸트 방향(능력 질의 → 손잡이 기본값 폴백).
    let (az, cos_pitch) = pen_azimuth(spec.tilt, spec.left_handed);
    let cs = spec.scale;
    let (len, w) = pen_barrel(cos_pitch, cs);
    let dir = Vec2::new(az.cos(), az.sin());
    let perp = Vec2::new(-dir.y, dir.x);
    // 볼펜은 볼 반지름만큼 뒤에서 배럴이 시작 (볼이 좌표에 놓임).
    let ball_r = if fountain { 0.0 } else { 1.2 * cs };
    let tip = pos - dir * ball_r;
    let tail = pos + dir * len;
    let tl = tail + perp * w;
    let tr = tail - perp * w;
    // 금속 채널 → 색 (k = 밝기 배율).
    let mk_col = |k: f32| -> Color32 {
        let c = |v: f32| (v * k * 255.0).clamp(0.0, 255.0) as u8;
        Color32::from_rgb(c(br), c(bg), c(bb))
    };

    // 1) 입체 그림자 2겹 (넓고 옅게 + 좁고 진하게) — **근접감** 반영:
    // 접촉하면 짙고 팁에 가깝게, 호버는 중간, 리포트가 끊기면 옅고 멀어진다.
    let prox = spec.proximity.clamp(0.0, 1.0);
    let sh_scale = 1.3 - 0.55 * prox;
    let (sh1_a, sh2_a) = ((38.0 + 24.0 * prox) as u8, (62.0 + 42.0 * prox) as u8);
    for (sh, alpha) in [
        (Vec2::new(4.0 * sh_scale, 5.0 * sh_scale), sh1_a),
        (Vec2::new(2.0 * sh_scale, 2.5 * sh_scale), sh2_a),
    ] {
        let sc = Color32::from_black_alpha(alpha);
        painter.add(egui::Shape::convex_polygon(
            vec![tip + sh, tr + sh, tl + sh],
            sc,
            Stroke::NONE,
        ));
        painter.circle_filled(tail + sh, w, sc);
        if ball_r > 0.0 {
            painter.circle_filled(pos + sh, ball_r, sc);
        }
    }

    // 2) 배럴 — 링 3개 × 폭 컬럼 5개의 정점색 메시.
    // 폭 방향 = 원통 음영(어두운 쪽 → 밝은 림), 길이 방향 = 흐르는 반짝임 밴드.
    let glint_x = 0.45 + 0.45 * (time * 1.6).sin();
    let pulse = 0.94 + 0.06 * (time * 2.2).sin();
    let mut m = egui::Mesh::default();
    let rings = [0.0f32, 0.55, 1.0];
    let cols = [0.0f32, 0.25, 0.5, 0.75, 1.0];
    for &f in &rings {
        let c = pos + dir * (ball_r + (len - ball_r) * f);
        let rw = w * f.max(0.02);
        for &u in &cols {
            let p = c + perp * (rw * (u * 2.0 - 1.0));
            let shade = 0.42 + 0.88 * u;
            let glint = 1.0 + 1.35 * (-(f - glint_x).powi(2) / 0.018).exp();
            let k = (shade * glint * pulse).clamp(0.0, 2.4);
            m.vertices
                .push(egui::epaint::Vertex::untextured(p, mk_col(k)));
        }
    }
    let cc = cols.len() as u32;
    for ri in 0..rings.len() - 1 {
        for ci in 0..cols.len() - 1 {
            let a = ri as u32 * cc + ci as u32;
            m.indices
                .extend_from_slice(&[a, a + 1, a + cc, a + 1, a + cc + 1, a + cc]);
        }
    }
    painter.add(egui::Shape::mesh(m));
    // 긴 변의 어두운 윤곽 + 꼬리 캡(중간톤 + 밝은 면).
    painter.line_segment([tip, tl], Stroke::new(1.0, dark));
    painter.line_segment([tip, tr], Stroke::new(1.0, dark));
    painter.circle_filled(tail, w, mk_col(0.55 * pulse));
    painter.circle_filled(tail + perp * (w * 0.45), w * 0.5, mk_col(1.5 * pulse));
    painter.circle_stroke(tail, w, Stroke::new(1.2, dark));

    // 3) 닙 끝.
    if fountain {
        // 뾰족한 밝은 금속 팁 + 좌우로 미끄러지는 반짝임 점 + 숨구멍.
        let t_len = 8.0 * cs;
        let t1 = pos + dir * t_len + perp * (2.4 * cs);
        let t2 = pos + dir * t_len - perp * (2.4 * cs);
        painter.add(egui::Shape::convex_polygon(
            vec![pos, t2, t1],
            bright,
            Stroke::new(1.0, dark),
        ));
        painter.circle_stroke(pos + dir * (5.0 * cs), 1.1 * cs, Stroke::new(1.0, dark));
        let gx = (time * 3.0).sin() * (1.2 * cs);
        painter.circle_filled(
            pos + dir * (2.5 * cs) + perp * gx,
            1.1 * cs,
            Color32::WHITE,
        );
    } else {
        // 볼 — 방사형 그라데이션 팬(어두운 림 → 밝은 코어) + 회전 반사점.
        let mut bm = egui::Mesh::default();
        let ring_r = [1.0f32, 0.62, 0.3];
        let ring_k = [0.35f32, 0.8, 1.7];
        for (&rr, &rk) in ring_r.iter().zip(ring_k.iter()) {
            for k in 0..12 {
                let a = std::f32::consts::TAU * (k as f32 / 12.0);
                let p = pos + Vec2::new(a.cos(), a.sin()) * (ball_r * rr);
                bm.vertices
                    .push(egui::epaint::Vertex::untextured(p, mk_col(rk * pulse)));
            }
        }
        let center = bm.vertices.len() as u32;
        bm.vertices
            .push(egui::epaint::Vertex::untextured(pos, mk_col(2.2 * pulse)));
        for ri in 0..2usize {
            for k in 0..12 {
                let k2 = (k + 1) % 12;
                let a0 = (ri * 12 + k) as u32;
                let a1 = (ri * 12 + k2) as u32;
                let b0 = ((ri + 1) * 12 + k) as u32;
                let b1 = ((ri + 1) * 12 + k2) as u32;
                bm.indices.extend_from_slice(&[a0, a1, b0, a1, b1, b0]);
            }
        }
        for k in 0..12 {
            let k2 = (k + 1) % 12;
            bm.indices
                .extend_from_slice(&[(2 * 12 + k) as u32, center, (2 * 12 + k2) as u32]);
        }
        painter.add(egui::Shape::mesh(bm));
        painter.circle_stroke(pos, ball_r, Stroke::new(1.2, dark));
        // 회전하는 흰 반사점 — 구체감.
        let sa = time * 1.4;
        let sp = pos + Vec2::new(sa.cos(), sa.sin()) * (ball_r * 0.38);
        painter.circle_filled(sp, ball_r * 0.22, Color32::WHITE);
    }
}

/// 하이라이터 — 실제 두께와 같은 반투명 사각형 (왼쪽 모서리 = 좌표).
/// 그을 때 실제 선이 여기서 시작하므로 WYSIWYG 미리보기가 된다.
fn paint_highlighter(painter: &egui::Painter, pos: Pos2, spec: &CursorSpec) {
    let [r, g, b, a] = spec.color;
    let color = Color32::from_rgba_unmultiplied(r, g, b, (a as f32 * 0.9) as u8);
    let wpx = (spec.width_pt * spec.zoom).clamp(3.0, 90.0);
    let len = 14.0_f32; // 커서 길이는 짧게(힌트만)
    let half = wpx * 0.5;
    let rect = egui::Rect::from_min_size(pos + Vec2::new(0.0, -half), Vec2::new(len, wpx));
    painter.rect_filled(rect, 0.0, color);
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0, Color32::from_white_alpha(200)),
        egui::StrokeKind::Inside,
    );
}

/// 지우개 — 가운데가 뚫린 흰 도넛 링 (구멍으로 지워질 내용이 그대로 보인다).
fn paint_eraser(painter: &egui::Painter, pos: Pos2, spec: &CursorSpec) {
    let r = spec.eraser_radius_px.max(8.0);
    let hole = r * 0.45; // 도넛 구멍 반지름.
    painter.add(egui::Shape::mesh(donut_ring_mesh(
        pos,
        r,
        hole,
        48,
        Color32::from_white_alpha(70),
    )));
    painter.circle_stroke(pos, r, Stroke::new(2.0, Color32::from_white_alpha(220)));
    painter.circle_stroke(pos, hole, Stroke::new(1.5, Color32::from_white_alpha(200)));
    // 구멍 중심의 작은 점 — 정렬 기준점.
    painter.circle_filled(pos, 2.0, Color32::from_gray(160));
}

/// 팬 — 작고 간결한 이동 십자선 (OS grab 손 커서보다 훨씬 작다).
fn paint_pan(painter: &egui::Painter, pos: Pos2) {
    let c = Color32::from_gray(180);
    let s = 6.0;
    painter.line_segment(
        [pos - Vec2::new(s, 0.0), pos + Vec2::new(s, 0.0)],
        Stroke::new(1.5, c),
    );
    painter.line_segment(
        [pos - Vec2::new(0.0, s), pos + Vec2::new(0.0, s)],
        Stroke::new(1.5, c),
    );
    painter.circle_filled(pos, 2.0, c);
}

/// 중심 `c`, 바깥 반지름 `r`, 구멍 반지름 `hole`의 **도넛(링) 메시**.
/// 바깥 원과 안쪽 구멍 사이를 삼각형 스트립으로 이어 가운데가 빈 링을 채운다
/// (볼록 다각형 팬 대신 직접 인덱스 — 비볼록 링도 안전).
fn donut_ring_mesh(c: Pos2, r: f32, hole: f32, segments: usize, color: Color32) -> egui::Mesh {
    let mut m = egui::Mesh::default();
    for i in 0..=segments {
        let a = std::f32::consts::TAU * (i as f32 / segments as f32);
        let (ca, sa) = (a.cos(), a.sin());
        m.vertices.push(egui::epaint::Vertex::untextured(
            c + Vec2::new(ca * r, sa * r),
            color,
        ));
        m.vertices.push(egui::epaint::Vertex::untextured(
            c + Vec2::new(ca * hole, sa * hole),
            color,
        ));
    }
    for i in 0..segments {
        let a = (i * 2) as u32;
        let b = a + 1;
        let cc = a + 2;
        let d = a + 3;
        m.indices.extend_from_slice(&[a, b, cc, b, d, cc]);
    }
    m
}

