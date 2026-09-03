//! FreeDF — lightweight PDF viewer + drawing pad.
//!
//! FreeDF v2: 모든 데이터(노트/PDF/주석/세션/로그)는 PostgreSQL(Docker)에
//! 저장됩니다. 연결은 `FREEDF_DATABASE_URL`(기본
//! `postgres://freedf:freedf@localhost:5432/freedf`)로 결정되며,
//! `docker compose up -d db`로 띄우면 바로 사용할 수 있습니다.
//! PDFium 라이브러리는 여전히 실행 파일 옆에 필요합니다.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod db;
mod fonts;
mod pdf;
mod recent;
mod settings;
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

    // PostgreSQL 연결 (없으면 앱을 실행할 수 없습니다 — Docker로 띄우세요).
    let db_url = std::env::var("FREEDF_DATABASE_URL")
        .unwrap_or_else(|_| db::DEFAULT_DATABASE_URL.to_string());
    let db = match db::Db::connect(&db_url) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("FreeDF database error: {e}");
            eprintln!("Start the database with: docker compose up -d db");
            std::process::exit(1);
        }
    };

    // 이벤트 로그 → PostgreSQL event_log 테이블.
    let logger = {
        let db = db.clone();
        Logger::to_sink(move |entry| {
            let event = serde_json::to_value(&entry.event).unwrap_or(serde_json::Value::Null);
            db.insert_log(entry.epoch_ms, entry.seq, &event);
        })
    };

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
                cc, db, logger, open_path, open_doc,
            )))
        }),
    )
}
