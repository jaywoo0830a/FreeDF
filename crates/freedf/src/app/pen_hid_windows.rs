//! Windows HID 펜 입력 (hidapi, 저수준) — **스켈레톤**.
//!
//! Windows에서는 egui/winit이 틸트를 노출하지 않으므로, HID 리포트 바이트를
//! 직접 읽어 파싱합니다. 이 모듈은 장치 선택·리포트 수신까지 갖춰져 있고,
//! **`parse_report` 한 함수만 채우면 됩니다**.
//!
//! # 사용자 구현 순서
//! 1. `freedf-hidprobe --list`(Windows 빌드)로 펜 장치의 vid/pid/usage 확인.
//! 2. 펜으로 그으면서 리포트 바이트(hex)를 관찰 — 움직이는 바이트가 X/Y/
//!    틸트/필압 필드입니다.
//! 3. `parse_report`에 바이트 → [`PenState`] 매핑을 구현합니다.
//!
//! # 흔한 리포트 레이아웃 (Microsoft 펜 컬렉션 예시)
//! - `report_id` (0번 바이트, 장치에 따라 있음)
//! - 버튼 비트마스크
//! - X(2바이트 LE) / Y(2바이트 LE)
//! - 필압 (1~2바이트, 0..1024 등 — 장치별 최대값으로 정규화)
//! - X Tilt / Y Tilt (각 1바이트, signed, ±60° 정도)
//!
//! **주의**: UCLogic/XP-Pen 계열은 `06` 1바이트 상태(하트비트) 리포트를
//! 계속 보냅니다 — 무시하세요. 펜을 대고 그릴 때 오는 여러 바이트
//! 리포트(보통 report ID 02)가 실제 펜 데이터입니다.
//!
//! 펜을 좌우로 눕히며 **마지막 2~4바이트**가 ±로 변하면 그게 틸트입니다.
//! 값이 안 보이면 리포터가 다르므로 다른 오프셋을 시도해 보세요.

use freedf_core::pen_input::{self, PenMonitor, PenState};

/// 디지타이저 장치 목록 (진단용).
pub fn list_devices() -> Vec<String> {
    let Ok(api) = hidapi::HidApi::new() else {
        return Vec::new();
    };
    api.device_list()
        .map(|d| {
            format!(
                "vid={:04x} pid={:04x} usage_page={:04x} usage={:04x} product={}",
                d.vendor_id(),
                d.product_id(),
                d.usage_page(),
                d.usage(),
                d.product_string().unwrap_or("?")
            )
        })
        .collect()
}

/// 펜(디지타이저) 입력 공급원을 시작합니다. 우선순위:
///
/// 1순위: **OTD 데몬 IPC** — VMulti/Raw Input/HIDAPI를 전부 우회, OTD가
/// 파싱한 리포트(틸트·필압 포함)를 데몬에서 직접 받습니다.
/// 2순위: Raw Input(WM_INPUT) — OTD가 없으면 가상/일반 HID 펜 리포트.
/// 3순위: hidapi 직접 읽기 폴백.
pub fn spawn_monitor() -> Option<PenMonitor> {
    if let Some(rx) = freedf_core::pen_input::spawn_otd_monitor() {
        return Some(pen_input::from_receiver(rx));
    }
    if let Some(rx) = freedf_core::pen_input::spawn_raw_reports() {
        return Some(monitor_from_raw_reports(rx));
    }
    spawn_hidapi_monitor()
}

/// Raw Input 리포트 바이트 → `parse_report` → 앱 모니터.
fn monitor_from_raw_reports(
    rx: std::sync::mpsc::Receiver<freedf_core::pen_input::RawReport>,
) -> PenMonitor {
    let (tx, monitor) = pen_input::channel();
    std::thread::spawn(move || {
        while let Ok(report) = rx.recv() {
            if let Some(state) = parse_report(&report.bytes) {
                if tx.send(state).is_err() {
                    break; // 앱 종료.
                }
            }
        }
    });
    monitor
}

/// hidapi 직접 읽기 폴백 — 드라이버가 장치를 독점 중이면 ReadFile이
/// "액세스 거부"로 실패할 수 있습니다.
fn spawn_hidapi_monitor() -> Option<PenMonitor> {
    let api = hidapi::HidApi::new().ok()?;

    // ── 장치 선택 ──
    // 디지타이저(usage_page=0x0D) + 펜(usage=0x02) 우선.
    // 특정 태블릿만 쓰려면 여기에 vid/pid 필터를 추가하세요.
    let candidates: Vec<hidapi::DeviceInfo> = api
        .device_list()
        .filter(|d| d.usage_page() == 0x0D)
        .cloned()
        .collect();
    let chosen = candidates
        .iter()
        .find(|d| d.usage() == 0x02)
        .or_else(|| candidates.first())?;
    let device = chosen.open_device(&api).ok()?;
    // 리포트를 기다리지 않고 폴링합니다 (read_timeout으로 타임아웃).
    let _ = device.set_blocking_mode(false);

    // ── 수신 스레드 — 원시 리포트 → parse_report → 앱으로 전달 ──
    let (tx, monitor) = pen_input::channel();
    std::thread::spawn(move || {
        let mut buf = [0u8; 128];
        let mut first_error = true;
        loop {
            match device.read_timeout(&mut buf, 500) {
                Ok(n) if n > 0 => {
                    if let Some(state) = parse_report(&buf[..n]) {
                        if tx.send(state).is_err() {
                            break; // 앱 종료.
                        }
                    }
                }
                Ok(_) => {} // 0바이트(타임아웃) — 계속.
                Err(e) => {
                    if first_error {
                        // 진단용 — 원인(독점/해제)을 한 번만 알립니다.
                        eprintln!("[pen_hid] 리포트 읽기 오류: {e}");
                        first_error = false;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(1000));
                }
            }
        }
    });
    Some(monitor)
}

/// 원시 HID 리포트 바이트 → 펜 상태. **여기에 실제 매핑을 구현하세요.**
///
/// 힌트: `tilt`는 도(±90), `pressure`는 0..1로 정규화해 주세요.
fn parse_report(data: &[u8]) -> Option<PenState> {
    // 1바이트 리포트(하트비트 등)는 무시합니다.
    if data.len() < 2 {
        return None;
    }
    // ── 구현 예시 (장치에 맞게 오프셋 수정) ──────────────────────────────
    // // 필압: 5바이트 (0..1024)
    // let raw_p = u16::from_le_bytes([data[5], data[6]]) as f32;
    // // 틸트: 7/8바이트 signed (±60°)
    // let t_x = (data[7] as i8) as f32;
    // let t_y = (data[8] as i8) as f32;
    // Some(PenState {
    //     tilt: [t_x.clamp(-90.0, 90.0), t_y.clamp(-90.0, 90.0)],
    //     pressure: Some((raw_p / 1024.0).clamp(0.0, 1.0)),
    //     contact: true,
    // })
    // ─────────────────────────────────────────────────────────────────────

    // 아직 매핑 전 — 디버그용으로 바이트 덤프가 필요하면 아래를 주석 해제:
    // eprintln!("report: {}", data.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" "));
    let _ = data;
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_devices_does_not_crash() {
        let _ = list_devices();
    }
}
