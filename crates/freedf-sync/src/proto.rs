//! wire 타입 — 서버 응답·JSONL 레코드·패치 (OpenAPI schemas와 1:1).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::digest::Digest;

/// strokes.jsonl 한 줄의 점 — {x, y, pressure, t_ms, width} (OpenAPI와 1:1).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct StrokePoint {
    pub x: f32,
    pub y: f32,
    #[serde(default)]
    pub pressure: f32,
    #[serde(default)]
    pub t_ms: u64,
    #[serde(default)]
    pub width: f32,
}

/// strokes.jsonl 한 줄 (= DB strokes 행과 같은 모양).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Stroke {
    pub id: i64,
    pub page_index: i32,
    pub tool: String,
    /// [r, g, b, a]
    pub color: Vec<i32>,
    #[serde(default)]
    pub width: f32,
    /// [{x, y, pressure, t_ms, width}, ...]
    pub points: Vec<StrokePoint>,
    #[serde(default)]
    pub created_at: i64,
}

/// pages.json 한 항목.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Page {
    pub page_index: i32,
    #[serde(default = "default_style")]
    pub style: String,
    /// [r, g, b, a]
    pub color: Vec<i32>,
    #[serde(default)]
    pub bookmarked: bool,
}

fn default_style() -> String {
    "Blank".to_string()
}

/// 스냅샷의 meta.json — 업로드/다운로드 공용.
///
/// - 업로드: 클라이언트가 `base_revision`을 제출.
/// - 다운로드: 서버가 `revision`을 발급.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SnapshotMeta {
    /// 서버가 발급한 revision (다운로드 메타).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<i64>,
    /// 클라이언트가 제출한 기준 revision (업로드 메타).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_revision: Option<i64>,
    pub page_count: i32,
    #[serde(default)]
    pub updated_at: i64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pdf_digest: Option<Digest>,
    /// 문서별 GUI 세션 (서버 sessions 테이블과 왕복).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<serde_json::Value>,
}

/// 두 상태 간 diff — 변경 로그와 충돌 패치에 공통 사용.
///
/// 서버가 "클라이언트가 보낸 전체 상태"와 "서버 현재 상태"를 비교해 계산합니다.
/// 클라이언트는 [`crate::Snapshot::apply_patch`]로 병합 후 재전송하면 됩니다.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Patch {
    pub from_revision: i64,
    pub to_revision: i64,
    #[serde(default)]
    pub strokes_added: Vec<Stroke>,
    #[serde(default)]
    pub stroke_ids_removed: Vec<i64>,
    #[serde(default)]
    pub pages_changed: bool,
    /// pages_changed일 때만 유효 (서버 기준 전체 목록).
    #[serde(default)]
    pub pages: Vec<Page>,
    /// null 또는 `{"page_count": n}`.
    #[serde(default)]
    pub meta: serde_json::Value,
    /// 바뀌었을 때만 Some.
    #[serde(default)]
    pub pdf: Option<Digest>,
}

/// `/changes` JSONL 한 줄 — `op` 태그로 판별.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op")]
pub enum ChangeRecord {
    #[serde(rename = "add")]
    StrokeAdded { stroke: Stroke },
    #[serde(rename = "remove")]
    StrokeRemoved { id: i64 },
    #[serde(rename = "pages")]
    PagesChanged { pages: Vec<Page> },
    #[serde(rename = "meta")]
    MetaChanged { meta: serde_json::Value },
    #[serde(rename = "pdf")]
    PdfChanged { pdf: Digest },
}

/// 비동기 업로드 작업 상태.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UploadState {
    Queued,
    Processing,
    Applied,
    Conflict,
    Failed,
    #[serde(other)]
    Unknown,
}

impl UploadState {
    pub fn is_terminal(&self) -> bool {
        matches!(self, UploadState::Applied | UploadState::Conflict | UploadState::Failed)
    }
}

/// PUT /v3/documents/{id}/snapshot 응답 (202).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UploadReceipt {
    pub upload_id: Uuid,
    pub state: UploadState,
}

/// GET /v3/uploads/{upload_id} 응답.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UploadStatus {
    pub upload_id: Uuid,
    pub state: UploadState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conflict: Option<Conflict>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// 충돌 상세 — 서버가 계산한 패치 동봉.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Conflict {
    pub latest_revision: i64,
    pub patch: Patch,
}

/// GET /v3/documents/{id}/revision 응답.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RevisionInfo {
    pub document_id: i64,
    pub revision: i64,
}

/// POST /v3/objects/query 요청.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DigestProbe {
    pub digests: Vec<Digest>,
}

/// POST /v3/objects/query 응답 — 서버에 없는 다이제스트만.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DigestProbeResult {
    pub missing: Vec<Digest>,
}

/// 서버 오류 본문.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

/// GET /v3/documents 목록 항목 (라이브러리/최근 목록의 원천).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DocumentInfo {
    pub id: i64,
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub origin_path: Option<String>,
    pub page_count: i32,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
    #[serde(default)]
    pub pdf_digest: Option<Digest>,
}

/// POST /v3/documents 요청 — PDF는 CAS에 먼저 올리고 다이제스트를 넘깁니다.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreateDocument {
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub origin_path: Option<String>,
    #[serde(default)]
    pub page_count: i32,
    #[serde(default)]
    pub pdf_digest: Option<Digest>,
}

/// POST /v3/documents 응답.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CreatedDocument {
    pub id: i64,
}

/// PUT /v3/documents/{id}/title 요청.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RenameDocument {
    pub title: String,
}

/// PUT /v3/objects/{digest} 응답.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ObjectInfo {
    pub digest: Digest,
    pub size: u64,
}

/// PUT /v3/documents/{id}/pdf 응답.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PdfInfo {
    pub digest: Digest,
    pub size: i64,
    /// PDF 변경으로 갱신된 문서 revision (내용 동일 시 기존 revision 그대로).
    pub revision: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_stroke(id: i64) -> Stroke {
        Stroke {
            id,
            page_index: 0,
            tool: "pen".into(),
            color: vec![10, 20, 30, 255],
            width: 2.0,
            points: vec![StrokePoint {
                x: 1.0,
                y: 2.0,
                pressure: 0.5,
                t_ms: 100,
                width: 2.0,
            }],
            created_at: 1000,
        }
    }

    #[test]
    fn upload_status_parses_server_shapes() {
        // applied
        let st: UploadStatus =
            serde_json::from_str(r#"{"upload_id":"f82a6e13-0ab2-42c7-bddb-95f2a9ee0cb4","state":"applied","revision":2}"#)
                .unwrap();
        assert_eq!(st.state, UploadState::Applied);
        assert_eq!(st.revision, Some(2));
        // conflict (서버 실응답 모양 — patch 동봉)
        let st: UploadStatus = serde_json::from_str(
            r#"{"upload_id":"f82a6e13-0ab2-42c7-bddb-95f2a9ee0cb4","state":"conflict",
                "conflict":{"latest_revision":3,"patch":{"from_revision":1,"to_revision":3,
                "strokes_added":[],"stroke_ids_removed":[],"pages_changed":false,"pages":[],
                "meta":null,"pdf":null}}}"#,
        )
        .unwrap();
        assert_eq!(st.state, UploadState::Conflict);
        let c = st.conflict.unwrap();
        assert_eq!(c.latest_revision, 3);
        assert_eq!(c.patch.from_revision, 1);
        // failed
        let st: UploadStatus =
            serde_json::from_str(r#"{"upload_id":"f82a6e13-0ab2-42c7-bddb-95f2a9ee0cb4","state":"failed","error":"db: x"}"#)
                .unwrap();
        assert_eq!(st.state, UploadState::Failed);
        // 미지 상태
        let st: UploadStatus =
            serde_json::from_str(r#"{"upload_id":"f82a6e13-0ab2-42c7-bddb-95f2a9ee0cb4","state":"weird"}"#)
                .unwrap();
        assert_eq!(st.state, UploadState::Unknown);
        assert!(!st.state.is_terminal());
    }

    #[test]
    fn patch_deserializes_and_keeps_fields() {
        let patch: Patch = serde_json::from_str(
            r#"{"from_revision":1,"to_revision":2,
                "strokes_added":[],"stroke_ids_removed":[7,8],
                "pages_changed":true,"pages":[{"page_index":0,"style":"Blank","color":[255,255,255,255]}],
                "meta":{"page_count":1},"pdf":null}"#,
        )
        .unwrap();
        assert_eq!(patch.stroke_ids_removed, vec![7, 8]);
        assert!(patch.pages_changed);
        assert_eq!(patch.pages.len(), 1);
        assert_eq!(patch.meta["page_count"], 1);
    }

    #[test]
    fn change_record_jsonl_roundtrip() {
        let rec = ChangeRecord::StrokeAdded { stroke: sample_stroke(42) };
        let line = serde_json::to_string(&rec).unwrap();
        assert!(line.starts_with(r#"{"op":"add""#));
        let back: ChangeRecord = serde_json::from_str(&line).unwrap();
        assert_eq!(rec, back);
    }
}
