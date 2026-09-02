//! FreeDF v2 — PostgreSQL storage layer (Docker).
//!
//! JSON 파일 저장소를 완전히 대체합니다. 노트/PDF 본문(BYTEA)/주석(행 단위
//! 스트로크)/용지/북마크/세션/최근 목록/이벤트 로그가 모두 이 모듈을 통해
//! PostgreSQL에 저장됩니다. 파일 I/O는 더 이상 존재하지 않습니다.
//!
//! 연결은 `FREEDF_DATABASE_URL` 환경 변수(기본 `postgres://freedf:freedf@
//! localhost:5432/freedf`)로 결정되고, 시작 시 `migrations/`의 SQL이
//! `schema_migrations` 테이블을 기준으로 순차 적용됩니다.

use freedf_core::model::{Stroke, StrokePoint};
use freedf_core::paper::{PagePaper, PaperStyle};
use freedf_core::store::AnnotationStore;
use postgres::{Client, NoTls};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

/// 기본 연결 문자열 (docker-compose.yml과 일치).
pub const DEFAULT_DATABASE_URL: &str = "postgres://freedf:freedf@localhost:5432/freedf";

const MIGRATIONS: &[(&str, &str)] = &[
    ("0001_init", include_str!("../migrations/0001_init.sql")),
    ("0002_extensions", include_str!("../migrations/0002_extensions.sql")),
    ("0003_word_cache", include_str!("../migrations/0003_word_cache.sql")),
];

/// 문서 행 (documents 테이블).
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
pub struct RecentRow {
    pub kind: String,
    pub doc_id: i64,
    pub title: String,
    pub opened_at: i64,
    pub origin_path: Option<String>,
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

impl Db {
    /// 연결 + 마이그레이션 적용.
    pub fn connect(url: &str) -> Result<Self, String> {
        let mut client = Client::connect(url, NoTls)
            .map_err(|e| format!("Could not connect to PostgreSQL at {url}: {e}"))?;
        Self::migrate(&mut client)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(client)),
        })
    }

    /// 미적용 마이그레이션을 순서대로 적용합니다.
    fn migrate(client: &mut Client) -> Result<(), String> {
        client
            .batch_execute(
                "CREATE TABLE IF NOT EXISTS schema_migrations (version TEXT PRIMARY KEY)",
            )
            .map_err(|e| format!("Migration setup failed: {e}"))?;
        for (version, sql) in MIGRATIONS {
            let applied: Option<String> = client
                .query_opt(
                    "SELECT version FROM schema_migrations WHERE version = $1",
                    &[version],
                )
                .map_err(|e| format!("Migration check failed: {e}"))?
                .map(|r| r.get(0));
            if applied.is_some() {
                continue;
            }
            let mut tx = client
                .transaction()
                .map_err(|e| format!("Migration transaction failed: {e}"))?;
            tx.batch_execute(sql)
                .map_err(|e| format!("Migration {version} failed: {e}"))?;
            tx.execute(
                "INSERT INTO schema_migrations (version) VALUES ($1)",
                &[version],
            )
            .map_err(|e| format!("Migration {version} bookmark failed: {e}"))?;
            tx.commit()
                .map_err(|e| format!("Migration {version} commit failed: {e}"))?;
        }
        Ok(())
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

    pub fn save_pdf(&self, id: i64, bytes: &[u8]) -> Result<(), String> {
        let mut c = conn_guard(&self.conn);
        c.execute(
            "UPDATE documents SET pdf = $2, updated_at = $3 WHERE id = $1",
            &[&id, &bytes.to_vec(), &now_ms()],
        )
        .map(|_| ())
        .map_err(|e| format!("Could not save PDF bytes: {e}"))
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

    pub fn update_page_count(&self, id: i64, page_count: i32) {
        let mut c = conn_guard(&self.conn);
        let _ = c.execute(
            "UPDATE documents SET page_count = $2, updated_at = $3 WHERE id = $1",
            &[&id, &page_count, &now_ms()],
        );
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

    pub fn load_pages(&self, doc_id: i64) -> Vec<(i32, PagePaper, bool)> {
        let mut c = conn_guard(&self.conn);
        c.query(
            "SELECT page_index, style, color, spacing, line_color, line_width, bookmarked
             FROM pages WHERE doc_id = $1 ORDER BY page_index",
            &[&doc_id],
        )
        .map(|rows| {
            rows.iter()
                .map(|r| {
                    let color: Vec<i32> = r.get(2);
                    let line_color: Vec<i32> = r.get(4);
                    let paper = PagePaper {
                        style: style_from(r.get(1)),
                        color: color_from_i32(&color),
                        spacing: r.get(3),
                        line_color: color_from_i32(&line_color),
                        line_width: r.get(5),
                    };
                    (r.get(0), paper, r.get(6))
                })
                .collect()
        })
        .unwrap_or_default()
    }

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
            "INSERT INTO pages (doc_id, page_index, style, color, spacing, line_color, line_width, bookmarked)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (doc_id, page_index) DO UPDATE SET
               style = EXCLUDED.style, color = EXCLUDED.color, spacing = EXCLUDED.spacing,
               line_color = EXCLUDED.line_color, line_width = EXCLUDED.line_width,
               bookmarked = EXCLUDED.bookmarked",
            &[
                &doc_id,
                &page_index,
                &style_str(paper.style),
                &color_to_i32(paper.color),
                &paper.spacing,
                &color_to_i32(paper.line_color),
                &paper.line_width,
                &bookmarked,
            ],
        );
    }

    /// 문서의 pages 테이블 전체를 스토어 상태로 재동기화합니다 (페이지 CRUD/회전 후).
    pub fn replace_pages(&self, doc_id: i64, entries: &[(i32, PagePaper, bool)]) {
        let mut c = conn_guard(&self.conn);
        let mut tx = match c.transaction() {
            Ok(tx) => tx,
            Err(_) => return,
        };
        let _ = tx.execute("DELETE FROM pages WHERE doc_id = $1", &[&doc_id]);
        for (idx, paper, bookmarked) in entries {
            let _ = tx.execute(
                "INSERT INTO pages (doc_id, page_index, style, color, spacing, line_color, line_width, bookmarked)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                &[
                    &doc_id,
                    idx,
                    &style_str(paper.style),
                    &color_to_i32(paper.color),
                    &paper.spacing,
                    &color_to_i32(paper.line_color),
                    &paper.line_width,
                    bookmarked,
                ],
            );
        }
        let _ = tx.commit();
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
    pub fn insert_strokes(&self, doc_id: i64, page_index: i32, strokes: &[Stroke]) {
        if strokes.is_empty() {
            return;
        }
        let mut c = conn_guard(&self.conn);
        let now = now_ms();
        for s in strokes {
            let points: Value = serde_json::to_value(&s.points).unwrap_or(Value::Array(Vec::new()));
            let tool = s.tool.label();
            let _ = c.execute(
                "INSERT INTO strokes (id, doc_id, page_index, tool, color, width, points, created_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                 ON CONFLICT (id) DO NOTHING",
                &[
                    &(s.id as i64),
                    &doc_id,
                    &page_index,
                    &tool,
                    &color_to_i32(s.color),
                    &s.width,
                    &points,
                    &now,
                ],
            );
        }
    }

    /// 스트로크 삭제 (지우개/clear/undo).
    pub fn delete_strokes(&self, doc_id: i64, ids: &[i64]) {
        if ids.is_empty() {
            return;
        }
        let mut c = conn_guard(&self.conn);
        for id in ids {
            let _ = c.execute(
                "DELETE FROM strokes WHERE doc_id = $1 AND id = $2",
                &[&doc_id, id],
            );
        }
    }

    /// 문서의 strokes 테이블 전체를 스토어 상태로 재동기화합니다.
    /// (페이지 삽입/삭제/회전처럼 페이지 인덱스·좌표가 통째로 바뀌는 구조 연산용)
    pub fn resync_strokes(&self, doc_id: i64, store: &AnnotationStore) {
        let mut c = conn_guard(&self.conn);
        let mut tx = match c.transaction() {
            Ok(tx) => tx,
            Err(_) => return,
        };
        let _ = tx.execute("DELETE FROM strokes WHERE doc_id = $1", &[&doc_id]);
        let now = now_ms();
        // 모든 (페이지, 스트로크)를 순서대로 다시 삽입.
        for page in store.pages() {
            for s in &page.strokes {
                let points: Value =
                    serde_json::to_value(&s.points).unwrap_or(Value::Array(Vec::new()));
                let _ = tx.execute(
                    "INSERT INTO strokes (id, doc_id, page_index, tool, color, width, points, created_at)
                     VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
                    &[
                        &(s.id as i64),
                        &doc_id,
                        &(page.page_index as i32),
                        &s.tool.label(),
                        &color_to_i32(s.color),
                        &s.width,
                        &points,
                        &now,
                    ],
                );
            }
        }
        let _ = tx.commit();
    }

    /// 문서의 전체 주석/용지/북마크를 메모리 스토어로 로드합니다.
    pub fn load_store(&self, doc_id: i64) -> AnnotationStore {
        let mut store = AnnotationStore::new();
        // 스트로크 (가드를 블록 안에서 해제 — 아래 load_pages가 다시 잠급니다)
        {
            let mut c = conn_guard(&self.conn);
            if let Ok(rows) = c.query(
                "SELECT page_index, id, tool, color, width, points FROM strokes
                 WHERE doc_id = $1 ORDER BY id",
                &[&doc_id],
            ) {
                let mut grouped: BTreeMap<i32, Vec<Stroke>> = BTreeMap::new();
                for r in &rows {
                    let page_index: i32 = r.get(0);
                    let width: f32 = r.get(4);
                    let points_value: Value = r.get(5);
                    let points: Vec<StrokePoint> =
                        serde_json::from_value(points_value).unwrap_or_default();
                    let tool = freedf_core::model::ToolType::from_label(r.get(2));
                    let color: Vec<i32> = r.get(3);
                    let stroke = Stroke {
                        id: r.get::<_, i64>(1) as u64,
                        tool,
                        color: color_from_i32(&color),
                        width,
                        points,
                    };
                    grouped.entry(page_index).or_default().push(stroke);
                }
                for (page, strokes) in grouped {
                    store.add_strokes(page as usize, strokes);
                }
            }
        }
        // 용지 + 북마크
        for (idx, paper, bookmarked) in self.load_pages(doc_id) {
            store.set_paper(idx as usize, paper);
            if bookmarked {
                store.toggle_bookmark(idx as usize);
            }
        }
        store
    }

    // ---------- sessions ----------

    pub fn load_session(&self, doc_id: i64) -> Option<Value> {
        let mut c = conn_guard(&self.conn);
        c.query_opt("SELECT state FROM sessions WHERE doc_id = $1", &[&doc_id])
            .ok()?
            .map(|r| r.get(0))
    }

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
        let mut c = conn_guard(&self.conn);
        let now = now_ms();
        let _ = c.execute(
            "DELETE FROM recents WHERE kind = $1 AND doc_id = $2",
            &[&kind, &doc_id],
        );
        let _ = c.execute(
            "INSERT INTO recents (kind, doc_id, title, opened_at) VALUES ($1, $2, $3, $4)",
            &[&kind, &doc_id, &title, &now],
        );
    }

    // ---------- edit journal (영속 히스토리) ----------

    /// 편집 하나를 저널에 기록하고 문서당 최근 500건만 유지합니다.
    pub fn log_edit(&self, doc_id: i64, edit: &freedf_core::history::Edit) {
        let mut c = conn_guard(&self.conn);
        let value = serde_json::to_value(edit).unwrap_or(Value::Null);
        let now = now_ms();
        let _ = c.execute(
            "INSERT INTO doc_edits (doc_id, edit, created_at) VALUES ($1, $2, $3)",
            &[&doc_id, &value, &now],
        );
        let _ = c.execute(
            "DELETE FROM doc_edits WHERE doc_id = $1 AND id NOT IN \
             (SELECT id FROM doc_edits WHERE doc_id = $1 ORDER BY id DESC LIMIT 500)",
            &[&doc_id],
        );
    }

    /// 편집 저널을 시간 순으로 로드합니다 (재시작 후 undo 스택 복원용).
    pub fn load_edits(&self, doc_id: i64) -> Vec<freedf_core::history::Edit> {
        let mut c = conn_guard(&self.conn);
        c.query(
            "SELECT edit FROM doc_edits WHERE doc_id = $1 ORDER BY id ASC",
            &[&doc_id],
        )
        .map(|rows| {
            rows.iter()
                .filter_map(|r| {
                    let v: Value = r.get(0);
                    serde_json::from_value(v).ok()
                })
                .collect()
        })
        .unwrap_or_default()
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
        let mut c = conn_guard(&self.conn);
        let _ = c.execute(
            "INSERT INTO event_log (epoch_ms, event) VALUES ($1, $2)",
            &[&(epoch_ms as i64), event],
        );
        let _ = seq;
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

        // ── strokes (시퀀스 id → load_store 일치) ──
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
            },
            Stroke {
                id: ids[1] as u64,
                tool: ToolType::Fountain,
                color: [200, 40, 40, 255],
                width: 3.0,
                points: vec![StrokePoint::new(0.0, 0.0, 1.0)],
            },
        ];
        db.insert_strokes(doc_id, 0, &strokes);
        let store = db.load_store(doc_id);
        assert_eq!(store.total_stroke_count(), 2);
        assert_eq!(store.strokes_on(0)[0].id, ids[0] as u64);
        assert_eq!(store.strokes_on(0)[0].tool, ToolType::Pen);
        assert_eq!(store.strokes_on(0)[0].points.len(), 2);

        // ── pages (용지/북마크) ──
        let paper = PagePaper {
            style: PaperStyle::Grid,
            color: [255, 255, 255, 255],
            spacing: 24.0,
            line_color: [1, 2, 3, 4],
            line_width: 1.0,
        };
        db.upsert_page(doc_id, 0, &paper, true);
        let pages = db.load_pages(doc_id);
        assert_eq!(pages.len(), 1);
        assert!(pages[0].2, "bookmarked");
        assert_eq!(pages[0].1.style, PaperStyle::Grid);
        assert_eq!(pages[0].1.line_color, [1, 2, 3, 4]);

        // ── sessions ──
        let state = serde_json::json!({"page": 2, "zoom": 1.5});
        db.upsert_session(doc_id, &state);
        assert_eq!(db.load_session(doc_id).unwrap(), state);

        // ── app_state ──
        db.set_app_state("smoke-key", &serde_json::json!({"a": 1}));
        assert_eq!(db.get_app_state("smoke-key").unwrap()["a"], 1);

        // ── recents ──
        db.touch_recent("note", doc_id, "Smoke Note");
        assert!(db.load_recents().iter().any(|r| r.doc_id == doc_id));
        assert_eq!(db.load_recents()[0].title, "Smoke Note");

        // ── pdf bytes ──
        db.save_pdf(doc_id, b"%PDF-1.4 real").expect("save pdf");
        assert_eq!(db.load_pdf(doc_id).unwrap(), b"%PDF-1.4 real");

        // ── resync / replace_pages ──
        db.resync_strokes(doc_id, &store);
        assert_eq!(db.load_store(doc_id).total_stroke_count(), 2);
        let entries = vec![(0i32, paper, true), (1i32, paper, false)];
        db.replace_pages(doc_id, &entries);
        assert_eq!(db.load_pages(doc_id).len(), 2);
        assert!(!db.load_pages(doc_id)[1].2);

        // ── event log ──
        db.insert_log(123, 1, &serde_json::json!({"kind": "AppStart"}));

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
        let edits = db.load_edits(doc_id);
        assert_eq!(edits.len(), 2);
        assert!(matches!(edits[0], Edit::AddStrokes { .. }));
        assert!(matches!(edits[1], Edit::RemoveStrokes { .. }));

        // ── media 테이블 존재 (스키마 준비 확인) ──
        {
            let mut c = conn_guard(&db.conn);
            let n: i64 = c
                .query_one("SELECT count(*) FROM media", &[])
                .map(|r| r.get(0))
                .unwrap_or(-1);
            assert_eq!(n, 0);
        }

        // ── word_cache (사전 오버레이 캐시) ──
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
        assert_eq!(db.load_store(doc_id).total_stroke_count(), 0);
        assert!(db.load_session(doc_id).is_none());
        assert!(db.load_recents().iter().all(|r| r.doc_id != doc_id));
        assert!(db.load_edits(doc_id).is_empty());
    }
}
