//! 문서 커맨드 실행기 — 워크스페이스가 생산한 커맨드를 앱 상태에 적용한다.
//!
//! ideation `canvas.js`의 projection에 해당하는 **앱 경계**다: 툴은 이 파일을
//! 모르고(커맨드만 생산), 이 파일은 툴을 모른다(커맨드만 소비). 기존
//! `input.rs`의 잉크 시작/확장/지우기 로직이 여기로 이관됐다 — 세세한 정책
//! (리프트 컷, 페이지 경계, 압력 소스)은 모두 보존.
//!
//! 렌더 상태(`self.tool`)는 워크스페이스의 파생 캐시다 — 진실원은
//! 워크스페이스(툴 레지스트리·홀드·획 경계 전환).

use super::*;
use freedf_core::input_commands::Command;

impl FreeDfApp {
    /// 커맨드 1건 적용. 매 프레임 워크스페이스 큐를 순서대로 흘린다.
    ///
    /// 두 소비자로 갈린다:
    /// ① **문서/세션 실행** — InkPipeline·store·db·history (툴의 의도를 문서에 반영)
    /// ② **렌더 투영** — 같은 커맨드를 CanvasSurface 연산으로 번역 (열린 레지스트리;
    ///    미처리 커맨드는 조용한 데이터 손실 금지 — 진단 로그)
    ///
    /// 반환값: 렌더 투영 성공 여부 — `false`면 이번 커맨드의 렌더가 누락됐다
    /// (가설 3 변별 지표: 프레임당 누적이 FRAME-DIAG의 `proj_err`로 나온다).
    pub(crate) fn execute_command(
        &mut self,
        cmd: Command,
        ctx: &egui::Context,
        origin: Pos2,
        canvas_size: [f32; 2],
    ) -> bool {
        match &cmd {
            Command::BeginStroke { tool, point, .. } => {
                self.begin_stroke_cmd(tool, *point, ctx, origin);
            }
            Command::ExtendStroke { point, .. } => self.extend_stroke_cmd(*point, ctx, origin),
            Command::EndStroke => {
                if self.active_stroke.is_some() {
                    self.finish_stroke();
                }
            }
            Command::EraseAt { point } => self.erase_at_cmd(*point, origin),
            Command::EndErase => {}
            Command::Undo => self.undo(),
            Command::Immediate { key } => self.immediate_cmd(key, ctx, origin, canvas_size),
        }

        // 렌더 투영 — take 중 프로젝션을 밖에 꺼내 소유권 충돌을 피한다.
        let mut projection = std::mem::take(&mut self.projection);
        let ok = match projection.project(std::slice::from_ref(&cmd), self) {
            Ok(_) => true,
            Err(e) => {
                pen_trace(&format!("PROJECTION-ERROR: {e}"));
                false
            }
        };
        self.projection = projection;
        ok
    }

    /// begin-stroke — 새 잉크 세션 시작 (기존 input.rs 스트로크 시작 로직).
    fn begin_stroke_cmd(
        &mut self,
        tool: &str,
        point: [f32; 2],
        ctx: &egui::Context,
        origin: Pos2,
    ) {
        if self.active_stroke.is_some() {
            return; // 세션 중복 방어 — 워크스페이스 계약상 없어야 한다
        }
        let page_w = self.page_size_pts[0];
        let page_h = self.page_size_pts[1];
        let p_ui = [point[0] - origin.x, point[1] - origin.y];
        let raw = self.view.view_to_page(p_ui);
        // 페이지(캔버스) 바깥에서는 필기 금지: 페이지 내부에서만 스트로크를
        // 시작하고, 벗어나면 점을 추가하지 않습니다 (기존 정책).
        let inside = raw[0] >= 0.0 && raw[0] <= page_w && raw[1] >= 0.0 && raw[1] <= page_h;
        if !inside {
            return;
        }
        let page = [raw[0].clamp(0.0, page_w), raw[1].clamp(0.0, page_h)];
        let (pressure, p_src) = self.pressure_source(ctx);
        // 시작/드래그 공용 시각 — clock 불변 대여와의 충돌을 피하려고 미리 읽는다.
        let drag_t = ctx.input(|i| i.time);
        let drag_t_ms = self.now_ms();
        let (color, width) = self.current_drawing_style();
        let smooth = if self.smoothing_enabled && self.smoothing > 0.001 {
            self.smoothing
        } else {
            0.0
        };
        let tilt = tilt_magnitude(&self.pen_tilt);
        let mut pipeline = freedf_core::pipeline::InkPipeline::new(
            Materials::new(self.pen_profile, self.fountain_profile),
            width,
            smooth,
        );
        let tip = pipeline.down(
            self.tool,
            color,
            page[0],
            page[1],
            pressure,
            drag_t,
            drag_t_ms,
            tilt,
        );
        self.ink = Some(pipeline);
        self.active_stroke = Some(ActiveStroke {
            tool: self.tool,
            color,
            width,
            points: vec![tip],
        });
        self.lift_cut_logged = false;
        pen_trace(&format!(
            "stroke start: tool={:?} (pkg:{tool}) base_w={width:.1}pt pressure_enabled={} device={:?} p_k={:.2} s_k={:.2} src={p_src} tilt=[{:+.0},{:+.0}]",
            self.tool,
            self.pressure_enabled,
            self.input_device,
            self.pen_profile.pressure_k,
            self.pen_profile.speed_k,
            self.pen_tilt[0],
            self.pen_tilt[1]
        ));
    }

    /// extend-stroke — 진행 중 획에 점 추가 (기존 input.rs 드래그 로직).
    fn extend_stroke_cmd(&mut self, point: [f32; 2], ctx: &egui::Context, origin: Pos2) {
        // 압력/시각은 `st` 가변 대여 이전에 미리 읽는다 (self 전체 불변 대여와의
        // 대여 충돌 회피 — 기존 코드와 동일한 순서).
        let (pressure, p_src) = self.pressure_source(ctx);
        let drag_t = ctx.input(|i| i.time);
        let drag_t_ms = self.now_ms();
        let Some(st) = self.active_stroke.as_mut() else {
            return;
        };
        let page_w = self.page_size_pts[0];
        let page_h = self.page_size_pts[1];
        let p_ui = [point[0] - origin.x, point[1] - origin.y];
        let raw = self.view.view_to_page(p_ui);
        // 세션 중 페이지를 벗어나면 점을 추가하지 않는다 (기존 정책).
        let inside = raw[0] >= 0.0 && raw[0] <= page_w && raw[1] >= 0.0 && raw[1] <= page_h;
        if !inside {
            return;
        }
        let page = [raw[0].clamp(0.0, page_w), raw[1].clamp(0.0, page_h)];
        // ── 펜 떼기 직전 처리: 접촉이 해제됐거나 필압이 사실상 0으로 무너진
        // 꼬리 리포트는 **버립니다** — 펜 떼는 순간 끝이 갑자기 가늘어지는
        // "확 바뀜"의 원인이었습니다. (첫 점 4개는 접촉 시작 타이밍 차이로
        // 잘릴 수 있으니 점이 쌓인 뒤에만 적용)
        let pen_lifted = !self.input_sources.pen_contact();
        // 직전에는 힘이 있었는데 지금 1% 미만 → 리프트 꼬리.
        let pressure_collapsed = pressure <= 0.01
            && st.points.last().is_some_and(|q| q.pressure > 0.05);
        let contact_lost = st.points.len() >= 4 && (pen_lifted || pressure_collapsed);
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
            // InkPipeline이 필터 → 폭 확정 → 점 추가까지 한 번에 처리합니다
            // (마지막 점은 펜업에서 확정).
            if let Some(p) = self.ink.as_mut() {
                p.drag(page[0], page[1], pressure, drag_t, drag_t_ms);
            }
            // 렌더 미러를 파이프라인 라이브 점(중간 확정 포함)과 동기화해
            // WYSIWYG을 보존합니다 (렌더 == 커밋).
            if let Some(p) = &self.ink {
                st.points = p.live().map(|l| l.points.clone()).unwrap_or_default();
            }
            // 진단: 25점마다 압력/잠금 폭을 남깁니다.
            if st.points.len() % 25 == 0 {
                pen_trace(&format!(
                    "pt {}: pressure={pressure:.3} (src={p_src}) locked_w={:.3}",
                    st.points.len(),
                    st.points.last().map(|q| q.width).unwrap_or(0.0)
                ));
            }
        }
    }

    /// erase-at — 지우기 세션 (기존 input.rs 지우개 로직).
    fn erase_at_cmd(&mut self, point: [f32; 2], origin: Pos2) {
        let p_ui = [point[0] - origin.x, point[1] - origin.y];
        let page = self.view.view_to_page(p_ui);
        let radius = self.eraser_radius / self.view.zoom;
        let removed = self.store.erase_at(self.current_page, page, radius);
        if !removed.is_empty() {
            // 지워진 행만 DB에서 삭제 (증분).
            if let Some(doc_id) = self.doc_id {
                let ids: Vec<i64> = removed.iter().map(|s| s.id as i64).collect();
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

    /// 즉시 커맨드 — 워크스페이스가 모르는 action 키의 도착지.
    fn immediate_cmd(
        &mut self,
        key: &str,
        ctx: &egui::Context,
        origin: Pos2,
        canvas_size: [f32; 2],
    ) {
        if key == "color-wheel" {
            // 펜 위치(버튼을 누른 순간의 포인터, 없으면 캔버스 중심)에 엽니다.
            if !self.color_wheel_open {
                self.color_wheel_anchor = ctx
                    .input(|i| i.pointer.hover_pos())
                    .map(|p| [p.x - origin.x, p.y - origin.y])
                    .unwrap_or([canvas_size[0] * 0.5, canvas_size[1] * 0.5]);
            }
            self.on_pen_button(1, true);
        }
        // 알 수 없는 즉시 커맨드 — 조용히 무시 (미지원 확장 안전).
    }
}
