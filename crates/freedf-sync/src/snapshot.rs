//! 스냅샷 — 문서 전체 상태의 ZIP 왕복 + 패치 병합.
//!
//! ZIP 레이아웃:
//! ```text
//! meta.json       # SnapshotMeta (업로드=base_revision, 다운로드=revision)
//! strokes.jsonl   # Stroke 한 줄씩
//! pages.json      # Page 배열
//! pdf.digest      # "sha256:…" 또는 빈 문자열
//! ```

use std::io::{Cursor, Read, Write};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::digest::Digest;
use crate::error::{Result, SyncError};
use crate::proto::{ChangeRecord, Page, Patch, SnapshotMeta, Stroke};

/// 문서 전체 상태.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub meta: SnapshotMeta,
    pub strokes: Vec<Stroke>,
    pub pages: Vec<Page>,
    pub pdf_digest: Option<Digest>,
    /// 영속 편집 저널(undo) — 서버 doc_edits와 왕복.
    pub edits: Vec<serde_json::Value>,
}

impl Snapshot {
    /// 업로드용 스냅샷 생성 — `base_revision`이 낙관적 동시성의 기준.
    pub fn for_upload(
        base_revision: i64,
        page_count: i32,
        strokes: Vec<Stroke>,
        pages: Vec<Page>,
        pdf_digest: Option<Digest>,
    ) -> Self {
        Self {
            meta: SnapshotMeta {
                revision: None,
                base_revision: Some(base_revision),
                page_count,
                ..SnapshotMeta::default()
            },
            strokes,
            pages,
            pdf_digest,
            edits: Vec::new(),
        }
    }

    pub fn base_revision(&self) -> Option<i64> {
        self.meta.base_revision
    }

    pub fn revision(&self) -> Option<i64> {
        self.meta.revision
    }

    pub fn contains_stroke(&self, id: i64) -> bool {
        self.strokes.iter().any(|s| s.id == id)
    }

    // ── ZIP 코덱 ──────────────────────────────────────────────────────────────

    /// 스냅샷 → ZIP 바이트.
    pub fn to_zip(&self) -> Result<Vec<u8>> {
        let mut buf: Vec<u8> = Vec::new();
        {
            let cur = Cursor::new(&mut buf);
            let mut w = ZipWriter::new(cur);
            let opts =
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

            w.start_file("meta.json", opts)?;
            serde_json::to_writer(&mut w, &self.meta)?;

            w.start_file("strokes.jsonl", opts)?;
            // 스트로크별 소량 write를 deflate 스트림에 직접 흘리면
            // 인코더 호출 오버헤드가 누적되어 매우 느림 (50k획: 18s).
            // → 중간 버퍼에 전부 직렬화한 뒤 한 번에 write_all (≈2s).
            let mut buf: Vec<u8> = Vec::new();
            for s in &self.strokes {
                serde_json::to_writer(&mut buf, s)?;
                buf.push(b'\n');
            }
            w.write_all(&buf)?;

            w.start_file("pages.json", opts)?;
            serde_json::to_writer(&mut w, &self.pages)?;

            w.start_file("pdf.digest", opts)?;
            let digest = self.pdf_digest.as_ref().map(|d| d.as_str()).unwrap_or("");
            w.write_all(digest.as_bytes())?;

            w.start_file("edits.json", opts)?;
            let mut edits_buf: Vec<u8> = Vec::new();
            serde_json::to_writer(&mut edits_buf, &self.edits)?;
            w.write_all(&edits_buf)?;

            w.finish()?;
        }
        Ok(buf)
    }

    /// ZIP 바이트 → 스냅샷.
    pub fn from_zip(bytes: &[u8]) -> Result<Self> {
        let mut zip = ZipArchive::new(Cursor::new(bytes))
            .map_err(|e| SyncError::Decode(format!("invalid zip: {e}")))?;

        let read_entry = |zip: &mut ZipArchive<Cursor<&[u8]>>,
                          name: &str|
         -> Result<Option<String>> {
            let Ok(mut f) = zip.by_name(name) else {
                return Ok(None);
            };
            let mut s = String::new();
            f.read_to_string(&mut s)
                .map_err(|e| SyncError::Decode(format!("cannot read {name}: {e}")))?;
            Ok(Some(s))
        };

        let meta_str = read_entry(&mut zip, "meta.json")?
            .ok_or_else(|| SyncError::Decode("meta.json missing".into()))?;
        let meta: SnapshotMeta = serde_json::from_str(&meta_str)
            .map_err(|e| SyncError::Decode(format!("meta.json: {e}")))?;

        let strokes = match read_entry(&mut zip, "strokes.jsonl")? {
            Some(text) => {
                let mut out = Vec::new();
                for line in text.lines() {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    let s: Stroke = serde_json::from_str(line)
                        .map_err(|e| SyncError::Decode(format!("strokes.jsonl: {e}")))?;
                    out.push(s);
                }
                out
            }
            None => Vec::new(),
        };

        let pages = match read_entry(&mut zip, "pages.json")? {
            Some(text) => {
                serde_json::from_str(&text).map_err(|e| SyncError::Decode(format!("pages.json: {e}")))?
            }
            None => Vec::new(),
        };

        let pdf_digest = match read_entry(&mut zip, "pdf.digest")? {
            Some(text) if !text.trim().is_empty() => Some(Digest::parse(text.trim())?),
            _ => None,
        };

        let edits = match read_entry(&mut zip, "edits.json")? {
            Some(text) => serde_json::from_str(&text)
                .map_err(|e| SyncError::Decode(format!("edits.json: {e}")))?,
            None => Vec::new(),
        };

        Ok(Self {
            meta,
            strokes,
            pages,
            pdf_digest,
            edits,
        })
    }

    // ── 병합 ─────────────────────────────────────────────────────────────────

    /// 충돌 패치 병합 (서버가 계산한 diff를 로컬 상태에 반영).
    ///
    /// - 획: 합집합 — 서버에만 있던 획 추가, 서버가 지운 획 제거.
    ///   클라이언트 로컬 획은 건드리지 않습니다(서버가 모르므로).
    /// - 페이지 구조: 다르면 서버 기준 전체 목록으로 교체.
    /// - meta/pdf: 서버 값 우선.
    /// - 병합 후 `base_revision`을 `patch.to_revision`으로 갱신 → 그대로 재전송 가능.
    pub fn apply_patch(&mut self, patch: &Patch) {
        for s in &patch.strokes_added {
            self.strokes.retain(|x| x.id != s.id);
            self.strokes.push(s.clone());
        }
        for id in &patch.stroke_ids_removed {
            self.strokes.retain(|x| x.id != *id);
        }
        if patch.pages_changed {
            self.meta.page_count = patch.pages.len() as i32;
            self.pages = patch.pages.clone();
        }
        if let Some(pc) = patch.meta.get("page_count").and_then(serde_json::Value::as_i64) {
            self.meta.page_count = pc as i32;
        }
        if let Some(d) = &patch.pdf {
            self.pdf_digest = Some(d.clone());
        }
        self.meta.base_revision = Some(patch.to_revision);
    }

    /// `/changes` JSONL 레코드 적용 (다중 창 pull 동기화).
    pub fn apply_changes(&mut self, records: &[ChangeRecord]) {
        for r in records {
            match r {
                ChangeRecord::StrokeAdded { stroke } => {
                    self.strokes.retain(|x| x.id != stroke.id);
                    self.strokes.push(stroke.clone());
                }
                ChangeRecord::StrokeRemoved { id } => {
                    self.strokes.retain(|x| x.id != *id);
                }
                ChangeRecord::PagesChanged { pages } => {
                    self.meta.page_count = pages.len() as i32;
                    self.pages = pages.clone();
                }
                ChangeRecord::MetaChanged { meta } => {
                    if let Some(pc) = meta.get("page_count").and_then(serde_json::Value::as_i64) {
                        self.meta.page_count = pc as i32;
                    }
                }
                ChangeRecord::PdfChanged { pdf } => {
                    self.pdf_digest = Some(pdf.clone());
                }
            }
        }
    }
}

impl Default for SnapshotMeta {
    fn default() -> Self {
        Self {
            revision: None,
            base_revision: None,
            page_count: 0,
            updated_at: 0,
            title: String::new(),
            kind: String::new(),
            pdf_digest: None,
            session: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto::{ChangeRecord, Page, Patch, StrokePoint};

    fn stroke(id: i64) -> Stroke {
        Stroke {
            id,
            page_index: 0,
            tool: "pen".into(),
            color: vec![0, 0, 0, 255],
            width: 1.0,
            points: vec![StrokePoint {
                x: 1.0,
                y: 2.0,
                pressure: 0.5,
                t_ms: 100,
                width: 1.0,
            }],
            created_at: 0,
        }
    }

    fn page(i: i32) -> Page {
        Page {
            page_index: i,
            style: "Blank".into(),
            color: vec![255, 255, 255, 255],
            bookmarked: false,
        }
    }

    fn sample() -> Snapshot {
        Snapshot::for_upload(
            7,
            1,
            vec![stroke(1), stroke(2)],
            vec![page(0)],
            Some(Digest::from_bytes(b"pdf")),
        )
    }

    #[test]
    fn zip_roundtrip_preserves_everything() {
        let snap = sample();
        let bytes = snap.to_zip().unwrap();
        let back = Snapshot::from_zip(&bytes).unwrap();
        assert_eq!(back, snap);
        assert_eq!(back.base_revision(), Some(7));
        assert_eq!(back.strokes.len(), 2);
        assert_eq!(back.pages.len(), 1);
        assert_eq!(back.pdf_digest, snap.pdf_digest);
    }

    #[test]
    fn zip_with_optional_entries_missing_is_ok() {
        let snap = Snapshot::for_upload(0, 0, vec![], vec![], None);
        let bytes = snap.to_zip().unwrap();
        let back = Snapshot::from_zip(&bytes).unwrap();
        assert!(back.strokes.is_empty());
        assert!(back.pages.is_empty());
        assert_eq!(back.pdf_digest, None);
        assert_eq!(back.base_revision(), Some(0));
    }

    #[test]
    fn zip_missing_meta_fails() {
        use std::io::Write as _;
        let mut buf: Vec<u8> = Vec::new();
        {
            let cur = Cursor::new(&mut buf);
            let mut w = ZipWriter::new(cur);
            let opts = SimpleFileOptions::default();
            w.start_file("strokes.jsonl", opts).unwrap();
            w.write_all(b"\n").unwrap();
            w.finish().unwrap();
        }
        assert!(Snapshot::from_zip(&buf).is_err());
    }

    #[test]
    fn apply_patch_merges_union_and_updates_base() {
        let mut snap = sample(); // strokes 1,2 — base 7
        let patch = Patch {
            from_revision: 7,
            to_revision: 9,
            strokes_added: vec![stroke(3), stroke(2)], // 2는 중복(멱등)
            stroke_ids_removed: vec![1],
            pages_changed: false,
            pages: vec![],
            meta: serde_json::Value::Null,
            pdf: None,
        };
        snap.apply_patch(&patch);
        // 1 제거, 2 유지(교체), 3 추가 → 2,3
        assert!(!snap.contains_stroke(1));
        assert!(snap.contains_stroke(2));
        assert!(snap.contains_stroke(3));
        assert_eq!(snap.strokes.len(), 2);
        // 재전송 준비 — base_revision이 서버 revision으로
        assert_eq!(snap.base_revision(), Some(9));
    }

    #[test]
    fn apply_patch_replaces_pages_and_pdf() {
        let mut snap = sample();
        let patch = Patch {
            from_revision: 7,
            to_revision: 8,
            strokes_added: vec![],
            stroke_ids_removed: vec![],
            pages_changed: true,
            pages: vec![page(0), page(1)],
            meta: serde_json::json!({"page_count": 2}),
            pdf: Some(Digest::from_bytes(b"new pdf")),
        };
        snap.apply_patch(&patch);
        assert_eq!(snap.pages.len(), 2);
        assert_eq!(snap.meta.page_count, 2);
        assert_eq!(snap.pdf_digest, Some(Digest::from_bytes(b"new pdf")));
    }

    #[test]
    fn apply_changes_applies_pull_records() {
        let mut snap = Snapshot::for_upload(0, 1, vec![stroke(1)], vec![page(0)], None);
        let records = vec![
            ChangeRecord::StrokeAdded { stroke: stroke(2) },
            ChangeRecord::StrokeRemoved { id: 1 },
            ChangeRecord::PagesChanged { pages: vec![page(0), page(1)] },
            ChangeRecord::MetaChanged { meta: serde_json::json!({"page_count": 2}) },
            ChangeRecord::PdfChanged { pdf: Digest::from_bytes(b"p") },
        ];
        snap.apply_changes(&records);
        assert_eq!(snap.strokes.len(), 1);
        assert!(snap.contains_stroke(2));
        assert!(!snap.contains_stroke(1));
        assert_eq!(snap.pages.len(), 2);
        assert_eq!(snap.meta.page_count, 2);
        assert_eq!(snap.pdf_digest, Some(Digest::from_bytes(b"p")));
    }
}
