//! API 서버 HTTP 클라이언트 (ureq, 블로킹).
//!
//! 앱에서는 **백그라운드 스레드에서만** 호출합니다 (필기 경로에 네트워크 금지).
//! 요청마다 `X-Api-Key` 헤더를 자동으로 붙입니다.

use std::io::Read;
use std::time::Duration;
use ureq::{Agent, AgentBuilder};

use crate::digest::Digest;
use crate::error::{Result, SyncError};
use crate::proto::*;
use crate::snapshot::Snapshot;
use crate::SNAPSHOT_MIME;

const API_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(250);

/// 다운로드 결과 (스냅샷 + 캐시 검증용 ETag).
#[derive(Debug, Clone)]
pub struct Downloaded {
    pub snapshot: Snapshot,
    pub etag: Option<String>,
}

/// 문서 PDF 다운로드 결과 (캐시 검증용 ETag).
#[derive(Debug, Clone)]
pub struct DownloadedPdf {
    pub bytes: Vec<u8>,
    pub etag: Option<String>,
}

/// Sync v3 API 클라이언트.
#[derive(Debug, Clone)]
pub struct SyncClient {
    agent: Agent,
    base: String,
    key: String,
}

impl SyncClient {
    pub fn new(base_url: &str, api_key: &str) -> Result<Self> {
        Self::new_with_timeout(base_url, api_key, API_TIMEOUT)
    }

    /// 타임아웃을 지정해 생성 (UI 즉시 응답용으로 짧게 줄 수 있음).
    pub fn new_with_timeout(base_url: &str, api_key: &str, timeout: Duration) -> Result<Self> {
        let base = base_url.trim_end_matches('/').to_string();
        if base.is_empty() || !(base.starts_with("http://") || base.starts_with("https://")) {
            return Err(SyncError::Transport(format!("invalid base_url: {base_url}")));
        }
        let agent = AgentBuilder::new().timeout(timeout).build();
        Ok(Self {
            agent,
            base,
            key: api_key.to_string(),
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base
    }

    fn request(&self, method: &str, path: &str) -> ureq::Request {
        let url = format!("{}{}", self.base, path);
        let req = match method {
            "GET" => self.agent.get(&url),
            "PUT" => self.agent.put(&url),
            "POST" => self.agent.post(&url),
            "DELETE" => self.agent.delete(&url),
            _ => unreachable!("unsupported method {method}"),
        };
        req.set("X-Api-Key", &self.key)
    }

    fn error_of(&self, e: ureq::Error) -> SyncError {
        match e {
            ureq::Error::Status(status, resp) => {
                let body = resp.into_string().unwrap_or_default();
                match serde_json::from_str::<ApiError>(&body) {
                    Ok(a) => SyncError::Api {
                        status,
                        code: a.code,
                        message: a.message,
                    },
                    Err(_) => SyncError::Http(status, body),
                }
            }
            ureq::Error::Transport(t) => SyncError::Transport(t.to_string()),
        }
    }

    fn into_string(resp: ureq::Response) -> Result<String> {
        resp.into_string().map_err(|e| SyncError::Transport(e.to_string()))
    }

    // ── 기본 ─────────────────────────────────────────────────────────────────

    pub fn health(&self) -> Result<()> {
        self.request("GET", "/health")
            .call()
            .map_err(|e| self.error_of(e))?;
        Ok(())
    }

    // ── 저장 ─────────────────────────────────────────────────────────────────

    /// 전체 문서 스냅샷 업로드 (비동기 — 202 + upload_id).
    pub fn upload_snapshot(&self, doc_id: i64, snapshot: &Snapshot) -> Result<UploadReceipt> {
        let body = snapshot.to_zip()?;
        let resp = self
            .request("PUT", &format!("/v3/documents/{doc_id}/snapshot"))
            .set("Content-Type", SNAPSHOT_MIME)
            .send_bytes(&body)
            .map_err(|e| self.error_of(e))?;
        let text = Self::into_string(resp)?;
        Ok(serde_json::from_str::<UploadReceipt>(&text)?)
    }

    /// 업로드 결과 조회.
    pub fn upload_status(&self, upload_id: uuid::Uuid) -> Result<UploadStatus> {
        let resp = self
            .request("GET", &format!("/v3/uploads/{upload_id}"))
            .call()
            .map_err(|e| self.error_of(e))?;
        let text = Self::into_string(resp)?;
        Ok(serde_json::from_str::<UploadStatus>(&text)?)
    }

    /// 종료 상태(applied/conflict/failed)까지 폴링.
    pub fn wait_upload(&self, upload_id: uuid::Uuid, timeout: Duration) -> Result<UploadStatus> {
        let start = std::time::Instant::now();
        loop {
            let st = self.upload_status(upload_id)?;
            if st.state.is_terminal() {
                return Ok(st);
            }
            if start.elapsed() > timeout {
                return Err(SyncError::Transport(format!(
                    "upload {upload_id} timed out in state {:?}",
                    st.state
                )));
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    }

    /// 업로드 + 완료 대기 — 저장 흐름 한 번에.
    pub fn save_and_wait(
        &self,
        doc_id: i64,
        snapshot: &Snapshot,
        timeout: Duration,
    ) -> Result<UploadStatus> {
        let receipt = self.upload_snapshot(doc_id, snapshot)?;
        self.wait_upload(receipt.upload_id, timeout)
    }

    // ── 로딩 ─────────────────────────────────────────────────────────────────

    /// 서버가 조립한 전체 ZIP 다운로드.
    pub fn download_snapshot(&self, doc_id: i64) -> Result<Downloaded> {
        self.download_inner(doc_id, None)
    }

    /// ETag 기반 조건부 다운로드 — 변경 없으면 `Ok(None)` (304).
    pub fn download_if_changed(&self, doc_id: i64, etag: &str) -> Result<Option<Downloaded>> {
        match self.download_inner(doc_id, Some(etag)) {
            Err(SyncError::Http(304, _)) => Ok(None),
            other => other.map(Some),
        }
    }

    fn download_inner(&self, doc_id: i64, if_none_match: Option<&str>) -> Result<Downloaded> {
        let mut req = self.request("GET", &format!("/v3/documents/{doc_id}"));
        if let Some(etag) = if_none_match {
            req = req.set("If-None-Match", etag);
        }
        let resp = req.call().map_err(|e| self.error_of(e))?;
        // ureq 2.x는 3xx를 리다이렉트로 취급 — 304(Location 없음)는 Ok로 온다.
        if resp.status() == 304 {
            return Err(SyncError::Http(304, String::new()));
        }
        let etag = resp.header("ETag").map(str::to_string);
        let mut reader = resp.into_reader();
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .map_err(|e| SyncError::Transport(e.to_string()))?;
        let snapshot = Snapshot::from_zip(&bytes)?;
        Ok(Downloaded { snapshot, etag })
    }

    /// 현재 revision 조회.
    pub fn revision(&self, doc_id: i64) -> Result<RevisionInfo> {
        let resp = self
            .request("GET", &format!("/v3/documents/{doc_id}/revision"))
            .call()
            .map_err(|e| self.error_of(e))?;
        let text = Self::into_string(resp)?;
        Ok(serde_json::from_str::<RevisionInfo>(&text)?)
    }

    /// since_revision 이후 변경분 (JSONL) — 다중 창 pull 동기화.
    pub fn changes(&self, doc_id: i64, since_revision: i64) -> Result<Vec<ChangeRecord>> {
        let resp = self
            .request(
                "GET",
                &format!("/v3/documents/{doc_id}/changes?since_revision={since_revision}"),
            )
            .call()
            .map_err(|e| self.error_of(e))?;
        let text = Self::into_string(resp)?;
        let mut out = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            out.push(serde_json::from_str(line)?);
        }
        Ok(out)
    }

    // ── CAS ──────────────────────────────────────────────────────────────────

    /// 문서 목록 (라이브러리/최근 목록의 원천).
    pub fn list_documents(&self) -> Result<Vec<DocumentInfo>> {
        let resp = self
            .request("GET", "/v3/documents")
            .call()
            .map_err(|e| self.error_of(e))?;
        let text = Self::into_string(resp)?;
        Ok(serde_json::from_str::<Vec<DocumentInfo>>(&text)?)
    }

    /// 문서 생성 — PDF는 CAS에 먼저 올리고 다이제스트를 넘기세요.
    pub fn create_document(&self, req: &CreateDocument) -> Result<CreatedDocument> {
        let resp = self
            .request("POST", "/v3/documents")
            .set("Content-Type", "application/json")
            .send_json(req)
            .map_err(|e| self.error_of(e))?;
        let text = Self::into_string(resp)?;
        Ok(serde_json::from_str::<CreatedDocument>(&text)?)
    }

    /// 문서 제목 변경.
    pub fn rename_document(&self, doc_id: i64, title: &str) -> Result<()> {
        let body = serde_json::to_vec(&RenameDocument {
            title: title.to_string(),
        })?;
        let resp = self
            .request("PUT", &format!("/v3/documents/{doc_id}/title"))
            .set("Content-Type", "application/json")
            .send_bytes(&body)
            .map_err(|e| self.error_of(e))?;
        if resp.status() == 204 || resp.status() == 200 {
            Ok(())
        } else {
            Err(SyncError::Http(resp.status(), "rename failed".into()))
        }
    }

    /// 문서 삭제 (204).
    pub fn delete_document(&self, doc_id: i64) -> Result<()> {
        let resp = self
            .request("DELETE", &format!("/v3/documents/{doc_id}"))
            .call()
            .map_err(|e| self.error_of(e))?;
        if resp.status() == 204 || resp.status() == 200 {
            Ok(())
        } else {
            Err(SyncError::Http(resp.status(), "delete failed".into()))
        }
    }

    /// 객체 업로드 (멱등) — 다이제스트 반환.
    pub fn put_object(&self, bytes: &[u8]) -> Result<Digest> {
        let digest = Digest::from_bytes(bytes);
        let resp = self
            .request("PUT", &format!("/v3/objects/{digest}"))
            .set("Content-Type", "application/octet-stream")
            .send_bytes(bytes)
            .map_err(|e| self.error_of(e))?;
        let text = Self::into_string(resp)?;
        let stored: ObjectInfo = serde_json::from_str(&text)?;
        Ok(stored.digest)
    }

    /// 객체 fetch.
    pub fn get_object(&self, digest: &Digest) -> Result<Vec<u8>> {
        let resp = self
            .request("GET", &format!("/v3/objects/{digest}"))
            .call()
            .map_err(|e| self.error_of(e))?;
        let mut reader = resp.into_reader();
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .map_err(|e| SyncError::Transport(e.to_string()))?;
        Ok(bytes)
    }

    /// 문서 원본 PDF 다운로드 (서버에 없으면 Err — Http(404)).
    pub fn download_pdf(&self, doc_id: i64) -> Result<DownloadedPdf> {
        match self.download_pdf_inner(doc_id, None)? {
            Some(d) => Ok(d),
            // 무조건 다운로드에 304는 오지 않음 — 방어적으로 빈 결과.
            None => Ok(DownloadedPdf {
                bytes: Vec::new(),
                etag: None,
            }),
        }
    }

    /// ETag 조건부 PDF 다운로드 — 변경 없으면 `Ok(None)` (304).
    pub fn download_pdf_if_changed(&self, doc_id: i64, etag: &str) -> Result<Option<DownloadedPdf>> {
        self.download_pdf_inner(doc_id, Some(etag))
    }

    fn download_pdf_inner(
        &self,
        doc_id: i64,
        if_none_match: Option<&str>,
    ) -> Result<Option<DownloadedPdf>> {
        let mut req = self.request("GET", &format!("/v3/documents/{doc_id}/pdf"));
        if let Some(etag) = if_none_match {
            req = req.set("If-None-Match", etag);
        }
        let resp = req.call().map_err(|e| self.error_of(e))?;
        // ureq 2.x는 3xx를 리다이렉트로 취급 — 304(Location 없음)는 Ok로 온다.
        if resp.status() == 304 {
            return Ok(None);
        }
        if resp.status() != 200 {
            return Err(SyncError::Http(resp.status(), "download pdf failed".into()));
        }
        let etag = resp.header("ETag").map(str::to_string);
        let mut reader = resp.into_reader();
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .map_err(|e| SyncError::Transport(e.to_string()))?;
        Ok(Some(DownloadedPdf { bytes, etag }))
    }

    /// 문서 원본 PDF 업로드/교체 (멱등 — 내용 동일 시 revision 불변).
    pub fn upload_pdf(&self, doc_id: i64, bytes: &[u8]) -> Result<PdfInfo> {
        let resp = self
            .request("PUT", &format!("/v3/documents/{doc_id}/pdf"))
            .set("Content-Type", "application/pdf")
            .send_bytes(bytes)
            .map_err(|e| self.error_of(e))?;
        let text = Self::into_string(resp)?;
        serde_json::from_str(&text).map_err(Into::into)
    }

    /// 서버에 없는 다이제스트만 조회 (업로드 전 dedup).
    pub fn probe_objects(&self, digests: &[Digest]) -> Result<Vec<Digest>> {
        let body = serde_json::to_vec(&DigestProbe {
            digests: digests.to_vec(),
        })?;
        let resp = self
            .request("POST", "/v3/objects/query")
            .set("Content-Type", "application/json")
            .send_bytes(&body)
            .map_err(|e| self.error_of(e))?;
        let text = Self::into_string(resp)?;
        let probe: DigestProbeResult = serde_json::from_str(&text)?;
        Ok(probe.missing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 실서버 대상 e2e — `FREEDF_TEST_SYNC=1`일 때만 실행.
    /// 대상 서버: 127.0.0.1:8080 (FREEDF_API_KEY=test-key), 문서 id는 env로.
    #[test]
    fn sync_client_against_live_server() {
        if std::env::var("FREEDF_TEST_SYNC").ok().as_deref() != Some("1") {
            return; // 평소엔 no-op
        }
        let doc_id: i64 = std::env::var("FREEDF_TEST_SYNC_DOC")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(3);

        let c = SyncClient::new("http://127.0.0.1:8080", "test-key").expect("client");
        c.health().expect("health");

        // 현재 상태 + revision
        let rev = c.revision(doc_id).expect("revision").revision;
        let base_snap = c.download_snapshot(doc_id).expect("download").snapshot;
        assert_eq!(base_snap.revision(), Some(rev));

        // 새 획 추가 업로드 (base=rev) → applied rev+1
        let new_id = 90_000 + rev;
        let mut snap = base_snap.clone();
        snap.strokes.push(test_stroke(new_id));
        snap.meta.base_revision = Some(rev);
        let st = c
            .save_and_wait(doc_id, &snap, Duration::from_secs(20))
            .expect("save");
        assert_eq!(st.state, UploadState::Applied);
        assert_eq!(st.revision, Some(rev + 1));

        // pull — since=rev에 새 획만
        let recs = c.changes(doc_id, rev).expect("changes");
        assert!(recs.iter().any(|r| matches!(r, ChangeRecord::StrokeAdded { stroke } if stroke.id == new_id)));

        // 충돌 — 낡은 base로 재전송 → 서버 계산 패치
        // (서버가 내려준 스냅샷은 revision만 있고 base_revision은 없으므로,
        //  클라이언트가 "rev 시점에 동기화된 상태"임을 명시적으로 표시)
        let mut stale = base_snap.clone();
        stale.meta.base_revision = Some(rev);
        let st = c
            .save_and_wait(doc_id, &stale, Duration::from_secs(20))
            .expect("stale save");
        assert_eq!(st.state, UploadState::Conflict);
        let conflict = st.conflict.clone().expect("conflict detail");
        assert_eq!(conflict.latest_revision, rev + 1);
        assert!(conflict.patch.strokes_added.iter().any(|s| s.id == new_id));

        // 패치 병합 → 재전송 → applied rev+2 (전체 사이클)
        let mut rebased = stale.clone();
        rebased.apply_patch(&conflict.patch);
        assert_eq!(rebased.base_revision(), Some(rev + 1));
        assert!(rebased.contains_stroke(new_id));
        let st = c
            .save_and_wait(doc_id, &rebased, Duration::from_secs(20))
            .expect("rebased save");
        assert_eq!(st.state, UploadState::Applied);
        assert_eq!(st.revision, Some(rev + 2));

        // ETag 조건부 다운로드
        let dl = c.download_snapshot(doc_id).expect("download 2");
        let etag = dl.etag.expect("etag");
        assert!(c
            .download_if_changed(doc_id, &etag)
            .expect("conditional")
            .is_none());

        // CAS
        let d = c.put_object(b"freedf-sync crate object").expect("put object");
        assert_eq!(
            c.get_object(&d).expect("get object"),
            b"freedf-sync crate object"
        );
        let missing = c
            .probe_objects(&[d.clone(), Digest::from_bytes(b"absent object")])
            .expect("probe");
        assert!(!missing.contains(&d));

        // 문서 PDF 업로드/다운로드 (클라우드 스토리지)
        let pdf_bytes: &[u8] = b"%PDF-1.4 freedf live pdf bytes";
        let info = c.upload_pdf(doc_id, pdf_bytes).expect("upload pdf");
        assert_eq!(info.size, pdf_bytes.len() as i64);
        let dl = c.download_pdf(doc_id).expect("download pdf");
        assert_eq!(dl.bytes, pdf_bytes);
        let etag = dl.etag.expect("pdf etag");
        assert!(c
            .download_pdf_if_changed(doc_id, &etag)
            .expect("pdf conditional")
            .is_none());
        // 멱등 — 같은 내용 재업로드는 revision 불변.
        let info2 = c.upload_pdf(doc_id, pdf_bytes).expect("upload pdf again");
        assert_eq!(info2.revision, info.revision);
    }

    fn test_stroke(id: i64) -> Stroke {
        Stroke {
            id,
            page_index: 0,
            tool: "pen".into(),
            color: vec![0, 0, 0, 255],
            width: 1.0,
            points: vec![StrokePoint {
                x: 0.0,
                y: 0.0,
                pressure: 0.5,
                t_ms: 0,
                width: 1.0,
            }],
            created_at: 0,
        }
    }
}
