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

pub use crate::db::{DocRow, LoadedBundle, RecentRow};

use crate::cache::{apply_op_to_store, CacheInner, LocalCache, PendingOp};
use freedf_core::history::Edit;
use freedf_core::model::Stroke;
use freedf_core::paper::PagePaper;
use freedf_core::store::AnnotationStore;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// 모든 메서드는 `&self` — 동시성은 백엔드 내부가 책임집니다
/// (Postgres 구현은 내부 Mutex로 직렬화).
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
    /// 전역 시퀀스에서 스트로크 id를 n개 할당 (undo/redo가 DB와 같은 id를 쓰게 함).
    fn alloc_stroke_ids(&self, n: usize) -> Vec<i64>;
    fn insert_strokes(&self, doc_id: i64, page_index: i32, strokes: &[Stroke]);
    fn delete_strokes(&self, doc_id: i64, ids: &[i64]);

    /// 문서의 주석/페이지/저널/세션을 **한 번의 왕복**으로 로드 (스키마 0007 함수).
    fn load_bundle(&self, doc_id: i64) -> LoadedBundle;

    /// 메타 동기화(0008) — 페이지(용지/북마크)·문서 정보·PDF만 반영.
    /// **획은 건드리지 않음** — 획은 write-behind 대기열이 이미 증분 반영합니다.
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
    /// 편집 저널 일괄 기록 (기본은 개별 log_edit — Db는 배치 오버라이드).
    fn log_edits(&self, doc_id: i64, edits: &[Edit]) {
        for e in edits {
            self.log_edit(doc_id, e);
        }
    }
    fn clear_edits(&self, doc_id: i64);

    // ---------- 사전 캐시 ----------
    fn get_word_cache(&self, word: &str) -> Option<Value>;
    fn set_word_cache(&self, word: &str, data: &Value);

    // ---------- 이벤트 로그 ----------
    fn insert_log(&self, epoch_ms: u128, seq: u64, event: &Value);
    /// 이벤트 로그 일괄 기록 (기본은 개별 insert_log — Db는 배치 오버라이드).
    fn insert_logs(&self, items: &[(u128, Value)]) {
        for (epoch_ms, event) in items {
            self.insert_log(*epoch_ms, 0, event);
        }
    }

    /// 원격 저장소 연결 확인 — write-behind 플러시 게이트용.
    fn ping(&self) -> bool;

    /// 로컬 캐시 무효화 (강제 새로고침 시 호출). 기본은 no-op.
    fn invalidate_document(&self, _doc_id: i64) {}

    /// DB 인스턴스 식별자 — 캐시 오염 방지용 (기본 None = 검사 생략).
    fn identity(&self) -> Option<String> {
        None
    }
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
        "postgres" => connect(db_url),
        other => Err(format!(
            "Unknown FREEDF_STORAGE backend: {other} (supported: postgres)"
        )),
    }
}

/// PostgreSQL + (기본) 로컬 캐시/write-behind 백엔드 연결.
/// `from_env`와 첫 실행 연결 대화상자 둘 다 이 경로를 사용합니다.
/// 로컬 캐시는 `FREEDF_STORAGE_CACHE=0`이면 끕니다.
pub fn connect(db_url: &str) -> Result<SharedStorage, String> {
    let db = Arc::new(crate::db::Db::connect(db_url)?);
    let cache_off = std::env::var("FREEDF_STORAGE_CACHE").as_deref() == Ok("0");
    if cache_off {
        return Ok(db);
    }
    let cached = Arc::new(CachingBackend::new(
        db,
        LocalCache::new(app_data_dir().join("cache")),
    ));
    cached.start_flusher();
    Ok(cached)
}

// ---------- DB 연결 정보 영속화 (connection.json) ----------

fn connection_path() -> PathBuf {
    app_data_dir().join("connection.json")
}

/// 마지막으로 연결에 성공한 DB URL (없으면 None).
pub fn load_saved_connection() -> Option<String> {
    std::fs::read_to_string(connection_path())
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("database_url")?.as_str().map(String::from))
}

/// 연결 성공 시 URL을 저장 — 다음 실행에서 자동으로 사용합니다.
pub fn save_connection(db_url: &str) {
    if let Some(parent) = connection_path().parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let json = serde_json::json!({ "database_url": db_url });
    let _ = std::fs::write(
        connection_path(),
        serde_json::to_string_pretty(&json).unwrap_or_default(),
    );
}

/// DB 연결 전 폴백 백엔드 — 읽기는 빈 결과, 쓰기는 오류/무시.
/// 첫 실행 대화상자에서 실제 백엔드로 교체됩니다.
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
        Err("Not connected to the database yet — set the DB URL in the setup dialog.".into())
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
        Err("Not connected to the database yet.".into())
    }
    fn delete_document(&self, _id: i64) -> Result<(), String> {
        Err("Not connected to the database yet.".into())
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
        Err("Not connected to the database yet.".into())
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

// ---------- 캐싱 백엔드 (로드맵 ④) ----------

/// 원격 저장소 앞에 로컬 디스크 캐시 + write-behind를 끼운 래퍼.
///
/// - **읽기**: 무거운 데이터(PDF 본문/주석 스토어)와 스냅샷(세션/편집 저널/
///   노트·최근 목록)은 캐시 우선 — 해외 DB 왕복 없이 즉시 열림.
/// - **쓰기**: 스트로크 삽입/삭제는 캐시에 즉시 병합 + `pending.jsonl`에
///   영속 기록, 원격 반영은 백그라운드 스레드(`start_flusher`)가 1초마다
///   순서대로 플러시 (연결이 끊기면 보류 — 오프라인 필기 보존).
/// - **불변식**: 캐시 = 원격 상태 + 미처리 대기열. 대기열이 남아 있는
///   문서는 캐시하지 않고 원격+대기열 병합본을 반환해 이중 적용을 막습니다.
pub struct CachingBackend {
    remote: Arc<dyn StorageBackend>,
    inner: Arc<Mutex<CacheInner>>,
}

/// 작업의 **원격 반영** 테이블 — flush_pending이 순서대로 실행합니다.
/// (apply_local과 짝 — 새 액션은 이 두 곳에 arm을 추가)
pub fn apply_remote(op: &PendingOp, remote: &dyn StorageBackend) {
    match op {
        PendingOp::InsertStrokes {
            doc_id,
            page_index,
            strokes,
        } => remote.insert_strokes(*doc_id, *page_index, strokes),
        PendingOp::DeleteStrokes { doc_id, ids } => remote.delete_strokes(*doc_id, ids),
        PendingOp::LogEdit { doc_id, edit } => remote.log_edit(*doc_id, edit),
        PendingOp::LogEdits { doc_id, edits } => remote.log_edits(*doc_id, edits),
        PendingOp::UpsertSession { doc_id, state } => remote.upsert_session(*doc_id, state),
        PendingOp::SetAppState { key, value } => remote.set_app_state(key, value),
        PendingOp::UpsertPage {
            doc_id,
            page_index,
            paper,
            bookmarked,
        } => remote.upsert_page(*doc_id, *page_index, paper, *bookmarked),
        PendingOp::TouchRecent {
            kind,
            doc_id,
            title,
        } => remote.touch_recent(kind, *doc_id, title),
    }
}

impl CachingBackend {
    /// 래퍼 생성 + 재시작 후 남아 있던 미처리 대기열 복원.
    /// (플러시 스레드는 `start_flusher`로 명시적으로 시작 — 테스트 제어용)
    pub fn new(remote: Arc<dyn StorageBackend>, cache: LocalCache) -> Self {
        let identity = remote.identity();
        Self::new_with_identity(remote, cache, identity)
    }

    /// DB 식별자를 명시적으로 주입하는 생성자 (테스트 및 new() 공용 경로).
    pub fn new_with_identity(
        remote: Arc<dyn StorageBackend>,
        cache: LocalCache,
        identity: Option<String>,
    ) -> Self {
        // DB가 초기화/교체되어 식별자가 달라지면 캐시 전체 폐기 —
        // 문서 id 재사용으로 옛 스토어/대기열/목록이 새 문서에 섞이는
        // 오염을 방지합니다.
        if let Some(id) = &identity {
            if cache.get_identity().as_deref() != Some(id.as_str()) {
                cache.clear_all();
                cache.set_identity(id);
            }
        }
        Self {
            remote,
            inner: Arc::new(Mutex::new(CacheInner::new(cache))),
        }
    }

    /// 백그라운드 플러시 스레드 시작 (from_env에서 호출).
    pub fn start_flusher(self: &Arc<Self>) {
        let me = Arc::clone(self);
        std::thread::spawn(move || loop {
            me.persist_dirty();
            me.flush_pending();
            std::thread::sleep(std::time::Duration::from_millis(1000));
        });
    }

    /// 더러워진 메모리 캐시(스토어/저널)를 디스크에 기록합니다.
    /// 직렬화는 **락 밖**에서 — UI 스레드가 파일 쓰기 동안 대기하지 않습니다.
    /// (스냅샷 이후 새로 쌓인 변경은 dirty 플래그가 남아 다음 주기에 기록)
    fn persist_dirty(&self) {
        let (stores, edits) = {
            let mut g = self.inner.lock().expect("cache mutex poisoned");
            let stores: Vec<(i64, AnnotationStore)> = g
                .dirty_stores
                .iter()
                .filter_map(|d| g.stores.get(d).map(|s| (*d, s.clone())))
                .collect();
            let edits: Vec<(i64, Vec<Edit>)> = g
                .dirty_edits
                .iter()
                .filter_map(|d| g.edits.get(d).map(|e| (*d, e.clone())))
                .collect();
            g.dirty_stores.clear();
            g.dirty_edits.clear();
            (stores, edits)
        };
        let g = self.inner.lock().expect("cache mutex poisoned");
        for (doc, store) in stores {
            g.cache.put_store(doc, &store);
        }
        for (doc, edits) in edits {
            g.cache.put_edits(doc, &edits);
        }
    }

    /// 대기열의 연속된 동일 작업을 병합 — 플러시 왕복을 획 단위에서 배치 단위로.
    /// (InsertStrokes는 같은 문서·페이지가 연속이면 합치고, DeleteStrokes는
    /// 같은 문서면 id를 이어 붙입니다. 순서는 그대로 유지됩니다.)
    fn coalesce_ops(ops: Vec<PendingOp>) -> Vec<PendingOp> {
        let mut out: Vec<PendingOp> = Vec::with_capacity(ops.len());
        for op in ops {
            // LogEdit + LogEdit → LogEdits 승격 (마지막 원소 교체).
            if let (
                Some(PendingOp::LogEdit { doc_id: d1, edit: e1 }),
                PendingOp::LogEdit { doc_id: d2, edit: e2 },
            ) = (out.last(), &op)
            {
                if d1 == d2 {
                    let doc = *d1;
                    let edits = vec![e1.clone(), e2.clone()];
                    *out.last_mut().unwrap() = PendingOp::LogEdits { doc_id: doc, edits };
                    continue;
                }
            }
            let merged = match (out.last_mut(), &op) {
                (
                    Some(PendingOp::InsertStrokes {
                        doc_id,
                        page_index,
                        strokes,
                    }),
                    PendingOp::InsertStrokes {
                        doc_id: d2,
                        page_index: p2,
                        strokes: s2,
                    },
                ) if doc_id == d2 && page_index == p2 => {
                    strokes.extend(s2.iter().cloned());
                    true
                }
                (
                    Some(PendingOp::DeleteStrokes { doc_id, ids }),
                    PendingOp::DeleteStrokes { doc_id: d2, ids: ids2 },
                ) if doc_id == d2 => {
                    ids.extend_from_slice(ids2);
                    true
                }
                (
                    Some(PendingOp::LogEdits { doc_id, edits }),
                    PendingOp::LogEdit { doc_id: d2, edit },
                ) if doc_id == d2 => {
                    edits.push(edit.clone());
                    true
                }
                _ => false,
            };
            if !merged {
                out.push(op);
            }
        }
        out
    }

    /// 대기열을 원격에 **순서대로** 플러시. 전부 반영되면 true,
    /// 원격 연결이 없으면 대기열을 그대로 보류하고 false.
    pub fn flush_pending(&self) -> bool {
        // 디스크 캐시도 함께 반영 (테스트/수동 플러시 경로에서도 동일 상태 유지).
        self.persist_dirty();
        loop {
            let ops = {
                let mut g = self.inner.lock().expect("cache mutex poisoned");
                std::mem::take(&mut g.pending)
            };
            if ops.is_empty() {
                // 대기열이 완전히 비면 영속 파일도 정리합니다.
                self.inner
                    .lock()
                    .expect("cache mutex poisoned")
                    .cache
                    .clear_pending();
                return true;
            }
            if !self.remote.ping() {
                // 연결 없음 — 대기열 복원 (그 사이 새로 쌓인 작업을 뒤에 이어 붙임).
                let mut g = self.inner.lock().expect("cache mutex poisoned");
                let mut all = ops;
                all.extend(std::mem::take(&mut g.pending));
                g.pending = all;
                return false;
            }
            for op in &Self::coalesce_ops(ops) {
                apply_remote(op, self.remote.as_ref());
            }
            // 이 배치는 반영 완료 — 동시에 새로 쌓인 작업은 다음 루프에서.
        }
    }
}

impl StorageBackend for CachingBackend {
    fn insert_document(
        &self,
        kind: &str,
        title: &str,
        origin_path: Option<&str>,
        page_count: i32,
        pdf: &[u8],
    ) -> Result<i64, String> {
        let id = self
            .remote
            .insert_document(kind, title, origin_path, page_count, pdf)?;
        self.inner.lock().expect("cache mutex poisoned").cache.invalidate_notes();
        Ok(id)
    }

    fn get_document(&self, id: i64) -> Option<DocRow> {
        self.remote.get_document(id)
    }

    fn find_document_by_path(&self, path: &str) -> Option<i64> {
        self.remote.find_document_by_path(path)
    }

    fn load_pdf(&self, id: i64) -> Option<Vec<u8>> {
        {
            let g = self.inner.lock().expect("cache mutex poisoned");
            if let Some(bytes) = g.cache.get_pdf(id) {
                return Some(bytes);
            }
        }
        let bytes = self.remote.load_pdf(id)?;
        self.inner
            .lock()
            .expect("cache mutex poisoned")
            .cache
            .put_pdf(id, &bytes);
        Some(bytes)
    }

    fn update_title(&self, id: i64, title: &str) -> Result<(), String> {
        self.remote.update_title(id, title)?;
        self.inner.lock().expect("cache mutex poisoned").cache.invalidate_notes();
        Ok(())
    }

    fn delete_document(&self, id: i64) -> Result<(), String> {
        self.remote.delete_document(id)?;
        let mut g = self.inner.lock().expect("cache mutex poisoned");
        g.cache.invalidate_notes();
        g.cache.invalidate_store(id);
        g.stores.remove(&id);
        g.dirty_stores.remove(&id);
        g.edits.remove(&id);
        g.dirty_edits.remove(&id);
        Ok(())
    }

    fn list_notes(&self) -> Vec<DocRow> {
        {
            let g = self.inner.lock().expect("cache mutex poisoned");
            if let Some(notes) = g.cache.get_notes() {
                return notes;
            }
        }
        let notes = self.remote.list_notes();
        self.inner
            .lock()
            .expect("cache mutex poisoned")
            .cache
            .put_notes(&notes);
        notes
    }

    fn upsert_page(&self, doc_id: i64, page_index: i32, paper: &PagePaper, bookmarked: bool) {
        // write-behind — 북마크 토글/용지 적용이 페이지 수만큼 왕복하지 않게.
        let mut g = self.inner.lock().expect("cache mutex poisoned");
        g.enqueue(PendingOp::UpsertPage {
            doc_id,
            page_index,
            paper: *paper,
            bookmarked,
        });
    }

    fn alloc_stroke_ids(&self, n: usize) -> Vec<i64> {
        self.remote.alloc_stroke_ids(n)
    }

    fn insert_strokes(&self, doc_id: i64, page_index: i32, strokes: &[Stroke]) {
        let mut g = self.inner.lock().expect("cache mutex poisoned");
        g.enqueue(PendingOp::InsertStrokes {
            doc_id,
            page_index,
            strokes: strokes.to_vec(),
        });
    }

    fn delete_strokes(&self, doc_id: i64, ids: &[i64]) {
        let mut g = self.inner.lock().expect("cache mutex poisoned");
        g.enqueue(PendingOp::DeleteStrokes {
            doc_id,
            ids: ids.to_vec(),
        });
    }

    fn sync_meta(
        &self,
        doc_id: i64,
        page_count: i32,
        entries: &[(i32, PagePaper, bool)],
        pdf: Option<&[u8]>,
    ) -> Result<(), String> {
        self.remote
            .sync_meta(doc_id, page_count, entries, pdf)?;
        let g = self.inner.lock().expect("cache mutex poisoned");
        // 페이지 수가 바뀌면 노트 목록 캐시 무효화. 스토어(획)는 불변.
        g.cache.invalidate_notes();
        if let Some(bytes) = pdf {
            g.cache.put_pdf(doc_id, bytes);
        }
        Ok(())
    }

    fn shift_strokes(&self, doc_id: i64, from: i32, delta: i32) {
        self.remote.shift_strokes(doc_id, from, delta);
    }

    fn delete_page_data(&self, doc_id: i64, page: i32) {
        self.remote.delete_page_data(doc_id, page);
    }

    fn rotate_page_data(&self, doc_id: i64, page: i32, clockwise: bool, w: f32, h: f32) {
        self.remote.rotate_page_data(doc_id, page, clockwise, w, h);
    }

    fn rotate_all_data(&self, doc_id: i64, clockwise: bool, sizes: &[[f32; 2]]) {
        self.remote.rotate_all_data(doc_id, clockwise, sizes);
    }

    fn flush_pending(&self) -> bool {
        Self::flush_pending(self)
    }

    fn load_bundle(&self, doc_id: i64) -> LoadedBundle {
        {
            let mut g = self.inner.lock().expect("cache mutex poisoned");
            let has_pending = g.pending.iter().any(|o| o.doc_id() == Some(doc_id));
            if !has_pending {
                // 캐시 히트 — 대기열 없으면 메모리/디스크 스냅샷이 곧 최신 상태.
                let store = g
                    .stores
                    .get(&doc_id)
                    .cloned()
                    .or_else(|| g.cache.get_store(doc_id));
                let edits = g
                    .edits
                    .get(&doc_id)
                    .cloned()
                    .or_else(|| g.cache.get_edits(doc_id));
                if let (Some(store), Some(edits)) = (store, edits) {
                    g.stores.insert(doc_id, store.clone());
                    g.edits.insert(doc_id, edits.clone());
                    return LoadedBundle {
                        store,
                        edits,
                        session: g.cache.get_session(doc_id),
                    };
                }
            }
        }
        // 캐시 미스 — 원격 1왕복 + 대기열 병합.
        let mut bundle = self.remote.load_bundle(doc_id);
        let pending: Vec<PendingOp> = {
            let g = self.inner.lock().expect("cache mutex poisoned");
            g.pending
                .iter()
                .filter(|o| o.doc_id() == Some(doc_id))
                .cloned()
                .collect()
        };
        if pending.is_empty() {
            let mut g = self.inner.lock().expect("cache mutex poisoned");
            g.stores.insert(doc_id, bundle.store.clone());
            g.dirty_stores.insert(doc_id);
            g.edits.insert(doc_id, bundle.edits.clone());
            g.dirty_edits.insert(doc_id);
            if let Some(s) = &bundle.session {
                g.cache.put_session(doc_id, s);
            }
        } else {
            for op in &pending {
                apply_op_to_store(&mut bundle.store, op);
                match op {
                    PendingOp::LogEdit { edit, .. } => bundle.edits.push(edit.clone()),
                    PendingOp::LogEdits { edits, .. } => {
                        bundle.edits.extend(edits.iter().cloned())
                    }
                    PendingOp::UpsertSession { state, .. } => {
                        bundle.session = Some(state.clone())
                    }
                    _ => {}
                }
            }
        }
        bundle
    }

    fn upsert_session(&self, doc_id: i64, state: &Value) {
        // write-behind — 도구/색 변경 등 빈번한 쓰기가 원격 왕복하지 않게.
        let mut g = self.inner.lock().expect("cache mutex poisoned");
        g.enqueue(PendingOp::UpsertSession {
            doc_id,
            state: state.clone(),
        });
    }

    fn get_app_state(&self, key: &str) -> Option<Value> {
        self.remote.get_app_state(key)
    }

    fn set_app_state(&self, key: &str, value: &Value) {
        // write-behind — 전역 기본 세션 저장도 마찬가지로 큐에 쌓습니다.
        let mut g = self.inner.lock().expect("cache mutex poisoned");
        g.enqueue(PendingOp::SetAppState {
            key: key.to_string(),
            value: value.clone(),
        });
    }

    fn load_recents(&self) -> Vec<RecentRow> {
        {
            let g = self.inner.lock().expect("cache mutex poisoned");
            if let Some(recents) = g.cache.get_recents() {
                return recents;
            }
        }
        let recents = self.remote.load_recents();
        self.inner
            .lock()
            .expect("cache mutex poisoned")
            .cache
            .put_recents(&recents);
        recents
    }

    fn touch_recent(&self, kind: &str, doc_id: i64, title: &str) {
        // write-behind — 문서 열기 경로의 왕복 하나를 더 제거.
        let mut g = self.inner.lock().expect("cache mutex poisoned");
        g.enqueue(PendingOp::TouchRecent {
            kind: kind.to_string(),
            doc_id,
            title: title.to_string(),
        });
    }

    fn log_edit(&self, doc_id: i64, edit: &Edit) {
        // write-behind — 스트로크마다 원격 왕복하지 않고, 순서 보장 큐로
        // 스트로크 작업과 함께 원격에 배치 반영합니다.
        let mut g = self.inner.lock().expect("cache mutex poisoned");
        g.enqueue(PendingOp::LogEdit {
            doc_id,
            edit: edit.clone(),
        });
    }

    fn log_edits(&self, doc_id: i64, edits: &[Edit]) {
        self.remote.log_edits(doc_id, edits);
    }

    fn clear_edits(&self, doc_id: i64) {
        self.remote.clear_edits(doc_id);
        let mut g = self.inner.lock().expect("cache mutex poisoned");
        g.edits.remove(&doc_id);
        g.dirty_edits.remove(&doc_id);
        g.cache.clear_edits(doc_id);
    }

    fn get_word_cache(&self, word: &str) -> Option<Value> {
        self.remote.get_word_cache(word)
    }

    fn set_word_cache(&self, word: &str, data: &Value) {
        self.remote.set_word_cache(word, data);
    }

    fn insert_log(&self, epoch_ms: u128, seq: u64, event: &Value) {
        self.remote.insert_log(epoch_ms, seq, event);
    }

    fn insert_logs(&self, items: &[(u128, Value)]) {
        self.remote.insert_logs(items);
    }

    fn ping(&self) -> bool {
        self.remote.ping()
    }

    fn invalidate_document(&self, doc_id: i64) {
        let mut g = self.inner.lock().expect("cache mutex poisoned");
        g.stores.remove(&doc_id);
        g.dirty_stores.remove(&doc_id);
        g.cache.invalidate_store(doc_id);
    }

    fn identity(&self) -> Option<String> {
        self.remote.identity()
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

    #[test]
    fn connection_url_roundtrips_through_file() {
        // app_data_dir은 LOCALAPPDATA/HOME 기준 — 테스트에서는 임시 폴더로 우회.
        let tmp = std::env::temp_dir().join(format!("freedf-conn-{}", std::process::id()));
        std::env::set_var("LOCALAPPDATA", &tmp);
        save_connection("postgres://u:p@h:5432/db");
        assert_eq!(
            load_saved_connection().as_deref(),
            Some("postgres://u:p@h:5432/db")
        );
        std::env::remove_var("LOCALAPPDATA");
        let _ = std::fs::remove_dir_all(&tmp);
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
    fn cache_is_purged_when_db_identity_changes() {
        let dir = std::env::temp_dir().join(format!("freedf-cache-ident-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let cache = LocalCache::new(dir.clone());
        cache.set_identity("OLD-DB");
        let mut store = AnnotationStore::new();
        store.add_strokes(0, Vec::new());
        cache.put_store(1, &store);
        cache.append_pending(&[PendingOp::DeleteStrokes {
            doc_id: 1,
            ids: vec![1],
        }]);
        cache.put_notes(&[]);

        // DB가 초기화되어 식별자가 바뀜 → 캐시 전체 폐기되어야 함.
        let cb = CachingBackend::new_with_identity(
            disconnected(),
            LocalCache::new(dir.clone()),
            Some("NEW-DB".into()),
        );
        let fresh = LocalCache::new(dir.clone());
        assert_eq!(fresh.get_identity().as_deref(), Some("NEW-DB"));
        assert_eq!(fresh.get_store(1), None, "옛 스토어가 남아 있으면 안 됨");
        assert!(
            fresh.load_pending().is_empty(),
            "옛 대기열이 남아 있으면 안 됨"
        );
        assert!(cb.ping() == false);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn coalesce_merges_consecutive_ops_in_order() {
        use freedf_core::history::Edit;
        use freedf_core::model::{Stroke, StrokePoint, ToolType};
        let stroke = |id: u64| Stroke {
            id,
            tool: ToolType::Pen,
            color: [20, 20, 20, 255],
            width: 2.0,
            points: vec![StrokePoint::new(0.0, 0.0, 0.5)],
            created_ms: 0,
        };
        let edit = || Edit::AddStrokes {
            page: 0,
            strokes: Vec::new(),
        };
        let ops = vec![
            PendingOp::InsertStrokes {
                doc_id: 1,
                page_index: 0,
                strokes: vec![stroke(1)],
            },
            PendingOp::InsertStrokes {
                doc_id: 1,
                page_index: 0,
                strokes: vec![stroke(2)],
            },
            PendingOp::InsertStrokes {
                doc_id: 1,
                page_index: 1,
                strokes: vec![stroke(3)],
            },
            PendingOp::LogEdit {
                doc_id: 1,
                edit: edit(),
            },
            PendingOp::LogEdit {
                doc_id: 1,
                edit: edit(),
            },
            PendingOp::LogEdit {
                doc_id: 2,
                edit: edit(),
            },
            PendingOp::DeleteStrokes {
                doc_id: 1,
                ids: vec![1],
            },
            PendingOp::DeleteStrokes {
                doc_id: 1,
                ids: vec![2, 3],
            },
        ];
        let merged = CachingBackend::coalesce_ops(ops);
        assert_eq!(merged.len(), 5, "병합 후: {merged:?}");
        assert!(matches!(
            &merged[0],
            PendingOp::InsertStrokes {
                doc_id: 1,
                page_index: 0,
                strokes
            } if strokes.len() == 2
        ));
        assert!(matches!(
            &merged[1],
            PendingOp::InsertStrokes {
                doc_id: 1,
                page_index: 1,
                ..
            }
        ));
        assert!(matches!(
            &merged[2],
            PendingOp::LogEdits { doc_id: 1, edits } if edits.len() == 2
        ));
        assert!(matches!(&merged[3], PendingOp::LogEdit { doc_id: 2, .. }));
        assert!(matches!(
            &merged[4],
            PendingOp::DeleteStrokes { doc_id: 1, ids } if ids == &vec![1, 2, 3]
        ));
    }

    /// 실 DB 왕복 — `FREEDF_TEST_DB=1 cargo test -p freedf caching_backend_against_live_db`    /// (freedf Postgres가 떠 있어야 함). 플러시 스레드는 시작하지 않아 결정적입니다.
    #[test]
    fn caching_backend_against_live_db() {
        if std::env::var("FREEDF_TEST_DB").as_deref() != Ok("1") {
            return;
        }
        let url = std::env::var("FREEDF_DATABASE_URL")
            .unwrap_or_else(|_| crate::db::DEFAULT_DATABASE_URL.to_string());
        let db = Arc::new(crate::db::Db::connect(&url).expect("connect"));
        let dir = std::env::temp_dir().join(format!("freedf-cache-live-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let cached = CachingBackend::new(db.clone(), LocalCache::new(dir.clone()));

        let doc_id = db
            .insert_document("note", "cache-live-test", None, 1, b"%PDF-1.4 fake")
            .expect("insert");

        // 1) 원격 로드 → 캐시에 저장.
        let s1 = cached.load_bundle(doc_id).store;
        assert_eq!(s1.stroke_count_on(0), 0);
        // 2) write-behind: 캐시엔 즉시 병합, 원격엔 아직 없음.
        let id = db.alloc_stroke_ids(1)[0];
        let stroke = Stroke {
            id: id as u64,
            tool: freedf_core::model::ToolType::Pen,
            color: [20, 20, 20, 255],
            width: 2.0,
            points: vec![freedf_core::model::StrokePoint::new(0.0, 0.0, 0.5)],
            created_ms: 0,
        };
        cached.insert_strokes(doc_id, 0, &[stroke.clone()]);
        assert_eq!(cached.load_bundle(doc_id).store.stroke_count_on(0), 1);
        assert_eq!(db.load_bundle(doc_id).store.stroke_count_on(0), 0);
        // 디스크 직렬화는 아직 백그라운드 몫 — 플러시 전엔 파일에 없음.
        let disk = LocalCache::new(dir.clone());
        assert_ne!(disk.get_store(doc_id).map(|s| s.stroke_count_on(0)), Some(1));
        // 3) 동기 플러시 → 원격 반영 + 디스크 캐시 직렬화.
        assert!(cached.flush_pending());
        assert_eq!(db.load_bundle(doc_id).store.stroke_count_on(0), 1);
        assert_eq!(disk.get_store(doc_id).map(|s| s.stroke_count_on(0)), Some(1));
        // 4) 무효화 → 원격(병합 상태)에서 다시 로드.
        cached.invalidate_document(doc_id);
        assert_eq!(cached.load_bundle(doc_id).store.stroke_count_on(0), 1);

        db.delete_document(doc_id).expect("cleanup");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
