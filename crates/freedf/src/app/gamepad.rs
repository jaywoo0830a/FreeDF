//! 게임패드(gilrs / Windows WGI 백엔드) — 프로세스가 살아있는 동안 매 프레임 폴링.
//!
//! 오른쪽 스틱 = 상하좌우 스크롤, LB = CTRL(스틱 상하 → 줌),
//! LT = Ctrl+Z(되돌리기). gilrs의 WGI(Windows.Gaming.Input) 백엔드는
//! XInput이 못 보는 컨트롤러(DualSense·8BitDo 등 WGI 지원 패드)까지
//! 인식하며, 컨트롤러 연결/해제도 자동으로 추적합니다.
//! 비-Windows 빌드에서는 안전한 no-op입니다.

use super::*;

/// 이번 프레임의 게임패드 상태 — 오른쪽 스틱(x: 오른쪽=+, y: 위=+), LB, LT.
pub(crate) struct Gamepad {
    pub stick: Vec2,
    pub lb: bool,
    pub lt: f32,
}

/// 스틱 X 축 — 데드존을 잘라냅니다.
#[cfg(target_os = "windows")]
fn stick_axis_x(v: f32) -> f32 {
    if v.abs() < 0.06 {
        0.0
    } else {
        v
    }
}

/// 스틱 Y 축 — gilrs 규약은 아래=+(evdev ABS_Y)라 위=+로 뒤집고,
/// 데드존을 잘라냅니다.
#[cfg(target_os = "windows")]
fn stick_axis_y(v: f32) -> f32 {
    let v = -v;
    if v.abs() < 0.06 {
        0.0
    } else {
        v
    }
}

impl FreeDfApp {
    /// gilrs에서 이번 프레임 상태를 읽습니다 — Windows만 실제 구현.
    #[cfg(target_os = "windows")]
    fn gamepad_state(&mut self) -> Option<Gamepad> {
        use gilrs::{Axis, Button};
        if self.gamepad.is_none() {
            self.gamepad = gilrs::Gilrs::new().ok();
        }
        let gilrs = self.gamepad.as_mut()?;
        // gilrs 내부 스레드가 상태를 갱신 — 이벤트 큐는 소비만 합니다.
        while gilrs.next_event().is_some() {}
        let (_, pad) = gilrs.gamepads().next()?;
        Some(Gamepad {
            stick: egui::vec2(
                stick_axis_x(pad.value(Axis::RightStickX)),
                stick_axis_y(pad.value(Axis::RightStickY)),
            ),
            lb: pad.is_pressed(Button::LeftTrigger2),
            lt: pad.button_data(Button::LeftTrigger).map(|d| d.value()).unwrap_or(0.0),
        })
    }

    #[cfg(not(target_os = "windows"))]
    fn gamepad_state(&self) -> Option<Gamepad> {
        None
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
        let gp = self.gamepad_state();
        let Some(gp) = gp else {
            return;
        };
        // 처음 인식되면 상태바로 알립니다 (연결 확인용).
        if !self.gamepad_notified {
            self.gamepad_notified = true;
            self.status = Some(
                "게임패드 연결됨 — 오른쪽 스틱=스크롤, LB=CTRL(스틱=줌), LT=Ctrl+Z"
                    .to_string(),
            );
        }

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
