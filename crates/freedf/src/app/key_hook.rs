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

use std::sync::atomic::AtomicBool;

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
        KeyHook { _thread: None }
    }
}

#[cfg(target_os = "windows")]
mod imp {
    use super::{CONFIG, KEY_TEXT_ACTIVE};
    use std::sync::atomic::Ordering;

    use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::Threading::GetCurrentProcessId;
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, SendInput, INPUT, INPUT_0, KEYBDINPUT, KEYBD_EVENT_FLAGS,
        KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, VIRTUAL_KEY, VK_CONTROL,
        VK_LEFT, VK_LWIN, VK_MENU, VK_NEXT, VK_PRIOR, VK_RIGHT, VK_RWIN, VK_SHIFT,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId,
        KBDLLHOOKSTRUCT, MSG, SetWindowsHookExW, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP,
        WM_SYSKEYDOWN, WM_SYSKEYUP,
    };

    // 번역 중인 키 집합 — 반복(repeat)은 1회만 처리.
    static ACTIVE: std::sync::Mutex<Vec<u16>> = std::sync::Mutex::new(Vec::new());

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
    /// 공식 문서(winuser SendInput)의 ShowDesktop 예제와 같은 방식:
    /// **wVk 기반** 이벤트를 한 번의 SendInput으로 배치 전송합니다
    /// (이벤트는 직렬로 삽입되어 다른 입력과 섞이지 않음).
    /// 실패 시 스캔코드 배치로 재시도합니다.
    fn send_desktop(prev: bool) {
        let arrow = if prev { VK_LEFT } else { VK_RIGHT };
        // ── 1차: wVk 기반 (공식 예제 스타일) ──
        let vk = [
            keybd_vk(VK_CONTROL, KEYBD_EVENT_FLAGS(0)),
            keybd_vk(VK_LWIN, KEYBD_EVENT_FLAGS(0)),
            keybd_vk(arrow, KEYBD_EVENT_FLAGS(0)),
            keybd_vk(arrow, KEYEVENTF_KEYUP),
            keybd_vk(VK_LWIN, KEYEVENTF_KEYUP),
            keybd_vk(VK_CONTROL, KEYEVENTF_KEYUP),
        ];
        let sent = unsafe { SendInput(&vk, std::mem::size_of::<INPUT>() as i32) };
        if sent as usize == vk.len() {
            return;
        }
        // ── 2차: 스캔코드 기반 (키보드 레이아웃과 무관한 하드웨어 수준) ──
        let tap = if prev { 0x4B } else { 0x4D }; // ← / → (Set 1 스캔코드)
        let ext = KEYEVENTF_SCANCODE | KEYEVENTF_EXTENDEDKEY;
        let sc = [
            keybd(0x1D, KEYEVENTF_SCANCODE),                    // Ctrl down
            keybd(0x5B, ext),                                   // LWin down
            keybd(tap, ext),                                    // ←/→ down
            keybd(tap, ext | KEYEVENTF_KEYUP),                  // ←/→ up
            keybd(0x5B, ext | KEYEVENTF_KEYUP),                 // LWin up
            keybd(0x1D, KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP),  // Ctrl up
        ];
        unsafe {
            let _ = SendInput(&sc, std::mem::size_of::<INPUT>() as i32);
        }
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
            let allowed = !repeat
                && !modifiers_held()
                && !KEY_TEXT_ACTIVE.load(Ordering::Relaxed)
                && foreground_is_ours();
            if allowed {
                if let Some(prev) = matched_page {
                    send_page(prev);
                } else if let Some(prev) = matched_desktop {
                    send_desktop(prev);
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
            let hook = SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(keyboard_cb),
                Some(HINSTANCE::default()),
                0,
            );
            let Ok(_hook) = hook else {
                return;
            };
            let mut msg: MSG = std::mem::zeroed();
            // LL 훅 콜백은 훅을 설치한 스레드의 메시지 루프에서 호출됩니다.
            // (스레드가 끝나면 시스템이 훅을 자동 해제 — 프로세스 수명 동안 유지)
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {}
        }
    }
}
