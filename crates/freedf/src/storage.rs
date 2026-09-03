//! StorageBackend — 앱의 저장소 경계 (로드맵 ②).
//!
//! FreeDF의 모든 영속 데이터(문서/페이지/스트로크/세션/최근 목록/편집 저널/
//! 사전 캐시/이벤트 로그)는 이 트레이트를 통해서만 접근합니다.
//!
//! - **현재 구현**: PostgreSQL (`db::Db`) — SQL은 `db.rs` 안에만 존재
//!   (단일 경계 원칙 유지)
//! - **확장**: 로컬 파일 백엔드, 자체 서버 API 백엔드 등을 추가할 때
//!   `impl StorageBackend for …` 하나만 작성하고 `from_env` 팩토리에 등록하면
//!   앱 코드(UI/캔버스/액션)는 한 줄도 바뀌지 않습니다.
//!
//! 선택은 **런타임**에 `FREEDF_STORAGE` 환경 변수로 결정됩니다
//! (기본 `postgres`). 미디어(녹음) 파일 스트리밍은 이 트레이트와 별개로
//! `server.rs`(HTTP API)가 담당합니다 — 리소스 API와 DB 인터페이스 분리.

pub use crate::db::{DocRow, RecentRow};

use freedf_core::history::Edit;
use freedf_core::model::Stroke;
use freedf_core::paper::PagePaper;
use freedf_core::store::AnnotationStore;
use serde_json::Value;
use std::sync::Arc;

/// 모든 메서드는 `&self` — 동시성은 백엔드 내부가 책임집니다
/// (Postgres 구현은 내부 Mutex로 직렬화).
pub trait StorageBackend: Send + Sync {
    // ---------- documents ----------
    fn insert_document(
        &self,
        kind: &str,
        title: &str,
        origin_path: Option<&str>,
        pdf: &[u8],
    ) -> Result<i64, String>;
    fn get_document(&self, id: i64) -> Option<DocRow>;
    fn find_document_by_path(&self, path: &str) -> Option<i64>;
    fn load_pdf(&self, id: i64) -> Option<Vec<u8>>;
    fn save_pdf(&self, id: i64, bytes: &[u8]) -> Result<(), String>;
    fn update_title(&self, id: i64, title: &str) -> Result<(), String>;
    fn update_page_count(&self, id: i64, page_count: i32);
    fn delete_document(&self, id: i64) -> Result<(), String>;
    fn list_notes(&self) -> Vec<DocRow>;

    // ---------- pages (용지 + 북마크) ----------
    fn upsert_page(&self, doc_id: i64, page_index: i32, paper: &PagePaper, bookmarked: bool);
    fn replace_pages(&self, doc_id: i64, entries: &[(i32, PagePaper, bool)]);

    // ---------- strokes ----------
    /// 전역 시퀀스에서 스트로크 id를 n개 할당 (undo/redo가 DB와 같은 id를 쓰게 함).
    fn alloc_stroke_ids(&self, n: usize) -> Vec<i64>;
    fn insert_strokes(&self, doc_id: i64, page_index: i32, strokes: &[Stroke]);
    fn delete_strokes(&self, doc_id: i64, ids: &[i64]);
    fn resync_strokes(&self, doc_id: i64, store: &AnnotationStore);
    fn load_store(&self, doc_id: i64) -> AnnotationStore;

    // ---------- sessions ----------
    fn load_session(&self, doc_id: i64) -> Option<Value>;
    fn upsert_session(&self, doc_id: i64, state: &Value);

    // ---------- 전역 앱 상태 ----------
    fn get_app_state(&self, key: &str) -> Option<Value>;
    fn set_app_state(&self, key: &str, value: &Value);

    // ---------- recents ----------
    fn load_recents(&self) -> Vec<RecentRow>;
    fn touch_recent(&self, kind: &str, doc_id: i64, title: &str);

    // ---------- 편집 저널 (영속 히스토리) ----------
    fn log_edit(&self, doc_id: i64, edit: &Edit);
    fn load_edits(&self, doc_id: i64) -> Vec<Edit>;
    fn clear_edits(&self, doc_id: i64);

    // ---------- 사전 캐시 ----------
    fn get_word_cache(&self, word: &str) -> Option<Value>;
    fn set_word_cache(&self, word: &str, data: &Value);

    // ---------- 이벤트 로그 ----------
    fn insert_log(&self, epoch_ms: u128, seq: u64, event: &Value);
}

/// 앱 전체가 공유하는 저장소 핸들.
pub type SharedStorage = Arc<dyn StorageBackend>;

/// 런타임 백엔드 선택: `FREEDF_STORAGE` 환경 변수 (기본 `postgres`).
///
/// 새 백엔드를 붙일 때는 여기에 한 줄 추가하면 됩니다 —
/// 예: `"local-file" => Arc::new(local::FileBackend::open(...)?)`.
pub fn from_env(db_url: &str) -> Result<SharedStorage, String> {
    let kind = std::env::var("FREEDF_STORAGE").unwrap_or_else(|_| "postgres".to_string());
    match kind.as_str() {
        "postgres" => Ok(Arc::new(crate::db::Db::connect(db_url)?)),
        other => Err(format!(
            "Unknown FREEDF_STORAGE backend: {other} (supported: postgres)"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_backend_fails_before_connecting() {
        // 별도 프로세스가 없으니 env를 직접 바꿔 테스트 (이 테스트 한정).
        std::env::set_var("FREEDF_STORAGE", "no-such-backend");
        let err = match from_env("postgres://invalid:1/x") {
            Ok(_) => String::new(),
            Err(e) => e,
        };
        assert!(err.contains("no-such-backend"), "{err}");
        std::env::remove_var("FREEDF_STORAGE");
    }

    #[test]
    fn shared_storage_is_object_safe_and_thread_safe() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SharedStorage>();
    }
}
