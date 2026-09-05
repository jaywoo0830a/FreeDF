//! Windows 전용 — 물리 키보드 LL 훅 + SendInput 매크로.
//!
//! 배경: egui의 일반 키 이벤트는 IME 조합(한글 입력 등)이나 포커스 상태에
//! 영향을 받을 수 있습니다. WH_KEYBOARD_LL 훅은 IME 처리 **이전**에 키를
//! 받으므로, FreeDF 창이 전경(포그라운드)이고 텍스트 입력 중이 아닐 때
//! 매핑된 키를 번역해 SendInput으로 재주입합니다:
//! - 페이지 키 → PgUp/PgDn (앱의 기존 단축키 파이프라인이 처리)
//! - 데스크탑 키 → Ctrl+Win+←/→ (Windows 가상 데스크탑 전환)
//!
//! 매핑은 [`HookConfig`]로 UI(Macro 설정 창)가 갱신합니다.

use std::collections::VecDeque;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

/// 훅/매크로 진단 로그 (최근 200줄, [경과초] 접두) — Macro 설정 창의
/// 디버그 패널이 표시합니다. Windows 빌드에서만 실제 훅 이벤트가 기록됩니다.
static HOOK_LOG: std::sync::Mutex<VecDeque<String>> = std::sync::Mutex::new(VecDeque::new());
const HOOK_LOG_MAX: usize = 200;

/// 훅 스레드가 설치에 성공했는지 (디버그 패널 상태줄).
static HOOK_ALIVE: AtomicBool = AtomicBool::new(false);

pub(crate) fn hook_alive() -> bool {
    HOOK_ALIVE.load(std::sync::atomic::Ordering::Relaxed)
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
/// 프레임 갱신합니다. true면 훅은 키를 그대로 통과시킵니다 (타이핑 보호).
pub(crate) static KEY_TEXT_ACTIVE: AtomicBool = AtomicBool::new(false);

/// 훅 번역 매핑 — Macro 설정 창에서 갱신.
/// (비 Windows 빌드에서는 Windows 훅이 비활성이라 읽는 쪽이 없습니다.)
#[derive(Debug, Clone, Default)]
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) struct HookConfig {
    /// 이전/다음 페이지 트리거 키 (None = 비활성).
    pub page_prev: Option<crate::settings::MacroKey>,
    pub page_next: Option<crate::settings::MacroKey>,
    /// 가상 데스크탑 전환 트리거 키 (None = 비활성).
    pub desktop_prev: Option<crate::settings::MacroKey>,
    pub desktop_next: Option<crate::settings::MacroKey>,
}

static CONFIG: std::sync::OnceLock<std::sync::RwLock<HookConfig>> = std::sync::OnceLock::new();

/// 매핑을 훅에 반영합니다 (설정 변경 시 호출).
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

/// 훅 스레드 핸들 — FreeDfApp이 프로세스 수명 동안 유지합니다.
pub(crate) struct KeyHook {
    #[allow(dead_code)]
    _thread: Option<std::thread::JoinHandle<()>>,
}

/// 키보드 훅을 띄웁니다 (비 Windows는 no-op).
pub(crate) fn spawn() -> KeyHook {
    #[cfg(target_os = "windows")]
    {
        KeyHook {
            _thread: Some(std::thread::spawn(imp::hook_thread)),
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        hook_log("non-Windows build — hook disabled");
        KeyHook { _thread: None }
    }
}

#[cfg(target_os = "windows")]
mod imp {
    use super::{hook_log, CONFIG, HOOK_ALIVE, KEY_TEXT_ACTIVE};
    use std::sync::atomic::Ordering;

    use windows::Win32::Foundation::{
        GetLastError, HINSTANCE, LPARAM, LRESULT, WPARAM,
    };
    use windows::Win32::System::Threading::{GetCurrentProcessId, GetCurrentThreadId};
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, SendInput, INPUT, INPUT_0, KEYBDINPUT, KEYBD_EVENT_FLAGS,
        KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, VIRTUAL_KEY, VK_CONTROL,
        VK_LCONTROL, VK_LEFT, VK_LWIN, VK_MENU, VK_NEXT, VK_PRIOR, VK_RIGHT, VK_RWIN, VK_SHIFT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId,
        KBDLLHOOKSTRUCT, KBDLLHOOKSTRUCT_FLAGS, LLKHF_INJECTED, MSG, PostThreadMessageW,
        SetWindowsHookExW, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
    };

    // 번역 중인 키 집합 — 반복(repeat)은 1회만 처리.
    static ACTIVE: std::sync::Mutex<Vec<u16>> = std::sync::Mutex::new(Vec::new());

    /// 훅 스레드 ID — 데스크탑 매크로를 훅 콜백 밖(메시지 루프)에서 보내도록
    /// 자기 스레드에 요청 메시지를 게시할 때 사용.
    static HOOK_THREAD_ID: std::sync::OnceLock<u32> = std::sync::OnceLock::new();
    /// 훅 스레드 전용 데스크탑 매크로 요청 메시지 (WM_APP + 1).
    const WM_DESKTOP_MACRO: u32 = 0x8001;

    fn hook_config() -> super::HookConfig {
        CONFIG
            .get()
            .and_then(|l| l.read().ok())
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    fn send_key(vk: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS) {
        let input = INPUT {
            r#type: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };
        let inputs = [input];
        unsafe {
            let _ = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        }
    }

    /// 스캔코드 기반 키 이벤트 — Win/화살표처럼 레이아웃과 무관한 키는
    /// VK 변환보다 스캔코드가 안정적으로 인식됩니다.
    fn keybd(scan: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
        INPUT {
            r#type: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0),
                    wScan: scan,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    /// wVk 기반 키 이벤트 — 공식 SendInput 예제 스타일.
    fn keybd_vk(vk: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS) -> INPUT {
        INPUT {
            r#type: windows::Win32::UI::Input::KeyboardAndMouse::INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    /// Ctrl+Win+←/→ — Windows 가상 데스크탑 전환.
    ///
    /// 사용자가 검증한 예제와 동일한 이벤트 구조:
    /// - **Win → Ctrl → 화살표** 순서로 누름 (Ctrl 먼저가 아님)
    /// - down/up을 **별도의 SendInput 호출**로 나눠 전송
    /// - 떼는 순서는 화살표 → Ctrl → Win (누른 역순)
    /// 1차가 전부 전달되지 않으면 스캔코드 배치(동일 순서)로 재시도합니다.
    /// 반환값은 디버그 로그용 결과 요약.
    fn send_desktop(prev: bool) -> String {
        let arrow = if prev { VK_LEFT } else { VK_RIGHT };
        // ── 1차: wVk 기반 (검증된 예제 스타일) ──
        let down = [
            keybd_vk(VK_LWIN, KEYBD_EVENT_FLAGS(0)),
            keybd_vk(VK_LCONTROL, KEYBD_EVENT_FLAGS(0)),
            keybd_vk(arrow, KEYBD_EVENT_FLAGS(0)),
        ];
        let up = [
            keybd_vk(arrow, KEYEVENTF_KEYUP),
            keybd_vk(VK_LCONTROL, KEYEVENTF_KEYUP),
            keybd_vk(VK_LWIN, KEYEVENTF_KEYUP),
        ];
        let cb = std::mem::size_of::<INPUT>() as i32;
        let n = unsafe { SendInput(&down, cb) };
        let m = unsafe { SendInput(&up, cb) };
        if n as usize == down.len() && m as usize == up.len() {
            return format!("vk batch ok (down {n}/3, up {m}/3)");
        }
        // ── 2차: 스캔코드 기반 (키보드 레이아웃과 무관한 하드웨어 수준) ──
        let tap = if prev { 0x4B } else { 0x4D }; // ← / → (Set 1 스캔코드)
        let ext = KEYEVENTF_SCANCODE | KEYEVENTF_EXTENDEDKEY;
        let down_sc = [
            keybd(0x5B, ext),                  // LWin down
            keybd(0x1D, KEYEVENTF_SCANCODE),   // Ctrl down
            keybd(tap, ext),                   // ←/→ down
        ];
        let up_sc = [
            keybd(tap, ext | KEYEVENTF_KEYUP),                 // ←/→ up
            keybd(0x1D, KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP), // Ctrl up
            keybd(0x5B, ext | KEYEVENTF_KEYUP),                // LWin up
        ];
        let n2 = unsafe { SendInput(&down_sc, cb) };
        let m2 = unsafe { SendInput(&up_sc, cb) };
        format!("vk partial (down {n}/3, up {m}/3) → scan fallback (down {n2}/3, up {m2}/3)")
    }

    /// PgUp/PgDn 탭 — 앱 단축키 파이프라인이 받아 처리.
    fn send_page(prev: bool) {
        let vk = if prev { VK_PRIOR } else { VK_NEXT };
        send_key(vk, KEYBD_EVENT_FLAGS(0));
        send_key(vk, KEYEVENTF_KEYUP);
    }

    fn modifiers_held() -> bool {
        let held = |vk: VIRTUAL_KEY| unsafe { GetAsyncKeyState(vk.0 as i32) as u32 & 0x8000 != 0 };
        held(VK_CONTROL) || held(VK_SHIFT) || held(VK_MENU) || held(VK_LWIN) || held(VK_RWIN)
    }

    /// 우리 프로세스의 창이 전경인지 — 아니면 다른 앱의 키를 뺏지 않습니다.
    fn foreground_is_ours() -> bool {
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

    fn key_is_down(vk: u16) -> bool {
        ACTIVE.lock().map(|g| g.contains(&vk)).unwrap_or(false)
    }

    fn set_key_down(vk: u16, down: bool) {
        if let Ok(mut g) = ACTIVE.lock() {
            if down {
                if !g.contains(&vk) {
                    g.push(vk);
                }
            } else {
                g.retain(|k| *k != vk);
            }
        }
    }

    unsafe extern "system" fn keyboard_cb(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code < 0 {
            return CallNextHookEx(None, code, wparam, lparam);
        }
        let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let down = wparam.0 as u32 == WM_KEYDOWN || wparam.0 as u32 == WM_SYSKEYDOWN;
        let up = wparam.0 as u32 == WM_KEYUP || wparam.0 as u32 == WM_SYSKEYUP;
        if !down && !up {
            return CallNextHookEx(None, code, wparam, lparam);
        }
        let cfg = hook_config();
        let vk = info.vkCode as u16;
        // 주입된 조합 키(Ctrl/Win/화살표)가 훅을 통과하는지 디버그 로그에 기록.
        if down && (info.flags & LLKHF_INJECTED) != KBDLLHOOKSTRUCT_FLAGS(0) {
            if vk == VK_LCONTROL.0 as u16
                || vk == VK_LWIN.0 as u16
                || vk == VK_LEFT.0 as u16
                || vk == VK_RIGHT.0 as u16
            {
                hook_log(format!("injected combo key down vk={vk}"));
            }
        }
        let matched_page = cfg
            .page_prev
            .filter(|k| k.vk() == vk)
            .map(|_| true)
            .or_else(|| cfg.page_next.filter(|k| k.vk() == vk).map(|_| false));
        let matched_desktop = cfg
            .desktop_prev
            .filter(|k| k.vk() == vk)
            .map(|_| true)
            .or_else(|| cfg.desktop_next.filter(|k| k.vk() == vk).map(|_| false));
        let is_page = matched_page.is_some();
        let is_desktop = matched_desktop.is_some();
        if down && (is_page || is_desktop) {
            let repeat = key_is_down(vk);
            set_key_down(vk, true);
            let mods = modifiers_held();
            let typing = KEY_TEXT_ACTIVE.load(Ordering::Relaxed);
            let fg = foreground_is_ours();
            let allowed = !repeat && !mods && !typing && fg;
            if !allowed {
                hook_log(format!(
                    "macro key vk={vk} BLOCKED repeat={repeat} mods={mods} typing={typing} fg={fg}"
                ));
            } else if let Some(prev) = matched_page {
                send_page(prev);
                hook_log(format!("macro key vk={vk} → send_page prev={prev}"));
            } else if let Some(prev) = matched_desktop {
                // 훅 콜백 안에서는 보내지 않고, 자기 스레드의 메시지
                // 루프에 요청을 게시합니다 — 검증된 예제처럼 훅 체인
                // 밖 컨텍스트에서 SendInput이 실행되도록 합니다.
                if let Some(&tid) = HOOK_THREAD_ID.get() {
                    let ok = PostThreadMessageW(
                        tid,
                        WM_DESKTOP_MACRO,
                        WPARAM(prev as usize),
                        LPARAM(0),
                    );
                    hook_log(format!(
                        "macro key vk={vk} → desktop request prev={prev} posted={}",
                        ok.is_ok()
                    ));
                } else {
                    hook_log(format!("macro key vk={vk} → desktop request FAILED (no hook thread id)"));
                }
            }
            // 매핑된 키는 원래 동작을 삼킵니다 (반복 포함 — 의도치 않은
            // 타이핑 방지).
            return LRESULT(1);
        }
        if up && (is_page || is_desktop) {
            set_key_down(vk, false);
            return LRESULT(1);
        }
        CallNextHookEx(None, code, wparam, lparam)
    }

    pub(super) fn hook_thread() {
        unsafe {
            let _ = HOOK_THREAD_ID.set(GetCurrentThreadId());
            let hook = SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(keyboard_cb),
                Some(HINSTANCE::default()),
                0,
            );
            match hook {
                Ok(_hook) => {
                    HOOK_ALIVE.store(true, Ordering::Relaxed);
                    hook_log("hook installed (WH_KEYBOARD_LL)");
                }
                Err(e) => {
                    hook_log(format!(
                        "hook install FAILED: {e:?} (GetLastError={})",
                        GetLastError().0
                    ));
                    return;
                }
            }
            let mut msg: MSG = std::mem::zeroed();
            // LL 훅 콜백은 훅을 설치한 스레드의 메시지 루프에서 호출됩니다.
            // (스레드가 끝나면 시스템이 훅을 자동 해제 — 프로세스 수명 동안 유지)
            // 데스크탑 매크로 요청 메시지는 콜백 밖인 여기서 처리합니다.
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                if msg.message == WM_DESKTOP_MACRO {
                    let prev = msg.wParam.0 != 0;
                    let res = send_desktop(prev);
                    hook_log(format!("desktop macro prev={prev} → {res}"));
                }
            }
            HOOK_ALIVE.store(false, Ordering::Relaxed);
            hook_log("hook thread exited");
        }
    }
}
