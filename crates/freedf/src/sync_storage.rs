//! Sync v3 API 백엔드 — 앱의 저장 경계(`StorageBackend`)를 API 서버로 연결.
//!
//! 설계: docs/sync-protocol-v3.md. 클라이언트 규칙:
//! - **로컬 미러**: 문서별 스토어/저널/세션을 메모리에 유지 — 필기 경로는
//!   메모리 O(1)만 건드리고 네트워크·직렬화가 없습니다 (§4 실시간성).
//! - **스냅샷 플러시**: 구조 연산/저장 시(`sync_meta`/`flush_pending`) 미러를
//!   ZIP 스냅샷으로 만들어 `PUT /v3/documents/{id}/snapshot` — 충돌이면
//!   서버가 계산한 패치를 병합해 재전송합니다 (보통 1회).
//! - 앱 전역 상태(app_state/recents/word_cache)는 로컬 파일에 저장합니다.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Mutex;

use freedf_core::history::Edit;
use freedf_core::model::{Stroke, StrokePoint, ToolType};
use freedf_core::paper::{PagePaper, PaperStyle};
use freedf_core::store::AnnotationStore;
use freedf_sync::{CreateDocument, Digest, DocumentInfo, Patch, Snapshot, SyncClient, UploadState};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

use crate::server::MediaServerConfig;
use crate::storage::{DocRow, LoadedBundle, RecentRow, StorageBackend};

const FLUSH_ATTEMPTS: usize = 4;
const RECENT_LIMIT: usize = 20;

/// 문서별 로컬 미러.
struct DocMirror {
    row: DocRow,
    store: AnnotationStore,
    edits: Vec<Edit>,
    session: Option<Value>,
    revision: i64,
    dirty: bool,
    pdf: Option<Vec<u8>>,
    pdf_digest: Option<Digest>,
}

struct Inner {
    docs: BTreeMap<i64, DocMirror>,
    next_id: AtomicI64,
    app_state: BTreeMap<String, Value>,
    recents: Vec<RecentRow>,
    word_cache: BTreeMap<String, Value>,
    dir: PathBuf,
}

/// Sync v3 서버를 저장소로 사용하는 백엔드.
pub struct SyncStorage {
    client: SyncClient,
    inner: Mutex<Inner>,
}

// ── 변환 헬퍼 ────────────────────────────────────────────────────────────────

fn style_str(style: PaperStyle) -> &'static str {
    match style {
        PaperStyle::Blank => "Blank",
        PaperStyle::Ruled => "Ruled",
        PaperStyle::Grid => "Grid",
        PaperStyle::Dotted => "Dotted",
    }
}

fn style_from(s: &str) -> PaperStyle {
    match s {
        "Ruled" => PaperStyle::Ruled,
        "Grid" => PaperStyle::Grid,
        "Dotted" => PaperStyle::Dotted,
        _ => PaperStyle::Blank,
    }
}

fn color_i32(c: [u8; 4]) -> Vec<i32> {
    c.iter().map(|v| *v as i32).collect()
}

fn color_u8(c: &[i32]) -> [u8; 4] {
    [
        c.first().copied().unwrap_or(0).clamp(0, 255) as u8,
        c.get(1).copied().unwrap_or(0).clamp(0, 255) as u8,
        c.get(2).copied().unwrap_or(0).clamp(0, 255) as u8,
        c.get(3).copied().unwrap_or(255).clamp(0, 255) as u8,
    ]
}

fn wire_to_core(s: &freedf_sync::Stroke) -> Stroke {
    Stroke {
        id: s.id.max(0) as u64,
        tool: ToolType::from_label(&s.tool),
        color: color_u8(&s.color),
        width: s.width,
        points: serde_json::from_value::<Vec<StrokePoint>>(s.points.clone()).unwrap_or_default(),
        created_ms: s.created_at.max(0) as u64,
    }
}

fn core_to_wire(page_index: i32, s: &Stroke) -> freedf_sync::Stroke {
    freedf_sync::Stroke {
        id: s.id as i64,
        page_index,
        tool: s.tool.label().to_lowercase(),
        color: color_i32(s.color),
        width: s.width,
        points: serde_json::to_value(&s.points).unwrap_or(Value::Array(Vec::new())),
        created_at: s.created_ms as i64,
    }
}

fn store_from_snapshot(snap: &Snapshot) -> AnnotationStore {
    let mut store = AnnotationStore::new();
    let mut grouped: BTreeMap<i32, Vec<Stroke>> = BTreeMap::new();
    for s in &snap.strokes {
        grouped
            .entry(s.page_index.max(0))
            .or_default()
            .push(wire_to_core(s));
    }
    for (page, strokes) in grouped {
        store.add_strokes(page as usize, strokes);
    }
    for p in &snap.pages {
        let idx = p.page_index.max(0) as usize;
        store.set_paper(
            idx,
            PagePaper {
                style: style_from(&p.style),
                color: color_u8(&p.color),
            },
        );
        if p.bookmarked {
            store.toggle_bookmark(idx);
        }
    }
    store
}

fn fallback_row(id: i64) -> DocRow {
    DocRow {
        id,
        kind: "note".into(),
        title: format!("Document {id}"),
        origin_path: None,
        page_count: 0,
        created_at: 0,
        updated_at: 0,
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn read_json<T: DeserializeOwned>(path: &std::path::Path) -> Option<T> {
    let s = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&s).ok()
}

fn write_json<T: Serialize>(path: &std::path::Path, value: &T) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(s) = serde_json::to_string_pretty(value) {
        let _ = std::fs::write(path, s);
    }
}

// ── 구현 ─────────────────────────────────────────────────────────────────────

impl SyncStorage {
    /// 서버 설정으로 백엔드 생성 (네트워크 없음 — 첫 요청에서 연결).
    pub fn new(config: &MediaServerConfig) -> Option<Self> {
        let base = config.base_url.trim().trim_end_matches('/');
        if base.is_empty() {
            return None;
        }
        let client =
            SyncClient::new_with_timeout(base, &config.api_key, std::time::Duration::from_secs(30))
                .ok()?;
        let dir = crate::storage::app_data_dir().join("v3_cache");
        let _ = std::fs::create_dir_all(&dir);
        let app_state: BTreeMap<String, Value> = read_json(&dir.join("app_state.json")).unwrap_or_default();
        let recents: Vec<RecentRow> = read_json(&dir.join("recents.json")).unwrap_or_default();
        let word_cache: BTreeMap<String, Value> = read_json(&dir.join("word_cache.json")).unwrap_or_default();
        Some(Self {
            client,
            inner: Mutex::new(Inner {
                docs: BTreeMap::new(),
                next_id: AtomicI64::new(1),
                app_state,
                recents,
                word_cache,
                dir,
            }),
        })
    }

    fn refresh_docs(&self) -> Result<(), String> {
        let infos = self.client.list_documents().map_err(err)?;
        let mut g = self.inner.lock().unwrap();
        for info in &infos {
            let entry = g.docs.entry(info.id).or_insert_with(|| DocMirror {
                row: fallback_row(info.id),
                store: AnnotationStore::new(),
                edits: Vec::new(),
                session: None,
                revision: 0,
                dirty: false,
                pdf: None,
                pdf_digest: None,
            });
            entry.row = DocRow {
                id: info.id,
                kind: info.kind.clone(),
                title: info.title.clone(),
                origin_path: info.origin_path.clone(),
                page_count: info.page_count,
                created_at: info.created_at,
                updated_at: info.updated_at,
            };
            entry.pdf_digest = info.pdf_digest.clone();
        }
        g.docs.retain(|id, _| infos.iter().any(|i| i.id == *id));
        Ok(())
    }

    fn info_of(&self, doc_id: i64) -> Option<DocumentInfo> {
        self.refresh_docs().ok()?;
        let g = self.inner.lock().unwrap();
        let m = g.docs.get(&doc_id)?;
        Some(DocumentInfo {
            id: m.row.id,
            kind: m.row.kind.clone(),
            title: m.row.title.clone(),
            origin_path: m.row.origin_path.clone(),
            page_count: m.row.page_count,
            created_at: m.row.created_at,
            updated_at: m.row.updated_at,
            pdf_digest: m.pdf_digest.clone(),
        })
    }

    /// 미러 → 업로드 스냅샷.
    fn build_snapshot(m: &DocMirror) -> Snapshot {
        let mut strokes: Vec<freedf_sync::Stroke> = Vec::new();
        for page in m.store.pages() {
            let idx = page.page_index as i32;
            for s in &page.strokes {
                strokes.push(core_to_wire(idx, s));
            }
        }
        let page_count = m.row.page_count.max(0);
        let mut pages: Vec<freedf_sync::Page> = Vec::new();
        for i in 0..page_count {
            let paper = m.store.paper_on_or(i as usize, PagePaper::default());
            pages.push(freedf_sync::Page {
                page_index: i,
                style: style_str(paper.style).to_string(),
                color: color_i32(paper.color),
                bookmarked: m.store.is_bookmarked(i as usize),
            });
        }
        let meta = freedf_sync::SnapshotMeta {
            revision: None,
            base_revision: Some(m.revision),
            page_count,
            updated_at: now_ms(),
            title: m.row.title.clone(),
            kind: m.row.kind.clone(),
            pdf_digest: m.pdf_digest.clone(),
            session: m.session.clone(),
        };
        Snapshot {
            meta,
            strokes,
            pages,
            pdf_digest: m.pdf_digest.clone(),
            edits: m
                .edits
                .iter()
                .map(|e| serde_json::to_value(e).unwrap_or(Value::Null))
                .collect(),
        }
    }

    /// 충돌 패치를 미러에 병합 (서버 계산 diff — 획 합집합, 구조 서버 기준).
    fn apply_patch_to_mirror(m: &mut DocMirror, patch: &Patch) {
        let pages: Vec<usize> = m.store.pages().map(|p| p.page_index).collect();
        for s in &patch.strokes_added {
            for page in &pages {
                m.store.remove_stroke(*page, s.id.max(0) as u64);
            }
            m.store.add_strokes(s.page_index.max(0) as usize, vec![wire_to_core(s)]);
        }
        for id in &patch.stroke_ids_removed {
            for page in &pages {
                m.store.remove_stroke(*page, id.max(&0).to_owned() as u64);
            }
        }
        if patch.pages_changed {
            for p in &patch.pages {
                let idx = p.page_index.max(0) as usize;
                m.store.set_paper(
                    idx,
                    PagePaper {
                        style: style_from(&p.style),
                        color: color_u8(&p.color),
                    },
                );
            }
        }
        if let Some(pc) = patch.meta.get("page_count").and_then(Value::as_i64) {
            m.row.page_count = pc.max(0) as i32;
        }
        if let Some(d) = &patch.pdf {
            m.pdf_digest = Some(d.clone());
            m.pdf = None; // 서버 기준 다이제스트 — 로컬 바이트는 다음 플러시에서 갱신.
        }
        m.revision = patch.to_revision;
    }

    /// 문서 미러를 스냅샷으로 플러시 (충돌 시 패치 병합 후 재시도).
    fn flush_document(&self, doc_id: i64) -> Result<(), String> {
        for _ in 0..FLUSH_ATTEMPTS {
            let (snap, pdf_upload) = {
                let mut g = self.inner.lock().unwrap();
                let m = g.docs.get_mut(&doc_id).ok_or("document not loaded")?;
                // PDF 바이트가 바뀌었으면 CAS에 먼저 업로드.
                let mut pdf_upload = false;
                if let Some(pdf) = &m.pdf {
                    let digest = Digest::from_bytes(pdf);
                    if m.pdf_digest.as_ref() != Some(&digest) {
                        self.client.put_object(pdf).map_err(err)?;
                        m.pdf_digest = Some(digest);
                        pdf_upload = true;
                    }
                }
                (Self::build_snapshot(m), pdf_upload)
            };
            let st = self
                .client
                .save_and_wait(doc_id, &snap, std::time::Duration::from_secs(30))
                .map_err(err)?;
            let mut g = self.inner.lock().unwrap();
            let m = g.docs.get_mut(&doc_id).ok_or("document not loaded")?;
            match st.state {
                UploadState::Applied => {
                    m.revision = st.revision.unwrap_or(m.revision + 1);
                    m.dirty = false;
                    return Ok(());
                }
                UploadState::Conflict => {
                    let conflict = st.conflict.ok_or("conflict without patch")?;
                    Self::apply_patch_to_mirror(m, &conflict.patch);
                    // 재시도.
                }
                UploadState::Failed => {
                    return Err(st.error.unwrap_or_else(|| "upload failed".into()))
                }
                other => return Err(format!("unexpected upload state {other:?}")),
            }
            let _ = pdf_upload;
        }
        Err("sync conflict loop — giving up".into())
    }
}

impl StorageBackend for SyncStorage {
    // ---------- documents ----------
    fn insert_document(
        &self,
        kind: &str,
        title: &str,
        origin_path: Option<&str>,
        page_count: i32,
        pdf: &[u8],
    ) -> Result<i64, String> {
        let pdf_digest = if pdf.is_empty() {
            None
        } else {
            let d = self.client.put_object(pdf).map_err(err)?;
            Some(d)
        };
        let created = self
            .client
            .create_document(&CreateDocument {
                kind: kind.to_string(),
                title: title.to_string(),
                origin_path: origin_path.map(String::from),
                page_count,
                pdf_digest: pdf_digest.clone(),
            })
            .map_err(err)?;
        let id = created.id;
        let now = now_ms();
        let mut g = self.inner.lock().unwrap();
        g.docs.insert(
            id,
            DocMirror {
                row: DocRow {
                    id,
                    kind: kind.to_string(),
                    title: title.to_string(),
                    origin_path: origin_path.map(String::from),
                    page_count,
                    created_at: now,
                    updated_at: now,
                },
                store: AnnotationStore::new(),
                edits: Vec::new(),
                session: None,
                revision: 0,
                dirty: false,
                pdf: if pdf.is_empty() {
                    None
                } else {
                    Some(pdf.to_vec())
                },
                pdf_digest,
            },
        );
        Ok(id)
    }

    fn get_document(&self, id: i64) -> Option<DocRow> {
        self.refresh_docs().ok()?;
        self.inner.lock().unwrap().docs.get(&id).map(|m| m.row.clone())
    }

    fn find_document_by_path(&self, path: &str) -> Option<i64> {
        self.refresh_docs().ok()?;
        self.inner
            .lock()
            .unwrap()
            .docs
            .values()
            .find(|m| m.row.origin_path.as_deref() == Some(path))
            .map(|m| m.row.id)
    }

    fn load_pdf(&self, id: i64) -> Option<Vec<u8>> {
        {
            let g = self.inner.lock().unwrap();
            if let Some(m) = g.docs.get(&id) {
                if let Some(pdf) = &m.pdf {
                    return Some(pdf.clone());
                }
            }
        }
        let digest = self.info_of(id)?.pdf_digest?;
        self.client.get_object(&digest).ok()
    }

    fn update_title(&self, id: i64, title: &str) -> Result<(), String> {
        self.client.rename_document(id, title).map_err(err)?;
        let mut g = self.inner.lock().unwrap();
        if let Some(m) = g.docs.get_mut(&id) {
            m.row.title = title.to_string();
            m.row.updated_at = now_ms();
        }
        Ok(())
    }

    fn delete_document(&self, id: i64) -> Result<(), String> {
        self.client.delete_document(id).map_err(err)?;
        self.inner.lock().unwrap().docs.remove(&id);
        Ok(())
    }

    fn list_notes(&self) -> Vec<DocRow> {
        self.refresh_docs().ok();
        self.inner
            .lock()
            .unwrap()
            .docs
            .values()
            .filter(|m| m.row.kind == "note")
            .map(|m| m.row.clone())
            .collect()
    }

    // ---------- pages ----------
    fn upsert_page(&self, doc_id: i64, page_index: i32, paper: &PagePaper, bookmarked: bool) {
        let mut g = self.inner.lock().unwrap();
        if let Some(m) = g.docs.get_mut(&doc_id) {
            let idx = page_index.max(0) as usize;
            m.store.set_paper(idx, *paper);
            if m.store.is_bookmarked(idx) != bookmarked {
                m.store.toggle_bookmark(idx);
            }
            m.dirty = true;
        }
    }

    // ---------- strokes ----------
    fn alloc_stroke_ids(&self, n: usize) -> Vec<i64> {
        let g = self.inner.lock().unwrap();
        (0..n)
            .map(|_| g.next_id.fetch_add(1, Ordering::Relaxed))
            .collect()
    }

    fn insert_strokes(&self, doc_id: i64, page_index: i32, strokes: &[Stroke]) {
        let mut g = self.inner.lock().unwrap();
        if let Some(m) = g.docs.get_mut(&doc_id) {
            m.store
                .add_strokes(page_index.max(0) as usize, strokes.to_vec());
            m.dirty = true;
        } else {
            return;
        }
        // id 카운터를 최대 스트로크 id 위로.
        let max_id = strokes.iter().map(|s| s.id).max().unwrap_or(0) as i64;
        let mut cur = g.next_id.load(Ordering::Relaxed);
        while cur <= max_id {
            match g.next_id.compare_exchange(
                cur,
                max_id + 1,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => cur = actual,
            }
        }
    }

    fn delete_strokes(&self, doc_id: i64, ids: &[i64]) {
        let mut g = self.inner.lock().unwrap();
        if let Some(m) = g.docs.get_mut(&doc_id) {
            let pages: Vec<usize> = m.store.pages().map(|p| p.page_index).collect();
            for page in pages {
                m.store
                    .remove_strokes(page, &ids.iter().map(|i| *i as u64).collect::<Vec<_>>());
            }
            m.dirty = true;
        }
    }

    fn load_bundle(&self, doc_id: i64) -> LoadedBundle {
        // 1) 로컬 미러 (가장 최신 상태 — dirty 포함).
        {
            let g = self.inner.lock().unwrap();
            if let Some(m) = g.docs.get(&doc_id) {
                return LoadedBundle {
                    store: m.store.clone(),
                    edits: m.edits.clone(),
                    session: m.session.clone(),
                };
            }
        }
        // 2) 서버에서 스냅샷 다운로드.
        let snap = match self.client.download_snapshot(doc_id) {
            Ok(dl) => dl.snapshot,
            Err(_) => return LoadedBundle::default(),
        };
        let store = store_from_snapshot(&snap);
        let edits: Vec<Edit> = snap
            .edits
            .iter()
            .filter_map(|e| serde_json::from_value(e.clone()).ok())
            .collect();
        let revision = snap.revision().unwrap_or(0);
        // id 카운터를 최대 스트로크 id 위로.
        let max_id = store
            .pages()
            .flat_map(|p| p.strokes.iter())
            .map(|s| s.id)
            .max()
            .unwrap_or(0);
        let mut g = self.inner.lock().unwrap();
        let cur = g.next_id.load(Ordering::Relaxed);
        if cur <= max_id as i64 {
            g.next_id.store(max_id as i64 + 1, Ordering::Relaxed);
        }
        let row = g
            .docs
            .get(&doc_id)
            .map(|m| m.row.clone())
            .unwrap_or_else(|| fallback_row(doc_id));
        g.docs.insert(
            doc_id,
            DocMirror {
                row,
                store: store.clone(),
                edits: edits.clone(),
                session: snap.meta.session.clone(),
                revision,
                dirty: false,
                pdf: None,
                pdf_digest: snap.pdf_digest.clone(),
            },
        );
        LoadedBundle {
            store,
            edits,
            session: snap.meta.session,
        }
    }

    fn sync_meta(
        &self,
        doc_id: i64,
        page_count: i32,
        entries: &[(i32, PagePaper, bool)],
        pdf: Option<&[u8]>,
    ) -> Result<(), String> {
        {
            let mut g = self.inner.lock().unwrap();
            let m = g.docs.get_mut(&doc_id).ok_or("document not loaded")?;
            for (page_index, paper, bookmarked) in entries {
                let idx = (*page_index).max(0) as usize;
                m.store.set_paper(idx, *paper);
                if m.store.is_bookmarked(idx) != *bookmarked {
                    m.store.toggle_bookmark(idx);
                }
            }
            m.row.page_count = page_count.max(0);
            if let Some(pdf) = pdf {
                m.pdf = Some(pdf.to_vec());
            }
            m.dirty = true;
        }
        self.flush_document(doc_id)
    }

    fn shift_strokes(&self, doc_id: i64, from: i32, delta: i32) {
        let mut g = self.inner.lock().unwrap();
        if let Some(m) = g.docs.get_mut(&doc_id) {
            m.store.shift_pages(from.max(0) as usize, delta);
            m.dirty = true;
        }
    }

    fn delete_page_data(&self, doc_id: i64, page: i32) {
        let mut g = self.inner.lock().unwrap();
        if let Some(m) = g.docs.get_mut(&doc_id) {
            m.store.remove_page(page.max(0) as usize);
            m.dirty = true;
        }
    }

    fn rotate_page_data(&self, doc_id: i64, page: i32, clockwise: bool, w: f32, h: f32) {
        let mut g = self.inner.lock().unwrap();
        if let Some(m) = g.docs.get_mut(&doc_id) {
            m.store.rotate_strokes_on(page.max(0) as usize, w, h, clockwise);
            m.dirty = true;
        }
    }

    fn rotate_all_data(&self, doc_id: i64, clockwise: bool, sizes: &[[f32; 2]]) {
        let mut g = self.inner.lock().unwrap();
        if let Some(m) = g.docs.get_mut(&doc_id) {
            for (i, size) in sizes.iter().enumerate() {
                m.store.rotate_strokes_on(i, size[0], size[1], clockwise);
            }
            m.dirty = true;
        }
    }

    fn flush_pending(&self) -> bool {
        let ids: Vec<i64> = self
            .inner
            .lock()
            .unwrap()
            .docs
            .iter()
            .filter(|(_, m)| m.dirty)
            .map(|(id, _)| *id)
            .collect();
        ids.iter().all(|id| self.flush_document(*id).is_ok())
    }

    // ---------- sessions ----------
    fn upsert_session(&self, doc_id: i64, state: &Value) {
        let mut g = self.inner.lock().unwrap();
        if let Some(m) = g.docs.get_mut(&doc_id) {
            m.session = Some(state.clone());
            m.dirty = true;
        }
    }

    // ---------- 전역 앱 상태 ----------
    fn get_app_state(&self, key: &str) -> Option<Value> {
        self.inner.lock().unwrap().app_state.get(key).cloned()
    }

    fn set_app_state(&self, key: &str, value: &Value) {
        let mut g = self.inner.lock().unwrap();
        g.app_state.insert(key.to_string(), value.clone());
        write_json(&g.dir.join("app_state.json"), &g.app_state);
    }

    // ---------- recents ----------
    fn load_recents(&self) -> Vec<RecentRow> {
        self.inner.lock().unwrap().recents.clone()
    }

    fn touch_recent(&self, kind: &str, doc_id: i64, title: &str) {
        let mut g = self.inner.lock().unwrap();
        let origin_path = g
            .docs
            .get(&doc_id)
            .and_then(|m| m.row.origin_path.clone());
        g.recents
            .retain(|r| !(r.kind == kind && r.doc_id == doc_id));
        g.recents.insert(
            0,
            RecentRow {
                kind: kind.to_string(),
                doc_id,
                title: title.to_string(),
                opened_at: now_ms(),
                origin_path,
            },
        );
        g.recents.truncate(RECENT_LIMIT);
        write_json(&g.dir.join("recents.json"), &g.recents);
    }

    // ---------- 편집 저널 (영속 히스토리) ----------
    fn log_edit(&self, doc_id: i64, edit: &Edit) {
        let mut g = self.inner.lock().unwrap();
        if let Some(m) = g.docs.get_mut(&doc_id) {
            m.edits.push(edit.clone());
            m.dirty = true;
        }
    }

    fn clear_edits(&self, doc_id: i64) {
        let mut g = self.inner.lock().unwrap();
        if let Some(m) = g.docs.get_mut(&doc_id) {
            m.edits.clear();
            m.dirty = true;
        }
    }

    // ---------- 사전 캐시 ----------
    fn get_word_cache(&self, word: &str) -> Option<Value> {
        self.inner.lock().unwrap().word_cache.get(word).cloned()
    }

    fn set_word_cache(&self, word: &str, data: &Value) {
        let mut g = self.inner.lock().unwrap();
        g.word_cache.insert(word.to_string(), data.clone());
        write_json(&g.dir.join("word_cache.json"), &g.word_cache);
    }

    // ---------- 이벤트 로그 ----------
    fn insert_log(&self, _epoch_ms: u128, _seq: u64, _event: &Value) {}
    fn insert_logs(&self, _items: &[(u128, Value)]) {}

    fn ping(&self) -> bool {
        self.client.health().is_ok()
    }

    fn invalidate_document(&self, doc_id: i64) {
        self.inner.lock().unwrap().docs.remove(&doc_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> MediaServerConfig {
        MediaServerConfig {
            enabled: true,
            base_url: "http://127.0.0.1:8080".into(),
            api_key: "test-key".into(),
        }
    }

    #[test]
    fn conversions_roundtrip() {
        let core = Stroke {
            id: 42,
            tool: ToolType::Fountain,
            color: [10, 20, 30, 255],
            width: 2.5,
            points: vec![StrokePoint::new(1.0, 2.0, 0.5)],
            created_ms: 1234,
        };
        let wire = core_to_wire(3, &core);
        assert_eq!(wire.tool, "fountain");
        assert_eq!(wire.page_index, 3);
        let back = wire_to_core(&wire);
        assert_eq!(back.id, core.id);
        assert_eq!(back.tool, ToolType::Fountain);
        assert_eq!(back.color, core.color);
        assert_eq!(back.points.len(), 1);
    }

    #[test]
    fn style_and_color_helpers() {
        assert_eq!(style_str(PaperStyle::Grid), "Grid");
        assert_eq!(style_from("Ruled"), PaperStyle::Ruled);
        assert_eq!(style_from("???".into()), PaperStyle::Blank);
        assert_eq!(color_u8(&color_i32([1, 2, 3, 255])), [1, 2, 3, 255]);
    }

    /// 실서버 대상 e2e — `FREEDF_TEST_SYNC=1`일 때만 실행.
    #[test]
    fn sync_storage_against_live_server() {
        if std::env::var("FREEDF_TEST_SYNC").ok().as_deref() != Some("1") {
            return; // 평소엔 no-op
        }
        let storage = SyncStorage::new(&config()).expect("storage");
        assert!(storage.ping());

        // 생성
        let id = storage
            .insert_document("note", "sync-storage-test", None, 2, b"")
            .expect("insert");
        assert!(id > 0);

        // 획/용지/저널/세션
        let ids = storage.alloc_stroke_ids(2);
        let strokes = vec![
            Stroke {
                id: ids[0] as u64,
                tool: ToolType::Pen,
                color: [0, 0, 0, 255],
                width: 1.0,
                points: vec![StrokePoint::new(1.0, 2.0, 0.5)],
                created_ms: 0,
            },
            Stroke {
                id: ids[1] as u64,
                tool: ToolType::Highlighter,
                color: [255, 230, 109, 120],
                width: 4.0,
                points: vec![StrokePoint::new(3.0, 4.0, 0.5)],
                created_ms: 0,
            },
        ];
        storage.insert_strokes(id, 0, &strokes);
        storage.upsert_page(id, 1, &PagePaper::default(), true);
        storage.log_edit(
            id,
            &Edit::AddStrokes {
                page: 0,
                strokes: strokes.clone(),
            },
        );
        storage.upsert_session(id, &serde_json::json!({"page": 1}));

        // 플러시 (스냅샷 업로드)
        storage
            .sync_meta(id, 2, &[(1, PagePaper::default(), true)], None)
            .expect("sync_meta");

        // 재로드 (미러 검증)
        let bundle = storage.load_bundle(id);
        assert_eq!(bundle.store.total_stroke_count(), 2);
        assert_eq!(bundle.edits.len(), 1);
        assert_eq!(bundle.session, Some(serde_json::json!({"page": 1})));
        assert!(bundle.store.is_bookmarked(1));

        // 서버 재다운로드 검증 (invalidate 후 load)
        storage.invalidate_document(id);
        let fresh = storage.load_bundle(id);
        assert_eq!(fresh.store.total_stroke_count(), 2);

        // 제목/목록
        storage.update_title(id, "renamed").expect("rename");
        let notes = storage.list_notes();
        assert!(notes.iter().any(|d| d.id == id && d.title == "renamed"));

        // 삭제
        storage.delete_document(id).expect("delete");
        let notes = storage.list_notes();
        assert!(!notes.iter().any(|d| d.id == id));
    }
}
