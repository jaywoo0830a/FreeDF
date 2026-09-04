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

    /// 녹음을 서버에서 스트리밍 다운로드한 뒤 앱 안에서 재생합니다.
    pub(crate) fn stream_media_item(&mut self, item: MediaObject) {
        if self.player.is_some() || self.streaming_dl.is_some() {
            self.show_error("A recording is already playing.".into());
            return;
        }
        let dir = std::env::temp_dir().join("freedf-stream");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("stream-{}-{}.wav", std::process::id(), now_ms()));
        let state_path = path.clone();
        let url = item.url.clone();
        let name = item.name.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let res = ureq::get(&url)
                .call()
                .map_err(|e| format!("Stream failed: {e}"))
                .and_then(|resp| {
                    let mut reader = resp.into_reader();
                    let mut bytes = Vec::new();
                    std::io::Read::read_to_end(&mut reader, &mut bytes)
                        .map_err(|e| format!("Stream failed: {e}"))?;
                    std::fs::write(&path, &bytes).map_err(|e| e.to_string())
                });
            let _ = tx.send(res);
        });
        self.streaming_dl = Some(crate::player::StreamDownload {
            name,
            path: state_path,
            rx,
        });
        self.media_status = Some("Buffering…".into());
    }

    /// 스트리밍 다운로드 완료/재생 종료 폴링 (매 프레임).
    pub(crate) fn poll_player(&mut self) {
        if let Some(dl) = self.streaming_dl.take() {
            match dl.rx.try_recv() {
                Ok(Ok(())) => match crate::player::open_player(&dl.path, &dl.name) {
                    Ok(p) => {
                        self.media_status = None;
                        self.player = Some(p);
                    }
                    Err(e) => {
                        let _ = std::fs::remove_file(&dl.path);
                        self.show_error(e);
                    }
                },
                Ok(Err(e)) => {
                    let _ = std::fs::remove_file(&dl.path);
                    self.show_error(e);
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => self.streaming_dl = Some(dl),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {}
            }
        }
        if let Some(p) = &self.player {
            if p.is_finished() {
                let p = self.player.take().expect("player");
                let path = p.finish();
                let _ = std::fs::remove_file(&path);
                self.media_status = Some("Playback finished.".into());
            }
        }
    }

    /// 재생 중단 (Stop 버튼).
    pub(crate) fn stop_player(&mut self) {
        if let Some(p) = self.player.take() {
            let path = p.finish();
            let _ = std::fs::remove_file(&path);
        }
    }

    /// 녹음 다운로드 경로 선택 — Windows는 네이티브 대화상자, 그 외엔 입력 모달.
    pub(crate) fn download_media_dialog(&mut self, item: MediaObject) {
        #[cfg(target_os = "windows")]
        {
            if let Some(path) = rfd::FileDialog::new()
                .set_file_name(&item.name)
                .add_filter("Audio", &["wav", "m4a", "mp3", "ogg", "webm", "aac"])
                .save_file()
            {
                self.download_media_action(item.url, item.name, path);
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            self.modal = Some(ModalState::ask_text_prefilled(
                "Download recording",
                "Enter the save path (e.g. /home/me/Downloads/rec.wav)",
                TextAction::DownloadMedia {
                    url: item.url,
                    name: item.name.clone(),
                },
                item.name,
            ));
        }
    }

    /// 녹음 파일을 로컬에 저장 (비동기 — UI를 막지 않음).
    pub(crate) fn download_media_action(&mut self, url: String, name: String, path: PathBuf) {
        let (tx, rx) = std::sync::mpsc::channel();
        self.pdf_dl_rx = Some(rx);
        std::thread::spawn(move || {
            let res = ureq::get(&url)
                .call()
                .map_err(|e| format!("Download failed for {name}: {e}"))
                .and_then(|resp| {
                    let mut reader = resp.into_reader();
                    let mut bytes = Vec::new();
                    std::io::Read::read_to_end(&mut reader, &mut bytes)
                        .map_err(|e| e.to_string())?;
                    std::fs::write(&path, &bytes)
                        .map_err(|e| format!("Could not write {}: {e}", path.display()))
                })
                .map(|_| format!("Saved recording to {}", path.display()));
            let _ = tx.send(res);
        });
    }
}

impl FreeDfApp {
    /// 마이크 녹음 시작 — WAV 임시 파일로 기록하다가 정지 시 업로드됩니다.
    pub(crate) fn start_recording_action(&mut self) {
        if self.recording.is_some() {
            return;
        }
        let Some(doc_id) = self.doc_id else {
            self.show_error("Open a document to record into.".into());
            return;
        };
        let dir = std::env::temp_dir().join("freedf-recordings");
        let _ = std::fs::create_dir_all(&dir);
        let name = format!("rec-{doc_id}-{}", now_ms());
        match crate::recording::start_recording(&dir, &name) {
            Ok(rec) => {
                self.media_status = Some("Recording…".into());
                self.recording = Some(rec);
            }
            Err(e) => self.show_error(format!("Could not start recording: {e}")),
        }
    }

    /// 녹음 정지 + 업로드 (패널의 Stop 버튼).
    pub(crate) fn stop_recording_action(&mut self) {
        let Some(rec) = self.recording.take() else {
            return;
        };
        let path = rec.stop();
        self.upload_media_path(&path);
    }
}