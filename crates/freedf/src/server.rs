//! 미디어 리소스 서버 연결 설정 + 클라이언트 (로드맵 ①).
//!
//! FreeDF의 미디어(음성 녹음 등)는 자체 호스팅 VPS의 얇은 API
//! (`server/` 디렉터리 참조)로 관리합니다. 클라이언트는 **빌드타임이 아니라
//! 런타임에** `server.json`(앱 데이터 폴더)에서 연결 정보를 읽습니다.
//!
//! - 다운로드/재생: nginx가 `/media/*`를 직접 서빙 → 응답의 `url`만 사용
//! - 업로드/목록/삭제: 이 모듈의 동기 HTTP 클라이언트 (`X-Api-Key` 인증)
//!
//! HTTP 호출은 전부 사용자가 버튼을 누를 때만 실행되는 **동기** 호출입니다
//! (4초 타임아웃). 배치 업로드/백그라운드 동기화는 로드맵 ④(로컬 캐시 +
//! write-behind) 단계에서 스레드로 분리합니다.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// 미디어 서버 연결 설정 (`server.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MediaServerConfig {
    /// 서버 기능 사용 여부 (비활성이면 모든 API 호출을 하지 않음).
    pub enabled: bool,
    /// 예: `https://media.example.com` (끝 슬래시 무시).
    pub base_url: String,
    /// API 키 — 서버의 `FREEDF_API_KEY`와 일치해야 함.
    pub api_key: String,
}

impl Default for MediaServerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_url: "https://media.example.com".into(),
            api_key: String::new(),
        }
    }
}

impl MediaServerConfig {
    /// 설정 파일 경로: `<앱 데이터 폴더>/server.json` (Windows:
    /// `%LOCALAPPDATA%/FreeDF/server.json`).
    pub fn config_path() -> PathBuf {
        crate::storage::app_data_dir().join("server.json")
    }

    /// 파일에서 로드 — 없거나 깨졌으면 기본값.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// JSON(pretty)으로 저장, 부모 폴더 자동 생성.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, json)
    }

    /// URL 경로 조합용 — 앞뒤 공백과 끝 슬래시 제거.
    fn normalized_base(&self) -> String {
        self.base_url.trim().trim_end_matches('/').to_string()
    }
}

/// 서버가 반환하는 미디어 객체 (백엔드 JSON과 1:1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaObject {
    pub id: i64,
    pub doc_id: Option<i64>,
    pub kind: String,
    pub name: String,
    pub mime: String,
    pub size: i64,
    /// 공개 URL — nginx가 직접 서빙하는 주소 (재생/다운로드용).
    pub url: String,
}

/// 미디어 API 클라이언트 (동기, 4초 타임아웃).
#[derive(Clone)]
pub struct MediaClient {
    base: String,
    api_key: String,
    agent: ureq::Agent,
}

impl MediaClient {
    pub fn new(config: &MediaServerConfig) -> Self {
        Self {
            base: config.normalized_base(),
            api_key: config.api_key.clone(),
            agent: ureq::AgentBuilder::new()
                .timeout(std::time::Duration::from_secs(4))
                .build(),
        }
    }

    /// 설정이 활성화(enabled)됐을 때만 클라이언트를 만듭니다.
    pub fn new_enabled(config: &MediaServerConfig) -> Option<Self> {
        config.enabled.then(|| Self::new(config))
    }

    /// GET 요청 (X-Api-Key 포함). 상태 코드가 2xx가 아니면 Err.
    fn get(&self, path: &str) -> Result<ureq::Response, String> {
        self.agent
            .get(&format!("{}{}", self.base, path))
            .set("X-Api-Key", &self.api_key)
            .call()
            .map_err(http_err)
    }

    /// 연결 확인 — `GET /health`가 200이면 Ok.
    pub fn health(&self) -> Result<(), String> {
        let resp = self.get("/health")?;
        match resp.status() {
            200 => Ok(()),
            s => Err(format!("HTTP {s}")),
        }
    }

    /// 미디어 목록 (최신순). `doc_id`가 Some이면 해당 문서만.
    pub fn list(
        &self,
        doc_id: Option<i64>,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<MediaObject>, String> {
        let mut query = vec![format!("limit={}", limit.clamp(1, 500)), format!("offset={offset}")];
        if let Some(d) = doc_id {
            query.push(format!("doc_id={d}"));
        }
        let resp = self.get(&format!("/api/media?{}", query.join("&")))?;
        resp.into_json::<Vec<MediaObject>>().map_err(|e| e.to_string())
    }

    /// 파일 업로드 (multipart/form-data 직접 구성 — ureq 2.x에는
    /// multipart 헬퍼 모듈이 없으므로 본문을 수동 조립).
    pub fn upload(
        &self,
        doc_id: Option<i64>,
        kind: &str,
        file_name: &str,
        mime: &str,
        data: &[u8],
    ) -> Result<MediaObject, String> {
        let boundary = format!("freedf-{}", std::process::id());
        let body = multipart_body(&boundary, file_name, mime, data);
        let mut query = format!("kind={}", urlencode(kind));
        if let Some(d) = doc_id {
            query.push_str(&format!("&doc_id={d}"));
        }
        let resp = self
            .agent
            .post(&format!("{}/api/media?{query}", self.base))
            .set("X-Api-Key", &self.api_key)
            .set("Content-Type", &format!("multipart/form-data; boundary={boundary}"))
            .send_bytes(&body)
            .map_err(http_err)?;
        resp.into_json::<MediaObject>().map_err(|e| e.to_string())
    }

    /// 삭제 — 파일과 메타데이터 행을 함께 제거.
    pub fn delete(&self, id: i64) -> Result<(), String> {
        let resp = self
            .agent
            .delete(&format!("{}/api/media/{id}", self.base))
            .set("X-Api-Key", &self.api_key)
            .call()
            .map_err(http_err)?;
        match resp.status() {
            204 => Ok(()),
            404 => Err("no such media object on server".into()),
            s => Err(format!("HTTP {s}")),
        }
    }
}

/// ureq 오류 → 사용자에게 보여줄 짧은 문자열.
fn http_err(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, resp) => {
            let body = resp.into_string().unwrap_or_default();
            if body.trim().is_empty() {
                format!("HTTP {code}")
            } else {
                format!("HTTP {code}: {}", body.trim())
            }
        }
        other => other.to_string(),
    }
}

/// 파일 확장자 → MIME (업로드 Content-Type 힌트용).
pub fn mime_for_ext(file_name: &str) -> &'static str {
    let ext = std::path::Path::new(file_name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "m4a" | "m4b" | "mp4" => "audio/mp4",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "webm" => "audio/webm",
        "ogg" | "oga" => "audio/ogg",
        "aac" => "audio/aac",
        "flac" => "audio/flac",
        "opus" => "audio/opus",
        _ => "application/octet-stream",
    }
}

/// multipart/form-data 본문 (파일 필드 하나).
fn multipart_body(boundary: &str, file_name: &str, mime: &str, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 256);
    out.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    out.extend_from_slice(
        format!(
            "Content-Disposition: form-data; name=\"file\"; filename=\"{file_name}\"\r\n"
        )
        .as_bytes(),
    );
    out.extend_from_slice(format!("Content-Type: {mime}\r\n\r\n").as_bytes());
    out.extend_from_slice(data);
    out.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    out
}

/// 쿼리 값 URL 인코딩 (영숫자/-_/. 외에는 %XX).
fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_disabled_with_empty_key() {
        let c = MediaServerConfig::default();
        assert!(!c.enabled);
        assert!(c.api_key.is_empty());
        assert!(c.base_url.contains("://"));
    }

    #[test]
    fn config_roundtrips_through_file() {
        let dir = std::env::temp_dir().join(format!("freedf-server-test-{}", std::process::id()));
        let path = dir.join("nested").join("server.json");
        let cfg = MediaServerConfig {
            enabled: true,
            base_url: "https://media.example.com/".into(),
            api_key: "secret".into(),
        };
        cfg.save(&path).expect("save");
        let loaded = MediaServerConfig::load(&path);
        assert_eq!(loaded.enabled, true);
        assert_eq!(loaded.api_key, "secret");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn normalized_base_trims_slash_and_whitespace() {
        let cfg = MediaServerConfig {
            base_url: "  https://media.example.com//  ".into(),
            ..Default::default()
        };
        assert_eq!(cfg.normalized_base(), "https://media.example.com");
    }

    #[test]
    fn multipart_body_contains_boundary_and_payload() {
        let body = multipart_body("BOUNDARY", "rec.m4a", "audio/mp4", b"\x00\x01hello");
        let text = String::from_utf8_lossy(&body);
        assert!(text.starts_with("--BOUNDARY\r\n"));
        assert!(text.contains("name=\"file\"; filename=\"rec.m4a\""));
        assert!(text.contains("Content-Type: audio/mp4"));
        assert!(text.ends_with("\r\n--BOUNDARY--\r\n"));
        // 페이로드 바이트가 그대로 포함됨.
        let needle = &b"\x00\x01hello"[..];
        assert!(body.windows(needle.len()).any(|w| w == needle));
    }

    #[test]
    fn urlencode_keeps_safe_chars_only() {
        assert_eq!(urlencode("audio"), "audio");
        assert_eq!(urlencode("a b"), "a%20b");
    }

    #[test]
    fn mime_for_ext_maps_common_audio_types() {
        assert_eq!(mime_for_ext("rec.m4a"), "audio/mp4");
        assert_eq!(mime_for_ext("REC.MP3"), "audio/mpeg");
        assert_eq!(mime_for_ext("voice.wav"), "audio/wav");
        assert_eq!(mime_for_ext("noext"), "application/octet-stream");
    }

    /// 실서버 왕복 — `FREEDF_TEST_MEDIA=1 cargo test -p freedf media_client_against_live_server`
    /// (로컬에서 서버/backend를 127.0.0.1:9091로 띄운 상태에서 실행).
    #[test]
    fn media_client_against_live_server() {
        if std::env::var("FREEDF_TEST_MEDIA").as_deref() != Ok("1") {
            return;
        }
        let cfg = MediaServerConfig {
            enabled: true,
            base_url: "http://127.0.0.1:9091".into(),
            api_key: "e2e-test-key".into(),
        };
        let client = MediaClient::new(&cfg);
        client.health().expect("health");

        // 수동 조립한 multipart 본문이 axum Multipart가 파싱하는지가 핵심.
        let obj = client
            .upload(Some(43), "audio", "client-upload.m4a", "audio/mp4", b"CLIENT-BODY-TEST")
            .expect("upload");
        assert_eq!(obj.name, "client-upload.m4a");
        assert_eq!(obj.doc_id, Some(43));

        let items = client.list(Some(43), 10, 0).expect("list");
        assert!(items.iter().any(|m| m.id == obj.id), "uploaded item missing");

        client.delete(obj.id).expect("delete");
        let after = client.list(Some(43), 10, 0).expect("list after delete");
        assert!(!after.iter().any(|m| m.id == obj.id));
    }
}
