//! services 셔임 — 실제 구현은 freedf-services 크레이트로 이동했습니다
//! (docs/freedf-gui-migration.md Phase 1). 기존 `crate::recording::*` 호출부를
//! 깨지 않으려고 공개 항목 전부를 재노출합니다.
pub(crate) use freedf_services::recording::*;
