//! 문서/노트/페이지/줌/미디어 액션 — 사용자 명령의 실행 로직.
//!
//! 하위 모듈 구성:
//! - [`bookmarks`]: 북마크 토글/정리
//! - [`notes`]: 노트/PDF 생성·이름변경·삭제
//! - [`paper`]: 용지 일괄 적용
//! - [`documents`]: 문서 열기/닫기/저장
//! - [`pages`]: 페이지 이동·삽입·회전·삭제
//! - [`view`]: 줌/핏/정렬
//! - [`history`]: undo/redo/페이지 지우기
//! - [`search`]: 검색/아웃라인
//! - [`save`]: 플러시(구조 델타)/수동 저장/로드
//! - [`media`]: 녹음 업로드·재생·삭제
//! - [`dialogs`]: 폴백 다이얼로그 액션 실행

pub(crate) use super::*;

/// 문서 열기의 DB 부분 — **백그라운드 스레드 전용** (UI를 절대 막지 않음).
/// 단계마다 `LoaderMsg::Stage`를 보내 진행 바에 무엇을 가져오는지 표시합니다.
fn load_document_bundle(
    db: &dyn StorageBackend,
    doc_id: i64,
    tx: &std::sync::mpsc::Sender<LoaderMsg>,
) -> Result<LoaderBundle, String> {
    let _ = tx.send(LoaderMsg::Stage("Loading: document info…".into()));
    let row = db
        .get_document(doc_id)
        .ok_or_else(|| format!("Document {doc_id} not found in the database."))?;
    let is_note = row.is_note();
    let _ = tx.send(LoaderMsg::Stage("Loading: PDF bytes…".into()));
    let pdf_bytes = db.load_pdf(doc_id).ok_or_else(|| {
        format!("{} has no PDF content in the database.", row.title)
    })?;
    // 주석(획 전체)·페이지·편집 저널·세션을 한 번의 왕복으로 로드
    // (migration 0007 — 서버가 JSONB로 집계, 클라이언트는 단일 패스 파싱).
    let _ = tx.send(LoaderMsg::Stage(format!(
        "Loading: annotations, history & session…"
    )));
    let bundle = db.load_bundle(doc_id);
    Ok(LoaderBundle {
        doc_id,
        is_note,
        row,
        pdf_bytes,
        store: bundle.store,
        edits: bundle.edits,
        session: bundle.session,
    })
}

/// 구조 연산 델타 — 서버(SQL 함수)에서 처리해 전체 스트로크 재전송을 피합니다.
///
/// 델타 프로토콜: 대기열 플러시(획 증분 반영) → 서버 구조 델타 → 메타 동기화.
pub(crate) enum StructureOp {
    /// 페이지 중간 삽입 — from 이상 획 인덱스 +1 (서버 이동, 재전송 없음).
    Shift { from: i32, delta: i32 },
    /// 페이지 삭제 — 해당 페이지 획 삭제 + 이후 인덱스 -1.
    DeletePage { page: i32 },
    /// 페이지 회전 — 해당 페이지 획 좌표를 서버에서 변환.
    RotatePage {
        page: i32,
        clockwise: bool,
        w: f32,
        h: f32,
    },
    /// 전체 페이지 회전.
    RotateAll {
        clockwise: bool,
        sizes: Vec<[f32; 2]>,
    },
}

impl FreeDfApp {
    /// Shows an error both in the status bar and as a popup alert.
    pub(crate) fn show_error(&mut self, msg: String) {
        self.status = Some(msg.clone());
        self.modal = Some(ModalState::alert("Error", &msg));
    }
    /// Returns a reference to the PDFium instance cached at startup.
    /// pdfium-render only allows one initialization per process, so everything
    /// must reuse this single instance (never call `load_pdfium` again).
    pub(crate) fn pdfium(&self) -> Result<&Pdfium, String> {
        self.pdfium.as_ref().map(|b| b.as_ref()).map_err(|e| e.clone())
    }

}

mod bookmarks;
mod dialogs;
mod documents;
mod history;
mod media;
mod notes;
mod pages;
mod paper;
mod save;
mod search;
mod view;
