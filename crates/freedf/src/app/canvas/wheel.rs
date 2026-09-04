//! 원형 색상 휠(굿노트식 컬러 팔레트)의 **순수 로직**.
//!
//! 화면에 그리는 것과 무관하게 "스와치가 어디에 있는지"와 "탭이 어디에
//! 닿았는지"만 계산합니다 — egui 없이 순수 계산이라 단위 테스트로
//! 완전히 검증할 수 있습니다.

use super::*;

/// 탭이 휠의 어디에 닿았는지.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WheelHit {
    /// 중앙(현재 색) — 변경 없이 닫기.
    Center,
    /// 둘레 i번째 색 — 그 색을 적용.
    Swatch(usize),
    /// 뒷판 빈 곳 — 그냥 닫기.
    Backplate,
    /// 휠 바깥 — 닫지 않고 무시.
    Outside,
}

/// 원형 색상 휠 — 중심 좌표와 둘레 색 목록만 갖는 아주 작은 객체입니다.
pub(crate) struct ColorWheel {
    pub center: Pos2,
    pub ring: Vec<[u8; 4]>,
}

impl ColorWheel {
    /// 펜 위치(앵커)를 캔버스 안으로 밀어 넣은 휠 중심.
    ///
    /// 캔버스가 휠보다 작으면 그냥 캔버스 중앙을 돌려줍니다.
    pub fn clamp_center(anchor: Pos2, canvas: Rect) -> Pos2 {
        if canvas.width() < WHEEL_BACK_R * 2.0 || canvas.height() < WHEEL_BACK_R * 2.0 {
            return canvas.center();
        }
        egui::pos2(
            anchor
                .x
                .clamp(canvas.min.x + WHEEL_BACK_R, canvas.max.x - WHEEL_BACK_R),
            anchor
                .y
                .clamp(canvas.min.y + WHEEL_BACK_R, canvas.max.y - WHEEL_BACK_R),
        )
    }

    /// i번째 스와치의 위치 — 12시 방향부터 시계 방향으로 균등 배치.
    pub fn swatch_pos(&self, i: usize) -> Pos2 {
        debug_assert!(!self.ring.is_empty(), "휠에 색이 하나도 없습니다");
        let angle = -std::f32::consts::TAU / 4.0
            + std::f32::consts::TAU * (i as f32) / (self.ring.len() as f32);
        self.center + egui::vec2(angle.cos(), angle.sin()) * WHEEL_RING_R
    }

    /// 탭 좌표가 휠의 어느 부분인지 판정합니다.
    ///
    /// 순서: 중앙 → 바깥 → 둘레 스와치 → 뒷판 빈 곳.
    pub fn hit(&self, pos: Pos2) -> WheelHit {
        if pos.distance(self.center) <= WHEEL_CENTER_R {
            return WheelHit::Center;
        }
        if pos.distance(self.center) > WHEEL_BACK_R {
            return WheelHit::Outside;
        }
        for i in 0..self.ring.len() {
            if pos.distance(self.swatch_pos(i)) <= WHEEL_SWATCH_R + 3.0 {
                return WheelHit::Swatch(i);
            }
        }
        WheelHit::Backplate
    }
}
