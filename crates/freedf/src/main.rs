//! FreeDF — lightweight PDF viewer + drawing pad.
//!
//! FreeDF v2: 모든 데이터(노트/PDF/주석/세션/로그)는 PostgreSQL(18.6, Docker)에
//! 저장됩니다. 연결은 `FREEDF_DATABASE_URL`(기본
//! `postgres://freedf:freedf@localhost:5432/freedf`)로 결정됩니다.
//! **스키마는 앱이 만들지 않습니다** — DB 호스트에서 `server/db/up.sh`를
//! 먼저 실행하세요 (컨테이너 기동 + 마이그레이션 자동 적용).
//! PDFium 라이브러리는 여전히 실행 파일 옆에 필요합니다.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod cache;
mod db;
mod fonts;
mod pdf;
mod recent;
mod server;
mod settings;
mod storage;
mod theme;

use eframe::egui;
use freedf_core::logging::Logger;
use std::path::PathBuf;

fn main() -> eframe::Result<()> {
    // CLI: `freedf <file.pdf>` — 외부 PDF import 후 열기.
    //      `freedf --doc <id>` — DB의 문서 id로 열기 ("새 창" 분리 시 사용).
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

    // DB 연결 — 실패해도 앱은 실행되고, 첫 화면 대화상자에서 URL을 입력받습니다.
    // 우선순위: FREEDF_DATABASE_URL 환경 변수 → 마지막 연결 성공(connection.json)
    // → 기본값. (하드코딩 없음 — 항상 런타임 입력 가능)
    let db_url = std::env::var("FREEDF_DATABASE_URL")
        .ok()
        .or_else(storage::load_saved_connection)
        .unwrap_or_else(|| db::DEFAULT_DATABASE_URL.to_string());
    let (db, db_connected, connect_error) = match storage::from_env(&db_url) {
        Ok(db) => (db, true, None),
        Err(e) => (storage::disconnected(), false, Some(e)),
    };

    // 이벤트 로그는 백그라운드 스레드로 비동기 기록 — 스트로크마다 원격 왕복
    // 없이 필기 응답성을 유지합니다 (연결 전이면 no-op 폴백).
    let (log_tx, log_rx) = std::sync::mpsc::channel();
    let log_db = db.clone();
    std::thread::spawn(move || {
        for (epoch_ms, seq, event) in log_rx {
            log_db.insert_log(epoch_ms, seq, &event);
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

            Ok(Box::new(app::FreeDfApp::new(
                cc,
                db,
                db_connected,
                db_url,
                connect_error,
                logger,
                open_path,
                open_doc,
            )))
        }),
    )
}
