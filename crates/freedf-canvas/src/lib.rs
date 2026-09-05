//! # freedf-canvas — 잉크 캔버스 엔진 (인터페이스 골격)
//!
//! FreeDF의 캔버스(페이지 렌더 + 잉크 메시)를 담당할 **순수 Rust** 크레이트.
//! 지금은 아이디에이션 단계로, 인터페이스와 계약 테스트만 정의합니다.
//!
//! ## 설계 원칙 (모든 코드가 지켜야 할 계약)
//!
//! 1. **순수/결정적** — 기하·잉크 계산은 전부 순수 함수.
//!    시간은 `now_ms` 인자로, 질감은 명시적 시드([`ink::GrainSeed`])로 주입.
//!    전역 상태 없음 → 같은 입력은 항상 같은 출력.
//! 2. **UI 스레드 무블록** — [`bake::BakeService`]의 공개 메서드는 전부
//!    `try_send`/`try_recv` 기반. 블로킹 `recv`는 워커 스레드 내부에만
//!    존재합니다 (UI 스레드가 호출할 수 있는 경로에 `recv`/`join` 금지).
//! 3. **컴포저블** — 각 계층이 트레이트로 교체 가능하고, 작은 부품을
//!    조합해 파이프라인을 만듭니다:
//!
//! ```text
//! SceneStore(rev diff) ─▶ Mesher(Width×Alpha×Grain) ─▶ BakeWorker
//!      ─▶ BakeService(스레드) ─▶ FrameAssembler ─▶ Surface(GPU/래스터)
//! ```
//!
//! 4. **테스트 가능** — [`clock::FakeClock`], [`surface::RecordingSurface`],
//!    즉시/수동 완료 워커로 타이밍·IO 의존을 제거합니다. 계약 테스트는
//!    각 모듈의 `#[cfg(test)]`에 TDD 골격으로 있습니다.

pub mod bake;
pub mod clock;
pub mod geom;
pub mod ink;
pub mod scene;
pub mod surface;

pub use bake::{BakeError, BakeParams, BakeWorker, BakedPage, BakeService, SimpleWorker};
pub use clock::{Clock, FakeClock, SystemClock};
pub use geom::{PagePoint, PageSize, ViewTransform};
pub use ink::{
    bake_strokes, AlphaModel, BallWidth, GrainSeed, Mesh, Mesher, RibbonMesher, SoakAlpha,
    StrokeCtx, WidthModel,
};
pub use scene::{Changes, LayerKind, Revision, SceneSnapshot, SceneStore, Stroke, StrokeId, StrokePoint};
pub use surface::{DrawCommand, FrameAssembler, RecordingSurface, Surface, Transform};
