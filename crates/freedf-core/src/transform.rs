//! 페이지 좌표계(포인트) ↔ 뷰/캔버스 좌표계(논리 픽셀) 변환.
//!
//! 순수 수학만 담고 있어 GUI 없이 단위 테스트로 검증합니다.

use serde::{Deserialize, Serialize};

/// 줌 배율 상/하한 (화면 픽셀 / 페이지 포인트).
pub const MIN_ZOOM: f32 = 0.08;
pub const MAX_ZOOM: f32 = 16.0;

/// 페이지를 100% 크기(96dpi)로 보여주는 배율.
/// 72포인트 = 1인치, 96픽셀 = 1인치 이므로 96/72.
pub const ZOOM_100_PERCENT: f32 = 96.0 / 72.0;

/// 캔버스에서 페이지의 가로 정렬.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PageAlign {
    Left,
    Center,
    Right,
}

impl PageAlign {
    pub fn label(self) -> &'static str {
        match self {
            PageAlign::Left => "Left",
            PageAlign::Center => "Center",
            PageAlign::Right => "Right",
        }
    }
}

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

    /// 페이지가 캔버스에 세로로 꽉 차는 배율. (상하 여백 포함)
    pub fn fit_height_zoom(page_h_pts: f32, canvas_h: f32, margin: f32) -> f32 {
        let usable = (canvas_h - 2.0 * margin).max(1.0);
        (usable / page_h_pts.max(1.0)).clamp(MIN_ZOOM, MAX_ZOOM)
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

    /// 페이지를 캔버스에 세로 위쪽 정렬 + 가로 정렬(왼쪽/가운데/오른쪽)로 배치합니다.
    /// `top_margin`은 캔버스 위쪽에서 페이지까지의 여백.
    pub fn align_page(
        &mut self,
        page: [f32; 2],
        canvas: [f32; 2],
        top_margin: f32,
        align: PageAlign,
    ) {
        let view_size = self.page_size_to_view(page[0], page[1]);
        let m = top_margin;
        self.pan_x = match align {
            PageAlign::Left => m,
            PageAlign::Center => ((canvas[0] - view_size[0]) / 2.0).max(m),
            PageAlign::Right => (canvas[0] - view_size[0] - m).max(m),
        };
        self.pan_y = m;
    }

    /// 팬을 제한해 페이지가 캔버스 밖으로 무한정 사라지지 않게 합니다.
    /// 페이지는 캔버스 가장자리에서 `margin` 이상 벗어날 수 없습니다.
    /// 페이지가 캔버스보다 작으면 여백 범위 안에서만 이동합니다.
    pub fn clamp_pan(&mut self, page: [f32; 2], canvas: [f32; 2], margin: f32) {
        let view_size = self.page_size_to_view(page[0], page[1]);
        // 페이지 왼쪽/위쪽이 캔버스 왼쪽/위에서 margin 밖으로 나가지 않고,
        // 페이지 오른쪽/아래가 캔버스 오른쪽/아래에서 margin 밖으로 나가지 않게 제한.
        let min_x = canvas[0] - view_size[0] - margin;
        let max_x = margin;
        let lo_x = min_x.min(max_x);
        let hi_x = min_x.max(max_x);
        self.pan_x = self.pan_x.clamp(lo_x, hi_x);

        let min_y = canvas[1] - view_size[1] - margin;
        let max_y = margin;
        let lo_y = min_y.min(max_y);
        let hi_y = min_y.max(max_y);
        self.pan_y = self.pan_y.clamp(lo_y, hi_y);
    }
}

/// 브라우저식 PgUp/PgDn 한 단계의 결과.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PageStep {
    /// 페이지 안에서 세로 스크롤 — 새 `pan_y`로 갱신하면 됩니다.
    ScrollTo { pan_y: f32 },
    /// 아래로 더 스크롤할 수 없음 → 다음 페이지로 이동.
    NextPage,
    /// 위로 더 스크롤할 수 없음 → 이전 페이지로 이동.
    PrevPage,
}

/// 브라우저처럼 PgDn/PgUp 한 번을 계산합니다 (순수 수학 — 테스트로 정의).
///
/// 규칙:
/// - 페이지가 캔버스 세로보다 **작으면**(스크롤 불가) → PgDn은 `NextPage`,
///   PgUp은 `PrevPage`.
/// - 페이지가 **더 크면** 한 뷰포트(캔버스 높이)만큼 이동합니다.
///   `pan_y`(페이지 위쪽의 캔버스 y)는 `top = margin`에서
///   `bottom = canvas_h - view_h - margin` 사이입니다.
///   - PgDn: 이미 바닥(bottom)이면 `NextPage`, 아니면 `pan_y - canvas_h`로
///     (바닥을 넘지 않게 clamp).
///   - PgUp: 이미 상단(top)이면 `PrevPage`, 아니면 `pan_y + canvas_h`로
///     (상단을 넘지 않게 clamp).
pub fn browser_page_step(
    page_h_pts: f32,
    zoom: f32,
    canvas_h: f32,
    margin: f32,
    pan_y: f32,
    down: bool,
) -> PageStep {
    let view_h = page_h_pts * zoom;
    let top = margin;
    let bottom = canvas_h - view_h - margin;
    if view_h <= canvas_h {
        return if down {
            PageStep::NextPage
        } else {
            PageStep::PrevPage
        };
    }
    const EPS: f32 = 0.5;
    if down {
        if (pan_y - bottom).abs() <= EPS {
            return PageStep::NextPage;
        }
        PageStep::ScrollTo {
            pan_y: (pan_y - canvas_h).max(bottom),
        }
    } else {
        if (pan_y - top).abs() <= EPS {
            return PageStep::PrevPage;
        }
        PageStep::ScrollTo {
            pan_y: (pan_y + canvas_h).min(top),
        }
    }
}
