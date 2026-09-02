//! FreeDF HID/evdev 진단 도구.
//!
//! egui/winit이 노출하지 않는 **펜 틸트/필압**이 시스템에 실제로 보고되는지
//! 확인하고, 값을 실시간으로 파싱해 출력합니다.
//!
//! ```text
//! freedf-hidprobe            # 펜 장치의 값을 실시간 스트리밍
//! freedf-hidprobe --list     # 장치 목록과 지원 축
//! ```
//!
//! - Linux: `/dev/input/event*`(evdev) — 틸트/필압/접촉을 바로 디코딩해 출력.
//! - Windows: hidapi — 펜 디지타이저의 **원시 리포트 바이트(hex)**를 출력해
//!   사용자가 오프셋을 직접 파악하도록 돕습니다 (변하는 바이트 = 필드).

use freedf_core::pen_input;

#[cfg(target_os = "windows")]
fn main() {
    let list_only = std::env::args().any(|a| a == "--list");
    let api = match hidapi::HidApi::new() {
        Ok(a) => a,
        Err(e) => {
            println!("[hidprobe] HidApi 초기화 실패: {e}");
            return;
        }
    };
    let devices: Vec<hidapi::DeviceInfo> = api
        .device_list()
        .filter(|d| d.usage_page == 0x0D) // 디지타이저만
        .cloned()
        .collect();
    if devices.is_empty() {
        println!("[hidprobe] 디지타이저(usage_page=0x0D) 장치를 찾지 못했습니다.");
        println!("[hidprobe] 태블릿 드라이버가 HID 펜 컬렉션을 노출하는지 확인하세요.");
        return;
    }
    println!("[hidprobe] 발견된 디지타이저:");
    for d in &devices {
        println!(
            "  vid={:04x} pid={:04x} usage={:04x} product={}",
            d.vendor_id,
            d.product_id,
            d.usage,
            d.product_string
                .as_ref()
                .and_then(|s| s.to_str().ok())
                .unwrap_or("?")
        );
    }
    if list_only {
        return;
    }
    let chosen = devices
        .iter()
        .find(|d| d.usage == 0x02) // 펜
        .or_else(|| devices.first());
    let Some(info) = chosen else {
        return;
    };
    let device = match info.open_device(&api) {
        Ok(d) => d,
        Err(e) => {
            println!("[hidprobe] 장치 열기 실패: {e}");
            return;
        }
    };
    let _ = device.set_blocking_mode(false);
    println!("[hidprobe] 스트리밍 시작 — 펜으로 그어 보세요 (Ctrl+C 종료).");
    println!("[hidprobe] 바이트 중 **움직이는 위치**가 X/Y/필압/틸트 필드입니다.");
    let mut buf = [0u8; 64];
    loop {
        match device.read_timeout(&mut buf, 500) {
            Ok(n) if n > 0 => {
                let hex: Vec<String> = buf[..n].iter().map(|b| format!("{b:02x}")).collect();
                println!("report({n}B): {}", hex.join(" "));
            }
            Ok(_) => {}
            Err(_) => {}
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn main() {
    let list_only = std::env::args().any(|a| a == "--list");

    let devices = pen_input::list_devices();
    if devices.is_empty() {
        println!("[hidprobe] 디지타이저(틸트/필압 보고) 장치를 찾지 못했습니다.");
        println!("[hidprobe] - /dev/input 이 없거나 장치가 연결되지 않았습니다.");
        println!("[hidprobe] - WSL/X11 포워딩 환경에서는 호스트 장치가 보이지 않습니다.");
        return;
    }
    println!("[hidprobe] 발견된 장치:");
    for d in &devices {
        println!(
            "  {}  name=\"{}\"  tilt={}  pressure={}",
            d.path, d.name, d.has_tilt, d.has_pressure
        );
    }
    if list_only {
        return;
    }

    let Some(mut mon) = pen_input::open_best() else {
        println!("[hidprobe] 장치를 열 수 없습니다 (권한? 그룹 input?).");
        return;
    };
    println!("[hidprobe] 스트리밍 시작 — 펜으로 그어 보세요 (Ctrl+C 종료).");
    let mut last_print = std::time::Instant::now();
    loop {
        if let Some(st) = mon.poll() {
            let t = st.tilt;
            println!(
                "tilt=[{:+.0}°, {:+.0}°]  pressure={}  contact={}",
                t[0],
                t[1],
                st.pressure
                    .map(|p| format!("{:.3}", p))
                    .unwrap_or_else(|| "미보고".into()),
                st.contact
            );
            last_print = std::time::Instant::now();
        }
        if last_print.elapsed() > std::time::Duration::from_secs(3) {
            // 조용한 상태 표시.
            let st = mon.snapshot();
            println!(
                "(대기 중… 마지막 상태) tilt=[{:+.0}°, {:+.0}°]  pressure={}",
                st.tilt[0],
                st.tilt[1],
                st.pressure
                    .map(|p| format!("{:.3}", p))
                    .unwrap_or_else(|| "미보고".into())
            );
            last_print = std::time::Instant::now();
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}
