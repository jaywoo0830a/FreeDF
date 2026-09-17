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
//! - [`input_events`]: 통합 입력 이벤트 어휘 (장치 축과 툴 축의 유일한 공유물)
//! - [`input_hub`]: 이벤트 허브 — 포인터 충돌 규칙(한 번에 한 포인터) 소유
//! - [`input_devices`]: 장치 어댑터 — raw 하드웨어 → 통합 어휘 번역 (펜 스트림)
//! - [`pen`]: 색상 팔레트(빨강/파랑/검정 계열) + 필압→두께 곡선
//! - [`ink`]: 잉크 질감(입체적 불균일) 모델 — 결정적 노이즈 + 도구별 잉크 물리
//! - [`paper`]: 용지 스타일(그리드/줄/점선) + 배경 색
//! - [`logging`]: 분석용 구조적 로그(JSON Lines)
//! - [`error`]: 에러 바운더리 — 크레이트 공통 [`error::Error`]/[`error::Result`]

pub mod dictionary;
pub mod error;
pub mod history;
pub mod ink;
pub mod input_commands;
pub mod input_controlmap;
pub mod input_devices;
pub mod input_events;
pub mod input_hub;
pub mod input_tools;
pub mod input_workspace;
pub mod logging;
pub mod model;
pub mod notes;
pub mod outline;
pub mod pages;
pub mod paper;
pub mod pen;
pub mod pen_input;
pub mod pipeline;
pub mod search;
pub mod store;
pub mod text;
pub mod transform;

pub use error::{Error, Result};
