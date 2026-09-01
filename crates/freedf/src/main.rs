//! FreeDF — 초경량 PDF 뷰어 + 드로잉 패드.
//!
//! Windows 11 친화적 설정(시스템 테마 따라가기, HiDPI, 네이티브 파일 대화상자)을
//! 적용해 실행합니다.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod export;
mod pdf;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        // wgpu(DX12) 대신 호환성 높은 OpenGL(glow) 렌더러 사용
        renderer: eframe::Renderer::Glow,
        viewport: egui::ViewportBuilder::default()
            .with_title("FreeDF — 초경량 PDF 뷰어")
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([760.0, 520.0]),
        ..Default::default()
    };

    eframe::run_native(
        "FreeDF",
        options,
        Box::new(|cc| Ok(Box::new(app::FreeDfApp::new(cc)))),
    )
}
