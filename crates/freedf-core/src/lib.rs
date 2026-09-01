//! FreeDF 핵심 로직.
//!
//! GUI(egui/pdfium)에 의존하지 않는 순수 Rust 모듈만 모아 두어
//! 창 없이도 단위 테스트로 검증할 수 있게 구성했습니다.
//!
//! - [`model`]: 스트로크(획)와 도구 모델
//! - [`store`]: 페이지별 주석 저장소 + 지우개 히트 테스트 + JSON 직렬화
//! - [`transform`]: 페이지 좌표 ↔ 뷰(캔버스) 좌표 변환
//! - [`history`]: 실행취소/다시실행(diff 기반) 이력

pub mod history;
pub mod model;
pub mod store;
pub mod transform;
