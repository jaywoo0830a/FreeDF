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
mod settings;
mod style;

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
            // Single UI font: PT Serif (bundled) + design system
            fonts::install_pt_serif(&cc.egui_ctx);
            style::install(&cc.egui_ctx);

            // App data layout: <data>/notes + <data>/logs + <data>/settings.json
            let data_dir = app_data_dir();
            let notes_dir = data_dir.join("notes");
            let logs_dir = data_dir.join("logs");
            let _ = std::fs::create_dir_all(&notes_dir);
            let _ = std::fs::create_dir_all(&logs_dir);
            let settings_path = data_dir.join("settings.json");

            let notes = NotesManager::load_or_create(notes_dir);

            let mut logger = Logger::to_file(&logs_dir.join("freedf.log"))
                .unwrap_or_else(|_| Logger::disabled());
            logger.log(AppEvent::AppStart {
                version: env!("CARGO_PKG_VERSION").to_string(),
            });

            Ok(Box::new(app::FreeDfApp::new(cc, notes, logger, settings_path)))
        }),
    )
}
