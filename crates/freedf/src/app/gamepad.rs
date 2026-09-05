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
    /// 평소 X축 반전 (컨트롤러마다 규약이 다름).
    pub invert_x: bool,
    /// 평소 Y축 반전.
    pub invert_y: bool,
    /// CTRL(LB) 중 X축 반전 (줌 방향).
    pub invert_x_ctrl: bool,
    /// CTRL(LB) 중 Y축 반전 (줌 방향).
    pub invert_y_ctrl: bool,
}

impl Default for GamepadCfg {
    fn default() -> Self {
        Self {
            enabled: true,
            speed: 720.0,
            invert_x: false,
            invert_y: false,
            invert_x_ctrl: false,
            invert_y_ctrl: false,
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

/// 스틱 Y 축 — gilrs 규약은 위=+ (WGI 백엔드가 -1.0 곱해 통일)라
/// 그대로 쓰고 데드존만 잘라냅니다.
#[cfg(target_os = "windows")]
fn stick_axis_y(v: f32) -> f32 {
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

/// 모든 게임패드 입력의 **공통 연타 리듬** (LB+스틱 줌 · LT undo · D패드 반복).
/// 누르고 있으면 이 간격으로 계속 발사됩니다 — 모든 입력이 같은 성격을 가집니다.
const GAMEPAD_REPEAT_MS: u64 = 250;

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
                    gamepad_log_push(format!("Gamepad {} connected", ev.id));
                }
                gilrs::EventType::Disconnected => {
                    gamepad_log_push(format!("Gamepad {} disconnected", ev.id));
                }
                gilrs::EventType::ButtonChanged(btn, value, _) if value > 0.5 => {
                    gamepad_log_push(format!("Button {btn:?} = {value:.2}"));
                }
                _ => {}
            }
        }
        let (_, pad) = gilrs.gamepads().next()?;
        // gilrs 이름 주의: `LeftTrigger` = BTN_TL = **범퍼(LB)**,
        // `LeftTrigger2` = BTN_TL2 = **아날로그 트리거(LT)** — Xbox 규약과 반대.
        Some(Gamepad {
            stick: egui::vec2(
                stick_axis_x(pad.value(Axis::LeftStickX)),
                stick_axis_y(pad.value(Axis::LeftStickY)),
            ),
            lb: pad.is_pressed(Button::LeftTrigger),
            lt: pad.button_data(Button::LeftTrigger2).map(|d| d.value()).unwrap_or(0.0),
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
            gamepad_log_push("Gamepad detected");
            self.status = Some(
                "Gamepad connected — L-stick = scroll, LB = CTRL (stick = zoom), LT = Ctrl+Z"
                    .to_string(),
            );
        }

        // D-pad = 화살표/PgUp·PgDn 키 — 누르는 순간 주입하고, 누르고 있으면
        // 다른 입력과 같은 100ms 리듬으로 연타(반복). 떼면 release 주입.
        let now = now_ms();
        let dpad_keys = [
            (gp.d_up, egui::Key::PageUp, "D-pad up — PageUp"),
            (gp.d_down, egui::Key::PageDown, "D-pad down — PageDown"),
            (gp.d_left, egui::Key::ArrowLeft, "D-pad left — ArrowLeft"),
            (gp.d_right, egui::Key::ArrowRight, "D-pad right — ArrowRight"),
        ];
        for (i, (down, key, msg)) in dpad_keys.iter().enumerate() {
            let was = self.gamepad_dpad_prev[i];
            if *down {
                let due = now.saturating_sub(self.gamepad_dpad_last_ms[i]) >= GAMEPAD_REPEAT_MS;
                if !was || due {
                    ctx.input_mut(|inp| {
                        inp.events.push(egui::Event::Key {
                            key: *key,
                            physical_key: None,
                            pressed: true,
                            repeat: was,
                            modifiers: egui::Modifiers::default(),
                        });
                    });
                    self.gamepad_dpad_last_ms[i] = now;
                    if !was {
                        gamepad_log_push(*msg);
                    }
                }
            } else if was {
                ctx.input_mut(|inp| {
                    inp.events.push(egui::Event::Key {
                        key: *key,
                        physical_key: None,
                        pressed: false,
                        repeat: false,
                        modifiers: egui::Modifiers::default(),
                    });
                });
                self.gamepad_dpad_last_ms[i] = 0;
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

        // 스틱 축: 반전 토글 — 평소(X/Y)와 CTRL(LB) 중(X/Y)을 따로 적용.
        // 컨트롤러마다 축 규약이 달라 설정 창에서 뒤집을 수 있습니다.
        let cfg = self.gamepad_cfg;
        let (sx, sy) = if gp.lb {
            let x = if cfg.invert_x_ctrl { -gp.stick.x } else { gp.stick.x };
            let y = if cfg.invert_y_ctrl { -gp.stick.y } else { gp.stick.y };
            (x, y)
        } else {
            let x = if cfg.invert_x { -gp.stick.x } else { gp.stick.x };
            let y = if cfg.invert_y { -gp.stick.y } else { gp.stick.y };
            (x, y)
        };

        if gp.lb {
            // CTRL + 스틱 = 줌 (Ctrl+휠과 동일). 어느 축이든 밀고 있으면
            // 공통 연타 리듬으로 계속 한 스텝씩 — 더 크게 움직인 축이 방향 결정.
            const ZOOM_PUSH_THRESHOLD: f32 = 0.25;
            let push = if sy.abs() > sx.abs() { sy } else { sx };
            if push.abs() > ZOOM_PUSH_THRESHOLD
                && now.saturating_sub(self.gamepad_zoom_last_ms) >= GAMEPAD_REPEAT_MS
            {
                if push > 0.0 {
                    self.zoom_by(ZOOM_STEP);
                    self.gamepad_zooms += 1;
                    gamepad_log_push("LB + stick — zoom +5%");
                } else {
                    self.zoom_by(1.0 / ZOOM_STEP);
                    self.gamepad_zooms += 1;
                    gamepad_log_push("LB + stick — zoom -5%");
                }
                self.gamepad_zoom_last_ms = now;
            }
            // Ctrl+휠처럼 LB 동안 스크롤은 억제.
            self.scroll_vel = Vec2::ZERO;
        } else {
            self.gamepad_zoom_last_ms = 0;
            // 스틱 아래 = 스크롤 아래(이전 페이지/이전 내용), 오른쪽 = 오른쪽.
            let stick = Vec2::new(sx, -sy);
            if stick.length_sq() > 0.02 {
                if page_h_px <= canvas[1] && stick.y.abs() > stick.x.abs() {
                    // 페이지 높이가 전부 보이면 세로 스크롤 = 페이지 전환
                    // (마우스 휠과 같은 자연 방향: 아래 = 이전 페이지).
                    if stick.y > 0.0 {
                        self.prev_page();
                        self.gamepad_flips += 1;
                        gamepad_log_push("Stick down — previous page");
                    } else {
                        self.next_page();
                        self.gamepad_flips += 1;
                        gamepad_log_push("Stick up — next page");
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

        // LT = Ctrl+Z — 당기는 순간 한 번, 누르고 있으면 공통 리듬으로 연타.
        let lt_pressed = gp.lt > 0.75;
        if lt_pressed
            && (!self.gamepad_lt_held
                || now.saturating_sub(self.gamepad_undo_last_ms) >= GAMEPAD_REPEAT_MS)
        {
            self.undo();
            self.gamepad_undos += 1;
            self.gamepad_undo_last_ms = now;
            gamepad_log_push("LT — undo (Ctrl+Z)");
        }
        self.gamepad_lt_held = lt_pressed;

        // 반복 리듬이 FPS와 무관하게 **확실히** 돌도록 — 게임패드 입력이
        // 살아있는 동안 계속 프레임을 요청합니다. (egui는 OS 입력 이벤트가
        // 없으면 repaint를 멈춰서, 줌/LT/D패드 반복 타이머가 멈추던 문제 수정.)
        if gp.lb
            || lt_pressed
            || gp.d_up
            || gp.d_down
            || gp.d_left
            || gp.d_right
            || gp.stick.length_sq() > 0.02
        {
            ctx.request_repaint();
        }
    }

    /// 게임패드 설정 창 내용 (수직 폼).
    pub(crate) fn gamepad_settings_ui(&mut self, ui: &mut egui::Ui) {
        form::help(
            ui,
            "Gamepad input works automatically for any controller Windows sees.\n\
             L-stick = scroll · LB = CTRL (stick = zoom) · LT = Ctrl+Z ·\n\
             D-pad = arrows / PgUp / PgDn.",
        );
        form::check(
            ui,
            &mut self.gamepad_cfg.enabled,
            "Enable gamepad input",
            "Turn off to ignore controller input entirely.",
        );
        form::number(&mut self.gamepad_cfg.speed)
            .range(200.0..=2400.0)
            .speed(40.0)
            .suffix(" pt/s")
            .label("Scroll speed")
            .help("Speed at full stick deflection (at 1× zoom)")
            .show(ui);
        form::check(
            ui,
            &mut self.gamepad_cfg.invert_x,
            "Invert stick X",
            "Flip the horizontal scroll direction (controllers differ).",
        );
        form::check(
            ui,
            &mut self.gamepad_cfg.invert_y,
            "Invert stick Y",
            "Flip the vertical scroll direction (controllers differ).",
        );
        form::check(
            ui,
            &mut self.gamepad_cfg.invert_x_ctrl,
            "Invert stick X with CTRL (LB)",
            "Flip the horizontal zoom direction while LB is held.",
        );
        form::check(
            ui,
            &mut self.gamepad_cfg.invert_y_ctrl,
            "Invert stick Y with CTRL (LB)",
            "Flip the vertical zoom direction while LB is held.",
        );
        ui.add_space(8.0);
        ui.separator();
        let mut debug = self.gamepad_debug_open;
        if form::check(
            ui,
            &mut debug,
            "Show debug panel",
            "Connection status, raw stick/button values and the event log.",
        )
        .changed()
        {
            self.gamepad_debug_open = debug;
        }
    }

    /// 게임패드 디버그 패널 — 원시 값 + 액션 카운터 + 이벤트 로그.
    pub(crate) fn gamepad_debug_ui(&mut self, ui: &mut egui::Ui) {
        egui::Window::new("Gamepad debug")
            .default_width(360.0)
            .collapsible(false)
            .show(ui.ctx(), |ui| {
                crate::ui::dialog::pad(ui, false, |ui| {
                    match self.gamepad_last {
                        Some(g) => {
                            let green = crate::theme::nord::semantic::COLOR_SUCCESS;
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("● Connected").color(green));
                                ui.label(
                                    egui::RichText::new(format!("speed {} pt/s", self.gamepad_cfg.speed))
                                        .weak(),
                                );
                            });
                            ui.label(format!(
                                "Left stick   X={:+.2}   Y={:+.2}",
                                g.stick.x, g.stick.y
                            ));
                            ui.label(format!(
                                "D-pad   up={} down={} left={} right={}",
                                if g.d_up { "●" } else { "-" },
                                if g.d_down { "●" } else { "-" },
                                if g.d_left { "●" } else { "-" },
                                if g.d_right { "●" } else { "-" },
                            ));
                            ui.label(format!(
                                "LB(CTRL)={}   LT(Ctrl+Z)={:.2}",
                                if g.lb { "held" } else { "-" },
                                g.lt
                            ));
                        }
                        None => {
                            ui.label(
                                egui::RichText::new("No gamepad connected")
                                    .weak(),
                            );
                        }
                    }
                    ui.label(
                        egui::RichText::new(format!(
                            "Page flips {} · zooms {} · undos {}",
                            self.gamepad_flips, self.gamepad_zooms, self.gamepad_undos
                        ))
                        .weak(),
                    );
                    ui.add_space(4.0);
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.strong("Event log");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Clear").clicked() {
                                gamepad_log_clear();
                            }
                            if ui.button("Copy").clicked() {
                                let lines = gamepad_log_snapshot();
                                ui.ctx()
                                    .copy_text(lines.join("\n"));
                            }
                        });
                    });
                    let lines = gamepad_log_snapshot();
                    if lines.is_empty() {
                        ui.label(egui::RichText::new("(no events)").weak().small());
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
