//! 페이지 좌표계(포인트) ↔ 뷰/캔버스 좌표계(논리 픽셀) 변환.
//!
//! 순수 수학만 담고 있어 GUI 없이 단위 테스트로 검증합니다.

/// 줌 배율 상/하한 (화면 픽셀 / 페이지 포인트).
pub const MIN_ZOOM: f32 = 0.08;
pub const MAX_ZOOM: f32 = 16.0;

/// 페이지를 100% 크기(96dpi)로 보여주는 배율.
/// 72포인트 = 1인치, 96픽셀 = 1인치 이므로 96/72.
pub const ZOOM_100_PERCENT: f32 = 96.0 / 72.0;

/// 뷰 변환. 페이지 좌상단이 캔버스의 `(pan_x, pan_y)`에 놓이고,
/// 1페이지 포인트가 `zoom` 논리 픽셀로 표시됩니다.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewTransform {
    /// 화면 픽셀 / 페이지 포인트
    pub zoom: f32,
    /// 캔버스(뷰포트)에서 페이지 좌상단의 X 오프셋
    pub pan_x: f32,
    /// 캔버스(뷰포트)에서 페이지 좌상단의 Y 오프셋
    pub pan_y: f32,
}

impl Default for ViewTransform {
    fn default() -> Self {
        Self::identity()
    }
}

impl ViewTransform {
    pub fn identity() -> Self {
        Self {
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
        }
    }

    /// 페이지 좌표 → 뷰(캔버스) 좌표.
    pub fn page_to_view(&self, p: [f32; 2]) -> [f32; 2] {
        [p[0] * self.zoom + self.pan_x, p[1] * self.zoom + self.pan_y]
    }

    /// 뷰(캔버스) 좌표 → 페이지 좌표.
    pub fn view_to_page(&self, v: [f32; 2]) -> [f32; 2] {
        [
            (v[0] - self.pan_x) / self.zoom,
            (v[1] - self.pan_y) / self.zoom,
        ]
    }

    /// 페이지 크기(포인트) → 뷰 크기(픽셀).
    pub fn page_size_to_view(&self, w_pts: f32, h_pts: f32) -> [f32; 2] {
        [w_pts * self.zoom, h_pts * self.zoom]
    }

    /// `anchor`(뷰 좌표)를 고정한 채 줌 배율에 `factor`를 곱합니다.
    /// 줌이 바뀌어도 마우스 커서가 가리키는 페이지 지점이 화면에 머물게 됩니다.
    pub fn zoom_at(&mut self, anchor: [f32; 2], factor: f32, min_zoom: f32, max_zoom: f32) {
        let page_point = self.view_to_page(anchor);
        let new_zoom = (self.zoom * factor).clamp(min_zoom, max_zoom);
        let zoom_factor = new_zoom / self.zoom;
        self.zoom = new_zoom;
        // anchor = page_point * new_zoom + new_pan  →  new_pan = anchor - page_point*new_zoom
        self.pan_x = anchor[0] - page_point[0] * self.zoom;
        self.pan_y = anchor[1] - page_point[1] * self.zoom;
        let _ = zoom_factor;
    }

    /// 줌 배율만 바꾸고 페이지 중심을 유지합니다.
    pub fn set_zoom_keep_center(&mut self, new_zoom: f32, page_center: [f32; 2], canvas: [f32; 2]) {
        let center = [canvas[0] / 2.0, canvas[1] / 2.0];
        let zoom = new_zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        self.zoom = zoom;
        self.pan_x = center[0] - page_center[0] * zoom;
        self.pan_y = center[1] - page_center[1] * zoom;
    }

    /// 뷰 좌표를 `(dx, dy)`만큼 이동합니다.
    pub fn pan_by(&mut self, dx: f32, dy: f32) {
        self.pan_x += dx;
        self.pan_y += dy;
    }

    /// 줌을 상/하한 안으로 조정합니다.
    pub fn clamp_zoom(&mut self) {
        self.zoom = self.zoom.clamp(MIN_ZOOM, MAX_ZOOM);
    }

    /// 페이지가 캔버스에 가로로 꽉 차는 배율. (좌우 여백 포함)
    pub fn fit_width_zoom(page_w_pts: f32, canvas_w: f32, margin: f32) -> f32 {
        let usable = (canvas_w - 2.0 * margin).max(1.0);
        (usable / page_w_pts.max(1.0)).clamp(MIN_ZOOM, MAX_ZOOM)
    }

    /// 페이지 전체(가로+세로)가 캔버스에 들어가는 배율.
    pub fn fit_page_zoom(page: [f32; 2], canvas: [f32; 2], margin: f32) -> f32 {
        let usable_w = (canvas[0] - 2.0 * margin).max(1.0);
        let usable_h = (canvas[1] - 2.0 * margin).max(1.0);
        let z = (usable_w / page[0].max(1.0)).min(usable_h / page[1].max(1.0));
        z.clamp(MIN_ZOOM, MAX_ZOOM)
    }

    /// 페이지를 캔버스 중앙에 세로 위쪽 정렬로 배치합니다.
    /// `top_margin`은 캔버스 위쪽에서 페이지까지의 여백.
    pub fn center_page(&mut self, page: [f32; 2], canvas: [f32; 2], top_margin: f32) {
        let view_size = self.page_size_to_view(page[0], page[1]);
        self.pan_x = ((canvas[0] - view_size[0]) / 2.0).max(0.0);
        self.pan_y = top_margin;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(a: [f32; 2], b: [f32; 2], eps: f32) {
        assert!(
            (a[0] - b[0]).abs() <= eps && (a[1] - b[1]).abs() <= eps,
            "{a:?} vs {b:?}"
        );
    }

    #[test]
    fn round_trip_page_view() {
        let mut t = ViewTransform::identity();
        t.zoom = 2.5;
        t.pan_x = 100.0;
        t.pan_y = -50.0;
        let page = [300.0, 200.0];
        let view = t.page_to_view(page);
        let back = t.view_to_page(view);
        assert_close(back, page, 1e-3);
    }

    #[test]
    fn zoom_at_keeps_anchor_fixed() {
        let mut t = ViewTransform {
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
        };
        let anchor = [400.0, 300.0]; // 뷰 좌표
        t.zoom_at(anchor, 2.0, MIN_ZOOM, MAX_ZOOM);
        // anchor가 가리키던 페이지 지점이 여전히 anchor에 있어야 함.
        assert_close(t.page_to_view(t.view_to_page(anchor)), anchor, 1e-3);
        assert!((t.zoom - 2.0).abs() < 1e-6);
    }

    #[test]
    fn zoom_at_then_zoom_out_returns() {
        let mut t = ViewTransform::identity();
        let anchor = [123.0, 456.0];
        t.zoom_at(anchor, 3.0, MIN_ZOOM, MAX_ZOOM);
        t.zoom_at(anchor, 1.0 / 3.0, MIN_ZOOM, MAX_ZOOM);
        assert!((t.zoom - 1.0).abs() < 1e-3);
        assert!((t.pan_x - 0.0).abs() < 1e-3);
        assert!((t.pan_y - 0.0).abs() < 1e-3);
    }

    #[test]
    fn zoom_is_clamped() {
        let mut t = ViewTransform::identity();
        t.zoom_at([0.0, 0.0], 10_000.0, MIN_ZOOM, MAX_ZOOM);
        assert!((t.zoom - MAX_ZOOM).abs() < 1e-6);
        t.zoom_at([0.0, 0.0], 1e-9, MIN_ZOOM, MAX_ZOOM);
        assert!((t.zoom - MIN_ZOOM).abs() < 1e-6);
    }

    #[test]
    fn pan_moves_page() {
        let mut t = ViewTransform::identity();
        t.pan_by(30.0, -20.0);
        assert_close([t.pan_x, t.pan_y], [30.0, -20.0], 1e-6);
        // 팬 후 원점의 페이지 좌표는 (-30, 20) 방향으로 이동
        assert_close(t.view_to_page([30.0, -20.0]), [0.0, 0.0], 1e-6);
    }

    #[test]
    fn fit_width_zoom_respects_aspect() {
        let zoom = ViewTransform::fit_width_zoom(595.0, 1200.0, 16.0);
        let view_w = 595.0 * zoom;
        assert!((view_w - 1168.0).abs() < 1.0);
    }

    #[test]
    fn fit_page_zoom_never_exceeds() {
        // 세로가 더 긴 A4: 595 x 842
        let zoom = ViewTransform::fit_page_zoom([595.0, 842.0], [1000.0, 800.0], 16.0);
        let [w, h] = [595.0 * zoom, 842.0 * zoom];
        assert!(w <= 968.0 + 1.0 && h <= 768.0 + 1.0);
    }

    #[test]
    fn center_page_places_top_left() {
        let mut t = ViewTransform {
            zoom: 2.0,
            pan_x: 9999.0,
            pan_y: 9999.0,
        };
        t.center_page([595.0, 842.0], [1400.0, 1000.0], 24.0);
        let view = t.page_to_view([0.0, 0.0]);
        // 페이지가 가로 중앙 + 상단 여백에 배치됨
        assert!((view[0] - (1400.0 - 595.0 * 2.0) / 2.0).abs() < 1e-3);
        assert!((view[1] - 24.0).abs() < 1e-3);
    }
}
