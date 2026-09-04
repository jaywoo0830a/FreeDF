//! 캔버스 위 플로팅 오버레이 — 페이지 내비게이션 바, 필기구 팔레트,
//! 굿노트식 원형 색상 휠(펜 사이드 버튼).

use super::*;

impl FreeDfApp {
    /// 펜 사이드 버튼 훅 — OTD/evdev 스트림에서 눌림 에지를 감지하면 호출됩니다.
    ///
    /// **새 액션을 연결하는 방법**: 이 match에 arm을 추가하면 됩니다.
    /// (기본 배선: 버튼 1 = 굿노트식 원형 색상 팔레트 토글, 버튼 2 = 예약)
    pub(crate) fn on_pen_button(&mut self, button: u8, _pressed: bool) {
        match button {
            1 => {
                // 버튼 1: 원형 색상 팔레트를 **캔버스 중앙**에 열고 닫습니다.
                // (OTD 전용 모드에서는 펜의 화면 좌표를 egui가 알 수 없어서
                // 포인터 위치 대신 중앙 고정 — 좌측 끝에 뜨던 문제 방지)
                // 상태바 메시지는 띄우지 않습니다 (상태바가 나타나 캔버스가
                // 리사이즈되며 화면이 튀는 것을 방지).
                self.color_wheel_open = !self.color_wheel_open;
                if self.color_wheel_open {
                    self.color_wheel_opened_at = now_ms();
                }
            }
            2 => {
                // 예약 — 펜 색 변경 등 추가 액션을 여기에 연결합니다.
            }
            _ => {}
        }
    }

    /// 원형 팔레트에서 고른 색을 **현재 잉크 도구**에 적용합니다.
    /// (팬/지우개 상태에서 고르면 펜 도구로 전환 — 굿노트 관례)
    pub(crate) fn apply_wheel_color(&mut self, color: [u8; 4]) {
        match self.tool {
            ToolType::Pen => self.pen_color = color,
            ToolType::Fountain => self.fountain_color = color,
            ToolType::Highlighter => self.hi_color = color,
            _ => {
                self.tool = ToolType::Pen;
                self.pen_color = color;
            }
        }
        self.save_default_session();
        self.save_session();
    }

    /// 굿노트식 **원형 색상 팔레트** 오버레이 — 펜 사이드 버튼으로 열립니다.
    /// 펜 위치(클램프 후)에 표시: 유리처럼 투명한 도넛(중앙 구멍), 둘레 =
    /// 사용자가 지정한 팔레트(즐겨찾기). 탭하면 적용+닫힘, 4초간 입력이
    /// 없으면 자동으로 닫힙니다.
    pub(crate) fn color_wheel_overlay(&mut self, ctx: &egui::Context, canvas: Rect) {
        if !self.color_wheel_open {
            return;
        }
        // 방치 시 자동 닫힘 (누르는 중이면 유지).
        const WHEEL_AUTO_CLOSE_MS: u64 = 4000;
        if self.color_wheel_opened_at != 0
            && now_ms().saturating_sub(self.color_wheel_opened_at) > WHEEL_AUTO_CLOSE_MS
            && !ctx.input(|i| i.pointer.any_down())
        {
            self.color_wheel_open = false;
            return;
        }
        // 둘레 색: **사용자가 지정한 팔레트(즐겨찾기)만** 사용.
        let mut ring = self.favorite_colors.clone();
        if ring.is_empty() {
            ring = crate::settings::SessionState::default().favorite_colors;
        }
        ring.truncate(MAX_FAVORITE_COLORS);
        let current = self.current_drawing_style().0;

        // 레이아웃/히트테스트는 순수 객체(ColorWheel)가 담당 — 테스트 대상.
        // (펜 위치 = 버튼을 누른 순간의 포인터, 캔버스 안으로 클램프)
        let wheel = ColorWheel {
            center: self.color_wheel_center(canvas),
            ring: ring.clone(),
        };
        let center = wheel.center;

        // 탭 판정은 위젯 라우팅(`clicked`)에 의존하지 않고 **프레스 이벤트를
        // 직접 읽습니다** — 마우스(포인터)든 펜(Touch)이든, 캔버스에 걸린
        // 클릭 제약(예: 마우스=팬)과도 무관하게 항상 동작합니다.
        let area_pos = center - egui::vec2(WHEEL_BACK_R, WHEEL_BACK_R);
        egui::Area::new(egui::Id::new("color_wheel"))
            .order(egui::Order::Foreground)
            .fixed_pos(area_pos)
            .show(ctx, |ui| {
                // 주의: `painter_at(rect)`의 rect는 **클립 영역**(화면 좌표) —
                // 좌표 오프셋이 아닙니다. ZERO 기준 rect를 넘기면 원이 화면
                // 좌상단에 그려지므로, 반드시 화면 좌표 rect를 넘깁니다.
                // (여유 8px — AA 페더링이 클립에 잘려 원이 찌그러져 보이는 것 방지)
                let rect = egui::Rect::from_center_size(
                    center,
                    egui::vec2(WHEEL_BACK_R * 2.0 + 8.0, WHEEL_BACK_R * 2.0 + 8.0),
                );
                let painter = ui.painter_at(rect);
                let c = rect.center();
                let fill = crate::theme::nord::semantic::overlay_bg();
                // 유리(더 투명) 백플레이트 — 뒤의 페이지/캔버스가 선명하게 비칩니다.
                // 검정 테두리는 없습니다 (은은한 광택 링만).
                painter.circle_filled(c, WHEEL_BACK_R, fill.gamma_multiply(0.12));
                // 유리 광택 — 위쪽으로 어긋난 얇은 하이라이트 링.
                painter.circle_stroke(
                    c + egui::vec2(0.0, -8.0),
                    WHEEL_BACK_R - 8.0,
                    Stroke::new(1.0, Color32::from_white_alpha(30)),
                );
                // 둘레 스와치 — 12시 방향부터 시계 방향.
                for (i, color) in wheel.ring.iter().enumerate() {
                    let sc = wheel.swatch_pos(i);
                    let col = Color32::from_rgba_unmultiplied(
                        color[0],
                        color[1],
                        color[2],
                        color[3],
                    );
                    painter.circle_filled(sc, WHEEL_SWATCH_R, col);
                    painter.circle_stroke(
                        sc,
                        WHEEL_SWATCH_R,
                        Stroke::new(1.0, Color32::from_gray(180)),
                    );
                    if *color == current {
                        painter.circle_stroke(
                            sc,
                            WHEEL_SWATCH_R + 3.0,
                            Stroke::new(
                                2.0,
                                crate::theme::nord::semantic::ACCENT_ACTIVE,
                            ),
                        );
                    }
                }
                // 중앙 = 도넛 구멍 — 현재 색 디스크 없이 뻥 뚫립니다.
                // (캔버스 배경색으로 채워 구멍처럼 보이게 하고, 안쪽 링으로
                // 유리 두께를 표현합니다. 탭하면 그냥 닫힘.)
                let hole = Color32::from_rgba_unmultiplied(
                    self.canvas_color[0],
                    self.canvas_color[1],
                    self.canvas_color[2],
                    self.canvas_color[3],
                );
                painter.circle_filled(c, WHEEL_CENTER_R, hole);
                painter.circle_stroke(
                    c,
                    WHEEL_CENTER_R,
                    Stroke::new(1.0, Color32::from_white_alpha(34)),
                );

                // 공간을 잡아 이 Area의 **레이어가 휠 영역을 덮게** 합니다 —
                // 휠 위에서는 캔버스 response.hovered()가 false가 되어
                // OS 커서가 표시됩니다 (닙 커서가 휠 위에 안 그려짐).
                ui.allocate_space(egui::vec2(WHEEL_BACK_R * 2.0, WHEEL_BACK_R * 2.0));
            });

        let Some(pos) = frame_tap_pos(ctx) else {
            return;
        };
        match wheel.hit(pos) {
            WheelHit::Center => self.color_wheel_open = false, // 변경 없이 닫기.
            WheelHit::Swatch(i) => {
                if let Some(color) = wheel.ring.get(i).copied() {
                    self.apply_wheel_color(color);
                }
                self.color_wheel_open = false;
            }
            WheelHit::Backplate => self.color_wheel_open = false, // 그냥 닫기.
            WheelHit::Outside => {} // 바깥 탭은 handle_canvas_input 가드가 닫습니다.
        }
    }

    /// 원형 팔레트의 화면 중심 — 펜 위치(버튼을 누른 순간의 포인터)를
    /// 캔버스 안으로 클램프합니다 (캔버스가 휠보다 작으면 캔버스 중심).
    pub(crate) fn color_wheel_center(&self, canvas: Rect) -> Pos2 {
        ColorWheel::clamp_center(
            egui::pos2(
                canvas.min.x + self.color_wheel_anchor[0],
                canvas.min.y + self.color_wheel_anchor[1],
            ),
            canvas,
        )
    }

    /// 페이지 내비게이션 오버레이: Prev/Next, 줌, Fit Width/Height를
    /// 캔버스 중앙 하단에 반투명하게 고정 표시합니다.
    pub(crate) fn canvas_nav_overlay(&mut self, ctx: &egui::Context, canvas: Rect) {
        let page_count = self.document.as_ref().map(|d| d.page_count()).unwrap_or(0);
        let can_prev = self.current_page > 0;
        let can_next = self.current_page + 1 < page_count;
        let fill = crate::theme::nord::semantic::overlay_bg();
        let stroke = crate::theme::nord::semantic::OVERLAY_BORDER;

        // 캔버스 중앙(왼쪽 패널이 열려 있어도)에 정렬되도록 화면 중앙 대비 오프셋.
        let screen = ctx.input(|i| i.raw.screen_rect).unwrap_or(canvas);
        let dx = canvas.center().x - screen.center().x;

        egui::Area::new(egui::Id::new("canvas_nav_overlay"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(dx, -12.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(fill)
                    .stroke(Stroke::new(1.0, stroke))
                    .corner_radius(8.0)
                    .inner_margin(egui::Margin::same(5))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                            if ui
                                .add_enabled(
                                    can_prev,
                                    egui::Button::new(icon_text(ui, "Prev", icons::CARET_LEFT)),
                                )
                                .on_hover_text("Previous page")
                                .clicked()
                            {
                                self.prev_page();
                            }
                            let mut page_num = self.current_page + 1;
                            if ui
                                .add(
                                    egui::DragValue::new(&mut page_num)
                                        .range(1..=page_count.max(1)),
                                )
                                .on_hover_text("Page number")
                                .changed()
                            {
                                self.goto_page(page_num.saturating_sub(1));
                            }
                            ui.label(format!("/ {}", page_count.max(1)));
                            if ui
                                .add_enabled(
                                    can_next,
                                    egui::Button::new(icon_text(ui, "Next", icons::CARET_RIGHT)),
                                )
                                .on_hover_text("Next page")
                                .clicked()
                            {
                                self.next_page();
                            }
                            ui.separator();
                            if ui
                                .add_enabled(
                                    !self.zoom_lock,
                                    egui::Button::new(icon_text(
                                        ui,
                                        "",
                                        icons::MAGNIFYING_GLASS_MINUS,
                                    )),
                                )
                                .on_hover_text("Zoom out 5% (locked: press the lock or Ctrl+L)")
                                .clicked()
                            {
                                self.zoom_by(1.0 / ZOOM_STEP);
                            }
                            ui.label(format!("{:.0}%", self.view.zoom / ZOOM_100_PERCENT * 100.0));
                            if ui
                                .add_enabled(
                                    !self.zoom_lock,
                                    egui::Button::new(icon_text(
                                        ui,
                                        "",
                                        icons::MAGNIFYING_GLASS_PLUS,
                                    )),
                                )
                                .on_hover_text("Zoom in 5% (locked: press the lock or Ctrl+L)")
                                .clicked()
                            {
                                self.zoom_by(ZOOM_STEP);
                            }
                            // 줌 잠금 토글 — 실수로 줌이 바뀌는 것을 방지합니다.
                            let lock_icon = if self.zoom_lock {
                                icons::LOCK_SIMPLE
                            } else {
                                icons::LOCK_SIMPLE_OPEN
                            };
                            if ui
                                .selectable_label(
                                    self.zoom_lock,
                                    icon_text(ui, "", lock_icon),
                                )
                                .on_hover_text(
                                    if self.zoom_lock {
                                        "Zoom locked — click to unlock (Ctrl+L)"
                                    } else {
                                        "Lock zoom in/out (Ctrl+L)"
                                    },
                                )
                                .clicked()
                            {
                                self.zoom_lock = !self.zoom_lock;
                                self.save_default_session();
                                self.save_session();
                            }
                            ui.separator();
                            if ui
                                .add_enabled(
                                    !self.zoom_lock,
                                    egui::Button::new(icon_text(
                                        ui,
                                        "Fit Width",
                                        icons::ARROWS_HORIZONTAL,
                                    )),
                                )
                                .on_hover_text("Fit width")
                                .clicked()
                            {
                                self.fit_width();
                            }
                            if ui
                                .add_enabled(
                                    !self.zoom_lock,
                                    egui::Button::new(icon_text(
                                        ui,
                                        "Fit Height",
                                        icons::ARROWS_VERTICAL,
                                    )),
                                )
                                .on_hover_text("Fit height")
                                .clicked()
                            {
                                self.fit_height();
                            }
                            ui.separator();
                            // Bookmark ↔ Bookmarked — 아이콘 토글 (툴바에서 이곳으로 이동).
                            let bookmarked = self.store.is_bookmarked(self.current_page);
                            let bm_icon = if bookmarked {
                                icons::BOOKMARK
                            } else {
                                icons::BOOKMARK_SIMPLE
                            };
                            if ui
                                .selectable_label(bookmarked, icon_text(ui, "", bm_icon))
                                .on_hover_text(if bookmarked {
                                    "Remove bookmark from this page"
                                } else {
                                    "Bookmark this page"
                                })
                                .clicked()
                            {
                                self.toggle_bookmark(self.current_page);
                            }
                        });
                    });
            });
    }

    /// 굿노트식 필기구 전용 세로 팔레트: 캔버스 오른쪽 중앙에 도구 선택과
    /// 자주 쓰는 색상(즐겨찾기)을 반투명 오버레이로 띄웁니다.
    pub(crate) fn canvas_palette_overlay(&mut self, ctx: &egui::Context, canvas: Rect) {
        if !self.show_palette || self.document.is_none() {
            return;
        }
        let fill = crate::theme::nord::semantic::overlay_bg();
        let stroke = crate::theme::nord::semantic::OVERLAY_BORDER;
        // 캔버스 오른쪽 끝에 붙도록 화면 대비 오프셋.
        let screen = ctx.input(|i| i.raw.screen_rect).unwrap_or(canvas);
        let dx = canvas.right() - screen.right() - 14.0;

        let mut to_add = false;
        let mut to_remove: Option<usize> = None;

        egui::Area::new(egui::Id::new("canvas_palette_overlay"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::RIGHT_CENTER, egui::vec2(dx, 0.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(fill)
                    .stroke(Stroke::new(1.0, stroke))
                    .corner_radius(8.0)
                    .inner_margin(egui::Margin::same(5))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                        // 도구 선택 (세로, 설정된 순서를 따름).
                        let order = self.tool_order.clone();
                        for tool in order {
                            let label = tool.label();
                            if ui
                                .selectable_label(
                                    self.tool == tool,
                                    icon_text(ui, "", tool_icon(tool)),
                                )
                                .on_hover_text(label)
                                .clicked()
                            {
                                self.tool = tool;
                                self.save_session();
                            }
                        }
                        ui.separator();

                        // 현재 펜 색 + 즐겨찾기에 추가 버튼.
                        let cur_rgba = if self.tool == ToolType::Fountain {
                            self.fountain_color
                        } else {
                            self.pen_color
                        };
                        let cur = Color32::from_rgba_unmultiplied(
                            cur_rgba[0],
                            cur_rgba[1],
                            cur_rgba[2],
                            cur_rgba[3],
                        );
                        if color_circle_swatch(ui, "current_color", cur, false)
                            .on_hover_text("Current pen color")
                            .clicked()
                        {
                            self.tool = ToolType::Pen;
                            self.save_session();
                        }
                        let full = self.favorite_colors.len() >= MAX_FAVORITE_COLORS;
                        // "+" 버튼 — 색 스와치들 아래 **중앙 정렬**.
                        ui.vertical_centered(|ui| {
                            if ui
                                .add_enabled(
                                    !full,
                                    egui::Button::new(icon_text(ui, "", icons::PLUS)).frame(false),
                                )
                                .on_hover_text(if full {
                                    format!(
                                        "Palette is full ({MAX_FAVORITE_COLORS} colors) — remove one first"
                                    )
                                } else {
                                    "Add current color to favorites".into()
                                })
                                .clicked()
                            {
                                to_add = true;
                            }
                        });
                        ui.separator();

                        // 자주 쓰는 색상 (클릭 = 적용, 우클릭 = 제거).
                        for i in 0..self.favorite_colors.len() {
                            let c = self.favorite_colors[i]; // Copy
                            let col = Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]);
                            let selected = if self.tool == ToolType::Fountain {
                                self.fountain_color == c
                            } else {
                                self.pen_color == c
                            };
                            let resp = color_circle_swatch(ui, ("fav_swatch", i), col, selected);
                            if resp
                                .clone()
                                .on_hover_text("Set pen color (right-click to remove)")
                                .clicked()
                            {
                                if self.tool == ToolType::Fountain {
                                    self.fountain_color = c;
                                } else {
                                    self.pen_color = c;
                                    self.tool = ToolType::Pen;
                                }
                                self.save_default_session();
                                self.save_session();
                            }
                            if resp.secondary_clicked() {
                                to_remove = Some(i);
                            }
                        }
                    });
            });

        if to_add {
            let c = self.pen_color;
            if !self.favorite_colors.contains(&c) && self.favorite_colors.len() < MAX_FAVORITE_COLORS {
                self.favorite_colors.push(c);
                self.save_default_session();
            }
        }
        if let Some(i) = to_remove {
            if i < self.favorite_colors.len() {
                self.favorite_colors.remove(i);
                self.save_default_session();
            }
        }
    }
}
