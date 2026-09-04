//! Sync v3 프로토콜 클라이언트 — 앱 ↔ API 서버 연결 지점.
//!
//! `freedf-sync` 크레이트의 타입을 앱이 사용하는 유일한 입구입니다.
//! 연결 설정은 미디어 서버와 동일한 `server.json`(`MediaServerConfig`)을
//! 공유합니다 — 같은 백엔드(`server/backend`)가 `/api/media`와 `/v3/*`를
//! 모두 서빙하므로.
//!
//! 다음 단계(예정, docs/sync-protocol-v3.md):
//! - 저장: `Arc<Snapshot>` + 백그라운드 업로더(코얼레싱, 펜-down 중 발사 금지)
//! - 로딩: `download_snapshot` / `download_if_changed` (ETag 캐시)
//! - 다중 창: `changes(since_revision)` + `Snapshot::apply_changes`

use std::time::Duration;

use crate::server::MediaServerConfig;

/// 앱 코드가 이 모듈에서 프로토콜 타입을 import하도록 재노출.
pub(crate) use freedf_sync::SyncClient;

/// UI 버튼 등 사용자 조작 경로의 타임아웃 (백그라운드 작업은 기본 30s 사용).
const UI_TIMEOUT: Duration = Duration::from_secs(4);

/// 설정이 활성화됐을 때만 프로토콜 클라이언트 생성.
///
/// 미활성/빈 URL이면 `None` — 호출부는 서버 기능을 건너뜁니다.
pub(crate) fn sync_client(config: &MediaServerConfig) -> Option<SyncClient> {
    if !config.enabled {
        return None;
    }
    let base = config.base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return None;
    }
    SyncClient::new_with_timeout(base, &config.api_key, UI_TIMEOUT).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(enabled: bool, url: &str) -> MediaServerConfig {
        MediaServerConfig {
            enabled,
            base_url: url.into(),
            api_key: "test-key".into(),
        }
    }

    #[test]
    fn disabled_config_returns_none() {
        assert!(sync_client(&cfg(false, "http://127.0.0.1:8080")).is_none());
    }

    #[test]
    fn enabled_config_normalizes_base_url() {
        let c = sync_client(&cfg(true, "http://127.0.0.1:8080/")).expect("client");
        assert_eq!(c.base_url(), "http://127.0.0.1:8080");
    }

    #[test]
    fn empty_base_url_returns_none() {
        assert!(sync_client(&cfg(true, "   ")).is_none());
    }

    /// 실서버 대상 e2e — `FREEDF_TEST_SYNC=1`일 때만 실행.
    /// 서버: 127.0.0.1:8080 (FREEDF_API_KEY=test-key), 문서 id는 env로.
    #[test]
    fn sync_v3_against_live_server() {
        if std::env::var("FREEDF_TEST_SYNC").ok().as_deref() != Some("1") {
            return; // 평소엔 no-op
        }
        let doc_id: i64 = std::env::var("FREEDF_TEST_SYNC_DOC")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3);
        let client = sync_client(&cfg(true, "http://127.0.0.1:8080")).expect("client from config");
        client.health().expect("health");
        let rev = client.revision(doc_id).expect("revision").revision;
        let snapshot = client.download_snapshot(doc_id).expect("download").snapshot;
        assert_eq!(snapshot.revision(), Some(rev));
        let _ = client.changes(doc_id, rev).expect("changes");
    }
}
