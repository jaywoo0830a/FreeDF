//! FreeDF — lightweight PDF viewer + drawing pad.
//!
//! Windows 11-friendly settings (system theme, HiDPI, native file dialogs) are
//! applied at startup. App data (notes + logs) lives under
//! `%LOCALAPPDATA%/FreeDF` on Windows and `~/.local/share/freedf` elsewhere.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod export;
mod fonts;
mod pdf;
mod recent;
mod settings;
mod theme;

use eframe::egui;
use freedf_core::logging::{AppEvent, Logger};
use freedf_core::notes::NotesManager;
use std::path::PathBuf;

/// Per-user app data directory.
fn app_data_dir() -> PathBuf {
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(local).join("FreeDF");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".local").join("share").join("freedf");
    }
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("freedf-data")
}

fn main() -> eframe::Result<()> {
    // Optional CLI: `freedf <file.pdf>` (or `freedf --open <file.pdf>`) opens a
    // standalone PDF on startup. "Open in New Window" from the tab bar re-launches
    // this executable with a document path as its argument.
    let mut args = std::env::args().skip(1);
    let mut open_path: Option<PathBuf> = None;
    while let Some(a) = args.next() {
        if a == "--open" {
            open_path = args.next().map(PathBuf::from);
        } else if !a.starts_with("--") {
            open_path = Some(PathBuf::from(a));
        }
    }
    let open_path = open_path.filter(|p| p.is_file());

    let options = eframe::NativeOptions {
        // wgpu(DX12) 대신 호환성 높은 OpenGL(glow) 렌더러 사용 (Windows 크래시 방지)
        renderer: eframe::Renderer::Glow,
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
            // Single UI font: PT Serif (bundled) + Nord design system
            fonts::install_inter(&cc.egui_ctx);
            theme::nord::install(&cc.egui_ctx);

            // App data layout: <data>/notes + <data>/logs + <data>/session.json
            let data_dir = app_data_dir();
            let notes_dir = data_dir.join("notes");
            let logs_dir = data_dir.join("logs");
            let _ = std::fs::create_dir_all(&notes_dir);
            let _ = std::fs::create_dir_all(&logs_dir);
            let default_session_path = data_dir.join("session.json");

            let notes = NotesManager::load_or_create(notes_dir);

            let mut logger = Logger::to_file(&logs_dir.join("freedf.log"))
                .unwrap_or_else(|_| Logger::disabled());
            logger.log(AppEvent::AppStart {
                version: env!("CARGO_PKG_VERSION").to_string(),
            });

            Ok(Box::new(app::FreeDfApp::new(
                cc,
                notes,
                logger,
                default_session_path,
                open_path,
            )))
        }),
    )
}
