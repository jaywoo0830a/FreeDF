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

### 성능 — OPTIMIZATION.md + P0 계측 (추정 벤치)
- **`docs/OPTIMIZATION.md` 신설**: live 잉크 렌더의 점당 비용 병목(live 5~7 O(n) 패스 +
  할당 7+, `value_noise` 점당 ~16 해시)을 정리하고, **증분 tail O(k) + 근사/비트(노이즈 타일·LUT·
  단일 패스·비트 플래그)** 전략의 **예상 절감 수치**(전체 재구성 −50~65%, 그리기 성장 경로 −90~99%,
  할당 −80~90%)와 벤치 표를 문서화.
- **P0 벤치 훅** `#[ignore]` `bench_live_render_cost_per_point` (`freedf-canvas/core_mesh.rs`):
  `CoreRibbonMesher::append_stroke` 실측 — **n=1k: 560 µs(560 ns/pt), n=10k: 5.93 ms(593 ns/pt)**(debug).
  OPTIMIZATION.md §4의 baseline을 실측치로 교체. CI에선 실행되지 않음(`ignored`).
- **동작보존 최적화 구현 (성능 1차)** — 출력·결정성 그대로:
  - `InkGrain::density_lr` + `value_noise_pair`/`ink_field_pair` (`freedf-core/ink.rs`): 단면(좌/우)
    밀도가 같은 `u`를 쓰는 점에서 각 옥타브의 **x 공통 부분을 1번만** 계산 (해시 수는 유지).
    parity 테스트 `density_lr_matches_two_density_calls`로 `density` 2회 호출과 동일함을 고정.
  - `append_stroke_ribbon` 메시 용량 예약 (`freedf-canvas/core_mesh.rs`): realloc 방지.
  - 실측: **n=1k 560→548 µs(−2.1%), n=10k 5933→5593 µs(−5.7%)** (debug). OPTIMIZATION.md §4에 실측 갱신.
  - 큰 폭(50~65%)은 **시각을 바꾸는 근사**(노이즈 타일·LUT·절대 호 길이)로만 가능 → GUI 검증
    필요로 **미구현 유지**하고 문서에 "P3 근사(추정)"로 구분.
- **시각 근사 캔버스 (P3 부분)**: `InkGrain.fast_noise`(기본 false) — 켜면 **고주파 위킹
  옥타브를 생략**해 질감 계산이 절반으로 줄어듭니다. `density`/`density_lr` 분기.
  - **앱 Debug HUD에 체크박스** 추가 (`paint_debug_hud` → `&mut self`, `fast_ink_noise` 필드) —
    켜는 즉시 펜/만년필 그레인에 전파되어 live·굽기 양쪽에 반영 (view_key에 그레인이 있어 캐시 재구성).
  - 테스트 `fast_noise_is_deterministic_bounded_and_non_popping`로 결정성·범위·no-popping 보호.
- **fast ink noise 기본 ON** — `InkGrain::default().fast_noise = true` + 앱 `fast_ink_noise` 초기값 true.
  parity 테스트(`density_lr_matches_two_density_calls`)를 두 모드(정확/fast) 모두 검증하도록 확장.
  P0 벤치 실측: **n=1k 504 µs(−10%), n=10k 5.13 ms(−13.6%)** (초기 560 µs/5.93 ms 대비). OPTIMIZATION.md §4 갱신.
- **Debug HUD 접근성**: `row_top`(상단 툴바)에 **"Debug HUD" 토글 버튼** 추가 — 설정 창을
  열지 않아도 오버레이를 바로 켜고 끌 수 있습니다 (기존 Pen Settings→Input & cursor(접힘) 경로 불필요).
- **매크로 기본 비활성화**: `MacroState::default()`의 `page_enabled`/`tab_enabled`/`desktop_enabled`/
  `desktop_focus_only`를 **모두 false**로 — 새 세션에서 매크로가 기본으로 꺼져 있고(UI 섹션 비활성),
  사용자가 Macro 창에서 개별 활성화합니다.

### 버그 수정
- **연속 줌 시 프리즈→크래시 (OOM)** — PDF 페이지 래스터의 최대 차원을
  `MAX_RENDER_DIM = 4096`px으로 제한 (`freedf/src/pdf.rs::MAX_RENDER_DIM`,
  `render_page`의 너비/높이/최대 차원 클램프 상한을 65,000→4096으로 인하,
  `ensure_texture`/`prefetch` 호출부 갱신).
  - 원인: 줌이 커지면 pdfium이 거대한 비트맵(고해상도·고줌에서 수십~수백 MB)을 할당하고,
    `as_rgba_bytes()`·`ColorImage` 복제로 순간 최대 ~3배 메모리 → 릴리즈에서 메모리 폭주(OOM)
    → 프리즈 후 크래시. 4096px 상한은 재생산 약 67MB/버퍼(복제 포함 ~200MB)로 안전.
  - 100%·fit-width·4K 고해상도는 상한 아래라 화질 영향 없음. 극단 줌만 소프트 (GPU 스케일).

### 검증
- `cargo test -p freedf-core`: **211 passed / 0 failed** (단위 200 + 통합 11), 0 경고.
- `cargo test -p freedf-canvas`: **29 passed / 0 failed / 1 ignored** (P0 벤치), 0 경고.
- 워크스페이스 `cargo build`: **0 에러 / 0 경고**.
