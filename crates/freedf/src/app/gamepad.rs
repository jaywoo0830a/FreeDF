//! 게임패드(gilrs / Windows WGI 백엔드) — 프로세스가 살아있는 동안 매 프레임 폴링.
//!
//! 왼쪽 스틱 = 상하좌우 스크롤, LB = CTRL(스틱 상하 → 줌),
//! LT = Ctrl+Z(되돌리기), D패드 = ←/→/PgUp/PgDn. gilrs의 WGI(Windows.Gaming.Input) 백엔드는
//! XInput이 못 보는 컨트롤러(DualSense·8BitDo 등 WGI 지원 패드)까지
//! 인식하며, 컨트롤러 연결/해제도 자동으로 추적합니다.
//! 비-Windows 빌드에서는 안전한 no-op입니다.
//!
//! 진단 도구: 설정 창([`FreeDfApp::gamepad_settings_ui`])과
//! 디버그 패널([`FreeDfApp::gamepad_debug_ui`]) — 원시 축/버튼 값과
//! 이벤트 로그를 보여줍니다.

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

use super::*;
use crate::ui::form;

/// 이번 프레임의 게임패드 상태 — 왼쪽 스틱(x: 오른쪽=+, y: 위=+), LB, LT, D패드.
#[derive(Clone, Copy)]
pub(crate) struct Gamepad {
    pub stick: Vec2,
    pub lb: bool,
    pub lt: f32,
    pub d_up: bool,
    pub d_down: bool,
    pub d_left: bool,
    pub d_right: bool,
}

/// 게임패드 런타임 설정 (설정 창에서 편집 — 아직 세션에 저장하지 않습니다).
#[derive(Clone, Copy, Debug)]
pub(crate) struct GamepadCfg {
    /// 게임패드 입력 사용 여부.
    pub enabled: bool,
    /// 스틱 끝 스크롤 속도 (pt/s, 줌 1배 기준).
    pub speed: f32,
    /// 스틱 상하 반전.
    pub invert_y: bool,
}

impl Default for GamepadCfg {
    fn default() -> Self {
        Self {
            enabled: true,
            speed: 720.0,
            invert_y: false,
        }
    }
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

// ---------- 디버그 로그 (링 버퍼) ----------

const GAMEPAD_LOG_CAP: usize = 200;
static GAMEPAD_LOG: OnceLock<Mutex<VecDeque<String>>> = OnceLock::new();
static GAMEPAD_T0: OnceLock<std::time::Instant> = OnceLock::new();

fn gamepad_log_push(msg: impl Into<String>) {
    let t0 = *GAMEPAD_T0.get_or_init(std::time::Instant::now);
    let secs = t0.elapsed().as_secs_f32();
    let buf = GAMEPAD_LOG.get_or_init(|| Mutex::new(VecDeque::new()));
    if let Ok(mut b) = buf.lock() {
        b.push_back(format!("[{secs:7.2}] {}", msg.into()));
        while b.len() > GAMEPAD_LOG_CAP {
            b.pop_front();
        }
    }
}

pub(crate) fn gamepad_log_snapshot() -> Vec<String> {
    GAMEPAD_LOG
        .get_or_init(|| Mutex::new(VecDeque::new()))
        .lock()
        .map(|b| b.iter().cloned().collect())
        .unwrap_or_default()
}

pub(crate) fn gamepad_log_clear() {
    if let Ok(mut b) = GAMEPAD_LOG.get_or_init(|| Mutex::new(VecDeque::new())).lock() {
        b.clear();
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
        // gilrs 내부 스레드가 상태를 갱신 — 이벤트 큐는 소비하면서
        // 진단에 유용한 것만 로그에 남깁니다.
        while let Some(ev) = gilrs.next_event() {
            match ev.event {
                gilrs::EventType::Connected => {
                    gamepad_log_push(format!("패드 {} 연결됨", ev.id));
                }
                gilrs::EventType::Disconnected => {
                    gamepad_log_push(format!("패드 {} 연결 해제됨", ev.id));
                }
                gilrs::EventType::ButtonChanged(btn, value, _) if value > 0.5 => {
                    gamepad_log_push(format!("버튼 {btn:?} = {value:.2}"));
                }
                _ => {}
            }
        }
        let (_, pad) = gilrs.gamepads().next()?;
        Some(Gamepad {
            stick: egui::vec2(
                stick_axis_x(pad.value(Axis::LeftStickX)),
                stick_axis_y(pad.value(Axis::LeftStickY)),
            ),
            lb: pad.is_pressed(Button::LeftTrigger2),
            lt: pad.button_data(Button::LeftTrigger).map(|d| d.value()).unwrap_or(0.0),
            d_up: pad.is_pressed(Button::DPadUp),
            d_down: pad.is_pressed(Button::DPadDown),
            d_left: pad.is_pressed(Button::DPadLeft),
            d_right: pad.is_pressed(Button::DPadRight),
        })
    }

    #[cfg(not(target_os = "windows"))]
    fn gamepad_state(&self) -> Option<Gamepad> {
        None
    }
}

impl FreeDfApp {
    /// 매 프레임 호출 — 게임패드 상태를 읽고 매핑을 적용합니다.
    ///
    /// - 왼쪽 스틱: 상하좌우 스크롤 (전체 페이지가 보이면 세로 = 페이지 전환,
    ///   아니면 휠과 같은 `scroll_vel`로 부드럽게 팬).
    /// - D패드: ←/→/PgUp/PgDn 키 이벤트로 주입.
    /// - LB = CTRL: 이 프레임의 egui 수정자에 ctrl을 주입하고, 스틱 상하를
    ///   줌 스텝으로 바꿉니다 (Ctrl+휠과 동일).
    /// - LT = Ctrl+Z: 트리거를 깊게 당기는 에지에서 한 번 undo.
    pub(crate) fn poll_gamepad(&mut self, ctx: &egui::Context) {
        if !self.gamepad_cfg.enabled {
            self.gamepad_last = None;
            return;
        }
        let gp = self.gamepad_state();
        self.gamepad_last = gp;
        let Some(gp) = gp else {
            return;
        };
        // 처음 인식되면 상태바로 알립니다 (연결 확인용).
        if !self.gamepad_notified {
            self.gamepad_notified = true;
            gamepad_log_push("게임패드 인식됨");
            self.status = Some(
                "Gamepad connected — L-stick = scroll, LB = CTRL (stick = zoom), LT = Ctrl+Z"
                    .to_string(),
            );
        }

        // D-pad = 화살표/PgUp·PgDn 키 — 에지에서 egui 키 이벤트로 주입해
        // 기존 키보드 처리(페이지 전환/텍스트 커서 등)가 그대로 동작합니다.
        let dpad_keys = [
            (gp.d_up, egui::Key::PageUp, "D-pad up — PageUp"),
            (gp.d_down, egui::Key::PageDown, "D-pad down — PageDown"),
            (gp.d_left, egui::Key::ArrowLeft, "D-pad left — ArrowLeft"),
            (gp.d_right, egui::Key::ArrowRight, "D-pad right — ArrowRight"),
        ];
        for (i, (down, key, msg)) in dpad_keys.iter().enumerate() {
            let was = self.gamepad_dpad_prev[i];
            if *down != was {
                ctx.input_mut(|inp| {
                    inp.events.push(egui::Event::Key {
                        key: *key,
                        physical_key: None,
                        pressed: *down,
                        repeat: false,
                        modifiers: egui::Modifiers::default(),
                    });
                });
                if *down {
                    gamepad_log_push(*msg);
                }
            }
            self.gamepad_dpad_prev[i] = *down;
        }

        // LB = CTRL — 프레임 앞부분에서 주입해 이후 모든 입력 처리에 반영.
        if gp.lb {
            ctx.input_mut(|i| i.modifiers.ctrl = true);
        }

        let canvas = self.last_canvas;
        let page_h_px = self.page_size_pts[1] * self.view.zoom;
        let page_w_px = self.page_size_pts[0] * self.view.zoom;

        // 스틱 Y: 위=+ 규약. 설정의 반전 토글 적용 (왼쪽 스틱 = 스크롤).
        let gy = if self.gamepad_cfg.invert_y {
            -gp.stick.y
        } else {
            gp.stick.y
        };

        if gp.lb {
            // CTRL + 스틱 상하 = 줌 스텝 (휠 노치처럼 히스테리시스 — 재래스터
            // 비용이 크므로 한 번에 한 스텝, 복귀해야 다음 스텝 허용).
            let push = gy;
            if self.gamepad_zoom_armed {
                if push.abs() < 0.3 {
                    self.gamepad_zoom_armed = false;
                }
            } else if push > 0.6 {
                self.zoom_by(ZOOM_STEP);
                self.gamepad_zoom_armed = true;
                self.gamepad_zooms += 1;
                gamepad_log_push("LB+스틱 위 — 확대 +5%");
            } else if push < -0.6 {
                self.zoom_by(1.0 / ZOOM_STEP);
                self.gamepad_zoom_armed = true;
                self.gamepad_zooms += 1;
                gamepad_log_push("LB+스틱 아래 — 축소 -5%");
            }
            // Ctrl+휠처럼 LB 동안 스크롤은 억제.
            self.scroll_vel = Vec2::ZERO;
        } else {
            self.gamepad_zoom_armed = false;
            // 스틱 아래 = 스크롤 아래(이전 페이지/이전 내용), 오른쪽 = 오른쪽.
            let stick = Vec2::new(gp.stick.x, -gy);
            if stick.length_sq() > 0.02 {
                if page_h_px <= canvas[1] && stick.y.abs() > stick.x.abs() {
                    // 페이지 높이가 전부 보이면 세로 스크롤 = 페이지 전환
                    // (마우스 휠과 같은 자연 방향: 아래 = 이전 페이지).
                    if stick.y > 0.0 {
                        self.prev_page();
                        self.gamepad_flips += 1;
                        gamepad_log_push("스틱 아래 — 이전 페이지");
                    } else {
                        self.next_page();
                        self.gamepad_flips += 1;
                        gamepad_log_push("스틱 위 — 다음 페이지");
                    }
                    self.scroll_vel = Vec2::ZERO;
                } else {
                    // 아날로그 스틱 → 휠과 같은 scroll_vel 누적 (기존 이징이
                    // 부드럽게 팬으로 전환). 스틱 끝 = 설정 속도 × 줌 배율.
                    let dt = ctx.input(|i| i.stable_dt).max(1e-4);
                    let speed = self.gamepad_cfg.speed * self.view.zoom.max(1.0) * dt;
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
            self.gamepad_undos += 1;
            gamepad_log_push("LT — 되돌리기 (Ctrl+Z)");
        }
        self.gamepad_lt_held = lt_pressed;
    }

    /// 게임패드 설정 창 내용 (수직 폼 — 한국어 라벨).
    pub(crate) fn gamepad_settings_ui(&mut self, ui: &mut egui::Ui) {
        form::help(
            ui,
            "컨트롤러 입력은 Windows가 인식하는 게임패드면 자동으로 동작합니다.\n\
             왼쪽 스틱 = 스크롤 · LB = CTRL(스틱=줌) · LT = Ctrl+Z ·\n\
             D패드 = ←/→/PgUp/PgDn.",
        );
        form::check(
            ui,
            &mut self.gamepad_cfg.enabled,
            "게임패드 입력 사용",
            "끄면 컨트롤러 입력을 완전히 무시합니다.",
        );
        form::number(&mut self.gamepad_cfg.speed)
            .range(200.0..=2400.0)
            .speed(40.0)
            .suffix(" pt/s")
            .label("스크롤 속도")
            .help("스틱 끝까지 밀었을 때의 속도 (줌 1배 기준)")
            .show(ui);
        form::check(
            ui,
            &mut self.gamepad_cfg.invert_y,
            "스틱 상하 반전",
            "켜면 스틱 방향과 스크롤 방향이 반대가 됩니다.",
        );
        ui.add_space(8.0);
        ui.separator();
        let mut debug = self.gamepad_debug_open;
        if form::check(
            ui,
            &mut debug,
            "디버그 패널 표시",
            "연결 상태·원시 스틱/버튼 값·이벤트 로그를 보여줍니다.",
        )
        .changed()
        {
            self.gamepad_debug_open = debug;
        }
    }

    /// 게임패드 디버그 패널 — 원시 값 + 액션 카운터 + 이벤트 로그.
    pub(crate) fn gamepad_debug_ui(&mut self, ui: &mut egui::Ui) {
        egui::Window::new("게임패드 디버그")
            .default_width(360.0)
            .collapsible(false)
            .show(ui.ctx(), |ui| {
                crate::ui::dialog::pad(ui, false, |ui| {
                    match self.gamepad_last {
                        Some(g) => {
                            let green = crate::theme::nord::semantic::COLOR_SUCCESS;
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("● 연결됨").color(green));
                                ui.label(
                                    egui::RichText::new(format!("속도 {} pt/s", self.gamepad_cfg.speed))
                                        .weak(),
                                );
                            });
                            ui.label(format!(
                                "왼쪽 스틱   X={:+.2}   Y={:+.2}",
                                g.stick.x, g.stick.y
                            ));
                            ui.label(format!(
                                "D패드   ↑={} ↓={} ←={} →={}",
                                if g.d_up { "●" } else { "-" },
                                if g.d_down { "●" } else { "-" },
                                if g.d_left { "●" } else { "-" },
                                if g.d_right { "●" } else { "-" },
                            ));
                            ui.label(format!(
                                "LB(CTRL)={}   LT(Ctrl+Z)={:.2}",
                                if g.lb { "눌림" } else { "-" },
                                g.lt
                            ));
                        }
                        None => {
                            ui.label(
                                egui::RichText::new("연결된 게임패드 없음")
                                    .weak(),
                            );
                        }
                    }
                    ui.label(
                        egui::RichText::new(format!(
                            "페이지 전환 {}회 · 줌 {}회 · 되돌리기 {}회",
                            self.gamepad_flips, self.gamepad_zooms, self.gamepad_undos
                        ))
                        .weak(),
                    );
                    ui.add_space(4.0);
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.strong("이벤트 로그");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("비우기").clicked() {
                                gamepad_log_clear();
                            }
                            if ui.button("복사").clicked() {
                                let lines = gamepad_log_snapshot();
                                ui.ctx()
                                    .copy_text(lines.join("\n"));
                            }
                        });
                    });
                    let lines = gamepad_log_snapshot();
                    if lines.is_empty() {
                        ui.label(egui::RichText::new("(이벤트 없음)").weak().small());
                    } else {
                        egui::ScrollArea::vertical()
                            .id_salt("gamepad_log_scroll")
                            .max_height(240.0)
                            .stick_to_bottom(true)
                            .show(ui, |ui| {
                                for l in &lines {
                                    ui.label(egui::RichText::new(l).monospace().size(12.0));
                                }
                            });
                    }
                });
            });
    }
}
