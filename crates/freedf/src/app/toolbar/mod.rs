//! 툴바 — 상단 4줄(파일/페이지/도구/검색)과 설정 플로팅 창.
//!
//! 하위 모듈 구성:
//! - [`rows`]: Row1~Row4 내용 (패널 토글, 페이지 그룹, 도구 피커, 검색)
//! - [`settings`]: 설정 창 내용(펜/만년필/휠/캔버스/종이/서버)과 창 렌더

pub(crate) use super::*;
use crate::ui::{check, slider};

/// 일반 펜(볼펜) 물리 모델의 실제 결과를 보여주는 미니 스트로크 미리보기.
fn pen_profile_preview(
    ui: &mut egui::Ui,
    color: Color32,
    width: f32,
    profile: &BallPenProfile,
) {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(152.0, 36.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 4.0, ui.visuals().faint_bg_color);
    let n = 48;
    let x0 = rect.left() + 4.0;
    let x1 = rect.right() - 4.0;
    let cy = rect.center().y;
    let amp = rect.height() * 0.30;
    // 필압 물결 + 속도 변화(느림→빠름)를 재현한 가상 스트로크.
    let mut pts: Vec<StrokePoint> = Vec::with_capacity(n);
    let step = (x1 - x0) / (n - 1) as f32;
    let mut t_ms = 0u64;
    for i in 0..n {
        let t = i as f32 / (n - 1) as f32;
        let x = x0 + (x1 - x0) * t;
        let y = cy + (t * 2.0 * std::f32::consts::PI).sin() * amp;
        let speed = 60.0 + 340.0 * (std::f32::consts::PI * t).sin().powi(2);
        if i > 0 {
            t_ms += (step / speed.max(1.0) * 1000.0) as u64;
        }
        let pressure = 0.3 + 0.7 * (t * 4.0 * std::f32::consts::PI).sin().abs();
        pts.push(StrokePoint::with_time(x, y, pressure, t_ms));
    }
    let widths = profile.widths(width, &pts, 0.0);
    let max_w = widths.iter().cloned().fold(0.5f32, f32::max);
    let scale = (rect.height() * 0.40 / max_w).clamp(0.3, 1.6);
    for i in 0..n - 1 {
        let wpx = (widths[i] + widths[i + 1]) * 0.5 * scale;
        let a = egui::pos2(pts[i].x, pts[i].y);
        let b = egui::pos2(pts[i + 1].x, pts[i + 1].y);
        painter.line_segment([a, b], Stroke::new(wpx, color));
    }
    let _ = resp.on_hover_text(
        "Live preview of the ballpen model:\nwidth = f(pressure, speed) — gentle, narrow range",
    );
}

/// 만년필 물리 모델의 실제 결과를 보여주는 미니 스트로크 미리보기.
/// 느린 시작(굵게) → 빠른 중간(가늘게) → 정지(잉크 고임)를 재현합니다.
fn fountain_profile_preview(
    ui: &mut egui::Ui,
    color: Color32,
    max_width: f32,
    profile: &FountainProfile,
) {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(152.0, 36.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 4.0, ui.visuals().faint_bg_color);
    let n = 48;
    let x0 = rect.left() + 4.0;
    let x1 = rect.right() - 4.0;
    let cy = rect.center().y;
    let amp = rect.height() * 0.30;
    // 가상 스트로크: 양끝 느림(굵게)·중간 빠름(가늘게), 필압 물결.
    let mut pts: Vec<StrokePoint> = Vec::with_capacity(n);
    let step = (x1 - x0) / (n - 1) as f32;
    let mut t_ms = 0u64;
    for i in 0..n {
        let t = i as f32 / (n - 1) as f32;
        let x = x0 + (x1 - x0) * t;
        let y = cy + (t * 2.0 * std::f32::consts::PI).sin() * amp;
        // 속도 프로파일: 양끝 20, 중앙 400 (px/초) → dt = step/speed.
        let speed = 20.0 + 380.0 * (std::f32::consts::PI * t).sin().powi(2);
        if i > 0 {
            t_ms += (step / speed.max(1.0) * 1000.0) as u64;
        }
        let pressure = 0.3 + 0.7 * (t * 4.0 * std::f32::consts::PI).sin().abs();
        pts.push(StrokePoint::with_time(x, y, pressure, t_ms));
    }
    let widths = profile.widths(max_width, &pts, 0.0);
    let max_w = widths.iter().cloned().fold(0.5f32, f32::max);
    let scale = (rect.height() * 0.40 / max_w).clamp(0.3, 1.6);
    for i in 0..n - 1 {
        let wpx = (widths[i] + widths[i + 1]) * 0.5 * scale;
        let a = egui::pos2(pts[i].x, pts[i].y);
        let b = egui::pos2(pts[i + 1].x, pts[i + 1].y);
        painter.line_segment([a, b], Stroke::new(wpx, color));
    }
    let _ = resp.on_hover_text(
        "Live preview of the fountain model:\nwidth = f(pressure × speed × tilt) + dwell blob",
    );
}

/// 잉크 질감(입체적 불균일) 커스텀 컨트롤 — 볼펜/만년필 공용.
/// 변경이 있으면 `true`를 반환합니다 (호출자가 세션 저장).
fn ink_grain_controls(ui: &mut egui::Ui, grain: &mut InkGrain) -> bool {
    let mut changed = check(
        ui,
        &mut grain.enabled,
        "Ink grain",
        "Real ink is never perfectly uniform — enable a subtle, stable \
         texture: flow waves, fiber wicking, start blobs and darker edges.",
    )
    .changed();
    if grain.enabled {
        changed |= slider(
            ui,
            &mut grain.flow_amp,
            0.0..=0.4,
            "Flow",
            "Low-frequency ink-flow waves along the stroke (amplitude).",
        )
        .changed();
        changed |= slider(
            ui,
            &mut grain.wick_amp,
            0.0..=0.4,
            "Wick",
            "Fine fiber-wicking speckle (amplitude) — typically bigger \
             for fountain ink than ballpen ink.",
        )
        .changed();
        changed |= slider(
            ui,
            &mut grain.pooling,
            0.0..=0.6,
            "Pooling",
            "Ink pooling strength: start blob / end bead (ballpen), \
             start & end pools (fountain).",
        )
        .changed();
        changed |= slider(
            ui,
            &mut grain.starvation,
            0.0..=0.6,
            "Starvation",
            "How much fast writing lightens the ink (mainly fountain).",
        )
        .changed();
        changed |= ui
            .add(egui::DragValue::new(&mut grain.seed).range(0..=65535))
            .on_hover_text(
                "Grain seed — reshuffles the texture pattern of new strokes. \
                 Each stroke gets its own texture derived from this seed.",
            )
            .changed();
    }
    changed
}

/// 줄/격자/점 색 프리셋 (RGBA) — GoodNotes 풍 회청 계열. Paper 설정 창에서 선택.
const LINE_COLOR_PRESETS: [[u8; 4]; 4] = [
    [180, 186, 198, 120], // 연회색 (기본)
    [150, 162, 184, 150], // 회청
    [138, 168, 208, 140], // 블루
    [112, 116, 128, 150], // 진회색
];

/// 캔버스(페이지 뒤 서라운드) 색 프리셋 (RGBA) — Canvas 설정 창에서 선택.
const CANVAS_COLOR_PRESETS: [[u8; 4]; 4] = [
    [46, 52, 64, 255],    // Nord (#2E3440 — 기본)
    [17, 17, 27, 255],    // 차콜
    [40, 44, 52, 255],    // 그라파이트
    [224, 228, 236, 255], // 라이트
];

impl FreeDfApp {
    pub(crate) fn toolbar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("toolbar").show(ui, |ui| {
            // Compact spacing + padding; uniform control height for tidy rows
            ui.spacing_mut().button_padding = egui::vec2(8.0, 4.0);
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
            ui.spacing_mut().interact_size = egui::vec2(0.0, 28.0);
            ui.add_space(4.0);
            self.row_top(ui);

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(4.0);

            self.row_pages(ui);

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(4.0);

            self.row_tools(ui);

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(4.0);

            // Row 4: search (only while Ctrl+F is pressed)
            self.search_row(ui);
        });

        self.settings_windows(ui);
    }
}

mod rows;
mod settings;
pub(crate) mod macros;
