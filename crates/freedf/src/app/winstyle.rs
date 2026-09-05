//! OS 네이티브 창 스타일 (Windows) — DWM 속성으로 현대적 외형 적용.
//!
//! egui/eframe API가 아니라 win32 DWM를 직접 조작합니다:
//! - **다크 타이틀바** (`DWMWA_USE_IMMERSIVE_DARK_MODE`)
//! - **Windows 11 라운드 코너** (`DWMWA_WINDOW_CORNER_PREFERENCE`)
//! - **Mica 계열 제목줄 배경** (`DWMWA_SYSTEMBACKDROP_TYPE`) — Win11 전용,
//!   불투명 클라이언트 영역에서는 제목줄에만 나타납니다.
//!
//! 각 속성은 OS 버전에 따라 지원되지 않을 수 있으며, 실패는 무시합니다
//! (Windows 10에서는 코너/배경 속성이 조용히 무시됨).

/// hwnd에 현대적 창 속성을 적용합니다 (Windows 전용 — 다른 OS는 no-op).
#[cfg(target_os = "windows")]
pub(crate) fn apply(hwnd: *mut std::ffi::c_void) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE,
        DWMWA_WINDOW_CORNER_PREFERENCE, DWMSBT_MAINWINDOW, DWMWCP_ROUND,
    };
    let hwnd = HWND(hwnd);
    unsafe {
        // 다크 타이틀바 (Win10 2004+ / Win11).
        let dark: i32 = 1;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark as *const i32 as *const _,
            std::mem::size_of::<i32>() as u32,
        );
        // 라운드 코너 (Win11).
        let corner: i32 = DWMWCP_ROUND.0;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner as *const i32 as *const _,
            std::mem::size_of::<i32>() as u32,
        );
        // Mica 계열 배경 (Win11 — 제목줄에만 표시).
        let backdrop: i32 = DWMSBT_MAINWINDOW.0;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            &backdrop as *const i32 as *const _,
            std::mem::size_of::<i32>() as u32,
        );
    }
}

#[cfg(not(target_os = "windows"))]
#[allow(dead_code)]
pub(crate) fn apply(_hwnd: *mut std::ffi::c_void) {}
