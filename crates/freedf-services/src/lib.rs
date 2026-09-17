//! # freedf-services — FreeDF 서비스 계층
//!
//! UI 없이 동작하는 런타임 서비스 모음 (저장소 백엔드 · Sync v3 캐시 · 미디어
//! 서버 클라이언트 · PDFium 렌더 · 세션 설정 · 최근 항목 · 오디오 녹음/재생).
//!
//! `docs/freedf-gui-migration.md` Phase 1: `crates/freedf`에서 추출했습니다.
//! `crates/freedf`와 `crates/freedf-gui` 양쪽이 이 크레이트를 공유하고, freedf는
//! 모듈 셔임(`pub use freedf_services::…`)으로 기존 경로를 유지합니다.
//!
//! 예외: `theme`(egui 스타일)과 `dictionary`의 오버레이 UI는 여전히 freedf 쪽에
//! 있습니다. settings의 `MacroKey::from_egui`만 egui::Key 타입을 씁니다.

pub mod pdf;
pub mod player;
pub mod recent;
pub mod recording;
pub mod server;
pub mod settings;
pub mod storage;
pub mod sync_client;
pub mod sync_storage;
