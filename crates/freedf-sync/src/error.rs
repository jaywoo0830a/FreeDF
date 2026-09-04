//! 공용 오류 타입.

use std::fmt;

/// Sync v3 관련 모든 오류.
#[derive(Debug)]
pub enum SyncError {
    /// 네트워크/전송 오류.
    Transport(String),
    /// 서버가 JSON 오류 본문(`{code, message}`)을 반환.
    Api {
        status: u16,
        code: String,
        message: String,
    },
    /// JSON 본문이 없는 상태 응답.
    Http(u16, String),
    /// ZIP/JSON 파싱·직렬화 오류.
    Decode(String),
    /// 다이제스트 형식/검증 오류.
    Digest(String),
}

impl fmt::Display for SyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SyncError::Transport(e) => write!(f, "transport: {e}"),
            SyncError::Api {
                status,
                code,
                message,
            } => write!(f, "api {status} {code}: {message}"),
            SyncError::Http(status, body) => write!(f, "http {status}: {body}"),
            SyncError::Decode(e) => write!(f, "decode: {e}"),
            SyncError::Digest(e) => write!(f, "digest: {e}"),
        }
    }
}

impl std::error::Error for SyncError {}

impl From<std::io::Error> for SyncError {
    fn from(e: std::io::Error) -> Self {
        SyncError::Decode(format!("io: {e}"))
    }
}

impl From<zip::result::ZipError> for SyncError {
    fn from(e: zip::result::ZipError) -> Self {
        SyncError::Decode(format!("zip: {e}"))
    }
}

impl From<serde_json::Error> for SyncError {
    fn from(e: serde_json::Error) -> Self {
        SyncError::Decode(format!("json: {e}"))
    }
}

pub type Result<T> = std::result::Result<T, SyncError>;
