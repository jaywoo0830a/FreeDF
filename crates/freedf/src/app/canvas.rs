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

/// 진행 중 획 지오메트리 재구성 스로틀 (ms) — 10ms면 사실상 매 프레임
/// (리본이 O(n)으로 저렴해져 지연 없이 바로 따라갑니다).
const ACTIVE_STROKE_GEOM_MS: u64 = 10;
/// 번지는 후광(병합 메시) 재구성 스로틀 (ms) — 후광은 느리게 자라므로
/// 20Hz면 충분하고, 페이지 전체 재구성 비용을 아낍니다.
const HALO_GEOM_MS: u64 = 50;

/// 펜 진단 로그 활성 스위치 — **Debug HUD가 켜져 있을 때만** 로그를 남깁니다
/// (평소에는 로그 파일/콘솔 I/O 비용 0).
static PEN_TRACE_ON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

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
// 정확 경로는 내보내기(export.rs) 전용으로 남습니다.

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

/// 점별 블리드 번짐 반경(pt) — 획 위 위치(d0/d1)와 나이에 따라.
fn bleed_radii(pts_pt: &[[f32; 2]], age_sec: f32, bleed: InkBleed) -> Vec<f32> {
    let n = pts_pt.len();
    let len: f32 = pts_pt
        .windows(2)
        .map(|w| ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt())
        .sum();
    let mut dists: Vec<(f32, f32)> = Vec::with_capacity(n);
    let mut acc = 0.0f32;
    for i in 0..n {
        if i > 0 {
            let a = pts_pt[i - 1];
            let b = pts_pt[i];
            acc += ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
        }
        dists.push((acc, len - acc));
    }
    dists
        .iter()
        .map(|(d0, d1)| bleed.radius(*d0, *d1, len, age_sec))
        .collect()
}


impl FreeDfApp {
    pub(crate) fn current_drawing_style(&self) -> ([u8; 4], f32) {
        match self.tool {
            ToolType::Pen | ToolType::Fountain => (self.pen_color, self.pen_width),
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
                    ui.label("render: ribbon ≈ O(n) · 10ms 스로틀  (완성 획: 동일 리본)");
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
            // id를 공유하게 합니다 (undo/redo가 정확히 같은 행을 복원).
            let db_id = self.db.alloc_stroke_ids(1).first().copied();
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
        // DB 시퀀스에서 밴드 수만큼 id를 미리 할당합니다.
        let ids = self.db.alloc_stroke_ids(rects.len());
        let created_ms = now_ms();
        let mut strokes = Vec::new();
        for (k, r) in rects.iter().enumerate() {
            // 밴드 높이 = 그 줄의 글자 높이(포인트). 필압은 1.0(무시).
            let line_h = (r[3] - r[1]).max(2.0);
            let yc = (r[1] + r[3]) * 0.5;
            let sid = ids.get(k).copied().map(|i| i as u64).unwrap_or(0);
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

        // Background behind the page (Nord canvas surround — dark mode)
        let bg = crate::theme::nord::semantic::PAGE_SURROUND;
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
        if let Some(mon) = &mut self.pen_monitor {
            if let Some(st) = mon.poll() {
                if self.last_pen_state_ms.is_none() {
                    pen_trace(&format!(
                        "pen stream 연결됨: tilt=[{:+.0}, {:+.0}] pressure={:?} contact={}",
                        st.tilt[0], st.tilt[1], st.pressure, st.contact
                    ));
                }
                self.last_pen_state_ms = Some(now_ms());
                self.pen_tilt = st.tilt;
                self.live_pressure = st.pressure;
            }
        }

        // ---------- Input ----------
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
        let paper = self.current_page_paper();
        let paper_tint = Color32::from_rgba_unmultiplied(
            paper.color[0],
            paper.color[1],
            paper.color[2],
            255,
        );

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
        let now = now_ms();
        if self.ink_needs_rebuild(now) {
            let strokes: Vec<_> = self.store.strokes_on(self.current_page).to_vec();
            if let Some(mesh) = self.build_ink_mesh(&strokes, draw_origin, painter.clip_rect(), now)
            {
                self.ink_mesh = Some(mesh);
                self.ink_key = (
                    self.current_page,
                    self.store.rev(),
                    self.store_generation,
                    self.view.pan_x,
                    self.view.pan_y,
                    self.view.zoom,
                    self.ink_bleed,
                    self.pen_profile,
                    self.fountain_profile,
                );
                self.ink_built_at = now;
            }
        }
        if let Some(mesh) = &self.ink_mesh {
            painter.add(egui::Shape::mesh(mesh.clone()));
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
                                .on_hover_text("Zoom out (locked: press the lock or Ctrl+L)")
                                .clicked()
                            {
                                self.zoom_by(1.0 / 1.25);
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
                                .on_hover_text("Zoom in (locked: press the lock or Ctrl+L)")
                                .clicked()
                            {
                                self.zoom_by(1.25);
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
                                if self.zoom_lock {
                                    if let Some(t) = self.zoom_target {
                                        self.view.zoom = t.clamp(MIN_ZOOM, MAX_ZOOM);
                                        self.render_dirty = true;
                                    }
                                    self.zoom_target = None;
                                    self.zoom_anchor_page = None;
                                    self.zoom_anchor_ui = None;
                                }
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
                        let cur = Color32::from_rgba_unmultiplied(
                            self.pen_color[0],
                            self.pen_color[1],
                            self.pen_color[2],
                            self.pen_color[3],
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
                                "Palette is full (3 colors) — right-click a swatch to remove one first"
                            } else {
                                "Add current color to favorites"
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
                            let selected = self.pen_color == c;
                            let resp = color_circle_swatch(ui, ("fav_swatch", i), col, selected);
                            if resp
                                .clone()
                                .on_hover_text("Set pen color (right-click to remove)")
                                .clicked()
                            {
                                self.pen_color = c;
                                self.tool = ToolType::Pen;
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
        let spacing = paper.spacing;
        let line = Color32::from_rgba_unmultiplied(
            paper.line_color[0],
            paper.line_color[1],
            paper.line_color[2],
            paper.line_color[3],
        );
        let zoom = self.view.zoom;
        let stroke_w = (paper.line_width * zoom).clamp(0.5, 24.0);
        let dot_r = (paper.line_width * zoom * 0.4).clamp(0.6, 8.0);
        for [x0, y0, x1, y1] in paper_lines(w, h, style, spacing) {
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

        // Zoom (pinch / trackpad pinch / Ctrl+wheel / Ctrl+two-finger scroll)
        let (zoom_delta, scroll) = ctx.input(|i| (i.zoom_delta(), i.smooth_scroll_delta));
        let scroll_x = scroll.x;
        let scroll_y = scroll.y;
        let ctrl_down = ctx.input(|i| i.modifiers.ctrl);
        let dt = ctx.input(|i| i.stable_dt).max(1e-4);
        let pointer_any_down = ctx.input(|i| i.pointer.any_down());

        // 줌 잠금이면 모든 줌 입력(핀치/Ctrl+휠/트랙패드)을 무시합니다.
        if !self.zoom_lock {

        // If egui already folded a pinch / Ctrl+scroll into zoom_delta, use it.
        // Otherwise synthesize zoom from Ctrl + wheel: discrete +1% per notch
        // that slowly accelerates (up to +8%) while you keep scrolling.
        let mut zoom_factor = zoom_delta;
        let mut scroll_zoom = false;
        let mut ctrl_wheel_notches = 0.0f32;
        {
            // Count raw wheel notches this frame (egui's smooth_scroll_delta is
            // smoothed, so a single notch can look like a huge jump).
            let events: Vec<egui::Event> = ctx.input(|i| i.events.iter().cloned().collect());
            for ev in &events {
                if let egui::Event::MouseWheel {
                    unit,
                    delta,
                    modifiers,
                    ..
                } = ev
                {
                    if modifiers.ctrl {
                        let n = match unit {
                            egui::MouseWheelUnit::Line => delta.y,
                            egui::MouseWheelUnit::Point => delta.y / 50.0,
                            egui::MouseWheelUnit::Page => delta.y,
                        };
                        ctrl_wheel_notches += n;
                    }
                }
            }
        }
        if ctrl_down && ctrl_wheel_notches.abs() > 1e-4 && (zoom_delta - 1.0).abs() <= 1e-4 {
            // Restart the ramp if the user paused between notches, then
            // accelerate from +1% up to +8% per notch while scrolling fast.
            let now = ctx.input(|i| i.time);
            if now - self.zoom_accel_last > 0.3 {
                self.zoom_accel = 0.0;
            }
            self.zoom_accel = (self.zoom_accel + 0.01 * ctrl_wheel_notches.abs()).min(0.08);
            self.zoom_accel_last = now;
            let dir = ctrl_wheel_notches.signum();
            zoom_factor = (1.0 + self.zoom_accel * dir).clamp(0.5, 2.0);
            scroll_zoom = true;
        } else if ctrl_down && ctx.input(|i| i.time) - self.zoom_accel_last > 0.3 {
            // Reset the acceleration ramp once scrolling pauses.
            self.zoom_accel = 0.0;
        }

        let zooming = (zoom_factor - 1.0).abs() > 1e-4;
        // Pinch / trackpad pinch already arrive as a *continuous* zoom_delta,
        // so they are applied immediately (they are smooth by nature). A
        // discrete Ctrl+wheel notch instead only sets an eased *target*: the
        // real zoom glides toward it over a few frames instead of jumping.
        let continuous_zoom = (zoom_delta - 1.0).abs() > 1e-4 && !scroll_zoom;
        if zooming && (response.hovered() || scroll_zoom) {
            // Anchor at the pointer when available, otherwise the canvas center.
            let anchor_ui = pointer_abs
                .map(|abs| [abs.x - origin.x, abs.y - origin.y])
                .unwrap_or([canvas_size[0] * 0.5, canvas_size[1] * 0.5]);
            if continuous_zoom {
                self.view.zoom_at(anchor_ui, zoom_factor, MIN_ZOOM, MAX_ZOOM);
                self.render_dirty = true;
                self.zoom_target = None;
                self.zoom_anchor_page = None;
                self.zoom_anchor_ui = None;
                ctx.request_repaint();
            } else {
                // Ctrl+wheel: remember the page point under the cursor, then
                // animate zoom toward the target (compounds if still gliding).
                let page = [
                    (anchor_ui[0] - self.view.pan_x) / self.view.zoom,
                    (anchor_ui[1] - self.view.pan_y) / self.view.zoom,
                ];
                self.zoom_anchor_ui = Some(anchor_ui);
                self.zoom_anchor_page = Some(page);
                let base = self.zoom_target.unwrap_or(self.view.zoom);
                let t = (base * zoom_factor).clamp(MIN_ZOOM, MAX_ZOOM);
                self.zoom_target = Some(t);
                ctx.request_repaint();
            }
        }
        // Cancel an in-flight zoom animation as soon as the user starts a
        // gesture (drawing / panning), snapping to the final zoom cleanly.
        if pointer_any_down {
            if let Some(t) = self.zoom_target {
                self.view.zoom = t.clamp(MIN_ZOOM, MAX_ZOOM);
                self.render_dirty = true;
            }
            self.zoom_target = None;
            self.zoom_anchor_page = None;
            self.zoom_anchor_ui = None;
        }
        // Drive the eased zoom toward its target every frame (smooth glide).
        if let Some(target) = self.zoom_target {
            let diff = (target - self.view.zoom).abs();
            if diff < 1e-4 {
                self.view.zoom = target.clamp(MIN_ZOOM, MAX_ZOOM);
                self.zoom_target = None;
                self.zoom_anchor_page = None;
                self.zoom_anchor_ui = None;
            } else {
                let k = 1.0 - (-ZOOM_SMOOTH_RATE * dt).exp();
                let next = self.view.zoom + (target - self.view.zoom) * k;
                self.view.zoom = next.clamp(MIN_ZOOM, MAX_ZOOM);
                // Keep the anchored page point under the cursor during the glide.
                if let (Some(ui_p), Some(pg)) = (self.zoom_anchor_ui, self.zoom_anchor_page) {
                    self.view.pan_x = ui_p[0] - pg[0] * self.view.zoom;
                    self.view.pan_y = ui_p[1] - pg[1] * self.view.zoom;
                    self.view
                        .clamp_pan(self.page_size_pts, canvas_size, CANVAS_MARGIN);
                }
                self.render_dirty = true;
                ctx.request_repaint();
            }
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
            let pad = (width * 0.7).max(6.0) + self.ink_bleed.max_spread_pt.max(0.0);
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
        let is_pen = tool == ToolType::Pen;
        let bleed_active = is_pen && self.ink_bleed.enabled;
        let settle = self.ink_bleed.settle_sec().max(1e-3);
        let age_sec = if created_ms == 0 {
            settle
        } else {
            (now_ms().saturating_sub(created_ms)) as f32 / 1000.0
        };
        let pts_pt: Vec<[f32; 2]> = pts.iter().map(|p| [p.x, p.y]).collect();
        // ── 10ms 스로틀: 지오메트리 재구성은 최대 100Hz(사실상 매 프레임) —
        // 그 사이엔 캐시된 메시를 그대로 다시 그립니다.
        let now = now_ms();
        let view_key = (
            self.view.zoom,
            self.view.pan_x,
            self.view.pan_y,
            origin.x,
            origin.y,
            self.ink_bleed,
            self.pen_profile,
            self.fountain_profile,
        );
        let key_eq = |a: &(f32, f32, f32, f32, f32, InkBleed, BallPenProfile, FountainProfile),
                      b: &(f32, f32, f32, f32, f32, InkBleed, BallPenProfile, FountainProfile)| {
            a.0 == b.0
                && a.1 == b.1
                && a.2 == b.2
                && a.3 == b.3
                && a.4 == b.4
                && a.5 == b.5
                && a.6 == b.6
                && a.7 == b.7
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
        if bleed_active {
            let radii = bleed_radii(&pts_pt, age_sec, self.ink_bleed);
            for (extra, alpha_k) in [(0.5f32, 0.35f32), (1.0, 0.12)] {
                let hb: Vec<f32> = (0..n).map(|i| halves_pt[i] + radii[i] * extra).collect();
                let alpha = (color.a() as f32 * alpha_k).clamp(0.0, 255.0) as u8;
                let halo_color =
                    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha);
                append_ribbon(
                    &mut mesh,
                    &freedf_core::pen::stroke_ribbon(&pts_pt, &hb, 0.0, true),
                    &to_view,
                    halo_color,
                );
            }
        }
        // 본체 — 리본 + 내장 페더(알파 램프).
        append_ribbon(
            &mut mesh,
            &freedf_core::pen::stroke_ribbon(&pts_pt, &halves_pt, feather_pt, round_caps),
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
        let bleed = self.ink_bleed;
        let settle = bleed.settle_sec().max(1e-3);
        let ball = self.pen_profile;
        let fountain = self.fountain_profile;
        let tilt = tilt_magnitude(&self.pen_tilt);
        let feather_pt = 1.0 / view.zoom.max(1e-3); // 화면 1px에 해당하는 pt

        // 1차 패스: 다음 정착 시각 (젊은 후광이 매 프레임 재구성을 요구하는
        // 동안만 병합 메시를 다시 만듭니다).
        let mut next_settle = u64::MAX;
        for s in strokes {
            if bleed.enabled && s.tool == ToolType::Pen && s.created_ms > 0 {
                let settle_ms = s.created_ms.saturating_add((settle * 1000.0) as u64);
                if now < settle_ms {
                    next_settle = next_settle.min(settle_ms);
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
                let pad = (s.width * 0.7).max(6.0) + bleed.max_spread_pt.max(0.0);
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
            let bleed_on = s.tool == ToolType::Pen && bleed.enabled;
            let age_sec = if s.created_ms == 0 {
                settle
            } else {
                (now.saturating_sub(s.created_ms)) as f32 / 1000.0
            };
            // 젊은 블리드: 후광을 매 프레임 반영 (소수의 최근 획뿐) — 리본.
            if bleed_on && age_sec < settle && age_sec > 0.05 {
                let radii = bleed_radii(&pts_pt, age_sec, bleed);
                for (extra, alpha_k) in [(0.5f32, 0.35f32), (1.0, 0.12)] {
                    let hb: Vec<f32> = (0..halves.len())
                        .map(|i| halves[i] + radii[i] * extra)
                        .collect();
                    let alpha = (color.a() as f32 * alpha_k).clamp(0.0, 255.0) as u8;
                    let halo_color = Color32::from_rgba_unmultiplied(
                        color.r(),
                        color.g(),
                        color.b(),
                        alpha,
                    );
                    append_ribbon(
                        &mut mesh,
                        &freedf_core::pen::stroke_ribbon(&pts_pt, &hb, 0.0, true),
                        &to_view,
                        halo_color,
                    );
                }
            }
            // 본체 — 진행 중 획과 동일한 리본 (시각 차이 없음).
            append_ribbon(
                &mut mesh,
                &freedf_core::pen::stroke_ribbon(&pts_pt, &halves, feather_pt, round_caps),
                &to_view,
                color,
            );
            // 정착 블리드 후광 — 리본.
            if bleed_on && age_sec >= settle {
                let radii = bleed_radii(&pts_pt, settle, bleed);
                for (extra, alpha_k) in [(0.5f32, 0.35f32), (1.0, 0.12)] {
                    let hb: Vec<f32> = (0..halves.len())
                        .map(|i| halves[i] + radii[i] * extra)
                        .collect();
                    let alpha = (color.a() as f32 * alpha_k).clamp(0.0, 255.0) as u8;
                    let halo_color = Color32::from_rgba_unmultiplied(
                        color.r(),
                        color.g(),
                        color.b(),
                        alpha,
                    );
                    append_ribbon(
                        &mut mesh,
                        &freedf_core::pen::stroke_ribbon(&pts_pt, &hb, 0.0, true),
                        &to_view,
                        halo_color,
                    );
                }
            }
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
            self.ink_bleed,
            self.pen_profile,
            self.fountain_profile,
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
    /// current tool's shape and color (Pen = 벡터풍 펜 닙 (틸트 방향 추적),
    /// Highlighter = colored rectangle, Eraser = white translucent circle).
    /// 마우스 + 잉크 도구(mouse_draws 꺼짐)면 팬 십자선으로 표시합니다.
    pub(crate) fn paint_custom_cursor(&self, painter: &egui::Painter, pos: Pos2, _time: f32) {
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
                // ── 벡터풍 펜 닙 커서 (원형 미리보기 제거 — 사용자 요청).
                // 닙 끝이 **실제 좌표**에 고정되고, 배럴이 펜 기울기 방향
                // (방위각)을 가리킵니다. 틸트 소스가 없으면 오른손잡이 기본
                // 각도(위-오른쪽)로 섭니다.
                let (az, cos_pitch) = if self.pen_monitor.is_some() {
                    tilt_azimuth(&self.pen_tilt)
                } else {
                    (DEFAULT_PEN_AZ, 1.0)
                };
                let pitch = cos_pitch.acos();
                // 눕힐수록 배럴은 길게, 폭은 원근으로 좁아집니다.
                let len = 18.0 + 26.0 * pitch.sin();
                let w = (4.5 * cos_pitch).max(1.5);
                let dir = egui::vec2(az.cos(), az.sin());
                let perp = egui::vec2(-dir.y, dir.x);
                let tail = pos + dir * len;
                let tail_l = tail + perp * w;
                let tail_r = tail - perp * w;
                let ink = Color32::from_rgba_unmultiplied(
                    self.pen_color[0],
                    self.pen_color[1],
                    self.pen_color[2],
                    (self.pen_color[3] as f32 * 0.75).max(90.0) as u8,
                );
                // 닙: 좌표에 꼭짓점이 닿는 삼각형 + 둥근 꼬리 캡.
                painter.add(egui::Shape::convex_polygon(
                    vec![pos, tail_r, tail_l],
                    ink,
                    Stroke::new(1.2, Color32::from_white_alpha(190)),
                ));
                painter.circle_filled(tail, w, ink);
                painter.circle_stroke(
                    tail,
                    w,
                    Stroke::new(1.2, Color32::from_white_alpha(190)),
                );
                // 닙 끝(정확 좌표) 강조.
                painter.circle_filled(pos, 1.6, Color32::from_white_alpha(220));
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
        let rb = freedf_core::pen::stroke_ribbon(pts, halves, 0.5, true);
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
            let rb = freedf_core::pen::stroke_ribbon(&pts, &halves, feather, true);
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
}
