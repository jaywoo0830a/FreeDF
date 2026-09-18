//! FreeDF GUI — elm-magic으로 처음부터 다시 만드는 UI 셸 (라이브러리).
//!
//! 바이너리(`src/main.rs`)는 **eframe 호스트만** 담당하고, 화면·엔진·스타일은
//! 이 라이브러리가 소유한다. 테스트는 `tests/`에서 이 공개 API로 검증한다
//! (크레이트 관례: 소스는 `src/`, 테스트는 `tests/`).
//!
//! ## 모듈
//!
//! - [`canvas`] — `<Raw>` 경계 뒤의 명령형 잉크 캔버스 엔진 (문서/탭, 입력,
//!   `freedf-core` 파이프라인 + `freedf-canvas` 메셔 배선).
//! - [`pen`] — 펜 입력(필압/틸트) 공급원 배선 (OTD 데몬 RPC → evdev → 없음).
//! - [`cursor`] — 도구별 커서 스프라이트 (freedf `paint_custom_cursor` 이식).
//! - [`palette`] — 즐겨찾기 색 목록/이름 해석 (`freedf-services::settings` 출처).
//! - [`ui`] — 재사용 컴포넌트(atoms/layout) + 아이콘/라벨 규칙 (하이브리드 구조).
//! - [`shell`] — `elm_magic::view!` 위젯 트리 (툴바/리본/패널/탭/모달).
//! - [`style`] — elm-magic CSS 규칙 + 팔레트 토큰 (유일한 스타일 출처).
//! - [`fonts`] — 임베드 폰트 설치.
//! - [`dev`] — eguidev 계측 헬퍼 (`dev-automation` 기능에서만 동작).
//!
//! ## elm-magic 계약
//!
//! 컴포넌트는 `elm_magic::view!` 한 블록에 모아 정의하고(0.7.2부터 여러 `fn`),
//! 문서 주석은 **블록 안**에 쓴다 — 호출부의 `///`는 rustc가 매크로 호출
//! 속성으로 두어 `unused doc comment` 경고만 난다. 그래서 경고를 끌 필요가 없다.
//! 남은 미수정 버그(값 prop의 재렌더 미반영)와 `key` 우회는 [`ui::atoms`] 참고.

pub mod canvas;
pub mod cursor;
pub mod dev;
pub mod fonts;
pub mod palette;
pub mod pen;
pub mod shell;
pub mod style;
pub mod ui;
