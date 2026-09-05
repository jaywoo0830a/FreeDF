//! 전역 키 입력 → 매크로 번역 파이프라인.
//!
//! - 입력: `rdev` 전역 리스너 — OS 수준 키 이벤트를 IME/포커스와 무관하게
//!   받습니다 (Windows/macOS/Linux 공통).
//! - 출력: `enigo` 주입 — 페이지 키(z/x)는 PgUp/PgDn으로, 데스크탑 키(q/w)는
//!   검증된 순서(Win↓ → Ctrl↓ → ←/→ → Ctrl↑ → Win↑)로 보냅니다.
//! - 게이트: FreeDF 전경(Windows)/수정키 조합/타이핑 중(KEY_TEXT_ACTIVE)/
//!   반복 입력이면 번역하지 않습니다.
//!
//! 탭 키(a/s)는 egui가 직접 처리합니다 (이 파이프라인에 없음).
//! 매핑은 [`HookConfig`]로 UI(Macro 설정 창)가 갱신합니다.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

/// 훅/매크로 진단 로그 (최근 200줄, [경과초] 접두) — Macro 설정 창의
/// 디버그 패널이 표시합니다.
static HOOK_LOG: std::sync::Mutex<VecDeque<String>> = std::sync::Mutex::new(VecDeque::new());
const HOOK_LOG_MAX: usize = 200;

/// 리스너 스레드가 가동 중인지 (디버그 패널 상태줄).
static HOOK_ALIVE: AtomicBool = AtomicBool::new(false);

pub(crate) fn hook_alive() -> bool {
    HOOK_ALIVE.load(Ordering::Relaxed)
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

/// FreeDF 창에서 텍스트 입력(검색창/제목 등)이 활성인지 — UI 스레드가 매
/// 프레임 갱신합니다. true면 번역하지 않습니다 (타이핑 보호).
pub(crate) static KEY_TEXT_ACTIVE: AtomicBool = AtomicBool::new(false);

/// 매크로 번역 매핑 — Macro 설정 창에서 갱신.
#[derive(Debug, Clone, Default)]
pub(crate) struct HookConfig {
    /// 이전/다음 페이지 트리거 키 (None = 비활성).
    pub page_prev: Option<crate::settings::MacroKey>,
    pub page_next: Option<crate::settings::MacroKey>,
    /// 가상 데스크탑 전환 트리거 키 (None = 비활성).
    pub desktop_prev: Option<crate::settings::MacroKey>,
    pub desktop_next: Option<crate::settings::MacroKey>,
}

static CONFIG: std::sync::OnceLock<std::sync::RwLock<HookConfig>> = std::sync::OnceLock::new();

/// 매핑을 리스너에 반영합니다 (설정 변경 시 호출).
pub(crate) fn update_config(cfg: HookConfig) {
    let name = |m: Option<crate::settings::MacroKey>| {
        m.map(|k| k.label().to_string())
            .unwrap_or_else(|| "off".into())
    };
    hook_log(format!(
        "macro config → page {}/{} · desktop {}/{}",
        name(cfg.page_prev),
        name(cfg.page_next),
        name(cfg.desktop_prev),
        name(cfg.desktop_next),
    ));
    if let Some(lock) = CONFIG.get() {
        if let Ok(mut g) = lock.write() {
            *g = cfg;
        }
    } else {
        let _ = CONFIG.set(std::sync::RwLock::new(cfg));
    }
}

/// 리스너 스레드 핸들 — FreeDfApp이 프로세스 수명 동안 유지합니다.
pub(crate) struct KeyHook {
    #[allow(dead_code)]
    _thread: Option<std::thread::JoinHandle<()>>,
}

/// 전역 리스너(rdev)를 띄우고 enigo로 번역 키를 주입합니다.
pub(crate) fn spawn() -> KeyHook {
    hook_log("macro listener thread starting (rdev → enigo)");
    KeyHook {
        _thread: Some(std::thread::spawn(listener_thread)),
    }
}

// ---------- 리스너 스레드 (rdev) + enigo 출력 ----------

/// 눌려 있는 수정키 집합 (rdev 이벤트로 직접 추적).
#[cfg(target_os = "windows")]
static MODS_HELD: std::sync::Mutex<Vec<rdev::Key>> = std::sync::Mutex::new(Vec::new());
/// 번역 중인 매핑 키 (반복 억제) — MacroKey 판별값.
#[cfg(target_os = "windows")]
static ACTIVE: std::sync::Mutex<Vec<u8>> = std::sync::Mutex::new(Vec::new());

#[cfg(target_os = "windows")]
fn hook_config() -> HookConfig {
    CONFIG
        .get()
        .and_then(|l| l.read().ok())
        .map(|g| g.clone())
        .unwrap_or_default()
}

#[cfg(target_os = "windows")]
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

/// 우리 프로세스의 창이 전경인지 — 아니면 다른 앱의 키를 번역하지 않습니다.
/// (Windows만 실제 검사 — 다른 플랫폼은 타이핑/수정키 게이트만 적용)
#[cfg(target_os = "windows")]
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

#[cfg(not(target_os = "windows"))]
#[allow(dead_code)]
fn foreground_is_ours() -> bool {
    true
}

/// PgUp/PgDn — enigo로 재주입 (egui 단축키 파이프라인이 받아 처리).
#[cfg(target_os = "windows")]
fn send_page(prev: bool, enigo: &mut enigo::Enigo) -> String {
    use enigo::{Direction, Key, Keyboard};
    let k = if prev { Key::PageUp } else { Key::PageDown };
    match enigo.key(k, Direction::Click) {
        Ok(()) => format!("page {}", if prev { "up" } else { "down" }),
        Err(e) => format!("page FAILED ({e:?})"),
    }
}

/// Win↓ → Ctrl↓ → ←/→ 탭 → Ctrl↑ → Win↑ — 검증된 순서로 enigo 주입.
#[cfg(target_os = "windows")]
fn send_desktop(prev: bool, enigo: &mut enigo::Enigo) -> String {
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

#[cfg(target_os = "windows")]
fn listener_thread() {
    let mut enigo = match enigo::Enigo::new(&enigo::Settings::default()) {
        Ok(e) => e,
        Err(e) => {
            hook_log(format!("enigo init FAILED: {e:?}"));
            return;
        }
    };
    HOOK_ALIVE.store(true, Ordering::Relaxed);
    hook_log("rdev listen started");
    let res = rdev::listen(move |event| handle_event(event, &mut enigo));
    HOOK_ALIVE.store(false, Ordering::Relaxed);
    hook_log(format!("rdev listen stopped ({res:?})"));
}

#[cfg(not(target_os = "windows"))]
fn listener_thread() {
    hook_log("non-Windows build — macro listener disabled (rdev/enigo Windows-only)");
}

#[cfg(target_os = "windows")]
fn handle_event(event: rdev::Event, enigo: &mut enigo::Enigo) {
    use rdev::EventType;
    let cfg = hook_config();
    match event.event_type {
        EventType::KeyPress(k) => {
            // 디버그: 받은 **모든** 키다운을 기록합니다 — 리스너가 이벤트를
            // 받는지, 어떤 키로 오는지 바로 알 수 있습니다.
            let name = event.name.clone().unwrap_or_default();
            hook_log(format!("key down {k:?} name={name}"));
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
            let matched_page = cfg
                .page_prev
                .filter(|m| *m == mk)
                .map(|_| true)
                .or_else(|| cfg.page_next.filter(|m| *m == mk).map(|_| false));
            let matched_desktop = cfg
                .desktop_prev
                .filter(|m| *m == mk)
                .map(|_| true)
                .or_else(|| cfg.desktop_next.filter(|m| *m == mk).map(|_| false));
            if matched_page.is_none() && matched_desktop.is_none() {
                return;
            }
            let key = mk as u8;
            let repeat = ACTIVE.lock().map(|g| g.contains(&key)).unwrap_or(false);
            if let Ok(mut g) = ACTIVE.lock() {
                if !g.contains(&key) {
                    g.push(key);
                }
            }
            let mods = MODS_HELD.lock().map(|m| !m.is_empty()).unwrap_or(false);
            let typing = KEY_TEXT_ACTIVE.load(Ordering::Relaxed);
            let fg = foreground_is_ours();
            let allowed = !repeat && !mods && !typing && fg;
            if !allowed {
                hook_log(format!(
                    "macro key {mk:?} BLOCKED repeat={repeat} mods={mods} typing={typing} fg={fg}"
                ));
            } else if let Some(prev) = matched_page {
                let res = send_page(prev, enigo);
                hook_log(format!("macro key {mk:?} → {res}"));
            } else if let Some(prev) = matched_desktop {
                let res = send_desktop(prev, enigo);
                hook_log(format!("macro key {mk:?} → desktop prev={prev} [{res}]"));
            }
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
