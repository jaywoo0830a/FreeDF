//! 인앱 오디오 재생 — 서버에서 스트리밍 다운로드 후 앱 안에서 재생 (rodio).
//!
//! 다운로드는 백그라운드 스레드(ureq), 재생은 완료된 WAV를 디코더로 열어
//! 시작합니다. Windows(WASAPI)/macOS(CoreAudio)만 지원 — 그 외 플랫폼은
//! (ALSA dev 부재 등) 재생 스텁입니다.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::Duration;

/// 재생 중 상태.
pub(crate) struct PlayerState {
    #[cfg_attr(not(any(target_os = "windows", target_os = "macos")), allow(dead_code))]
    name: String,
    total: Option<Duration>,
    temp_path: PathBuf,
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    inner: Inner,
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
struct Inner {
    sink: rodio::Sink,
    _stream: rodio::OutputStream,
    /// 일시정지 시점까지의 누적 재생 위치.
    pos: std::sync::Mutex<Duration>,
    /// 재생 중일 때의 시작 시각 (None = 일시정지/정지).
    since: std::sync::Mutex<Option<std::time::Instant>>,
}

impl PlayerState {
    #[cfg_attr(not(any(target_os = "windows", target_os = "macos")), allow(dead_code))]
    pub(crate) fn name(&self) -> &str {
        &self.name
    }
    pub(crate) fn total(&self) -> Option<Duration> {
        self.total
    }
    pub(crate) fn elapsed(&self) -> Duration {
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        {
            // rodio 0.18의 Sink에는 get_pos가 없음 — 수동 누적 추적.
            let pos = *self.inner.pos.lock().unwrap();
            match *self.inner.since.lock().unwrap() {
                Some(since) => pos + since.elapsed(),
                None => pos,
            }
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            Duration::ZERO
        }
    }
    pub(crate) fn is_paused(&self) -> bool {
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        {
            self.inner.sink.is_paused()
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            false
        }
    }
    pub(crate) fn toggle(&self) {
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        {
            if self.inner.sink.is_paused() {
                self.inner.sink.play();
                *self.inner.since.lock().unwrap() = Some(std::time::Instant::now());
            } else {
                self.inner.sink.pause();
                let d = self
                    .inner
                    .since
                    .lock()
                    .unwrap()
                    .take()
                    .map(|i| i.elapsed())
                    .unwrap_or_default();
                *self.inner.pos.lock().unwrap() += d;
            }
        }
    }
    pub(crate) fn seek(&self, #[allow(unused_variables)] pos: Duration) {
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        {
            let _ = self.inner.sink.try_seek(pos);
            *self.inner.pos.lock().unwrap() = pos;
            if !self.inner.sink.is_paused() {
                *self.inner.since.lock().unwrap() = Some(std::time::Instant::now());
            }
        }
    }
    pub(crate) fn is_finished(&self) -> bool {
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        {
            self.inner.sink.empty()
        }
        #[cfg(not(any(target_os = "windows", target_os = "macos")))]
        {
            false
        }
    }
    /// 재생 중단 — 임시 파일 경로 반환 (호출자가 정리).
    pub(crate) fn finish(self) -> PathBuf {
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        {
            self.inner.sink.stop();
            drop(self.inner.sink);
            drop(self.inner._stream);
        }
        self.temp_path
    }
}

/// 스트리밍 다운로드 진행 상태.
pub(crate) struct StreamDownload {
    pub(crate) name: String,
    pub(crate) path: PathBuf,
    pub(crate) rx: Receiver<Result<(), String>>,
}

/// 다운로드가 끝난 WAV를 디코더로 열어 재생 시작.
pub(crate) fn open_player(path: &Path, name: &str) -> Result<PlayerState, String> {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        use rodio::Source; // total_duration 트레이트 메서드.
        let (stream, handle) = rodio::OutputStream::try_default()
            .map_err(|e| format!("No audio output device: {e}"))?;
        let sink = rodio::Sink::try_new(&handle).map_err(|e| e.to_string())?;
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let decoder = rodio::Decoder::new(std::io::BufReader::new(file))
            .map_err(|e| format!("Could not decode WAV: {e}"))?;
        let total = decoder.total_duration();
        sink.append(decoder);
        sink.play();
        Ok(PlayerState {
            name: name.to_string(),
            total,
            temp_path: path.to_path_buf(),
            inner: Inner {
                sink,
                _stream: stream,
                pos: std::sync::Mutex::new(Duration::ZERO),
                since: std::sync::Mutex::new(Some(std::time::Instant::now())),
            },
        })
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = (path, name);
        Err("Audio playback is not supported on this platform yet.".into())
    }
}
