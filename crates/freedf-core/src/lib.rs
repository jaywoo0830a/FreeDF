//! FreeDF 핵심 로직.
//!
//! GUI(egui/pdfium)에 의존하지 않는 순수 Rust 모듈만 모아 두어
//! 창 없이도 단위 테스트로 검증할 수 있게 구성했습니다.
//!
//! - [`model`]: 스트로크(획)와 도구 모델
//! - [`store`]: 페이지별 주석 저장소 + 지우개 히트 테스트 + JSON 직렬화
//! - [`transform`]: 페이지 좌표 ↔ 뷰(캔버스) 좌표 변환
//! - [`history`]: 실행취소/다시실행(diff 기반) 이력
//! - [`notes`]: 노트 CRUD(라이브러리 인덱스)
//! - [`pages`]: 페이지 삽입/삭제 시 주석 인덱스 정리
//! - [`outline`]: PDF 아웃라인(북마크) 트리 모델
//! - [`search`]: 페이지 내 단어 검색 + 하이라이트 사각형
//! - [`pen`]: 색상 팔레트(빨강/파랑/검정 계열) + 필압→두께 곡선
//! - [`paper`]: 용지 스타일(그리드/줄/점선) + 배경 색
//! - [`logging`]: 분석용 구조적 로그(JSON Lines)

pub mod dictionary;
pub mod history;
pub mod logging;
pub mod model;
pub mod notes;
pub mod outline;
pub mod pages;
pub mod paper;
pub mod pen;
pub mod search;
pub mod store;
pub mod text;
pub mod transform;
