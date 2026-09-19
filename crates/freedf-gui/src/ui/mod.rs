//! UI 컴포넌트 — **하이브리드 구조**.
//!
//! - 태그는 **의미만** 갖는다(`Button` `Strong` `Text` `Tab` `Row` `Col` `Modal`).
//! - 스타일은 전부 CSS(`src/style.rs`)가 갖는다 — 마크업에는 BEM 클래스 이름만
//!   남고, 문맥 차이는 CSS 하위 셀렉터나 수정자(`--on`/`--sel`)가 처리한다.
//! - 반복되는 조합(버튼/탭/도구 줄/영역)은 컴포넌트로 뽑아 셸 본문을 짧게 유지한다.
//!
//! ## 모듈
//!
//! - [`atoms`] — 버튼/제목/행/탭 같은 최소 단위 (버튼 위계 표가 그 파일에 있다)
//! - [`layout`] — 영역(블록) 컨테이너 (`{children}` 통과)
//! - [`icons`] — Heroicons 글리프 + 라벨/슬러그 규칙 (마크업·테스트·계약 id의 공통 출처)
//!
//! ## 컴포넌트를 추가할 때
//!
//! 1. `atoms.rs`/`layout.rs`에 컴포넌트를 만들고 **클래스 리터럴만** 붙인다.
//! 2. 그 클래스 이름을 `style.rs`의 `css!`에 등록한다 — 마크업이 쓰는데 CSS에 없는
//!    클래스는 `tests/style_tests.rs`가 잡는다.
//! 3. 라벨은 계약이다 — 버튼 문구를 바꾸면 `gui.<슬러그>`가 바뀌고 스모크가 깨진다.

pub mod atoms;
pub mod icons;
pub mod layout;

pub use atoms::*;
pub use icons::{icon, label, slug};
pub use layout::*;
