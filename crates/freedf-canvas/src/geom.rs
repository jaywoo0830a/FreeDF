//! 기하 기본 타입 — 페이지 좌표와 뷰 변환 (순수 함수).
//!
//! 좌표계 혼동(페이지 pt ↔ 화면 px)은 캔버스 버그의 고전 원인입니다.
//! 여기서는 `PagePoint` 뉴타입으로 페이지 좌표를 명시하고, 변환은
//! [`ViewTransform`]의 순수 함수로만 수행합니다.

/// 페이지 좌표 (pt) — 원점은 페이지 왼쪽 위.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PagePoint {
    pub x: f32,
    pub y: f32,
}

impl PagePoint {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// 페이지 크기 (pt).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PageSize {
    pub width: f32,
    pub height: f32,
}

impl PageSize {
    pub fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

/// 페이지 ↔ 화면 변환 — 줌(px/pt)과 팬(px).
///
/// **계약**: 이 변환만 좌표계를 바꿉니다. 메시는 항상 페이지 좌표로 굽고,
/// 그릴 때 이 변환을 곱합니다 (팬만 바뀌면 재굽기 없이 정점 이동).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewTransform {
    pub zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
}

impl Default for ViewTransform {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
        }
    }
}

impl ViewTransform {
    pub fn new(zoom: f32, pan_x: f32, pan_y: f32) -> Self {
        Self { zoom, pan_x, pan_y }
    }

    /// 페이지 pt → 화면 px.
    pub fn page_to_view(&self, p: PagePoint) -> PagePoint {
        PagePoint::new(p.x * self.zoom + self.pan_x, p.y * self.zoom + self.pan_y)
    }

    /// 화면 px → 페이지 pt.
    pub fn view_to_page(&self, p: PagePoint) -> PagePoint {
        PagePoint::new(
            (p.x - self.pan_x) / self.zoom,
            (p.y - self.pan_y) / self.zoom,
        )
    }

    /// 줌을 안전 범위로 클램프한 사본 (팬 불변).
    pub fn clamped_zoom(&self, min: f32, max: f32) -> Self {
        Self {
            zoom: self.zoom.clamp(min, max),
            ..*self
        }
    }
}
