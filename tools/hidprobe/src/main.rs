//! FreeDF 펜 입력 진단 도구.
//!
//! egui/winit이 노출하지 않는 **펜 틸트/필압**을 확인하고, 값을 실시간으로
//! 출력합니다.
//!
//! - Linux: `/dev/input/event*`(evdev) — 틸트/필압/접촉을 바로 디코딩해 출력.
//! - Windows: **OTD 데몬 IPC** — OpenTabletDriver가 파싱한 리포트(틸트·필압
//!   포함)를 직접 수신해 출력합니다.

use freedf_core::pen_input;

#[cfg(target_os = "windows")]
fn main() {
    // Windows: OTD 데몬 IPC 스트림.
    match pen_input::spawn_otd_monitor() {
        Some(rx) => {
            println!("[hidprobe] OTD 데몬 IPC 스트림 시작 — 그려 보세요 (Ctrl+C 종료).");
            let mut last = std::time::Instant::now();
            while let Ok(st) = rx.recv() {
                println!(
                    "tilt=[{:+.0}°, {:+.0}°]  pressure={:.3}  contact={}",
                    st.tilt[0],
                    st.tilt[1],
                    st.pressure.unwrap_or(0.0),
                    st.contact
                );
                last = std::time::Instant::now();
            }
            let _ = last;
            println!("[hidprobe] OTD 스트림이 종료되었습니다.");
        }
        None => {
            println!("[hidprobe] OTD 데몬에 연결할 수 없습니다 (OTD daemon 실행 중인지 확인).");
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
