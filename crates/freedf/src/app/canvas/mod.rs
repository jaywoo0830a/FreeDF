//! Page canvas — 화면 그리기/입력/오버레이/렌더링의 메인 모듈.
//!
//! 하위 모듈 구성:
//! - [`input`]: 캔버스 입력(팬/줌/필기 시작) 처리
//! - [`ink`]: 획 완료/하이라이트/점 커밋
//! - [`paint`]: 스트로크/용지/잉크 메시/커서 그리기
//! - [`overlays`]: 내비/팔레트/원형 색상 휠 오버레이
//! - [`wheel`]: 원형 색상 휠의 **순수 로직** (레이아웃/히트테스트 — 테스트 대상)
//! - [`tests`]: 단위 테스트

pub(crate) use super::*;

/// 펜 기울기 벡터(도, ±90) → 모델용 0..1 크기.
fn tilt_magnitude(tilt: &[f32; 2]) -> f32 {
    let m = (tilt[0] * tilt[0] + tilt[1] * tilt[1]).sqrt();
    (m / 90.0).min(1.0).max(0.0)
}

/// 펜 틸트(±도) → (방위각 rad, 기울기 코사인).
/// 방위각 0 = 오른쪽(+x), +π/2 = 아래(화면 y — tilt_y 양수가 사용자 쪽일 때).
/// 기울기 0(수직)이면 (0, 1).
fn tilt_azimuth(tilt: &[f32; 2]) -> (f32, f32) {
    let (x, y) = (tilt[0], tilt[1]);
    let mag = (x * x + y * y).sqrt();
    if mag < 1e-3 {
        return (0.0, 1.0);
    }
    let cos_pitch = (mag.min(90.0) * std::f32::consts::PI / 180.0).cos();
    (y.atan2(x), cos_pitch)
}

/// 틸트 소스가 없을 때의 펜 커서 기본 방위각 (rad) — 오른손잡이 관례 위-오른쪽.
const DEFAULT_PEN_AZ: f32 = -0.6;

/// 펜 사이드 버튼으로 여는 굿노트식 **원형 색상 팔레트** 기하 (캔버스 픽셀).
const WHEEL_RING_R: f32 = 34.0;
const WHEEL_SWATCH_R: f32 = 12.0;
const WHEEL_CENTER_R: f32 = 15.0;
const WHEEL_BACK_R: f32 = 56.0;

/// 틸트 노이즈 필터 — 패드 진입 시(호버 시작) 격렬하게 떨리는 틸트 리포트를
/// 무시합니다. 리포트당 최대 변화를 제한하고 EMA로 부드럽게 수렴시킵니다.
fn smooth_tilt(prev: [f32; 2], next: [f32; 2]) -> [f32; 2] {
    const MAX_STEP: f32 = 24.0; // 한 리포트당 최대 변화(도) — 이보다 큰 점프는 잘라냄.
    const ALPHA: f32 = 0.3; // EMA 계수.
    let mut out = prev;
    for i in 0..2 {
        let d = (next[i] - prev[i]).clamp(-MAX_STEP, MAX_STEP);
        out[i] = prev[i] + d * ALPHA;
    }
    out
}

/// 손잡이에 따라 방위각을 유효 반평면으로 되돌립니다 — 오른손잡이는 배럴이
/// 오른쪽(1·4사분면), 왼손잡이는 왼쪽(2·3사분면)에만 머뭅니다. 반대편이면
/// 세로축 대칭으로 접어서 같은 상하 방향의 반대쪽으로 보냅니다.
fn clamp_azimuth_hand(az: f32, left_handed: bool) -> f32 {
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

/// 펜 진단 로그 활성 스위치 — **Debug HUD가 켜져 있을 때만** 로그를 남깁니다
/// (평소에는 로그 파일/콘솔 I/O 비용 0).
static PEN_TRACE_ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 이번 프레임의 탭(펜 터치 시작/마우스 클릭) 좌표 — 이벤트 소스에 무관하게
/// 잡습니다 (Windows Ink 펜은 `Event::Touch`, 마우스는 `PointerButton`로 옴).
fn frame_tap_pos(ctx: &egui::Context) -> Option<Pos2> {
    ctx.input(|i| {
        i.events.iter().find_map(|e| match e {
            egui::Event::PointerButton {
                pos,
                pressed: true,
                ..
            } => Some(*pos),
            egui::Event::Touch {
                phase: egui::TouchPhase::Start,
                pos,
                ..
            } => Some(*pos),
            _ => None,
        })
    })
}

pub(crate) fn set_pen_trace(on: bool) {
    PEN_TRACE_ON.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub(crate) fn pen_trace_on() -> bool {
    PEN_TRACE_ON.load(std::sync::atomic::Ordering::Relaxed)
}

/// 펜 진단 로그 — 필압/폭이 변하지 않을 때 원인을 찾기 위한 흔적.
/// Debug HUD가 켜져 있을 때만 stderr와 `freedf_pendebug.log`에 남깁니다.
fn pen_trace(msg: &str) {
    if !pen_trace_on() {
        return;
    }
    let line = format!("[{}] {msg}", now_ms());
    eprintln!("[pen-trace] {line}");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("freedf_pendebug.log")
    {
        use std::io::Write as _;
        let _ = writeln!(f, "{line}");
    }
}

// ── 캔버스 렌더 = 리본 단일 경로 ──────────────────────────────────────────────
// 진행 중 획과 완성 획은 **같은 지오메트리 생성기**(freedf-core stroke_ribbon)를
// freedf-canvas 경유로 씁니다 — 화면 변환은 아래 `canvas_mesh_to_egui`가 담당.

/// freedf-canvas 메시(페이지 좌표) → egui 메시 (경계 어댑터).
/// 팬/줌은 여기서만 적용됩니다 — 메시는 항상 페이지 좌표로 굽습니다.
fn canvas_mesh_to_egui(
    mesh: &freedf_canvas::Mesh,
    origin: Pos2,
    zoom: f32,
    pan_x: f32,
    pan_y: f32,
) -> egui::Mesh {
    let mut out = egui::Mesh::default();
    for (p, c) in mesh.vertices.iter().zip(&mesh.colors) {
        let x = origin.x + p[0] * zoom + pan_x;
        let y = origin.y + p[1] * zoom + pan_y;
        let a = (c[3] * 255.0).clamp(0.0, 255.0) as u8;
        let col = Color32::from_rgba_unmultiplied(
            (c[0] * 255.0).clamp(0.0, 255.0) as u8,
            (c[1] * 255.0).clamp(0.0, 255.0) as u8,
            (c[2] * 255.0).clamp(0.0, 255.0) as u8,
            a,
        );
        out.vertices
            .push(egui::epaint::Vertex::untextured(egui::pos2(x, y), col));
    }
    out.indices.extend_from_slice(&mesh.indices);
    out
}

impl FreeDfApp {
    pub(crate) fn current_drawing_style(&self) -> ([u8; 4], f32) {
        match self.tool {
            ToolType::Pen => (self.pen_color, self.pen_width),
            ToolType::Fountain => (self.fountain_color, self.fountain_width),
            ToolType::Highlighter => (self.hi_color, self.hi_width),
            _ => ([0, 0, 0, 255], 2.0),
        }
    }

    /// Pen pressure — 우선순위: evdev에서 직접 읽은 필압 → egui Touch force
    /// → (없으면) 풀 필압.
    pub(crate) fn sample_pressure(&self, ctx: &egui::Context) -> f32 {
        self.pressure_source(ctx).0
    }

    /// (압력, 출처) — 진단 로그가 어느 입력이 실제로 쓰였는지 알 수 있게 합니다.
    pub(crate) fn pressure_source(&self, ctx: &egui::Context) -> (f32, &'static str) {
        if !self.pressure_enabled {
            return (1.0, "off(체크박스)");
        }
        if let Some(p) = self.live_pressure {
            return (p.clamp(0.0, 1.0), "pen-monitor");
        }
        if let Some(mon) = &self.pen_monitor {
            if let Some(p) = mon.snapshot().pressure {
                return (p.clamp(0.0, 1.0), "pen-monitor(스냅샷)");
            }
        }
        let force: Option<f32> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Touch { force, .. } => *force,
                    _ => None,
                })
                .last()
        });
        match force {
            Some(f) => (f.clamp(0.0, 1.0), "egui-touch"),
            None => (1.0, "없음(1.0 고정)"),
        }
    }

    pub(crate) fn ensure_texture(&mut self, ctx: &egui::Context) {
        let Some(doc) = &self.document else {
            return;
        };
        let ppp = ctx.pixels_per_point();
        let target_w = self.page_size_pts[0] * self.view.zoom * ppp;
        let needs_render = self.render_dirty
            || self.texture.is_none()
            || (self.last_render_zoom - self.view.zoom).abs() / self.view.zoom.max(1e-3) > 0.15
            || (self.last_render_ppp - ppp).abs() > 0.01;

        if !needs_render {
            return;
        }

        match doc.render_page(self.current_page, target_w, 4096.0 * ppp) {
            Ok(rendered) => {
                let img = egui::ColorImage::from_rgba_unmultiplied(
                    [rendered.width, rendered.height],
                    &rendered.rgba,
                );
                if let Some(t) = self.texture.as_mut() {
                    t.set(img, egui::TextureOptions::LINEAR);
                } else {
                    self.texture =
                        Some(ctx.load_texture("page", img, egui::TextureOptions::LINEAR));
                }
                self.last_render_zoom = self.view.zoom;
                self.last_render_ppp = ppp;
                self.render_dirty = false;
            }
            Err(e) => self.status = Some(format!("Render error: {e}")),
        }
    }

    // ---------- Search ----------

    pub(crate) fn canvas(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let canvas = response.rect;
        let origin = canvas.min;
        let canvas_size = [canvas.width(), canvas.height()];
        // Preserve the zoom when the canvas resizes (panel toggles / window
        // resize): re-center the page at the current zoom instead of re-fitting.
        let resized = (self.prev_canvas[0] - canvas_size[0]).abs() > 2.0
            || (self.prev_canvas[1] - canvas_size[1]).abs() > 2.0;
        if self.document.is_some() && self.pending_fit.is_none() && resized {
            // 캔버스 크기가 바뀌어도(패널/툴바/상태바 펼침·접힘, Hide/Show UI)
            // 페이지가 **화면상 같은 자리**에 남도록 origin 이동분만큼 pan을
            // 보정합니다. (이전의 중앙 재정렬은 필기 중 화면이 슥 튀어나가
            // 매우 성가셨음 — 정렬은 Fit Width/Height나 realign으로만.)
            let origin_delta = canvas.min - self.prev_canvas_origin;
            self.view.pan_x -= origin_delta.x;
            self.view.pan_y -= origin_delta.y;
        }
        self.prev_canvas = canvas_size;
        self.prev_canvas_origin = canvas.min;
        self.last_canvas = canvas_size;

        // Background behind the page (canvas surround — 사용자 지정 색)
        let bg = Color32::from_rgba_unmultiplied(
            self.canvas_color[0],
            self.canvas_color[1],
            self.canvas_color[2],
            self.canvas_color[3],
        );
        painter.rect_filled(canvas, egui::CornerRadius::ZERO, bg);

        if self.document.is_none() {
            ui.painter_at(canvas).text(
                canvas.center(),
                egui::Align2::CENTER_CENTER,
                "Open a PDF or create a note to start annotating (Ctrl+O)",
                egui::TextStyle::Heading.resolve(ui.style()),
                ui.visuals().weak_text_color(),
            );
            return;
        }

        // Apply pending fit + render cache
        self.apply_pending_fit(canvas_size);
        // 세션 복원 직후 — 저장된 pan 대신 현재 정렬 설정으로 재정렬 (줌 유지).
        if self.pending_align && canvas_size[0] > 1.0 && canvas_size[1] > 1.0 {
            self.pending_align = false;
            self.view
                .align_page(self.page_size_pts, canvas_size, TOP_MARGIN, self.page_align);
            self.render_dirty = true;
        }
        self.ensure_texture(&ctx);

        // evdev/OTD 펜 입력 폴링 — egui가 노출하지 않는 틸트/필압 공급원.
        let pen_state = self.pen_monitor.as_mut().and_then(|mon| mon.poll());
        if let Some(st) = &pen_state {
            if self.last_pen_state_ms.is_none() {
                pen_trace(&format!(
                    "pen stream 연결됨: tilt=[{:+.0}, {:+.0}] pressure={:?} contact={} b1={} b2={}",
                    st.tilt[0], st.tilt[1], st.pressure, st.contact, st.buttons.button1, st.buttons.button2
                ));
            }
            self.last_pen_state_ms = Some(now_ms());
            // 패드 진입 시 격렬한 틸트 노이즈를 필터링 (점프 제한 + EMA).
            self.pen_tilt = smooth_tilt(self.pen_tilt, st.tilt);
            self.live_pressure = st.pressure;
            // 사이드 버튼 에지(눌림) 감지 → `on_pen_button` 훅으로 라우팅.
            let prev = self.pen_buttons;
            self.pen_buttons = st.buttons;
            // ── 창 간 격리: 두 창이 같은 펜 장치(evdev/OTD)를 공유하므로,
            // **포커스된 창만** 사이드 버튼에 반응합니다 — 배경 창의 휠이
            // 함께 열리는 버그를 막습니다 (순수 판정: wheel_toggle_allowed).
            if wheel_toggle_allowed(ctx.input(|i| i.viewport().focused)) {
                if st.buttons.button1 && !prev.button1 {
                    // 펜 위치(버튼을 누른 순간의 포인터, 없으면 캔버스 중심)에 엽니다.
                    if !self.color_wheel_open {
                        self.color_wheel_anchor = ctx
                            .input(|i| i.pointer.hover_pos())
                            .map(|p| [p.x - origin.x, p.y - origin.y])
                            .unwrap_or([canvas_size[0] * 0.5, canvas_size[1] * 0.5]);
                    }
                    self.on_pen_button(1, true);
                }
                if st.buttons.button2 && !prev.button2 {
                    self.on_pen_button(2, true);
                }
            }
        }
        // 입력 소스(펜/마우스/트랙패드) 추정 갱신 — 판정 규칙은 hooks.rs.
        self.input_sources.update(
            &ctx,
            pen_state.as_ref(),
            self.last_pen_state_ms,
            now_ms(),
        );

        // ── 스플릿 뷰 포커스 제스처 ──────────────────────────────────────
        // 펜(OTD/evdev)이 우리 창 위를 호버 중인데 포커스가 없으면 한 번만
        // 포커스를 요청합니다 — 빈 노트에 점을 찍어야만 포커스가 잡히던
        // 불편을 없앱니다. (첫 탭 가드는 handle_canvas_input에 있음)
        let pen_alive = self
            .last_pen_state_ms
            .is_some_and(|t| now_ms().saturating_sub(t) < 1000);
        let hovered_over_canvas = ctx
            .input(|i| i.pointer.hover_pos())
            .is_some_and(|pos| canvas.contains(pos));
        if pen_alive && hovered_over_canvas {
            if ctx.input(|i| i.viewport().focused == Some(false)) {
                if !self.focus_grabbed {
                    self.focus_grabbed = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
            } else if ctx.input(|i| i.viewport().focused != Some(false)) {
                self.focus_grabbed = false;
            }
        }

        // ---------- Input ----------
        // 이번 프레임에 뷰포트 포커스가 false→true로 바뀌었는지 기록. 그 전환이
        // **포인터 프레스와 동시에** 일어났다면 그 탭이 포커스를 만든 것이므로
        // 짧은 잉크 유예를 겁니다 (호버 포커스 전환은 프레스가 없어 유예 없음).
        let focused_now = ctx.input(|i| i.viewport().focused);
        let pressed_now = ctx.input(|i| i.pointer.any_pressed());
        if self.prev_viewport_focused == Some(false) && focused_now != Some(false) && pressed_now {
            self.focus_grace_until_ms = Some(now_ms().saturating_add(400));
        }
        self.prev_viewport_focused = focused_now;
        self.handle_canvas_input(&ctx, &response, origin, canvas_size);
        // Keep the page within the canvas (no infinite panning)
        // — 문서 바깥 여유(overscroll)는 설정값을 따릅니다.
        self.view
            .clamp_pan(self.page_size_pts, canvas_size, self.edge_overscroll);

        // ── 엣지 자동 스크롤 ───────────────────────────────────────────
        // 줌인 상태에서 커서(펜 호버 포함)가 캔버스 가장자리 근처에 닿으면
        // 그 방향으로 뷰를 자동 패닝합니다. 경계에서 0, 가장자리에서 최대
        // 속도로 부드럽게 증가합니다. 방향별 반응 지연(delay)과 "숨쉬는"
        // 표시(pulse)는 설정 창에서 조정합니다.
        if self.edge_autoscroll {
            let pos = ctx.input(|i| i.pointer.hover_pos());
            if let Some(pos) = pos {
                // 팔레트/하단 플로팅 바 등 캔버스 위 UI가 포인터를 가로채면
                // response.hovered()가 false — 이때는 발동 금지. (커스텀
                // 커서가 이미 검증한 신호: 오버레이 Area가 위에 있으면 false)
                // 그리고 "펜을 쓰는 중일 때만"(설정)이 켜져 있으면 단순
                // 마우스/트랙패드 커서는 무시합니다 (판정: input_sources).
                let pen_ok = !self.edge_autoscroll_pen_only
                    || self.input_sources.is_pen_in_use(now_ms())
                    || self.input_sources.pen_undetectable();
                if response.hovered() && canvas.contains(pos) && pen_ok {
                    let zone = self.edge_zone.clamp(8.0, 300.0);
                    let sp = [
                        self.edge_speeds[0].clamp(20.0, 4000.0),
                        self.edge_speeds[1].clamp(20.0, 4000.0),
                        self.edge_speeds[2].clamp(20.0, 4000.0),
                        self.edge_speeds[3].clamp(20.0, 4000.0),
                    ];
                    let t = |d: f32| (1.0 - d / zone).max(0.0);
                    let raw = [
                        t(pos.x - canvas.left()),   // 좌측 가장자리
                        t(canvas.right() - pos.x),  // 우측 가장자리
                        t(pos.y - canvas.top()),    // 위쪽 가장자리
                        t(canvas.bottom() - pos.y), // 아래쪽 가장자리
                    ];
                    // 방향별 반응 지연 — 가장자리에 머문 시간이 delay를 넘어야
                    // 스크롤이 시작됩니다 (delay 0 = 즉시).
                    let now = now_ms();
                    let mut te = [0.0f32; 4];
                    for i in 0..4 {
                        if raw[i] > 0.0 {
                            if self.edge_zone_enter_ms[i] == 0 {
                                self.edge_zone_enter_ms[i] = now;
                            }
                            let delay_ms = (self.edge_delays[i].clamp(0.0, 3.0) * 1000.0) as u64;
                            if now.saturating_sub(self.edge_zone_enter_ms[i]) >= delay_ms {
                                te[i] = raw[i];
                            }
                        } else {
                            self.edge_zone_enter_ms[i] = 0;
                        }
                    }
                    self.edge_glow = raw; // 숨쉬는 글로우는 지연과 무관하게 즉시.
                    let dt = ctx.input(|i| i.stable_dt).clamp(0.0, 0.1);
                    let mut dx = 0.0f32;
                    let mut dy = 0.0f32;
                    dx += te[0] * sp[0] * dt;
                    dx -= te[1] * sp[1] * dt;
                    dy += te[2] * sp[2] * dt;
                    dy -= te[3] * sp[3] * dt;
                    if dx != 0.0 || dy != 0.0 {
                        self.view.pan_x += dx;
                        self.view.pan_y += dy;
                        self.view
                            .clamp_pan(self.page_size_pts, canvas_size, self.edge_overscroll);
                    }
                } else {
                    self.edge_glow = [0.0; 4];
                    self.edge_zone_enter_ms = [0; 4];
                }
            } else {
                self.edge_glow = [0.0; 4];
                self.edge_zone_enter_ms = [0; 4];
            }
        }

        // Advance the page transition animation
        let mut animating = false;
        if let Some(anim) = &mut self.page_anim {
            let dt = ctx.input(|i| i.stable_dt).max(1e-4);
            anim.progress += dt / PAGE_ANIM_SECS;
            animating = anim.progress < 1.0;
            if !animating {
                self.page_anim = None;
                self.prev_texture = None;
                // 애니메이션 종료 → 다음 페이지를 미리 렌더.
                self.prefetch_pending = true;
            }
        }
        if animating {
            ctx.request_repaint();
        } else if self.prefetch_pending {
            // 다음/이전 페이지 텍스처 프리페치 (CPU 래스터 대기 제거).
            self.prefetch_page(&ctx);
        }

        // ---------- Draw ----------
        let page_view = self.view.page_size_to_view(self.page_size_pts[0], self.page_size_pts[1]);
        let page_rect = Rect::from_min_size(
            origin + Vec2::new(self.view.pan_x, self.view.pan_y),
            Vec2::new(page_view[0], page_view[1]),
        );

        // Paper color tint applied to the page image (colored paper).
        // **노트에만 적용** — 스탠드얼론 PDF는 원본 배경색을 유지합니다.
        let paper = self.current_page_paper();
        let paper_tint = if self.current_note.is_some() {
            Color32::from_rgba_unmultiplied(
                paper.color[0],
                paper.color[1],
                paper.color[2],
                255,
            )
        } else {
            Color32::WHITE
        };

        // During a transition, draw the outgoing + incoming pages sliding.
        // PgUp/PgDn 키 전환은 세로(위/아래)로, 그 외(내비게이션/휠/화살표)는
        // 기존처럼 가로로 슬라이드합니다.
        let mut anim_dx = 0.0_f32;
        let mut anim_dy = 0.0_f32;
        if let (Some(anim), Some(prev)) = (&self.page_anim, &self.prev_texture) {
            let dir = anim.direction;
            let p = anim.progress;
            let span = if anim.vertical {
                page_rect.height()
            } else {
                page_rect.width()
            };
            let old_off = -p * dir * span;
            let new_off = (1.0 - p) * dir * span;
            let old_vec = if anim.vertical {
                Vec2::new(0.0, old_off)
            } else {
                Vec2::new(old_off, 0.0)
            };
            let new_vec = if anim.vertical {
                Vec2::new(0.0, new_off)
            } else {
                Vec2::new(new_off, 0.0)
            };
            anim_dx = new_vec.x;
            anim_dy = new_vec.y;

            let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
            // Outgoing page (old texture)
            painter.rect_filled(
                page_rect.translate(old_vec).expand(6.0),
                egui::CornerRadius::same(4),
                Color32::from_black_alpha(70),
            );
            painter.image(
                prev.id(),
                page_rect.translate(old_vec),
                uv,
                paper_tint,
            );
            // Incoming page (new texture)
            painter.rect_filled(
                page_rect.translate(new_vec).expand(6.0),
                egui::CornerRadius::same(4),
                Color32::from_black_alpha(70),
            );
            if let Some(tex) = &self.texture {
                painter.image(
                    tex.id(),
                    page_rect.translate(new_vec),
                    uv,
                    paper_tint,
                );
            }
        }

        // Current-page rect/origin (shifted during a transition so border & ink follow)
        let draw_rect = page_rect.translate(Vec2::new(anim_dx, anim_dy));
        let draw_origin = origin + Vec2::new(anim_dx, anim_dy);

        // Page shadow, image and border (single page when not mid-transition)
        if self.page_anim.is_none() {
            painter.rect_filled(
                draw_rect.expand(6.0),
                egui::CornerRadius::same(4),
                Color32::from_black_alpha(70),
            );
            if let Some(tex) = &self.texture {
                painter.image(
                    tex.id(),
                    draw_rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    paper_tint,
                );
            }
            painter.rect_stroke(
                draw_rect,
                egui::CornerRadius::same(2),
                Stroke::new(1.0, Color32::from_gray(120)),
                egui::StrokeKind::Inside,
            );
            // 종이 질감 — 페이지 위, 잉크/줄보다 아래 (설정: Paper settings).
            self.paint_paper_texture(&ctx, &painter, draw_rect);
            // Paper grid / ruling (only for notes)
            if self.current_note.is_some() {
                self.paint_paper(&painter, draw_origin);
            }
        }

        // Search highlights (under ink so annotations stay readable)
        self.paint_search_highlights(&painter, draw_origin);

        let now = now_ms();
        let rev = self.store.rev();
        let count = self.store.stroke_count_on(self.current_page);
        // 병합 메시는 **정착된 획만** 담고, 스밈(진해짐)이 진행 중인 젊은
        // 획은 오버레이가 매 프레임 현재 나이로 재굽기합니다 (O(젊은 획) —
        // 전체 재굽기 불필요, 펜업 순간에도 끊김 없음). 북킹 규칙은
        // freedf-canvas의 순수 계약 `soak::InkSettling`이 담당합니다.
        if let Some(from) = self
            .ink_settling
            .new_from(self.current_page, count, rev)
        {
            // 방금 끝난 획 → 젊은 목록 (오버레이가 즉시 그려줘 깜빡임 없음).
            let new_strokes = self.store.strokes_on(self.current_page)[from..].to_vec();
            self.add_ink_young(&new_strokes, now);
        }
        // 정착된 젊은 획을 병합 메시로 이동 (최종 알파, 획당 1회).
        self.sweep_ink_young(now);
        let full_needed = self.ink_needs_rebuild()
            || self.ink_settling.deleted(self.current_page, count, rev);
        if full_needed && self.ink_bake_pending.is_none() {
            *self.ink_baker_mesher.write().expect("bake mesher lock") = self.core_mesher();
            let mesher = self.core_mesher();
            // 스냅샷은 **정착된 획만** — 젊은 획은 오버레이가 담당하므로
            // 결과 메시와 오버레이가 겹치지 않습니다.
            let strokes: Vec<freedf_canvas::Stroke> = self
                .store
                .strokes_on(self.current_page)
                .iter()
                .filter(|s| mesher.next_settle(&canvas_stroke(s), now) == u64::MAX)
                .map(canvas_stroke)
                .collect();
            let snapshot = freedf_canvas::SceneSnapshot {
                revision: freedf_canvas::Revision(rev),
                strokes,
            };
            if self
                .ink_baker
                .request(
                    snapshot,
                    freedf_canvas::BakeParams {
                        zoom: self.view.zoom,
                    },
                    now,
                )
                .is_ok()
            {
                self.ink_bake_pending =
                    Some((self.current_page, self.store_generation, rev, count, self.view.zoom));
            }
        }
        // ── 백그라운드 전체 굽기 결과 수신 (매 프레임 try_recv) ──
        if let Some(result) = self.ink_baker.poll() {
            let pending = self.ink_bake_pending.take();
            match result {
                Ok(page) => {
                    let (req_page, req_gen, _req_rev, req_count, req_zoom) = pending
                        .unwrap_or((usize::MAX, u64::MAX, 0, 0, self.view.zoom));
                    let cur_rev = self.store.rev();
                    let cur_count = self.store.stroke_count_on(self.current_page);
                    let zoom_ok = (req_zoom - self.view.zoom).abs()
                        / self.view.zoom.max(1e-3)
                        <= 0.15;
                    if req_page != self.current_page
                        || req_gen != self.store_generation
                        || !zoom_ok
                        || (cur_rev != page.revision.0 && cur_count <= req_count)
                    {
                        // 문서/페이지/줌 변경 또는 삭제가 섞임 — 폐기 후
                        // 다음 프레임에 다시 요청합니다.
                    } else {
                        // 요청 시점엔 젊어서 스냅샷에 없었지만 도중에 정착된
                        // 획을 결과 메시에 붙여 유실을 막습니다.
                        let mesher = self.core_mesher();
                        let mut mesh = page.mesh;
                        let late: Vec<freedf_canvas::Stroke> = self
                            .ink_settling
                            .young
                            .iter()
                            .filter(|s| mesher.next_settle(s, now) == u64::MAX)
                            .cloned()
                            .collect();
                        for s in &late {
                            mesher.append_stroke(&mut mesh, s, now);
                        }
                        self.install_ink_mesh(mesh, cur_rev, cur_count, now);
                    }
                }
                Err(freedf_canvas::BakeError::Busy) => {
                    self.ink_bake_pending = pending; // 다음 프레임 재수신.
                }
                Err(freedf_canvas::BakeError::WorkerStopped) => {
                    // 워커 종료 — 다음 프레임의 request가 다시 Err를 반환해 무해.
                }
            }
        }
        // 스밈 애니메이션/굽기 대기 동안 연속 repaint — egui는 이벤트 기반이라
        // 요청하지 않으면 프레임이 멈춰 잉크가 옅은 채로 남습니다.
        if self.ink_bake_pending.is_some()
            || self.active_stroke.is_some()
            || !self.ink_settling.young.is_empty()
        {
            ctx.request_repaint();
        }
        if let Some(mesh) = &self.ink_mesh {
            // 페이지 좌표 → 화면 좌표 변환은 **팬/줌/페이지가 바뀔 때만**
            // (O(V)) — 팬/엣지 스크롤 중엔 변환된 egui 메시를 재사용해
            // 프레임 비용이 스트로크 수와 무관하게 유지됩니다.
            let key = (
                self.current_page,
                self.view.zoom,
                self.view.pan_x,
                self.view.pan_y,
            );
            if self.ink_egui_key != Some(key) {
                self.ink_egui_mesh = Some(std::sync::Arc::new(canvas_mesh_to_egui(
                    mesh,
                    origin,
                    self.view.zoom,
                    self.view.pan_x,
                    self.view.pan_y,
                )));
                self.ink_egui_key = Some(key);
            }
            if let Some(mesh) = &self.ink_egui_mesh {
                // 페이지 전환 애니메이션 오프셋은 매 프레임 정점 이동.
                let shifted = anim_dx.abs() > 0.5 || anim_dy.abs() > 0.5;
                if shifted {
                    let mut m = (**mesh).clone();
                    let shift = egui::vec2(anim_dx, anim_dy);
                    for v in &mut m.vertices {
                        v.pos += shift;
                    }
                    painter.add(egui::Shape::mesh(std::sync::Arc::new(m)));
                } else {
                    painter.add(egui::Shape::mesh(mesh.clone()));
                }
            }
        }
        // 젊은(스밈 진행 중) 획 오버레이 — 매 프레임 현재 나이로 재굽어
        // 부드러운 진해짐 애니메이션. O(젊은 획)라 펜업 직후에도 끊김이
        // 없고, 정착분 병합 메시는 건드리지 않아 egui 변환 캐시도 유지됩니다.
        if !self.ink_settling.young.is_empty()
            && self.ink_settling.page == Some(self.current_page)
        {
            let mesher = self.core_mesher();
            let mut cm = freedf_canvas::Mesh::default();
            for s in self.ink_settling.young.clone() {
                mesher.append_stroke(&mut cm, &s, now);
            }
            let mut ym = canvas_mesh_to_egui(
                &cm,
                origin,
                self.view.zoom,
                self.view.pan_x,
                self.view.pan_y,
            );
            if anim_dx.abs() > 0.5 || anim_dy.abs() > 0.5 {
                let shift = egui::vec2(anim_dx, anim_dy);
                for v in &mut ym.vertices {
                    v.pos += shift;
                }
            }
            painter.add(egui::Shape::mesh(std::sync::Arc::new(ym)));
        }
        if let Some(active) = self.active_stroke.clone() {
            self.paint_active(&painter, &active, draw_origin);
        }

        // Tool cursor — custom sprite only when the pointer is actually over
        // the canvas (not covered by a floating overlay, not over a side
        // panel) *and* inside the page rect. `response.hovered()` is false when
        // an overlay Area sits on top, and false outside the canvas rect — so
        // the OS cursor is always restored elsewhere (it used to disappear:
        // `draw_rect` could extend past the canvas / under overlays and then
        // `CursorIcon::None` hid the pointer with no custom sprite drawn).
        let pointer_pos = ctx.input(|i| i.pointer.hover_pos());
        let over_page = pointer_pos
            .is_some_and(|pos| canvas.contains(pos) && draw_rect.contains(pos));
        // 히스테리시스: 커서 상태가 무한히 바뀌어도 커스텀 커서가 시스템 커서
        // 위에 겹쳐 깜빡이지 않도록, 같은 want가 3프레임 연속일 때만 전환
        // (순수 판정: cursor_hysteresis — 테스트로 검증).
        let want_custom = response.hovered() && over_page;
        let (counter, show) = cursor_hysteresis(
            self.cursor_prev_want,
            want_custom,
            self.cursor_custom_counter,
            self.cursor_custom_shown,
            3,
        );
        self.cursor_prev_want = want_custom;
        self.cursor_custom_counter = counter;
        self.cursor_custom_shown = show;
        if show {
            ctx.set_cursor_icon(egui::CursorIcon::None);
            let time = ctx.input(|i| i.time) as f32;
            if let Some(pos) = pointer_pos {
                self.paint_custom_cursor(&painter, pos, time);
            }
        } else {
            ctx.set_cursor_icon(egui::CursorIcon::Default);
        }

        // ── 엣지 스크롤 숨쉬는 글로우 (맨 위 레이어) ──
        if self.edge_pulse && self.edge_glow.iter().any(|&s| s > 0.0) {
            let time = ctx.input(|i| i.time) as f32;
            let breathe = 0.5 + 0.5 * (time * std::f32::consts::TAU * 0.8).sin();
            let base = crate::theme::nord::semantic::ACCENT_ACTIVE;
            let bw = 10.0;
            let band = |s: f32| base.gamma_multiply(0.08 + 0.18 * s * breathe);
            if self.edge_glow[0] > 0.0 {
                painter.rect_filled(
                    egui::Rect::from_min_size(
                        egui::pos2(canvas.left(), canvas.top()),
                        egui::vec2(bw, canvas.height()),
                    ),
                    egui::CornerRadius::ZERO,
                    band(self.edge_glow[0]),
                );
            }
            if self.edge_glow[1] > 0.0 {
                painter.rect_filled(
                    egui::Rect::from_min_size(
                        egui::pos2(canvas.right() - bw, canvas.top()),
                        egui::vec2(bw, canvas.height()),
                    ),
                    egui::CornerRadius::ZERO,
                    band(self.edge_glow[1]),
                );
            }
            if self.edge_glow[2] > 0.0 {
                painter.rect_filled(
                    egui::Rect::from_min_size(
                        egui::pos2(canvas.left(), canvas.top()),
                        egui::vec2(canvas.width(), bw),
                    ),
                    egui::CornerRadius::ZERO,
                    band(self.edge_glow[2]),
                );
            }
            if self.edge_glow[3] > 0.0 {
                painter.rect_filled(
                    egui::Rect::from_min_size(
                        egui::pos2(canvas.left(), canvas.bottom() - bw),
                        egui::vec2(canvas.width(), bw),
                    ),
                    egui::CornerRadius::ZERO,
                    band(self.edge_glow[3]),
                );
            }
            ctx.request_repaint(); // 숨쉬는 애니메이션 유지.
        }

        // Debug HUD — 실시간 입력값 확인용 오버레이.
        if self.debug_hud {
            self.paint_debug_hud(&ctx, origin);
        }

        // Zoom hint
        if self.document.is_some() && self.view.zoom >= 4.0 {
            painter.text(
                canvas.left_top() + Vec2::new(10.0, 10.0),
                egui::Align2::LEFT_TOP,
                "Ctrl+wheel: zoom / wheel: scroll & page / middle button: pan",
                egui::TextStyle::Small.resolve(ui.style()),
                ui.visuals().text_color(),
            );
        }

        // Floating page navigation overlay (bottom-center, semi-transparent).
        self.canvas_nav_overlay(&ctx, canvas);
        // Floating writing-tool / color palette (right-center of the canvas).
        self.canvas_palette_overlay(&ctx, canvas);
        // 펜 사이드 버튼으로 여는 굿노트식 원형 색상 팔레트.
        self.color_wheel_overlay(&ctx, canvas);
        // 사전 오버레이 (단어 탭 조회 결과).
        self.dict_overlay(&ctx);
    }

    /// 다음(또는 이전) 페이지를 미리 렌더해 둡니다 — 페이지 전환 시
    /// CPU 래스터 대기를 없애 부드럽게 넘어갑니다.
    pub(crate) fn prefetch_page(&mut self, ctx: &egui::Context) {
        self.prefetch_pending = false;
        let Some(doc) = &self.document else {
            return;
        };
        let next = if self.current_page + 1 < doc.page_count() {
            self.current_page + 1
        } else if self.current_page > 0 {
            self.current_page - 1
        } else {
            return;
        };
        // 이미 같은 페이지를 같은 줌으로 프리페치해 두었으면 스킵.
        if let Some((p, z, _)) = &self.prefetch {
            if *p == next && (*z - self.view.zoom).abs() < 1e-3 {
                return;
            }
        }
        let size = doc.page_size_pts(next);
        let ppp = ctx.pixels_per_point();
        let target_w = size[0] * self.view.zoom * ppp;
        if let Ok(rendered) = doc.render_page(next, target_w, 4096.0 * ppp) {
            let img = egui::ColorImage::from_rgba_unmultiplied(
                [rendered.width, rendered.height],
                &rendered.rgba,
            );
            self.prefetch = Some((
                next,
                self.view.zoom,
                ctx.load_texture("prefetch", img, egui::TextureOptions::LINEAR),
            ));
        }
    }
}

mod ink;
mod input;
mod overlays;
mod paint;
mod wheel;
pub(crate) use paint::{canvas_stroke, InkBakeWorker};
#[cfg(test)]
mod tests;

// 원형 색상 휠의 순수 로직을 하위 모듈/테스트에서 편하게 쓰도록 재노출.
pub(crate) use wheel::{ColorWheel, WheelHit};
