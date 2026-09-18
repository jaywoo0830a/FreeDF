//! `logging` 모듈 단위 테스트 — `src/logging.rs`의 `#[cfg(test)]` 모듈에서 이동했습니다.

use freedf_core::logging::*;
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
        logger.log(AppEvent::AppStart {
            version: "0.1.0".into(),
        });
        logger.log(AppEvent::StrokeAdded {
            page: 0,
            points: 12,
            tool: "Pen".into(),
            width: 2.0,
        });
        logger.log(AppEvent::Search {
            query: "word".into(),
            results: 3,
        });
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
    assert!(
        e1.event
            == AppEvent::AppStart {
                version: "0.1.0".into()
            }
    );
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
        l.log(AppEvent::AppStart {
            version: "1".into(),
        });
    }
    {
        let mut l = Logger::to_file(&path).unwrap();
        l.log(AppEvent::Error {
            message: "boom".into(),
        });
    }
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(text.lines().count(), 2);
    let _ = std::fs::remove_file(path);
}

#[test]
fn disabled_logger_writes_nothing() {
    let mut logger = Logger::disabled();
    assert!(!logger.enabled());
    logger.log(AppEvent::AppStart {
        version: "x".into(),
    });
    assert_eq!(logger.seq(), 0);
}
