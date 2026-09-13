# Changelog

형식: **[버전/일자]** 변경 요약. 항목은 되도록 **행동/계약 변경**과 **리팩터**를 구분합니다.
검증은 `cargo build`(0 경고) + `cargo test -p freedf-core` / `-p freedf-canvas` 기준입니다.

## [Unreleased] — 2026-09-13

### 필기 파이프라인 리팩터 (InkPipeline / WritingMaterial / Materials)
- **`LiveStroke` + `InkPipeline` 신설** (`crates/freedf-core/src/pipeline.rs`)
  - 진행 획을 한 객체로 묶어 호출 시퀀스를 `down()`+`drag()`+`up()`으로 압축.
  - `LiveStroke`는 append-only 점 버퍼 + frontier(`mark_meshed`) + 증분 bbox,
    `freeze()`로 **불변 `Stroke`** 동결.
- **`WritingMaterial` trait + `Materials` 퍼사드** (`pen.rs`)
  - `LockerProfile` enum 분기 제거 → `BallPenProfile`/`FountainProfile`/`ConstantMaterial`
    이 `WritingMaterial` 구현.
  - `widths()`(배치)를 trait로 이동 → `halves_for_stroke`가 `Materials::for_tool(...).widths(...)`로 위임.
  - `material_for` 자유 함수 → `Materials` 퍼사드로 교체.
- **`InkPipeline::down`에 `tilt_mag` 파라미터 추가** — 만년필(italic/틸트) 폭 대비 보존.
  - 계약 테스트 `pipeline_down_threads_tilt_into_width_locker`.
- **`smoothing <= 0.001`이면 1€ 필터 생략 → raw 좌표 통과** — 앱의 "스무딩 꺼짐" 경로와
  행동 일치. 계약 테스트 `pipeline_smoothing_zero_passes_raw_coords`.

### 앱 배선 (input / ink / finish_stroke / commit_dot)
- `FreeDfApp`에 `ink: Option<InkPipeline>` 필드 도입, `width_locker`·`smooth_x/y/p`·`smooth_active` 제거.
- `input.rs` 펜 down/drag가 파이프라인 경유; `active_stroke`는 `pipeline.live().points`와
  매 드래그 동기화해 **렌더 == 커밋 (WYSIWYG) 보존**.
- `ink.rs::finish_stroke`가 `self.ink.take().up(0)`의 **동결·불변 `Stroke`**로 커밋.
- `ink.rs::commit_dot`(탭 점)도 동일 파이프라인 down→up 경유.
- `ActiveStroke::push` 제거(미사용).

### 서비스 계층 — 인터페이스화 + 컴포지션 루
- **`BakeService` 비제네릭화** (`freedf-canvas/bake.rs`): `W: BakeWorker` 제네릭 →
  `Arc<Box<dyn BakeWorker + Send + Sync + 'static>>` 보유. `start(Box<dyn BakeWorker>)`로 생성.
  → DI/테스트로 서비스 교체 가능.
- **`AppDeps` 컴포지션 루 신설** (`crates/freedf/src/app/deps.rs`): `{ clock, logger }`를
  `AppDeps::compose(...)`로 조립, `FreeDfApp::new(cc, db, ..., deps, ...)`에 **생성자 주입**.
  - `logger: Logger` 파라미터 → `deps: AppDeps`로 교체.
  - `FreeDfApp::now_ms() = self.clock.now_ms()` — 앱 전역이 **주입된 Clock** 사용.

### Clock 배선 (전역)
- 앱의 자유 `now_ms()`(벽시계 직접) 호출을 `FreeDfApp::now_ms()`(주입 Clock)으로 전환
  (canvas/actions/panels/gamepad 포함).
- 자유 `now_ms()`는 **sync_storage 백그라운드 워커 전용**으로 잔존.

### 문서
- `docs/ink-pipeline-design.md`: 실제 시그니처(tilt_mag, smoothing-raw), 앱 배선 표,
  `AppDeps` 컴포지션 루 섹션 반영.

### 검증
- `cargo test -p freedf-core`: **209 passed / 0 failed** (단위 198 + 통합 11), 0 경고.
- `cargo test -p freedf-canvas`: **29 passed / 0 failed**, 0 경고.
- 워크스페이스 `cargo build`: **0 에러 / 0 경고**.
