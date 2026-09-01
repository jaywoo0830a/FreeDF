//! 분석용 구조적 로그. JSON Lines 형태로 파일에 기록합니다.

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

/// 앱에서 기록할 이벤트.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum AppEvent {
    AppStart { version: String },
    NoteOpened { note_id: u64, title: String, page_count: usize },
    NoteCreated { note_id: u64, title: String },
    NoteRenamed { note_id: u64, from: String, to: String },
    NoteDeleted { note_id: u64, title: String },
    PageChanged { page: usize, total: usize },
    PageAdded { page: usize, total: usize },
    PageDeleted { page: usize, total: usize },
    StrokeAdded { page: usize, points: usize, tool: String, width: f32 },
    StrokeErased { page: usize, strokes: usize },
    UndoRedo { kind: String },
    Search { query: String, results: usize },
    OutlineJump { title: String, page: usize },
    ExportPng { page: usize },
    Error { message: String },
}

/// 로그 한 줄. 시각과 순번을 붙여 기록합니다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    /// epoch 밀리초
    pub epoch_ms: u128,
    /// 순번 (1부터 증가)
    pub seq: u64,
    #[serde(flatten)]
    pub event: AppEvent,
}

/// 구조적 로거. 한 줄 = 하나의 JSON. 비활성화 가능.
pub struct Logger {
    writer: Option<BufWriter<File>>,
    seq: u64,
}

impl Logger {
    /// 파일(append)에 기록하는 로거 생성.
    pub fn to_file(path: &Path) -> std::io::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = File::options().create(true).append(true).open(path)?;
        Ok(Self {
            writer: Some(BufWriter::new(file)),
            seq: 0,
        })
    }

    /// 기록하지 않는 로거 (예: 파일 열기 실패 시).
    pub fn disabled() -> Self {
        Self { writer: None, seq: 0 }
    }

    pub fn enabled(&self) -> bool {
        self.writer.is_some()
    }

    pub fn seq(&self) -> u64 {
        self.seq
    }

    /// 이벤트를 한 줄로 기록합니다.
    pub fn log(&mut self, event: AppEvent) {
        let Some(writer) = self.writer.as_mut() else {
            return;
        };
        self.seq += 1;
        let entry = LogEntry {
            epoch_ms: now_ms(),
            seq: self.seq,
            event,
        };
        if let Ok(line) = serde_json::to_string(&entry) {
            let _ = writeln!(writer, "{line}");
        }
    }

    pub fn flush(&mut self) {
        if let Some(writer) = self.writer.as_mut() {
            let _ = writer.flush();
        }
    }
}

impl Drop for Logger {
    fn drop(&mut self) {
        self.flush();
    }
}

pub fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_log() -> std::path::PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        std::env::temp_dir().join(format!("freedf-log-{}-{n}.log", std::process::id()))
    }

    #[test]
    fn writes_json_lines_in_order() {
        let path = temp_log();
        {
            let mut logger = Logger::to_file(&path).unwrap();
            logger.log(AppEvent::AppStart { version: "0.1.0".into() });
            logger.log(AppEvent::StrokeAdded { page: 0, points: 12, tool: "Pen".into(), width: 2.0 });
            logger.log(AppEvent::Search { query: "word".into(), results: 3 });
            logger.flush();
        }
        let text = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3);

        let e1: LogEntry = serde_json::from_str(lines[0]).unwrap();
        let e2: LogEntry = serde_json::from_str(lines[1]).unwrap();
        let e3: LogEntry = serde_json::from_str(lines[2]).unwrap();

        assert_eq!(e1.seq, 1);
        assert_eq!(e2.seq, 2);
        assert_eq!(e3.seq, 3);
        assert!(e1.epoch_ms > 0);
        assert!(e1.event == AppEvent::AppStart { version: "0.1.0".into() });
        assert!(matches!(
            e2.event,
            AppEvent::StrokeAdded { page: 0, points: 12, tool: ref t, width: 2.0 } if t == "Pen"
        ));
        assert!(matches!(e3.event, AppEvent::Search { results: 3, .. }));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn appends_to_existing_file() {
        let path = temp_log();
        {
            let mut l = Logger::to_file(&path).unwrap();
            l.log(AppEvent::AppStart { version: "1".into() });
        }
        {
            let mut l = Logger::to_file(&path).unwrap();
            l.log(AppEvent::Error { message: "boom".into() });
        }
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 2);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn disabled_logger_writes_nothing() {
        let mut logger = Logger::disabled();
        assert!(!logger.enabled());
        logger.log(AppEvent::AppStart { version: "x".into() });
        assert_eq!(logger.seq(), 0);
    }
}
