//! 필기/메모 스트로크의 데이터 모델.
//!
//! 모든 좌표는 **페이지 좌표계**(PDF 포인트, 1/72인치)로 저장됩니다.
//! 화면 줌/팬과 무관하게 원본 위치를 유지하기 위함입니다.

use serde::{Deserialize, Serialize};

/// 그리기 도구 종류.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolType {
    /// 펜 — 불투명한 필기선
    Pen,
    /// 형광펜 — 투명한 두꺼운 강조선
    Highlighter,
    /// 지우개 — 스트로크 벡터 삭제
    Eraser,
    /// 이동(팬) — 화면 스크롤
    Pan,
}

impl ToolType {
    /// 툴바 표시용 한글 이름.
    pub fn label(self) -> &'static str {
        match self {
            ToolType::Pen => "펜",
            ToolType::Highlighter => "형광펜",
            ToolType::Eraser => "지우개",
            ToolType::Pan => "이동",
        }
    }
}

/// 페이지 좌표계(포인트)로 표현되는 스트로크의 한 점.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StrokePoint {
    /// 페이지 좌표 X (포인트)
    pub x: f32,
    /// 페이지 좌표 Y (포인트)
    pub y: f32,
    /// 태블릿 펜 압력 0.0 ~ 1.0. 압력 미지원 입력 장치는 보통 0.5.
    pub pressure: f32,
}

impl StrokePoint {
    pub fn new(x: f32, y: f32, pressure: f32) -> Self {
        Self {
            x,
            y,
            pressure: pressure.clamp(0.0, 1.0),
        }
    }

    pub fn to_array(&self) -> [f32; 2] {
        [self.x, self.y]
    }
}

/// 하나의 스트로크(획). 좌표는 전부 페이지 좌표계 기준.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    /// 저장소 내 고유 ID
    pub id: u64,
    pub tool: ToolType,
    /// RGBA 색상
    pub color: [u8; 4],
    /// 페이지 좌표계 기준 선 두께 (포인트)
    pub width: f32,
    /// 스트로크를 구성하는 점들 (2개 이상일 때 선, 1개일 때 점)
    pub points: Vec<StrokePoint>,
}

impl Stroke {
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// [min_x, min_y, max_x, max_y] 경계 상자.
    pub fn bounding_box(&self) -> Option<[f32; 4]> {
        let (mut min_x, mut min_y) = (f32::INFINITY, f32::INFINITY);
        let (mut max_x, mut max_y) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
        for p in &self.points {
            min_x = min_x.min(p.x);
            min_y = min_y.min(p.y);
            max_x = max_x.max(p.x);
            max_y = max_y.max(p.y);
        }
        if self.points.is_empty() {
            None
        } else {
            Some([min_x, min_y, max_x, max_y])
        }
    }

    /// 어떤 점이라도 `point`를 중심으로 반지름 `radius`(페이지 좌표) 안에 있는지.
    pub fn any_point_within(&self, point: [f32; 2], radius: f32) -> bool {
        let r2 = radius * radius;
        self.points
            .iter()
            .any(|p| (p.x - point[0]).powi(2) + (p.y - point[1]).powi(2) <= r2)
    }
}

/// 페이지 번호 (0부터 시작).
pub type PageIndex = usize;
