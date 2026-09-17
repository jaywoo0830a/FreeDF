//! 마이크 녹음 — cpal(캡처) + hound(WAV 저장).
//!
//! 오디오 콜백은 실시간 스레드에서 도므로, 파일 쓰기는 별도 스레드가
//! mpsc 채널로 받아 hound에 기록합니다. stop()이 스트림을 내리고
//! 채널을 닫으면 작성 스레드가 WAV를 확정(finalize)하고 조인합니다.

use std::path::{Path, PathBuf};
#[cfg(any(target_os = "windows", target_os = "macos"))]
use std::sync::mpsc::{Receiver, Sender};

/// 진행 중인 녹음 핸들 — `stop()`으로 WAV 파일을 확정합니다.
pub(crate) struct Recorder {
    started_ms: u64,
    path: PathBuf,
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    inner: Option<Inner>,
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
struct Inner {
    stream: cpal::Stream,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    tx: Option<Sender<Vec<f32>>>,
    writer: Option<std::thread::JoinHandle<()>>,
}

impl Recorder {
    pub(crate) fn started_ms(&self) -> u64 {
        self.started_ms
    }

    /// 녹음 중지 — WAV 확정 후 경로 반환.
    pub(crate) fn stop(#[allow(unused_mut)] mut self) -> PathBuf {
        #[cfg(any(target_os = "windows", target_os = "macos"))]
        if let Some(inner) = self.inner.take() {
            inner.stop.store(true, std::sync::atomic::Ordering::Relaxed);
            drop(inner.stream); // 콜백과 그 tx 클론 제거.
            drop(inner.tx); // 채널 닫기 → 작성 스레드 종료.
            if let Some(join) = inner.writer {
                let _ = join.join(); // finalize까지 대기.
            }
        }
        self.path
    }
}

/// 녹음 시작 — `dir`에 `{name}.wav`로 저장합니다.
/// (Windows WASAPI / macOS CoreAudio — 그 외 플랫폼은 미지원)
pub(crate) fn start_recording(dir: &Path, name: &str) -> Result<Recorder, String> {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        start_recording_impl(dir, name).map(|(started_ms, path, inner)| Recorder {
            started_ms,
            path,
            inner: Some(inner),
        })
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = (dir, name);
        Err("Microphone recording is not supported on this platform yet.".into())
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn start_recording_impl(dir: &Path, name: &str) -> Result<(u64, PathBuf, Inner), String> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or_else(|| "No microphone found.".to_string())?;
    let supported = device
        .default_input_config()
        .map_err(|e| format!("Microphone config failed: {e}"))?;
    let channels = supported.channels() as u16;
    let sample_rate = supported.sample_rate().0;
    let stream_config: cpal::StreamConfig = supported.clone().into();

    let path = dir.join(format!("{name}.wav"));
    let spec = hound::WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let writer = hound::WavWriter::create(&path, spec).map_err(|e| e.to_string())?;
    let (tx, rx): (Sender<Vec<f32>>, Receiver<Vec<f32>>) = std::sync::mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));

    // 파일 쓰기 스레드 — 콜백(실시간)에서 직접 디스크 I/O를 하지 않습니다.
    let writer_thread = std::thread::spawn(move || {
        let mut w = writer;
        for chunk in rx.iter() {
            for s in chunk {
                let i = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
                if w.write_sample(i).is_err() {
                    break;
                }
            }
        }
        let _ = w.finalize();
    });

    let err_fn = |err| eprintln!("mic stream error: {err}");
    let stop_cb = Arc::clone(&stop);
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => {
            let tx = tx.clone();
            device
                .build_input_stream(
                    &stream_config,
                    move |data: &[f32], _| {
                        if stop_cb.load(Ordering::Relaxed) {
                            return;
                        }
                        let _ = tx.send(data.to_vec());
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| e.to_string())?
        }
        cpal::SampleFormat::I16 => {
            let tx = tx.clone();
            device
                .build_input_stream(
                    &stream_config,
                    move |data: &[i16], _| {
                        if stop_cb.load(Ordering::Relaxed) {
                            return;
                        }
                        let chunk: Vec<f32> = data.iter().map(|s| *s as f32 / 32768.0).collect();
                        let _ = tx.send(chunk);
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| e.to_string())?
        }
        cpal::SampleFormat::U16 => {
            let tx = tx.clone();
            device
                .build_input_stream(
                    &stream_config,
                    move |data: &[u16], _| {
                        if stop_cb.load(Ordering::Relaxed) {
                            return;
                        }
                        let chunk: Vec<f32> =
                            data.iter().map(|s| (*s as f32 - 32768.0) / 32768.0).collect();
                        let _ = tx.send(chunk);
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| e.to_string())?
        }
        cpal::SampleFormat::I32 => {
            let tx = tx.clone();
            device
                .build_input_stream(
                    &stream_config,
                    move |data: &[i32], _| {
                        if stop_cb.load(Ordering::Relaxed) {
                            return;
                        }
                        let chunk: Vec<f32> =
                            data.iter().map(|s| *s as f32 / 2_147_483_648.0).collect();
                        let _ = tx.send(chunk);
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| e.to_string())?
        }
        cpal::SampleFormat::U8 => {
            let tx = tx.clone();
            device
                .build_input_stream(
                    &stream_config,
                    move |data: &[u8], _| {
                        if stop_cb.load(Ordering::Relaxed) {
                            return;
                        }
                        let chunk: Vec<f32> =
                            data.iter().map(|s| (*s as f32 - 128.0) / 128.0).collect();
                        let _ = tx.send(chunk);
                    },
                    err_fn,
                    None,
                )
                .map_err(|e| e.to_string())?
        }
        other => {
            return Err(format!("Unsupported microphone sample format: {other:?}"));
        }
    };
    stream.play().map_err(|e| e.to_string())?;

    let started_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    Ok((
        started_ms,
        path,
        Inner {
            stream,
            stop,
            tx: Some(tx),
            writer: Some(writer_thread),
        },
    ))
}
