//! services 셔임 — 실제 구현은 freedf-services 크레이트로 이동했습니다
//! (docs/freedf-gui-migration.md Phase 1). 현재 freedf 코드는 직접 참조하지
//! 않지만(소비처가 services 내부 storage.rs뿐) 경로 안정성을 위해 셔임을 유지합니다.
#[allow(unused_imports)]
pub(crate) use freedf_services::sync_storage::*;
