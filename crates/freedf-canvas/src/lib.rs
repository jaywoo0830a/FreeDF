//! # freedf-canvas — 잉크 캔버스 엔진
//!
//! FreeDF의 캔버스(잉크 메시) 렌더링 파이프라인을 담당하는 **순수 계산** 크레이트.
//! IO·GPU·스레드는 전부 경계 밖(앱/워커)에 있고, 결정적 계산만 합니다.
//!
//! 물리 모델(폭 리본·잉크 스밈·질감)은 [`freedf_core`]의 검증된 구현을
//! 재사용하고([`core_mesh`]), 이 크레이트는 파이프라인(장면 diff·굽기·프레임
//! 조립)만 소유합니다. 아키텍처는 `doc/architecture.md` 참조.
//!
//! ## 설계 원칙 (모든 코드가 지켜야 할 계약)
//!
//! 1. **순수/결정적** — 기하·잉크 계산은 전부 순수 함수.
//!    시간은 `now_ms` 인자로, 질감은 명시적 시드로 주입. 전역 상태 없음.
//! 2. **UI 스레드 무블록** — [`bake::BakeService`]의 공개 메서드는 전부
//!    `try_send`/`try_recv` 기반. 블로킹 `recv`는 워커 스레드 내부에만
//!    존재합니다 (UI 스레드가 호출할 수 있는 경로에 `recv`/`join` 금지).
//! 3. **컴포저블** — 각 계층이 트레이트로 교체 가능하고, 작은 부품을
//!    조합해 파이프라인을 만듭니다:
//!
//! ```text
//! SceneStore(rev diff) ─▶ Mesher(Width×Alpha×Grain / CoreRibbon) ─▶ BakeWorker
//!      ─▶ BakeService(스레드) ─▶ FrameAssembler ─▶ Surface(GPU/래스터)
//! ```
//!
//! 4. **테스트 가능** — [`clock::FakeClock`], [`surface::RecordingSurface`],
//!    수동 완료 워커로 타이밍·IO 의존을 제거합니다.

pub mod bake;
pub mod clock;
pub mod core_mesh;
pub mod geom;
pub mod ink;
pub mod scene;
pub mod soak;
pub mod surface;

pub use bake::{BakeError, BakeParams, BakeService, BakeWorker, BakedPage, SimpleWorker};
pub use clock::{Clock, FakeClock, SystemClock};
pub use core_mesh::{
    alphas_for_stroke, append_stroke_ribbon, halves_for_stroke, CoreRibbonMesher,
};
pub use geom::{PagePoint, PageSize, ViewTransform};
pub use ink::{
    bake_strokes, AlphaModel, BallWidth, GrainSeed, Mesh, Mesher, RibbonMesher, SoakAlpha,
    StrokeCtx, WidthModel,
};
pub use scene::{
    Changes, LayerKind, Revision, SceneSnapshot, SceneStore, Stroke, StrokeId, StrokePoint,
};
pub use soak::{
    ink_pacing_for, snap_refresh_hz, InkPacing, InkSettling, REFRESH_PRESETS,
};
pub use surface::{DrawCommand, FrameAssembler, RecordingSurface, Surface, Transform};
