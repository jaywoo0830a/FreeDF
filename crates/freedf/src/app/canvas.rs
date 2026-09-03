//! Page canvas: pan/zoom input, page painting, text highlight, palette & nav overlays, custom cursors, page rendering.
//!
//! Extracted from `app.rs` during the layered refactor — these are
//! `FreeDfApp` methods living in a submodule (`use super::*` re-exports the
//! app state types and shared helpers).

use super::*;

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

/// 진행 중 획 지오메트리 재구성 스로틀 (ms) — 20ms면 50Hz. 사람 눈에는
/// 충분히 부드럽고(리본이 O(n)으로 저렴), 재구성 비용을 반으로 아낍니다.
const ACTIVE_STROKE_GEOM_MS: u64 = 20;
/// 번지는 후광(병합 메시) 재구성 스로틀 (ms) — 후광은 느리게 자라므로
/// 20Hz면 충분하고, 페이지 전체 재구성 비용을 아낍니다.
const HALO_GEOM_MS: u64 = 50;

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
// 진행 중 획과 완성 획이 **같은 지오메트리 생성기(stroke_ribbon)** 를 씁니다.
// 과거 완성 획은 귀 자르기 삼각분할(정확 경로)을 썼는데, 곡선 필기는 분할이
// 실패해 폴백(딱딱한 quad, AA 없음)으로 바뀌며 진행 중과 시각 차이가 났음.
// 정확 삼각분할 경로는 과거 PNG 내보내기 전용이었고 지금은 사용처가 없어
// core에 pub API로만 유지됩니다.

/// 리본(근사) 지오메트리를 메시에 덧붙입니다 — 버텍스별 알파 램프
/// (1 = 본체, 0 = 페더 바깥 가장자리)를 베이스 색에 곱해 칠합니다.
fn append_ribbon(
    mesh: &mut egui::Mesh,
    ribbon: &freedf_core::pen::StrokeRibbon,
    to_view: &impl Fn([f32; 2]) -> Pos2,
    color: Color32,
) {
    let base = mesh.vertices.len() as u32;
    for (p, a) in ribbon.verts.iter().zip(&ribbon.alphas) {
        let alpha = (color.a() as f32 * a).clamp(0.0, 255.0) as u8;
        let c = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha);
        mesh.vertices.push(egui::epaint::Vertex::untextured(to_view(*p), c));
    }
    for t in &ribbon.tris {
        mesh.indices
            .extend_from_slice(&[base + t[0], base + t[1], base + t[2]]);
    }
}

/// 점별 절반 두께(pt) — 입력 시점에 잠금된 폭(`StrokePoint.width`)이 있으면
/// 그대로 쓰고, 없으면(이전 데이터) 프로파일 배치 계산으로 폴백합니다.
fn stroke_halves(
    tool: ToolType,
    width: f32,
    points: &[StrokePoint],
    ball: &BallPenProfile,
    fountain: &FountainProfile,
    tilt_mag: f32,
) -> Vec<f32> {
    let n = points.len();
    let locked = !points.is_empty() && points.iter().all(|p| p.width > 0.0);
    if tool == ToolType::Highlighter {
        // 마커: 필압/테이퍼 없이 일정한 두께 (잠금 폭도 동일 규칙).
        let mut halves = Vec::with_capacity(n);
        if locked {
            for p in points {
                halves.push((p.width * 0.5).max(0.5));
            }
        } else {
            halves.resize(n, (width * 0.5).max(0.5));
        }
        return halves;
    }
    let mut halves = Vec::with_capacity(n);
    if locked {
        for p in points {
            // 바닥값은 기하학 퇴화 방지용 최소값 — 예전 0.3pt 바닥은
            // 0.5pt 펜의 폭 변동(절반 0.22~0.30)을 **전부 삼켜** 굵기가
            // 항상 0.6pt로 보였습니다 (사용자 보고 버그의 원인).
            halves.push((p.width * 0.5).max(0.05));
        }
        return halves;
    }
    if tool == ToolType::Fountain {
        for w in fountain.widths(width, points, tilt_mag) {
            halves.push((w * 0.5).max(0.05));
        }
    } else {
        for w in ball.widths(width, points, tilt_mag) {
            halves.push((w * 0.5).max(0.05));
        }
    }
    halves
}

/// 획별 질감 시드 — 획 시작 시각에서 유도해 **같은 획은 항상 같은 질감**,
/// 다른 획은 거의 항상 다른 질감을 갖습니다 (질감이 프레임마다 깜빡이지 않음).
fn ink_seed(grain: InkGrain, created_ms: u64) -> InkGrain {
    InkGrain {
        seed: grain.seed ^ created_ms.wrapping_mul(0x9E37_79B9_7F4A_7C15),
        ..grain
    }
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
                self.color_wheel_open = !self.color_wheel_open;
                if self.color_wheel_open {
                    self.color_wheel_opened_at = now_ms();
                }
                self.status = Some(if self.color_wheel_open {
                    "Color wheel open — tap a color".into()
                } else {
                    "Color wheel closed".into()
                });
            }
            2 => {
                // 예약 — 펜 색 변경 등 추가 액션을 여기에 연결합니다.
            }
            _ => {}
        }
    }

    /// 원형 팔레트에서 고른 색을 **현재 잉크 도구**에 적용합니다.
    /// (팬/지우개 상태에서 고르면 펜 도구로 전환 — 굿노트 관례)
    fn apply_wheel_color(&mut self, color: [u8; 4]) {
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
    /// 펜 위치(클램프 후)에 표시: 중앙 = 현재 색, 둘레 = 사용자가 지정한
    /// 팔레트(즐겨찾기). 탭하면 적용+닫힘, 4초간 입력 없으면 자동으로 닫힙니다.
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
        // 펜 위치(버튼을 누른 순간의 포인터)에 열리되, 캔버스 안으로 클램프.
        let center = self.color_wheel_center(canvas);

        // 둘레 색: **사용자가 지정한 팔레트(즐겨찾기)만** 사용.
        let mut ring = self.favorite_colors.clone();
        if ring.is_empty() {
            ring = crate::settings::SessionState::default().favorite_colors;
        }
        ring.truncate(MAX_FAVORITE_COLORS);
        let current = self.current_drawing_style().0;

        // 탭 좌표 — 포인터 클릭(마우스)과 터치(펜) 양쪽 경로에서 수집합니다.
        let area_pos = center - egui::vec2(WHEEL_BACK_R, WHEEL_BACK_R);
        let mut tap: Option<Pos2> = None;
        egui::Area::new(egui::Id::new("color_wheel"))
            .order(egui::Order::Foreground)
            .fixed_pos(area_pos)
            .show(ctx, |ui| {
                // 주의: `painter_at(rect)`의 rect는 **클립 영역**(화면 좌표) —
                // 좌표 오프셋이 아닙니다. ZERO 기준 rect를 넘기면 원이 화면
                // 좌상단에 그려지므로, 반드시 화면 좌표 rect를 넘깁니다.
                let rect = egui::Rect::from_center_size(
                    center,
                    egui::vec2(WHEEL_BACK_R * 2.0, WHEEL_BACK_R * 2.0),
                );
                let painter = ui.painter_at(rect);
                let c = rect.center();
                let fill = crate::theme::nord::semantic::overlay_bg();
                let stroke = crate::theme::nord::semantic::OVERLAY_BORDER;
                painter.circle_filled(c, WHEEL_BACK_R, fill);
                painter.circle_stroke(c, WHEEL_BACK_R, Stroke::new(1.0, stroke));
                use std::f32::consts::TAU;
                // 둘레 스와치 — 12시 방향부터 시계 방향.
                for (i, color) in ring.iter().enumerate() {
                    let ang = -TAU / 4.0 + TAU * (i as f32) / (ring.len() as f32);
                    let sc = c + egui::vec2(ang.cos(), ang.sin()) * WHEEL_RING_R;
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
                // 중앙 = 현재 색 (탭하면 그냥 닫힘).
                let cc = Color32::from_rgba_unmultiplied(
                    current[0],
                    current[1],
                    current[2],
                    current[3],
                );
                painter.circle_filled(c, WHEEL_CENTER_R, cc);
                painter.circle_stroke(c, WHEEL_CENTER_R, Stroke::new(1.5, Color32::from_gray(120)));

                // 인터랙션 — 이 응답이 휠 영역의 탭을 받아 캔버스로 새지 않게 합니다.
                let local_rect = egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(WHEEL_BACK_R * 2.0, WHEEL_BACK_R * 2.0),
                );
                let resp = ui.interact(
                    local_rect,
                    ui.id().with("color_wheel_hit"),
                    egui::Sense::click(),
                );
                if resp.clicked() {
                    // interact_pointer_pos는 영역 로컬 좌표 → 화면 좌표로 변환.
                    tap = resp.interact_pointer_pos().map(|p| area_pos + p.to_vec2());
                }
            });

        // egui의 clicked()는 포인터(마우스) 경로 전용 — Windows Ink 펜 탭이
        // `Event::Touch`로만 오는 경우를 위해 이벤트에서 직접 판정합니다
        // (클릭이 안 먹던 원인 대응 — 두 경로 모두 지원).
        if tap.is_none() {
            tap = ctx.input(|i| {
                i.events.iter().rev().find_map(|e| match e {
                    egui::Event::Touch {
                        phase: egui::TouchPhase::End,
                        pos,
                        ..
                    } => Some(*pos),
                    egui::Event::PointerButton {
                        pos,
                        pressed: false,
                        ..
                    } => Some(*pos),
                    _ => None,
                })
            });
        }
        let Some(pos) = tap else {
            return;
        };
        if pos.distance(center) <= WHEEL_CENTER_R {
            self.color_wheel_open = false; // 중앙 탭 = 변경 없이 닫기.
            return;
        }
        if pos.distance(center) > WHEEL_BACK_R {
            return; // 바깥 탭은 handle_canvas_input 가드가 닫습니다.
        }
        use std::f32::consts::TAU;
        for (i, color) in ring.iter().enumerate() {
            let ang = -TAU / 4.0 + TAU * (i as f32) / (ring.len() as f32);
            let sc = center + egui::vec2(ang.cos(), ang.sin()) * WHEEL_RING_R;
            if pos.distance(sc) <= WHEEL_SWATCH_R + 3.0 {
                self.apply_wheel_color(*color);
                self.color_wheel_open = false;
                return;
            }
        }
        // 뒷판의 빈 곳 탭 → 그냥 닫기.
        self.color_wheel_open = false;
    }

    /// 원형 팔레트의 화면 중심 — 펜 위치(버튼을 누른 순간의 포인터)를
    /// 캔버스 안으로 클램프합니다 (캔버스가 휠보다 작으면 캔버스 중심).
    fn color_wheel_center(&self, canvas: Rect) -> Pos2 {
        let ax = canvas.min.x + self.color_wheel_anchor[0];
        let ay = canvas.min.y + self.color_wheel_anchor[1];
        if canvas.width() < WHEEL_BACK_R * 2.0 || canvas.height() < WHEEL_BACK_R * 2.0 {
            return canvas.center();
        }
        egui::pos2(
            ax.clamp(canvas.min.x + WHEEL_BACK_R, canvas.max.x - WHEEL_BACK_R),
            ay.clamp(canvas.min.y + WHEEL_BACK_R, canvas.max.y - WHEEL_BACK_R),
        )
    }

    /// Pen pressure — 우선순위: evdev에서 직접 읽은 필압 → egui Touch force
    /// → (없으면) 풀 필압.
    pub(crate) fn sample_pressure(&self, ctx: &egui::Context) -> f32 {
        self.pressure_source(ctx).0
    }

    /// (압력, 출처) — 진단 로그가 어느 입력이 실제로 쓰였는지 알 수 있게 합니다.
    fn pressure_source(&self, ctx: &egui::Context) -> (f32, &'static str) {
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

    /// 실시간 입력 디버그 HUD — 필압/틸트/속도/폭이 실제로 어떻게 들어오는지
    /// 바로 확인할 수 있습니다 (입력 장치가 필압을 보고하지 않으면 pressure가
    /// 계속 1.0으로 표시됩니다).
    pub(crate) fn paint_debug_hud(&self, ctx: &egui::Context, origin: Pos2) {
        let pressure = self.sample_pressure(ctx);
        let (_, p_src) = self.pressure_source(ctx);
        let (speed, tip_w, pts_n) = match &self.active_stroke {
            Some(st) if st.points.len() >= 2 => {
                let n = st.points.len();
                let a = st.points[n - 2];
                let b = st.points[n - 1];
                let dist = ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
                let dt = (b.t_ms.saturating_sub(a.t_ms)) as f32 / 1000.0;
                let v = if dt > 1e-4 { dist / dt } else { 0.0 };
                (v, st.points.last().map(|p| p.width).unwrap_or(0.0), n)
            }
            Some(st) => (
                0.0,
                st.points.first().map(|p| p.width).unwrap_or(0.0),
                st.points.len(),
            ),
            None => (0.0, 0.0, 0),
        };
        let (fps, touch_events) = ctx.input(|i| {
            let fps = if i.unstable_dt > 1e-4 {
                1.0 / i.unstable_dt
            } else {
                0.0
            };
            let touches = i
                .events
                .iter()
                .filter(|e| matches!(e, egui::Event::Touch { .. }))
                .count();
            (fps, touches)
        });
        let device = match self.input_device {
            InputDevice::Pen => "Pen",
            InputDevice::Mouse => "Mouse",
        };
        let is_fountain = self.tool == ToolType::Fountain;
        let (p_k, s_ref, t_k) = if is_fountain {
            (
                self.fountain_profile.pressure_alpha,
                self.fountain_profile.speed_ref,
                self.fountain_profile.tilt_k,
            )
        } else {
            (
                self.pen_profile.pressure_k,
                self.pen_profile.speed_max,
                self.pen_profile.tilt_k,
            )
        };
        let pen_src: &str = if cfg!(target_os = "windows") {
            "OTD"
        } else {
            "evdev"
        };
        egui::Area::new(egui::Id::new("freedf_debug_hud"))
            .fixed_pos(origin + egui::vec2(14.0, 14.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.set_min_width(220.0);
                    ui.strong("Debug HUD");
                    ui.label(format!(
                        "device: {device}  (touch events/frame: {touch_events})"
                    ));
                    ui.label(format!(
                        "pressure: {pressure:.3}  (src: {p_src})"
                    ));
                    ui.label(format!(
                        "tilt: [{:+.0}°, {:+.0}°]  (src: {})",
                        self.pen_tilt[0],
                        self.pen_tilt[1],
                        if self.pen_monitor.is_some() {
                            pen_src
                        } else {
                            "없음"
                        }
                    ));
                    ui.label(format!(
                        "pen buttons: b1={} b2={}",
                        self.pen_buttons.button1, self.pen_buttons.button2
                    ));
                    ui.label(format!("tip speed: {speed:.0} pt/s"));
                    ui.label(format!("tip width: {tip_w:.2} pt"));
                    ui.label(format!("active points: {pts_n}"));
                    ui.label(format!(
                        "pen stream: {}",
                        match self.last_pen_state_ms {
                            Some(t) => format!("수신됨 ({}ms 전)", now_ms().saturating_sub(t)),
                            None => "수신 없음 — OTD/장치 확인".to_string(),
                        }
                    ));
                    if let Some(v) = &self.pen_verdict {
                        ui.label(format!("verdict: {v}"));
                    }
                    ui.label("render: ribbon ≈ O(n) · 20ms 스로틀  (완성 획: 동일 리본)");
                    ui.label(format!("fps: {fps:.0}"));
                    ui.separator();
                    ui.label(format!(
                        "model: p_k={p_k:.2}  speed_ref/max={s_ref:.0}  tilt_k={t_k:.2}"
                    ));
                    ui.label(format!(
                        "tool: {}",
                        if is_fountain { "Fountain" } else { "Pen" }
                    ));
                });
            });
    }

    pub(crate) fn finish_stroke(&mut self) {
        // 스로틀 캐시 무효화 — 완성된 획은 병합 메시(정확 지오메트리)로 넘어갑니다.
        self.active_mesh = None;
        if let Some(mut active) = self.active_stroke.take() {
            self.smooth_active = false;
            if active.points.is_empty() {
                return;
            }
            // ── 펜업 전환 진단: 표시되던 마지막 점들의 (필압, 폭) vs 펜 뗀 뒤.
            let before_penup: Vec<(f32, f32)> = active
                .points
                .iter()
                .rev()
                .take(4)
                .map(|p| (p.pressure, p.width))
                .collect();
            // 마지막 점의 폭을 확정합니다 (인과적 — 이후 절대 변하지 않음).
            if let Some(mut locker) = self.width_locker.take() {
                if let Some(final_pt) = locker.finish() {
                    if let Some(last) = active.points.last_mut() {
                        *last = final_pt;
                    }
                }
            }
            let after_penup: Vec<(f32, f32)> = active
                .points
                .iter()
                .rev()
                .take(4)
                .map(|p| (p.pressure, p.width))
                .collect();
            if before_penup != after_penup {
                pen_trace(&format!(
                    "PENUP-CHANGED: 표시={before_penup:?} 확정={after_penup:?} live_pressure={:?} ← 펜 떼는 순간 폭 데이터가 바뀜!",
                    self.live_pressure
                ));
            } else {
                pen_trace(&format!(
                    "penup tail (pressure,width): {after_penup:?} live_pressure={:?}",
                    self.live_pressure
                ));
            }
            // ── 펜 진단: 획이 끝나면 필압/**렌더 폭** 변화량을 로그로 남깁니다.
            if active.tool != ToolType::Highlighter {
                let n_pt = active.points.len();
                let (mut pmn, mut pmx) = (f32::MAX, f32::MIN);
                let (mut wmn, mut wmx) = (f32::MAX, f32::MIN);
                let (mut hmn, mut hmx) = (f32::MAX, f32::MIN);
                let mut unlocked = 0usize;
                for p in &active.points {
                    pmn = pmn.min(p.pressure);
                    pmx = pmx.max(p.pressure);
                    if p.width > 0.0 {
                        wmn = wmn.min(p.width);
                        wmx = wmx.max(p.width);
                        // 실제 렌더에 쓰이는 절반 폭 (stroke_halves와 동일 규칙).
                        let h = (p.width * 0.5).max(0.05);
                        hmn = hmn.min(h);
                        hmx = hmx.max(h);
                    } else {
                        unlocked += 1;
                    }
                }
                let verdict = if n_pt < 8 {
                    "점 부족"
                } else if pmx - pmn < 0.05 {
                    "필압 일정 → 입력 문제 (OTD 연결/필압 소스 확인)"
                } else if unlocked > 0 {
                    "폭 잠금 안 됨 → locker 버그"
                } else if hmx - hmn < 0.02 {
                    "필압은 변하는데 렌더 폭 고정 → 모델/바닥값 버그"
                } else {
                    "OK — 렌더 폭 변화 정상"
                };
                self.pen_verdict = Some(verdict.to_string());
                pen_trace(&format!(
                    "stroke end: tool={:?} n={n_pt} pressure=[{pmn:.3}..{pmx:.3}] width=[{wmn:.3}..{wmx:.3}] half=[{hmn:.3}..{hmx:.3}] unlocked={unlocked} live_pressure={:?} tilt=[{:+.0},{:+.0}] → {verdict}",
                    active.tool, self.live_pressure, self.pen_tilt[0], self.pen_tilt[1]
                ));
            }
            // 하이라이터 + 텍스트 인식 모드면 스와이프가 닿은 문서 텍스트 위로
            // 깔끔한 하이라이트를 만들어 저장하고, 원본 자유선은 버립니다.
            if active.tool == ToolType::Highlighter
                && self.text_highlight_snap
                && self.document.is_some()
                && self.add_text_highlights(&active)
            {
                return;
            }
            // 블리드 나이의 기준 = **획을 그리기 시작한 시각**(첫 점 t_ms).
            // 펜을 뗀 시각이 아니라 시작 시각이어야, 그리는 동안 자라던
            // 번짐이 펜을 떼는 순간에도 끊김 없이 이어집니다.
            let created_ms = active
                .points
                .first()
                .map(|p| p.t_ms)
                .filter(|t| *t > 0)
                .unwrap_or_else(now_ms);
            // DB 시퀀스에서 id를 미리 할당받아 스토어/히스토리/DB 행이 같은
            // id를 공유하게 합니다 (풀링 — 스트로크마다 왕복하지 않음).
            let db_id = self.next_stroke_ids(1).first().copied();
            let id = match (self.doc_id, db_id) {
                (Some(doc_id), Some(sid)) => {
                    self.store.add_stroke_with_id(
                        self.current_page,
                        sid as u64,
                        active.tool,
                        active.color,
                        active.width,
                        active.points,
                    );
                    self.store
                        .set_stroke_created_ms(self.current_page, sid as u64, created_ms);
                    let strokes: Vec<_> = self
                        .store
                        .strokes_on(self.current_page)
                        .iter()
                        .filter(|s| s.id == sid as u64)
                        .cloned()
                        .collect();
                    self.db
                        .insert_strokes(doc_id, self.current_page as i32, &strokes);
                    sid as u64
                }
                _ => {
                    let id = self.store.add_stroke(
                        self.current_page,
                        active.tool,
                        active.color,
                        active.width,
                        active.points,
                    );
                    self.store
                        .set_stroke_created_ms(self.current_page, id, created_ms);
                    // 풀 소진 폴백이어도 문서가 열려 있으면 write-behind 큐에
                    // 보냅니다 (메타-only 저장에서도 유실되지 않도록).
                    if let Some(doc_id) = self.doc_id {
                        let strokes: Vec<_> = self
                            .store
                            .strokes_on(self.current_page)
                            .iter()
                            .filter(|s| s.id == id)
                            .cloned()
                            .collect();
                        self.db
                            .insert_strokes(doc_id, self.current_page as i32, &strokes);
                    }
                    id
                }
            };
            self.last_finished_id = Some(id);
            if let Some(stroke) = self.store.stroke(self.current_page, id).cloned() {
                self.push_history(Edit::AddStrokes {
                    page: self.current_page,
                    strokes: vec![stroke.clone()],
                });
                self.logger.log(AppEvent::StrokeAdded {
                    page: self.current_page,
                    points: stroke.points.len(),
                    tool: tool_label(active.tool).to_string(),
                    width: active.width,
                });
            }
        }
    }

    /// 스트로크가 닿은 **글자**들을 줄 단위로 묶어 밴드 하이라이트를 만듭니다.
    ///
    /// pdfium `tight_bounds()`(글자별 박스)로 정밀 판정하며, 각 줄은 그 줄의
    /// 높이만큼의 반투명 밴드 하나로 칠합니다. **필압은 전혀 쓰지 않습니다.**
    /// 성공(텍스트 하이라이트를 만든 경우)하면 `true`를 반환합니다.
    pub(crate) fn add_text_highlights(&mut self, active: &ActiveStroke) -> bool {
        let Some(doc) = &self.document else {
            return false;
        };
        let (mut x0, mut y0, mut x1, mut y1) =
            (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in &active.points {
            x0 = x0.min(p.x);
            y0 = y0.min(p.y);
            x1 = x1.max(p.x);
            y1 = y1.max(p.y);
        }
        if x1 < x0 || y1 < y0 {
            return false;
        }
        // 항상 현재 페이지의 글자 좌표를 새로 읽습니다 (캐시 없음 → 정확).
        let char_rects = doc.page_char_rects(self.current_page).unwrap_or_default();
        if char_rects.is_empty() {
            // 페이지에 선택 가능한 텍스트가 없음(스캔/이미지 PDF 등).
            self.status = Some(
                "No selectable text on this page — drew a free-form highlight."
                    .to_string(),
            );
            return false;
        }
        // 닿은 글자를 줄 단위로 합쳐 연속 밴드로 만듭니다.
        let rects = char_line_highlights(&char_rects, [x0, y0, x1, y1], 4.0);
        if rects.is_empty() {
            return false;
        }
        // DB 시퀀스에서 밴드 수만큼 id를 미리 할당합니다 (풀링).
        let ids = self.next_stroke_ids(rects.len());
        // 풀이 마르면(연결 늦음/끊김) 나머지 밴드는 로컬 id 폴백 — UI 왕복 없음.
        let mut local_next = ids
            .iter()
            .copied()
            .map(|i| i as u64)
            .max()
            .or_else(|| {
                self.store
                    .strokes_on(self.current_page)
                    .iter()
                    .map(|s| s.id)
                    .max()
            })
            .unwrap_or(0)
            + 1;
        let created_ms = now_ms();
        let mut strokes = Vec::new();
        for (k, r) in rects.iter().enumerate() {
            // 밴드 높이 = 그 줄의 글자 높이(포인트). 필압은 1.0(무시).
            let line_h = (r[3] - r[1]).max(2.0);
            let yc = (r[1] + r[3]) * 0.5;
            let sid = match ids.get(k) {
                Some(i) => *i as u64,
                None => {
                    let id = local_next;
                    local_next += 1;
                    id
                }
            };
            strokes.push(freedf_core::model::Stroke {
                id: sid,
                tool: ToolType::Highlighter,
                color: active.color,
                width: line_h,
                points: vec![
                    StrokePoint::new(r[0], yc, 1.0),
                    StrokePoint::new(r[2], yc, 1.0),
                ],
                created_ms,
            });
        }
        self.store.add_strokes(self.current_page, strokes.clone());
        if let Some(doc_id) = self.doc_id {
            self.db
                .insert_strokes(doc_id, self.current_page as i32, &strokes);
        }
        self.push_history(Edit::AddStrokes {
            page: self.current_page,
            strokes: strokes.clone(),
        });
        self.logger.log(AppEvent::StrokeAdded {
            page: self.current_page,
            points: strokes.len() * 2,
            tool: "Highlighter".to_string(),
            width: active.width,
        });
        true
    }

    pub(crate) fn commit_dot(&mut self, point: [f32; 2], pressure: f32) {
        let (color, width) = self.current_drawing_style();
        self.width_locker = Some(freedf_core::pen::WidthLocker::new(
            self.tool,
            width,
            self.pen_profile,
            self.fountain_profile,
            tilt_magnitude(&self.pen_tilt),
        ));
        let mut point = StrokePoint::with_time(point[0], point[1], pressure, now_ms());
        if let Some(locker) = &mut self.width_locker {
            let (_, tip) = locker.push(point);
            point = tip;
        }
        self.active_stroke = Some(ActiveStroke {
            tool: self.tool,
            color,
            width,
            points: vec![point],
        });
        self.finish_stroke();
    }

    // ---------- Texture rendering ----------

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
            self.view
                .align_page(self.page_size_pts, canvas_size, TOP_MARGIN, self.page_align);
            self.render_dirty = true;
        }
        self.prev_canvas = canvas_size;
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
        self.ensure_texture(&ctx);

        // evdev/OTD 펜 입력 폴링 — egui가 노출하지 않는 틸트/필압 공급원.
        let pen_state = self.pen_monitor.as_mut().and_then(|mon| mon.poll());
        if let Some(st) = pen_state {
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
        self.view.clamp_pan(self.page_size_pts, canvas_size, CANVAS_MARGIN);

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
            // Paper grid / ruling (only for notes)
            if self.current_note.is_some() {
                self.paint_paper(&painter, draw_origin);
            }
        }

        // Search highlights (under ink so annotations stay readable)
        self.paint_search_highlights(&painter, draw_origin);

        // Annotation strokes — 완성 획 전부를 병합 잉크 메시 하나로.
        // 주의: 메시는 **오프셋 없는 origin 기준**으로 구워야 합니다.
        // 과거에 draw_origin(페이지 전환 오프셋 포함)으로 구웠다가, 애니메이션
        // 종료 후에도 오프셋이 구워진 메시가 캐시에 남아 스트로크가 페이지
        // 옆으로 어긋나는 버그가 있었습니다 (Fit Width로 키가 바뀌면 복귀).
        let now = now_ms();
        if self.ink_needs_rebuild(now) {
            let strokes: Vec<_> = self.store.strokes_on(self.current_page).to_vec();
            if let Some(mesh) = self.build_ink_mesh(&strokes, origin, painter.clip_rect(), now) {
                self.ink_mesh = Some(mesh);
                self.ink_key = (
                    self.current_page,
                    self.store.rev(),
                    self.store_generation,
                    self.view.pan_x,
                    self.view.pan_y,
                    self.view.zoom,
                    self.pen_soak,
                    self.fountain_soak,
                    self.pen_profile,
                    self.fountain_profile,
                    self.pen_grain,
                    self.fountain_grain,
                );
                self.ink_built_at = now;
            }
        }
        if let Some(mesh) = &self.ink_mesh {
            let shifted = anim_dx.abs() > 0.5 || anim_dy.abs() > 0.5;
            if shifted {
                // 페이지 전환 애니메이션 중 — 재구성 없이 정점만 평행 이동한
                // 사본(O(V))을 그려 잉크가 페이지와 함께 미끄러지게 합니다.
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
        if response.hovered() && over_page {
            ctx.set_cursor_icon(egui::CursorIcon::None);
            let time = ctx.input(|i| i.time) as f32;
            if let Some(pos) = pointer_pos {
                self.paint_custom_cursor(&painter, pos, time);
            }
        } else {
            ctx.set_cursor_icon(egui::CursorIcon::Default);
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
    fn prefetch_page(&mut self, ctx: &egui::Context) {
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

    pub(crate) fn paint_search_highlights(&self, painter: &egui::Painter, origin: Pos2) {
        let match_fill = Color32::from_rgba_unmultiplied(255, 235, 60, 80);
        let current_fill = Color32::from_rgba_unmultiplied(255, 200, 40, 120);
        let current_stroke = Color32::from_rgb(255, 140, 0);
        for (i, m) in self.search_matches.iter().enumerate() {
            let r = m.rect;
            let a = self.view.page_to_view([r[0], r[1]]);
            let b = self.view.page_to_view([r[2], r[3]]);
            let rect = Rect::from_min_max(
                origin + Vec2::new(a[0], a[1]),
                origin + Vec2::new(b[0], b[1]),
            );
            if Some(i) == self.search_current {
                painter.rect_filled(rect, 2.0, current_fill);
                painter.rect_stroke(
                    rect,
                    2.0,
                    Stroke::new(2.0, current_stroke),
                    egui::StrokeKind::Inside,
                );
            } else {
                painter.rect_filled(rect, 2.0, match_fill);
            }
        }
    }

    /// Draws the paper grid / ruling / dots onto the page (notes only).
    ///
    /// The line **color and thickness are per-page settings** (`PagePaper`):
    /// thickness is stored in page points and scaled with the zoom so it stays
    /// proportional to the page, like real printed ruling.
    pub(crate) fn paint_paper(&self, painter: &egui::Painter, origin: Pos2) {
        let w = self.page_size_pts[0];
        let h = self.page_size_pts[1];
        let paper = self.current_page_paper();
        let style = paper.style;
        // 줄/격자/점 세부설정은 **스타일별 프리셋**을 참조합니다 — 페이지에는
        // 스타일만 저장되고, 프리셋을 바꾸면 그 스타일 페이지 전부가 함께 바뀝니다.
        let Some(ls) = self.paper_style_settings.of(style) else {
            return; // Blank — 그릴 줄 없음.
        };
        let spacing = ls.spacing;
        let line = Color32::from_rgba_unmultiplied(ls.color[0], ls.color[1], ls.color[2], ls.color[3]);
        let zoom = self.view.zoom;
        let stroke_w = (ls.width * zoom).clamp(0.5, 24.0);
        let dot_r = (ls.width * zoom * 0.4).clamp(0.6, 8.0);
        // 회전된 페이지는 줄도 종이와 함께 돌아야 합니다 (90/270° → 세로줄).
        let rotation = self
            .document
            .as_ref()
            .map(|d| d.page_rotation(self.current_page))
            .unwrap_or(freedf_core::text::PageRotation::None);
        for [x0, y0, x1, y1] in paper_lines_rotated(w, h, style, spacing, rotation) {
            let a = self.view.page_to_view([x0, y0]);
            let b = self.view.page_to_view([x1, y1]);
            painter.line_segment(
                [origin + Vec2::new(a[0], a[1]), origin + Vec2::new(b[0], b[1])],
                Stroke::new(stroke_w, line),
            );
        }
        for [x, y] in paper_dots(w, h, style, spacing) {
            let v = self.view.page_to_view([x, y]);
            painter.circle_filled(origin + Vec2::new(v[0], v[1]), dot_r, line);
        }
    }

    // ---------- Input handling ----------

    pub(crate) fn handle_canvas_input(
        &mut self,
        ctx: &egui::Context,
        response: &egui::Response,
        origin: Pos2,
        canvas_size: [f32; 2],
    ) {
        let pointer_abs = response.interact_pointer_pos();

        // ── 원형 색상 팔레트(펜 버튼)가 열려 있으면 — 탭을 휠이 전담합니다.
        if self.color_wheel_open {
            if let Some(abs) = frame_tap_pos(ctx) {
                let canvas_rect =
                    egui::Rect::from_min_size(origin, egui::vec2(canvas_size[0], canvas_size[1]));
                let wheel_center = self.color_wheel_center(canvas_rect);
                if abs.distance(wheel_center) <= WHEEL_BACK_R + 4.0 {
                    // 휠 안 탭 — color_wheel_overlay가 처리, 캔버스 입력은 스킵.
                    return;
                }
                // 바깥 탭 — 닫고 점 없이 삼킵니다.
                self.color_wheel_open = false;
                self.wheel_swallow_click = true;
                return;
            }
        }

        // Zoom (pinch / trackpad pinch / Ctrl+wheel / Ctrl+two-finger scroll)
        let (zoom_delta, scroll) = ctx.input(|i| (i.zoom_delta(), i.smooth_scroll_delta));
        let scroll_x = scroll.x;
        let scroll_y = scroll.y;
        let ctrl_down = ctx.input(|i| i.modifiers.ctrl);
        let dt = ctx.input(|i| i.stable_dt).max(1e-4);
        let pointer_any_down = ctx.input(|i| i.pointer.any_down());

        // 줌 잠금이면 모든 줌 입력(핀치/Ctrl+휠/트랙패드)을 무시합니다.
        if !self.zoom_lock {
            // PDF 렌더러 특성상 연속 줌(애니메이션)은 매 프레임 재래스터라
            // 뭘 해도 렉이 걸립니다. 모든 줌 입력을 **고정 5% 스텝**으로
            // 양자화해 한 번에 적용합니다 — 스텝당 재렌더 1회만 발생합니다.
            let mut steps = 0.0f32;
            // 1) 핀치/트랙패드 핀치 (연속 배율) → ln으로 스텝 수 환산 후 반올림.
            if (zoom_delta - 1.0).abs() > 1e-4 {
                steps += (zoom_delta.ln() / ZOOM_STEP.ln()).round();
            }
            // 2) Ctrl+휠 노치 → 노치당 1스텝 (±5%).
            let mut ctrl_notches = 0.0f32;
            if ctrl_down {
                // egui의 smooth_scroll_delta는 스무딩돼 노치 1개가 크게
                // 튈 수 있으므로, 이번 프레임의 원시 휠 이벤트를 셉니다.
                let events: Vec<egui::Event> =
                    ctx.input(|i| i.events.iter().cloned().collect());
                for ev in &events {
                    if let egui::Event::MouseWheel {
                        unit,
                        delta,
                        modifiers,
                        ..
                    } = ev
                    {
                        if modifiers.ctrl {
                            ctrl_notches += match unit {
                                egui::MouseWheelUnit::Line => delta.y,
                                egui::MouseWheelUnit::Point => delta.y / 50.0,
                                egui::MouseWheelUnit::Page => delta.y,
                            };
                        }
                    }
                }
                if ctrl_notches.abs() > 1e-4 {
                    steps += ctrl_notches.round();
                }
            }
            if steps.abs() >= 0.5 && (response.hovered() || ctrl_notches.abs() > 1e-4) {
                // 포인터가 있으면 그 아래 페이지 점을 앵커로, 없으면 캔버스 중심.
                let anchor_ui = pointer_abs
                    .map(|abs| [abs.x - origin.x, abs.y - origin.y])
                    .unwrap_or([canvas_size[0] * 0.5, canvas_size[1] * 0.5]);
                self.view.zoom_at(anchor_ui, ZOOM_STEP.powf(steps), MIN_ZOOM, MAX_ZOOM);
                self.render_dirty = true;
                ctx.request_repaint();
            }
        } // end !zoom_lock (줌 잠금)

        // ── Animated scroll (mouse wheel / trackpad) ─────────────────────
        // Wheel/trackpad deltas are not applied in one jump. They accumulate
        // in `scroll_vel` (pending pixels) and are eased into a pan each frame,
        // so scrolling glides instead of stepping. A mostly-vertical gesture
        // over a fully-visible page still flips to the previous/next page.
        let page_h_px = self.page_size_pts[1] * self.view.zoom;
        let page_w_px = self.page_size_pts[0] * self.view.zoom;
        if (scroll_x.abs() + scroll_y.abs()) > 0.0 && response.hovered() && !ctrl_down {
            if page_h_px <= canvas_size[1] && scroll_x.abs() <= scroll_y.abs() {
                // Whole page height visible & mostly-vertical gesture -> page flip.
                // Content follows the fingers (natural scrolling): positive
                // scroll_y (fingers down) shows earlier content -> previous page.
                if scroll_y > 0.0 {
                    self.prev_page();
                } else {
                    self.next_page();
                }
                self.scroll_vel = Vec2::ZERO;
            } else {
                // Accumulate; the per-frame easing below glides smoothly.
                self.scroll_vel += Vec2::new(scroll_x, scroll_y);
            }
            ctx.request_repaint();
        }
        if self.scroll_vel.length_sq() > 1e-8 {
            let k = (1.0 - (-SCROLL_SMOOTH_RATE * dt).exp()).min(1.0);
            let step = self.scroll_vel * k;
            self.scroll_vel -= step;
            let dx = if page_w_px <= canvas_size[0] { 0.0 } else { step.x };
            let dy = if page_h_px <= canvas_size[1] { 0.0 } else { step.y };
            if dx != 0.0 || dy != 0.0 {
                self.view.pan_by(dx, dy);
                ctx.request_repaint();
            }
        } else if !pointer_any_down {
            self.scroll_vel = Vec2::ZERO;
        }

        // Middle-button pan
        let middle_down = ctx.input(|i| i.pointer.button_down(egui::PointerButton::Middle));
        if middle_down {
            if let Some(abs) = ctx.input(|i| i.pointer.interact_pos()) {
                if let Some(last) = self.middle_pan_last {
                    let d = abs - last;
                    self.view.pan_by(d.x, d.y);
                }
                self.middle_pan_last = Some(abs);
            }
        } else {
            self.middle_pan_last = None;
        }

        let primary_down = ctx.input(|i| i.pointer.primary_down());

        // ── 입력 장치 판별 (래치 + 유예 시간) ─────────────────────────────
        // egui 0.36 이벤트에는 장치 필드가 없어, Windows Ink 펜의 `Event::Touch`
        // 유무로 펜/마우스를 구분합니다. 펜 입력 중 일부 프레임에는 Touch
        // 이벤트가 아예 없을 수 있는데, 그때마다 Mouse로 뒤집히면 **필기 중
        // 팬(페이지 이동)으로 전환되어 페이지가 갑자기 확 이동**합니다
        // (펜을 떼는 순간 장치 변환이 감지되던 버그의 원인).
        // → 마지막 터치 후 1초간은 Pen으로 유지하고, 스트로크 진행 중에는
        //   절대 Mouse로 뒤집지 않습니다.
        let has_touch = ctx
            .input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Touch { .. })));
        let any_pointer = ctx.input(|i| {
            i.pointer.any_down() || i.pointer.any_pressed() || i.pointer.any_released()
        });
        if has_touch {
            self.input_device = InputDevice::Pen;
            self.last_touch_time = Some(ctx.input(|i| i.time));
        } else if any_pointer && self.active_stroke.is_none() {
            let now = ctx.input(|i| i.time);
            let stale = self.last_touch_time.map_or(true, |t| now - t > 1.0);
            if stale {
                self.input_device = InputDevice::Mouse;
            }
        }

        // ── 사전 오버레이: 단어 탭 조회 (다른 동작보다 우선) ─────────────
        if response.clicked() && self.dictionary.enabled && self.document.is_some() {
            if let Some(abs) = pointer_abs {
                let p = abs - origin;
                let raw = self.view.view_to_page([p.x, p.y]);
                let page_w = self.page_size_pts[0];
                let page_h = self.page_size_pts[1];
                if raw[0] >= 0.0 && raw[0] <= page_w && raw[1] >= 0.0 && raw[1] <= page_h {
                    self.lookup_word_at(raw, abs);
                    return;
                }
            }
        }

        // 마우스/트랙패드는 (mouse_draws가 꺼져 있으면) 모든 잉크 도구에서
        // 팬으로 동작 — 팬만 글을 쓰게 하는 범용 관례를 따릅니다.
        let panning = self.tool == ToolType::Pan
            || (!self.mouse_draws
                && self.input_device == InputDevice::Mouse
                && matches!(
                    self.tool,
                    ToolType::Pen | ToolType::Fountain | ToolType::Highlighter | ToolType::Eraser
                ));

        if panning {
            if response.dragged() || response.is_pointer_button_down_on() {
                if let Some(abs) = pointer_abs {
                    if let Some(last) = self.pan_last {
                        let d = abs - last;
                        self.view.pan_by(d.x, d.y);
                    }
                    self.pan_last = Some(abs);
                }
            }
            if !primary_down {
                self.pan_last = None;
            }
            return;
        }

        match self.tool {
            ToolType::Pen | ToolType::Fountain | ToolType::Highlighter => {
                let page_w = self.page_size_pts[0];
                let page_h = self.page_size_pts[1];
                if primary_down && (response.is_pointer_button_down_on() || response.dragged()) {
                    // ── 포커스 제스처 (스플릿 뷰) ─────────────────────────
                    // ① 아직 포커스 없음 → 이 프레스는 잉크 없이 포커스만
                    //    요청합니다 (한 번만 — 플랫폼이 무시하면 다음부터
                    //    그대로 그립니다). ② 이 프레스가 방금 포커스를 만든
                    //    직후(유예 중)라면 역시 삼킵니다.
                    let unfocused = ctx.input(|i| i.viewport().focused == Some(false));
                    if unfocused {
                        if !self.focus_grabbed {
                            self.focus_grabbed = true;
                            self.focus_swallow_next_click = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                            return;
                        }
                    } else if self.focus_grace_until_ms.is_some_and(|t| now_ms() < t) {
                        return;
                    }
                    if let Some(abs) = pointer_abs {
                        let p = abs - origin;
                        let raw = self.view.view_to_page([p.x, p.y]);
                        // 페이지(캔버스) 바깥에서는 필기 금지: 페이지 내부에서만
                        // 스트로크를 시작하고, 벗어나면 점을 추가하지 않습니다.
                        let inside = raw[0] >= 0.0
                            && raw[0] <= page_w
                            && raw[1] >= 0.0
                            && raw[1] <= page_h;
                        let page = [raw[0].clamp(0.0, page_w), raw[1].clamp(0.0, page_h)];
                        let (pressure, p_src) = self.pressure_source(ctx);
                        if self.active_stroke.is_none() {
                            if inside {
                                // 새 스트로크 시작: 스무딩 필터를 리셋해
                                // 이전 획과 섞이지 않게 합니다.
                                let sm = OneEuroFilter::from_smoothing(self.smoothing);
                                self.smooth_x = sm;
                                self.smooth_y = sm;
                                self.smooth_p = sm;
                                self.smooth_active = true;
                                let (color, width) = self.current_drawing_style();
                                // 선폭 확정기 — 점이 들어오는 즉시 폭을 잠급니다.
                                self.width_locker = Some(freedf_core::pen::WidthLocker::new(
                                    self.tool,
                                    width,
                                    self.pen_profile,
                                    self.fountain_profile,
                                    tilt_magnitude(&self.pen_tilt),
                                ));
                                self.active_stroke = Some(ActiveStroke {
                                    tool: self.tool,
                                    color,
                                    width,
                                    points: Vec::new(),
                                });
                                self.lift_cut_logged = false;
                                pen_trace(&format!(
                                    "stroke start: tool={:?} base_w={width:.1}pt pressure_enabled={} device={:?} p_k={:.2} s_k={:.2} src={p_src} tilt=[{:+.0},{:+.0}]",
                                    self.tool,
                                    self.pressure_enabled,
                                    self.input_device,
                                    self.pen_profile.pressure_k,
                                    self.pen_profile.speed_k,
                                    self.pen_tilt[0],
                                    self.pen_tilt[1]
                                ));
                            }
                        }
                        if let Some(st) = self.active_stroke.as_mut() {
                            if inside {
                                // ── 펜 떼기 직전 처리: 접촉이 해제됐거나 필압이
                                // 사실상 0으로 무너진 꼬리 리포트는 **버립니다** —
                                // 펜 떼는 순간 끝이 갑자기 가늘어지는 "확 바뀜"의
                                // 원인이었습니다. (첫 점 4개는 접촉 시작 타이밍
                                // 차이로 잘릴 수 있으니 점이 쌓인 뒤에만 적용)
                                let pen_lifted = self
                                    .pen_monitor
                                    .as_ref()
                                    .is_some_and(|m| !m.snapshot().contact);
                                // 직전에는 힘이 있었는데 지금 1% 미만 → 리프트 꼬리.
                                let pressure_collapsed = pressure <= 0.01
                                    && st
                                        .points
                                        .last()
                                        .map_or(false, |q| q.pressure > 0.05);
                                let contact_lost = st.points.len() >= 4
                                    && (pen_lifted || pressure_collapsed);
                                if contact_lost {
                                    // 표시 중인 진행 획을 즉시 갱신하도록 캐시 무효화.
                                    self.active_mesh = None;
                                    if !self.lift_cut_logged {
                                        self.lift_cut_logged = true;
                                        pen_trace(
                                            "LIFT-CUT: 접촉 해제/필압 붕괴 뒤 도착한 꼬리 점 제거 (펜 떼는 순간 가늘어지는 것 방지)",
                                        );
                                    }
                                } else {
                                    // 만년필 모델은 점별 시각으로 속도를 계산합니다.
                                    let t_ms = now_ms();
                                    // 1€ 필터(선택적) — OTD 같은 드라이버가 이미
                                    // 안정화하는 환경에서는 꺼둘 수 있습니다.
                                    let (x, y, p) = if self.smoothing_enabled
                                        && self.smoothing > 0.001
                                        && self.smooth_active
                                    {
                                        let t = ctx.input(|i| i.time);
                                        let sx = self.smooth_x.filter(page[0], t);
                                        let sy = self.smooth_y.filter(page[1], t);
                                        let sp = self.smooth_p.filter(pressure, t);
                                        (sx, sy, sp.clamp(0.0, 1.0))
                                    } else {
                                        (page[0], page[1], pressure)
                                    };
                                    let raw = StrokePoint::with_time(x, y, p, t_ms);
                                    if let Some(locker) = &mut self.width_locker {
                                        // 이전 점의 폭을 확정하고 새 점을 잠급니다 —
                                        // 미래 점이 이전 폭을 바꾸는 일이 없습니다.
                                        let (locked_prev, tip) = locker.push(raw);
                                        if let Some(prev) = locked_prev {
                                            if let Some(last) = st.points.last_mut() {
                                                *last = prev;
                                            }
                                        }
                                        st.points.push(tip);
                                        // 진단: 25점마다 압력/잠금 폭을 남깁니다.
                                        if st.points.len() % 25 == 0 {
                                            pen_trace(&format!(
                                                "pt {}: pressure={p:.3} (src={p_src}) locked_w={:.3}",
                                                st.points.len(),
                                                st.points.last().map(|q| q.width).unwrap_or(0.0)
                                            ));
                                        }
                                    } else {
                                        st.push([x, y], p, t_ms);
                                    }
                                }
                            }
                        }
                    }
                }
                if !primary_down && self.active_stroke.is_some() {
                    self.finish_stroke();
                }
                if response.clicked() && self.active_stroke.is_none() {
                    // 포커스용 탭은 점을 찍지 않습니다 (프레스에서 삼킨 표식
                    // 또는 포커스 획득 직후 유예).
                    if self.focus_swallow_next_click {
                        self.focus_swallow_next_click = false;
                        return;
                    }
                    if self.wheel_swallow_click {
                        self.wheel_swallow_click = false;
                        return;
                    }
                    if self.focus_grace_until_ms.is_some_and(|t| now_ms() < t) {
                        return;
                    }
                    if let Some(abs) = pointer_abs {
                        let p = abs - origin;
                        let raw = self.view.view_to_page([p.x, p.y]);
                        // 클릭(점)도 페이지 내부일 때만 기록합니다.
                        if raw[0] >= 0.0
                            && raw[0] <= page_w
                            && raw[1] >= 0.0
                            && raw[1] <= page_h
                        {
                            let page = [raw[0].clamp(0.0, page_w), raw[1].clamp(0.0, page_h)];
                            let pressure = self.sample_pressure(ctx);
                            self.commit_dot(page, pressure);
                        }
                    }
                } else if !primary_down {
                    // 클릭이 완성되지 않았으면 삼킴 표식을 폐기합니다.
                    self.focus_swallow_next_click = false;
                    self.wheel_swallow_click = false;
                }
            }
            ToolType::Eraser => {
                if primary_down && (response.is_pointer_button_down_on() || response.dragged()) {
                    if let Some(abs) = pointer_abs {
                        let p = abs - origin;
                        let page = self.view.view_to_page([p.x, p.y]);
                        let radius = self.eraser_radius / self.view.zoom;
                        let removed = self.store.erase_at(self.current_page, page, radius);
                        if !removed.is_empty() {
                            // 지워진 행만 DB에서 삭제 (증분).
                            if let Some(doc_id) = self.doc_id {
                                let ids: Vec<i64> =
                                    removed.iter().map(|s| s.id as i64).collect();
                                self.db.delete_strokes(doc_id, &ids);
                            }
                            self.push_history(Edit::RemoveStrokes {
                                page: self.current_page,
                                strokes: removed.clone(),
                            });
                            self.logger.log(AppEvent::StrokeErased {
                                page: self.current_page,
                                strokes: removed.len(),
                            });
                        }
                    }
                }
            }
            ToolType::Pan => {}
        }
    }

    // ---------- Stroke painting ----------

    pub(crate) fn paint_active(
        &mut self,
        painter: &egui::Painter,
        active: &ActiveStroke,
        origin: Pos2,
    ) {
        // 나이 기준 = 첫 점의 시각(획 시작) — 그리는 동안에도 번짐이
        // 실시간으로 자라 펜을 떼는 순간 굵기가 튀지 않습니다.
        // (점 벡터 복사 없이 참조로 전달 — 매 프레임 O(n) 복사 제거)
        let created_ms = active
            .points
            .first()
            .map(|p| p.t_ms)
            .filter(|t| *t > 0)
            .unwrap_or_else(now_ms);
        self.paint_stroke(
            painter,
            active.tool,
            active.color,
            active.width,
            created_ms,
            &active.points,
            origin,
        );
    }

    /// 진행 중인 스트로크(또는 단일 스트로크)를 그립니다. 완성된 스트로크는
    /// [`FreeDfApp::build_ink_mesh`]의 병합 메시로 그려지고, 이 함수는
    /// 활성(진행 중) 획 전용입니다. 점의 폭은 입력 시점에 이미 잠겨 있으므로
    /// 여기서는 절대 다시 계산하지 않습니다.
    pub(crate) fn paint_stroke(
        &mut self,
        painter: &egui::Painter,
        tool: ToolType,
        color_in: [u8; 4],
        width: f32,
        created_ms: u64,
        pts: &[StrokePoint],
        origin: Pos2,
    ) {
        if pts.is_empty() {
            return;
        }
        let n = pts.len();
        let color =
            Color32::from_rgba_unmultiplied(color_in[0], color_in[1], color_in[2], color_in[3]);
        // ── 뷰포트 컬링: 페이지 bbox가 화면 밖이면 통째로 스킵 (줌인 시 큰 절약).
        let mut bb = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for p in pts {
            bb[0] = bb[0].min(p.x);
            bb[1] = bb[1].min(p.y);
            bb[2] = bb[2].max(p.x);
            bb[3] = bb[3].max(p.y);
        }
        {
            let pad = (width * 0.7).max(6.0);
            let a = self.view.page_to_view([bb[0] - pad, bb[1] - pad]);
            let b = self.view.page_to_view([bb[2] + pad, bb[3] + pad]);
            let rect = Rect::from_min_max(
                origin + egui::vec2(a[0], a[1]),
                origin + egui::vec2(b[0], b[1]),
            );
            if !rect.intersects(painter.clip_rect()) {
                return;
            }
        }
        let round_caps = matches!(tool, ToolType::Pen | ToolType::Fountain);
        let tilt = tilt_magnitude(&self.pen_tilt);
        let halves_pt = stroke_halves(tool, width, pts, &self.pen_profile, &self.fountain_profile, tilt);
        // ── 라이브 진단 (Debug HUD 켜져 있을 때만): 렌더 폭이 평평하면 경고.
        if pen_trace_on() && n >= 8 && now_ms().saturating_sub(self.pen_flat_log_ms) > 2000 {
            let (mut wmn, mut wmx) = (f32::MAX, f32::MIN);
            for h in &halves_pt {
                wmn = wmn.min(*h);
                wmx = wmx.max(*h);
            }
            let (mut pmn, mut pmx) = (f32::MAX, f32::MIN);
            for p in pts.iter() {
                pmn = pmn.min(p.pressure);
                pmx = pmx.max(p.pressure);
            }
            if wmx - wmn < 0.05 {
                self.pen_flat_log_ms = now_ms();
                pen_trace(&format!(
                    "LIVE-FLAT: n={n} widths=[{wmn:.3}..{wmx:.3}] pressure=[{pmn:.3}..{pmx:.3}] live_pressure={:?}",
                    self.live_pressure
                ));
            }
        }
        // 잉크 스밈 — **도구별**(볼펜은 은은하게, 만년필은 뚜렷하게).
        // 굵기는 쓴 그대로, 색만 점점 진해짐.
        let soak = if tool == ToolType::Fountain {
            self.fountain_soak
        } else {
            self.pen_soak
        };
        let soak_active =
            soak.enabled && matches!(tool, ToolType::Pen | ToolType::Fountain);
        let age_sec = if created_ms == 0 {
            soak.saturate_sec
        } else {
            (now_ms().saturating_sub(created_ms)) as f32 / 1000.0
        };
        let pts_pt: Vec<[f32; 2]> = pts.iter().map(|p| [p.x, p.y]).collect();
        // ── 20ms 스로틀: 지오메트리 재구성은 최대 50Hz — 그 사이엔
        // 캐시된 메시를 그대로 다시 그립니다.
        let now = now_ms();
        let view_key = (
            self.view.zoom,
            self.view.pan_x,
            self.view.pan_y,
            origin.x,
            origin.y,
            self.pen_soak,
            self.fountain_soak,
            self.pen_profile,
            self.fountain_profile,
            self.pen_grain,
            self.fountain_grain,
        );
        let key_eq = |a: &(f32, f32, f32, f32, f32, InkSoak, InkSoak, BallPenProfile, FountainProfile, InkGrain, InkGrain),
                      b: &(f32, f32, f32, f32, f32, InkSoak, InkSoak, BallPenProfile, FountainProfile, InkGrain, InkGrain)| {
            a.0 == b.0
                && a.1 == b.1
                && a.2 == b.2
                && a.3 == b.3
                && a.4 == b.4
                && a.5 == b.5
                && a.6 == b.6
                && a.7 == b.7
                && a.8 == b.8
                && a.9 == b.9
                && a.10 == b.10
        };
        if let Some((built_ms, n0, k0, mesh)) = &self.active_mesh {
            if *n0 == n && key_eq(k0, &view_key) {
                // 동일 입력·동일 뷰 — 캐시 그대로 (후광도 스로틀 단위로 갱신).
                painter.add(egui::Shape::mesh(mesh.clone()));
                return;
            }
            let view_changed = k0.0 != view_key.0
                || k0.1 != view_key.1
                || k0.2 != view_key.2
                || k0.3 != view_key.3
                || k0.4 != view_key.4;
            if !view_changed && now.saturating_sub(*built_ms) < ACTIVE_STROKE_GEOM_MS {
                // 새 점이 왔지만 스로틀 안 — 이번 프레임은 기존 메시 그대로.
                painter.add(egui::Shape::mesh(mesh.clone()));
                return;
            }
        }
        // ── 재구성 (리본 O(n) 단일 스캔 — 귀 자르기 없음).
        if pen_trace_on() {
            // 진단: 진행 중 렌더(리본)가 실제로 쓰는 폭 — 펜업 후 정착 렌더와 대조.
            let (mut hmn, mut hmx) = (f32::MAX, f32::MIN);
            for h in &halves_pt {
                hmn = hmn.min(*h);
                hmx = hmx.max(*h);
            }
            pen_trace(&format!(
                "ACTIVE-RENDER: n={n} half=[{hmn:.3}..{hmx:.3}] first_w={:.3} tip_w={:.3}",
                pts[0].width,
                pts[n - 1].width
            ));
        }
        let to_view = |p: [f32; 2]| -> Pos2 {
            let v = self.view.page_to_view(p);
            egui::pos2(origin.x + v[0], origin.y + v[1])
        };
        let feather_pt = 1.0 / self.view.zoom.max(1e-3); // 화면 1px에 해당하는 pt
        let mut mesh = egui::Mesh::default();
        // 잉크 스밈(도구별) + 잉크 질감(입체적 불균일) 합성:
        // **굵기는 쓴 그대로**, 좌우 정점 알파에 [포화 램프 × 질감 밀도]를 곱합니다.
        // 질감은 획 공간의 결정적 노이즈라 같은 획은 항상 같은 모양입니다.
        let alphas: Option<Vec<[f32; 2]>> =
            if matches!(tool, ToolType::Pen | ToolType::Fountain) {
                let grain = if tool == ToolType::Fountain {
                    self.fountain_grain
                } else {
                    self.pen_grain
                };
                let grain = ink_seed(grain, created_ms);
                let dens = stroke_ink_lr(tool, pts, grain);
                let now = now_ms();
                Some(
                    pts.iter()
                        .enumerate()
                        .map(|(i, p)| {
                            let sat = if soak_active {
                                let age = if p.t_ms == 0 {
                                    age_sec
                                } else {
                                    (now.saturating_sub(p.t_ms)) as f32 / 1000.0
                                };
                                soak.sat_at(age)
                            } else {
                                1.0
                            };
                            [
                                combine_saturation(sat, dens[i][0]),
                                combine_saturation(sat, dens[i][1]),
                            ]
                        })
                        .collect(),
                )
            } else {
                None
            };
        append_ribbon(
            &mut mesh,
            &freedf_core::pen::stroke_ribbon_lr(
                &pts_pt,
                &halves_pt,
                feather_pt,
                round_caps,
                alphas.as_deref(),
            ),
            &to_view,
            color,
        );
        let mesh = std::sync::Arc::new(mesh);
        painter.add(egui::Shape::mesh(mesh.clone()));
        self.active_mesh = Some((now, n, view_key, mesh));
    }

    /// 페이지의 **모든 완성 획**을 병합 잉크 메시 하나로 만듭니다.
    /// 진행 중 획과 **같은 리본 지오메트리**를 쓰므로 시각 차이가 없고,
    /// 드로우 콜은 페이지당 1개입니다.
    pub(crate) fn build_ink_mesh(
        &mut self,
        strokes: &[freedf_core::model::Stroke],
        origin: Pos2,
        clip: Rect,
        now: u64,
    ) -> Option<std::sync::Arc<egui::Mesh>> {
        let view = self.view;
        let to_view = |p: [f32; 2]| -> Pos2 {
            let v = view.page_to_view(p);
            egui::pos2(origin.x + v[0], origin.y + v[1])
        };
        let pen_soak = self.pen_soak;
        let fountain_soak = self.fountain_soak;
        let ball = self.pen_profile;
        let fountain = self.fountain_profile;
        let pen_grain = self.pen_grain;
        let fountain_grain = self.fountain_grain;
        let tilt = tilt_magnitude(&self.pen_tilt);
        let feather_pt = 1.0 / view.zoom.max(1e-3); // 화면 1px에 해당하는 pt

        // 1차 패스: 다음 재구성 시각 — **점별** (그 점이 닿은 시각 +
        // 포화 시간). 스밈이 켜진 볼펜/만년필 획 전부 포함.
        let mut next_settle = u64::MAX;
        for s in strokes {
            let soak = if s.tool == ToolType::Fountain {
                fountain_soak
            } else if s.tool == ToolType::Pen {
                pen_soak
            } else {
                continue;
            };
            if !soak.enabled {
                continue;
            }
            let deadline = (soak.saturate_sec.max(1e-3) * 1000.0) as u64;
            for p in &s.points {
                if p.t_ms > 0 {
                    let settle_ms = p.t_ms.saturating_add(deadline);
                    if now < settle_ms {
                        next_settle = next_settle.min(settle_ms);
                    }
                }
            }
        }
        self.ink_next_settle_ms = next_settle;

        let mut mesh = egui::Mesh::default();
        for s in strokes {
            if s.points.is_empty() {
                continue;
            }
            // 뷰포트 컬링.
            if let Some(bb) = s.bounding_box() {
                let pad = (s.width * 0.7).max(6.0);
                let a = view.page_to_view([bb[0] - pad, bb[1] - pad]);
                let b = view.page_to_view([bb[2] + pad, bb[3] + pad]);
                let rect = Rect::from_min_max(
                    origin + egui::vec2(a[0], a[1]),
                    origin + egui::vec2(b[0], b[1]),
                );
                if !rect.intersects(clip) {
                    continue;
                }
            }
            let color = Color32::from_rgba_unmultiplied(
                s.color[0],
                s.color[1],
                s.color[2],
                s.color[3],
            );
            let pts_pt: Vec<[f32; 2]> = s.points.iter().map(|p| [p.x, p.y]).collect();
            let halves = stroke_halves(s.tool, s.width, &s.points, &ball, &fountain, tilt);
            let round_caps = matches!(s.tool, ToolType::Pen | ToolType::Fountain);
            if Some(s.id) == self.last_finished_id {
                self.last_finished_id = None;
                if pen_trace_on() {
                    // 진단: 방금 끝난 획의 **정착 렌더** 폭 — ACTIVE-RENDER 로그와
                    // 대조해서 펜업 순간 뭐가 바뀌는지 확인.
                    let (mut hmn, mut hmx) = (f32::MAX, f32::MIN);
                    for h in &halves {
                        hmn = hmn.min(*h);
                        hmx = hmx.max(*h);
                    }
                    pen_trace(&format!(
                        "SETTLED-RENDER: id={} n={} half=[{hmn:.3}..{hmx:.3}] first_w={:.3} tip_w={:.3}",
                        s.id,
                        s.points.len(),
                        s.points[0].width,
                        s.points[s.points.len() - 1].width
                    ));
                }
            }
            // 잉크 스밈(도구별) + 질감 불균일 합성 — 좌우 정점 알파에
            // [포화 램프 × 밀도]를 곱합니다. 굵기는 쓴 그대로.
            let soak = if s.tool == ToolType::Fountain {
                fountain_soak
            } else {
                pen_soak
            };
            let soak_on = soak.enabled;
            let alphas: Option<Vec<[f32; 2]>> =
                if matches!(s.tool, ToolType::Pen | ToolType::Fountain) {
                    let grain = if s.tool == ToolType::Fountain {
                        fountain_grain
                    } else {
                        pen_grain
                    };
                    let created = if s.created_ms > 0 { s.created_ms } else { s.id };
                    let grain = ink_seed(grain, created);
                    let dens = stroke_ink_lr(s.tool, &s.points, grain);
                    Some(
                        s.points
                            .iter()
                            .enumerate()
                            .map(|(i, p)| {
                                let sat = if soak_on {
                                    let age = if p.t_ms == 0 {
                                        soak.saturate_sec
                                    } else {
                                        (now.saturating_sub(p.t_ms)) as f32 / 1000.0
                                    };
                                    soak.sat_at(age)
                                } else {
                                    1.0
                                };
                                [
                                    combine_saturation(sat, dens[i][0]),
                                    combine_saturation(sat, dens[i][1]),
                                ]
                            })
                            .collect(),
                    )
                } else {
                    None
                };
            append_ribbon(
                &mut mesh,
                &freedf_core::pen::stroke_ribbon_lr(
                    &pts_pt,
                    &halves,
                    feather_pt,
                    round_caps,
                    alphas.as_deref(),
                ),
                &to_view,
                color,
            );
        }
        Some(std::sync::Arc::new(mesh))
    }

    /// 병합 잉크 메시를 다시 만들어야 하는지 — 뷰/페이지/스트로크/설정이
    /// 바뀌었거나, 아직 번지고 있는(젊은) 블리드가 있거나, 방금 정착된
    /// 블리드가 있을 때만 재구성합니다.
    pub(crate) fn ink_needs_rebuild(&self, now: u64) -> bool {
        if self.ink_mesh.is_none() {
            return true;
        }
        let key = (
            self.current_page,
            self.store.rev(),
            self.store_generation,
            self.view.pan_x,
            self.view.pan_y,
            self.view.zoom,
            self.pen_soak,
            self.fountain_soak,
            self.pen_profile,
            self.fountain_profile,
            self.pen_grain,
            self.fountain_grain,
        );
        if key != self.ink_key {
            return true;
        }
        if self.ink_next_settle_ms != u64::MAX {
            // 방금 정착된 획이 있으면 이번 프레임에 한 번 더 재구성합니다.
            if self.ink_built_at < self.ink_next_settle_ms {
                return true;
            }
            // 아직 번지고 있으면 **50ms 스로틀**로만 재구성 (후광은 느리게
            // 자라므로 20Hz 갱신으로도 충분 — 사람은 못 느낍니다).
            if now < self.ink_next_settle_ms
                && now.saturating_sub(self.ink_built_at) >= HALO_GEOM_MS
            {
                return true;
            }
        }
        false
    }


    /// Draws a custom cursor sprite confined to the canvas, previewing the
    /// current tool's shape and color (Pen = 은색, Fountain = 금색 금속 닙 +
    /// 입체 그림자, Highlighter = colored rectangle, Eraser = white circle).
    /// 마우스 + 잉크 도구(mouse_draws 꺼짐)면 팬 십자선으로 표시합니다.
    pub(crate) fn paint_custom_cursor(&self, painter: &egui::Painter, pos: Pos2, time: f32) {
        // 실제로 쓰일 도구: 마우스는 기본적으로 팬처럼 동작.
        let mouse_panning = !self.mouse_draws
            && self.input_device == InputDevice::Mouse
            && matches!(
                self.tool,
                ToolType::Pen | ToolType::Fountain | ToolType::Highlighter | ToolType::Eraser
            );
        let tool = if mouse_panning {
            ToolType::Pan
        } else {
            self.tool
        };
        match tool {
            ToolType::Pen | ToolType::Fountain => {
                // ── 게임풍 금속 펜 닙 커서 — GPU 정점색 그라데이션(원통 음영) +
                // 배럴을 따라 **흐르는 반짝임 밴드** + 2겹 입체 그림자.
                // 만년필=금+뾰족 닙, 볼펜=은+볼 닙. 닙 끝(볼 중심)이 실제 좌표에 고정.
                let is_fountain = tool == ToolType::Fountain;
                // 금속 기본색 (금/은, f32 채널).
                let (br, bg, bb) = if is_fountain {
                    (0.82f32, 0.60, 0.08)
                } else {
                    (0.74, 0.77, 0.80)
                };
                let dark = if is_fountain {
                    Color32::from_rgb(74, 52, 6)
                } else {
                    Color32::from_rgb(58, 62, 68)
                };
                let bright = if is_fountain {
                    Color32::from_rgb(255, 240, 160)
                } else {
                    Color32::from_rgb(250, 252, 255)
                };
                let (az_raw, cos_pitch) = if self.pen_monitor.is_some() {
                    tilt_azimuth(&self.pen_tilt)
                } else if self.left_handed {
                    (-std::f32::consts::PI - DEFAULT_PEN_AZ, 1.0) // 위-왼쪽 기본.
                } else {
                    (DEFAULT_PEN_AZ, 1.0)
                };
                // 손잡이 반평면 제한 — 오른손잡이는 오른쪽, 왼손잡이는 왼쪽만.
                let az = clamp_azimuth_hand(az_raw, self.left_handed);
                let pitch = cos_pitch.acos();
                // 눕힐수록 배럴은 길게, 폭은 원근으로 좁아집니다.
                let len = 24.0 + 30.0 * pitch.sin();
                let w = (5.5 * cos_pitch).max(1.8);
                let dir = egui::vec2(az.cos(), az.sin());
                let perp = egui::vec2(-dir.y, dir.x);
                // 볼펜은 볼 반지름만큼 뒤에서 배럴이 시작 (볼이 좌표에 놓임).
                let ball_r = if is_fountain { 0.0 } else { 1.2 };
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
                // 펜이 패드에 닿으면(접촉) 짙고 팁에 가깝게, 호버는 중간,
                // 패드에서 떨어져 리포트가 끊기면 옅고 멀어집니다 (입체감).
                let pen_contact = self
                    .pen_monitor
                    .as_ref()
                    .is_some_and(|m| m.snapshot().contact);
                let hover_age = self
                    .last_pen_state_ms
                    .map_or(u64::MAX, |t| now_ms().saturating_sub(t));
                let prox = if pen_contact {
                    1.0
                } else {
                    let base = if hover_age < 400 { 0.65 } else { 0.0 };
                    let fade = 1.0 - ((hover_age.saturating_sub(400)) as f32 / 900.0).clamp(0.0, 1.0);
                    (base + 0.65 * fade).clamp(0.0, 1.0)
                };
                let sh_scale = 1.3 - 0.55 * prox; // 멀수록 그림자가 더 떨어짐.
                let (sh1_a, sh2_a) = ((38.0 + 24.0 * prox) as u8, (62.0 + 42.0 * prox) as u8);
                for (sh, alpha) in [
                    (egui::vec2(4.0 * sh_scale, 5.0 * sh_scale), sh1_a),
                    (egui::vec2(2.0 * sh_scale, 2.5 * sh_scale), sh2_a),
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
                // 폭 방향 = 원통 음영(어두운 쪽 → 밝은 림), 길이 방향 = 흐르는
                // 반짝임 밴드, 전체 = 은은한 숨(펄스).
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
                if is_fountain {
                    // 뾰족한 밝은 금속 팁 + 좌우로 미끄러지는 반짝임 점.
                    let t_len = 8.0;
                    let t1 = pos + dir * t_len + perp * 2.4;
                    let t2 = pos + dir * t_len - perp * 2.4;
                    painter.add(egui::Shape::convex_polygon(
                        vec![pos, t2, t1],
                        bright,
                        Stroke::new(1.0, dark),
                    ));
                    // 닙 숨구멍(원형 홀) — 만년필 특유의 디테일.
                    painter.circle_stroke(pos + dir * 5.0, 1.1, Stroke::new(1.0, dark));
                    let gx = (time * 3.0).sin() * 1.2;
                    painter.circle_filled(pos + dir * 2.5 + perp * gx, 1.1, Color32::WHITE);
                } else {
                    // 볼 — 방사형 그라데이션 팬(어두운 림 → 밝은 코어) + 회전 반사점.
                    let mut bm = egui::Mesh::default();
                    let ring_r = [1.0f32, 0.62, 0.3];
                    let ring_k = [0.35f32, 0.8, 1.7];
                    for (&rr, &rk) in ring_r.iter().zip(ring_k.iter()) {
                        for k in 0..12 {
                            let a = std::f32::consts::TAU * (k as f32 / 12.0);
                            let p = pos + egui::vec2(a.cos(), a.sin()) * (ball_r * rr);
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
                            bm.indices
                                .extend_from_slice(&[a0, a1, b0, a1, b1, b0]);
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
                    let sp = pos + egui::vec2(sa.cos(), sa.sin()) * (ball_r * 0.38);
                    painter.circle_filled(sp, ball_r * 0.22, Color32::WHITE);
                }
            }
            ToolType::Highlighter => {
                // 정밀한 마커 닙 커서: **작고 반듯한 사각형** — 두께는 실제
                // 하이라이트 두께와 같고, 왼쪽 시작점이 커서 위치에 고정됩니다
                // (그을 때 실제 선이 여기서 시작됨).
                let color = Color32::from_rgba_unmultiplied(
                    self.hi_color[0],
                    self.hi_color[1],
                    self.hi_color[2],
                    (self.hi_color[3] as f32 * 0.9) as u8,
                );
                let wpx = (self.hi_width * self.view.zoom).clamp(3.0, 90.0);
                let len = 14.0_f32; // 커서 길이는 짧게(힌트만)
                let half = wpx * 0.5;
                // 왼쪽 시작 모서리가 커서 위치.
                let min = pos + Vec2::new(0.0, -half);
                let rect = Rect::from_min_size(min, Vec2::new(len, wpx));
                // 반듯한 사각(모서리 없음) — 위치/크기를 정확히 미리보기.
                painter.rect_filled(rect, 0.0, color);
                painter.rect_stroke(
                    rect,
                    0.0,
                    Stroke::new(1.0, Color32::from_white_alpha(200)),
                    egui::StrokeKind::Inside,
                );
            }
            ToolType::Eraser => {
                // White translucent circle with a soft dark drop shadow so it
                // reads clearly even on white paper.
                let r = self.eraser_radius.max(6.0);
                painter.circle_filled(
                    pos + Vec2::new(2.5, 2.5),
                    r,
                    Color32::from_black_alpha(40),
                );
                painter.circle_filled(pos, r, Color32::from_white_alpha(85));
                painter.circle_stroke(pos, r, Stroke::new(2.0, Color32::from_white_alpha(215)));
                painter.circle_filled(pos, 2.0, Color32::from_black_alpha(110));
            }
            ToolType::Pan => {
                // Small, compact "move" crosshair (much smaller than the OS grab hand).
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 실제 렌더 경로(리본 → 메시)를 돌려, 모든 정점이 유한하고
    /// 스트로크 근처에 머무는지 확인합니다.
    fn mesh_bounds(pts: &[[f32; 2]], halves: &[f32]) -> egui::Rect {
        let ident = |p: [f32; 2]| egui::pos2(p[0], p[1]);
        let rb = freedf_core::pen::stroke_ribbon(pts, halves, 0.5, true, None);
        let mut mesh = egui::Mesh::default();
        append_ribbon(&mut mesh, &rb, &ident, Color32::RED);
        assert!(!mesh.indices.is_empty(), "빈 메시");
        let mut bounds = egui::Rect::NOTHING;
        for v in &mesh.vertices {
            assert!(v.pos.x.is_finite() && v.pos.y.is_finite(), "NaN: {:?}", v.pos);
            bounds.extend_with(v.pos);
        }
        bounds
    }

    #[test]
    fn straight_stroke_mesh_stays_bounded() {
        let pts: Vec<[f32; 2]> = (0..24).map(|i| [20.0 + i as f32 * 8.0, 80.0]).collect();
        let halves: Vec<f32> = vec![1.5; 24];
        // 직선 렌즈도 본체가 빠지지 않아야 함 (면적 기준 완전 커버).
        match freedf_core::pen::stroke_geometry(&pts, &halves, true) {
            freedf_core::pen::StrokeFill::Tris(t) => {
                let area: f32 = t.tris.iter().fold(0.0, |acc, tr| {
                    let (a, b, c) = (
                        t.poly[tr[0] as usize],
                        t.poly[tr[1] as usize],
                        t.poly[tr[2] as usize],
                    );
                    acc + ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs()
                        * 0.5
                });
                let expected = 184.0 * 3.0 + std::f32::consts::PI * 1.5 * 1.5;
                assert!((area - expected).abs() < 1.0, "직선 렌즈 면적: {area} vs {expected}");
            }
            freedf_core::pen::StrokeFill::Fallback(_) => {
                panic!("직선 렌즈는 완전 분할이어야 함")
            }
        }
        let b = mesh_bounds(&pts, &halves);
        assert!(b.min.x > 10.0 && b.max.x < 210.0, "x 경계: {b:?}");
        assert!(b.min.y > 70.0 && b.max.y < 90.0, "y 경계: {b:?}");
    }

    #[test]
    fn scribble_cluster_mesh_stays_bounded() {
        // 스크린샷과 같은 밀집 클러스터 + 급격한 루프 입력 — 리본이 유한한
        // 경계 안에서 커버해야 합니다.
        let mut pts: Vec<[f32; 2]> = Vec::new();
        let mut t = 0.0f32;
        while t < std::f32::consts::TAU * 3.0 {
            let r = 20.0 + 8.0 * (t * 3.0).sin();
            pts.push([100.0 + r * t.cos(), 100.0 + r * t.sin() * 0.7]);
            t += 0.08;
        }
        let halves: Vec<f32> = pts
            .iter()
            .enumerate()
            .map(|(i, _)| 0.4 + 2.0 * (i as f32 / pts.len() as f32))
            .collect();
        let b = mesh_bounds(&pts, &halves);
        assert!(b.min.x > 50.0 && b.max.x < 150.0, "x 경계: {b:?}");
        assert!(b.min.y > 50.0 && b.max.y < 150.0, "y 경계: {b:?}");
    }

    #[test]
    fn duplicate_points_mesh_stays_bounded() {
        // 펜을 누른 채 정지(중복 점) → 필압 램프.
        let mut pts: Vec<[f32; 2]> = Vec::new();
        for _ in 0..6 {
            pts.push([50.0, 50.0]);
        }
        for i in 0..16 {
            pts.push([50.0 + i as f32 * 6.0, 50.0]);
        }
        let halves: Vec<f32> = (0..pts.len())
            .map(|i| 0.4 + 2.0 * (i as f32 / pts.len() as f32))
            .collect();
        let b = mesh_bounds(&pts, &halves);
        assert!(b.min.x > 30.0 && b.max.x < 160.0, "x 경계: {b:?}");
        assert!(b.min.y > 30.0 && b.max.y < 70.0, "y 경계: {b:?}");
    }

    #[test]
    fn ribbon_active_mesh_stays_bounded() {
        // 진행 중 획이 매 프레임 쓰는 리본 경로 — 유한하고 스트로크 근처에 머뭄.
        let mut pts: Vec<[f32; 2]> = Vec::new();
        let mut t = 0.0f32;
        while t < std::f32::consts::TAU * 2.0 {
            let r = 20.0 + 6.0 * (t * 2.0).sin();
            pts.push([100.0 + r * t.cos(), 100.0 + r * t.sin() * 0.6]);
            t += 0.05;
        }
        let halves: Vec<f32> = vec![2.0; pts.len()];
        let ident = |p: [f32; 2]| egui::pos2(p[0], p[1]);
        for feather in [0.0f32, 0.5] {
            let rb = freedf_core::pen::stroke_ribbon(&pts, &halves, feather, true, None);
            assert_eq!(rb.verts.len(), rb.alphas.len());
            assert!(!rb.tris.is_empty());
            let mut mesh = egui::Mesh::default();
            append_ribbon(&mut mesh, &rb, &ident, Color32::RED);
            let mut bounds = egui::Rect::NOTHING;
            for v in &mesh.vertices {
                assert!(v.pos.x.is_finite() && v.pos.y.is_finite(), "NaN: {:?}", v.pos);
                bounds.extend_with(v.pos);
            }
            assert!(bounds.min.x > 50.0 && bounds.max.x < 150.0, "x: {bounds:?}");
            assert!(bounds.min.y > 50.0 && bounds.max.y < 150.0, "y: {bounds:?}");
        }
    }

    #[test]
    fn ink_grain_alphas_stay_bounded_for_ribbon() {
        // 통합 경로: 질감 밀도 × 포화 램프를 좌우 알파로 합성해 리본에 넣어도
        // 모든 정점 알파가 0..1 안에 머물고 정점/알파 수가 일치합니다.
        let pts: Vec<StrokePoint> = (0..64)
            .map(|i| StrokePoint::with_time(i as f32 * 3.0, 50.0, 0.5, i as u64 * 5))
            .collect();
        let g = freedf_core::ink::InkGrain::default();
        let dens = freedf_core::ink::stroke_ink_lr(ToolType::Fountain, &pts, g);
        let alphas: Vec<[f32; 2]> = pts
            .iter()
            .enumerate()
            .map(|(i, _)| {
                let sat = 0.35 + 0.65 * (i as f32 / pts.len() as f32);
                [
                    freedf_core::ink::combine_saturation(sat, dens[i][0]),
                    freedf_core::ink::combine_saturation(sat, dens[i][1]),
                ]
            })
            .collect();
        assert!(alphas
            .iter()
            .flatten()
            .all(|a| a.is_finite() && (0.0..=1.0).contains(a)));
        let p: Vec<[f32; 2]> = pts.iter().map(|q| [q.x, q.y]).collect();
        let halves = vec![1.0f32; pts.len()];
        let rb = freedf_core::pen::stroke_ribbon_lr(&p, &halves, 0.5, true, Some(&alphas));
        assert_eq!(rb.verts.len(), rb.alphas.len());
        assert!(rb.alphas.iter().all(|a| *a >= 0.0 && *a <= 1.0));
    }

    #[test]
    fn tilt_azimuth_maps_direction() {
        let (az, cos) = tilt_azimuth(&[20.0, 0.0]);
        assert!(az.abs() < 1e-3, "오른쪽 기울기 → 방위각 0");
        assert!((cos - 20.0f32.to_radians().cos()).abs() < 1e-4);
        let (az2, _) = tilt_azimuth(&[0.0, 25.0]);
        assert!(
            (az2 - std::f32::consts::FRAC_PI_2).abs() < 1e-3,
            "사용자 쪽 기울기 → +90°"
        );
    }

    #[test]
    fn clamp_azimuth_hand_keeps_half_plane() {
        // 오른손잡이: 오른쪽 반평면(|az| ≤ 90°)만.
        assert!(clamp_azimuth_hand(-0.6, false).abs() < 1.0);
        assert!(clamp_azimuth_hand(2.2, false).cos() >= 0.0, "왼쪽 아래 → 오른쪽");
        assert!(clamp_azimuth_hand(-2.2, false).cos() >= 0.0, "왼쪽 위 → 오른쪽");
        // 왼손잡이: 왼쪽 반평면만.
        assert!(clamp_azimuth_hand(0.6, true).cos() <= 0.0, "오른쪽 → 왼쪽");
        assert!(clamp_azimuth_hand(-2.2, true).cos() <= 0.0);
        // 경계는 유지.
        let f = std::f32::consts::FRAC_PI_2;
        assert!((clamp_azimuth_hand(f, false) - f).abs() < 1e-4);
        assert!((clamp_azimuth_hand(-f, true) - (-f)).abs() < 1e-4);
    }

    #[test]
    fn smooth_tilt_rejects_violent_jumps() {
        // 패드 진입 시 ±90° 스파이크가 연달아 와도 한 걸음이 24°×0.3 = 7.2°를
        // 넘지 않고, 같은 값이 계속되면 서서히 수렴합니다.
        let mut t = [0.0f32, 0.0];
        for _ in 0..8 {
            let prev = t;
            t = smooth_tilt(t, [90.0, -90.0]);
            assert!((t[0] - prev[0]).abs() <= 7.2 + 1e-3, "급격 점프 제한");
            assert!((t[1] - prev[1]).abs() <= 7.2 + 1e-3);
        }
        assert!(t[0] > 40.0 && t[1] < -40.0, "결국 목표로 수렴");
        // 상수 입력에는 정확히 수렴.
        let mut t2 = [10.0f32, -10.0];
        for _ in 0..50 {
            t2 = smooth_tilt(t2, [20.0, 5.0]);
        }
        assert!((t2[0] - 20.0).abs() < 0.5 && (t2[1] - 5.0).abs() < 0.5);
    }
}
