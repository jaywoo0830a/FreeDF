//! FreeDF Sync v3 — 프로토콜 전용 타입·코덱·클라이언트.
//!
//! 설계: `docs/sync-protocol-v3.md` · OpenAPI: `docs/openapi/sync-v3.openapi.yaml`.
//!
//! 클라이언트가 알아야 하는 것 셋:
//! - [`Snapshot`] — 문서 전체 상태의 ZIP 왕복. `to_zip()`/`from_zip()`.
//! - [`Patch`]/[`ChangeRecord`] — 충돌 병합(`apply_patch`)과 변경분 적용(`apply_changes`).
//! - [`SyncClient`] — API 서버 HTTP 통신 (feature `client`).
//!
//! 서버(`server/backend`)도 이 크레이트의 타입을 공유합니다 — 단일 진실 공급원.

pub mod digest;
pub mod error;
pub mod proto;
pub mod snapshot;
#[cfg(feature = "client")]
pub mod client;

pub use digest::Digest;
pub use error::{Result, SyncError};
pub use proto::*;
pub use snapshot::Snapshot;
#[cfg(feature = "client")]
pub use client::SyncClient;

/// 스냅샷 ZIP 미디어 타입.
pub const SNAPSHOT_MIME: &str = "application/vnd.freedf.snapshot+zip";
