//! StorageBackend — 앱의 저장소 경계.
//!
//! FreeDF의 모든 영속 데이터(문서/페이지/스트로크/세션/최근 목록/편집 저널/
//! 사전 캐시)는 이 트레이트를 통해서만 접근합니다.
//!
//! - **유일 구현**: `sync_storage::SyncStorage` — Sync v3 API 서버
//!   (docs/sync-protocol-v3.md). 스냅샷 ZIP으로 왕복합니다.
//! - **폴백**: `DisconnectedStorage` — 서버 미설정 시 빈 저장소.
//!
//! 미디어(녹음) 파일 스트리밍은 이 트레이트와 별개로 `server.rs`가 담당합니다.

use freedf_core::history::Edit;
use freedf_core::model::Stroke;
use freedf_core::paper::PagePaper;
use freedf_core::store::AnnotationStore;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;

// ── 공용 행 타입 (서버 GET /v3/documents 항목과 1:1) ─────────────────────

/// 문서 행.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocRow {
    pub id: i64,
    /// "note" | "pdf"
    pub kind: String,
    pub title: String,
    pub origin_path: Option<String>,
    pub page_count: i32,
    pub created_at: i64,
    pub updated_at: i64,
}

impl DocRow {
    pub fn is_note(&self) -> bool {
        self.kind == "note"
    }
}

/// 최근 항목.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecentRow {
    pub kind: String,
    pub doc_id: i64,
    pub title: String,
    pub opened_at: i64,
    pub origin_path: Option<String>,
}

/// 문서 전체 상태 로드 번들 (스냅샷 다운로드의 앱 측 결과).
#[derive(Debug, Default)]
pub struct LoadedBundle {
    pub store: AnnotationStore,
    pub edits: Vec<Edit>,
    pub session: Option<Value>,
}

/// 모든 메서드는 `&self` — 동시성은 백엔드 내부가 책임집니다.
pub trait StorageBackend: Send + Sync {
    // ---------- documents ----------
    fn insert_document(
        &self,
        kind: &str,
        title: &str,
        origin_path: Option<&str>,
        page_count: i32,
        pdf: &[u8],
    ) -> Result<i64, String>;
    fn get_document(&self, id: i64) -> Option<DocRow>;
    fn find_document_by_path(&self, path: &str) -> Option<i64>;
    fn load_pdf(&self, id: i64) -> Option<Vec<u8>>;
    fn update_title(&self, id: i64, title: &str) -> Result<(), String>;
    fn delete_document(&self, id: i64) -> Result<(), String>;
    fn list_notes(&self) -> Vec<DocRow>;

    // ---------- pages (용지 + 북마크) ----------
    fn upsert_page(&self, doc_id: i64, page_index: i32, paper: &PagePaper, bookmarked: bool);

    // ---------- strokes ----------
    /// 전역 시퀀스에서 스트로크 id를 n개 할당 (undo/redo가 같은 id 공간을 쓰게 함).
    fn alloc_stroke_ids(&self, n: usize) -> Vec<i64>;
    fn insert_strokes(&self, doc_id: i64, page_index: i32, strokes: &[Stroke]);
    fn delete_strokes(&self, doc_id: i64, ids: &[i64]);

    /// 문서의 주석/페이지/저널/세션을 **한 번의 왕복**으로 로드.
    fn load_bundle(&self, doc_id: i64) -> LoadedBundle;

    /// 메타 동기화 — 페이지(용지/북마크)·문서 정보·PDF만 반영.
    /// **획은 건드리지 않음** — 획은 로컬 미러에 이미 반영되어 있습니다.
    fn sync_meta(
        &self,
        doc_id: i64,
        page_count: i32,
        entries: &[(i32, PagePaper, bool)],
        pdf: Option<&[u8]>,
    ) -> Result<(), String>;

    /// 페이지 중간 삽입 — from 이상 획의 page_index를 서버에서 이동 (재전송 없음).
    fn shift_strokes(&self, doc_id: i64, from: i32, delta: i32);
    /// 페이지 삭제 — 해당 페이지 획 삭제 + 이후 인덱스 -1 (서버 처리).
    fn delete_page_data(&self, doc_id: i64, page: i32);
    /// 페이지 회전 — 해당 페이지 획의 x/y를 서버에서 변환 (재전송 없음).
    fn rotate_page_data(&self, doc_id: i64, page: i32, clockwise: bool, w: f32, h: f32);
    /// 전체 페이지 회전 — 페이지별 크기를 서버에 보내 내부에서 반복 변환.
    fn rotate_all_data(&self, doc_id: i64, clockwise: bool, sizes: &[[f32; 2]]);

    /// 미처리 write-behind 대기열을 지금 원격에 반영 (기본 true — 큐 없음).
    fn flush_pending(&self) -> bool {
        true
    }

    // ---------- sessions ----------
    fn upsert_session(&self, doc_id: i64, state: &Value);

    // ---------- 전역 앱 상태 ----------
    fn get_app_state(&self, key: &str) -> Option<Value>;
    fn set_app_state(&self, key: &str, value: &Value);

    // ---------- recents ----------
    fn load_recents(&self) -> Vec<RecentRow>;
    fn touch_recent(&self, kind: &str, doc_id: i64, title: &str);

    // ---------- 편집 저널 (영속 히스토리) ----------
    fn log_edit(&self, doc_id: i64, edit: &Edit);
    fn clear_edits(&self, doc_id: i64);

    // ---------- 사전 캐시 ----------
    fn get_word_cache(&self, word: &str) -> Option<Value>;
    fn set_word_cache(&self, word: &str, data: &Value);

    // ---------- 이벤트 로그 ----------
    fn insert_log(&self, epoch_ms: u128, seq: u64, event: &Value);
    /// 이벤트 로그 일괄 기록 (기본은 개별 insert_log).
    fn insert_logs(&self, items: &[(u128, Value)]) {
        for (epoch_ms, event) in items {
            self.insert_log(*epoch_ms, 0, event);
        }
    }

    /// 원격 저장소 연결 확인 — write-behind 플러시 게이트용.
    fn ping(&self) -> bool;

    /// 로컬 캐시 무효화 (강제 새로고침 시 호출). 기본은 no-op.
    fn invalidate_document(&self, _doc_id: i64) {}
}

/// 앱 전체가 공유하는 저장소 핸들.
pub type SharedStorage = Arc<dyn StorageBackend>;

/// Sync v3 API 서버를 저장소로 사용 (server.json 설정 기반).
///
/// 앱의 유일한 연결 경로 — 모든 문서/주석/세션이 이 서버를 통해
/// 스냅샷 ZIP으로 왕복합니다 (docs/sync-protocol-v3.md).
/// 서버가 설정되지 않았거나 URL이 잘못됐으면 폴백(disconnected)을 돌려줍니다.
pub fn from_server_config(config: &crate::server::MediaServerConfig) -> SharedStorage {
    match crate::sync_storage::SyncStorage::new(config) {
        Some(s) => Arc::new(s),
        None => disconnected(),
    }
}

/// 서버 연결 전 폴백 백엔드 — 읽기는 빈 결과, 쓰기는 오류/무시.
/// 첫 실행 대화상자에서 서버 주소를 입력하면 실제 백엔드로 교체됩니다.
#[derive(Default)]
pub struct DisconnectedStorage;

pub fn disconnected() -> SharedStorage {
    Arc::new(DisconnectedStorage)
}

impl StorageBackend for DisconnectedStorage {
    fn insert_document(
        &self,
        _kind: &str,
        _title: &str,
        _origin_path: Option<&str>,
        _page_count: i32,
        _pdf: &[u8],
    ) -> Result<i64, String> {
        Err("Not connected to a server yet — set the server address in the setup dialog.".into())
    }
    fn get_document(&self, _id: i64) -> Option<DocRow> {
        None
    }
    fn find_document_by_path(&self, _path: &str) -> Option<i64> {
        None
    }
    fn load_pdf(&self, _id: i64) -> Option<Vec<u8>> {
        None
    }
    fn update_title(&self, _id: i64, _title: &str) -> Result<(), String> {
        Err("Not connected to a server yet.".into())
    }
    fn delete_document(&self, _id: i64) -> Result<(), String> {
        Err("Not connected to a server yet.".into())
    }
    fn list_notes(&self) -> Vec<DocRow> {
        Vec::new()
    }
    fn upsert_page(&self, _doc_id: i64, _page_index: i32, _paper: &PagePaper, _bookmarked: bool) {}
    fn alloc_stroke_ids(&self, _n: usize) -> Vec<i64> {
        Vec::new()
    }
    fn insert_strokes(&self, _doc_id: i64, _page_index: i32, _strokes: &[Stroke]) {}
    fn delete_strokes(&self, _doc_id: i64, _ids: &[i64]) {}
    fn load_bundle(&self, _doc_id: i64) -> LoadedBundle {
        LoadedBundle::default()
    }
    fn sync_meta(
        &self,
        _doc_id: i64,
        _page_count: i32,
        _entries: &[(i32, PagePaper, bool)],
        _pdf: Option<&[u8]>,
    ) -> Result<(), String> {
        Err("Not connected to a server yet.".into())
    }
    fn shift_strokes(&self, _doc_id: i64, _from: i32, _delta: i32) {}
    fn delete_page_data(&self, _doc_id: i64, _page: i32) {}
    fn rotate_page_data(&self, _doc_id: i64, _page: i32, _clockwise: bool, _w: f32, _h: f32) {}
    fn rotate_all_data(&self, _doc_id: i64, _clockwise: bool, _sizes: &[[f32; 2]]) {}
    fn upsert_session(&self, _doc_id: i64, _state: &Value) {}
    fn get_app_state(&self, _key: &str) -> Option<Value> {
        None
    }
    fn set_app_state(&self, _key: &str, _value: &Value) {}
    fn load_recents(&self) -> Vec<RecentRow> {
        Vec::new()
    }
    fn touch_recent(&self, _kind: &str, _doc_id: i64, _title: &str) {}
    fn log_edit(&self, _doc_id: i64, _edit: &Edit) {}
    fn clear_edits(&self, _doc_id: i64) {}
    fn get_word_cache(&self, _word: &str) -> Option<Value> {
        None
    }
    fn set_word_cache(&self, _word: &str, _data: &Value) {}
    fn insert_log(&self, _epoch_ms: u128, _seq: u64, _event: &Value) {}
    fn ping(&self) -> bool {
        false
    }
}

/// 앱 데이터 폴더 (%LOCALAPPDATA%/FreeDF 또는 ~/.local/share/freedf).
pub(crate) fn app_data_dir() -> PathBuf {
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(local).join("FreeDF");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".local").join("share").join("freedf");
    }
    PathBuf::new()
}

// ---------- 캐싱 백엔드(레거시, 제거됨) ----------
//
// (PostgreSQL + 로컬 캐시/write-behind 구현은 Sync v3 서버
// `sync_storage::SyncStorage`로 대체되었습니다.)

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_storage_is_object_safe_and_thread_safe() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SharedStorage>();
    }

    #[test]
    fn disconnected_backend_is_safe_fallback() {
        let db = disconnected();
        assert!(!db.ping());
        assert!(db.list_notes().is_empty());
        assert!(db.load_recents().is_empty());
        assert!(db.get_document(1).is_none());
        assert!(db.insert_document("note", "x", None, 1, b"x").is_err());
        // 쓰기는 크래시 없이 무시됩니다.
        db.insert_strokes(1, 0, &[]);
        db.delete_strokes(1, &[1]);
    }

    #[test]
    fn unconfigured_server_falls_back_to_disconnected() {
        let db = from_server_config(&crate::server::MediaServerConfig::default());
        assert!(!db.ping());
    }
}
