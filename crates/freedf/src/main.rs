//! FreeDF — lightweight PDF viewer + drawing pad.
//!
//! FreeDF v3: 모든 문서는 **Sync v3 API 서버**(server/backend, 스냅샷 ZIP
//! 동기화)를 통해 저장됩니다. 연결은 `server.json`(앱 데이터 폴더)의
//! 서버 주소/API 키로 결정됩니다 — 첫 실행 대화상자에서 입력합니다.
//! PDFium 라이브러리는 여전히 실행 파일 옆에 필요합니다.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod fonts;
mod pdf;
mod player;
mod recent;
mod recording;
mod server;
mod settings;
mod storage;
mod sync_client;
mod sync_storage;
mod theme;
mod ui;

use eframe::egui;
use freedf_core::logging::Logger;
use std::path::PathBuf;

fn main() -> eframe::Result<()> {
    // CLI: `freedf <file.pdf>` — 외부 PDF import 후 열기.
    //      `freedf --doc <id>` — 서버의 문서 id로 열기 ("새 창" 분리 시 사용).
    let mut args = std::env::args().skip(1);
    let mut open_path: Option<PathBuf> = None;
    let mut open_doc: Option<i64> = None;
    while let Some(a) = args.next() {
        if a == "--open" {
            open_path = args.next().map(PathBuf::from);
        } else if a == "--doc" {
            open_doc = args.next().and_then(|s| s.parse::<i64>().ok());
        } else if !a.starts_with("--") {
            open_path = Some(PathBuf::from(a));
        }
    }
    let open_path = open_path.filter(|p| p.is_file());

    // 서버 연결 — `server.json`(Sync v3 + 미디어 공용)에서 런타임 로드.
    // 설정이 없으면 disconnected 폴백으로 시작하고 첫 실행 대화상자가 떠서
    // 서버 주소/API 키를 입력받습니다 (DB URL 입력 워크플로우는 제거됨).
    let media_config = server::MediaServerConfig::load(&server::MediaServerConfig::config_path());
    let db_connected = media_config.enabled;
    let connect_error: Option<String> = None;
    let db = storage::from_server_config(&media_config);

    // 이벤트 로그는 백그라운드 스레드로 비동기 기록 — 스트로크마다 원격 왕복
    // 없이 필기 응답성을 유지합니다 (연결 전이면 no-op 폴백).
    let (log_tx, log_rx) = std::sync::mpsc::channel();
    let log_db = db.clone();
    std::thread::spawn(move || {
        // 이벤트를 모아서 배치 기록 — 이벤트마다 원격 왕복하지 않습니다.
        loop {
            match log_rx.recv() {
                Ok((epoch_ms, seq, event)) => {
                    let mut items = vec![(epoch_ms, event)];
                    let _ = seq;
                    while items.len() < 200 {
                        match log_rx.try_recv() {
                            Ok((e2, _s2, ev2)) => items.push((e2, ev2)),
                            Err(_) => break,
                        }
                    }
                    log_db.insert_logs(&items);
                }
                Err(_) => break,
            }
        }
    });
    let logger = Logger::to_sink(move |entry| {
        let event = serde_json::to_value(&entry.event).unwrap_or(serde_json::Value::Null);
        let _ = log_tx.send((entry.epoch_ms, entry.seq, event));
    });

    let options = eframe::NativeOptions {
        // 기본: OpenGL(glow) — 호환성이 가장 좋습니다 (Windows 크래시 방지).
        // `FREEDF_RENDERER=wgpu`로 실행하면 DirectX 12 백엔드로 GPU 오프로드
        // (페이지 전환/줌/합성이 더 부드러움, 전용 GPU 권장).
        renderer: match std::env::var("FREEDF_RENDERER").as_deref() {
            Ok("wgpu") => eframe::Renderer::Wgpu,
            _ => eframe::Renderer::Glow,
        },
        viewport: egui::ViewportBuilder::default()
            .with_title("FreeDF — Lightweight PDF Viewer & Ink")
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([760.0, 520.0]),
        ..Default::default()
    };

    eframe::run_native(
        "FreeDF",
        options,
        Box::new(|cc| {
            // Bundled Inter(+NanumGothic) 폰트 + Nord 디자인 시스템
            fonts::install_inter(&cc.egui_ctx);
            theme::nord::install(&cc.egui_ctx);

            // OS 네이티브 창 — Windows 11 스타일 (다크 타이틀바·라운드 코너·Mica).
            #[cfg(target_os = "windows")]
            {
                use raw_window_handle::{HasWindowHandle, RawWindowHandle};
                if let Some(hwnd) = cc.winit_window().and_then(|w| match w.window_handle() {
                    Ok(h) => match h.as_raw() {
                        RawWindowHandle::Win32(wh) => {
                            Some(wh.hwnd.get() as *mut std::ffi::c_void)
                        }
                        _ => None,
                    },
                    Err(_) => None,
                }) {
                    app::winstyle::apply(hwnd);
                }
            }

            Ok(Box::new(app::FreeDfApp::new(
                cc,
                db,
                db_connected,
                connect_error,
                logger,
                open_path,
                open_doc,
            )))
        }),
    )
}
