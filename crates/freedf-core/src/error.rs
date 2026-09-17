//! freedf-core의 **에러 바운더리**.
//!
//! 3계층 정책 — 어느 층에서 에러를 다루는지가 코드에 그대로 보이게 한다:
//!
//! 1. **모듈 경계** — 각 모듈은 자기 도메인의 세밀한 에러
//!    ([`crate::notes::NoteError`], [`crate::input_commands::MalformedStream`] 등)를
//!    thiserror로 정의하고 그대로 반환합니다. 호출자는 변형(variant)을 매치해
//!    복구 전략을 고를 수 있습니다.
//! 2. **크레이트 경계(이 모듈)** — [`Error`]가 모듈 에러를 `#[from]`으로 수렴하고,
//!    이질적 원인(io, JSON)도 여기서 표준화합니다. 크레이트 밖으로 나가는
//!    fallible 공개 API는 [`Result`](crate::Result)를 사용합니다.
//!    (`Logger::to_file`, `AnnotationStore::from_json`, `check_well_formed` 등.)
//! 3. **앱 경계** — 크레이트 밖(`freedf`, `freedf-gui`, 통합 테스트)은
//!    `anyhow::Result`로 수렴합니다. [`Error`]가 `std::error::Error`를 구현하므로
//!    `?` 한 번으로 변환되고, `anyhow::Error::downcast_ref`로 변형 복원이 가능해
//!    정보 손실이 없습니다. 검증은 `tests/error_boundary_tests.rs`가 담당합니다.
//!
//! 라이브러리 내부에서 anyhow를 쓰지 않는 이유: 타입화된 에러는 호출자가
//! 프로그램적으로 복구할 수 있어야 하기 때문입니다(Busy → 다음 프레임 재시도 등).

use crate::input_commands::MalformedStream;
use crate::notes::NoteError;

/// 크레이트 공통 에러 — 모듈 에러의 수렴점(에러 바운더리 2계층).
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// 노트 CRUD 도메인 에러 — [`crate::notes`].
    /// `#[from]`이 `#[source]`를 의미하므로 원인 체인에서 `NoteError` 복원이 가능.
    #[error("{0}")]
    Note(#[from] NoteError),
    /// 커맨드 스트림 불변식 위반 — [`crate::input_commands`]의 잘-형성 검사 실패.
    #[error("{0}")]
    MalformedStream(#[from] MalformedStream),
    /// JSON (역)직렬화 오류 — [`crate::store`] 등.
    #[error("JSON (역)직렬화 오류: {0}")]
    Json(#[from] serde_json::Error),
    /// 파일/장치 IO 오류 — [`crate::logging`], [`crate::pen_input`] 등.
    #[error("IO 오류: {0}")]
    Io(#[from] std::io::Error),
}

/// 크레이트 공통 Result 별칭 — fallible 공개 API의 표준 반환형.
pub type Result<T, E = Error> = std::result::Result<T, E>;
