//! 원형 색상 휠(굿노트식 컬러 팔레트)의 **순수 로직**.
//!
//! 화면에 그리는 것과 무관하게 "스와치가 어디에 있는지"와 "탭이 어디에
//! 닿았는지"만 계산합니다 — egui 없이 순수 계산이라 단위 테스트로
//! 완전히 검증할 수 있습니다.
//!
//! **히트테스트 수학의 소유자는 장치/라우터 축**(`app::input::wheel_sink`의
//! [`WheelGeom`])이다: 같은 프레스를 잉크와 휠이 **같은 기하**로 판정해야
//! 하므로, 판정식을 오버레이(egui) 쪽에 두지 않는다. 이 파일은 레이아웃
//! 상수에서 기하를 만들어 넘기고(렌더 = 판정), 그리기를 담당한다.
//!
//! `WheelHit`/`WheelGeom`은 여기서 재수출된다 — 오버레이/테스트가 쓰는 이름은
//! 그대로다 (정의만 소유자가 옮겨졌다).

use super::*;

pub(crate) use crate::app::input::wheel_sink::WheelGeom;

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

    /// 레이아웃 상수 → 판정 기하. **렌더와 판정이 같은 값**을 쓰게 하는 다리다
    /// (앱이 이 값을 휠 싱크에 주입한다).
    pub fn geom(&self) -> WheelGeom {
        WheelGeom {
            center: [self.center.x, self.center.y],
            back_r: WHEEL_BACK_R,
            ring_r: WHEEL_RING_R,
            swatch_r: WHEEL_SWATCH_R,
            center_r: WHEEL_CENTER_R,
            ring_len: self.ring.len(),
        }
    }

    /// i번째 스와치의 위치 — 12시 방향부터 시계 방향으로 균등 배치.
    pub fn swatch_pos(&self, i: usize) -> Pos2 {
        debug_assert!(!self.ring.is_empty(), "휠에 색이 하나도 없습니다");
        let p = self.geom().swatch_pos(i);
        egui::pos2(p[0], p[1])
    }
}
