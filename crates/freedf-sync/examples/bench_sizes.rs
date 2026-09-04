//! 실제 필기 부하 시 스냅샷 ZIP 크기/시간 실측.
//!
//! 실행: cargo run -p freedf-sync --example bench_sizes --release
//!
//! 합성 스트로크: 획당 40점, 점당 {x,y,pressure,t_ms,width} — 평균적인
//! 일반 필기(1획 0.2~0.5초, 100~200Hz 샘플링)에 해당하는 밀도입니다.
//! (참고: serde_json::Value 경유라 실제 앱의 직렬화 성능 하한에 가깝습니다)

use freedf_sync::{Digest, Snapshot, SnapshotMeta, Stroke};
use serde_json::json;
use std::io::{Cursor, Write};
use std::time::Instant;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

fn make_strokes(count: usize) -> Vec<Stroke> {
    (0..count)
        .map(|i| {
            let mut pts = Vec::with_capacity(40);
            let mut x: f64 = 20.0 + (i as f64 % 40.0) * 15.0;
            let mut y: f64 = 20.0 + (i as f64 % 400.0) * 1.7;
            for k in 0..40 {
                x += 1.8 + (k as f64 * 0.13).sin() * 0.6;
                y += (k as f64 * 0.21).sin() * 1.4;
                pts.push(json!({
                    "x": (x * 100.0).round() / 100.0,
                    "y": (y * 100.0).round() / 100.0,
                    "pressure": 0.4 + ((k as f64 * 0.9).sin() * 0.5 + 0.5) * 0.6,
                    "t_ms": k * 8,
                    "width": 2.0,
                }));
            }
            Stroke {
                id: i as i64 + 1,
                page_index: (i % 10) as i32,
                tool: "Pen".into(),
                color: vec![20, 20, 20, 255],
                width: 2.0,
                points: serde_json::Value::Array(pts),
                created_at: 1_700_000_000_000 + i as i64,
            }
        })
        .collect()
}

fn make_snapshot(strokes: Vec<Stroke>) -> Snapshot {
    Snapshot {
        meta: SnapshotMeta {
            revision: Some(7),
            base_revision: None,
            page_count: 10,
            updated_at: 1_700_000_000_000,
            title: "bench".into(),
            kind: "note".into(),
            pdf_digest: Some(Digest::from_bytes(b"%PDF-fake")),
            session: Some(json!({"page": 3, "zoom": 1.2})),
        },
        pages: (0..10)
            .map(|p| freedf_sync::Page {
                page_index: p,
                style: "Grid".into(),
                color: vec![255, 255, 255, 255],
                bookmarked: p == 0,
            })
            .collect(),
        pdf_digest: Some(Digest::from_bytes(b"%PDF-fake")),
        edits: (0..strokes.len() / 2)
            .map(|i| json!({"op": "AddStrokes", "page": 0, "ids": [i]}))
            .collect(),
        strokes,
    }
}

/// 지정 압축 레벨로 스냅샷 ZIP 생성 → (zip 바이트, 소요 ms).
fn zip_with_level(snap: &Snapshot, level: Option<i64>) -> (usize, f64) {
    let t0 = Instant::now();
    let mut buf: Vec<u8> = Vec::new();
    {
        let cur = Cursor::new(&mut buf);
        let mut w = ZipWriter::new(cur);
        let opts = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .compression_level(level);

        w.start_file("meta.json", opts).expect("meta");
        serde_json::to_writer(&mut w, &snap.meta).expect("meta json");
        w.start_file("strokes.jsonl", opts).expect("strokes");
        for s in &snap.strokes {
            serde_json::to_writer(&mut w, s).expect("stroke json");
            w.write_all(b"\n").expect("nl");
        }
        w.start_file("pages.json", opts).expect("pages");
        serde_json::to_writer(&mut w, &snap.pages).expect("pages json");
        w.start_file("pdf.digest", opts).expect("digest");
        w.write_all(
            snap.pdf_digest
                .as_ref()
                .map(|d| d.as_str())
                .unwrap_or("")
                .as_bytes(),
        )
        .expect("digest write");
        w.start_file("edits.json", opts).expect("edits");
        serde_json::to_writer(&mut w, &snap.edits).expect("edits json");
        w.finish().expect("finish");
    }
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    (buf.len(), ms)
}

fn transfer_label(bytes: usize, bps: f64) -> String {
    let secs = bytes as f64 * 8.0 / bps;
    if secs < 0.001 {
        "<1 ms".into()
    } else if secs < 1.0 {
        format!("{:.0} ms", secs * 1000.0)
    } else {
        format!("{:.1} s", secs)
    }
}

fn fmt_kb(b: usize) -> String {
    if b >= 1_000_000 {
        format!("{:.2} MB", b as f64 / 1_000_000.0)
    } else {
        format!("{:.0} KB", b as f64 / 1000.0)
    }
}

fn main() {
    println!("── 압축 레벨별 (레벨 1 = fast, 6 = 기본) ──");
    println!("획 수 | 원본 | L1 ZIP | L1 시간 | L6 ZIP | L6 시간");
    for n in [100usize, 1_000, 5_000, 50_000] {
        let snap = make_snapshot(make_strokes(n));
        let raw: usize = snap.strokes.iter().map(|s| s.points.to_string().len() + 90).sum();
        let (z1, t1) = zip_with_level(&snap, Some(1));
        let (z6, t6) = zip_with_level(&snap, Some(6));
        println!(
            "{:>6} | {} | {} | {:>6.1} ms | {} | {:>7.1} ms",
            n,
            fmt_kb(raw),
            fmt_kb(z1),
            t1,
            fmt_kb(z6),
            t6,
        );
    }

    println!();
    println!("── 전송 시간 (레벨 6 ZIP 기준, RTT 미포함) ──");
    println!("획 수 | ZIP | 1Gbps | 100M | 50M | 10M | 5M");
    for n in [100usize, 1_000, 5_000, 50_000] {
        let snap = make_snapshot(make_strokes(n));
        let (z, _) = zip_with_level(&snap, Some(6));
        println!(
            "{:>6} | {} | {} | {} | {} | {} | {}",
            n,
            fmt_kb(z),
            transfer_label(z, 1_000_000_000.0),
            transfer_label(z, 100_000_000.0),
            transfer_label(z, 50_000_000.0),
            transfer_label(z, 10_000_000.0),
            transfer_label(z, 5_000_000.0),
        );
    }
}
