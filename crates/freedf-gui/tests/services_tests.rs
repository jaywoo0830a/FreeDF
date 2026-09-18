//! 서비스 계층 연결 확인 — `src/main.rs`의 `#[cfg(test)]` 모듈에서 이동했습니다.
//!
//! freedf-gui가 `freedf-services`(서버/저장소/PDF/설정)를 직접 쓸 수 있는지
//! (Phase 1 — docs/freedf-gui-migration.md).

/// freedf-services 계층이 이 크레이트에서 그대로 쓰인다.
#[test]
fn services_available() {
    let cfg = freedf_services::server::MediaServerConfig::default();
    let _ = cfg.normalized_base();
    let _ = freedf_services::storage::app_data_dir();
    assert_eq!(freedf_services::settings::MAX_FAVORITE_COLORS, 8);
    let _ = freedf_services::pdf::MAX_RENDER_DIM;
}
