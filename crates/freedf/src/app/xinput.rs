//! XInput(Xbox) 컨트롤러 — 프로세스가 살아있는 동안 매 프레임 폴링.
//!
//! 오른쪽 스틱 = 상하좌우 스크롤, LB = CTRL(스틱 상하 → 줌),
//! LT = Ctrl+Z(되돌리기). XInput은 Windows 전용 API라 비-Windows에서는
//! [`poll_gamepad`]가 항상 `None`을 반환합니다 (안전한 no-op).

use super::*;

/// 이번 프레임의 게임패드 상태 (데드존 제거, -1..=1 정규화).
pub(crate) struct Gamepad {
    /// 오른쪽 스틱 — x: 오른쪽=+, y: 위=+.
    pub stick: Vec2,
    /// LB(왼쪽 범퍼) — CTRL 역할.
    pub lb: bool,
    /// LT(왼쪽 트리거) 0..=1 (0 = 안 당김).
    pub lt: f32,
}

/// 오른쪽 스틱 값을 -1..=1로 정규화 (데드존은 0으로 잘라냄).
#[cfg(windows)]
fn stick_axis(v: f32, deadzone: f32) -> f32 {
    if v.abs() <= deadzone {
        0.0
    } else {
        (v - v.signum() * deadzone) / (32767.0 - deadzone)
    }
}

#[cfg(not(windows))]
pub(crate) fn poll_gamepad() -> Option<Gamepad> {
    None
}

#[cfg(windows)]
pub(crate) fn poll_gamepad() -> Option<Gamepad> {
    use windows::Win32::UI::Input::XboxController::{
        XInputGetState, XINPUT_GAMEPAD_LEFT_SHOULDER, XINPUT_GAMEPAD_RIGHT_THUMB_DEADZONE,
        XINPUT_STATE,
    };
    unsafe {
        let mut state = XINPUT_STATE::default();
        // 0 = 성공(연결됨), 그 외 = 컨트롤러 없음.
        if XInputGetState(0, &mut state) != 0 {
            return None;
        }
        let g = state.Gamepad;
        // windows 0.62는 상수가 XINPUT_GAMEPAD_BUTTON_FLAGS(u16) newtype.
        let dz = XINPUT_GAMEPAD_RIGHT_THUMB_DEADZONE.0 as f32;
        Some(Gamepad {
            stick: egui::vec2(
                stick_axis(g.sThumbRX as f32, dz),
                stick_axis(g.sThumbRY as f32, dz),
            ),
            lb: (g.wButtons.0 & XINPUT_GAMEPAD_LEFT_SHOULDER.0) != 0,
            lt: g.bLeftTrigger as f32 / 255.0,
        })
    }
}

impl FreeDfApp {
    /// 매 프레임 호출 — XInput 컨트롤러를 폴링하고 매핑을 적용합니다.
    ///
    /// - 오른쪽 스틱: 상하좌우 스크롤 (전체 페이지가 보이면 세로 = 페이지 전환,
    ///   아니면 휠과 같은 `scroll_vel`로 부드럽게 팬).
    /// - LB = CTRL: 이 프레임의 egui 수정자에 ctrl을 주입하고, 스틱 상하를
    ///   줌 스텝으로 바꿉니다 (Ctrl+휠과 동일).
    /// - LT = Ctrl+Z: 트리거를 깊게 당기는 에지에서 한 번 undo.
    pub(crate) fn poll_gamepad(&mut self, ctx: &egui::Context) {
        let Some(gp) = poll_gamepad() else {
            return;
        };
        // LB = CTRL — 프레임 앞부분에서 주입해 이후 모든 입력 처리에 반영.
        if gp.lb {
            ctx.input_mut(|i| i.modifiers.ctrl = true);
        }

        let canvas = self.last_canvas;
        let page_h_px = self.page_size_pts[1] * self.view.zoom;
        let page_w_px = self.page_size_pts[0] * self.view.zoom;

        if gp.lb {
            // CTRL + 스틱 상하 = 줌 스텝 (휠 노치처럼 히스테리시스 — 재래스터
            // 비용이 크므로 한 번에 한 스텝, 복귀해야 다음 스텝 허용).
            let push = gp.stick.y;
            if self.gamepad_zoom_armed {
                if push.abs() < 0.3 {
                    self.gamepad_zoom_armed = false;
                }
            } else if push > 0.6 {
                self.zoom_by(ZOOM_STEP);
                self.gamepad_zoom_armed = true;
            } else if push < -0.6 {
                self.zoom_by(1.0 / ZOOM_STEP);
                self.gamepad_zoom_armed = true;
            }
            // Ctrl+휠처럼 LB 동안 스크롤은 억제.
            self.scroll_vel = Vec2::ZERO;
        } else {
            self.gamepad_zoom_armed = false;
            // 스틱 위 = 스크롤 위(다음 페이지), 오른쪽 = 오른쪽 스크롤.
            let stick = Vec2::new(gp.stick.x, -gp.stick.y);
            if stick.length_sq() > 0.02 {
                if page_h_px <= canvas[1] && stick.y.abs() > stick.x.abs() {
                    // 페이지 높이가 전부 보이면 세로 스크롤 = 페이지 전환
                    // (마우스 휠과 동일한 자연 스크롤 방향).
                    if stick.y > 0.0 {
                        self.next_page();
                    } else {
                        self.prev_page();
                    }
                    self.scroll_vel = Vec2::ZERO;
                } else {
                    // 아날로그 스틱 → 휠과 같은 scroll_vel 누적 (기존 이징이
                    // 부드럽게 팬으로 전환). 스틱 끝 = 초당 720pt × 줌 배율.
                    let dt = ctx.input(|i| i.stable_dt).max(1e-4);
                    let speed = 720.0 * self.view.zoom.max(1.0) * dt;
                    self.scroll_vel += stick * speed;
                    let _ = page_w_px; // 가로 스크롤 가능 여부는 팬 단계에서 판정.
                }
                ctx.request_repaint();
            }
        }

        // LT = Ctrl+Z — 깊게 당기는 에지에서 한 번.
        let lt_pressed = gp.lt > 0.75;
        if lt_pressed && !self.gamepad_lt_held {
            self.undo();
        }
        self.gamepad_lt_held = lt_pressed;
    }
}
