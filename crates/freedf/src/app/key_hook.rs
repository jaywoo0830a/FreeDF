//! 매크로 출력 파이프라인 — egui 키 이벤트(입력) + enigo 주입(출력).
//!
//! 배경: 전역 LL 훅(직접 구현·rdev 모두)은 **화상 키보드(TabTip 등)의
//! 문자키를 받지 못합니다** — 그 입력은 훅 체인을 우회해 창에 직접 전달됩니다
//! (실측: 화살표만 훅에 도달했고, 문자키는 egui 이벤트로만 도달 — a/s 탭
//! 키가 동작한 것이 증거). 따라서 입력은 **egui 이벤트**를 쓰고, 출력만
//! enigo로 보냅니다 (화상·물리 키보드 모두 동작).
//!
//! - 페이지 z/x → 앱 내부 next/prev_page 직접 호출 (주입 불필요)
//! - 데스크탑 q/w → enigo로 Win↓→Ctrl↓→←/→→Ctrl↑→Win↑ 주입
//! - 탭 a/s → 앱 내부 next_tab/prev_tab (기존과 동일)
//!
//! **"Activate on focus"를 끈 데스크탑 키**는 전역 리스너(rdev)가
//! FreeDF가 포커스가 아닐 때에도 감지해 주입합니다 (물리 키보드 한정 —
//! 화상 키보드는 OS 수준 이벤트가 없어 전역 감지가 불가합니다).
//! 게이트는 `handle_shortcuts`가 담당합니다 (타이핑 중/수정키 조합 무시).
//! 이 모듈에는 디버그 로그(패널 표시)와 enigo 전송만 남습니다.

use std::collections::VecDeque;
use std::time::Instant;

/// 매크로 진단 로그 (최근 200줄, [경과초] 접두) — Macro 설정 창의
/// 디버그 패널이 표시합니다.
static HOOK_LOG: std::sync::Mutex<VecDeque<String>> = std::sync::Mutex::new(VecDeque::new());
const HOOK_LOG_MAX: usize = 200;

/// 데스크탑 매크로를 쓸 수 있는지 (enigo는 Windows 전용).
pub(crate) fn pipeline_enabled() -> bool {
    cfg!(target_os = "windows")
}

pub(crate) fn hook_log(msg: impl std::fmt::Display) {
    static T0: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    let t = T0.get_or_init(Instant::now).elapsed().as_secs_f64();
    if let Ok(mut q) = HOOK_LOG.lock() {
        while q.len() >= HOOK_LOG_MAX {
            q.pop_front();
        }
        q.push_back(format!("[{t:>8.3}s] {msg}"));
    }
}

pub(crate) fn hook_log_snapshot() -> Vec<String> {
    HOOK_LOG
        .lock()
        .map(|q| q.iter().cloned().collect())
        .unwrap_or_default()
}

pub(crate) fn hook_log_clear() {
    if let Ok(mut q) = HOOK_LOG.lock() {
        q.clear();
    }
}

/// 전역 데스크탑 리스너 설정 (Macro 설정 창이 갱신).
/// (필드는 Windows 전역 리스너만 읽습니다.)
#[derive(Debug, Clone, Default)]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) struct DesktopCfg {
    /// 이전/다음 데스크탑 트리거 키 (None = 비활성).
    pub prev: Option<crate::settings::MacroKey>,
    pub next: Option<crate::settings::MacroKey>,
    /// true = FreeDF 포커스일 때만 (egui 경로).
    /// false = 비포커스에서도 동작 (전역 리스너).
    pub focus_only: bool,
}

static DESKTOP_CFG: std::sync::OnceLock<std::sync::RwLock<DesktopCfg>> =
    std::sync::OnceLock::new();

/// 전역 리스너에 데스크탑 설정을 반영합니다.
pub(crate) fn update_desktop_cfg(cfg: DesktopCfg) {
    if let Some(lock) = DESKTOP_CFG.get() {
        if let Ok(mut g) = lock.write() {
            *g = cfg;
        }
    } else {
        let _ = DESKTOP_CFG.set(std::sync::RwLock::new(cfg));
    }
}

/// 전역 데스크탑 리스너 스레드 (Windows 전용 — 프로세스 수명 동안 유지).
pub(crate) fn spawn_global_listener() -> Option<std::thread::JoinHandle<()>> {
    #[cfg(target_os = "windows")]
    {
        hook_log("global desktop listener starting (rdev)");
        Some(std::thread::spawn(global::listener_thread))
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = ();
        None
    }
}

/// Win↓ → Ctrl↓ → ←/→ 탭 → Ctrl↑ → Win↑ — 검증된 순서로 enigo 주입.
/// egui 단축키 처리(UI 스레드)에서 직접 호출합니다 — 사용자가 검증한
/// 예제와 같은 컨텍스트(메인 스레드)입니다.
#[cfg(target_os = "windows")]
pub(crate) fn send_desktop(prev: bool) {
    let mut enigo = match enigo::Enigo::new(&enigo::Settings::default()) {
        Ok(e) => e,
        Err(e) => {
            hook_log(format!("enigo init FAILED: {e:?}"));
            return;
        }
    };
    let out = send_desktop_seq(prev, &mut enigo);
    hook_log(format!("desktop prev={prev} [{out}]"));
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn send_desktop(prev: bool) {
    hook_log(format!("desktop prev={prev} — disabled (Windows only)"));
}

/// 검증된 순서의 enigo 전송 (공용 — UI 경로/전역 경로 모두 사용).
#[cfg(target_os = "windows")]
fn send_desktop_seq(prev: bool, enigo: &mut enigo::Enigo) -> String {
    use enigo::{Direction, Key, Keyboard};
    let arrow = if prev { Key::LeftArrow } else { Key::RightArrow };
    let steps: [(&str, Key, Direction); 5] = [
        ("win↓", Key::Meta, Direction::Press),
        ("ctrl↓", Key::Control, Direction::Press),
        ("arrow", arrow, Direction::Click),
        ("ctrl↑", Key::Control, Direction::Release),
        ("win↑", Key::Meta, Direction::Release),
    ];
    let mut out = String::new();
    for (label, k, dir) in steps {
        match enigo.key(k, dir) {
            Ok(()) => out.push_str(label),
            Err(e) => out.push_str(&format!("{label}!({e:?})")),
        }
        out.push(' ');
    }
    out
}

// ---------- 전역 리스너 ("Activate on focus" 꺼짐 전용) ----------
// FreeDF가 포커스가 아닐 때에도 데스크탑 키를 감지합니다. 포커스일 때는
// egui 경로가 처리하므로 여기서는 건너뜁니다 (이중 주입 방지).

#[cfg(target_os = "windows")]
mod global {
    use super::{hook_log, send_desktop_seq, DESKTOP_CFG};

    /// 눌려 있는 수정키 집합 (rdev 이벤트로 직접 추적).
    static MODS_HELD: std::sync::Mutex<Vec<rdev::Key>> = std::sync::Mutex::new(Vec::new());
    /// 번역 중인 데스크탑 키 (반복 억제) — MacroKey 판별값.
    static ACTIVE: std::sync::Mutex<Vec<u8>> = std::sync::Mutex::new(Vec::new());

    fn desktop_cfg() -> super::DesktopCfg {
        DESKTOP_CFG
            .get()
            .and_then(|l| l.read().ok())
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    fn is_modifier(k: rdev::Key) -> bool {
        matches!(
            k,
            rdev::Key::ControlLeft
                | rdev::Key::ControlRight
                | rdev::Key::ShiftLeft
                | rdev::Key::ShiftRight
                | rdev::Key::MetaLeft
                | rdev::Key::MetaRight
                | rdev::Key::Alt
                | rdev::Key::AltGr
        )
    }

    /// 우리 프로세스의 창이 전경인지 — 전경이면 egui 경로가 이미 처리합니다.
    fn foreground_is_ours() -> bool {
        use windows::Win32::System::Threading::GetCurrentProcessId;
        use windows::Win32::UI::WindowsAndMessaging::{
            GetForegroundWindow, GetWindowThreadProcessId,
        };
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.0.is_null() {
                return false;
            }
            let mut pid: u32 = 0;
            let _ = GetWindowThreadProcessId(hwnd, Some(&mut pid));
            pid == GetCurrentProcessId()
        }
    }

    pub(super) fn listener_thread() {
        let mut enigo = match enigo::Enigo::new(&enigo::Settings::default()) {
            Ok(e) => e,
            Err(e) => {
                hook_log(format!("global listener enigo init FAILED: {e:?}"));
                return;
            }
        };
        hook_log("global desktop listener ready (focus-off mode)");
        let res = rdev::listen(move |event| handle_event(event, &mut enigo));
        hook_log(format!("global desktop listener stopped ({res:?})"));
    }

    fn handle_event(event: rdev::Event, enigo: &mut enigo::Enigo) {
        use rdev::EventType;
        match event.event_type {
            EventType::KeyPress(k) => {
                if is_modifier(k) {
                    if let Ok(mut m) = MODS_HELD.lock() {
                        if !m.contains(&k) {
                            m.push(k);
                        }
                    }
                }
                let Some(mk) = crate::settings::MacroKey::from_rdev(k) else {
                    return;
                };
                let cfg = desktop_cfg();
                let matched = cfg
                    .prev
                    .filter(|m| *m == mk)
                    .map(|_| true)
                    .or_else(|| cfg.next.filter(|m| *m == mk).map(|_| false));
                let Some(prev) = matched else {
                    return;
                };
                let key = mk as u8;
                let repeat = ACTIVE.lock().map(|g| g.contains(&key)).unwrap_or(false);
                if let Ok(mut g) = ACTIVE.lock() {
                    if !g.contains(&key) {
                        g.push(key);
                    }
                }
                let mods = MODS_HELD.lock().map(|m| !m.is_empty()).unwrap_or(false);
                let fg = foreground_is_ours();
                // focus_only면 전역 경로 비활성. 포커스면 egui 경로가 처리 —
                // 여기서는 비포커스에서만 주입합니다 (이중 주입 방지).
                if cfg.focus_only || fg || repeat || mods {
                    return;
                }
                let out = send_desktop_seq(prev, enigo);
                hook_log(format!("global desktop prev={prev} [{out}]"));
            }
            EventType::KeyRelease(k) => {
                if is_modifier(k) {
                    if let Ok(mut m) = MODS_HELD.lock() {
                        m.retain(|x| *x != k);
                    }
                }
                if let Some(mk) = crate::settings::MacroKey::from_rdev(k) {
                    let key = mk as u8;
                    if let Ok(mut g) = ACTIVE.lock() {
                        g.retain(|x| *x != key);
                    }
                }
            }
            _ => {}
        }
    }
}
