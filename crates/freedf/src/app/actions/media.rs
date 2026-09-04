//! media — 액션 실행 로직 (자세한 내용은 mod.rs 참조).

use super::*;

impl FreeDfApp {
    /// 목록 조회 작업을 백그라운드 스레드로 실행 (로딩 오버레이 표시).
    pub(crate) fn media_list_job(&mut self) {
        if self.media_rx.is_some() || self.loading.is_some() {
            return;
        }
        let Some(doc_id) = self.doc_id else {
            return;
        };
        let config = self.media_config.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        self.media_rx = Some(rx);
        // 실패해도 재시도 루프가 생기지 않게 문서 id를 먼저 기록.
        self.media_loaded_for = Some(doc_id);
        self.begin_loading("Loading recordings…");
        std::thread::spawn(move || {
            let res = match MediaClient::new_enabled(&config) {
                None => Err("Media server is not enabled — open Server settings.".to_string()),
                Some(client) => client
                    .list(Some(doc_id), 100, 0)
                    .map(MediaOutcome::Listed),
            };
            let _ = tx.send(res);
        });
    }

    /// 현재 문서의 미디어 목록을 서버에서 다시 불러옵니다 (비동기).
    pub(crate) fn media_refresh(&mut self) {
        self.media_list_job();
    }

    /// 업로드 파일 선택 — Windows는 네이티브 대화상자, 그 외엔 경로 입력.
    pub(crate) fn upload_media_dialog(&mut self) {
        #[cfg(target_os = "windows")]
        {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter(
                    "Audio files",
                    &["m4a", "mp3", "wav", "webm", "ogg", "aac", "flac", "m4b", "opus"],
                )
                .pick_file()
            {
                self.upload_media_path(&path);
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            self.modal = Some(ModalState::ask_text(
                "Upload recording",
                "Enter the audio file path (e.g. /home/me/rec.m4a)",
                TextAction::UploadMedia,
            ));
        }
    }

    /// 파일을 현재 문서의 녹음으로 업로드합니다 (비동기 — UI를 막지 않음).
    pub(crate) fn upload_media_path(&mut self, path: &Path) {
        if self.media_rx.is_some() || self.loading.is_some() {
            return;
        }
        let Some(doc_id) = self.doc_id else {
            self.media_status = Some("Open a document first.".into());
            return;
        };
        let Some(client) = MediaClient::new_enabled(&self.media_config) else {
            self.media_status = Some("Media server is not enabled — open Server settings.".into());
            return;
        };
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "upload.bin".into());
        let path = path.to_path_buf();
        let (tx, rx) = std::sync::mpsc::channel();
        self.media_rx = Some(rx);
        self.begin_loading(format!("Uploading {name}…"));
        std::thread::spawn(move || {
            let res = (|| -> Result<MediaOutcome, String> {
                let bytes =
                    std::fs::read(&path).map_err(|e| format!("Could not read file: {e}"))?;
                if bytes.len() > 200 * 1024 * 1024 {
                    return Err("File is larger than 200 MB (server limit).".into());
                }
                let mime = crate::server::mime_for_ext(&name);
                client
                    .upload(Some(doc_id), "audio", &name, mime, &bytes)
                    .map(MediaOutcome::Uploaded)
            })();
            let _ = tx.send(res);
        });
    }

    /// 녹음 하나를 서버에서 삭제합니다 (비동기 — 파일 + 메타데이터).
    pub(crate) fn delete_media_item(&mut self, id: i64) {
        if self.media_rx.is_some() || self.loading.is_some() {
            return;
        }
        let Some(client) = MediaClient::new_enabled(&self.media_config) else {
            self.media_status = Some("Media server is not enabled.".into());
            return;
        };
        let (tx, rx) = std::sync::mpsc::channel();
        self.media_rx = Some(rx);
        self.begin_loading("Deleting recording…");
        std::thread::spawn(move || {
            let res = client.delete(id).map(|()| MediaOutcome::Deleted);
            let _ = tx.send(res);
        });
    }

    /// 녹음 URL을 OS 기본 미디어 플레이어로 엽니다 (nginx가 스트리밍).
    pub(crate) fn play_media_item(&mut self, url: String) {
        if let Err(e) = open_in_system_player(&url) {
            self.media_status = Some(format!("Could not open player: {e}"));
        }
    }
}


#[cfg(target_os = "windows")]
fn open_in_system_player(url: &str) -> std::io::Result<()> {
    std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn()
        .map(|_| ())
}

#[cfg(target_os = "macos")]
fn open_in_system_player(url: &str) -> std::io::Result<()> {
    std::process::Command::new("open").arg(url).spawn().map(|_| ())
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn open_in_system_player(url: &str) -> std::io::Result<()> {
    std::process::Command::new("xdg-open")
        .arg(url)
        .spawn()
        .map(|_| ())
}
