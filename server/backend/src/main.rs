//! FreeDF 미디어 리소스 서버 — 자체 호스팅용.
//!
//! nginx가 `/media/*` 정적 파일을 직접 서빙(Range 스트리밍)하고,
//! 이 서버는 업로드/목록/삭제와 메타데이터(PostgreSQL)만 담당합니다.
//!
//! 모든 API 호출에는 `X-Api-Key: <FREEDF_API_KEY>` 헤더가 필요합니다.

use axum::{
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{delete, get, post, put},
    Router,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_postgres::Client;

mod sync_v3;

// ── 앱 상태 ──────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct AppState {
    db: Arc<Mutex<Client>>,
    media_dir: PathBuf,
    public_base: String,
    api_key: String,
}

#[derive(Serialize)]
struct MediaObject {
    id: i64,
    doc_id: Option<i64>,
    kind: String,
    name: String,
    mime: String,
    size: i64,
    url: String,
}

#[derive(Deserialize)]
struct UploadParams {
    doc_id: Option<i64>,
    kind: Option<String>,
}

#[derive(Deserialize)]
struct ListParams {
    doc_id: Option<i64>,
    limit: Option<i64>,
    offset: Option<i64>,
}

// ── main ─────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    let database_url = env("DATABASE_URL", "postgres://freedf:freedf@localhost:5432/freedf");
    let media_dir = env("MEDIA_DIR", "/srv/freedf-server/media");
    let public_base = env("PUBLIC_BASE_URL", "https://media.example.com");
    let api_key = env("FREEDF_API_KEY", "");
    let bind = env("BIND", "127.0.0.1:8080");

    if api_key.is_empty() {
        eprintln!("FREEDF_API_KEY가 설정되지 않았습니다 — 기동을 거부합니다.");
        std::process::exit(1);
    }

    let (client, connection) = tokio_postgres::connect(&database_url, tokio_postgres::NoTls)
        .await
        .expect("PostgreSQL 연결 실패 — server/db/up.sh로 DB를 먼저 띄우세요");
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("DB connection lost: {e}");
        }
    });

    // 스키마(media_objects 포함)는 server/db/up.sh의 마이그레이션이 담당합니다.

    let state = AppState {
        db: Arc::new(Mutex::new(client)),
        media_dir: PathBuf::from(media_dir),
        public_base: public_base.trim_end_matches('/').to_string(),
        api_key,
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/media", post(upload).get(list))
        // axum 0.7은 matchit 0.7 — 경로 파라미터는 `:id` 문법 ({}는 0.8+).
        .route("/api/media/:id", delete(remove))
        // ── Sync v3 (docs/sync-protocol-v3.md) — 클라이언트는 ZIP만 ──
        .route("/v3/documents/:id/snapshot", put(sync_v3::put_snapshot))
        .route("/v3/uploads/:upload_id", get(sync_v3::get_upload_status))
        .route("/v3/documents/:id", get(sync_v3::download_snapshot))
        .route("/v3/documents/:id/revision", get(sync_v3::get_revision))
        .route("/v3/documents/:id/changes", get(sync_v3::get_changes))
        .route("/v3/objects/query", post(sync_v3::probe_objects))
        .route(
            "/v3/objects/:digest",
            put(sync_v3::put_object).get(sync_v3::get_object),
        )
        .layer(DefaultBodyLimit::max(512 * 1024 * 1024))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .unwrap_or_else(|e| panic!("{bind} 바인딩 실패: {e}"));
    println!("FreeDF media server listening on {bind}");
    axum::serve(listener, app).await.unwrap();
}

fn env(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

// ── 인증 ─────────────────────────────────────────────────────────────────────

async fn check_auth(state: &AppState, headers: &HeaderMap) -> bool {
    headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map_or(false, |k| k == state.api_key)
}

fn unauthorized() -> Response {
    (StatusCode::UNAUTHORIZED, "invalid or missing X-Api-Key").into_response()
}

// ── 핸들러 ───────────────────────────────────────────────────────────────────

async fn health() -> &'static str {
    "ok"
}

/// POST /api/media?doc_id=&kind= — multipart `file` 업로드.
async fn upload(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<UploadParams>,
    mut mp: Multipart,
) -> Response {
    if !check_auth(&state, &headers).await {
        return unauthorized();
    }
    let Some(field) = mp.next_field().await.ok().flatten() else {
        return (StatusCode::BAD_REQUEST, "no file field").into_response();
    };
    let file_name = field.file_name().unwrap_or("upload.bin").to_string();
    let mime = field
        .content_type()
        .map(|c| c.to_string())
        .unwrap_or_else(|| "application/octet-stream".to_string());
    let bytes = match field.bytes().await {
        Ok(b) => b,
        Err(_) => return (StatusCode::BAD_REQUEST, "could not read file").into_response(),
    };
    let size = bytes.len() as i64;

    // object key: YYYY/MM/{uuid}.{ext} — 확장자는 안전한 문자만 허용.
    let ext = safe_ext(&file_name);
    let now = chrono::Utc::now();
    let key = format!("{}/{}.{}", now.format("%Y/%m"), uuid::Uuid::new_v4(), ext);
    let path = state.media_dir.join(&key);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if std::fs::write(&path, &bytes).is_err() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "could not save file on disk",
        )
            .into_response();
    }

    let kind = params.kind.unwrap_or_else(|| "audio".to_string());
    let db = state.db.lock().await;
    let row = match db
        .query_one(
            "INSERT INTO media_objects (doc_id, kind, name, mime, size, object_key) \
             VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
            &[&params.doc_id, &kind, &file_name, &mime, &size, &key],
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            let _ = std::fs::remove_file(&path); // DB 실패 시 파일 정리
            return (StatusCode::INTERNAL_SERVER_ERROR, format!("db error: {e}")).into_response();
        }
    };
    let id: i64 = row.get(0);
    Json(MediaObject {
        id,
        doc_id: params.doc_id,
        kind,
        name: file_name,
        mime,
        size,
        url: format!("{}/media/{}", state.public_base, key),
    })
    .into_response()
}

/// GET /api/media?doc_id=&limit=&offset= — 최신순 목록.
async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<ListParams>,
) -> Response {
    if !check_auth(&state, &headers).await {
        return unauthorized();
    }
    let limit = params.limit.unwrap_or(50).clamp(1, 500);
    let offset = params.offset.unwrap_or(0).max(0);
    let db = state.db.lock().await;
    let rows = match db
        .query(
            "SELECT id, doc_id, kind, name, mime, size, object_key \
             FROM media_objects \
             WHERE ($1::bigint IS NULL OR doc_id = $1) \
             ORDER BY created_at DESC, id DESC \
             LIMIT $2 OFFSET $3",
            &[&params.doc_id, &limit, &offset],
        )
        .await
    {
        Ok(r) => r,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("db error: {e}")).into_response(),
    };
    let out: Vec<MediaObject> = rows
        .iter()
        .map(|r| {
            let key: String = r.get(6);
            MediaObject {
                id: r.get(0),
                doc_id: r.get(1),
                kind: r.get(2),
                name: r.get(3),
                mime: r.get(4),
                size: r.get(5),
                url: format!("{}/media/{}", state.public_base, key),
            }
        })
        .collect();
    Json(out).into_response()
}

/// DELETE /api/media/:id — 파일 + 행 삭제.
async fn remove(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Response {
    if !check_auth(&state, &headers).await {
        return unauthorized();
    }
    let db = state.db.lock().await;
    let row = match db
        .query_opt("DELETE FROM media_objects WHERE id = $1 RETURNING object_key", &[&id])
        .await
    {
        Ok(r) => r,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("db error: {e}")).into_response(),
    };
    match row {
        Some(row) => {
            let key: String = row.get(0);
            let _ = std::fs::remove_file(state.media_dir.join(&key));
            StatusCode::NO_CONTENT.into_response()
        }
        None => (StatusCode::NOT_FOUND, "no such media object").into_response(),
    }
}

// ── 유틸 ─────────────────────────────────────────────────────────────────────

/// 확장자만 안전 문자로 제한 (경로 조작 방지).
fn safe_ext(file_name: &str) -> String {
    let ext = std::path::Path::new(file_name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(10)
        .collect::<String>();
    if ext.is_empty() {
        "bin".to_string()
    } else {
        ext.to_lowercase()
    }
}
