# Changelog

형식: **[버전/일자]** 변경 요약. 항목은 되도록 **행동/계약 변경**과 **리팩터**를 구분합니다.
검증은 `cargo build`(0 경고) + `cargo test -p freedf-core` / `-p freedf-canvas` 기준입니다.

## [Unreleased] — 2026-09-13

### UI/UX 개편 (툴바 계층 · 그룹핑 · 진단 확충) — `docs/UI-UX-REPORT.md` 반영
- **기본 한글/UI 폰트를 Asta Sans로 교체** — Google Fonts Asta Sans(위 300–800) Regular를
  내장해 'Proportional'의 1순위로 등록(Inter·NanumGothic은 폴백 유지). `fonts.rs`.
- **툴바 Row1 계층화(P0)** — 자주 안 쓰는 도구/설정(Dictionary·Media Server·Media·Macro·
  Gamepad·Cache·정렬)을 1층에서 제거하고 끝의 **"More" 오버플로 메뉴**로 이동. 첫 줄 요소를
  약 22 → 16개(그룹: 창/패널 · 명령 · 저장 · More)로 정돈. `rows.rs::row_top`.
- **Fast ink noise 위치 이동(P1)** — Debug HUD 체크박스 제거, 펜/만년필 설정 창의
  **"Ink grain" 필드셋**(도구별 질감 옵션)으로 이동. `paint.rs`·`toolbar/mod.rs`.
- **Debug HUD 확충(P3 부분)** — 디버그 창에 **"System / About"** 접이식 섹션 추가: 소프트웨어
  버전(`CARGO_PKG_VERSION`)·OS·아키텍처·빌드 프로필·렌더러(glow/wgpu)·PID·CPU·DB 연결 상태 +
  **"Copy diagnostics"**(클립보드로 1-클릭 진단 복사). `gamepad.rs`.
- **설정 다이얼로그 통합(P2)** — 11개 분산 모달(도구/커서/종이/캔버스/휠/페이지/엣지/포커스/
  서버/매크로/게임패드)을 **단일 "Settings" 창(좌측 탭 레일 + 우측 내용)** 으로 통합.
  툴바의 각 `*_open` 요청이 해당 탭으로 라우팅되고 창을 닫으면 해제. `settings.rs`·`mod.rs`.
- 그 외: 미사용이 된 `fast_ink_noise` App 필드/`form` import 제거, `ui::dialog::dialog` 헬퍼 제거(경고 0 유지).
- **툴바 전체에 컴포넌트 키트 적용** — Row1~Row3의 모든 그룹 경계를 `layout::group`+`vdivider`,
  행 조립은 `layout::vstack`+`hseparator`, `toolbar()` 패널은 `containers::container()` 래핑.
  More 메뉴의 순수 텍스트 버튼을 3계층 `buttons::Button::secondary`로 교체. `rows.rs`/`toolbar/mod.rs`.
- **툴바 키트 반복 적용(R2)** — Row2를 G1 Page/G2 Canvas/G3 ColorWheel/G4 Paper로 `layout::group`
  세분화, Rotate 메뉴 4개 버튼을 `buttons::Button::secondary().enabled()`로, Paper 색 스와치 행을
  `layout::hstack`(flex)로 래핑.
- **디자인 시스템 키트 `ui::ds` + 자율 적용(R3)** — `Tone`·`badge`·`status_dot`·`alert`·`kbd`·`card`
  를 `crate::ui::ds`로 신설(재사용, dead_code 유지). 상태바에 연결 배지(online/offline), Debug HUD
  System에 `badge`(DB), 게임패드 로그에 `scroll` 키트 적용. Row3 펜/하이라이터 스와치를
  `layout::hstack`(flex)로 래핑. (정리: `ui::components` 확장 블록은 `ui::ds`로 이관.)
- **앱 레이아웃 프레임을 원시 컨테이너로 감쌈** — 탭바(`tabs_bar`), 툴바, 상태바, 중앙
  캔버스 스테이지를 `containers::container()`(패딩/필/보더) + `layout::vstack`로 래핑.
  여백/스타일이 컴포넌트 키트를 통해 일원화됨.
- **웹 엘리먼트 라이브러리 확장(R1)** — `ui::components`에 `Tone`(세마틱 색)과 함께
  `badge`/`tag`/`alert`/`status_dot`/`progress`/`spinner`/`kbd`/`code`/`breadcrumb`/
  `divider_label`/`avatar`/`count`/`tabs`/`card` 추가(Bootstrap `.badge/.alert/.progress/…` 매핑).
- **컴포넌트 라이브러리 확장(React·Bootstrap 스타일)** — 3계층 버튼 `ui::buttons`
  (`Button::{primary,secondary,ghost}`·size/danger/icon·숏컷), 범용 컨테이너 `ui::containers`
  (margin/padding/fill/border 컨테이너 + grid + flex row/centered), 스크롤바 `ui::scroll`,
  토스트 `ui::toast`(우상단 스택·자동 소멸·✕; 앱 `toasts` 필드에 배선, 시작 환영 토스트).
  폼 확장 `textarea`/`segmented`. 문서 `docs/UI-COMPONENTS.md` 갱신.
- **레이아웃/범용 컴포넌트 키트 신설(React·Bootstrap 스타일)** — `crate::ui::layout`(8px 그리드
  `SP_*`, `hstack`/`vstack`, `vdivider`/`hseparator`, `toolbar_row`/`group`)과
  `crate::ui::components`(`pill`/`caption`/`help`/`placeholder`) 추가. `toolbar_row`를 app→ui로 이전해
  재사용. 툴바 조립부(`toolbar()`·`row_top`)를 이 키트(vstack/hseparator/vdivider/group)로 재구성.
  설계서 `docs/UI-COMPONENTS.md`(툴바 계층 트리 + 카탈로그 + 작성 가이드).
- **툴바 Row1 컴포넌트 분해(React 스타일)** — `row_top`을 컨테이너 컴포넌트 합성으로 정리:
  `toolbar_panel_group`(패널 토글 묶음)과 `toolbar_overflow_menu`(More 메뉴)를 메서드 컴포넌트로
  추출, 프레젠테이션은 `crate::ui` 컴포넌트(props)가 담당·상태는 컨테이너가 연결하는
  기존 아키텍처 주석과 일치. `rows.rs`.

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

### UI — 통합 디버그 HUD
- 흩어져 있던 **펜/캔버스 오버레이**와 **분리된 "Gamepad debug" 창**을 **한 개의
  `Debug HUD` 윈도우**로 통합 (`crates/freedf/src/app/gamepad.rs::debug_hud_ui`).
  - `Pen / Canvas`(기본 열림) + `Gamepad`(기본 닫힘) 접이식 섹션을
    `form::fieldset`으로 구성. 토글(이미지 근사 등)은 `form::check` 사용.
  - 단일 토글 `debug_hud`로 열고 닫으며, 창 ✕로 닫아도 상태가 동기화.
  - 캔버스 오버레이 `Area`(paint_debug_hud)와 게임패드 전용 창/`gamepad_debug_open`
    필드 제거, 로그는 전부 영어로 통일 (한국어 "수신됨/없음/off(…)" → 영어).
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
