//! Three-tier toolbar: panel/page toggles, drawing-tool picker with drag-to-reorder, and per-tool settings.
//!
//! Extracted from `app.rs` during the layered refactor — these are
//! `FreeDfApp` methods living in a submodule (`use super::*` re-exports the
//! app state types and shared helpers).

use super::*;

/// 일반 펜(볼펜) 물리 모델의 실제 결과를 보여주는 미니 스트로크 미리보기.
fn pen_profile_preview(
    ui: &mut egui::Ui,
    color: Color32,
    width: f32,
    profile: &BallPenProfile,
) {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(150.0, 34.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 4.0, ui.visuals().faint_bg_color);
    let n = 48;
    let x0 = rect.left() + 3.0;
    let x1 = rect.right() - 3.0;
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
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(150.0, 34.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 4.0, ui.visuals().faint_bg_color);
    let n = 48;
    let x0 = rect.left() + 3.0;
    let x1 = rect.right() - 3.0;
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
    let mut changed = ui
        .checkbox(&mut grain.enabled, "Ink grain")
        .on_hover_text(
            "Real ink is never perfectly uniform — enable a subtle, stable \
             texture: flow waves, fiber wicking, start blobs and darker edges.",
        )
        .changed();
    if grain.enabled {
        changed |= ui
            .add(egui::Slider::new(&mut grain.flow_amp, 0.0..=0.4).text("Flow"))
            .on_hover_text("Low-frequency ink-flow waves along the stroke (amplitude).")
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut grain.wick_amp, 0.0..=0.4).text("Wick"))
            .on_hover_text(
                "Fine fiber-wicking speckle (amplitude) — typically bigger \
                 for fountain ink than ballpen ink.",
            )
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut grain.pooling, 0.0..=0.6).text("Pooling"))
            .on_hover_text(
                "Ink pooling strength: start blob / end bead (ballpen), \
                 start & end pools (fountain).",
            )
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut grain.starvation, 0.0..=0.6).text("Starvation"))
            .on_hover_text("How much fast writing lightens the ink (mainly fountain).")
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

/// 줄/격자/점 색 프리셋 (RGBA) — Paper 설정 창에서 한 번에 선택.
const LINE_COLOR_PRESETS: [[u8; 4]; 4] = [
    [120, 120, 140, 110], // 회보라 (기본)
    [90, 95, 115, 150],   // 진한 회청
    [100, 130, 170, 140], // 파랑
    [45, 48, 56, 150],    // 진회색
];

impl FreeDfApp {
    /// 볼펜(일반 펜) 세부 설정 — 전용 플로팅 창 내용 (툴바 Settings 버튼으로 열림).
    /// 툴바에는 색/두께/필수 토글만 남기고 나머지는 여기서 지정합니다.
    fn pen_settings_ui(&mut self, ui: &mut egui::Ui) {
        // 미리보기: 실제 볼펜 모델 수식으로 그립니다.
        let preview_color = Color32::from_rgba_unmultiplied(
            self.pen_color[0],
            self.pen_color[1],
            self.pen_color[2],
            self.pen_color[3],
        );
        pen_profile_preview(ui, preview_color, self.pen_width, &self.pen_profile);
        ui.separator();
        egui::CollapsingHeader::new("Physics model")
            .id_salt("pen_win_model")
            .default_open(true)
            .show(ui, |ui| {
                let any_changed = ui
                    .add(
                        egui::Slider::new(&mut self.pen_profile.pressure_k, 0.0..=0.5)
                            .text("Press k"),
                    )
                    .on_hover_text(
                        "Pressure influence (small for ballpens, 0.1~0.3).\n\
                         0 = constant width.",
                    )
                    .changed()
                    | ui
                        .add(
                            egui::Slider::new(&mut self.pen_profile.speed_k, 0.0..=0.3)
                                .text("Speed k"),
                        )
                        .on_hover_text(
                            "Speed influence (small, 0.05~0.15).\n\
                             Fast strokes thin slightly.",
                        )
                        .changed()
                    | ui
                        .add(
                            egui::Slider::new(&mut self.pen_profile.starve_v, 200.0..=3000.0)
                                .text("Starve v"),
                        )
                        .on_hover_text(
                            "Speed (pt/s) where ink starvation starts — above this \
                             the line thins and breaks like a real ballpen.",
                        )
                        .changed()
                    | ui
                        .add(
                            egui::Slider::new(&mut self.pen_profile.tilt_k, 0.0..=1.0)
                                .text("Tilt"),
                        )
                        .on_hover_text(
                            "Tilt influence (continuous, no threshold): laying the pen \
                             down widens the line up to (1 + tilt_k) times.\n\
                             egui/winit don't expose pen tilt — needs the HID/WM_POINTER \
                             hook (`set_pen_tilt`) to feed values.",
                        )
                        .changed();
                if any_changed {
                    self.save_default_session();
                    self.save_session();
                }
            });
        egui::CollapsingHeader::new("Ink soak")
            .id_salt("pen_win_soak")
            .default_open(true)
            .show(ui, |ui| {
                if ui
                    .checkbox(&mut self.pen_soak.enabled, "Ink soak")
                    .on_hover_text(
                        "Ballpen ink soak: after you write, the ink darkens \
                         slightly as it soaks into the paper (subtler than \
                         fountain ink). Thickness never changes.",
                    )
                    .changed()
                {
                    self.save_default_session();
                    self.save_session();
                }
                if self.pen_soak.enabled {
                    let changed = ui
                        .add(
                            egui::Slider::new(&mut self.pen_soak.saturate_sec, 0.5..=5.0)
                                .text("Soak time"),
                        )
                        .on_hover_text(
                            "How long (seconds) fresh ballpen ink takes to \
                             reach its full color.",
                        )
                        .changed()
                        | ui
                            .add(
                                egui::Slider::new(&mut self.pen_soak.initial, 0.1..=0.9)
                                    .text("Initial"),
                            )
                            .on_hover_text(
                                "How light the ink is the moment it touches \
                                 paper (lower = starts paler).",
                            )
                            .changed();
                    if changed {
                        self.save_default_session();
                        self.save_session();
                    }
                }
            });
        egui::CollapsingHeader::new("Ink grain")
            .id_salt("pen_win_grain")
            .default_open(false)
            .show(ui, |ui| {
                if ink_grain_controls(ui, &mut self.pen_grain) {
                    self.save_default_session();
                    self.save_session();
                }
            });
        egui::CollapsingHeader::new("Input & cursor")
            .id_salt("pen_win_input")
            .default_open(false)
            .show(ui, |ui| {
                egui::ComboBox::from_id_salt("pen_cursor_style_win")
                    .selected_text(self.pen_cursor_style.label())
                    .show_ui(ui, |ui| {
                        for style in PenCursorStyle::all() {
                            ui.selectable_value(&mut self.pen_cursor_style, style, style.label());
                        }
                    })
                    .response
                    .on_hover_text("Pen cursor shape");
                if ui
                    .checkbox(&mut self.smoothing_enabled, "Stabilize")
                    .on_hover_text(
                        "Optional tremor filtering (1€ filter).\n\
                         Turn off if your tablet driver (e.g. OpenTabletDriver) \
                         already smooths the input.",
                    )
                    .changed()
                {
                    self.save_default_session();
                    self.save_session();
                }
                if self.smoothing_enabled
                    && ui
                        .add(egui::Slider::new(&mut self.smoothing, 0.0..=1.0).text("Strength"))
                        .on_hover_text(
                            "Filter strength while Stabilize is on.\n\
                             0 = raw input, 1 = silky smooth.",
                        )
                        .changed()
                {
                    self.smoothing = self.smoothing.clamp(0.0, 1.0);
                    self.save_default_session();
                    self.save_session();
                }
                if ui
                    .checkbox(&mut self.mouse_draws, "Mouse ink")
                    .on_hover_text(
                        "Draw ink with the mouse/trackpad too.\n\
                         Off (default): mouse & trackpad pan the page — \
                         only a pen writes, like real note-taking apps.",
                    )
                    .changed()
                {
                    self.save_default_session();
                    self.save_session();
                }
                if ui
                    .checkbox(&mut self.debug_hud, "Debug HUD")
                    .on_hover_text(
                        "Live input overlay: pressure, tilt, tip speed, tip width.\n\
                         Use it to check what your device actually reports.",
                    )
                    .changed()
                {
                    self.save_default_session();
                    self.save_session();
                }
            });
    }

    /// 만년필 세부 설정 — 전용 플로팅 창 내용.
    fn fountain_settings_ui(&mut self, ui: &mut egui::Ui) {
        let preview_color = Color32::from_rgba_unmultiplied(
            self.fountain_color[0],
            self.fountain_color[1],
            self.fountain_color[2],
            self.fountain_color[3],
        );
        fountain_profile_preview(
            ui,
            preview_color,
            self.fountain_width,
            &self.fountain_profile,
        );
        ui.separator();
        egui::CollapsingHeader::new("Physics model")
            .id_salt("fountain_win_model")
            .default_open(true)
            .show(ui, |ui| {
                let any_changed = ui
                    .add(
                        egui::Slider::new(&mut self.fountain_profile.min_width_pt, 0.1..=2.0)
                            .text("Min"),
                    )
                    .on_hover_text("Thinnest line width (pt) when writing fast and light.")
                    .changed()
                    | ui
                        .add(
                            egui::Slider::new(&mut self.fountain_profile.pressure_alpha, 0.3..=2.0)
                                .text("Press α"),
                        )
                        .on_hover_text(
                            "Pressure sensitivity: how strongly pressure widens \
                             the line (0.7~1.2 typical).",
                        )
                        .changed()
                    | ui
                        .add(
                            egui::Slider::new(&mut self.fountain_profile.speed_beta, 0.3..=3.0)
                                .text("Speed β"),
                        )
                        .on_hover_text(
                            "Speed sensitivity: how strongly fast strokes thin \
                             the line (1.0~1.5 typical).",
                        )
                        .changed()
                    | ui
                        .add(
                            egui::Slider::new(&mut self.fountain_profile.speed_ref, 10.0..=200.0)
                                .text("Speed ref"),
                        )
                        .on_hover_text(
                            "Reference speed (pt/s) — at this speed the speed \
                             factor is 0.5. Lower = thinner when writing normally.",
                        )
                        .changed()
                    | ui
                        .add(
                            egui::Slider::new(&mut self.fountain_profile.tilt_k, 0.0..=1.0)
                                .text("Tilt"),
                        )
                        .on_hover_text(
                            "Tilt influence: laying the pen down widens the line.\n\
                             Note: egui/winit don't expose pen tilt yet, so this \
                             is 0 until a HID/WM_POINTER hook feeds set_pen_tilt.",
                        )
                        .changed();
                if ui
                    .checkbox(&mut self.fountain_profile.italic, "Italic nib")
                    .on_hover_text(
                        "Stub/italic nib effect: strokes along the nib axis are \
                         wide, across it are thin — no azimuth sensor needed, \
                         the nib angle is fixed.",
                    )
                    .changed()
                {
                    self.save_default_session();
                    self.save_session();
                }
                let any_changed2 = if self.fountain_profile.italic {
                    ui.add(
                        egui::Slider::new(&mut self.fountain_profile.nib_angle_deg, 0.0..=180.0)
                            .text("Nib angle"),
                    )
                    .on_hover_text("Nib axis direction (degrees).")
                    .changed()
                        | ui
                            .add(
                                egui::Slider::new(&mut self.fountain_profile.italic_k, 0.0..=0.6)
                                    .text("Contrast"),
                            )
                            .on_hover_text("Italic direction contrast (0.2~0.5 looks stub-like).")
                            .changed()
                } else {
                    false
                };
                let dwell_changed = ui
                    .add(
                        egui::Slider::new(&mut self.fountain_profile.dwell_k, 0.0..=0.5)
                            .text("Dwell"),
                    )
                    .on_hover_text(
                        "Ink pooling when the pen nearly stops — the classic \
                         fountain-pen blob at the end of a stroke.",
                    )
                    .changed();
                if any_changed || any_changed2 || dwell_changed {
                    self.save_default_session();
                    self.save_session();
                }
            });
        egui::CollapsingHeader::new("Ink soak")
            .id_salt("fountain_win_soak")
            .default_open(true)
            .show(ui, |ui| {
                if ui
                    .checkbox(&mut self.fountain_soak.enabled, "Ink soak")
                    .on_hover_text(
                        "Fountain ink soak: after you write, the ink gradually \
                         darkens as it soaks into the paper. Line thickness \
                         stays exactly as drawn — only the color deepens.",
                    )
                    .changed()
                {
                    self.save_default_session();
                    self.save_session();
                }
                if self.fountain_soak.enabled {
                    let changed = ui
                        .add(
                            egui::Slider::new(&mut self.fountain_soak.saturate_sec, 0.5..=5.0)
                                .text("Soak time"),
                        )
                        .on_hover_text(
                            "How long (seconds) fresh ink takes to deepen \
                             from its light state to the full ink color.",
                        )
                        .changed()
                        | ui
                            .add(
                                egui::Slider::new(&mut self.fountain_soak.initial, 0.1..=0.9)
                                    .text("Initial"),
                            )
                            .on_hover_text(
                                "How light the ink is the moment it touches \
                                 paper (lower = starts paler).",
                            )
                            .changed();
                    if changed {
                        self.save_default_session();
                        self.save_session();
                    }
                }
            });
        egui::CollapsingHeader::new("Ink grain")
            .id_salt("fountain_win_grain")
            .default_open(false)
            .show(ui, |ui| {
                if ink_grain_controls(ui, &mut self.fountain_grain) {
                    self.save_default_session();
                    self.save_session();
                }
            });
    }

    /// Paper 세부 설정 — 전용 플로팅 창 내용 (툴바 Paper 옆 Settings 버튼으로 열림).
    ///
    /// 적용 규칙 (명확하게):
    /// - 스타일/색 선택: **현재 페이지에 즉시 적용** + 앞으로 만드는
    ///   페이지의 기본값이 됩니다.
    /// - 스타일별 세부설정(간격/색/두께): **스타일 프리셋** — 현재 선택된
    ///   스타일의 값을 편집하며, 그 스타일을 쓰는 **모든 페이지**가 함께 바뀝니다.
    /// - 대량 적용: 아래 Apply 섹션에서 모든 페이지/범위를 명시적으로 선택.
    fn paper_settings_ui(&mut self, ui: &mut egui::Ui) {
        const MM_TO_PT: f32 = 72.0 / 25.4;
        let page_count = self.document.as_ref().map(|d| d.page_count()).unwrap_or(0);
        ui.label(
            egui::RichText::new(
                "Style & color apply to the current page right away\n\
                 and become the default for new pages.",
            )
            .weak()
            .small(),
        );
        egui::CollapsingHeader::new("Paper")
            .id_salt("paper_win_paper")
            .default_open(true)
            .show(ui, |ui| {
                egui::ComboBox::from_id_salt("paper_style_win")
                    .selected_text(self.paper_style.label())
                    .show_ui(ui, |ui| {
                        for style in PaperStyle::all() {
                            let changed = ui
                                .selectable_value(&mut self.paper_style, style, style.label())
                                .changed();
                            if changed {
                                self.apply_paper_to_current_page();
                                self.save_default_session();
                                self.save_session();
                            }
                        }
                    })
                    .response
                    .on_hover_text(
                        "Paper style for the current page.\n\
                         New pages & new notes use it as their default.",
                    );
                for (i, paper) in PAPER_COLORS.iter().enumerate() {
                    let color =
                        Color32::from_rgba_unmultiplied(paper[0], paper[1], paper[2], paper[3]);
                    let selected = self.paper_color == *paper;
                    if color_circle_swatch(ui, ("paper_swatch_win", i), color, selected)
                        .on_hover_text("Paper color (current page)")
                        .clicked()
                    {
                        self.paper_color = *paper;
                        self.apply_paper_to_current_page();
                        self.save_default_session();
                        self.save_session();
                    }
                }
                // 프리셋 5색 외에 원하는 배경색을 직접 고릅니다.
                let mut paper_color = Color32::from_rgba_unmultiplied(
                    self.paper_color[0],
                    self.paper_color[1],
                    self.paper_color[2],
                    self.paper_color[3],
                );
                if ui
                    .color_edit_button_srgba(&mut paper_color)
                    .on_hover_text("Custom paper color (current page)")
                    .changed()
                {
                    self.paper_color = paper_color.to_array();
                    self.apply_paper_to_current_page();
                    self.save_default_session();
                    self.save_session();
                }
            });
        // ── 스타일별 독립 세부설정 — **현재 선택된 스타일**을 편집합니다.
        // Ruled/Grid/Dotted 각각 자기만의 간격/색/두께를 가집니다.
        egui::CollapsingHeader::new("Style details")
            .id_salt("paper_win_style")
            .default_open(true)
            .show(ui, |ui| {
                match self.paper_style {
                    PaperStyle::Blank => {
                        ui.label(
                            egui::RichText::new("Blank paper has no lines — nothing to configure.")
                                .weak(),
                        );
                    }
                    PaperStyle::Ruled | PaperStyle::Grid | PaperStyle::Dotted => {
                        let style = self.paper_style;
                        ui.label(
                            egui::RichText::new(format!(
                                "Editing the '{}' style — every page using it updates together.",
                                style.label()
                            ))
                            .weak()
                            .small(),
                        );
                        // 값 복사본으로 편집 → 변경 시 프리셋에 다시 기록.
                        let mut ls = self.paper_style_settings.of(style).unwrap();
                        let mut changed = false;
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut ls.spacing)
                                    .range(12.0..=120.0)
                                    .speed(1.0)
                                    .prefix("Spacing ")
                                    .suffix("pt"),
                            )
                            .on_hover_text("Line / dot spacing (pt)")
                            .changed();
                        changed |= ui
                            .add(
                                egui::DragValue::new(&mut ls.width)
                                    .range(0.25..=8.0)
                                    .speed(0.05)
                                    .fixed_decimals(2)
                                    .prefix("Line ")
                                    .suffix("pt"),
                            )
                            .on_hover_text("Line / dot thickness (pt)")
                            .changed();
                        // 줄 색: 프리셋 스와치 + 커스텀 컬러.
                        for (i, preset) in LINE_COLOR_PRESETS.iter().enumerate() {
                            let col = Color32::from_rgba_unmultiplied(
                                preset[0],
                                preset[1],
                                preset[2],
                                preset[3],
                            );
                            let selected = ls.color == *preset;
                            if color_circle_swatch(ui, ("line_swatch_win", i), col, selected)
                                .on_hover_text("Line color preset")
                                .clicked()
                            {
                                ls.color = *preset;
                                changed = true;
                            }
                        }
                        let mut line_color = Color32::from_rgba_unmultiplied(
                            ls.color[0],
                            ls.color[1],
                            ls.color[2],
                            ls.color[3],
                        );
                        if ui
                            .color_edit_button_srgba(&mut line_color)
                            .on_hover_text("Custom line color")
                            .changed()
                        {
                            ls.color =
                                [line_color.r(), line_color.g(), line_color.b(), line_color.a()];
                            changed = true;
                        }
                        if changed {
                            ls.spacing = clamp_spacing(ls.spacing);
                            ls.width = clamp_line_width(ls.width);
                            self.paper_style_settings.set(style, ls);
                            self.save_default_session();
                            self.save_session();
                            // 프리셋 변경 → 그 스타일을 쓰는 페이지 전부 다시 그리기.
                            self.render_dirty = true;
                        }
                    }
                }
            });
        egui::CollapsingHeader::new("Page size")
            .id_salt("paper_win_size")
            .default_open(true)
            .show(ui, |ui| {
                egui::ComboBox::from_id_salt("paper_size_win")
                    .selected_text(self.paper_size.label())
                    .show_ui(ui, |ui| {
                        for size in PaperSize::all() {
                            let changed = ui
                                .selectable_value(&mut self.paper_size, size, size.label())
                                .changed();
                            if changed {
                                self.save_default_session();
                                self.save_session();
                                let pts = self.new_page_size_pts();
                                self.status = Some(format!(
                                    "New pages & notes will use {} ({:.0} × {:.0} pt) — insert a page at the very back to verify",
                                    size.label(),
                                    pts[0],
                                    pts[1]
                                ));
                            }
                        }
                    })
                    .response
                    .on_hover_text(
                        "Size of new pages & new notes (existing pages keep their size).",
                    );
                let pts = self.new_page_size_pts();
                ui.label(
                    egui::RichText::new(format!(
                        "{} — {:.0} × {:.0} pt\n\
                         Applies to NEW pages & notes only; existing pages\n\
                         keep their physical size.",
                        self.paper_size.label(),
                        pts[0],
                        pts[1]
                    ))
                    .weak()
                    .small(),
                );
                // 사용자 정의 크기: mm 단위 숫자 입력.
                if self.paper_size == PaperSize::Custom {
                    let mut w_mm = self.custom_paper_size[0] / MM_TO_PT;
                    let mut h_mm = self.custom_paper_size[1] / MM_TO_PT;
                    let w_changed = ui
                        .add(
                            egui::DragValue::new(&mut w_mm)
                                .range(50.0..=1200.0)
                                .speed(1.0)
                                .prefix("W "),
                        )
                        .on_hover_text("Custom page width (mm)")
                        .changed();
                    let h_changed = ui
                        .add(
                            egui::DragValue::new(&mut h_mm)
                                .range(50.0..=1200.0)
                                .speed(1.0)
                                .prefix("H "),
                        )
                        .on_hover_text("Custom page height (mm)")
                        .changed();
                    if w_changed || h_changed {
                        self.custom_paper_size = [
                            (w_mm * MM_TO_PT).clamp(100.0, 3400.0),
                            (h_mm * MM_TO_PT).clamp(100.0, 3400.0),
                        ];
                        self.save_default_session();
                        self.save_session();
                        self.status = Some(format!(
                            "New pages & notes will use {:.0} × {:.0} pt",
                            self.custom_paper_size[0],
                            self.custom_paper_size[1]
                        ));
                    }
                }
            });
        // ── 대량 적용 대상 (명시적) ──
        ui.separator();
        ui.label(egui::RichText::new("Apply selection to…").strong());
        if ui
            .add_enabled(
                page_count > 0,
                egui::Button::new(icon_text(ui, "All pages", icons::CHECK_SQUARE_OFFSET)),
            )
            .on_hover_text("Set every page to the current style & color.")
            .clicked()
        {
            self.apply_paper_to_all_pages();
        }
        ui.horizontal(|ui| {
            ui.label("Range:");
            let (mut from, mut to) = (
                (self.paper_range_from + 1).max(1),
                (self.paper_range_to + 1).max(1),
            );
            let changed_from = ui
                .add(egui::DragValue::new(&mut from).range(1..=page_count.max(1)))
                .changed();
            ui.label("–");
            let changed_to = ui
                .add(egui::DragValue::new(&mut to).range(1..=page_count.max(1)))
                .changed();
            if changed_from || changed_to {
                self.paper_range_from = from.saturating_sub(1);
                self.paper_range_to = to.saturating_sub(1);
            }
            if ui
                .add_enabled(page_count > 0, egui::Button::new("Apply"))
                .on_hover_text(
                    "Set pages in this range (inclusive) to the current style & color.",
                )
                .clicked()
            {
                self.apply_paper_to_range(self.paper_range_from, self.paper_range_to);
            }
        });
    }

    /// 미디어 서버 연결 설정 창 내용 (툴바 Server 버튼으로 열림).
    ///
    /// 설정은 `server.json`에 저장되고 다음 실행에서 로드됩니다 — 서버 주소는
    /// 빌드타임이 아니라 **런타임 입력**입니다.
    fn server_settings_ui(&mut self, ui: &mut egui::Ui) {
        // ── Database (런타임 입력 — 하드코딩 없음) ──
        ui.label(egui::RichText::new("Database (PostgreSQL 18.6)").strong());
        ui.horizontal(|ui| {
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.connect_url)
                    .hint_text("postgres://freedf:<password>@<host>:5432/freedf")
                    .desired_width(210.0),
            );
            if ui
                .button(if self.db_connected { "Reconnect" } else { "Connect" })
                .clicked()
            {
                self.try_connect_db();
            }
            if self.db_connected
                && ui
                    .button("Disconnect")
                    .on_hover_text(
                        "Close open documents and switch to offline mode — \n\
                         then enter a different database URL.",
                    )
                    .clicked()
            {
                self.pending_connect = None; // 진행 중이던 시도 폐기.
                self.close_all_documents();
                self.db = crate::storage::disconnected();
                self.db_connected = false;
                self.setup_open = true;
                self.connect_status =
                    Some((false, "Disconnected — enter a new URL and Connect.".into()));
            }
            if resp.lost_focus() && ui.ctx().input(|i| i.key_pressed(egui::Key::Enter)) {
                self.try_connect_db();
            }
        });
        if self.pending_connect.is_some() {
            ui.label(egui::RichText::new("Connecting…").weak());
        } else if let Some((ok, msg)) = &self.connect_status {
            let color = if *ok {
                ui.visuals().hyperlink_color
            } else {
                ui.visuals().error_fg_color
            };
            ui.colored_label(color, msg);
        } else if self.db_connected {
            ui.label(egui::RichText::new("Connected.").weak());
        }
        ui.add_space(6.0);
        ui.separator();

        ui.label(
            "Self-hosted media server for audio recordings.\n\
             Playback streams straight from nginx; this key only guards\n\
             uploads, lists and deletes.",
        );
        ui.add_space(4.0);
        let mut changed = false;
        changed |= ui
            .checkbox(&mut self.media_config.enabled, "Connect to media server")
            .on_hover_text("Leave off until your VPS server is deployed.")
            .changed();
        ui.horizontal(|ui| {
            ui.label("Server URL");
            changed |= ui
                .add(
                    egui::TextEdit::singleline(&mut self.media_config.base_url)
                        .hint_text("https://media.example.com")
                        .desired_width(230.0),
                )
                .changed();
        });
        ui.horizontal(|ui| {
            ui.label("API key");
            changed |= ui
                .add(
                    egui::TextEdit::singleline(&mut self.media_config.api_key)
                        .password(true)
                        .desired_width(230.0),
                )
                .changed();
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    self.media_config.enabled,
                    egui::Button::new(icon_text(ui, "Test connection", icons::PLUG)),
                )
                .on_hover_text("GET /health on the server (4s timeout)")
                .clicked()
            {
                let client = MediaClient::new(&self.media_config);
                let start = std::time::Instant::now();
                self.server_msg = Some(match client.health() {
                    Ok(()) => (
                        true,
                        format!("Connected — {} ms", start.elapsed().as_millis()),
                    ),
                    Err(e) => (false, e),
                });
            }
            if ui.button("Save").clicked() {
                let path = MediaServerConfig::config_path();
                self.server_msg = Some(match self.media_config.save(&path) {
                    Ok(()) => (true, format!("Saved to {}", path.display())),
                    Err(e) => (false, format!("Save failed: {e}")),
                });
            }
        });
        if changed {
            // 값이 바뀌면 이전 테스트 결과는 더 이상 유효하지 않음.
            self.server_msg = None;
        }
        if let Some((ok, msg)) = &self.server_msg {
            let color = if *ok {
                ui.visuals().hyperlink_color
            } else {
                ui.visuals().error_fg_color
            };
            ui.colored_label(color, msg);
        }
    }

    pub(crate) fn toolbar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("toolbar").show(ui, |ui| {
            // Compact spacing + padding; uniform control height for tidy rows
            ui.spacing_mut().button_padding = egui::vec2(9.0, 5.0);
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);
            ui.spacing_mut().interact_size = egui::vec2(0.0, 28.0);
            ui.add_space(4.0);
            // Row 1: panels / page tools / ink tools
            toolbar_row(ui, |ui| {
                ui.horizontal(|ui| {
                if ui
                    .toggle_value(&mut self.show_library, icon_text(ui, "Library", icons::NOTEBOOK))
                    .on_hover_text("Library (notes, PDFs, recents)")
                    .changed()
                {
                    // Zoom is preserved; the canvas re-centers on resize.
                    self.save_session();
                }
                if ui
                    .toggle_value(&mut self.show_outline, icon_text(ui, "Outline", icons::LIST_BULLETS))
                    .on_hover_text("Outline")
                    .changed()
                {
                    // Zoom is preserved; the canvas re-centers on resize.
                    self.save_session();
                }
                if ui
                    .toggle_value(&mut self.show_palette, icon_text(ui, "Palette", icons::PALETTE))
                    .on_hover_text("Writing-tool color palette (right side of canvas)")
                    .changed()
                {
                    self.save_default_session();
                }
                if ui
                    .toggle_value(
                        &mut self.dictionary.enabled,
                        icon_text(ui, "Dictionary", icons::BOOK_OPEN_TEXT),
                    )
                    .on_hover_text(
                        "Tap any word on the page to look it up in the dictionary.\n\
                         Needs internet once per word; results are cached in the database.",
                    )
                    .changed()
                {
                    self.save_default_session();
                }
                if ui
                    .toggle_value(
                        &mut self.server_settings_open,
                        icon_text(ui, "Server", icons::CLOUD),
                    )
                    .on_hover_text(
                        "Media server connection settings — upload & play audio recordings\n\
                         from your self-hosted VPS.",
                    )
                    .changed()
                {
                    self.server_msg = None;
                }
                if ui
                    .toggle_value(&mut self.show_media, icon_text(ui, "Media", icons::MICROPHONE))
                    .on_hover_text("Recordings for this document — upload / play / delete")
                    .changed()
                {
                    // 열릴 때 목록 갱신.
                    self.media_refresh();
                }
                ui.separator();

                // Bookmark the current page + jump list.
                let bookmarked = self.store.is_bookmarked(self.current_page);
                if ui
                    .selectable_label(
                        bookmarked,
                        icon_text(ui, "Bookmark", icons::BOOKMARK_SIMPLE),
                    )
                    .on_hover_text(if bookmarked {
                        "Remove bookmark from this page"
                    } else {
                        "Bookmark this page"
                    })
                    .clicked()
                {
                    self.toggle_bookmark(self.current_page);
                }
                ui.menu_button(icon_text(ui, "Bookmarks", icons::BOOKMARKS_SIMPLE), |ui| {
                    let pages: Vec<PageIndex> = self.store.bookmarks().to_vec();
                    if pages.is_empty() {
                        ui.label("No bookmarks yet");
                        return;
                    }
                    for p in pages {
                        if ui.button(format!("Page {}", p + 1)).clicked() {
                            ui.close();
                            self.goto_page(p);
                        }
                    }
                    ui.separator();
                    if ui.button("Clear all bookmarks").clicked() {
                        self.clear_bookmarks();
                    }
                });
                ui.separator();

                if !self.show_library && !self.show_outline {
                    // With the side panels collapsed the canvas is wide, so let
                    // the page be aligned left / center / right.
                    ui.separator();
                    let aligns = [
                        (PageAlign::Left, icons::TEXT_ALIGN_LEFT, "Align left"),
                        (PageAlign::Center, icons::TEXT_ALIGN_CENTER, "Align center"),
                        (PageAlign::Right, icons::TEXT_ALIGN_RIGHT, "Align right"),
                    ];
                    for (a, ic, hint) in aligns {
                        if ui
                            .selectable_label(self.page_align == a, icon_text(ui, "", ic))
                            .on_hover_text(hint)
                            .clicked()
                        {
                            self.page_align = a;
                            self.realign();
                            self.save_session();
                        }
                    }
                }
                ui.separator();

                if ui
                    .add_enabled(
                        self.history.can_undo(),
                        egui::Button::new(icon_text(ui, "Undo", icons::ARROW_COUNTER_CLOCKWISE)),
                    )
                    .on_hover_text("Undo (Ctrl+Z)")
                    .clicked()
                {
                    self.undo();
                }
                if ui
                    .add_enabled(
                        self.history.can_redo(),
                        egui::Button::new(icon_text(ui, "Redo", icons::ARROW_CLOCKWISE)),
                    )
                    .on_hover_text("Redo (Ctrl+Y)")
                    .clicked()
                {
                    self.redo();
                }
                if ui
                    .button(icon_text(ui, "Clear", icons::X_CIRCLE))
                    .on_hover_text("Clear page")
                    .clicked()
                {
                    self.clear_page();
                }
                ui.separator();

                if ui
                    .button(icon_text(ui, "Save", icons::FLOPPY_DISK))
                    .on_hover_text("Save annotations (Ctrl+S)")
                    .clicked()
                {
                    self.save_annotations();
                }
                if ui
                    .button(icon_text(ui, "Load", icons::FOLDER_SIMPLE))
                    .on_hover_text("Load annotations")
                    .clicked()
                {
                    self.load_annotations();
                }
                });
            });

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(2.0);

            // Row 2: Page (structure + paper styling)
            toolbar_row(ui, |ui| {
                ui.horizontal(|ui| {
                ui.label(icon_text(ui, "Page", icons::FILES));
                let page_count = self.document.as_ref().map(|d| d.page_count()).unwrap_or(0);
                ui.menu_button(icon_text(ui, "Insert Page", icons::PLUS_SQUARE), |ui| {
                    let insert = [
                        (InsertTarget::FromCurrent, "From current page (copies size & paper)"),
                        (InsertTarget::AtVeryFront, "At the very front"),
                        (InsertTarget::AtVeryBack, "At the very back"),
                        (InsertTarget::BeforeCurrent, "Before current page"),
                        (InsertTarget::AfterCurrent, "After current page"),
                    ];
                    for (target, label) in insert {
                        if ui
                            .add_enabled(page_count > 0, egui::Button::new(label))
                            .clicked()
                        {
                            ui.close();
                            self.insert_page_action(target);
                        }
                    }
                })
                .response
                .on_hover_text("Insert a blank page");
                ui.menu_button(icon_text(ui, "Rotate", icons::REPEAT), |ui| {
                    if ui
                        .add_enabled(page_count > 0, egui::Button::new("Rotate current page CW"))
                        .clicked()
                    {
                        ui.close();
                        self.rotate_page_action(true);
                    }
                    if ui
                        .add_enabled(
                            page_count > 0,
                            egui::Button::new("Rotate current page CCW"),
                        )
                        .clicked()
                    {
                        ui.close();
                        self.rotate_page_action(false);
                    }
                    if ui
                        .add_enabled(page_count > 0, egui::Button::new("Rotate all pages CW"))
                        .clicked()
                    {
                        ui.close();
                        self.rotate_all_pages_action(true);
                    }
                    if ui
                        .add_enabled(page_count > 0, egui::Button::new("Rotate all pages CCW"))
                        .clicked()
                    {
                        ui.close();
                        self.rotate_all_pages_action(false);
                    }
                })
                .response
                .on_hover_text("Rotate pages (CW = clockwise)");
                if ui
                    .add_enabled(
                        page_count > 1,
                        egui::Button::new(icon_text(ui, "Delete", icons::TRASH_SIMPLE)),
                    )
                    .on_hover_text("Delete this page")
                    .clicked()
                {
                    self.delete_page_action();
                }
                ui.separator();

                // Paper (grid / ruling / color) — applied to the **current
                // page**; new pages use these values as their defaults.
                // "Apply to all" pushes the current values onto every page.
                ui.label(icon_text(ui, "Paper", icons::NOTEBOOK));
                egui::ComboBox::from_id_salt("paper_style")
                    .selected_text(self.paper_style.label())
                    .show_ui(ui, |ui| {
                        for style in PaperStyle::all() {
                            let changed = ui
                                .selectable_value(&mut self.paper_style, style, style.label())
                                .changed();
                            if changed {
                                self.apply_paper_to_current_page();
                                self.save_default_session();
                                self.save_session();
                            }
                        }
                    })
                    .response
                    .on_hover_text(
                        "Paper style for the current page.\n\
                         New pages & new notes use it as their default.",
                    );
                for (i, paper) in PAPER_COLORS.iter().enumerate() {
                    let color =
                        Color32::from_rgba_unmultiplied(paper[0], paper[1], paper[2], paper[3]);
                    let selected = self.paper_color == *paper;
                    if color_circle_swatch(ui, ("paper_swatch", i), color, selected)
                        .on_hover_text("Paper color (current page)")
                        .clicked()
                    {
                        self.paper_color = *paper;
                        self.apply_paper_to_current_page();
                        self.save_default_session();
                        self.save_session();
                    }
                }
                // 프리셋 5색 외에 원하는 배경색을 직접 고릅니다.
                let mut paper_color = Color32::from_rgba_unmultiplied(
                    self.paper_color[0],
                    self.paper_color[1],
                    self.paper_color[2],
                    self.paper_color[3],
                );
                if ui
                    .color_edit_button_srgba(&mut paper_color)
                    .on_hover_text("Custom paper color (current page)")
                    .changed()
                {
                    self.paper_color = paper_color.to_array();
                    self.apply_paper_to_current_page();
                    self.save_default_session();
                    self.save_session();
                }
                // 세부 설정 전용 창 트리거 — 크기/간격/줄 색·두께/전체 적용.
                if ui
                    .add(egui::Button::new(icon_text(ui, "Settings", icons::GEAR)))
                    .on_hover_text(
                        "Open the paper settings window:\n\
                         page size, grid spacing, line color & thickness, apply to all.",
                    )
                    .clicked()
                {
                    self.paper_settings_open = true;
                }
                });
            });

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(2.0);

            // Row 3: drawing tools (drag to reorder) + settings
            toolbar_row(ui, |ui| {
                ui.horizontal(|ui| {
                // ── 도구 선택기 (드래그 앤 드롭 재정렬) ─────────────
                let mut order = self.tool_order.clone();
                let mut rects: Vec<egui::Rect> = Vec::with_capacity(order.len());
                let mut src = self.tool_drag;
                let mut dst = self.tool_drop;
                for (i, tool) in order.iter().copied().enumerate() {
                    let label = tool.label();
                    let selected = self.tool == tool;
                    let btn = egui::Button::new(icon_text(ui, "", tool_icon(tool))).selected(selected);
                    let resp = ui
                        .add(btn.sense(egui::Sense::click_and_drag()))
                        .on_hover_text(format!("{label}  (drag to reorder)"));
                    rects.push(resp.rect);
                    if resp.clicked() {
                        self.tool = tool;
                        self.save_session();
                    }
                    if resp.drag_started() {
                        src = Some(i);
                        dst = Some(i);
                    }
                    if resp.dragged() && src == Some(i) {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                    }
                }
                // 드래그 중 포인터가 놓인 버튼을 드롭 대상으로 지정.
                if let Some(_s) = src {
                    if let Some(pos) = ui.input(|i| i.pointer.interact_pos()) {
                        if let Some(idx) = rects.iter().position(|r| r.contains(pos)) {
                            dst = Some(idx);
                        }
                    }
                }
                // 놓으면 순서 이동 + 저장.
                let down = ui.input(|i| i.pointer.any_down());
                if src.is_some() && !down {
                    if let (Some(s), Some(d)) = (src, dst) {
                        if s != d && s < order.len() && d < order.len() {
                            let item = order.remove(s);
                            order.insert(d, item);
                            self.tool_order = order;
                            self.save_default_session();
                            self.save_session();
                        }
                    }
                    src = None;
                    dst = None;
                }
                self.tool_drag = src;
                self.tool_drop = dst;
                // 드롭 대상 표시 (작은 캐럿)
                if let Some(d) = self.tool_drop {
                    if self.tool_drag.is_some() {
                        if let Some(r) = rects.get(d) {
                            let x = r.left();
                            let y0 = r.top();
                            let y1 = r.bottom();
                            ui.painter().line_segment(
                                [egui::pos2(x, y0), egui::pos2(x, y1)],
                                egui::Stroke::new(
                                    2.0,
                                    egui::Color32::from_rgba_unmultiplied(255, 200, 60, 220),
                                ),
                            );
                        }
                    }
                }
                ui.separator();

                match self.tool {
                    ToolType::Pen => {
                        egui::ComboBox::from_id_salt("family")
                            .selected_text(self.color_family.label())
                            .show_ui(ui, |ui| {
                                for family in ColorFamily::all() {
                                    if ui
                                        .selectable_value(
                                            &mut self.color_family,
                                            family,
                                            family.label(),
                                        )
                                        .changed()
                                    {
                                        self.save_session();
                                    }
                                }
                            });
                        let swatches = Palette::swatches(self.color_family);
                        // Round color swatches forming a neat color bar.
                        for (i, swatch) in swatches.iter().enumerate() {
                            let color = Color32::from_rgba_unmultiplied(
                                swatch[0],
                                swatch[1],
                                swatch[2],
                                swatch[3],
                            );
                            let selected = *swatch == self.pen_color;
                            if color_circle_swatch(ui, ("pen_swatch", i), color, selected)
                                .on_hover_text("Pen color")
                                .clicked()
                            {
                                self.pen_color = *swatch;
                                self.save_default_session();
                                self.save_session();
                            }
                        }
                        // 계열 스와치 외의 원하는 색을 직접 고릅니다.
                        let mut pen_color = Color32::from_rgba_unmultiplied(
                            self.pen_color[0],
                            self.pen_color[1],
                            self.pen_color[2],
                            self.pen_color[3],
                        );
                        if ui
                            .color_edit_button_srgba(&mut pen_color)
                            .on_hover_text("Custom pen color")
                            .changed()
                        {
                            self.pen_color = pen_color.to_array();
                            self.save_default_session();
                            self.save_session();
                        }
                        // Width 슬라이더 툴팁.
                        let width_resp = ui
                            .add(egui::Slider::new(&mut self.pen_width, 0.5..=12.0).text("Width"))
                            .on_hover_text(
                                "Base line width (pt). The ballpen model varies it only \
                                 a little (±30%) by pressure & speed.",
                            );
                        if width_resp.changed() {
                            self.save_session();
                        }
                        // 세부 설정 전용 창 트리거 — 툴바는 필수만 남깁니다.
                        if ui
                            .add(egui::Button::new(icon_text(ui, "Settings", icons::GEAR)))
                            .on_hover_text(
                                "Open the ballpen settings window:\n\
                                 physics model, ink soak, ink grain, cursor, smoothing.",
                            )
                            .clicked()
                        {
                            self.tool_settings_open = true;
                        }
                        if ui
                            .checkbox(&mut self.pressure_enabled, "Pressure")
                            .on_hover_text(
                                "Use pen/tablet pressure. Off = always full pressure.",
                            )
                            .changed()
                        {
                            self.save_session();
                        }
                        if ui
                            .checkbox(&mut self.left_handed, "Left-handed")
                            .on_hover_text(
                                "Pen cursor barrel points to the LEFT half-plane\n\
                                 (right-handed = right half-plane).",
                            )
                            .changed()
                        {
                            self.save_default_session();
                            self.save_session();
                        }
                    }
                    ToolType::Fountain => {
                        // 만년필: 색/두께 모두 볼펜과 **완전히 독립**.
                        let swatches = Palette::swatches(self.color_family);
                        for (i, swatch) in swatches.iter().enumerate() {
                            let color = Color32::from_rgba_unmultiplied(
                                swatch[0],
                                swatch[1],
                                swatch[2],
                                swatch[3],
                            );
                            let selected = *swatch == self.fountain_color;
                            if color_circle_swatch(ui, ("fountain_swatch", i), color, selected)
                                .on_hover_text("Ink color")
                                .clicked()
                            {
                                self.fountain_color = *swatch;
                                self.save_default_session();
                                self.save_session();
                            }
                        }
                        let mut fountain_color = Color32::from_rgba_unmultiplied(
                            self.fountain_color[0],
                            self.fountain_color[1],
                            self.fountain_color[2],
                            self.fountain_color[3],
                        );
                        if ui
                            .color_edit_button_srgba(&mut fountain_color)
                            .on_hover_text("Custom ink color")
                            .changed()
                        {
                            self.fountain_color = fountain_color.to_array();
                            self.save_default_session();
                            self.save_session();
                        }
                        let nib_resp = ui
                            .add(
                                egui::Slider::new(&mut self.fountain_width, 0.5..=12.0)
                                    .text("Nib"),
                            )
                            .on_hover_text(
                                "Nib width = maximum line width (pt).\n\
                                 The model varies it by pressure, speed and tilt.",
                            );
                        if nib_resp.changed() {
                            self.save_session();
                        }
                        // 세부 설정 전용 창 트리거 — 툴바는 필수만 남깁니다.
                        if ui
                            .add(egui::Button::new(icon_text(ui, "Settings", icons::GEAR)))
                            .on_hover_text(
                                "Open the fountain pen settings window:\n\
                                 physics model, ink soak, ink grain, italic nib, dwell.",
                            )
                            .clicked()
                        {
                            self.tool_settings_open = true;
                        }
                    }
                    ToolType::Highlighter => {
                        let mut color = Color32::from_rgba_unmultiplied(
                            self.hi_color[0],
                            self.hi_color[1],
                            self.hi_color[2],
                            self.hi_color[3],
                        );
                        if ui.color_edit_button_srgba(&mut color).changed() {
                            self.hi_color = color.to_array();
                            self.save_session();
                        }
                        if ui
                            .add(egui::Slider::new(&mut self.hi_width, 4.0..=40.0).text("Width"))
                            .changed()
                        {
                            self.save_session();
                        }
                        if ui
                            .checkbox(&mut self.text_highlight_snap, "Snap to text")
                            .on_hover_text(
                                "Highlight the recognized document text your stroke touches\n\
                                 (off = freehand translucent stroke)",
                            )
                            .changed()
                        {
                            self.save_default_session();
                            self.save_session();
                        }
                    }
                    ToolType::Eraser => {
                        if ui
                            .add(egui::Slider::new(&mut self.eraser_radius, 4.0..=60.0).text("Radius"))
                            .changed()
                        {
                            self.save_session();
                        }
                    }
                    ToolType::Pan => {}
                }
                });
            });

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(2.0);

            // Row 4: search (only while Ctrl+F is pressed)
            if self.show_search {
                toolbar_row(ui, |ui| {
                    ui.horizontal(|ui| {
                    ui.label(icon_text(ui, "Find", icons::MAGNIFYING_GLASS));
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.search_query)
                            .hint_text("Search in this page...")
                            .desired_width(200.0),
                    );
                    let submitted =
                        resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if self.focus_search {
                        resp.request_focus();
                        self.focus_search = false;
                    }
                    if ui.button("Find").clicked() || submitted {
                        self.search_update();
                    }
                    let can = !self.search_matches.is_empty();
                    if ui
                        .add_enabled(can, egui::Button::new(icon_text(ui, "", icons::CARET_UP)))
                        .on_hover_text("Previous match")
                        .clicked()
                    {
                        self.search_find(false);
                    }
                    if ui
                        .add_enabled(can, egui::Button::new(icon_text(ui, "", icons::CARET_DOWN)))
                        .on_hover_text("Next match")
                        .clicked()
                    {
                        self.search_find(true);
                    }
                    if !self.search_matches.is_empty() {
                        let cur = self.search_current.map(|c| c + 1).unwrap_or(0);
                        ui.label(format!("{cur}/{}", self.search_matches.len()));
                    }
                    if ui
                        .add(egui::Button::new(icon_text(ui, "", icons::X)).frame(false))
                        .on_hover_text("Close search (Ctrl+F)")
                        .clicked()
                    {
                        self.show_search = false;
                        self.search_clear();
                    }
                });
                });
                ui.add_space(4.0);
            }
        });

        // ── 도구별 세부 설정 플로팅 창 (툴바의 Settings 버튼으로 열림) ──
        // Photoshop식: 툴바는 색/두께/필수 토글만 남기고, 세부 파라미터는
        // 이 전용 창에서 지정합니다. 창은 현재 도구(볼펜/만년필)를 따릅니다.
        if self.tool_settings_open {
            let mut open = self.tool_settings_open;
            let (title, is_fountain) = if self.tool == ToolType::Fountain {
                ("Fountain pen settings", true)
            } else {
                ("Ballpen settings", false)
            };
            egui::Window::new(title)
                .open(&mut open)
                .resizable(true)
                .default_width(330.0)
                .show(ui.ctx(), |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        if is_fountain {
                            self.fountain_settings_ui(ui);
                        } else {
                            self.pen_settings_ui(ui);
                        }
                    });
                });
            self.tool_settings_open = open;
        }

        // ── Paper 세부 설정 플로팅 창 (툴바 Paper 옆 Settings 버튼) ──
        if self.paper_settings_open {
            let mut open = self.paper_settings_open;
            egui::Window::new("Paper settings")
                .open(&mut open)
                .resizable(true)
                .default_width(330.0)
                .show(ui.ctx(), |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        self.paper_settings_ui(ui);
                    });
                });
            self.paper_settings_open = open;
        }

        // ── 미디어 서버 연결 설정 창 (툴바 Server 버튼) ──
        if self.server_settings_open {
            let mut open = self.server_settings_open;
            egui::Window::new("Media server")
                .open(&mut open)
                .resizable(false)
                .default_width(380.0)
                .show(ui.ctx(), |ui| {
                    self.server_settings_ui(ui);
                });
            self.server_settings_open = open;
        }
    }
}
