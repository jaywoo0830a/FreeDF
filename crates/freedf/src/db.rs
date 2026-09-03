//! FreeDF v2 — PostgreSQL storage layer (Docker).
//!
//! JSON 파일 저장소를 완전히 대체합니다. 노트/PDF 본문(BYTEA)/주석(행 단위
//! 스트로크)/용지/북마크/세션/최근 목록/이벤트 로그가 모두 이 모듈을 통해
//! PostgreSQL에 저장됩니다. 파일 I/O는 더 이상 존재하지 않습니다.
//!
//! 연결은 `FREEDF_DATABASE_URL` 환경 변수(기본 `postgres://freedf:freedf@
//! localhost:5432/freedf`)로 결정됩니다. **스키마는 앱이 만들지 않습니다** —
//! DB 호스트에서 `server/db/up.sh`(PostgreSQL 18.6 + 마이그레이션)를
//! 먼저 실행해야 합니다. 앱은 `schema_migrations` 존재 여부만 확인합니다.

use freedf_core::model::{Stroke, StrokePoint};
use freedf_core::paper::{PagePaper, PaperStyle};
use freedf_core::store::AnnotationStore;
use postgres::types::ToSql;
use postgres::{Client, NoTls};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use crate::storage::StorageBackend;

/// 기본 연결 문자열 (server/db/docker-compose.yml과 일치).
pub const DEFAULT_DATABASE_URL: &str = "postgres://freedf:freedf@localhost:5432/freedf";

/// 문서 행 (documents 테이블).
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

/// 최근 항목 행 (recents JOIN documents).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecentRow {
    pub kind: String,
    pub doc_id: i64,
    pub title: String,
    pub opened_at: i64,
    pub origin_path: Option<String>,
}

/// 문서 전체 상태 로드 번들 (migration 0007 — 왕복 1회 + 단일 패스 파싱).
#[derive(Debug, Default)]
pub struct LoadedBundle {
    pub store: AnnotationStore,
    pub edits: Vec<freedf_core::history::Edit>,
    pub session: Option<Value>,
}

/// document_load 함수가 집계해 반환하는 획 행 (서버 JSONB → 단일 패스 serde).
#[derive(serde::Deserialize)]
struct StrokeRowSerde {
    id: u64,
    page_index: usize,
    tool: String,
    color: Vec<i32>,
    width: f32,
    points: Vec<StrokePoint>,
    created_at: i64,
}

/// document_load 함수가 집계해 반환하는 페이지 행.
#[derive(serde::Deserialize)]
struct PageRowSerde {
    page_index: usize,
    style: String,
    color: Vec<i32>,
    bookmarked: bool,
}

/// DB 핸들. 단일 연결을 `Mutex`로 감싸 UI 스레드 어디서든 동기 호출합니다.
#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Client>>,
}

fn conn_guard(conn: &Arc<Mutex<Client>>) -> std::sync::MutexGuard<'_, Client> {
    conn.lock().expect("db connection mutex poisoned")
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0) as i64
}

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

fn color_to_i32(c: [u8; 4]) -> Vec<i32> {
    vec![c[0] as i32, c[1] as i32, c[2] as i32, c[3] as i32]
}

fn color_from_i32(v: &[i32]) -> [u8; 4] {
    let at = |i: usize| v.get(i).copied().unwrap_or(0).clamp(0, 255) as u8;
    [at(0), at(1), at(2), at(3)]
}

/// entries → document_sync/document_sync_meta용 페이지 JSONB 배열.
fn pages_json(entries: &[(i32, PagePaper, bool)]) -> Value {
    Value::Array(
        entries
            .iter()
            .map(|(idx, paper, bookmarked)| {
                serde_json::json!({
                    "page_index": idx,
                    "style": style_str(paper.style),
                    "color": paper.color,
                    "bookmarked": bookmarked,
                })
            })
            .collect(),
    )
}

impl Db {
    /// 연결 + 스키마 존재 확인. 스키마 생성/마이그레이션은 서버 측
    /// (`server/db/up.sh`)이 담당합니다. 연결 대화상자에서 즉각적인
    /// 피드백이 가능하도록 TCP 타임아웃 5초를 적용합니다.
    pub fn connect(url: &str) -> Result<Self, String> {
        let mut config: postgres::Config = url
            .parse()
            .map_err(|e| format!("Invalid connection URL {url}: {e}"))?;
        config.connect_timeout(std::time::Duration::from_secs(5));
        let mut client = config
            .connect(NoTls)
            .map_err(|e| format!("Could not connect to PostgreSQL at {url}: {e}"))?;
        let has_schema: Option<i32> = client
            .query_opt(
                "SELECT 1 FROM pg_tables WHERE schemaname = 'public' \
                 AND tablename = 'schema_migrations'",
                &[],
            )
            .ok()
            .flatten()
            .map(|r| r.get(0));
        if has_schema.is_none() {
            return Err(
                "FreeDF schema is not initialized — run server/db/up.sh on the database host"
                    .to_string(),
            );
        }
        // 저장 함수(migration 0006) — 없으면 구형 스키마로 간주하고 거부.
        // (regprocedure는 OID 타입이라 ::text로 받아야 String 변환 가능)
        let has_sync: Option<String> = client
            .query_opt(
                "SELECT to_regprocedure('public.document_sync(bigint,integer,jsonb,jsonb,bytea)')::text",
                &[],
            )
            .ok()
            .flatten()
            .map(|r| r.get(0));
        if has_sync.is_none() {
            return Err(
                "FreeDF schema is outdated — run server/db/up.sh on the database host \
                 (migration 0006_document_sync is missing)"
                    .to_string(),
            );
        }
        // 로드 함수(migration 0007)도 필수 — 주석/저널/세션을 한 번에 로드합니다.
        let has_load: Option<String> = client
            .query_opt(
                "SELECT to_regprocedure('public.document_load(bigint)')::text",
                &[],
            )
            .ok()
            .flatten()
            .map(|r| r.get(0));
        if has_load.is_none() {
            return Err(
                "FreeDF schema is outdated — run server/db/up.sh on the database host \
                 (migration 0007_document_load is missing)"
                    .to_string(),
            );
        }
        // 델타 함수(migration 0008)도 필수 — 구조 연산을 서버에서 처리합니다.
        let has_delta: Option<String> = client
            .query_opt(
                "SELECT to_regprocedure('public.document_sync_meta(bigint,integer,jsonb,bytea)')::text",
                &[],
            )
            .ok()
            .flatten()
            .map(|r| r.get(0));
        if has_delta.is_none() {
            return Err(
                "FreeDF schema is outdated — run server/db/up.sh on the database host \
                 (migration 0008_document_delta is missing)"
                    .to_string(),
            );
        }
        Ok(Self {
            conn: Arc::new(Mutex::new(client)),
        })
    }

    // ---------- documents ----------

    pub fn insert_document(
        &self,
        kind: &str,
        title: &str,
        origin_path: Option<&str>,
        pdf: &[u8],
    ) -> Result<i64, String> {
        let mut c = conn_guard(&self.conn);
        let now = now_ms();
        c.query_one(
            "INSERT INTO documents (kind, title, origin_path, pdf, page_count, created_at, updated_at)
             VALUES ($1, $2, $3, $4, 1, $5, $5)
             RETURNING id",
            &[&kind, &title, &origin_path, &pdf.to_vec(), &now],
        )
        .map(|r| r.get(0))
        .map_err(|e| format!("Could not insert document: {e}"))
    }

    pub fn get_document(&self, id: i64) -> Option<DocRow> {
        let mut c = conn_guard(&self.conn);
        c.query_opt(
            "SELECT id, kind, title, origin_path, page_count, created_at, updated_at
             FROM documents WHERE id = $1",
            &[&id],
        )
        .ok()?
        .map(|r| DocRow {
            id: r.get(0),
            kind: r.get(1),
            title: r.get(2),
            origin_path: r.get(3),
            page_count: r.get(4),
            created_at: r.get(5),
            updated_at: r.get(6),
        })
    }

    /// 원래 경로가 같은 외부 PDF 문서 탐색 (중복 import 방지).
    pub fn find_document_by_path(&self, path: &str) -> Option<i64> {
        let mut c = conn_guard(&self.conn);
        c.query_opt(
            "SELECT id FROM documents WHERE kind = 'pdf' AND origin_path = $1",
            &[&path],
        )
        .ok()?
        .map(|r| r.get(0))
    }

    pub fn load_pdf(&self, id: i64) -> Option<Vec<u8>> {
        let mut c = conn_guard(&self.conn);
        c.query_opt("SELECT pdf FROM documents WHERE id = $1", &[&id])
            .ok()?
            .map(|r| r.get(0))
    }

    pub fn update_title(&self, id: i64, title: &str) -> Result<(), String> {
        let mut c = conn_guard(&self.conn);
        c.execute(
            "UPDATE documents SET title = $2, updated_at = $3 WHERE id = $1",
            &[&id, &title, &now_ms()],
        )
        .map(|_| ())
        .map_err(|e| format!("Could not rename document: {e}"))
    }

    pub fn delete_document(&self, id: i64) -> Result<(), String> {
        let mut c = conn_guard(&self.conn);
        c.execute("DELETE FROM documents WHERE id = $1", &[&id])
            .map(|_| ())
            .map_err(|e| format!("Could not delete document: {e}"))
    }

    pub fn list_notes(&self) -> Vec<DocRow> {
        let mut c = conn_guard(&self.conn);
        c.query(
            "SELECT id, kind, title, origin_path, page_count, created_at, updated_at
             FROM documents WHERE kind = 'note' ORDER BY updated_at DESC",
            &[],
        )
        .map(|rows| {
            rows.iter()
                .map(|r| DocRow {
                    id: r.get(0),
                    kind: r.get(1),
                    title: r.get(2),
                    origin_path: r.get(3),
                    page_count: r.get(4),
                    created_at: r.get(5),
                    updated_at: r.get(6),
                })
                .collect()
        })
        .unwrap_or_default()
    }

    // ---------- pages (paper + bookmarks) ----------

    /// 페이지 하나의 용지/북마크 상태를 저장합니다.
    pub fn upsert_page(
        &self,
        doc_id: i64,
        page_index: i32,
        paper: &PagePaper,
        bookmarked: bool,
    ) {
        let mut c = conn_guard(&self.conn);
        let _ = c.execute(
            "INSERT INTO pages (doc_id, page_index, style, color, bookmarked)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (doc_id, page_index) DO UPDATE SET
               style = EXCLUDED.style, color = EXCLUDED.color,
               bookmarked = EXCLUDED.bookmarked",
            &[
                &doc_id,
                &page_index,
                &style_str(paper.style),
                &color_to_i32(paper.color),
                &bookmarked,
            ],
        );
    }

    // ---------- strokes ----------

    /// 전역 시퀀스에서 스트로크 id를 `n`개 할당합니다.
    /// 스토어의 로컬 id 대신 이 id를 스트로크에 부여해 히스토리/undo가 DB와 일치하게 합니다.
    pub fn alloc_stroke_ids(&self, n: usize) -> Vec<i64> {
        if n == 0 {
            return Vec::new();
        }
        let mut c = conn_guard(&self.conn);
        c.query(
            "SELECT nextval('stroke_id_seq') FROM generate_series(1, $1::bigint)",
            &[&(n as i64)],
        )
        .map(|rows| rows.iter().map(|r| r.get(0)).collect())
        .unwrap_or_default()
    }

    /// 스트로크 삽입 (id는 이미 최종 값 — undo/redo 복원에도 동일하게 사용).
    /// 왕복 최소화를 위해 청크 단위 다중 행 INSERT를 사용합니다.
    pub fn insert_strokes(&self, doc_id: i64, page_index: i32, strokes: &[Stroke]) {
        if strokes.is_empty() {
            return;
        }
        let mut c = conn_guard(&self.conn);
        let now = now_ms();
        const CHUNK: usize = 4000; // 행당 파라미터 8개 → 문장당 32,000개
        for chunk in strokes.chunks(CHUNK) {
            let mut sql = String::from(
                "INSERT INTO strokes (id, doc_id, page_index, tool, color, width, points, created_at) VALUES ",
            );
            let mut params: Vec<Box<dyn ToSql + Sync>> = Vec::with_capacity(chunk.len() * 8);
            for (i, s) in chunk.iter().enumerate() {
                if i > 0 {
                    sql.push(',');
                }
                let b = i * 8;
                sql.push_str(&format!(
                    "(${},${},${},${},${},${},${},${})",
                    b + 1,
                    b + 2,
                    b + 3,
                    b + 4,
                    b + 5,
                    b + 6,
                    b + 7,
                    b + 8
                ));
                let id = s.id as i64;
                let points: Value =
                    serde_json::to_value(&s.points).unwrap_or(Value::Array(Vec::new()));
                let tool = s.tool.label().to_string();
                let color = color_to_i32(s.color);
                let created = if s.created_ms > 0 { s.created_ms as i64 } else { now };
                params.push(Box::new(id));
                params.push(Box::new(doc_id));
                params.push(Box::new(page_index));
                params.push(Box::new(tool));
                params.push(Box::new(color));
                params.push(Box::new(s.width));
                params.push(Box::new(points));
                params.push(Box::new(created));
            }
            sql.push_str(" ON CONFLICT (id) DO NOTHING");
            let refs: Vec<&(dyn ToSql + Sync)> = params.iter().map(|p| &**p).collect();
            let _ = c.execute(&sql, &refs);
        }
    }

    /// 스트로크 삭제 (지우개/clear/undo) — id별 왕복 없이 한 번의 쿼리로.
    pub fn delete_strokes(&self, doc_id: i64, ids: &[i64]) {
        if ids.is_empty() {
            return;
        }
        let mut c = conn_guard(&self.conn);
        let _ = c.execute(
            "DELETE FROM strokes WHERE doc_id = $1 AND id = ANY($2)",
            &[&doc_id, &ids],
        );
    }

    /// 메타 동기화(migration 0008) — 페이지(용지/북마크)·문서 정보·PDF만
    /// 한 번의 왕복으로 반영합니다. **스트로크는 건드리지 않습니다**
    /// (획은 write-behind 대기열이 이미 증분 반영).
    pub fn sync_meta(
        &self,
        doc_id: i64,
        page_count: i32,
        entries: &[(i32, PagePaper, bool)],
        pdf: Option<&[u8]>,
    ) -> Result<(), String> {
        let mut c = conn_guard(&self.conn);
        let pages = pages_json(entries);
        c.execute(
            "SELECT public.document_sync_meta($1, $2, $3, $4)",
            &[&doc_id, &page_count, &pages, &pdf.map(|b| b.to_vec())],
        )
        .map(|_| ())
        .map_err(|e| format!("document_sync_meta failed: {e}"))
    }

    /// 페이지 중간 삽입 — from 이상 획의 page_index를 서버에서 이동합니다
    /// (재전송 없음 — 앱 로컬 스토어는 이미 동일하게 이동돼 있습니다).
    pub fn shift_strokes(&self, doc_id: i64, from: i32, delta: i32) {
        let mut c = conn_guard(&self.conn);
        let _ = c.execute(
            "SELECT public.document_shift_strokes($1, $2, $3)",
            &[&doc_id, &from, &delta],
        );
    }

    /// 페이지 삭제 — 해당 페이지 획 삭제 + 이후 인덱스 -1 (서버 처리).
    pub fn delete_page_data(&self, doc_id: i64, page: i32) {
        let mut c = conn_guard(&self.conn);
        let _ = c.execute(
            "SELECT public.document_delete_page($1, $2)",
            &[&doc_id, &page],
        );
    }

    /// 페이지 회전 — 해당 페이지 획의 x/y를 서버에서 변환 (재전송 없음).
    pub fn rotate_page_data(&self, doc_id: i64, page: i32, clockwise: bool, w: f32, h: f32) {
        let mut c = conn_guard(&self.conn);
        let _ = c.execute(
            "SELECT public.document_rotate_page($1, $2, $3, $4, $5)",
            &[&doc_id, &page, &clockwise, &w, &h],
        );
    }

    /// 전체 페이지 회전 — 페이지별 크기를 서버에 보내 내부에서 반복 변환합니다.
    pub fn rotate_all_data(&self, doc_id: i64, clockwise: bool, sizes: &[[f32; 2]]) {
        let mut c = conn_guard(&self.conn);
        let sizes: Value = Value::Array(
            sizes
                .iter()
                .map(|s| serde_json::json!([s[0], s[1]]))
                .collect(),
        );
        let _ = c.execute(
            "SELECT public.document_rotate_all($1, $2, $3)",
            &[&doc_id, &clockwise, &sizes],
        );
    }

    /// 문서의 전체 주석/용지/북마크/저널/세션을 **한 번의 왕복**으로 로드
    /// (migration 0007 함수) — 서버가 JSONB 배열로 집계해 주므로 클라이언트는
    /// 단일 패스 serde 파싱으로 끝납니다 (획마다 개별 파싱 없음).
    pub fn load_bundle(&self, doc_id: i64) -> LoadedBundle {
        let mut c = conn_guard(&self.conn);
        let row = match c.query_opt(
            "SELECT strokes, pages, edits, session FROM public.document_load($1)",
            &[&doc_id],
        ) {
            Ok(Some(r)) => r,
            _ => return LoadedBundle::default(),
        };
        let strokes_val: Value = row.get(0);
        let pages_val: Value = row.get(1);
        let edits_val: Value = row.get(2);
        let session: Option<Value> = row.get(3);
        let mut store = AnnotationStore::new();
        if let Ok(rows) = serde_json::from_value::<Vec<StrokeRowSerde>>(strokes_val) {
            let mut grouped: BTreeMap<usize, Vec<Stroke>> = BTreeMap::new();
            for s in rows {
                grouped.entry(s.page_index).or_default().push(Stroke {
                    id: s.id,
                    tool: freedf_core::model::ToolType::from_label(&s.tool),
                    color: color_from_i32(&s.color),
                    width: s.width,
                    points: s.points,
                    created_ms: s.created_at.max(0) as u64,
                });
            }
            for (page, strokes) in grouped {
                store.add_strokes(page, strokes);
            }
        }
        if let Ok(pages) = serde_json::from_value::<Vec<PageRowSerde>>(pages_val) {
            for p in pages {
                store.set_paper(
                    p.page_index,
                    PagePaper {
                        style: style_from(&p.style),
                        color: color_from_i32(&p.color),
                    },
                );
                if p.bookmarked {
                    store.toggle_bookmark(p.page_index);
                }
            }
        }
        let edits = serde_json::from_value::<Vec<freedf_core::history::Edit>>(edits_val)
            .unwrap_or_default();
        LoadedBundle {
            store,
            edits,
            session,
        }
    }

    // ---------- sessions ----------

    pub fn upsert_session(&self, doc_id: i64, state: &Value) {
        let mut c = conn_guard(&self.conn);
        let _ = c.execute(
            "INSERT INTO sessions (doc_id, state, updated_at) VALUES ($1, $2, $3)
             ON CONFLICT (doc_id) DO UPDATE SET state = EXCLUDED.state, updated_at = EXCLUDED.updated_at",
            &[&doc_id, state, &now_ms()],
        );
    }

    // ---------- global app state ----------

    pub fn get_app_state(&self, key: &str) -> Option<Value> {
        let mut c = conn_guard(&self.conn);
        c.query_opt("SELECT value FROM app_state WHERE key = $1", &[&key])
            .ok()?
            .map(|r| r.get(0))
    }

    pub fn set_app_state(&self, key: &str, value: &Value) {
        let mut c = conn_guard(&self.conn);
        let _ = c.execute(
            "INSERT INTO app_state (key, value) VALUES ($1, $2)
             ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
            &[&key, value],
        );
    }

    // ---------- recents ----------

    pub fn load_recents(&self) -> Vec<RecentRow> {
        let mut c = conn_guard(&self.conn);
        c.query(
            "SELECT r.kind, r.doc_id, r.title, r.opened_at, d.origin_path
             FROM recents r JOIN documents d ON d.id = r.doc_id
             ORDER BY r.opened_at DESC LIMIT 20",
            &[],
        )
        .map(|rows| {
            rows.iter()
                .map(|r| RecentRow {
                    kind: r.get(0),
                    doc_id: r.get(1),
                    title: r.get(2),
                    opened_at: r.get(3),
                    origin_path: r.get(4),
                })
                .collect()
        })
        .unwrap_or_default()
    }

    pub fn touch_recent(&self, kind: &str, doc_id: i64, title: &str) {
        // DELETE + INSERT 두 왕복 → upsert 한 번으로 (PK (kind, doc_id) 충돌 시 갱신).
        let mut c = conn_guard(&self.conn);
        let _ = c.execute(
            "INSERT INTO recents (kind, doc_id, title, opened_at) VALUES ($1, $2, $3, $4)
             ON CONFLICT (kind, doc_id) DO UPDATE SET
               title = EXCLUDED.title, opened_at = EXCLUDED.opened_at",
            &[&kind, &doc_id, &title, &now_ms()],
        );
    }

    // ---------- edit journal (영속 히스토리) ----------

    /// 편집 저널 일괄 기록 — 청크 다중 행 INSERT 후 트리밍 1회.
    /// (편집별로 2왕복(INSERT+트리밍) 하던 것을 배치로 줄입니다.)
    pub fn log_edits(&self, doc_id: i64, edits: &[freedf_core::history::Edit]) {
        if edits.is_empty() {
            return;
        }
        let mut c = conn_guard(&self.conn);
        let now = now_ms();
        const CHUNK: usize = 500; // 행당 파라미터 3개
        for chunk in edits.chunks(CHUNK) {
            let mut sql =
                String::from("INSERT INTO doc_edits (doc_id, edit, created_at) VALUES ");
            let mut params: Vec<Box<dyn ToSql + Sync>> = Vec::with_capacity(chunk.len() * 3);
            for (i, edit) in chunk.iter().enumerate() {
                if i > 0 {
                    sql.push(',');
                }
                let b = i * 3;
                sql.push_str(&format!("(${},${},${})", b + 1, b + 2, b + 3));
                let value = serde_json::to_value(edit).unwrap_or(Value::Null);
                params.push(Box::new(doc_id));
                params.push(Box::new(value));
                params.push(Box::new(now));
            }
            let refs: Vec<&(dyn ToSql + Sync)> = params.iter().map(|p| &**p).collect();
            let _ = c.execute(&sql, &refs);
        }
        // 문서당 최근 500건만 유지 (배치 후 1회 트리밍).
        let _ = c.execute(
            "DELETE FROM doc_edits WHERE doc_id = $1 AND id NOT IN \
             (SELECT id FROM doc_edits WHERE doc_id = $1 ORDER BY id DESC LIMIT 500)",
            &[&doc_id],
        );
    }

    /// 편집 하나를 저널에 기록 (배치 경로의 단일 편집 케이스).
    pub fn log_edit(&self, doc_id: i64, edit: &freedf_core::history::Edit) {
        self.log_edits(doc_id, std::slice::from_ref(edit));
    }

    /// 문서의 편집 저널 전체 삭제 (페이지 회전처럼 좌표계가 바뀌는 구조 연산 시).
    pub fn clear_edits(&self, doc_id: i64) {
        let mut c = conn_guard(&self.conn);
        let _ = c.execute("DELETE FROM doc_edits WHERE doc_id = $1", &[&doc_id]);
    }

    // ---------- word cache (사전 오버레이) ----------

    pub fn get_word_cache(&self, word: &str) -> Option<Value> {
        let mut c = conn_guard(&self.conn);
        c.query_opt("SELECT data FROM word_cache WHERE word = $1", &[&word])
            .ok()?
            .map(|r| r.get(0))
    }

    pub fn set_word_cache(&self, word: &str, data: &Value) {
        let mut c = conn_guard(&self.conn);
        let _ = c.execute(
            "INSERT INTO word_cache (word, data, updated_at) VALUES ($1, $2, $3)
             ON CONFLICT (word) DO UPDATE SET data = EXCLUDED.data, updated_at = EXCLUDED.updated_at",
            &[&word, data, &now_ms()],
        );
    }

    // ---------- event log ----------

    pub fn insert_log(&self, epoch_ms: u128, seq: u64, event: &Value) {
        let _ = seq;
        self.insert_logs(&[(epoch_ms, event.clone())]);
    }

    /// 이벤트 로그 일괄 기록 — 로거 스레드가 모아서 보내므로 왕복 수를 줄입니다.
    pub fn insert_logs(&self, items: &[(u128, Value)]) {
        if items.is_empty() {
            return;
        }
        let mut c = conn_guard(&self.conn);
        const CHUNK: usize = 500; // 행당 파라미터 2개
        for chunk in items.chunks(CHUNK) {
            let mut sql = String::from("INSERT INTO event_log (epoch_ms, event) VALUES ");
            let mut params: Vec<Box<dyn ToSql + Sync>> = Vec::with_capacity(chunk.len() * 2);
            for (i, (epoch_ms, event)) in chunk.iter().enumerate() {
                if i > 0 {
                    sql.push(',');
                }
                let b = i * 2;
                sql.push_str(&format!("(${},${})", b + 1, b + 2));
                params.push(Box::new(*epoch_ms as i64));
                params.push(Box::new(event.clone()));
            }
            let refs: Vec<&(dyn ToSql + Sync)> = params.iter().map(|p| &**p).collect();
            let _ = c.execute(&sql, &refs);
        }
    }

    /// 연결 상태 확인 (SELECT 1).
    pub fn ping(&self) -> bool {
        let mut c = conn_guard(&self.conn);
        c.query_opt("SELECT 1", &[]).map(|r| r.is_some()).unwrap_or(false)
    }

    /// DB 인스턴스 식별자 (migrations/0005_db_identity.sql의 UUID).
    /// DB를 초기화하면 값이 바뀌므로 클라이언트가 로컬 캐시를 폐기할 수 있습니다.
    pub fn identity(&self) -> Option<String> {
        let mut c = conn_guard(&self.conn);
        c.query_opt("SELECT uuid FROM db_identity LIMIT 1", &[])
            .ok()
            .flatten()
            .map(|r| r.get(0))
    }
}

/// `Db`의 메서드 시그니처가 `StorageBackend`와 1:1로 일치합니다 —
/// 앱 코드는 이 트레이트만 바라보고, SQL은 db.rs 안에만 남습니다.
impl StorageBackend for Db {
    fn insert_document(
        &self,
        kind: &str,
        title: &str,
        origin_path: Option<&str>,
        pdf: &[u8],
    ) -> Result<i64, String> {
        Db::insert_document(self, kind, title, origin_path, pdf)
    }
    fn get_document(&self, id: i64) -> Option<DocRow> {
        Db::get_document(self, id)
    }
    fn find_document_by_path(&self, path: &str) -> Option<i64> {
        Db::find_document_by_path(self, path)
    }
    fn load_pdf(&self, id: i64) -> Option<Vec<u8>> {
        Db::load_pdf(self, id)
    }
    fn update_title(&self, id: i64, title: &str) -> Result<(), String> {
        Db::update_title(self, id, title)
    }
    fn delete_document(&self, id: i64) -> Result<(), String> {
        Db::delete_document(self, id)
    }
    fn list_notes(&self) -> Vec<DocRow> {
        Db::list_notes(self)
    }

    fn upsert_page(&self, doc_id: i64, page_index: i32, paper: &PagePaper, bookmarked: bool) {
        Db::upsert_page(self, doc_id, page_index, paper, bookmarked)
    }

    fn alloc_stroke_ids(&self, n: usize) -> Vec<i64> {
        Db::alloc_stroke_ids(self, n)
    }
    fn insert_strokes(&self, doc_id: i64, page_index: i32, strokes: &[Stroke]) {
        Db::insert_strokes(self, doc_id, page_index, strokes)
    }
    fn delete_strokes(&self, doc_id: i64, ids: &[i64]) {
        Db::delete_strokes(self, doc_id, ids)
    }
    fn sync_meta(
        &self,
        doc_id: i64,
        page_count: i32,
        entries: &[(i32, PagePaper, bool)],
        pdf: Option<&[u8]>,
    ) -> Result<(), String> {
        Db::sync_meta(self, doc_id, page_count, entries, pdf)
    }
    fn shift_strokes(&self, doc_id: i64, from: i32, delta: i32) {
        Db::shift_strokes(self, doc_id, from, delta)
    }
    fn delete_page_data(&self, doc_id: i64, page: i32) {
        Db::delete_page_data(self, doc_id, page)
    }
    fn rotate_page_data(&self, doc_id: i64, page: i32, clockwise: bool, w: f32, h: f32) {
        Db::rotate_page_data(self, doc_id, page, clockwise, w, h)
    }
    fn rotate_all_data(&self, doc_id: i64, clockwise: bool, sizes: &[[f32; 2]]) {
        Db::rotate_all_data(self, doc_id, clockwise, sizes)
    }
    fn load_bundle(&self, doc_id: i64) -> LoadedBundle {
        Db::load_bundle(self, doc_id)
    }

    fn upsert_session(&self, doc_id: i64, state: &Value) {
        Db::upsert_session(self, doc_id, state)
    }

    fn get_app_state(&self, key: &str) -> Option<Value> {
        Db::get_app_state(self, key)
    }
    fn set_app_state(&self, key: &str, value: &Value) {
        Db::set_app_state(self, key, value)
    }

    fn load_recents(&self) -> Vec<RecentRow> {
        Db::load_recents(self)
    }
    fn touch_recent(&self, kind: &str, doc_id: i64, title: &str) {
        Db::touch_recent(self, kind, doc_id, title)
    }

    fn log_edit(&self, doc_id: i64, edit: &freedf_core::history::Edit) {
        Db::log_edit(self, doc_id, edit)
    }
    fn log_edits(&self, doc_id: i64, edits: &[freedf_core::history::Edit]) {
        Db::log_edits(self, doc_id, edits)
    }
    fn clear_edits(&self, doc_id: i64) {
        Db::clear_edits(self, doc_id)
    }

    fn get_word_cache(&self, word: &str) -> Option<Value> {
        Db::get_word_cache(self, word)
    }
    fn set_word_cache(&self, word: &str, data: &Value) {
        Db::set_word_cache(self, word, data)
    }

    fn insert_log(&self, epoch_ms: u128, seq: u64, event: &Value) {
        Db::insert_log(self, epoch_ms, seq, event)
    }
    fn insert_logs(&self, items: &[(u128, Value)]) {
        Db::insert_logs(self, items)
    }

    fn ping(&self) -> bool {
        Db::ping(self)
    }

    fn identity(&self) -> Option<String> {
        Db::identity(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use freedf_core::model::ToolType;

    /// 실제 PostgreSQL로 전체 저장 계층을 검증합니다.
    /// `FREEDF_TEST_DB=1 cargo test -p freedf smoke_against_live_postgres`
    /// (docker로 postgres가 떠 있어야 합니다 — docker-compose.yml 참고)
    #[test]
    fn smoke_against_live_postgres() {
        if std::env::var("FREEDF_TEST_DB").is_err() {
            return;
        }
        let url = std::env::var("FREEDF_DATABASE_URL")
            .unwrap_or_else(|_| DEFAULT_DATABASE_URL.to_string());
        let db = Db::connect(&url).expect("connect");

        // ── documents ──
        let doc_id = db
            .insert_document("note", "Smoke Note", None, b"%PDF-1.4 fake")
            .expect("insert document");
        assert!(doc_id > 0);
        let row = db.get_document(doc_id).expect("get document");
        assert_eq!(row.title, "Smoke Note");
        assert!(row.is_note());
        assert_eq!(row.page_count, 1);

        // ── strokes (시퀀스 id → load_bundle 스토어 일치) ──
        let ids = db.alloc_stroke_ids(2);
        assert_eq!(ids.len(), 2);
        let strokes = vec![
            Stroke {
                id: ids[0] as u64,
                tool: ToolType::Pen,
                color: [20, 20, 20, 255],
                width: 2.0,
                points: vec![
                    StrokePoint::new(1.0, 2.0, 0.5),
                    StrokePoint::new(3.0, 4.0, 0.9),
                ],
                created_ms: 1234,
            },
            Stroke {
                id: ids[1] as u64,
                tool: ToolType::Pen,
                color: [200, 40, 40, 255],
                width: 3.0,
                points: vec![StrokePoint::new(0.0, 0.0, 1.0)],
                created_ms: 0,
            },
        ];
        db.insert_strokes(doc_id, 0, &strokes);
        let store = db.load_bundle(doc_id).store;
        assert_eq!(store.total_stroke_count(), 2);
        assert_eq!(store.strokes_on(0)[0].id, ids[0] as u64);
        assert_eq!(store.strokes_on(0)[0].tool, ToolType::Pen);
        assert_eq!(store.strokes_on(0)[0].points.len(), 2);
        assert_eq!(store.strokes_on(0)[0].created_ms, 1234, "created_at 왕복");

        // ── pages (용지/북마크) ──
        let paper = PagePaper {
            style: PaperStyle::Grid,
            color: [255, 255, 255, 255],
        };
        db.upsert_page(doc_id, 0, &paper, true);
        let pages_bundle = db.load_bundle(doc_id);
        assert!(pages_bundle.store.is_bookmarked(0), "bookmarked");
        assert_eq!(
            pages_bundle.store.paper_on(0).map(|p| p.style),
            Some(PaperStyle::Grid)
        );
        assert_eq!(
            pages_bundle.store.paper_on(0).map(|p| p.color),
            Some([255, 255, 255, 255])
        );

        // ── sessions ──
        let state = serde_json::json!({"page": 2, "zoom": 1.5});
        db.upsert_session(doc_id, &state);
        assert_eq!(db.load_bundle(doc_id).session.unwrap(), state);

        // ── app_state ──
        db.set_app_state("smoke-key", &serde_json::json!({"a": 1}));
        assert_eq!(db.get_app_state("smoke-key").unwrap()["a"], 1);

        // ── recents ──
        db.touch_recent("note", doc_id, "Smoke Note");
        assert!(db.load_recents().iter().any(|r| r.doc_id == doc_id));
        assert_eq!(db.load_recents()[0].title, "Smoke Note");

        // ── 델타 함수 (migration 0008) — 구조 연산을 재전송 없이 서버 처리 ──
        {
            // ① shift: 페이지 0부터 +1 → 기존 2획은 페이지 1로 이동.
            db.shift_strokes(doc_id, 0, 1);
            let shifted = db.load_bundle(doc_id);
            assert_eq!(shifted.store.stroke_count_on(0), 0);
            assert_eq!(shifted.store.stroke_count_on(1), 2);

            // ② 새 획을 (이동 후) 페이지 0에 추가.
            let sid = db.alloc_stroke_ids(1)[0];
            db.insert_strokes(
                doc_id,
                0,
                &[Stroke {
                    id: sid as u64,
                    tool: ToolType::Pen,
                    color: [9, 9, 9, 255],
                    width: 1.5,
                    points: vec![StrokePoint::new(5.0, 5.0, 0.5)],
                    created_ms: 0,
                }],
            );

            // ③ rotate: 페이지 0의 점 (x,y) → (h-y, x) 시계방향 (w=100,h=200).
            db.rotate_page_data(doc_id, 0, true, 100.0, 200.0);
            let rotated = db.load_bundle(doc_id);
            let p0 = &rotated.store.strokes_on(0)[0].points[0];
            assert!((p0.x - 195.0).abs() < 0.01, "x' = h - y = 195");
            assert!((p0.y - 5.0).abs() < 0.01, "y' = x = 5");

            // ④ meta: 3페이지 확장 + PDF 교체 — 획은 불변.
            let entries2 = vec![
                (0i32, paper, true),
                (1i32, paper, true),
                (2i32, paper, false),
            ];
            db.sync_meta(doc_id, 3, &entries2, Some(b"%PDF-1.4 synced"))
                .expect("sync_meta");
            let synced = db.load_bundle(doc_id);
            assert_eq!(synced.store.total_stroke_count(), 3);
            for i in 0..3 {
                assert!(synced.store.paper_on(i).is_some(), "page {i} paper");
            }
            assert!(synced.store.is_bookmarked(1));
            assert!(!synced.store.is_bookmarked(2));
            assert_eq!(db.get_document(doc_id).unwrap().page_count, 3);
            assert_eq!(db.load_pdf(doc_id).unwrap(), b"%PDF-1.4 synced");

            // ⑤ delete_page_data: 페이지 1 삭제 → 그 페이지 획 제거 + 이후 -1.
            db.delete_page_data(doc_id, 1);
            let deleted = db.load_bundle(doc_id);
            assert_eq!(deleted.store.total_stroke_count(), 1);
            assert_eq!(deleted.store.stroke_count_on(0), 1);

            // ⑥ meta 축소: 1페이지로 줄이면 잉여 페이지 삭제, PDF NULL 유지.
            let entries3 = vec![(0i32, paper, false)];
            db.sync_meta(doc_id, 1, &entries3, None)
                .expect("sync shrink");
            let shrunk = db.load_bundle(doc_id);
            assert!(shrunk.store.paper_on(1).is_none(), "page 1 removed");
            assert!(shrunk.store.paper_on(2).is_none(), "page 2 removed");
            assert_eq!(db.get_document(doc_id).unwrap().page_count, 1);
            assert_eq!(db.load_pdf(doc_id).unwrap(), b"%PDF-1.4 synced");
        }

        // ── event log ──
        db.insert_log(123, 1, &serde_json::json!({"kind": "AppStart"}));

        // ── event log batch (logger 스레드가 사용하는 경로) ──
        // 이전 실행이 남긴 행을 먼저 지워 단언을 멱등하게 만듭니다.
        {
            let mut c = conn_guard(&db.conn);
            let _ = c.execute(
                "DELETE FROM event_log WHERE epoch_ms IN (456, 457)",
                &[],
            );
        }
        db.insert_logs(&[
            (456, serde_json::json!({"kind": "Batch1"})),
            (457, serde_json::json!({"kind": "Batch2"})),
        ]);
        {
            let mut c = conn_guard(&db.conn);
            let n: i64 = c
                .query_one(
                    "SELECT count(*) FROM event_log WHERE epoch_ms IN (456, 457)",
                    &[],
                )
                .map(|r| r.get(0))
                .unwrap_or(-1);
            assert_eq!(n, 2);
        }

        // ── 영속 편집 저널 (doc_edits) ──
        use freedf_core::history::Edit;
        db.log_edit(
            doc_id,
            &Edit::AddStrokes {
                page: 0,
                strokes: strokes.clone(),
            },
        );
        db.log_edit(
            doc_id,
            &Edit::RemoveStrokes {
                page: 0,
                strokes: strokes.clone(),
            },
        );
        let edits = db.load_bundle(doc_id).edits;
        assert_eq!(edits.len(), 2);
        assert!(matches!(edits[0], Edit::AddStrokes { .. }));
        assert!(matches!(edits[1], Edit::RemoveStrokes { .. }));

        // ── media_objects 테이블 존재 (서버 측 마이그레이션 0004 확인) ──
        {
            let mut c = conn_guard(&db.conn);
            let n: i64 = c
                .query_one("SELECT count(*) FROM media_objects", &[])
                .map(|r| r.get(0))
                .unwrap_or(-1);
            assert_eq!(n, 0);
        }

        // ── word_cache (사전 오버레이 캐시) ──
        // 이전 실행이 남긴 캐시를 먼저 지워 단언을 멱등하게 만듭니다.
        {
            let mut c = conn_guard(&db.conn);
            c.execute("DELETE FROM word_cache WHERE word = 'hello'", &[])
                .expect("clear word_cache");
        }
        let entry = serde_json::json!([
            {"word": "hello", "phonetic": "/həˈloʊ/",
             "meanings": [{"partOfSpeech": "interjection",
                           "definitions": [{"definition": "used as a greeting"}]}]}
        ]);
        assert!(db.get_word_cache("hello").is_none());
        db.set_word_cache("hello", &entry);
        let cached = db.get_word_cache("hello").expect("cached");
        assert_eq!(cached[0]["phonetic"], "/həˈloʊ/");
        db.set_word_cache("hello", &serde_json::json!([{"word": "hello", "meanings": []}]));
        assert_eq!(db.get_word_cache("hello").unwrap()[0]["meanings"].as_array().unwrap().len(), 0);

        // ── delete + cascade ──
        db.delete_document(doc_id).expect("delete");
        assert!(db.get_document(doc_id).is_none());
        let after = db.load_bundle(doc_id);
        assert_eq!(after.store.total_stroke_count(), 0);
        assert!(after.session.is_none());
        assert!(db.load_recents().iter().all(|r| r.doc_id != doc_id));
        assert!(after.edits.is_empty());
    }

    /// 왕복 수 스트레스 검증 (기본 제외) — 대량 획 삽입과 전체 동기화 측정.
    /// `FREEDF_TEST_DB=1 cargo test -p freedf sync_document_batch_live -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn sync_document_batch_live() {
        if std::env::var("FREEDF_TEST_DB").is_err() {
            return;
        }
        let url = std::env::var("FREEDF_DATABASE_URL")
            .unwrap_or_else(|_| DEFAULT_DATABASE_URL.to_string());
        let db = Db::connect(&url).expect("connect");
        let doc_id = db
            .insert_document("note", "Batch Perf", None, b"%PDF-1.4 perf")
            .expect("insert document");

        const N: usize = 5000;
        let ids = db.alloc_stroke_ids(N);
        assert_eq!(ids.len(), N);
        let strokes: Vec<Stroke> = ids
            .iter()
            .map(|id| Stroke {
                id: *id as u64,
                tool: ToolType::Pen,
                color: [20, 20, 20, 255],
                width: 2.0,
                points: vec![
                    StrokePoint::new(1.0, 2.0, 0.5),
                    StrokePoint::new(3.0, 4.0, 0.9),
                ],
                created_ms: 0,
            })
            .collect();

        let t0 = std::time::Instant::now();
        db.insert_strokes(doc_id, 0, &strokes);
        let insert_ms = t0.elapsed().as_millis();

        let paper = PagePaper {
            style: PaperStyle::Grid,
            color: [255, 255, 255, 255],
        };
        let entries = vec![(0i32, paper, false)];
        // 델타 프로토콜: 메타 동기화는 획과 무관 — 획 수에 비례하지 않습니다.
        let t1 = std::time::Instant::now();
        db.sync_meta(doc_id, 1, &entries, None).expect("sync_meta");
        let sync_ms = t1.elapsed().as_millis();

        let t2 = std::time::Instant::now();
        let loaded = db.load_bundle(doc_id);
        let load_ms = t2.elapsed().as_millis();

        // 서버 회전도 획 수와 무관하게 한 번의 UPDATE입니다.
        let t3 = std::time::Instant::now();
        db.rotate_page_data(doc_id, 0, true, 595.0, 842.0);
        let rotate_ms = t3.elapsed().as_millis();

        assert_eq!(loaded.store.total_stroke_count(), N);
        println!(
            "batch perf: insert {N} strokes = {insert_ms}ms, sync_meta = {sync_ms}ms, document_load = {load_ms}ms, rotate = {rotate_ms}ms"
        );

        // 관대한 상한 — 행별 왕복이었다면 수 분 걸리는 시나리오입니다.
        assert!(insert_ms < 30_000, "insert too slow: {insert_ms}ms");
        assert!(sync_ms < 30_000, "sync too slow: {sync_ms}ms");
        assert!(load_ms < 30_000, "load too slow: {load_ms}ms");
        assert!(rotate_ms < 30_000, "rotate too slow: {rotate_ms}ms");

        db.delete_document(doc_id).expect("delete");
    }
}
