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
//!
//! Windows 옵션:
//!   --list           # 장치 목록만
//!   --index N        # 목록의 N번째 장치 선택 (기본: 펜 usage=0x02 첫 번째)

use freedf_core::pen_input;

#[cfg(target_os = "windows")]
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let list_only = args.iter().any(|a| a == "--list");
    let index_arg = args
        .iter()
        .position(|a| a == "--index")
        .and_then(|i| args.get(i + 1))
        .and_then(|s| s.parse::<usize>().ok());

    // ── 1순위: Raw Input — 드라이버 독점이어도 리포트 바이트를 받습니다 ──
    if !list_only && index_arg.is_none() {
        if let Some(rx) = pen_input::spawn_raw_reports() {
            println!("[hidprobe] Raw Input 캡처 시작 (드라이버 독점이어도 동작).");
            println!("[hidprobe] 같은 리포트는 압축해 표시합니다 — 펜을 대고 그리면");
            println!("[hidprobe] 여러 바이트의 다른 ID 리포트가 섞여 나와야 합니다.");
            let mut last: Option<Vec<u8>> = None;
            let mut repeat: u32 = 0;
            while let Ok(bytes) = rx.recv() {
                if last.as_ref() == Some(&bytes) {
                    repeat += 1;
                    continue;
                }
                if repeat > 0 {
                    println!("  … 위 리포트 {repeat}회 반복");
                    repeat = 0;
                }
                let hex: Vec<String> =
                    bytes.iter().map(|b| format!("{b:02x}")).collect();
                let marker = if bytes.len() > 1 { " ← 펜 데이터!" } else { "" };
                println!("report({}B): {}{marker}", bytes.len(), hex.join(" "));
                last = Some(bytes);
            }
            println!("[hidprobe] Raw Input 스트림이 종료되었습니다.");
            return;
        }
    }

    // ── 2순위: hidapi 직접 읽기 (장치 목록/선택 포함) ──
    let api = match hidapi::HidApi::new() {
        Ok(a) => a,
        Err(e) => {
            println!("[hidprobe] HidApi 초기화 실패: {e}");
            return;
        }
    };
    let devices: Vec<hidapi::DeviceInfo> = api
        .device_list()
        .filter(|d| d.usage_page() == 0x0D) // 디지타이저만
        .cloned()
        .collect();
    if devices.is_empty() {
        println!("[hidprobe] 디지타이저(usage_page=0x0D) 장치를 찾지 못했습니다.");
        println!("[hidprobe] 태블릿 드라이버가 HID 펜 컬렉션을 노출하는지 확인하세요.");
        return;
    }
    println!("[hidprobe] 발견된 디지타이저:");
    for (i, d) in devices.iter().enumerate() {
        println!(
            "  [{i}] vid={:04x} pid={:04x} usage={:04x} product={}",
            d.vendor_id(),
            d.product_id(),
            d.usage(),
            d.product_string().unwrap_or("?")
        );
    }
    if list_only {
        return;
    }
    let chosen = match index_arg {
        Some(i) => devices.get(i),
        None => devices
            .iter()
            .find(|d| d.usage() == 0x02) // 펜
            .or_else(|| devices.first()),
    };
    let Some(info) = chosen else {
        println!("[hidprobe] 선택된 장치가 없습니다 (--index 범위 확인).");
        return;
    };
    let device = match info.open_device(&api) {
        Ok(d) => d,
        Err(e) => {
            println!("[hidprobe] 장치 열기 실패: {e}");
            println!("[hidprobe] 다른 프로세스(태블릿 드라이버 등)가 독점 중일 수 있습니다.");
            return;
        }
    };
    let _ = device.set_blocking_mode(false);
    println!(
        "[hidprobe] 선택: vid={:04x} pid={:04x} — 스트리밍 시작 (Ctrl+C 종료).",
        info.vendor_id(),
        info.product_id()
    );
    println!("[hidprobe] 바이트 중 **움직이는 위치**가 X/Y/필압/틸트 필드입니다.");
    let mut buf = [0u8; 128];
    let mut err_count: u64 = 0;
    let mut empty_count: u64 = 0;
    let mut last_notice = std::time::Instant::now();
    loop {
        match device.read_timeout(&mut buf, 500) {
            Ok(n) if n > 0 => {
                err_count = 0;
                empty_count = 0;
                let hex: Vec<String> = buf[..n].iter().map(|b| format!("{b:02x}")).collect();
                println!("report({n}B): {}", hex.join(" "));
            }
            Ok(_) => {
                // 리포트 없음(타임아웃) — 펜이 닿지 않았거나 이 장치가 펜이 아님.
                empty_count += 1;
                if last_notice.elapsed() > std::time::Duration::from_secs(3) {
                    println!(
                        "(3초간 리포트 없음 — 펜으로 직접 그어 보세요. 안 나오면 --index로 다른 장치 시도)"
                    );
                    last_notice = std::time::Instant::now();
                }
            }
            Err(e) => {
                err_count += 1;
                if err_count == 1 || (err_count <= 5 && err_count % 3 == 0) {
                    println!("[hidprobe] 읽기 오류({err_count}회): {e}");
                }
                if err_count == 1 {
                    println!(
                        "[hidprobe] 힌트: 태블릿 드라이버가 장치를 독점 중이면 읽기가 막힙니다. \\\n\
                         드라이버를 잠시 종료하거나 --index로 다른 장치를 시도해 보세요."
                    );
                }
                std::thread::sleep(std::time::Duration::from_millis(1000));
            }
        }
        let _ = empty_count;
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
