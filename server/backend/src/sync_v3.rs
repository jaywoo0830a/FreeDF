//! FreeDF Sync v3 — 스냅샷 중심 동기화 (docs/sync-protocol-v3.md).
//!
//! 클라이언트 관점의 규칙:
//!   저장  PUT  /v3/documents/{id}/snapshot   — 전체 문서 ZIP (비동기 처리)
//!   결과  GET  /v3/uploads/{upload_id}       — applied / conflict(패치 동봉)
//!   로딩  GET  /v3/documents/{id}            — 서버가 조립한 ZIP
//!   상태  GET  /v3/documents/{id}/revision   — 현재 revision
//!   싱크  GET  /v3/documents/{id}/changes    — since_revision 이후 변경분
//!   CAS   POST /v3/objects/query, PUT/GET /v3/objects/{digest}
//!
//! 핵심 아이디어: 충돌 시 클라이언트 diff 로직은 필요 없습니다. 업로드가
//! 어차피 전체 스냅샷이므로, 서버가 "방금 받은 클라이언트 상태"와 "서버 현재
//! 상태"를 비교해 패치를 계산하고 conflict 응답에 동봉합니다.
//! 적용·압축·diff 등 무거운 작업은 전부 백그라운드 태스크에서 실행합니다.

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use freedf_sync::{
    ChangeRecord, Conflict, CreateDocument, CreatedDocument, Digest, DigestProbe,
    DigestProbeResult, DocumentInfo, Page, Patch, RenameDocument, RevisionInfo,
    Snapshot, SnapshotMeta, Stroke, StrokePoint, UploadReceipt, UploadState, UploadStatus,
    SNAPSHOT_MIME,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use tokio_postgres::types::Json as PgJson;
use tokio_postgres::{Client, GenericClient};
use uuid::Uuid;

use crate::{check_auth, unauthorized, AppState};

// ── 오류 ─────────────────────────────────────────────────────────────────────

struct ApiError(StatusCode, String);

impl ApiError {
    fn new(code: StatusCode, msg: impl Into<String>) -> Self {
        Self(code, msg.into())
    }
    fn bad(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, msg)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.0,
            Json(freedf_sync::ApiError {
                code: self.0.as_u16().to_string(),
                message: self.1,
            }),
        )
            .into_response()
    }
}

fn db_err(e: tokio_postgres::Error) -> ApiError {
    let msg = e
        .as_db_error()
        .map(|d| format!("db: {}", d.message()))
        .unwrap_or_else(|| format!("db: {e}"));
    ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, msg)
}

fn sync_err(e: &freedf_sync::SyncError) -> ApiError {
    ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, format!("sync: {e}"))
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

// ── 서버 상태/결과 (wire 타입은 freedf-sync 크레이트와 공유) ──────────────

/// 서버가 보는 문서 상태 (diff 계산용).
#[derive(Debug, Clone)]
struct StateView {
    page_count: i32,
    strokes: Vec<Stroke>,
    pages: Vec<Page>,
    pdf_digest: Option<Digest>,
}

/// 적용 결과.
enum ApplyOutcome {
    Applied { revision: i64, patch: Value },
    Conflict { latest_revision: i64, patch: Value },
}

// ── ZIP 파싱/조립 (코덱은 freedf-sync 크레이트와 공유) ─────────────────────

/// 서버가 DB에서 문서 전체를 조회해 Snapshot으로 구성 (ZIP 직렬화 전 단계).
/// 무거운 직렬화(to_zip)는 이 함수 밖에서 — DB 락 없이 실행합니다.
async fn load_full_snapshot<C: GenericClient>(db: &C, doc_id: i64) -> Result<(Snapshot, i64), ApiError> {
    let revision: i64 = db
        .query_opt("SELECT revision FROM doc_revisions WHERE doc_id=$1", &[&doc_id])
        .await
        .map_err(db_err)?
        .map(|r| r.get(0))
        .unwrap_or(0);

    let doc = db
        .query_opt(
            "SELECT title, kind, page_count, pdf_digest, updated_at FROM documents WHERE id=$1",
            &[&doc_id],
        )
        .await
        .map_err(db_err)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "document not found"))?;
    let title: String = doc.get(0);
    let kind: String = doc.get(1);
    let page_count: i32 = doc.get(2);
    let pdf_digest: Option<String> = doc.get(3);
    let updated_at: i64 = doc.get(4);

    let state = load_state(db, doc_id).await?;
    let edits: Vec<Value> = db
        .query("SELECT edit FROM doc_edits WHERE doc_id=$1 ORDER BY id", &[&doc_id])
        .await
        .map_err(db_err)?
        .iter()
        .map(|r| r.get(0))
        .collect();
    let session: Option<Value> = db
        .query_opt("SELECT state FROM sessions WHERE doc_id=$1", &[&doc_id])
        .await
        .map_err(db_err)?
        .map(|r| r.get(0));
    let meta = SnapshotMeta {
        revision: Some(revision),
        base_revision: None,
        page_count,
        updated_at,
        title,
        kind,
        pdf_digest: pdf_digest.and_then(|s| Digest::parse(&s).ok()),
        session,
    };
    let snap = Snapshot {
        meta,
        strokes: state.strokes,
        pages: state.pages,
        pdf_digest: state.pdf_digest,
        edits,
    };
    Ok((snap, revision))
}

/// 리비전 일치 시에만 유효한 조립 ZIP 캐시 조회.
fn cached_zip(state: &AppState, doc_id: i64, revision: i64) -> Option<Vec<u8>> {
    state
        .zip_cache
        .lock()
        .unwrap()
        .get(&doc_id)
        .and_then(|(rev, bytes)| if *rev == revision { Some(bytes.clone()) } else { None })
}

/// 문서 전체 ZIP — 캐시 히트 시 즉시, 미스 시 조회(DB 락) + 조립(락 밖).
/// 조립은 리비전별 1회만 실행됩니다 (이후 요청/304는 캐시 경로).
async fn assemble_snapshot(state: &AppState, doc_id: i64, revision: i64) -> Result<Vec<u8>, ApiError> {
    if let Some(bytes) = cached_zip(state, doc_id, revision) {
        return Ok(bytes);
    }
    let (snap, rev) = {
        let db = state.db.lock().await;
        load_full_snapshot(&*db, doc_id).await?
    };
    // 직렬화·압축은 DB 연결을 점유하지 않습니다 (다른 요청이 계속 처리됨).
    let bytes = snap.to_zip().map_err(|e| sync_err(&e))?;
    state.zip_cache.lock().unwrap().insert(doc_id, (rev, bytes.clone()));
    Ok(bytes)
}

// ── 상태 조회/비교/적용 ──────────────────────────────────────────────────────

/// DB에서 현재 상태를 읽는다.
async fn load_state<C: GenericClient>(db: &C, doc_id: i64) -> Result<StateView, ApiError> {
    let doc = db
        .query_opt("SELECT page_count, pdf_digest FROM documents WHERE id=$1", &[&doc_id])
        .await
        .map_err(db_err)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "document not found"))?;
    let page_count: i32 = doc.get(0);
    let pdf_digest: Option<String> = doc.get(1);
    let pdf_digest = pdf_digest.and_then(|s| Digest::parse(&s).ok());

    let stroke_rows = db
        .query(
            "SELECT id, page_index, tool, color, width, points, created_at \
             FROM strokes WHERE doc_id=$1 ORDER BY id",
            &[&doc_id],
        )
        .await
        .map_err(db_err)?;
    let strokes: Vec<Stroke> = stroke_rows
        .iter()
        .map(|r| {
            let pts: PgJson<Vec<StrokePoint>> = r.get(5);
            Stroke {
                id: r.get(0),
                page_index: r.get(1),
                tool: r.get(2),
                color: r.get(3),
                width: r.get(4),
                points: pts.0,
                created_at: r.get(6),
            }
        })
        .collect();

    let page_rows = db
        .query(
            "SELECT page_index, style, color, bookmarked FROM pages WHERE doc_id=$1 ORDER BY page_index",
            &[&doc_id],
        )
        .await
        .map_err(db_err)?;
    let pages: Vec<Page> = page_rows
        .iter()
        .map(|r| Page {
            page_index: r.get(0),
            style: r.get(1),
            color: r.get(2),
            bookmarked: r.get(3),
        })
        .collect();

    Ok(StateView {
        page_count,
        strokes,
        pages,
        pdf_digest,
    })
}

/// old → new diff.
fn diff_states(from: i64, to: i64, old: &StateView, new: &StateView) -> Patch {
    let old_ids: BTreeSet<i64> = old.strokes.iter().map(|s| s.id).collect();
    let new_ids: BTreeSet<i64> = new.strokes.iter().map(|s| s.id).collect();
    let strokes_added: Vec<Stroke> = new
        .strokes
        .iter()
        .filter(|s| !old_ids.contains(&s.id))
        .cloned()
        .collect();
    let stroke_ids_removed: Vec<i64> = old
        .strokes
        .iter()
        .filter(|s| !new_ids.contains(&s.id))
        .map(|s| s.id)
        .collect();
    let pages_changed = old.pages != new.pages;
    let meta = if old.page_count != new.page_count {
        json!({"page_count": new.page_count})
    } else {
        Value::Null
    };
    let pdf = if old.pdf_digest != new.pdf_digest {
        new.pdf_digest.clone()
    } else {
        None
    };
    Patch {
        from_revision: from,
        to_revision: to,
        strokes_added,
        stroke_ids_removed,
        pages_changed,
        pages: if pages_changed { new.pages.clone() } else { Vec::new() },
        meta,
        pdf,
    }
}

/// 클라이언트 상태(전체 스냅샷)를 기준으로 서버 상태로 가는 충돌 패치.
///
/// 획은 합집합 병합: 서버에만 있는 획은 add, 클라이언트에만 있는 획은
/// (서버가 모르므로) 건드리지 않습니다 — 재전송 후 정상 적용됩니다.
fn conflict_patch(cur: i64, old: &StateView, snap: &Snapshot) -> Patch {
    let client_ids: BTreeSet<i64> = snap.strokes.iter().map(|s| s.id).collect();
    let strokes_added: Vec<Stroke> = old
        .strokes
        .iter()
        .filter(|s| !client_ids.contains(&s.id))
        .cloned()
        .collect();
    let pages_changed = snap.pages != old.pages;
    let meta = if snap.meta.page_count != old.page_count {
        json!({"page_count": old.page_count})
    } else {
        Value::Null
    };
    let pdf = if snap.pdf_digest != old.pdf_digest {
        old.pdf_digest.clone()
    } else {
        None
    };
    Patch {
        from_revision: snap.base_revision().unwrap_or(0),
        to_revision: cur,
        strokes_added,
        stroke_ids_removed: Vec::new(),
        pages_changed,
        pages: if pages_changed { old.pages.clone() } else { Vec::new() },
        meta,
        pdf,
    }
}

/// 스냅샷을 DB에 반영 (트랜잭션 안에서 호출).
async fn apply_to_db<C: GenericClient>(db: &C, doc_id: i64, snap: &Snapshot, old: &StateView) -> Result<(), ApiError> {
    // 획 전체 교체 — f32 최단 표현으로 통째 직렬화해 $2::jsonb로 파싱
    // (to_value는 f32→f64 확장으로 jsonb 256MB 한도를 넘을 수 있음).
    db.execute("DELETE FROM strokes WHERE doc_id=$1", &[&doc_id])
        .await
        .map_err(db_err)?;
    if !snap.strokes.is_empty() {
        let arr_text: String = serde_json::to_string(&snap.strokes).expect("stroke serialize");
        db.execute(
            "INSERT INTO strokes (id, doc_id, page_index, tool, color, width, points, created_at) \
             SELECT (s->>'id')::bigint, $1, (s->>'page_index')::int, s->>'tool', \
                    ARRAY(SELECT jsonb_array_elements_text(s->'color')::int), \
                    COALESCE((s->>'width')::real, 0), s->'points', \
                    COALESCE((s->>'created_at')::bigint, 0) \
             FROM jsonb_array_elements($2::text::jsonb) s \
             ON CONFLICT (doc_id, id) DO NOTHING",
            &[&doc_id, &arr_text],
        )
        .await
        .map_err(db_err)?;
    }

    // 페이지 전체 교체
    db.execute("DELETE FROM pages WHERE doc_id=$1", &[&doc_id])
        .await
        .map_err(db_err)?;
    if !snap.pages.is_empty() {
        let arr = Value::Array(
            snap.pages
                .iter()
                .map(|p| serde_json::to_value(p).expect("page serialize"))
                .collect(),
        );
        db.execute(
            "INSERT INTO pages (doc_id, page_index, style, color, bookmarked) \
             SELECT $1, (p->>'page_index')::int, p->>'style', \
                    ARRAY(SELECT jsonb_array_elements_text(p->'color')::int), \
                    COALESCE((p->>'bookmarked')::boolean, false) \
             FROM jsonb_array_elements($2::jsonb) p",
            &[&doc_id, &arr],
        )
        .await
        .map_err(db_err)?;
    }

    // PDF: 다이제스트가 바뀌었으면 CAS에서 바이트를 가져와 반영.
    if let Some(digest) = &snap.pdf_digest {
        if old.pdf_digest.as_ref() != Some(digest) {
            let d: &str = digest.as_str();
            let row = db
                .query_opt("SELECT bytes FROM cas_objects WHERE digest=$1", &[&d])
                .await
                .map_err(db_err)?
                .ok_or_else(|| ApiError::bad(format!("pdf object {digest} not in CAS — PUT /v3/objects/{digest} first")))?;
            let bytes: Vec<u8> = row.get(0);
            db.execute(
                "UPDATE documents SET pdf=$2, pdf_digest=$3 WHERE id=$1",
                &[&doc_id, &bytes, &d],
            )
            .await
            .map_err(db_err)?;
        }
    }

    // 문서 메타
    db.execute(
        "UPDATE documents SET page_count=$2, updated_at=$3 WHERE id=$1",
        &[&doc_id, &snap.meta.page_count, &now_ms()],
    )
    .await
    .map_err(db_err)?;

    // 편집 저널(undo) 전체 교체
    db.execute("DELETE FROM doc_edits WHERE doc_id=$1", &[&doc_id])
        .await
        .map_err(db_err)?;
    if !snap.edits.is_empty() {
        let arr_text: String = serde_json::to_string(&snap.edits).expect("edits serialize");
        db.execute(
            "INSERT INTO doc_edits (doc_id, edit, created_at) \
             SELECT $1, e, $2 FROM jsonb_array_elements($3::text::jsonb) e",
            &[&doc_id, &now_ms(), &arr_text],
        )
        .await
        .map_err(db_err)?;
    }

    // 문서 GUI 세션
    if let Some(session) = &snap.meta.session {
        db.execute(
            "INSERT INTO sessions (doc_id, state, updated_at) VALUES ($1, $2, $3) \
             ON CONFLICT (doc_id) DO UPDATE SET state = EXCLUDED.state, updated_at = EXCLUDED.updated_at",
            &[&doc_id, session, &now_ms()],
        )
        .await
        .map_err(db_err)?;
    }

    Ok(())
}

/// 업로드 적용 (백그라운드 태스크, apply 전용 연결에서 실행).
async fn apply_snapshot(
    state: &AppState,
    db: &mut Client,
    doc_id: i64,
    snap: Snapshot,
) -> Result<ApplyOutcome, ApiError> {
    let tx = db.transaction().await.map_err(db_err)?;

    let cur: i64 = tx
        .query_opt("SELECT revision FROM doc_revisions WHERE doc_id=$1", &[&doc_id])
        .await
        .map_err(db_err)?
        .map(|r| r.get(0))
        .unwrap_or(0);

    let old = load_state(&tx, doc_id).await?;

    // ── 충돌: base_revision 불일치 → 서버 계산 패치만 반환 ──
    let base = snap
        .base_revision()
        .ok_or_else(|| ApiError::bad("meta.json: base_revision missing"))?;
    if base != cur {
        let patch = conflict_patch(cur, &old, &snap);
        let patch_val = serde_json::to_value(&patch).expect("patch serialize");
        tx.commit().await.map_err(db_err)?;
        return Ok(ApplyOutcome::Conflict {
            latest_revision: cur,
            patch: patch_val,
        });
    }

    // ── 적용 ──
    apply_to_db(&tx, doc_id, &snap, &old).await?;
    let new_rev = cur + 1;
    let new = StateView {
        page_count: snap.meta.page_count,
        strokes: snap.strokes.clone(),
        pages: snap.pages.clone(),
        pdf_digest: snap.pdf_digest.clone(),
    };
    let patch = diff_states(cur, new_rev, &old, &new);
    let patch_val = serde_json::to_value(&patch).expect("patch serialize");

    tx.execute(
        "INSERT INTO doc_revisions (doc_id, revision, updated_at) VALUES ($1, $2, $3) \
         ON CONFLICT (doc_id) DO UPDATE SET revision = EXCLUDED.revision, updated_at = EXCLUDED.updated_at",
        &[&doc_id, &new_rev, &now_ms()],
    )
    .await
    .map_err(db_err)?;

    tx.execute(
        "INSERT INTO doc_changelog (doc_id, revision, patch, created_at) VALUES ($1, $2, $3, $4)",
        &[&doc_id, &new_rev, &patch_val, &now_ms()],
    )
    .await
    .map_err(db_err)?;

    tx.commit().await.map_err(db_err)?;
    // 서버 상태가 바뀌었으므로 조립 ZIP 캐시 무효화.
    state.zip_cache.lock().unwrap().remove(&doc_id);
    Ok(ApplyOutcome::Applied {
        revision: new_rev,
        patch: patch_val,
    })
}

// ── 문서 CRUD ───────────────────────────────────────────────────────────────

/// GET /v3/documents — 문서 목록 (라이브러리/최근).
pub async fn list_documents(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Response {
    if !check_auth(&state, &headers).await {
        return unauthorized();
    }
    let db = state.db.lock().await;
    let rows = match db
        .query(
            "SELECT id, kind, title, origin_path, page_count, created_at, updated_at, pdf_digest \
             FROM documents ORDER BY id",
            &[],
        )
        .await
    {
        Ok(r) => r,
        Err(e) => return db_err(e).into_response(),
    };
    let docs: Vec<DocumentInfo> = rows
        .iter()
        .map(|r| {
            let digest: Option<String> = r.get(7);
            DocumentInfo {
                id: r.get(0),
                kind: r.get(1),
                title: r.get(2),
                origin_path: r.get(3),
                page_count: r.get(4),
                created_at: r.get(5),
                updated_at: r.get(6),
                pdf_digest: digest.and_then(|s| Digest::parse(&s).ok()),
            }
        })
        .collect();
    Json(docs).into_response()
}

/// POST /v3/documents — 문서 생성 (PDF는 CAS 다이제스트 참조).
pub async fn create_document(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<CreateDocument>,
) -> Response {
    if !check_auth(&state, &headers).await {
        return unauthorized();
    }
    let db = state.db.lock().await;
    // PDF: CAS에서 바이트를 가져옵니다.
    let (pdf_bytes, digest): (Option<Vec<u8>>, Option<String>) = match &req.pdf_digest {
        Some(d) => {
            let d: &str = d.as_str();
            let row = match db
                .query_opt("SELECT bytes FROM cas_objects WHERE digest=$1", &[&d])
                .await
            {
                Ok(r) => r,
                Err(e) => return db_err(e).into_response(),
            };
            let Some(row) = row else {
                return ApiError::bad(format!("pdf object {d} not in CAS — PUT /v3/objects/{d} first"))
                    .into_response();
            };
            (Some(row.get(0)), Some(d.to_string()))
        }
        None => (None, None),
    };
    let now = now_ms();
    let row = match db
        .query_one(
            "INSERT INTO documents (kind, title, origin_path, page_count, pdf, pdf_digest, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $7) RETURNING id",
            &[&req.kind, &req.title, &req.origin_path, &req.page_count, &pdf_bytes, &digest, &now],
        )
        .await
    {
        Ok(r) => r,
        Err(e) => return db_err(e).into_response(),
    };
    Json(CreatedDocument { id: row.get(0) }).into_response()
}

/// PUT /v3/documents/{id}/title — 제목 변경.
pub async fn rename_document(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(doc_id): Path<i64>,
    Json(req): Json<RenameDocument>,
) -> Response {
    if !check_auth(&state, &headers).await {
        return unauthorized();
    }
    let db = state.db.lock().await;
    match db
        .execute(
            "UPDATE documents SET title=$2, updated_at=$3 WHERE id=$1",
            &[&doc_id, &req.title, &now_ms()],
        )
        .await
    {
        Ok(0) => ApiError::new(StatusCode::NOT_FOUND, "document not found").into_response(),
        Ok(_) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => db_err(e).into_response(),
    }
}

/// DELETE /v3/documents/{id} — 문서 삭제 (CASCADE).
pub async fn delete_document(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(doc_id): Path<i64>,
) -> Response {
    if !check_auth(&state, &headers).await {
        return unauthorized();
    }
    let db = state.db.lock().await;
    match db
        .execute("DELETE FROM documents WHERE id=$1", &[&doc_id])
        .await
    {
        Ok(0) => ApiError::new(StatusCode::NOT_FOUND, "document not found").into_response(),
        Ok(_) => {
            state.zip_cache.lock().unwrap().remove(&doc_id);
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => db_err(e).into_response(),
    }
}

// ── 핸들러 ───────────────────────────────────────────────────────────────────

/// PUT /v3/documents/{id}/snapshot — 전체 문서 ZIP 업로드 (비동기).
pub async fn put_snapshot(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(doc_id): Path<i64>,
    body: Bytes,
) -> Response {
    if !check_auth(&state, &headers).await {
        return unauthorized();
    }
    let upload_id = Uuid::new_v4();
    {
        let db = state.db.lock().await;
        let exists = db
            .query_opt("SELECT 1 FROM documents WHERE id=$1", &[&doc_id])
            .await
            .map_err(db_err);
        let exists = match exists {
            Ok(r) => r.is_some(),
            Err(e) => return e.into_response(),
        };
        if !exists {
            return ApiError::new(StatusCode::NOT_FOUND, "document not found").into_response();
        }
        // 멱등: 같은 upload_id 재전송은 no-op.
        let inserted = db
            .execute(
                "INSERT INTO sync_uploads (upload_id, doc_id, state, created_at) \
                 VALUES ($1, $2, 'queued', $3) ON CONFLICT (upload_id) DO NOTHING",
                &[&upload_id, &doc_id, &now_ms()],
            )
            .await
            .map_err(db_err);
        match inserted {
            Ok(0) => {
                // 이미 존재하는 작업 — 현재 상태를 그대로 반환.
                let row = db
                    .query_opt("SELECT state, revision, patch, error FROM sync_uploads WHERE upload_id=$1", &[&upload_id])
                    .await
                    .map_err(db_err);
                return match row {
                    Ok(Some(r)) => upload_status_response(upload_id, &r),
                    Ok(None) => ApiError::new(StatusCode::NOT_FOUND, "upload not found").into_response(),
                    Err(e) => e.into_response(),
                };
            }
            Ok(_) => {}
            Err(e) => return e.into_response(),
        }
    }

    // 무거운 작업(파싱·적용)은 apply 전용 연결 + 백그라운드 태스크에서 —
    // 일반 요청(상태 폴링/다운로드)은 메인 연결을 계속 씁니다.
    let state2 = state.clone();
    tokio::spawn(async move {
        let mut apply_db = state2.apply_db.lock().await;
        let _ = apply_db
            .execute("UPDATE sync_uploads SET state='processing' WHERE upload_id=$1", &[&upload_id])
            .await;
        let outcome = match Snapshot::from_zip(&body) {
            Ok(snap) => apply_snapshot(&state2, &mut apply_db, doc_id, snap).await,
            Err(e) => Err(ApiError::bad(format!("snapshot: {e}"))),
        };
        let _ = match outcome {
            Ok(ApplyOutcome::Applied { revision, patch }) => {
                apply_db
                    .execute(
                        "UPDATE sync_uploads SET state='applied', revision=$1, patch=$2 WHERE upload_id=$3",
                        &[&revision, &patch, &upload_id],
                    )
                    .await
            }
            Ok(ApplyOutcome::Conflict { latest_revision, patch }) => {
                apply_db
                    .execute(
                        "UPDATE sync_uploads SET state='conflict', revision=$1, patch=$2 WHERE upload_id=$3",
                        &[&latest_revision, &patch, &upload_id],
                    )
                    .await
            }
            Err(e) => {
                apply_db
                    .execute(
                        "UPDATE sync_uploads SET state='failed', error=$1 WHERE upload_id=$2",
                        &[&e.1, &upload_id],
                    )
                    .await
            }
        };
    });

    (
        StatusCode::ACCEPTED,
        Json(UploadReceipt {
            upload_id,
            state: UploadState::Queued,
        }),
    )
        .into_response()
}

fn empty_patch() -> Patch {
    Patch {
        from_revision: 0,
        to_revision: 0,
        strokes_added: Vec::new(),
        stroke_ids_removed: Vec::new(),
        pages_changed: false,
        pages: Vec::new(),
        meta: Value::Null,
        pdf: None,
    }
}

fn upload_status_response(upload_id: Uuid, row: &tokio_postgres::Row) -> Response {
    let state: String = row.get(0);
    let revision: Option<i64> = row.get(1);
    let patch: Option<Value> = row.get(2);
    let error: Option<String> = row.get(3);
    let state = match state.as_str() {
        "applied" => UploadState::Applied,
        "processing" => UploadState::Processing,
        "queued" => UploadState::Queued,
        "conflict" => UploadState::Conflict,
        "failed" => UploadState::Failed,
        _ => UploadState::Unknown,
    };
    let conflict = if state == UploadState::Conflict {
        patch.map(|p| Conflict {
            latest_revision: revision.unwrap_or(0),
            patch: serde_json::from_value::<Patch>(p).unwrap_or_else(|_| empty_patch()),
        })
    } else {
        None
    };
    Json(UploadStatus {
        upload_id,
        state,
        revision,
        conflict,
        error,
    })
    .into_response()
}

/// GET /v3/uploads/{upload_id} — 비동기 업로드 결과 조회.
pub async fn get_upload_status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(upload_id): Path<Uuid>,
) -> Response {
    if !check_auth(&state, &headers).await {
        return unauthorized();
    }
    let db = state.db.lock().await;
    match db
        .query_opt(
            "SELECT state, revision, patch, error FROM sync_uploads WHERE upload_id=$1",
            &[&upload_id],
        )
        .await
    {
        Ok(Some(row)) => upload_status_response(upload_id, &row),
        Ok(None) => ApiError::new(StatusCode::NOT_FOUND, "upload not found").into_response(),
        Err(e) => db_err(e).into_response(),
    }
}

/// GET /v3/documents/{id} — 서버가 조립한 전체 ZIP (ETag/304 지원).
///
/// 304 판정은 무거운 조립 **전에** — 리비전 조회 한 번으로 끝납니다.
/// 이후 요청은 리비전별 ZIP 캐시(재조립 없음)를 사용합니다.
pub async fn download_snapshot(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(doc_id): Path<i64>,
) -> Response {
    if !check_auth(&state, &headers).await {
        return unauthorized();
    }
    // 1) 문서 존재 + 리비전만 조회 (조립 전 304 판정).
    let revision = {
        let db = state.db.lock().await;
        match db
            .query_opt(
                "SELECT d.id, COALESCE(r.revision, 0) FROM documents d \
                 LEFT JOIN doc_revisions r ON r.doc_id = d.id WHERE d.id=$1",
                &[&doc_id],
            )
            .await
        {
            Ok(Some(row)) => row.get::<_, i64>(1),
            Ok(None) => {
                return ApiError::new(StatusCode::NOT_FOUND, "document not found").into_response()
            }
            Err(e) => return db_err(e).into_response(),
        }
    };
    let etag = format!("\"rev-{revision}\"");
    if headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        == Some(etag.as_str())
    {
        return StatusCode::NOT_MODIFIED.into_response();
    }
    // 2) 캐시/조립 (ZIP 직렬화는 DB 락 밖).
    let zip = match assemble_snapshot(&state, doc_id, revision).await {
        Ok(v) => v,
        Err(e) => return e.into_response(),
    };
    let mut resp = ([(header::CONTENT_TYPE, SNAPSHOT_MIME)], zip).into_response();
    if let Ok(v) = etag.parse() {
        resp.headers_mut().insert(header::ETAG, v);
    }
    resp
}

/// GET /v3/documents/{id}/pdf — 원본 PDF 다운로드 (ETag `"pdf-<digest>"`).
///
/// PDF 본문은 스냅샷 ZIP이 아니라 여기서 직접 받습니다 — "클라우드처럼"
/// 다른 기기에서 원본 파일을 내려받는 경로 (CAS 다이제스트를 몰라도 됨).
pub async fn get_pdf(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(doc_id): Path<i64>,
) -> Response {
    if !check_auth(&state, &headers).await {
        return unauthorized();
    }
    let (pdf, digest) = {
        let db = state.db.lock().await;
        match db
            .query_opt("SELECT pdf, pdf_digest FROM documents WHERE id=$1", &[&doc_id])
            .await
        {
            Ok(Some(row)) => {
                let pdf: Option<Vec<u8>> = row.get(0);
                let digest: Option<String> = row.get(1);
                match (pdf, digest) {
                    (Some(bytes), Some(d)) => (bytes, d),
                    _ => {
                        return ApiError::new(StatusCode::NOT_FOUND, "document has no PDF")
                            .into_response()
                    }
                }
            }
            Ok(None) => {
                return ApiError::new(StatusCode::NOT_FOUND, "document not found").into_response()
            }
            Err(e) => return db_err(e).into_response(),
        }
    };
    let etag = format!("\"pdf-{digest}\"");
    if headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        == Some(etag.as_str())
    {
        return StatusCode::NOT_MODIFIED.into_response();
    }
    let mut resp = ([(header::CONTENT_TYPE, "application/pdf")], pdf).into_response();
    if let Ok(v) = etag.parse() {
        resp.headers_mut().insert(header::ETAG, v);
    }
    resp
}

/// PUT /v3/documents/{id}/pdf — 원본 PDF 업로드/교체 (멱등).
///
/// 내용이 동일하면 revision 그대로, 바뀌면 CAS 저장 + documents 갱신 +
/// revision+1 + PdfChanged 변경 로그 — 다른 기기가 pull로 수신합니다.
pub async fn put_pdf(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(doc_id): Path<i64>,
    body: Bytes,
) -> Response {
    if !check_auth(&state, &headers).await {
        return unauthorized();
    }
    if body.is_empty() {
        return ApiError::bad("empty pdf body").into_response();
    }
    let digest = Digest::from_bytes(&body);
    let mut db = state.apply_db.lock().await;
    let tx = match db.transaction().await {
        Ok(tx) => tx,
        Err(e) => return db_err(e).into_response(),
    };
    let cur_digest: Option<String> = match tx
        .query_opt("SELECT pdf_digest FROM documents WHERE id=$1", &[&doc_id])
        .await
    {
        Ok(Some(row)) => row.get(0),
        Ok(None) => {
            return ApiError::new(StatusCode::NOT_FOUND, "document not found").into_response()
        }
        Err(e) => return db_err(e).into_response(),
    };
    let cur_rev: i64 = match tx
        .query_opt("SELECT revision FROM doc_revisions WHERE doc_id=$1", &[&doc_id])
        .await
    {
        Ok(Some(row)) => row.get(0),
        Ok(None) => 0,
        Err(e) => return db_err(e).into_response(),
    };
    let size = body.len() as i64;

    // 멱등 — 내용 동일하면 변경 없이 현재 revision 그대로.
    if cur_digest.as_deref() == Some(digest.as_str()) {
        if let Err(e) = tx.commit().await {
            return db_err(e).into_response();
        }
        return Json(freedf_sync::PdfInfo {
            digest,
            size,
            revision: cur_rev,
        })
        .into_response();
    }

    let slice: &[u8] = &body;
    let now = now_ms();
    if let Err(e) = tx
        .execute(
            "INSERT INTO cas_objects (digest, bytes, size, created_at) VALUES ($1, $2, $3, $4) \
             ON CONFLICT (digest) DO NOTHING",
            &[&digest.as_str(), &slice, &size, &now],
        )
        .await
    {
        return db_err(e).into_response();
    }
    if let Err(e) = tx
        .execute(
            "UPDATE documents SET pdf=$2, pdf_digest=$3, updated_at=$4 WHERE id=$1",
            &[&doc_id, &slice, &digest.as_str(), &now],
        )
        .await
    {
        return db_err(e).into_response();
    }
    let new_rev = cur_rev + 1;
    let patch = Patch {
        from_revision: cur_rev,
        to_revision: new_rev,
        strokes_added: Vec::new(),
        stroke_ids_removed: Vec::new(),
        pages_changed: false,
        pages: Vec::new(),
        meta: Value::Null,
        pdf: Some(digest.clone()),
    };
    let patch_val = serde_json::to_value(&patch).expect("patch serialize");
    if let Err(e) = tx
        .execute(
            "INSERT INTO doc_revisions (doc_id, revision, updated_at) VALUES ($1, $2, $3) \
             ON CONFLICT (doc_id) DO UPDATE SET revision = EXCLUDED.revision, updated_at = EXCLUDED.updated_at",
            &[&doc_id, &new_rev, &now],
        )
        .await
    {
        return db_err(e).into_response();
    }
    if let Err(e) = tx
        .execute(
            "INSERT INTO doc_changelog (doc_id, revision, patch, created_at) VALUES ($1, $2, $3, $4)",
            &[&doc_id, &new_rev, &patch_val, &now],
        )
        .await
    {
        return db_err(e).into_response();
    }
    if let Err(e) = tx.commit().await {
        return db_err(e).into_response();
    }
    state.zip_cache.lock().unwrap().remove(&doc_id); // meta.pdf_digest가 바뀜
    Json(freedf_sync::PdfInfo {
        digest,
        size,
        revision: new_rev,
    })
    .into_response()
}

/// GET /v3/documents/{id}/revision — 현재 revision.
pub async fn get_revision(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(doc_id): Path<i64>,
) -> Response {
    if !check_auth(&state, &headers).await {
        return unauthorized();
    }
    let db = state.db.lock().await;
    match db
        .query_opt(
            "SELECT d.id, COALESCE(r.revision, 0) FROM documents d \
             LEFT JOIN doc_revisions r ON r.doc_id = d.id WHERE d.id=$1",
            &[&doc_id],
        )
        .await
    {
        Ok(Some(row)) => {
            Json(RevisionInfo {
                document_id: row.get(0),
                revision: row.get(1),
            })
            .into_response()
        }
        Ok(None) => ApiError::new(StatusCode::NOT_FOUND, "document not found").into_response(),
        Err(e) => db_err(e).into_response(),
    }
}

#[derive(Deserialize)]
pub struct ChangesParams {
    since_revision: i64,
    format: Option<String>,
}

/// GET /v3/documents/{id}/changes?since_revision=&format= — 변경분 pull.
pub async fn get_changes(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(doc_id): Path<i64>,
    Query(params): Query<ChangesParams>,
) -> Response {
    if !check_auth(&state, &headers).await {
        return unauthorized();
    }
    let db = state.db.lock().await;

    let exists = match db
        .query_opt("SELECT 1 FROM documents WHERE id=$1", &[&doc_id])
        .await
    {
        Ok(r) => r.is_some(),
        Err(e) => return db_err(e).into_response(),
    };
    if !exists {
        return ApiError::new(StatusCode::NOT_FOUND, "document not found").into_response();
    }

    // format=zip — 폴백: 전체 스냅샷 (다운로드와 같은 캐시 경로).
    if params.format.as_deref() == Some("zip") {
        let revision = match db
            .query_opt("SELECT revision FROM doc_revisions WHERE doc_id=$1", &[&doc_id])
            .await
        {
            Ok(Some(r)) => r.get(0),
            Ok(None) => 0,
            Err(e) => return db_err(e).into_response(),
        };
        drop(db); // 캐시/조립 경로가 필요할 때만 DB 락 재획득.
        return match assemble_snapshot(&state, doc_id, revision).await {
            Ok(zip) => ([(header::CONTENT_TYPE, SNAPSHOT_MIME)], zip).into_response(),
            Err(e) => e.into_response(),
        };
    }

    let rows = match db
        .query(
            "SELECT revision, patch FROM doc_changelog \
             WHERE doc_id=$1 AND revision > $2 ORDER BY revision",
            &[&doc_id, &params.since_revision],
        )
        .await
    {
        Ok(r) => r,
        Err(e) => return db_err(e).into_response(),
    };

    let mut body = String::new();
    for row in rows {
        let patch: Value = row.get(1);
        let Ok(patch) = serde_json::from_value::<Patch>(patch) else {
            continue; // 손상된 로그는 건너뜀 (치명적이지 않음)
        };
        for id in &patch.stroke_ids_removed {
            let rec = ChangeRecord::StrokeRemoved { id: *id };
            body.push_str(&serde_json::to_string(&rec).unwrap());
            body.push('\n');
        }
        for s in &patch.strokes_added {
            let rec = ChangeRecord::StrokeAdded { stroke: s.clone() };
            body.push_str(&serde_json::to_string(&rec).unwrap());
            body.push('\n');
        }
        if patch.pages_changed {
            let rec = ChangeRecord::PagesChanged {
                pages: patch.pages.clone(),
            };
            body.push_str(&serde_json::to_string(&rec).unwrap());
            body.push('\n');
        }
        if !patch.meta.is_null() {
            let rec = ChangeRecord::MetaChanged {
                meta: patch.meta.clone(),
            };
            body.push_str(&serde_json::to_string(&rec).unwrap());
            body.push('\n');
        }
        if let Some(digest) = &patch.pdf {
            let rec = ChangeRecord::PdfChanged {
                pdf: digest.clone(),
            };
            body.push_str(&serde_json::to_string(&rec).unwrap());
            body.push('\n');
        }
    }

    ([(header::CONTENT_TYPE, "application/jsonl")], body).into_response()
}

/// POST /v3/objects/query — 서버에 없는 다이제스트만 골라준다.
pub async fn probe_objects(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(probe): Json<DigestProbe>,
) -> Response {
    if !check_auth(&state, &headers).await {
        return unauthorized();
    }
    let db = state.db.lock().await;
    let keys: Vec<String> = probe.digests.iter().map(|d| d.as_str().to_string()).collect();
    let rows = match db
        .query("SELECT digest FROM cas_objects WHERE digest = ANY($1)", &[&keys])
        .await
    {
        Ok(r) => r,
        Err(e) => return db_err(e).into_response(),
    };
    let found: BTreeSet<String> = rows.iter().map(|r| r.get(0)).collect();
    let missing: Vec<Digest> = probe
        .digests
        .iter()
        .filter(|d| !found.contains(d.as_str()))
        .cloned()
        .collect();
    Json(DigestProbeResult { missing }).into_response()
}

/// PUT /v3/objects/{digest} — CAS 업로드 (멱등, 본문-다이제스트 검증).
pub async fn put_object(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(digest): Path<String>,
    body: Bytes,
) -> Response {
    if !check_auth(&state, &headers).await {
        return unauthorized();
    }
    let parsed = match Digest::parse(&digest) {
        Ok(d) => d,
        Err(e) => return ApiError::bad(e.to_string()).into_response(),
    };
    if Digest::from_bytes(&body) != parsed {
        return ApiError::bad("body does not match digest").into_response();
    }
    let db = state.db.lock().await;
    let slice: &[u8] = &body;
    match db
        .execute(
            "INSERT INTO cas_objects (digest, bytes, size, created_at) VALUES ($1, $2, $3, $4) \
             ON CONFLICT (digest) DO NOTHING",
            &[&digest, &slice, &(body.len() as i64), &now_ms()],
        )
        .await
    {
        Ok(_) => Json(freedf_sync::ObjectInfo {
            digest: parsed,
            size: body.len() as u64,
        })
        .into_response(),
        Err(e) => db_err(e).into_response(),
    }
}

/// GET /v3/objects/{digest} — CAS fetch.
pub async fn get_object(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(digest): Path<String>,
) -> Response {
    if !check_auth(&state, &headers).await {
        return unauthorized();
    }
    let db = state.db.lock().await;
    match db
        .query_opt("SELECT bytes FROM cas_objects WHERE digest=$1", &[&digest])
        .await
    {
        Ok(Some(row)) => {
            let bytes: Vec<u8> = row.get(0);
            ([(header::CONTENT_TYPE, "application/octet-stream")], bytes).into_response()
        }
        Ok(None) => ApiError::new(StatusCode::NOT_FOUND, "object not found").into_response(),
        Err(e) => db_err(e).into_response(),
    }
}
