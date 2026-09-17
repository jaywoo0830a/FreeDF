//! freedf-canvas의 **에러 바운더리**.
//!
//! 정책은 `freedf-core/src/error.rs`의 3계층 정책과 같습니다.
//!
//! 1. **모듈 경계** — 도메인 에러([`crate::bake::BakeError`] 등)는 thiserror로
//!    정의해 그대로 반환합니다. `BakeService`처럼 논블로킹 계약(즉시 `Busy`)이
//!    있는 API는 굳이 크레이트 공통 타입으로 감싸지 않고 원형을 유지합니다.
//! 2. **크레이트 경계(이 모듈)** — [`Error`]가 모듈 에러를 `#[from]`으로 수렴해
//!    크레이트 전체를 아우르는 fallible API가 늘어날 착지점을 제공합니다.
//! 3. **앱 경계** — 크레이트 밖(`freedf` 앱, 통합 테스트)은 `anyhow::Result`로
//!    수렴합니다. 검증은 `tests/error_boundary_tests.rs`가 담당합니다.

use crate::bake::BakeError;

/// 크레이트 공통 에러 — 모듈 에러의 수렴점(에러 바운더리 2계층).
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// 굽기 파이프라인 오류 — [`crate::bake`] (`Busy`는 재시도 가능).
    /// `#[from]`이 `#[source]`를 의미하므로 원인 체인에서 `BakeError` 복원이 가능.
    #[error("{0}")]
    Bake(#[from] BakeError),
}

/// 크레이트 공통 Result 별칭 — fallible 공개 API의 표준 반환형.
pub type Result<T, E = Error> = std::result::Result<T, E>;
