//! 서버 측 Sync v3 핫 패스 벤치마크 (PostgreSQL 직접 측정).
//!
//! 실행:
//!   FREEDF_BENCH_DB=1 \
//!   DATABASE_URL="postgres://freedf:<pw>@127.0.0.1:5432/freedf" \
//!   cargo run -p freedf-media-server --example bench_server --release
//!
//! 측정 항목 (스크래치 문서에 실제 SQL 실행):
//!   1) strokes 삽입 — 현재 방식: jsonb_array_elements 단일 문장
//!   2) strokes 삽입 — 대안: 4,000행 청크 다중 행 INSERT
//!   3) load_state — SELECT strokes ORDER BY id + Stroke 변환
//!   4) assemble — Snapshot 조립 + to_zip (GET /v3/documents/{id} 경로)
//!   5) diff_states — 서버가 충돌 패치 계산하는 인메모리 diff

use freedf_sync::{Digest, Page, Snapshot, SnapshotMeta, Stroke, StrokePoint};
use std::time::Instant;
use tokio_postgres::types::ToSql;
use tokio_postgres::NoTls;

const CHUNK: usize = 4000;

fn make_strokes(count: usize) -> Vec<Stroke> {
    (0..count)
        .map(|i| {
            let mut pts = Vec::with_capacity(40);
            let mut x: f64 = 20.0 + (i as f64 % 40.0) * 15.0;
            let mut y: f64 = 20.0 + (i as f64 % 400.0) * 1.7;
            for k in 0..40 {
                x += 1.8 + (k as f64 * 0.13).sin() * 0.6;
                y += (k as f64 * 0.21).sin() * 1.4;
                pts.push(StrokePoint {
                    x: ((x * 100.0).round() / 100.0) as f32,
                    y: ((y * 100.0).round() / 100.0) as f32,
                    pressure: (0.4 + ((k as f64 * 0.9).sin() * 0.5 + 0.5) * 0.6) as f32,
                    t_ms: (k * 8) as u64,
                    width: 2.0,
                });
            }
            Stroke {
                id: i as i64 + 1,
                page_index: (i % 10) as i32,
                tool: "Pen".into(),
                color: vec![20, 20, 20, 255],
                width: 2.0,
                points: pts,
                created_at: 1_700_000_000_000 + i as i64,
            }
        })
        .collect()
}

/// 현재 방식 — 전체를 JSONB 배열 하나로 보내 jsonb_array_elements로 풀기.
async fn insert_jsonb_array(
    db: &tokio_postgres::Client,
    doc_id: i64,
    strokes: &[Stroke],
) -> f64 {
    db.execute("DELETE FROM strokes WHERE doc_id=$1", &[&doc_id])
        .await
        .expect("delete");
    if strokes.is_empty() {
        return 0.0;
    }
    let arr_text: String = serde_json::to_string(strokes).expect("stroke serialize");
    let t0 = Instant::now();
    db.execute(
        "INSERT INTO strokes (id, doc_id, page_index, tool, color, width, points, created_at) \
         SELECT (s->>'id')::bigint, $1, (s->>'page_index')::int, s->>'tool', \
                ARRAY(SELECT jsonb_array_elements_text(s->'color')::int), \
                COALESCE((s->>'width')::real, 0), s->'points', \
                COALESCE((s->>'created_at')::bigint, 0) \
         FROM jsonb_array_elements($2::text::jsonb) s",
        &[&doc_id, &arr_text],
    )
    .await
    .expect("jsonb insert");
    t0.elapsed().as_secs_f64() * 1000.0
}

/// 대안 — 4,000행 청크 다중 행 INSERT (예전 Db::resync_strokes 방식).
async fn insert_batched(
    db: &tokio_postgres::Client,
    doc_id: i64,
    strokes: &[Stroke],
) -> f64 {
    db.execute("DELETE FROM strokes WHERE doc_id=$1", &[&doc_id])
        .await
        .expect("delete");
    let t0 = Instant::now();
    for chunk in strokes.chunks(CHUNK) {
        let mut params: Vec<Box<dyn ToSql + Sync>> = Vec::with_capacity(chunk.len() * 7 + 1);
        params.push(Box::new(doc_id));
        let mut vals = String::new();
        for (i, s) in chunk.iter().enumerate() {
            if i > 0 {
                vals.push(',');
            }
            let b = 2 + i * 7;
            vals.push_str(&format!(
                "(${b}, $1, ${}, ${}, ${}, ${}, ${}, ${})",
                b + 1,
                b + 2,
                b + 3,
                b + 4,
                b + 5,
                b + 6,
            ));
            params.push(Box::new(s.id));
            params.push(Box::new(s.page_index));
            params.push(Box::new(s.tool.clone()));
            params.push(Box::new(s.color.clone()));
            params.push(Box::new(s.width));
            params.push(Box::new(tokio_postgres::types::Json(s.points.clone())));
            params.push(Box::new(s.created_at));
        }
        let sql = format!(
            "INSERT INTO strokes (id, doc_id, page_index, tool, color, width, points, created_at) \
             VALUES {vals}"
        );
        let refs: Vec<&(dyn ToSql + Sync)> = params.iter().map(|b| b.as_ref()).collect();
        db.execute(&sql, &refs).await.expect("batched insert");
    }
    t0.elapsed().as_secs_f64() * 1000.0
}

/// 대안 2 — 평행 배열 UNNEST: 타입 배열 8개로 한 문장.
/// (jsonb_array_elements의 객체 파싱/함수 호출을 피하고 타입 배열로 전달)
async fn insert_unnest(db: &tokio_postgres::Client, doc_id: i64, strokes: &[Stroke]) -> f64 {
    db.execute("DELETE FROM strokes WHERE doc_id=$1", &[&doc_id])
        .await
        .expect("delete");
    if strokes.is_empty() {
        return 0.0;
    }
    let ids: Vec<i64> = strokes.iter().map(|s| s.id).collect();
    let pages: Vec<i32> = strokes.iter().map(|s| s.page_index).collect();
    let tools: Vec<String> = strokes.iter().map(|s| s.tool.clone()).collect();
    let colors: Vec<String> = strokes
        .iter()
        .map(|s| s.color.iter().map(|c| c.to_string()).collect::<Vec<_>>().join(","))
        .collect();
    let widths: Vec<f32> = strokes.iter().map(|s| s.width).collect();
    let points: Vec<String> = strokes
        .iter()
        .map(|s| serde_json::to_string(&s.points).expect("points serialize"))
        .collect();
    let created: Vec<i64> = strokes.iter().map(|s| s.created_at).collect();
    let t0 = Instant::now();
    db.execute(
        "INSERT INTO strokes (id, doc_id, page_index, tool, color, width, points, created_at) \
         SELECT u.id, $1, u.page_index, u.tool, string_to_array(u.color, ',')::int[], \
                u.width, u.points, u.created_at \
         FROM unnest($2::bigint[], $3::int[], $4::text[], $5::text[], $6::real[], $7::text[]::jsonb[], $8::bigint[]) \
              AS u(id, page_index, tool, color, width, points, created_at)",
        &[
            &doc_id,
            &ids,
            &pages,
            &tools,
            &colors,
            &widths,
            &points,
            &created,
        ],
    )
    .await
    .expect("unnest insert");
    t0.elapsed().as_secs_f64() * 1000.0
}

/// 대안 3 — jsonb_to_recordset: JSONB 배열 하나를 타입 지정 레코드셋으로.
/// (jsonb_array_elements의 수동 →> 캐스팅 연쇄를 피하고 타입 선언으로 파싱)
async fn insert_recordset(db: &tokio_postgres::Client, doc_id: i64, strokes: &[Stroke]) -> f64 {
    db.execute("DELETE FROM strokes WHERE doc_id=$1", &[&doc_id])
        .await
        .expect("delete");
    if strokes.is_empty() {
        return 0.0;
    }
    let arr_text: String = serde_json::to_string(strokes).expect("stroke serialize");
    let t0 = Instant::now();
    db.execute(
        "INSERT INTO strokes (id, doc_id, page_index, tool, color, width, points, created_at) \
         SELECT s.id, $1, s.page_index, s.tool, s.color, s.width, s.points, s.created_at \
         FROM jsonb_to_recordset($2::text::jsonb) AS \
              s(id bigint, page_index int, tool text, color int[], width real, points jsonb, created_at bigint)",
        &[&doc_id, &arr_text],
    )
    .await
    .expect("recordset insert");
    t0.elapsed().as_secs_f64() * 1000.0
}

/// load_state — 적용/조립 공용 경로의 strokes SELECT + 변환.
async fn bench_load_state(db: &tokio_postgres::Client, doc_id: i64) -> (usize, f64) {
    let t0 = Instant::now();
    let rows = db
        .query(
            "SELECT id, page_index, tool, color, width, points, created_at \
             FROM strokes WHERE doc_id=$1 ORDER BY id",
            &[&doc_id],
        )
        .await
        .expect("load strokes");
    let strokes: Vec<Stroke> = rows
        .iter()
        .map(|r| {
            let pts: tokio_postgres::types::Json<Vec<StrokePoint>> = r.get(5);
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
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    (strokes.len(), ms)
}

/// assemble — Snapshot 조립 + ZIP (GET /v3/documents/{id} 경로와 동일 코덱).
fn bench_assemble(strokes: Vec<Stroke>) -> (usize, f64) {
    let snap = Snapshot {
        meta: SnapshotMeta {
            revision: Some(1),
            base_revision: None,
            page_count: 10,
            updated_at: 1_700_000_000_000,
            title: "bench".into(),
            kind: "note".into(),
            pdf_digest: Some(Digest::from_bytes(b"%PDF-fake")),
            session: None,
        },
        pages: (0..10)
            .map(|p| Page {
                page_index: p,
                style: "Grid".into(),
                color: vec![255, 255, 255, 255],
                bookmarked: false,
            })
            .collect(),
        pdf_digest: Some(Digest::from_bytes(b"%PDF-fake")),
        edits: Vec::new(),
        strokes,
    };
    let t0 = Instant::now();
    let zip = snap.to_zip().expect("zip");
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    (zip.len(), ms)
}

fn fmt_ms(ms: f64) -> String {
    if ms < 1000.0 {
        format!("{ms:.1} ms")
    } else {
        format!("{:.2} s", ms / 1000.0)
    }
}

#[tokio::main]
async fn main() {
    if std::env::var("FREEDF_BENCH_DB").as_deref() != Ok("1") {
        eprintln!("FREEDF_BENCH_DB=1 로 켜야 합니다 (스크래치 문서를 실제 DB에 생성/삭제).");
        std::process::exit(1);
    }
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL 필요");
    let (db, conn) = tokio_postgres::connect(&url, NoTls).await.expect("connect");
    tokio::spawn(async move {
        let _ = conn.await;
    });

    let doc_id: i64 = db
        .query_one(
            "INSERT INTO documents (kind, title, page_count, created_at, updated_at) \
             VALUES ('note', 'server-bench', 10, 0, 0) RETURNING id",
            &[],
        )
        .await
        .expect("create doc")
        .get(0);

    println!("획 수 | JSONB배열(현행) | UNNEST | recordset | load_state | assemble ZIP");
    let mut loaded: Vec<Stroke> = Vec::new();
    for n in [1_000usize, 5_000, 50_000] {
        let strokes = make_strokes(n);
        let t_jsonb = insert_jsonb_array(&db, doc_id, &strokes).await;
        let (cnt, t_load) = bench_load_state(&db, doc_id).await;
        let t_unnest = insert_unnest(&db, doc_id, &strokes).await;
        let t_recordset = insert_recordset(&db, doc_id, &strokes).await;
        let t_batch = insert_batched(&db, doc_id, &strokes).await;
        loaded = strokes;
        let (zip, t_zip) = bench_assemble(loaded.clone());
        println!(
            "{:>6} | {:>15} | {:>8} | {:>11} | {:>12} | {:>6} ({:.2} MB)",
            n,
            fmt_ms(t_jsonb),
            fmt_ms(t_unnest),
            fmt_ms(t_recordset),
            fmt_ms(t_load),
            fmt_ms(t_zip),
            zip as f64 / 1_000_000.0,
        );
        assert_eq!(cnt, n);
    }

    // 인메모리 diff (충돌 패치 계산) — 50,000획에서의 비용.
    let other = make_strokes(50_000);
    let t0 = Instant::now();
    let old_ids: std::collections::BTreeSet<i64> = loaded.iter().map(|s| s.id).collect();
    let _added: Vec<&Stroke> = other.iter().filter(|s| !old_ids.contains(&s.id)).collect();
    println!();
    println!("diff(50k) = {}", fmt_ms(t0.elapsed().as_secs_f64() * 1000.0));

    let _ = db
        .execute("DELETE FROM documents WHERE id=$1", &[&doc_id])
        .await;
    for t in ["doc_revisions", "doc_changelog", "sync_uploads"] {
        let _ = db
            .execute(&format!("DELETE FROM {t} WHERE doc_id=$1"), &[&doc_id])
            .await;
    }
    println!("정리 완료 (doc {doc_id})");
}
