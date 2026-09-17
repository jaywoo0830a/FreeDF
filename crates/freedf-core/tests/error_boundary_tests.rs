//! 에러 바운더리 통합 테스트.
//!
//! `src/error.rs` 모듈 문서의 3계층 정책 계약을 검증합니다:
//! 1. **모듈 → 크레이트**: `NoteError`, `MalformedStream`이 `#[from]`으로
//!    [`Error`](freedf_core::Error)로 수렴하는가
//! 2. **이질적 원인**: io / JSON도 같은 경계로 수렴하는가
//! 3. **크레이트 → 앱(anyhow)**: `?` 한 번으로 흘러가고, `downcast_ref`로
//!    구체적 변형을 복원할 수 있는가 (정보 손실 없음)

use freedf_core::notes::{NoteError, NotesManager};
use freedf_core::store::AnnotationStore;
use freedf_core::{Error, Result as CoreResult};

/// 노트 생성 흐름 — 도메인 에러가 `?`로 크레이트 경계까지 자동 변환되는 전형.
fn create_duplicate_note() -> CoreResult<()> {
    let mut m = NotesManager::new();
    m.create_note("Alpha")?;
    m.create_note("alpha")?; // NoteError::DuplicateTitle → Error::Note
    Ok(())
}

#[test]
fn module_errors_converge_into_crate_error() {
    let err = create_duplicate_note().unwrap_err();
    assert!(matches!(err, Error::Note(NoteError::DuplicateTitle)));
    // thiserror가 만든 표시 메시지가 보존되는지.
    assert!(err.to_string().contains("already exists"));
}

#[test]
fn crate_error_flows_into_anyhow_boundary_without_loss() {
    // 앱 크레이트가 쓰는 전형: `?` 한 번으로 anyhow::Error 변환.
    let err: anyhow::Error = create_duplicate_note().unwrap_err().into();
    // 최상위는 크레이트 경계 타입(freedf_core::Error).
    let outer = err.downcast_ref::<Error>().expect("Error 복원");
    assert!(matches!(outer, Error::Note(NoteError::DuplicateTitle)));
    // 원인 체인(source)을 따라가면 도메인 변형도 복원할 수 있다.
    let note = err
        .chain()
        .filter_map(|e| e.downcast_ref::<NoteError>())
        .next()
        .expect("NoteError 복원");
    assert!(matches!(note, NoteError::DuplicateTitle));
    // 원인 체인에 컨텍스트를 붙여도 표시 메시지는 유지된다.
    let wrapped = err.context("노트 생성 실패");
    assert!(format!("{wrapped:#}").contains("already exists"));
}

#[test]
fn foreign_error_sources_share_one_boundary() {
    // IO — 상위 컴포넌트가 파일인 경로 → Error::Io
    let base = std::env::temp_dir().join(format!("freedf-eb-file-{}", std::process::id()));
    std::fs::write(&base, b"x").unwrap();
    let mut inside_file = base.clone();
    inside_file.push("log.jsonl");
    let io_err = freedf_core::logging::Logger::to_file(&inside_file)
        .err()
        .expect("파일 안에 디렉터리 생성은 실패해야 함");
    let _ = std::fs::remove_file(&base);

    // JSON — 깨진 입력 → Error::Json
    let json_err = AnnotationStore::from_json("not json")
        .err()
        .expect("깨진 JSON은 실패");

    // 커맨드 스트림 불변식 위반 → MalformedStream → Error::MalformedStream
    use freedf_core::input_commands::Command;
    let stream_err = Error::from(
        freedf_core::input_commands::check_well_formed(&[Command::ExtendStroke {
            point: [0.0, 0.0],
            pressure: 0.5,
        }])
        .expect_err("세션 밖 extend-stroke는 위반"),
    );

    assert!(matches!(io_err, Error::Io(_)));
    assert!(matches!(json_err, Error::Json(_)));
    assert!(matches!(stream_err, Error::MalformedStream(_)));

    // 셋 모두 앱(anyhow) 경계로 흘러가고, 복원 가능.
    for e in [io_err, json_err, stream_err] {
        let a: anyhow::Error = e.into();
        assert!(a.downcast_ref::<Error>().is_some(), "변형 복원 가능해야 함");
    }
}

#[test]
fn malformed_stream_keeps_first_violation_message() {
    use freedf_core::input_commands::{check_well_formed, Command};
    let err = check_well_formed(&[
        Command::BeginStroke {
            tool: "pen".into(),
            point: [0.0, 0.0],
            pressure: 0.5,
        },
        Command::BeginStroke {
            tool: "pen".into(),
            point: [1.0, 1.0],
            pressure: 0.5,
        },
    ])
    .expect_err("세션 중복 begin은 위반");
    let anyhow_err: anyhow::Error = err.into();
    let m = anyhow_err
        .downcast_ref::<freedf_core::input_commands::MalformedStream>()
        .expect("MalformedStream 복원");
    assert!(m.0.contains("중복 begin"), "첫 위반 설명 보존: {}", m.0);
}
