//! view — 액션 실행 로직 (자세한 내용은 mod.rs 참조).

use super::*;

impl FreeDfApp {
    /// 줌이 바뀐 뒤 **고품질 재렌더를 잠시 미룹니다** (ZOON-OPT.md 연속 줌).
    /// 연속 줌(버튼 연타/핀치/Ctrl+휠) 동안 매 스텝 재렌더가 쌓여 멈추던
    /// 것을 막기 위해, 마지막 줌 변경 시각으로부터 정착 데드라인을 잡고
    /// 그 전까지는 이전 텍스처를 스케일해 표시합니다. 데드라인이 지나면
    /// `ensure_texture`가 한 번 고품질로 재렌더합니다.
    pub(crate) fn mark_zoom_dirty(&mut self) {
        // 120ms 동안 추가 줌이 없으면 재렌더 — 연속 입력 중엔 계속 미룹니다.
        const ZOOM_SETTLE_MS: u64 = 120;
        self.zoom_settle_deadline_ms = now_ms().saturating_add(ZOOM_SETTLE_MS);
        self.zoom_render_pending = true;
    }

    pub(crate) fn zoom_by(&mut self, factor: f32) {
        if self.zoom_lock {
            return;
        }
        let anchor = [self.last_canvas[0] * 0.5, self.last_canvas[1] * 0.5];
        self.view.zoom_at(anchor, factor, MIN_ZOOM, MAX_ZOOM);
        self.mark_zoom_dirty();
        self.save_session();
    }

    pub(crate) fn fit_width(&mut self) {
        if self.zoom_lock {
            return;
        }
        self.pending_fit = Some(FitMode::Width);
    }

    pub(crate) fn fit_height(&mut self) {
        if self.zoom_lock {
            return;
        }
        self.pending_fit = Some(FitMode::Height);
    }

    /// Applies a pending fit once the canvas size is known.
    pub(crate) fn apply_pending_fit(&mut self, canvas: [f32; 2]) {
        let Some(mode) = self.pending_fit else {
            return;
        };
        // 캔버스가 아직 잡히지 않았으면(창 초기화 등) 소비하지 말고
        // 다음 프레임에 재시도 — 첫 프레임에서 fit이 유실돼 정렬이
        // 어긋나던 버그 방지.
        if canvas[0] <= 1.0 || canvas[1] <= 1.0 {
            return;
        }
        self.pending_fit = None;
        match mode {
            FitMode::Width => {
                self.view.zoom =
                    ViewTransform::fit_width_zoom(self.page_size_pts[0], canvas[0], CANVAS_MARGIN);
            }
            FitMode::Height => {
                self.view.zoom =
                    ViewTransform::fit_height_zoom(self.page_size_pts[1], canvas[1], CANVAS_MARGIN);
            }
        }
        self.view
            .align_page(self.page_size_pts, canvas, TOP_MARGIN, self.page_align);
        self.render_dirty = true;
        self.save_session();
    }

    /// Re-applies the current horizontal alignment without changing the zoom.
    pub(crate) fn realign(&mut self) {
        if self.document.is_none() {
            return;
        }
        self.view
            .align_page(self.page_size_pts, self.last_canvas, TOP_MARGIN, self.page_align);
        self.save_session();
    }

    // ---------- Undo / redo / clear ----------
}
