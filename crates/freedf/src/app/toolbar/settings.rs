//! 설정 UI — 펜/만년필/휠/캔버스/종이/서버 설정 창 내용 + 창 렌더.

use super::*;

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

    /// Color wheel(펜 사이드 버튼 원형 팔레트) 색 지정 — 전용 창 내용.
    pub(crate) fn wheel_settings_ui(&mut self, ui: &mut egui::Ui) {
        ui.label(
            egui::RichText::new(
                "Colors shown around the pen wheel (side-button palette).\n\
                 Click a color to remove it. Set R/G/B (or use the picker)\n\
                 and press Add to add a new color.",
            )
            .weak()
            .small(),
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
        ui.horizontal(|ui| {
            ui.label("Add:");
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
            if ui
                .color_edit_button_srgba(&mut pick)
                .on_hover_text("Pick a color")
                .changed()
            {
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
        if full {
            ui.label(
                egui::RichText::new(format!(
                    "Wheel is full ({MAX_FAVORITE_COLORS} colors) — remove one first."
                ))
                .weak()
                .small(),
            );
        }
    }

    /// Canvas(페이지 뒤 배경) 색 설정 — 전용 창 내용.
    pub(crate) fn canvas_settings_ui(&mut self, ui: &mut egui::Ui) {
        ui.label(
            egui::RichText::new(
                "The area behind the page (page surround).\n\
                 Applies immediately and is saved with the session.",
            )
            .weak()
            .small(),
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
        if ui
            .color_edit_button_srgba(&mut custom)
            .on_hover_text("Custom canvas color")
            .changed()
        {
            self.canvas_color = custom.to_array();
            self.save_default_session();
            self.save_session();
        }
    }

    /// Insert Page 플로팅 창 내용 — 페이지 수 입력 + 위치 선택.
    pub(crate) fn insert_page_ui(&mut self, ui: &mut egui::Ui) {
        let page_count = self.document.as_ref().map(|d| d.page_count()).unwrap_or(0);
        ui.horizontal(|ui| {
            ui.label("Pages:");
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
        ui.label(egui::RichText::new("Insert blank pages at:").weak().small());
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
    pub(crate) fn server_settings_ui(&mut self, ui: &mut egui::Ui) {
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

    /// 설정 플로팅 창들 렌더 (툴바 뒤에 호출).
    pub(crate) fn settings_windows(&mut self, ui: &mut egui::Ui) {
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
    
            // ── Canvas(배경색) 설정 플로팅 창 ──
            if self.canvas_settings_open {
                let mut open = self.canvas_settings_open;
                egui::Window::new("Canvas settings")
                    .open(&mut open)
                    .resizable(false)
                    .default_width(330.0)
                    .show(ui.ctx(), |ui| {
                        self.canvas_settings_ui(ui);
                    });
                self.canvas_settings_open = open;
            }
    
            // ── Color wheel(원형 팔레트 색 지정) 설정 플로팅 창 ──
            if self.wheel_settings_open {
                let mut open = self.wheel_settings_open;
                egui::Window::new("Color wheel settings")
                    .open(&mut open)
                    .resizable(false)
                    .default_width(330.0)
                    .show(ui.ctx(), |ui| {
                        self.wheel_settings_ui(ui);
                    });
                self.wheel_settings_open = open;
            }
    
            // ── Insert Page 플로팅 창 (메뉴 대신 — 타이핑이 유지됨) ──
            if self.insert_page_open {
                let mut open = self.insert_page_open;
                egui::Window::new("Insert pages")
                    .open(&mut open)
                    .resizable(false)
                    .default_width(300.0)
                    .show(ui.ctx(), |ui| {
                        self.insert_page_ui(ui);
                    });
                self.insert_page_open = open;
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
