//! 에러 바운더리 통합 테스트.
//!
//! `src/error.rs` 모듈 문서의 정책 계약을 검증합니다:
//! 1. **모듈 → 크레이트**: `BakeError`가 `#[from]`으로 [`Error`](freedf_canvas::Error)로
//!    수렴하는가
//! 2. **크레이트 → 앱(anyhow)**: `?` 한 번으로 흘러가고, `downcast_ref`로
//!    변형 복원이 가능한가 — 특히 `Busy`(재시도 가능) 판별이 살아있는가

use freedf_canvas::bake::BakeError;
use freedf_canvas::{Error, Result as CanvasResult};

/// 굽기 결과를 기다리는 전형 — 도메인 에러가 `?`로 크레이트 경계까지 변환됨.
fn require_completed(result: Result<(), BakeError>) -> CanvasResult<()> {
    Ok(result?)
}

#[test]
fn module_error_converges_into_crate_error() {
    let err = require_completed(Err(BakeError::Busy)).unwrap_err();
    assert!(matches!(err, Error::Bake(BakeError::Busy)));
    assert!(err.to_string().contains("진행 중"), "{}", err);
}

#[test]
fn crate_error_flows_into_anyhow_boundary_without_loss() {
    // 앱(freedf)의 패턴: Busy → 다음 프레임 재시도, WorkerStopped → 치명적.
    let busy: anyhow::Error = require_completed(Err(BakeError::Busy)).unwrap_err().into();
    // 최상위는 크레이트 경계 타입(freedf_canvas::Error).
    let outer = busy.downcast_ref::<Error>().expect("Error 복원");
    assert!(matches!(outer, Error::Bake(BakeError::Busy)));
    // 원인 체인(source)을 따라가면 도메인 변형도 복원된다.
    let bake = busy
        .chain()
        .filter_map(|e| e.downcast_ref::<BakeError>())
        .next()
        .expect("BakeError 복원");
    assert_eq!(bake, &BakeError::Busy, "Busy 판별이 살아있어야 한다");

    let stopped: anyhow::Error = Error::from(BakeError::WorkerStopped).into();
    assert!(matches!(
        stopped.downcast_ref::<Error>(),
        Some(Error::Bake(BakeError::WorkerStopped))
    ));
    assert!(stopped.to_string().contains("종료"), "{}", stopped);
}
