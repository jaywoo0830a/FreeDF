//! UI 컴포넌트 — **하이브리드 구조**.
//!
//! - 태그는 **의미만** 갖는다(`Button` `Strong` `Text` `Tab` `Row` `Col` `Modal`).
//! - 스타일은 전부 CSS(`src/style.rs`)가 갖는다 — 마크업에는 BEM 클래스 이름만 남고,
//!   문맥 차이는 CSS의 하위 셀렉터(`.navbar .btn`)나 크기 속성(`height`)이 처리한다.
//! - 반복되는 조합(버튼/탭/행/영역)은 컴포넌트로 뽑아 셸 본문을 짧게 유지한다.
//!
//! ## 모듈
//!
//! - [`atoms`] — 버튼/제목/행/탭 같은 최소 단위
//! - [`layout`] — 영역(블록) 컨테이너 (`{children}` 통과)
//! - [`icons`] — Phosphor 글리프 + 라벨/슬러그 규칙 (마크업·테스트·계약 id의 공통 출처)

pub mod atoms;
pub mod icons;
pub mod layout;

pub use atoms::*;
pub use icons::{icon, label, slug};
pub use layout::*;

/// elm-magic `key=` 문자열 — 값 prop 갱신용 (`ui::atoms`의 "버그 11" 참고).
///
/// 키는 **인스턴스마다 유일**해야 하고(같은 키가 둘이면 elm-magic이 슬롯 경로를
/// 공유해 마지막 값으로 덮는다 — 실측: 사이드바가 여러 개로 늘고 Fountain이 사라졌다),
/// 값이 바뀔 때마다 함께 바뀌어야 한다. 그래서 이름(라벨)과 상태를 합쳐 만든다.
///
/// 상태 슬롯은 `key={format!("{slot}")}`처럼 키 식 안에서 직접 읽을 수 없다
/// (매크로가 슬롯 읽기를 치환하지 못해 E0425). 이 헬퍼를 **본문 지역값**으로
/// 만들어 `key={지역값}`으로 넘기는 게 안전하다.
pub fn state_key(name: &str, state: impl std::fmt::Display) -> String {
    format!("{name}-{state}")
}
