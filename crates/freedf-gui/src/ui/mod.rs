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
//! - [`icons`] — Heroicons(iconflow) 글리프 + 라벨/슬러그 규칙 (마크업·테스트·계약 id의 공통 출처)

pub mod atoms;
pub mod icons;
pub mod layout;

pub use atoms::*;
pub use icons::{icon, label, slug};
pub use layout::*;
