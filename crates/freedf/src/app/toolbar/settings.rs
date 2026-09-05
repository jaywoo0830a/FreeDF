//! 설정 UI — 펜/만년필/휠/캔버스/종이/서버 설정 창 내용 + 창 렌더.
//!
//! 모든 데이터 입력은 Bootstrap 5 스타일 **폼 컴포넌트**(`crate::ui::form`)를
//! 사용합니다 — 슬라이더/체크박스/숫자/텍스트/콤보는 `form::range`, `form::check`,
//! `form::number`, `form::text`, `form::select`의 단일 경로로 들어갑니다.

use super::*;
use crate::ui::form;

/// 플로팅 설정 창 boilerplate 제거 — 열림 상태를 바인딩합니다
/// (React의 <Modal open={..}>와 같은 역할). 실제 렌더링/여백은
/// 공용 컴포넌트 `crate::ui::dialog::dialog`가 담당합니다.
fn settings_window(
    ctx: &egui::Context,
    open: &mut bool,
    title: &str,
    width: f32,
    resizable: bool,
    scroll: bool,
    content: impl FnOnce(&mut egui::Ui),
) {
    crate::ui::dialog::dialog(ctx, open, title, width, resizable, scroll, content);
}

impl FreeDfApp {
    /// 볼펜(일반 펜) 세부 설정 — 전용 플로팅 창 내용 (툴바 Settings 버튼으로 열림).
    /// 툴바에는 색/두께/필수 토글만 남기고 나머지는 여기서 지정합니다.
    pub(crate) fn pen_settings_ui(&mut self, ui: &mut egui::Ui) {
        // 미리보기: 실제 볼펜 모델 수식으로 그립니다.
        let preview_color = Color32::from_rgba_unmultiplied(
            self.pen_color[0],
            self.pen_color[1],
            self.pen_color[2],
            self.pen_color[3],
        );
        pen_profile_preview(ui, preview_color, self.pen_width, &self.pen_profile);
        ui.separator();
        form::fieldset(ui, "pen_win_model", "Physics model", true, |ui| {
            let any_changed = form::range(
                ui,
                &mut self.pen_profile.pressure_k,
                0.0..=0.5,
                "Press k",
                "Pressure influence (small for ballpens, 0.1~0.3).\n\
                 0 = constant width.",
            )
            .changed()
                | form::range(
                    ui,
                    &mut self.pen_profile.speed_k,
                    0.0..=0.3,
                    "Speed k",
                    "Speed influence (small, 0.05~0.15).\n\
                     Fast strokes thin slightly.",
                )
                .changed()
                | form::range(
                    ui,
                    &mut self.pen_profile.starve_v,
                    200.0..=3000.0,
                    "Starve v",
                    "Speed (pt/s) where ink starvation starts — above this \
                     the line thins and breaks like a real ballpen.",
                )
                .changed()
                | form::range(
                    ui,
                    &mut self.pen_profile.tilt_k,
                    0.0..=1.0,
                    "Tilt",
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
        form::fieldset(ui, "pen_win_soak", "Ink soak", true, |ui| {
            if form::check(
                ui,
                &mut self.pen_soak.enabled,
                "Ink soak",
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
                let changed = form::range(
                    ui,
                    &mut self.pen_soak.saturate_sec,
                    0.5..=5.0,
                    "Soak time",
                    "How long (seconds) fresh ballpen ink takes to \
                     reach its full color.",
                )
                .changed()
                    | form::range(
                        ui,
                        &mut self.pen_soak.initial,
                        0.1..=0.9,
                        "Initial",
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
        form::fieldset(ui, "pen_win_grain", "Ink grain", false, |ui| {
            if ink_grain_controls(ui, &mut self.pen_grain) {
                self.save_default_session();
                self.save_session();
            }
        });
        form::fieldset(ui, "pen_win_input", "Input & cursor", false, |ui| {
            form::select(
                ui,
                "pen_cursor_style_win",
                self.pen_cursor_style.label(),
                "Cursor",
                "Pen cursor shape",
                |ui| {
                    for style in PenCursorStyle::all() {
                        ui.selectable_value(&mut self.pen_cursor_style, style, style.label());
                    }
                },
            );
            if form::check(
                ui,
                &mut self.smoothing_enabled,
                "Stabilize",
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
                && form::range(
                    ui,
                    &mut self.smoothing,
                    0.0..=1.0,
                    "Strength",
                    "Filter strength while Stabilize is on.\n\
                     0 = raw input, 1 = silky smooth.",
                )
                .changed()
            {
                self.smoothing = self.smoothing.clamp(0.0, 1.0);
                self.save_default_session();
                self.save_session();
            }
            if form::check(
                ui,
                &mut self.mouse_draws,
                "Mouse ink",
                "Draw ink with the mouse/trackpad too.\n\
                 Off (default): mouse & trackpad pan the page — \
                 only a pen writes, like real note-taking apps.",
            )
            .changed()
            {
                self.save_default_session();
                self.save_session();
            }
            if form::check(
                ui,
                &mut self.debug_hud,
                "Debug HUD",
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
    pub(crate) fn fountain_settings_ui(&mut self, ui: &mut egui::Ui) {
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
        form::fieldset(ui, "fountain_win_model", "Physics model", true, |ui| {
            let any_changed = form::range(
                ui,
                &mut self.fountain_profile.min_width_pt,
                0.1..=2.0,
                "Min",
                "Thinnest line width (pt) when writing fast and light.",
            )
            .changed()
                | form::range(
                    ui,
                    &mut self.fountain_profile.pressure_alpha,
                    0.3..=2.0,
                    "Press α",
                    "Pressure sensitivity: how strongly pressure widens \
                     the line (0.7~1.2 typical).",
                )
                .changed()
                | form::range(
                    ui,
                    &mut self.fountain_profile.speed_beta,
                    0.3..=3.0,
                    "Speed β",
                    "Speed sensitivity: how strongly fast strokes thin \
                     the line (1.0~1.5 typical).",
                )
                .changed()
                | form::range(
                    ui,
                    &mut self.fountain_profile.speed_ref,
                    10.0..=200.0,
                    "Speed ref",
                    "Reference speed (pt/s) — at this speed the speed \
                     factor is 0.5. Lower = thinner when writing normally.",
                )
                .changed()
                | form::range(
                    ui,
                    &mut self.fountain_profile.tilt_k,
                    0.0..=1.0,
                    "Tilt",
                    "Tilt influence: laying the pen down widens the line.\n\
                     Note: egui/winit don't expose pen tilt yet, so this \
                     is 0 until a HID/WM_POINTER hook feeds set_pen_tilt.",
                )
                .changed();
            if form::check(
                ui,
                &mut self.fountain_profile.italic,
                "Italic nib",
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
                form::range(
                    ui,
                    &mut self.fountain_profile.nib_angle_deg,
                    0.0..=180.0,
                    "Nib angle",
                    "Nib axis direction (degrees).",
                )
                .changed()
                    | form::range(
                        ui,
                        &mut self.fountain_profile.italic_k,
                        0.0..=0.6,
                        "Contrast",
                        "Italic direction contrast (0.2~0.5 looks stub-like).",
                    )
                    .changed()
            } else {
                false
            };
            let dwell_changed = form::range(
                ui,
                &mut self.fountain_profile.dwell_k,
                0.0..=0.5,
                "Dwell",
                "Ink pooling when the pen nearly stops — the classic \
                 fountain-pen blob at the end of a stroke.",
            )
            .changed();
            if any_changed || any_changed2 || dwell_changed {
                self.save_default_session();
                self.save_session();
            }
        });
        form::fieldset(ui, "fountain_win_soak", "Ink soak", true, |ui| {
            if form::check(
                ui,
                &mut self.fountain_soak.enabled,
                "Ink soak",
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
                let changed = form::range(
                    ui,
                    &mut self.fountain_soak.saturate_sec,
                    0.5..=5.0,
                    "Soak time",
                    "How long (seconds) fresh ink takes to deepen \
                     from its light state to the full ink color.",
                )
                .changed()
                    | form::range(
                        ui,
                        &mut self.fountain_soak.initial,
                        0.1..=0.9,
                        "Initial",
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
        form::fieldset(ui, "fountain_win_grain", "Ink grain", false, |ui| {
            if ink_grain_controls(ui, &mut self.fountain_grain) {
                self.save_default_session();
                self.save_session();
            }
        });
    }

    /// Color wheel(펜 사이드 버튼 원형 팔레트) 색 지정 — 전용 창 내용.
    pub(crate) fn wheel_settings_ui(&mut self, ui: &mut egui::Ui) {
        form::help(
            ui,
            "Colors shown around the pen wheel (side-button palette).\n\
             Click a color to remove it. Set R/G/B (or use the picker)\n\
             and press Add to add a new color.",
        );
        ui.add_space(4.0);
        let mut remove_idx: Option<usize> = None;
        ui.horizontal_wrapped(|ui| {
            for (i, color) in self.favorite_colors.iter().enumerate() {
                let c = Color32::from_rgba_unmultiplied(color[0], color[1], color[2], color[3]);
                let resp = color_circle_swatch(ui, ("wheel_color", i), c, false)
                    .on_hover_text("Click to remove from the wheel");
                if resp.clicked() {
                    remove_idx = Some(i);
                }
            }
        });
        if let Some(i) = remove_idx {
            self.favorite_colors.remove(i);
            self.save_default_session();
            self.save_session();
        }
        // 새 색 추가 — **RGB 숫자 입력 + 팝업 컬러픽커** (최대 MAX_FAVORITE_COLORS).
        let full = self.favorite_colors.len() >= MAX_FAVORITE_COLORS;
        form::group("Add").optional().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut self.wheel_pick_color[0])
                        .range(0..=255)
                        .prefix("R "),
                );
                ui.add(
                    egui::DragValue::new(&mut self.wheel_pick_color[1])
                        .range(0..=255)
                        .prefix("G "),
                );
                ui.add(
                    egui::DragValue::new(&mut self.wheel_pick_color[2])
                        .range(0..=255)
                        .prefix("B "),
                );
                let mut pick = Color32::from_rgba_unmultiplied(
                    self.wheel_pick_color[0],
                    self.wheel_pick_color[1],
                    self.wheel_pick_color[2],
                    self.wheel_pick_color[3],
                );
                if form::color(ui, &mut pick, "", "Pick a color").changed() {
                    self.wheel_pick_color = pick.to_array();
                }
                // 휠 색은 불투명으로 유지 (알파는 팔레트에 의미 없음).
                self.wheel_pick_color[3] = 255;
                let picked = self.wheel_pick_color;
                if ui
                    .add_enabled(!full, egui::Button::new("Add to wheel"))
                    .clicked()
                {
                    if !self.favorite_colors.contains(&picked) {
                        self.favorite_colors.push(picked);
                        self.save_default_session();
                        self.save_session();
                    }
                }
            });
        });
        if full {
            form::help(
                ui,
                format!("Wheel is full ({MAX_FAVORITE_COLORS} colors) — remove one first."),
            );
        }
    }

    /// Canvas(페이지 뒤 배경) 색 설정 — 전용 창 내용.
    pub(crate) fn canvas_settings_ui(&mut self, ui: &mut egui::Ui) {
        form::help(
            ui,
            "The area behind the page (page surround).\n\
             Applies immediately and is saved with the session.",
        );
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            for (i, preset) in CANVAS_COLOR_PRESETS.iter().enumerate() {
                let color = Color32::from_rgba_unmultiplied(
                    preset[0],
                    preset[1],
                    preset[2],
                    preset[3],
                );
                let selected = self.canvas_color == *preset;
                if color_circle_swatch(ui, ("canvas_preset", i), color, selected)
                    .on_hover_text("Preset")
                    .clicked()
                {
                    self.canvas_color = *preset;
                    self.save_default_session();
                    self.save_session();
                }
            }
        });
        ui.add_space(4.0);
        let mut custom = Color32::from_rgba_unmultiplied(
            self.canvas_color[0],
            self.canvas_color[1],
            self.canvas_color[2],
            self.canvas_color[3],
        );
        if form::color(ui, &mut custom, "Color", "Custom canvas color").changed() {
            self.canvas_color = custom.to_array();
            self.save_default_session();
            self.save_session();
        }
    }

    /// Insert Page 플로팅 창 내용 — 페이지 수 입력 + 위치 선택.
    pub(crate) fn insert_page_ui(&mut self, ui: &mut egui::Ui) {
        let page_count = self.document.as_ref().map(|d| d.page_count()).unwrap_or(0);
        form::group("Pages").required().show(ui, |ui| {
            // 포커스가 없을 때만 카운트와 동기화 — 타이핑 중에는
            // 매 프레임 덮어쓰지 않습니다.
            if !self.insert_page_focus {
                self.insert_page_text = self.insert_page_count.to_string();
            }
            let resp = ui.add(
                egui::TextEdit::singleline(&mut self.insert_page_text)
                    .desired_width(56.0)
                    .char_limit(3)
                    .hint_text("1–200"),
            );
            self.insert_page_focus = resp.has_focus();
            if resp.changed() {
                if let Ok(v) = self.insert_page_text.trim().parse::<usize>() {
                    self.insert_page_count = v.clamp(1, 200);
                }
            }
            if resp.lost_focus() {
                self.insert_page_count = self
                    .insert_page_text
                    .trim()
                    .parse()
                    .unwrap_or(self.insert_page_count)
                    .clamp(1, 200);
                self.insert_page_text = self.insert_page_count.to_string();
            }
        });
        ui.add_space(6.0);
        form::label(ui, "Insert blank pages at:");
        let insert = [
            (InsertTarget::FromCurrent, "From current page (copies size & paper)"),
            (InsertTarget::AtVeryFront, "At the very front"),
            (InsertTarget::AtVeryBack, "At the very back"),
            (InsertTarget::BeforeCurrent, "Before current page"),
            (InsertTarget::AfterCurrent, "After current page"),
        ];
        let mut chosen: Option<InsertTarget> = None;
        for (target, label) in insert {
            if ui
                .add_enabled(page_count > 0, egui::Button::new(label))
                .clicked()
            {
                chosen = Some(target);
            }
        }
        if let Some(target) = chosen {
            self.insert_pages_action(target, self.insert_page_count);
            self.insert_page_open = false;
        }
    }

    /// 적용 규칙 (명확하게):
    /// - 스타일/색 선택: **현재 페이지에 즉시 적용** + 앞으로 만드는
    ///   페이지의 기본값이 됩니다.
    /// - 스타일별 세부설정(간격/색/두께): **스타일 프리셋** — 현재 선택된
    ///   스타일의 값을 편집하며, 그 스타일을 쓰는 **모든 페이지**가 함께 바뀝니다.
    /// - 대량 적용: 아래 Apply 섹션에서 모든 페이지/범위를 명시적으로 선택.
    pub(crate) fn paper_settings_ui(&mut self, ui: &mut egui::Ui) {
        const MM_TO_PT: f32 = 72.0 / 25.4;
        let page_count = self.document.as_ref().map(|d| d.page_count()).unwrap_or(0);
        form::help(
            ui,
            "Style & color apply to the current page right away\n\
             and become the default for new pages.",
        );
        form::fieldset(ui, "paper_win_paper", "Paper", true, |ui| {
            form::select(
                ui,
                "paper_style_win",
                self.paper_style.label(),
                "Style",
                "Paper style for the current page.\n\
                 New pages & new notes use it as their default.",
                |ui| {
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
                },
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
            if form::color(ui, &mut paper_color, "Color", "Custom paper color (current page)").changed()
            {
                self.paper_color = paper_color.to_array();
                self.apply_paper_to_current_page();
                self.save_default_session();
                self.save_session();
            }
        });
        // ── 종이 질감 — 물리 기반 표면 모델 (docs/paper-texture-model.md) ──
        // 모든 질감 설정을 한 곳에 통합: 켜기/끄기 + 강도 + 표면·조명 10개.
        form::fieldset(ui, "paper_win_texture", "Paper texture", false, |ui| {
            form::help(
                ui,
                "Physical paper surface: height field → normals → lighting.\n\
                 Pick a preset (Lowest…Highest) or enable Custom for fine control.",
            );
            let mut changed = form::check(
                ui,
                &mut self.paper_texture,
                "Paper texture",
                "Fiber grain over the page (drawn under the ruling and your ink).",
            )
            .changed();
            // 초보자용 5단계 프리셋 — Custom이 켜지면 상세 값이 우선.
            ui.add_enabled_ui(self.paper_texture && !self.paper_texture_custom, |ui| {
                let mut lvl = self.paper_texture_level as i32;
                if form::range_i(
                    ui,
                    &mut lvl,
                    0..=4,
                    "Preset",
                    "Lowest / Low / Medium / High / Highest",
                    Some(|v, _| freedf_core::paper::paper_texture_preset_label(v as u8).to_owned()),
                )
                .changed()
                {
                    self.paper_texture_level = lvl as u8;
                    self.apply_paper_texture_preset();
                    changed = true;
                }
            });
            if form::check(
                ui,
                &mut self.paper_texture_custom,
                "Custom",
                "Unlock the detailed surface & lighting values below.\n\
                 Unchecking returns to the preset.",
            )
            .changed()
            {
                if !self.paper_texture_custom {
                    self.apply_paper_texture_preset(); // 프리셋으로 되돌림.
                }
                changed = true;
            }
            ui.add_enabled_ui(self.paper_texture && self.paper_texture_custom, |ui| {
                changed |= form::range(
                    ui,
                    &mut self.paper_texture_strength,
                    0.0..=1.0,
                    "Strength",
                    "How visible the paper grain is (0 = invisible).",
                )
                .changed();
                let s = &mut self.paper_surface;
                egui::Grid::new("paper_surface_grid")
                    .num_columns(2)
                    .spacing(egui::vec2(8.0, 2.0))
                    .show(ui, |ui| {
                        changed |= form::range(
                            ui,
                            &mut s.bump,
                            0.0..=2.0,
                            "Bump β",
                            "Relief strength — height gradient scale.",
                        )
                        .changed();
                        changed |= form::range(
                            ui,
                            &mut s.albedo_l,
                            0.0..=0.3,
                            "Light var a_L",
                            "Shared luminance variation (fiber absorption).",
                        )
                        .changed();
                        ui.end_row();
                        changed |= form::range(
                            ui,
                            &mut s.albedo_c,
                            0.0..=0.2,
                            "Chroma var a_C",
                            "Per-channel spectral variation (warm/cool flecks).",
                        )
                        .changed();
                        changed |= form::range(
                            ui,
                            &mut s.ao_strength,
                            0.0..=1.0,
                            "Valley AO k",
                            "Ambient occlusion in the fiber valleys.",
                        )
                        .changed();
                        ui.end_row();
                        changed |= form::range(
                            ui,
                            &mut s.light_azimuth_deg,
                            0.0..=360.0,
                            "Light azimuth °",
                            "Directional light angle around the page.",
                        )
                        .changed();
                        changed |= form::range(
                            ui,
                            &mut s.light_elevation_deg,
                            5.0..=90.0,
                            "Light elevation °",
                            "Light height above the page (90° = straight down).",
                        )
                        .changed();
                        ui.end_row();
                        changed |= form::range(
                            ui,
                            &mut s.ambient,
                            0.0..=1.0,
                            "Ambient E_a",
                            "Hemispheric ambient light intensity.",
                        )
                        .changed();
                        changed |= form::range(
                            ui,
                            &mut s.direct,
                            0.0..=1.0,
                            "Direct E_d",
                            "Directional light intensity.",
                        )
                        .changed();
                        ui.end_row();
                        changed |= form::range(
                            ui,
                            &mut s.sheen,
                            0.0..=0.3,
                            "Sheen ρ_s",
                            "Subtle specular sheen strength.",
                        )
                        .changed();
                        changed |= form::range(
                            ui,
                            &mut s.gloss,
                            1.0..=64.0,
                            "Gloss α",
                            "Sheen sharpness (Blinn-Phong exponent).",
                        )
                        .changed();
                        ui.end_row();
                    });
            });
            if changed {
                self.save_default_session();
                self.save_session();
            }
        });
        // ── 스타일별 독립 세부설정 — **현재 선택된 스타일**을 편집합니다.
        // Ruled/Grid/Dotted 각각 자기만의 간격/색/두께를 가집니다.
        form::fieldset(ui, "paper_win_style", "Style details", true, |ui| {
            match self.paper_style {
                PaperStyle::Blank => {
                    form::help(ui, "Blank paper has no lines — nothing to configure.");
                }
                PaperStyle::Ruled | PaperStyle::Grid | PaperStyle::Dotted => {
                    let style = self.paper_style;
                    form::help(
                        ui,
                        format!(
                            "Editing the '{}' style — every page using it updates together.",
                            style.label()
                        ),
                    );
                    // 값 복사본으로 편집 → 변경 시 프리셋에 다시 기록.
                    let mut ls = self.paper_style_settings.of(style).unwrap();
                    let mut changed = false;
                    changed |= form::number(&mut ls.spacing)
                        .label("Spacing")
                        .range(12.0..=120.0)
                        .speed(1.0)
                        .suffix(" pt")
                        .help("Line / dot spacing (pt)")
                        .show(ui)
                        .changed();
                    changed |= form::number(&mut ls.width)
                        .label("Line")
                        .range(0.25..=8.0)
                        .speed(0.05)
                        .decimals(2)
                        .suffix(" pt")
                        .help("Line / dot thickness (pt)")
                        .show(ui)
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
                    if form::color(ui, &mut line_color, "Color", "Custom line color").changed() {
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
        form::fieldset(ui, "paper_win_size", "Page size", true, |ui| {
            form::select(
                ui,
                "paper_size_win",
                self.paper_size.label(),
                "Size",
                "Size of new pages & new notes (existing pages keep their size).",
                |ui| {
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
                },
            );
            let pts = self.new_page_size_pts();
            form::help(
                ui,
                format!(
                    "{} — {:.0} × {:.0} pt\n\
                     Applies to NEW pages & notes only; existing pages\n\
                     keep their physical size.",
                    self.paper_size.label(),
                    pts[0],
                    pts[1]
                ),
            );
            // 사용자 정의 크기: mm 단위 숫자 입력.
            if self.paper_size == PaperSize::Custom {
                let mut w_mm = self.custom_paper_size[0] / MM_TO_PT;
                let mut h_mm = self.custom_paper_size[1] / MM_TO_PT;
                let w_changed = form::number(&mut w_mm)
                    .label("Width (mm)")
                    .optional()
                    .range(50.0..=1200.0)
                    .speed(1.0)
                    .help("Custom page width (mm)")
                    .show(ui)
                    .changed();
                let h_changed = form::number(&mut h_mm)
                    .label("Height (mm)")
                    .optional()
                    .range(50.0..=1200.0)
                    .speed(1.0)
                    .help("Custom page height (mm)")
                    .show(ui)
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
        form::input_group(ui, "Range:", "", |ui| {
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

    /// Sync v3 서버 연결 설정 창 내용 (툴바 Server 버튼으로 열림).
    ///
    /// 모든 문서 저장(Sync v3)과 미디어가 이 서버를 통합니다. 주소는
    /// `server.json`에 저장되고 다음 실행에서 로드됩니다 — 빌드타임이 아니라
    /// **런타임 입력**입니다.
    pub(crate) fn server_settings_ui(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("Sync server (v3)").strong());
        form::help(
            ui,
            "All documents are stored through this server — Sync v3\n\
             snapshots and media uploads share one address and API key.",
        );
        let mut changed = false;
        form::group("Server URL")
            .required()
            .hint("Sync v3 snapshots + media share one address.")
            .show(ui, |ui| {
                changed |= form::text(&mut self.media_config.base_url)
                    .hint("https://your-server.example.com")
                    .width(230.0)
                    .help("Sync v3 + media server address")
                    .show(ui)
                    .changed();
            });
        form::group("API key")
            .required()
            .show(ui, |ui| {
                changed |= form::password(&mut self.media_config.api_key)
                    .hint("key")
                    .width(230.0)
                    .help("API key — guards snapshots, uploads, lists and deletes.")
                    .show(ui)
                    .changed();
            });
        ui.horizontal(|ui| {
            if ui
                .button(if self.db_connected { "Reconnect" } else { "Connect" })
                .clicked()
            {
                self.try_connect_server(false);
            }
            if ui.button("Save").clicked() {
                let path = MediaServerConfig::config_path();
                self.server_msg = Some(match self.media_config.save(&path) {
                    Ok(()) => (true, format!("Saved to {}", path.display())),
                    Err(e) => (false, format!("Save failed: {e}")),
                });
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
        form::check(
            ui,
            &mut self.media_config.enabled,
            "Enable media features (audio, photos, video)",
            "Playback streams straight from nginx; the API key guards uploads, lists and deletes.",
        );
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
                let result = match client.health() {
                    Ok(()) => {
                        // 같은 서버를 Sync v3 프로토콜 클라이언트로도 확인 —
                        // 앱의 v3 연결 지점이 살아 있는지 검증.
                        match crate::sync_client::sync_client(&self.media_config) {
                            Some(sync) => match sync.health() {
                                Ok(()) => (
                                    true,
                                    format!("Connected — {} ms (Sync v3)", start.elapsed().as_millis()),
                                ),
                                Err(e) => (false, format!("Sync v3 check failed: {e}")),
                            },
                            None => (true, format!("Connected — {} ms", start.elapsed().as_millis())),
                        }
                    }
                    Err(e) => (false, e),
                };
                self.server_msg = Some(result);
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

    /// 엣지 자동 스크롤 설정 창 내용.
    pub(crate) fn edge_scroll_settings_ui(&mut self, ui: &mut egui::Ui) {
        form::help(
            ui,
            "When zoomed in, moving the pen (or the mouse cursor, if enabled \
             below) close to the canvas edge pans the view in that direction. \
             It is ignored over the palette, the bottom bar and other floating UI.",
        );
        ui.add_space(6.0);
        if form::check(ui, &mut self.edge_autoscroll, "Enable edge auto-scroll", "").changed() {
            self.save_default_session();
            self.save_session();
        }
        ui.add_space(4.0);
        ui.add_enabled_ui(self.edge_autoscroll, |ui| {
            if form::range(
                ui,
                &mut self.edge_zone,
                16.0..=300.0,
                "Edge zone (px)",
                "How close to the edge (in screen pixels) the cursor must be \
                 before scrolling starts.",
            )
            .changed()
            {
                self.save_default_session();
                self.save_session();
            }
            if form::check(
                ui,
                &mut self.edge_autoscroll_pen_only,
                "Only while the pen is in use",
                "Edge auto-scroll reacts only while the pen is driving \
                 the cursor (hover or touch) — a bare mouse/trackpad \
                 cursor at the edge does nothing. Turn off to let the \
                 mouse cursor trigger it too.",
            )
            .changed()
            {
                self.save_default_session();
                self.save_session();
            }
            if form::check(
                ui,
                &mut self.edge_pulse,
                "Breathing edge glow",
                "Soft pulsing glow on the canvas edge while auto-scroll is active.",
            )
            .changed()
            {
                self.save_default_session();
                self.save_session();
            }
            ui.add_space(4.0);
            form::help(
                ui,
                "Direction tuning — speed (px/s) and reaction delay (s).\n\
                 Delay 0 = scrolling starts the moment the cursor touches the edge.\n\
                 Writing flows left→right and top→bottom.",
            );
            let labels = ["Left", "Right", "Up", "Down"];
            let mut changed = false;
            egui::Grid::new("edge_dir_grid")
                .num_columns(3)
                .spacing(egui::vec2(8.0, 2.0))
                .show(ui, |ui| {
                    for (i, label) in labels.iter().enumerate() {
                        ui.label(*label);
                        changed |= form::range(
                            ui,
                            &mut self.edge_speeds[i],
                            20.0..=2000.0,
                            "px/s",
                            "Scroll speed in this direction (screen px per second).",
                        )
                        .changed();
                        changed |= form::range(
                            ui,
                            &mut self.edge_delays[i],
                            0.0..=2.0,
                            "s",
                            "Reaction delay before scrolling starts in this direction.",
                        )
                        .changed();
                        ui.end_row();
                    }
                });
            if changed {
                self.save_default_session();
                self.save_session();
            }
        });
        ui.add_space(4.0);
        // 문서 바깥 패닝 여유 — 엣지 스크롤과 무관하게 일반 팬 범위에도 적용.
        if form::range(
            ui,
            &mut self.edge_overscroll,
            0.0..=600.0,
            "Overscroll beyond page (px)",
            "How far past the document edges the view may scroll \
             (up/down/left/right).",
        )
        .changed()
        {
            self.save_default_session();
            self.save_session();
        }
    }

    /// Window Focus 설정 창 내용 — 커서가 일정 시간 머물면 포커스.
    pub(crate) fn window_focus_settings_ui(&mut self, ui: &mut egui::Ui) {
        form::help(
            ui,
            "In split view, focus this window when the cursor stays \
             over it for the dwell time below.",
        );
        ui.add_space(6.0);
        if form::check(ui, &mut self.window_focus_on_move, "Focus on cursor dwell", "").changed() {
            self.save_default_session();
        }
        ui.add_space(4.0);
        ui.add_enabled_ui(self.window_focus_on_move, |ui| {
            if form::range(
                ui,
                &mut self.window_focus_dwell_sec,
                0.0..=5.0,
                "Dwell time (s)",
                "0 = focus the moment the cursor moves over this window.\n\
                 Higher = the cursor must stay this long before focusing.",
            )
            .changed()
            {
                self.save_default_session();
            }
        });
    }

    /// 설정 플로팅 창들 렌더 (툴바 뒤에 호출).
    pub(crate) fn settings_windows(&mut self, ui: &mut egui::Ui) {
        // ── 도구별 세부 설정 플로팅 창 (툴바의 Settings 버튼으로 열림) ──
        // Photoshop식: 툴바는 색/두께/필수 토글만 남기고, 세부 파라미터는
        // 이 전용 창에서 지정합니다. 창은 현재 도구(볼펜/만년필)를 따릅니다.
        if self.tool_settings_open {
            let (title, is_fountain) = if self.tool == ToolType::Fountain {
                ("Fountain pen settings", true)
            } else {
                ("Ballpen settings", false)
            };
            let mut open = self.tool_settings_open;
            settings_window(ui.ctx(), &mut open, title, 330.0, true, true, |ui| {
                if is_fountain {
                    self.fountain_settings_ui(ui);
                } else {
                    self.pen_settings_ui(ui);
                }
            });
            self.tool_settings_open = open;
        }

        // ── Paper 세부 설정 플로팅 창 (툴바 Paper 옆 Settings 버튼) ──
        if self.paper_settings_open {
            let mut open = self.paper_settings_open;
            settings_window(ui.ctx(), &mut open, "Paper settings", 330.0, true, true, |ui| {
                self.paper_settings_ui(ui)
            });
            self.paper_settings_open = open;
        }

        // ── Canvas(배경색) 설정 플로팅 창 ──
        if self.canvas_settings_open {
            let mut open = self.canvas_settings_open;
            settings_window(
                ui.ctx(),
                &mut open,
                "Canvas settings",
                330.0,
                false,
                false,
                |ui| self.canvas_settings_ui(ui),
            );
            self.canvas_settings_open = open;
        }

        // ── Color wheel(원형 팔레트 색 지정) 설정 플로팅 창 ──
        if self.wheel_settings_open {
            let mut open = self.wheel_settings_open;
            settings_window(
                ui.ctx(),
                &mut open,
                "Color wheel settings",
                330.0,
                false,
                false,
                |ui| self.wheel_settings_ui(ui),
            );
            self.wheel_settings_open = open;
        }

        // ── Edge auto-scroll 설정 플로팅 창 (Row2의 토글/기어) ──
        if self.edge_scroll_settings_open {
            let mut open = self.edge_scroll_settings_open;
            settings_window(
                ui.ctx(),
                &mut open,
                "Edge auto-scroll",
                330.0,
                false,
                false,
                |ui| self.edge_scroll_settings_ui(ui),
            );
            self.edge_scroll_settings_open = open;
        }

        // ── Window Focus 설정 플로팅 창 (Row1의 Focus Delay 버튼) ──
        if self.window_focus_settings_open {
            let mut open = self.window_focus_settings_open;
            settings_window(
                ui.ctx(),
                &mut open,
                "Window Focus",
                330.0,
                false,
                false,
                |ui| self.window_focus_settings_ui(ui),
            );
            self.window_focus_settings_open = open;
        }

        // ── Insert Page 플로팅 창 (메뉴 대신 — 타이핑이 유지됨) ──
        if self.insert_page_open {
            let mut open = self.insert_page_open;
            settings_window(ui.ctx(), &mut open, "Insert pages", 300.0, false, false, |ui| {
                self.insert_page_ui(ui)
            });
            self.insert_page_open = open;
        }

        // ── 미디어 서버 연결 설정 창 (툴바 Server 버튼) ──
        if self.server_settings_open {
            let mut open = self.server_settings_open;
            settings_window(ui.ctx(), &mut open, "Media server", 380.0, false, false, |ui| {
                self.server_settings_ui(ui)
            });
            self.server_settings_open = open;
        }

        // ── Macro 단축키 매핑 창 (Row1의 Macro 버튼) ──
        if self.macro_settings_open {
            let mut open = self.macro_settings_open;
            settings_window(
                ui.ctx(),
                &mut open,
                "Macro settings",
                440.0,
                false,
                false,
                |ui| {
                    self.macro_settings_ui(ui);
                    self.macro_capture_finish(ui.ctx());
                },
            );
            self.macro_settings_open = open;
        }
    }
}
