//! Linux evdev 펜 입력 스캐너 — egui/winit이 노출하지 않는 **펜 틸트/필압**을
//! `/dev/input/event*`에서 직접 읽습니다.
//!
//! - [`list_devices`]: 틸트/필압을 보고하는 장치 목록 (진단용).
//! - [`PenMonitor`]: 가장 적합한 장치(틸트 우선)를 열어 [`PenMonitor::poll`]로
//!   최신 틸트·필압·접촉 상태를 받습니다.
//!
//! Linux 전용입니다 (다른 OS에서는 스텁 — `open_best() == None`).
//! Windows에서는 WM_POINTER(POINTER_PEN_INFO) 훅이 필요하며, egui/winit이
//! 이를 노출하지 않아 별도 통합이 필요합니다.

/// 펜 상태 스냅샷 — 틸트(도, ±90), 필압(0..1, 장치가 보고할 때만),
/// 접촉 여부.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PenState {
    pub tilt: [f32; 2],
    pub pressure: Option<f32>,
    pub contact: bool,
}

/// 발견된 펜 장치 정보 (진단 출력용).
#[derive(Debug, Clone)]
pub struct PenDeviceInfo {
    pub path: String,
    pub name: String,
    pub has_tilt: bool,
    pub has_pressure: bool,
}

/// Windows Raw Input으로 받은 리포트 하나 — 어느 장치에서 왔는지
/// (장치 경로)와 원시 바이트를 함께 담습니다.
#[derive(Debug, Clone)]
pub struct RawReport {
    pub device: String,
    pub bytes: Vec<u8>,
}

#[cfg(target_os = "linux")]
mod linux {
    use super::{PenDeviceInfo, PenState};
    use std::os::unix::io::AsRawFd;

    // ── evdev 상수 (linux/input-event-codes.h) ──
    const EV_ABS: u16 = 0x03;
    const EV_KEY: u16 = 0x01;
    const ABS_PRESSURE: u16 = 0x18;
    const ABS_TILT_X: u16 = 0x1a;
    const ABS_TILT_Y: u16 = 0x1b;
    const BTN_TOUCH: u16 = 0x14a;

    /// _IOC(_IOC_READ, 'E', 0x06, len) — 장치 이름 읽기.
    fn eviocgname(len: usize) -> libc::c_ulong {
        (2u64 << 30) | ((len as u64) << 16) | (0x45u64 << 8) | 0x06
    }
    /// _IOC(_IOC_READ, 'E', 0x20 + ev, len) — 지원 이벤트/축 비트맵.
    fn eviocgbit(ev: u16, len: usize) -> libc::c_ulong {
        (2u64 << 30) | ((len as u64) << 16) | (0x45u64 << 8) | (0x20 + ev as u64)
    }
    /// _IOC(_IOC_READ, 'E', 0x40 + abs, sizeof(input_absinfo)) — 축 min/max 조회.
    /// (커널: abs 코드가 **NR 필드**에 인코딩됨)
    fn eviocgabs(code: u16) -> libc::c_ulong {
        (2u64 << 30)
            | ((std::mem::size_of::<InputAbsInfo>() as u64) << 16)
            | (0x45u64 << 8)
            | (0x40 + code as u64)
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct InputAbsInfo {
        value: i32,
        min: i32,
        max: i32,
        fuzz: i32,
        flat: i32,
        resolution: i32,
    }

    /// struct input_event — 24바이트 (x86_64/aarch64 공통, 패딩 없음).
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default)]
    struct InputEvent {
        sec: i64,
        usec: i64,
        etype: u16,
        code: u16,
        value: i32,
    }

    /// 장치의 ABS 지원 비트맵에서 특정 축 지원 여부.
    fn supports_abs(fd: libc::c_int, code: u16) -> bool {
        let mut bits: [u8; 8] = [0; 8];
        let r = unsafe { libc::ioctl(fd, eviocgbit(EV_ABS, bits.len()), bits.as_mut_ptr()) };
        if r < 0 || code as usize >= 64 {
            return false;
        }
        bits[(code as usize) / 8] & (1 << (code % 8)) != 0
    }

    fn device_name(fd: libc::c_int) -> String {
        let mut buf: [u8; 256] = [0; 256];
        let r = unsafe { libc::ioctl(fd, eviocgname(buf.len()), buf.as_mut_ptr()) };
        if r >= 0 {
            String::from_utf8_lossy(&buf[..buf.iter().position(|&b| b == 0).unwrap_or(0)])
                .into_owned()
        } else {
            String::new()
        }
    }

    fn abs_max(fd: libc::c_int, code: u16) -> Option<i32> {
        let mut info = InputAbsInfo::default();
        let r = unsafe { libc::ioctl(fd, eviocgabs(code), &mut info as *mut _) };
        if r < 0 {
            None
        } else {
            Some(info.max)
        }
    }

    pub(super) fn list_devices() -> Vec<PenDeviceInfo> {
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir("/dev/input") else {
            return out;
        };
        let mut paths: Vec<_> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("event"))
            })
            .collect();
        paths.sort();
        for path in paths {
            let Ok(file) = std::fs::File::open(&path) else {
                continue;
            };
            let fd = file.as_raw_fd();
            let has_pressure = supports_abs(fd, ABS_PRESSURE);
            let has_tilt = supports_abs(fd, ABS_TILT_X) && supports_abs(fd, ABS_TILT_Y);
            if !has_pressure && !has_tilt {
                continue;
            }
            out.push(PenDeviceInfo {
                path: path.display().to_string(),
                name: device_name(fd),
                has_tilt,
                has_pressure,
            });
        }
        out
    }

    /// 열린 evdev 펜 장치 — 틸트/필압/접촉을 폴링합니다.
    pub(super) struct PenMonitor {
        file: std::fs::File,
        pending: Vec<u8>,
        tilt: [f32; 2],
        pressure: Option<f32>,
        pressure_max: i32,
        contact: bool,
    }

    impl PenMonitor {
        pub(super) fn open_best() -> Option<Self> {
            // 틸트 지원 장치 우선, 없으면 필압 장치.
            let devices = list_devices();
            let chosen = devices
                .iter()
                .find(|d| d.has_tilt)
                .or_else(|| devices.iter().find(|d| d.has_pressure))?;
            let file = std::fs::File::open(&chosen.path).ok()?;
            let fd = file.as_raw_fd();
            // 논블로킹 — UI 스레드 호출에서 멈추지 않도록.
            unsafe {
                let flags = libc::fcntl(fd, libc::F_GETFL);
                libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
            }
            let pressure_max = abs_max(fd, ABS_PRESSURE).unwrap_or(2047).max(1);
            Some(Self {
                file,
                pending: Vec::with_capacity(64),
                tilt: [0.0, 0.0],
                pressure: None,
                pressure_max,
                contact: false,
            })
        }

        /// 이벤트를 소비하고, 상태가 바뀌었으면 새 스냅샷을 반환합니다.
        pub(super) fn poll(&mut self) -> Option<PenState> {
            let fd = self.file.as_raw_fd();
            let mut buf = [0u8; 512];
            let mut changed = false;
            loop {
                let n = unsafe {
                    libc::read(
                        fd,
                        buf.as_mut_ptr() as *mut libc::c_void,
                        buf.len(),
                    )
                };
                if n <= 0 {
                    break; // EAGAIN 또는 오류
                }
                self.pending.extend_from_slice(&buf[..n as usize]);
                while self.pending.len() >= std::mem::size_of::<InputEvent>() {
                    let ev: InputEvent = {
                        let mut bytes = [0u8; 24];
                        bytes.copy_from_slice(&self.pending[..24]);
                        self.pending.drain(..24);
                        unsafe { std::mem::transmute(bytes) }
                    };
                    match (ev.etype, ev.code) {
                        (EV_ABS, ABS_TILT_X) => {
                            let v = ev.value.clamp(-90, 90) as f32;
                            if self.tilt[0] != v {
                                self.tilt[0] = v;
                                changed = true;
                            }
                        }
                        (EV_ABS, ABS_TILT_Y) => {
                            let v = ev.value.clamp(-90, 90) as f32;
                            if self.tilt[1] != v {
                                self.tilt[1] = v;
                                changed = true;
                            }
                        }
                        (EV_ABS, ABS_PRESSURE) => {
                            let p = (ev.value as f32 / self.pressure_max as f32).clamp(0.0, 1.0);
                            if self.pressure != Some(p) {
                                self.pressure = Some(p);
                                changed = true;
                            }
                        }
                        (EV_KEY, BTN_TOUCH) => {
                            let c = ev.value != 0;
                            if self.contact != c {
                                self.contact = c;
                                changed = true;
                            }
                        }
                        _ => {}
                    }
                }
            }
            if changed {
                Some(self.snapshot())
            } else {
                None
            }
        }

        pub(super) fn snapshot(&self) -> PenState {
            PenState {
                tilt: self.tilt,
                pressure: self.pressure,
                contact: self.contact,
            }
        }
    }
}

/// 디지타이저(틸트/필압 보고) 장치 목록 — 진단용.
#[cfg(target_os = "linux")]
pub fn list_devices() -> Vec<PenDeviceInfo> {
    linux::list_devices()
}

/// Linux evdev에서 가장 적합한 펜 장치를 열어 모니터를 만듭니다 (없으면 None).
/// 내부적으로 채널을 쓰므로 [`PenMonitor`]는 모든 OS에서 동일하게 동작합니다.
#[cfg(target_os = "linux")]
pub fn open_best() -> Option<PenMonitor> {
    let (tx, rx) = std::sync::mpsc::channel();
    let mut inner = linux::PenMonitor::open_best()?;
    std::thread::spawn(move || {
        loop {
            let st = inner.poll();
            let Some(state) = st else {
                std::thread::sleep(std::time::Duration::from_millis(10));
                continue;
            };
            if tx.send(state).is_err() {
                break; // 소비자가 사라짐.
            }
        }
    });
    Some(PenMonitor {
        rx,
        latest: PenState::default(),
    })
}

/// **사용자 공급원용 시임(seam)** — 임의 OS의 HID 스레드에서
/// `Sender<PenState>`로 파싱한 값을 보내면 됩니다.
///
/// ```ignore
/// let (tx, monitor) = freedf_core::pen_input::channel();
/// std::thread::spawn(move || {
///     // hidapi 등으로 원시 리포트를 읽고...
///     if let Some(state) = parse_report(&bytes) {
///         tx.send(state).ok();
///     }
/// });
/// ```
pub fn channel() -> (std::sync::mpsc::Sender<PenState>, PenMonitor) {
    let (tx, rx) = std::sync::mpsc::channel();
    (
        tx,
        PenMonitor {
            rx,
            latest: PenState::default(),
        },
    )
}

/// 이미 있는 채널 수신부로 모니터를 만듭니다.
pub fn from_receiver(rx: std::sync::mpsc::Receiver<PenState>) -> PenMonitor {
    PenMonitor {
        rx,
        latest: PenState::default(),
    }
}

/// 펜 상태 모니터 — 어떤 공급원(evdev/HID 스레드)이든 채널로 값을 넣으면
/// [`PenMonitor::poll`]이 최신 상태를 돌려줍니다. 모든 OS에서 동일한 타입입니다.
pub struct PenMonitor {
    rx: std::sync::mpsc::Receiver<PenState>,
    latest: PenState,
}

impl PenMonitor {
    /// 새 상태가 도착했으면 반환합니다 (누적된 값 중 마지막).
    pub fn poll(&mut self) -> Option<PenState> {
        let mut changed = false;
        while let Ok(state) = self.rx.try_recv() {
            self.latest = state;
            changed = true;
        }
        if changed {
            Some(self.latest)
        } else {
            None
        }
    }

    /// 현재 상태 스냅샷.
    pub fn snapshot(&self) -> PenState {
        self.latest
    }
}

#[cfg(not(target_os = "linux"))]
pub fn list_devices() -> Vec<PenDeviceInfo> {
    Vec::new()
}

#[cfg(not(target_os = "linux"))]
pub fn open_best() -> Option<PenMonitor> {
    None
}

// ── Windows Raw Input (WM_INPUT → RAWHID 바이트) ────────────────────────────
// HIDAPI 직접 읽기는 태블릿 드라이버(예: XP-Pen)가 장치를 **독점**하면
// ReadFile이 "액세스 거부"로 실패합니다. Raw Input(입력 싱크)은 드라이버가
// 실행 중이어도 시스템이 HID 리포트 바이트를 그대로 전달해 줍니다.
// 참고: https://learn.microsoft.com/en-us/windows/win32/inputdev/raw-input

/// Windows에서 디지타이저(펜)의 원시 HID 리포트 바이트를 캡처하는 스레드를
/// 시작합니다. 실패(등록 불가 등)하면 None.
#[cfg(target_os = "windows")]
pub fn spawn_raw_reports() -> Option<std::sync::mpsc::Receiver<RawReport>> {
    windows_raw::spawn()
}

#[cfg(not(target_os = "windows"))]
pub fn spawn_raw_reports() -> Option<std::sync::mpsc::Receiver<RawReport>> {
    None
}

#[cfg(target_os = "windows")]
mod windows_raw {
    use super::RawReport;
    use std::cell::RefCell;
    use std::ptr;
    use std::sync::mpsc::{Receiver, Sender};

    use windows::core::PCWSTR;
    use windows::Win32::Devices::HumanInterfaceDevice::HID_USAGE_PAGE_DIGITIZER;
    use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, POINT, WPARAM};
    use windows::Win32::Graphics::Gdi::HBRUSH;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Input::{
        GetRawInputData, GetRawInputDeviceInfoW, RegisterRawInputDevices, HRAWINPUT, RAWINPUT,
        RAWINPUTDEVICE, RAWINPUTHEADER, RIDEV_INPUTSINK, RIDI_DEVICENAME, RID_INPUT, RIM_TYPEHID,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, HCURSOR, HICON,
        PostQuitMessage, RegisterClassW, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE,
        WNDCLASSW, WNDCLASS_STYLES, HWND_MESSAGE, MSG, WM_CLOSE, WM_INPUT,
    };

    // WndProc은 상태 없는 함수여야 하므로, 스레드 로컬로 송신부를 공유합니다
    // (창과 메시지 루프가 같은 스레드에서 돌기 때문에 안전합니다).
    thread_local! {
        static REPORT_TX: RefCell<Option<Sender<RawReport>>>> = RefCell::new(None);
    }

    pub(super) fn spawn() -> Option<Receiver<RawReport>> {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = run(tx);
        });
        Some(rx)
    }

    /// 메시지 전용 창 생성 + Raw Input 등록 + 메시지 루프.
    fn run(tx: Sender<RawReport>) -> Result<(), ()> {
        let module = unsafe { GetModuleHandleW(PCWSTR::null()) }.map_err(|_| ())?;
        let instance: windows::Win32::Foundation::HINSTANCE = module.into();
        let class: Vec<u16> = "FreeDFPenRawInput\0".encode_utf16().collect();
        let wc = WNDCLASSW {
            style: WNDCLASS_STYLES(0),
            lpfnWndProc: Some(wnd_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: HICON(ptr::null_mut()),
            hCursor: HCURSOR(ptr::null_mut()),
            hbrBackground: HBRUSH(ptr::null_mut()),
            lpszMenuName: PCWSTR::null(),
            lpszClassName: PCWSTR(class.as_ptr()),
        };
        unsafe { RegisterClassW(&wc) };
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                PCWSTR(class.as_ptr()),
                PCWSTR::null(),
                WINDOW_STYLE(0),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                Some(instance),
                None,
            )
        }
        .map_err(|_| ())?;

        // 디지타이저 페이지의 펜(0x02)/디지타이저(0x01)를 입력 싱크로 등록 —
        // 다른 앱/드라이버가 포커스를 가져도 리포트를 받습니다.
        let devices = [
            RAWINPUTDEVICE {
                usUsagePage: HID_USAGE_PAGE_DIGITIZER,
                usUsage: 0x02, // 펜
                dwFlags: RIDEV_INPUTSINK,
                hwndTarget: hwnd,
            },
            RAWINPUTDEVICE {
                usUsagePage: HID_USAGE_PAGE_DIGITIZER,
                usUsage: 0x01, // 디지타이저
                dwFlags: RIDEV_INPUTSINK,
                hwndTarget: hwnd,
            },
        ];
        unsafe {
            RegisterRawInputDevices(&devices, std::mem::size_of::<RAWINPUTDEVICE>() as u32)
        }
        .map_err(|_| ())?;

        REPORT_TX.with(|slot| *slot.borrow_mut() = Some(tx));

        let mut msg = MSG {
            hwnd: HWND(ptr::null_mut()),
            message: 0,
            wParam: WPARAM(0),
            lParam: LPARAM(0),
            time: 0,
            pt: POINT { x: 0, y: 0 },
        };
        loop {
            let ret = unsafe { GetMessageW(&mut msg, None, 0, 0) };
            if ret.0 <= 0 {
                break; // WM_QUIT 또는 오류.
            }
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        Ok(())
    }

    extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        match msg {
            WM_INPUT => {
                if let Some(rep) = read_hid(lparam) {
                    REPORT_TX.with(|slot| {
                        if let Some(tx) = &*slot.borrow() {
                            let _ = tx.send(rep);
                        }
                    });
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                unsafe { PostQuitMessage(0) };
                LRESULT(0)
            }
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }

    /// WM_INPUT의 lParam에서 (장치 경로, HID 리포트 바이트)를 추출합니다
    /// (GetRawInputData 2단계: 크기 조회 → 복사).
    fn read_hid(lparam: LPARAM) -> Option<RawReport> {
        let hraw = HRAWINPUT(lparam.0 as *mut core::ffi::c_void);
        let header_size = std::mem::size_of::<RAWINPUTHEADER>() as u32;
        let mut size: u32 = 0;
        let _ = unsafe { GetRawInputData(hraw, RID_INPUT, None, &mut size, header_size) };
        if size == 0 || size == u32::MAX {
            return None;
        }
        let mut buf: Vec<u8> = vec![0u8; size as usize];
        let copied = unsafe {
            GetRawInputData(
                hraw,
                RID_INPUT,
                Some(buf.as_mut_ptr() as *mut core::ffi::c_void),
                &mut size,
                header_size,
            )
        };
        if copied == u32::MAX || copied == 0 {
            return None;
        }
        buf.truncate(size as usize);
        let raw: &RAWINPUT = unsafe { &*(buf.as_ptr() as *const RAWINPUT) };
        if raw.header.dwType != RIM_TYPEHID.0 {
            return None;
        }
        let device = device_name(raw.header.hDevice);
        let hid = unsafe { raw.data.hid };
        let bytes =
            unsafe { std::slice::from_raw_parts(hid.bRawData.as_ptr(), hid.dwCount as usize) };
        Some(RawReport {
            device,
            bytes: bytes.to_vec(),
        })
    }

    /// hDevice의 장치 경로(`\\?\HID#...`)를 조회합니다.
    fn device_name(handle: HANDLE) -> String {
        let mut size: u32 = 0;
        let _ = unsafe { GetRawInputDeviceInfoW(Some(handle), RIDI_DEVICENAME, None, &mut size) };
        if size == 0 || size == u32::MAX {
            return String::new();
        }
        let mut buf: Vec<u16> = vec![0; (size / 2) as usize];
        let copied = unsafe {
            GetRawInputDeviceInfoW(
                Some(handle),
                RIDI_DEVICENAME,
                Some(buf.as_mut_ptr() as *mut core::ffi::c_void),
                &mut size,
            )
        };
        if copied == u32::MAX || copied == 0 {
            return String::new();
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..len])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_devices_returns_without_crashing() {
        // 장치가 있든 없든 패닉 없이 반환해야 합니다.
        let _ = list_devices();
    }

    #[test]
    fn pen_state_default_is_neutral() {
        let s = PenState::default();
        assert_eq!(s.tilt, [0.0, 0.0]);
        assert!(s.pressure.is_none());
        assert!(!s.contact);
    }
}
