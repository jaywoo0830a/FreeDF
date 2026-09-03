//! 로컬 디스크 캐시 + write-behind 대기열 (로드맵 ④).
//!
//! DB가 해외에 있어도 문서 열기가 빠르도록, 무거운 데이터(PDF 본문, 주석
//! 스토어)와 자주 읽는 스냅샷(세션/편집 저널/노트·최근 목록)을 앱 데이터
//! 폴더의 `cache/`에 보관합니다.
//!
//! 스트로크 삽입/삭제는 **write-behind** 방식입니다:
//! 1. 로컬 캐시의 스토어에 **즉시 병합** (불변식: 캐시 = 원격 + 미처리 대기열)
//! 2. 작업을 `pending.jsonl`에 append (영속 — 재시작에도 유지)
//! 3. 백그라운드 스레드가 원격(PostgreSQL)에 **순서대로** 플러시
//!
//! 모든 파일 접근은 `CachingBackend`의 뮤텍스 안에서 직렬화됩니다
//! (UI 스레드와 플러시 스레드가 동시에 만지지 않음). 파일 쓰기는 원자적이지
//! 않지만, 파손 시 파싱 실패 → None 처리로 안전하게 폴백합니다.

use crate::db::{DocRow, RecentRow};
use freedf_core::history::Edit;
use freedf_core::model::Stroke;
use freedf_core::paper::PagePaper;
use freedf_core::store::AnnotationStore;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

/// 원격에 아직 반영되지 않은 쓰기 작업 (JSONL로 영속).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PendingOp {
    InsertStrokes {
        doc_id: i64,
        page_index: i32,
        strokes: Vec<Stroke>,
    },
    DeleteStrokes {
        doc_id: i64,
        ids: Vec<i64>,
    },
    /// 영속 편집 저널(undo) 기록 — 스트로크마다 왕복하지 않도록 배치 반영.
    LogEdit {
        doc_id: i64,
        edit: Edit,
    },
    /// 편집 저널 일괄 기록 — 플러시 시 연속 LogEdit을 합친 형태.
    LogEdits {
        doc_id: i64,
        edits: Vec<Edit>,
    },
    /// 문서별 GUI 세션 저장 (도구/색 변경 등 빈번한 쓰기).
    UpsertSession {
        doc_id: i64,
        state: Value,
    },
    /// 전역 앱 상태(기본 세션) 저장.
    SetAppState {
        key: String,
        value: Value,
    },
    /// 페이지 하나의 용지/북마크 저장 (북마크 토글, 용지 적용).
    UpsertPage {
        doc_id: i64,
        page_index: i32,
        paper: PagePaper,
        bookmarked: bool,
    },
    /// 최근 문서 기록 (문서 열 때).
    TouchRecent {
        kind: String,
        doc_id: i64,
        title: String,
    },
}

impl PendingOp {
    /// 문서 단위 작업이면 해당 doc_id (SetAppState는 None).
    pub fn doc_id(&self) -> Option<i64> {
        match self {
            PendingOp::InsertStrokes { doc_id, .. } => Some(*doc_id),
            PendingOp::DeleteStrokes { doc_id, .. } => Some(*doc_id),
            PendingOp::LogEdit { doc_id, .. } => Some(*doc_id),
            PendingOp::LogEdits { doc_id, .. } => Some(*doc_id),
            PendingOp::UpsertSession { doc_id, .. } => Some(*doc_id),
            PendingOp::UpsertPage { doc_id, .. } => Some(*doc_id),
            PendingOp::TouchRecent { doc_id, .. } => Some(*doc_id),
            PendingOp::SetAppState { .. } => None,
        }
    }
}

/// 대기열 작업을 스토어에 적용 (로컬 병합 상태 유지).
pub fn apply_op_to_store(store: &mut AnnotationStore, op: &PendingOp) {
    match op {
        PendingOp::InsertStrokes {
            page_index,
            strokes,
            ..
        } => {
            store.add_strokes(*page_index as usize, strokes.clone());
        }
        PendingOp::DeleteStrokes { ids, .. } => {
            let ids: Vec<u64> = ids.iter().map(|i| *i as u64).collect();
            let pages: Vec<usize> = store.pages().map(|p| p.page_index).collect();
            for page in pages {
                store.remove_strokes(page, &ids);
            }
        }
        // 저널 기록/세션/앱상태/용지/최근은 apply_local에서 처리 (스토어 전용
        // 헬퍼인 이 함수에서는 무시 — 용지/북마크는 apply_local이 스토어에 반영).
        PendingOp::LogEdit { .. } => {}
        PendingOp::LogEdits { .. } => {}
        PendingOp::UpsertSession { .. } => {}
        PendingOp::SetAppState { .. } => {}
        PendingOp::UpsertPage { .. } => {}
        PendingOp::TouchRecent { .. } => {}
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Write-behind 공용 파이프라인
//
// 새 write-behind 액션을 추가하려면 3곳만 고치면 됩니다:
//   1) `PendingOp`에 변형 추가 (serde)
//   2) `apply_local`에 arm — 로컬(메모리/파일) 캐시 즉시 반영
//   3) storage.rs의 `apply_remote`에 arm — 원격 반영
// 그리고 CachingBackend 메서드는 `g.enqueue(op)` 한 줄이면 끝입니다.
// ─────────────────────────────────────────────────────────────────────────────

/// 메모리 병합 캐시 상태 (CachingBackend 내부 — 뮤텍스로 직렬화).
pub(crate) struct CacheInner {
    pub cache: LocalCache,
    pub pending: Vec<PendingOp>,
    /// 메모리 병합 스토어 — UI 스레드는 파일 직렬화 없이 O(1)로 갱신.
    pub stores: BTreeMap<i64, AnnotationStore>,
    pub dirty_stores: HashSet<i64>,
    /// 메모리 편집 저널 캐시.
    pub edits: BTreeMap<i64, Vec<Edit>>,
    pub dirty_edits: HashSet<i64>,
}

impl CacheInner {
    /// 재시작 후 남아 있던 미처리 대기열을 복원해 초기화.
    pub fn new(cache: LocalCache) -> Self {
        let pending = cache.load_pending();
        Self {
            cache,
            pending,
            stores: BTreeMap::new(),
            dirty_stores: HashSet::new(),
            edits: BTreeMap::new(),
            dirty_edits: HashSet::new(),
        }
    }

    /// **write-behind 공용 파이프라인** — 어떤 액션이든 이 한 줄로:
    /// ① 로컬 캐시 즉시 반영(apply_local) → ② 메모리 대기열 → ③ JSONL 영속.
    pub fn enqueue(&mut self, op: PendingOp) {
        apply_local(&op, self);
        self.pending.push(op.clone());
        self.cache.append_pending(&[op]);
    }
}

fn ensure_store(g: &mut CacheInner, doc_id: i64) {
    if !g.stores.contains_key(&doc_id) {
        let s = g.cache.get_store(doc_id).unwrap_or_default();
        g.stores.insert(doc_id, s);
    }
}

/// 작업의 **로컬 반영** — 불변식 유지: 캐시 = 원격 상태 + 미처리 대기열.
/// (apply_op_to_store와 달리 세션/저널/앱상태까지 한 테이블에서 처리)
pub fn apply_local(op: &PendingOp, g: &mut CacheInner) {
    match op {
        PendingOp::InsertStrokes {
            doc_id,
            page_index,
            strokes,
        } => {
            ensure_store(g, *doc_id);
            if let Some(store) = g.stores.get_mut(doc_id) {
                store.add_strokes(*page_index as usize, strokes.clone());
                g.dirty_stores.insert(*doc_id);
            }
        }
        PendingOp::DeleteStrokes { doc_id, .. } => {
            ensure_store(g, *doc_id);
            if let Some(store) = g.stores.get_mut(doc_id) {
                apply_op_to_store(store, op);
                g.dirty_stores.insert(*doc_id);
            }
        }
        PendingOp::LogEdit { doc_id, edit } => {
            if !g.edits.contains_key(doc_id) {
                let e = g.cache.get_edits(*doc_id).unwrap_or_default();
                g.edits.insert(*doc_id, e);
            }
            if let Some(edits) = g.edits.get_mut(doc_id) {
                edits.push(edit.clone());
                g.dirty_edits.insert(*doc_id);
            }
        }
        PendingOp::LogEdits { doc_id, edits } => {
            if !g.edits.contains_key(doc_id) {
                let e = g.cache.get_edits(*doc_id).unwrap_or_default();
                g.edits.insert(*doc_id, e);
            }
            if let Some(store_edits) = g.edits.get_mut(doc_id) {
                store_edits.extend(edits.iter().cloned());
                g.dirty_edits.insert(*doc_id);
            }
        }
        PendingOp::UpsertSession { doc_id, state } => {
            // 세션 JSON은 작아 파일 캐시도 즉시 갱신해도 저렴합니다.
            g.cache.put_session(*doc_id, state);
        }
        PendingOp::SetAppState { .. } => {
            // 로컬 미러 없음 — 큐에만 (다음 시작 시 pending에서 플러시).
        }
        PendingOp::UpsertPage {
            doc_id,
            page_index,
            paper,
            bookmarked,
        } => {
            ensure_store(g, *doc_id);
            if let Some(store) = g.stores.get_mut(doc_id) {
                let idx = *page_index as usize;
                store.set_paper(idx, *paper);
                if store.is_bookmarked(idx) != *bookmarked {
                    store.toggle_bookmark(idx);
                }
                g.dirty_stores.insert(*doc_id);
            }
        }
        PendingOp::TouchRecent { .. } => {
            // 다음 load_recents가 원격에서 다시 가져오도록 스냅샷 무효화.
            g.cache.invalidate_recents();
        }
    }
}

/// 파일 기반 로컬 캐시 (호출자가 뮤텍스로 직렬화).
pub struct LocalCache {
    dir: PathBuf,
}

impl LocalCache {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    fn doc_path(&self, id: i64, ext: &str) -> PathBuf {
        self.path(&format!("doc_{id}.{ext}"))
    }

    fn ensure_dir(&self) {
        let _ = std::fs::create_dir_all(&self.dir);
    }

    fn read_json<T: for<'de> Deserialize<'de>>(&self, path: &Path) -> Option<T> {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
    }

    fn write_json<T: Serialize>(&self, path: &Path, value: &T) {
        self.ensure_dir();
        if let Ok(json) = serde_json::to_string(value) {
            let _ = std::fs::write(path, json);
        }
    }

    // ---------- PDF 본문 ----------

    pub fn get_pdf(&self, id: i64) -> Option<Vec<u8>> {
        std::fs::read(self.doc_path(id, "pdf")).ok()
    }

    pub fn put_pdf(&self, id: i64, bytes: &[u8]) {
        self.ensure_dir();
        let _ = std::fs::write(self.doc_path(id, "pdf"), bytes);
    }

    // ---------- 주석 스토어 ----------

    pub fn get_store(&self, id: i64) -> Option<AnnotationStore> {
        let s = std::fs::read_to_string(self.doc_path(id, "store.json")).ok()?;
        AnnotationStore::from_json(&s).ok()
    }

    pub fn put_store(&self, id: i64, store: &AnnotationStore) {
        self.ensure_dir();
        let _ = std::fs::write(self.doc_path(id, "store.json"), store.to_json());
    }

    pub fn invalidate_store(&self, id: i64) {
        let _ = std::fs::remove_file(self.doc_path(id, "store.json"));
    }

    // ---------- 세션 / 편집 저널 ----------

    pub fn get_session(&self, id: i64) -> Option<Value> {
        self.read_json(&self.doc_path(id, "session.json"))
    }

    pub fn put_session(&self, id: i64, state: &Value) {
        self.write_json(&self.doc_path(id, "session.json"), state);
    }

    pub fn get_edits(&self, id: i64) -> Option<Vec<Edit>> {
        self.read_json(&self.doc_path(id, "edits.json"))
    }

    pub fn put_edits(&self, id: i64, edits: &[Edit]) {
        self.write_json(&self.doc_path(id, "edits.json"), &edits.to_vec());
    }

    pub fn clear_edits(&self, id: i64) {
        let _ = std::fs::remove_file(self.doc_path(id, "edits.json"));
    }

    // ---------- 노트 / 최근 목록 스냅샷 ----------

    pub fn get_notes(&self) -> Option<Vec<DocRow>> {
        self.read_json(&self.path("notes.json"))
    }

    pub fn put_notes(&self, rows: &[DocRow]) {
        self.write_json(&self.path("notes.json"), &rows.to_vec());
    }

    pub fn invalidate_notes(&self) {
        let _ = std::fs::remove_file(self.path("notes.json"));
    }

    pub fn get_recents(&self) -> Option<Vec<RecentRow>> {
        self.read_json(&self.path("recents.json"))
    }

    pub fn put_recents(&self, rows: &[RecentRow]) {
        self.write_json(&self.path("recents.json"), &rows.to_vec());
    }

    pub fn invalidate_recents(&self) {
        let _ = std::fs::remove_file(self.path("recents.json"));
    }

    // ---------- write-behind 대기열 (JSONL) ----------

    pub fn pending_path(&self) -> PathBuf {
        self.path("pending.jsonl")
    }
    pub fn append_pending(&self, ops: &[PendingOp]) {
        use std::io::Write;
        self.ensure_dir();
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.pending_path())
        {
            for op in ops {
                if let Ok(line) = serde_json::to_string(op) {
                    let _ = writeln!(f, "{line}");
                }
            }
        }
    }

    /// 대기열 로드 — 파손된 줄은 건너뜁니다.
    pub fn load_pending(&self) -> Vec<PendingOp> {
        std::fs::read_to_string(self.pending_path())
            .map(|s| {
                s.lines()
                    .filter_map(|l| serde_json::from_str(l).ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn clear_pending(&self) {
        let _ = std::fs::remove_file(self.pending_path());
    }

    // ---------- DB 인스턴스 식별자 ----------

    fn identity_path(&self) -> PathBuf {
        self.path("identity")
    }

    pub fn get_identity(&self) -> Option<String> {
        std::fs::read_to_string(self.identity_path())
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    pub fn set_identity(&self, id: &str) {
        self.ensure_dir();
        let _ = std::fs::write(self.identity_path(), id);
    }

    /// 캐시 전체 폐기 — DB가 초기화/교체되어 식별자가 달라졌을 때 호출.
    /// (문서 id 재사용으로 옛 스토어/대기열/목록이 섞이는 오염 방지)
    pub fn clear_all(&self) {
        if let Ok(entries) = std::fs::read_dir(&self.dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                let known = name.starts_with("doc_")
                    || matches!(
                        name.as_str(),
                        "notes.json" | "recents.json" | "pending.jsonl" | "identity"
                    );
                if known {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use freedf_core::model::{StrokePoint, ToolType};

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("freedf-cache-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn sample_stroke(id: u64) -> Stroke {
        Stroke {
            id,
            tool: ToolType::Pen,
            color: [20, 20, 20, 255],
            width: 2.0,
            points: vec![
                StrokePoint::new(0.0, 0.0, 0.5),
                StrokePoint::new(10.0, 5.0, 0.6),
            ],
            created_ms: 1000,
        }
    }

    #[test]
    fn pdf_roundtrips() {
        let cache = LocalCache::new(temp_dir("pdf"));
        assert_eq!(cache.get_pdf(7), None);
        cache.put_pdf(7, b"%PDF-1.4 fake");
        assert_eq!(cache.get_pdf(7).as_deref(), Some(&b"%PDF-1.4 fake"[..]));
    }

    #[test]
    fn store_roundtrips_and_invalidation() {
        let dir = temp_dir("store");
        let cache = LocalCache::new(dir.clone());
        let mut store = AnnotationStore::new();
        store.add_strokes(0, vec![sample_stroke(1)]);
        cache.put_store(3, &store);
        let loaded = cache.get_store(3).expect("store cached");
        assert_eq!(loaded.stroke_count_on(0), 1);
        cache.invalidate_store(3);
        assert_eq!(cache.get_store(3), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn session_and_edits_roundtrip() {
        let dir = temp_dir("session");
        let cache = LocalCache::new(dir.clone());
        let session = serde_json::json!({"page": 2, "zoom": 1.5});
        cache.put_session(4, &session);
        assert_eq!(cache.get_session(4), Some(session));

        let edits = vec![freedf_core::history::Edit::AddStrokes {
            page: 0,
            strokes: vec![sample_stroke(2)],
        }];
        cache.put_edits(4, &edits);
        assert_eq!(cache.get_edits(4).map(|e| e.len()), Some(1));
        cache.clear_edits(4);
        assert_eq!(cache.get_edits(4), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn notes_and_recents_roundtrip() {
        let dir = temp_dir("meta");
        let cache = LocalCache::new(dir.clone());
        let note = DocRow {
            id: 1,
            kind: "note".into(),
            title: "Math".into(),
            origin_path: None,
            page_count: 3,
            created_at: 1,
            updated_at: 2,
        };
        cache.put_notes(&[note]);
        assert_eq!(cache.get_notes().map(|v| v.len()), Some(1));
        cache.invalidate_notes();
        assert_eq!(cache.get_notes(), None);

        let recent = RecentRow {
            kind: "note".into(),
            doc_id: 1,
            title: "Math".into(),
            opened_at: 2,
            origin_path: None,
        };
        cache.put_recents(&[recent]);
        assert_eq!(cache.get_recents().map(|v| v.len()), Some(1));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pending_jsonl_roundtrip_skips_corrupt_lines() {
        let dir = temp_dir("pending");
        let cache = LocalCache::new(dir.clone());
        let op = PendingOp::InsertStrokes {
            doc_id: 5,
            page_index: 0,
            strokes: vec![sample_stroke(9)],
        };
        cache.append_pending(&[op.clone()]);
        // 파손 줄 추가.
        let broken = cache.pending_path();
        {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new().append(true).open(&broken).unwrap();
            writeln!(f, "not json at all").unwrap();
        }
        let loaded = cache.load_pending();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0], op);
        cache.clear_pending();
        assert!(cache.load_pending().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pending_log_edit_roundtrip() {
        let dir = temp_dir("logedit");
        let cache = LocalCache::new(dir.clone());
        let edit = freedf_core::history::Edit::AddStrokes {
            page: 1,
            strokes: vec![sample_stroke(7)],
        };
        let op = PendingOp::LogEdit {
            doc_id: 3,
            edit: edit.clone(),
        };
        let batch = PendingOp::LogEdits {
            doc_id: 3,
            edits: vec![edit.clone(), edit],
        };
        cache.append_pending(&[op.clone(), batch.clone()]);
        let loaded = cache.load_pending();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0], op);
        assert_eq!(loaded[1], batch);
        assert_eq!(loaded[0].doc_id(), Some(3));
        assert_eq!(loaded[1].doc_id(), Some(3));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pending_session_and_app_state_roundtrip() {
        let dir = temp_dir("sessionops");
        let cache = LocalCache::new(dir.clone());
        let ops = vec![
            PendingOp::UpsertSession {
                doc_id: 4,
                state: serde_json::json!({ "page": 2 }),
            },
            PendingOp::SetAppState {
                key: "session".into(),
                value: serde_json::json!({ "tool": "Pen" }),
            },
        ];
        cache.append_pending(&ops);
        let loaded = cache.load_pending();
        assert_eq!(loaded, ops);
        assert_eq!(loaded[0].doc_id(), Some(4));
        assert_eq!(loaded[1].doc_id(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_insert_and_delete_merge() {
        let mut store = AnnotationStore::new();
        apply_op_to_store(
            &mut store,
            &PendingOp::InsertStrokes {
                doc_id: 1,
                page_index: 2,
                strokes: vec![sample_stroke(10)],
            },
        );
        assert_eq!(store.stroke_count_on(2), 1);
        apply_op_to_store(
            &mut store,
            &PendingOp::DeleteStrokes {
                doc_id: 1,
                ids: vec![10],
            },
        );
        assert_eq!(store.stroke_count_on(2), 0);
    }

    #[test]
    fn enqueue_pipeline_updates_local_and_persists() {
        let dir = temp_dir("pipeline");
        let cache = LocalCache::new(dir.clone());
        let mut inner = CacheInner::new(cache);
        inner.enqueue(PendingOp::InsertStrokes {
            doc_id: 1,
            page_index: 0,
            strokes: vec![sample_stroke(1)],
        });
        // ① 로컬(메모리) 즉시 반영.
        assert_eq!(inner.stores.get(&1).map(|s| s.stroke_count_on(0)), Some(1));
        // ② 메모리 대기열.
        assert_eq!(inner.pending.len(), 1);
        // ③ JSONL 영속.
        let fresh = LocalCache::new(dir.clone());
        assert_eq!(fresh.load_pending().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn enqueue_page_op_applies_bookmark_and_paper_locally() {
        let dir = temp_dir("pageops");
        let cache = LocalCache::new(dir.clone());
        let mut inner = CacheInner::new(cache);
        let op = PendingOp::UpsertPage {
            doc_id: 2,
            page_index: 1,
            paper: PagePaper {
                style: freedf_core::paper::PaperStyle::Grid,
                color: [1, 2, 3, 4],
            },
            bookmarked: true,
        };
        inner.enqueue(op.clone());
        let store = inner.stores.get(&2).expect("store in memory");
        assert!(store.is_bookmarked(1));
        assert_eq!(
            store.paper_on(1).map(|p| p.style),
            Some(freedf_core::paper::PaperStyle::Grid)
        );
        let loaded = LocalCache::new(dir.clone()).load_pending();
        assert_eq!(loaded, vec![op]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
