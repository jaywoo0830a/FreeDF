# InkPipeline 설계 — 인터페이스 / 불변성 / TDD 명세

> **목표**: 진행 획을 한 객체로 묶어 **호출 시퀀스를 압축**하고,
> **증분(frontier) + 커밋 시 불변(동결)** 으로 **프레임당 연산을 줄인다**.
>
> 구현 위치: `crates/freedf-core/src/pipeline.rs`
> 검증: `cargo test -p freedf-core` (TDD — 테스트 선 작성 → RED → 구현 → GREEN)

---

## 1. 두 가지 설계 이점

1. **증분(frontier) — 매 프레임 O(신규점)**
   - `LiveStroke`는 **append-only** 점 버퍼. `mark_meshed()`가 가리키는
     **frontier(`mesh_done`) 이전은 다시 쓰지 않는** 준-불변 접두부.
   - renderer는 `tail()`(= `points[frontier..]`, 덜 구운 꼬리)만 읽어
     메시에 덧붙이고 `mark_meshed()`로 전진.
   - → 점 추가 프레임에만 O(k), 그 외 프레임은 재계산 0.
   - → `fix_last`는 frontier 이전을 못 건드려 접두부 불변을 보장.

2. **커밋 시 불변(동결) — 안전한 공유**
   - `LiveStroke.freeze()`가 `LiveStroke` 전체를 읽기 전용 `Stroke`로 **복사·동결**.
   - 이후 라이브에 점을 추가해도 커밋본은 변하지 않음(테스트로 고정).
   - → BakeService 워커가 `Arc`/불변 `Stroke`를 **락 없이** 읽기 가능.
   - → History/Undo가 `Edit{old: Stroke, new: Stroke}`를 얕게 공유(클론 절감).

---

## 2. 인터페이스

### `LiveStroke` — 진행 중 획 (append-only + frontier + 증분 bbox)

```v
pub struct LiveStroke {   // 모든 필드는 pub (model::Stroke와 같은 데이터 홀더 스타일)
    tool: ToolType
    color: [u8; 4]
    width: f32
    points: Vec<StrokePoint>     // pub — 진행 점
    mesh_done: usize             // pub — frontier, 이 인덱스 앞은 확정 구간
    bbox: Option<[f32; 4]>       // pub — 증분 경계 상자
}

impl LiveStroke {   // A안: 공개 메서드는 핵심 6개만. 나머지는 pub 필드 직접 읽기
    pub fn begin(tool, color, width) -> Self
    pub fn append(&mut self, p) -> usize             // append-only + bbox O(1)
    pub fn fix_last(&mut self, p)                    // frontier 이전은 no-op(불변 보호)
    pub fn tail(&self) -> &[StrokePoint]             // frontier 이후 꼬리(증분 메시 입력)
    pub fn mark_meshed(&mut self)                    // frontier = len
    pub fn freeze(&mut self, id, created_ms) -> Stroke  // 불변 커밋본
}
```

### `InkPipeline` — 필터·락커·진행 획 조율 (down/drag/up)

```v
pub struct InkPipeline {
    materials: Materials           // ball/fountain 프로파일을 값으로 보관 (퍼사드)
    max_width_pt: f32
    smoothing: f32                 // ≤ 0.001이면 1€ 필터 생략 → raw 좌표 통과
    filter_x / filter_y / filter_p: Option<OneEuroFilter>
    locker: Option<WidthLocker>    // tilt_mag는 down()에서 주입
    live: Option<LiveStroke>
}

impl InkPipeline {
    pub fn new(materials: Materials, max_width_pt, smoothing) -> Self
    pub fn set_smoothing(&mut self, s: f32)
    pub fn smoothing(&self) -> f32
    pub fn is_drawing(&self) -> bool
    pub fn live(&self) -> Option<&LiveStroke>

    // 외부로 노출되는 호출은 3개뿐 — 요구된 "호출 시퀀스 압축"
    pub fn down(&mut self, tool, color, x, y, pressure, t: f64, t_ms: u64, tilt_mag: f32) -> StrokePoint
    pub fn drag(&mut self, x, y, pressure, t: f64, t_ms: u64) -> Option<StrokePoint>
    pub fn up(&mut self, id: u64) -> Option<Stroke>   // 마지막 폭 확정 → freeze → 비활성

    // 내부
    fn filter_lock(&mut self, x, y, pressure, t, t_ms) -> StrokePoint
}
```

`drag()` 1회 = `[필터(x/y/p) → WidthLocker.push(이전 점 폭 확정) → LiveStroke.append]`.

- `tilt_mag`(0..1)는 down()에서 WidthLocker로 전달돼 italic/틸트 폭 대비에 반영됩니다
  (앱의 만년필 틸트 보존 — 테스트 `pipeline_down_threads_tilt_into_width_locker`).
- `smoothing <= 0.001`이면 필터를 건너뛰어 **raw 좌표**를 그대로 통과시킵니다 —
  앱의 "스무딩 꺼짐 → raw push" 경로와 행동 일치(테스트 `pipeline_smoothing_zero_passes_raw_coords`).

### `WritingMaterial` — 필기 재료 추상화 (열린 확장)

`WidthLocker`의 폭 계산은 `LockerProfile` enum 분기가 아니라 **trait** 에 위임합니다.
새 재료 추가 = struct + `impl WritingMaterial` + `Materials::for_tool` 등록 1줄. locker/pipeline은 무변경(open/closed).

```v
pub trait WritingMaterial: Send + Sync {
    fn smoothing_alpha(&self) -> f32   // 인과적 EMA 계수 (0 = 무스무딩, e.g. 하이라이터)
    fn point_width(&self, max_width_pt, pressure, tilt_mag, speed, dir) -> f32 // 이탤릭+클램프 포함
    fn widths(&self, max_width_pt, pts, tilt_mag) -> Vec<f32>                  // 배치(굽기 경로)
}

impl WritingMaterial for BallPenProfile   { /* dir 무시 — 회전 불변 */ }
impl WritingMaterial for FountainProfile { /* width_at → italic_factor → clamp */ }
impl WritingMaterial for ConstantMaterial { /* 항상 max_width_pt (하이라이터) */ }

// 단일 분기점(퍼사드) — ball/fountain을 소유하고 ToolType → 재료로 해석
pub struct Materials { ball: BallPenProfile, fountain: FountainProfile }
impl Materials {
    pub fn new(ball, fountain) -> Self
    pub fn default() -> Self
    pub fn for_tool(&self, tool: ToolType) -> Option<Box<dyn WritingMaterial>>
}
```

- `WidthLocker`는 `profile: Box<dyn WritingMaterial>`을 보유, `lock_width()`는
  `match` 없이 `profile.point_width(...)` **한 번의 다형 호출**.
- `WidthLocker::with_material(Box<dyn WritingMaterial>, ...)` — 임의 재료 주입 확장점.
- `WidthLocker::new(tool, max, &materials, tilt)` — `Materials::for_tool`로 위임.
- `halves_for_stroke`(freedf-canvas 굽기 경로)도 `&materials`를 받아 `for_tool(...).widths(...)`로
  Fountain/Highlighter 분기를 제거해 동일한 open/closed 이점을 얻습니다.
- `InkPipeline`/`CoreRibbonMesher`는 별도 ball/fountain 대신 `materials: Materials`를 보관.

---

## 3. 수준별 변경 (기존 대비)

| | 기존 | InkPipeline 설계 |
|---|---|---|
| FreeDfApp fan-out | `OneEuroFilter`·`WidthLocker`·`ActiveStroke` 직접 연관 | `InkPipeline` 하나로만 |
| 드래그 외부 호출 | `Input→F/W/S` 3단 전파 | `drag()` 1회 (내부 위임) |
| 렌더 | 매 프레임 `active_stroke.clone()` O(n) + 전체 halves/리본 O(n) | `tail()`만 증분 append O(k) |
| 폭 | 프레임마다 `halves_for_stroke` 재확인 | 잠금 폭 단일 판정 + frontier로 변경 불가 구간 보호 |
| 커밋 획 | 변경 가능(복사 등) | **불변 `Stroke`(동결)** → 워커 락 없음 / History 얕은 공유 |
| bbox | 전역 스캔 | append 시 O(1) 증분 |

---

## 4. 쓰레기/불변성 의미

- **append-only(준-불변)**: 진행 중엔 점을 추가만. 기존 점을 고치는 `fix_last`는
  frontier **이전이면 no-op** (테스트 `live_stroke_fix_last_blocked_at_frontier`가 고정).
- **동결(freeze, 완전 불변)**: `up()` 시 `LiveStroke` → `Stroke` 복사.
  이후 라이브 변이는 커밋본에 영향 없음 (테스트 `live_stroke_freeze_detaches_from_live`).
- **비용**: freeze는 펜업 1회 O(k) 복사. 그 외 프레임엔 clone 없음.
- **희생**: 커밋 후 점 수정은 COW/재생성으로 갈아타야 함 (지우개와 같은 희소 경로는 허용).

---

## 5. TDD — 테스트 목록 (RED → GREEN)

구현 전 테스트만 작성해 **RED**(`cannot find type InkPipeline/LiveStroke`) 확인 후,
구현으로 **GREEN**. 최종 `cargo test -p freedf-core` **209개 전부 통과**(0 경고) +
freedf-canvas 29개. WritingMaterial 확장분(`WritingMaterial` trait + `Materials::for_tool` 퍼사드)도
같은 방식(TDD — `for_tool_*`, `locker_accepts_any_custom_writing_material`,
`writing_material_widths_survive_via_factory_box`, `constant_material_widths_are_all_max`)으로 검증.
피어-입력 tilt/smoothing-raw 계약도 같은 방식으로 추가(`pipeline_down_threads_tilt_into_width_locker`,
`pipeline_smoothing_zero_passes_raw_coords`).

---

## 6. 앱 배선 + 서비스 조립(AppDeps) — 구현 상태

### 6-1. InkPipeline 앱 배선 (완료)

`FreeDfApp`은 이제 필터·선폭 확정·진행 획을 하나의 `ink: Option<InkPipeline>`으로 조율합니다.

| 항목 | 배선 |
|---|---|
| down | `input.rs` — 스트로크 시작 시 `InkPipeline::new(Materials, width, smoothing)` + `down(tool, color, x, y, pressure, t, t_ms, tilt)` |
| drag | `input.rs` — `pipeline.drag(...)` 1회로 필터→폭 확정→점 추가 위임 |
| up | `ink.rs::finish_stroke` — `self.ink.take().up(0)`으로 **동결·불변 `Stroke`** 반환 후 커밋 |
| 점(탭) | `ink.rs::commit_dot` — 동일 파이프라인 down→up 경유 |
| 렌더 미러 | `active_stroke`는 `pipeline.live().points`와 **동기화**(매 드래그) — 렌더 == 커밋 (WYSIWYG 보존) |
| 필드 | `width_locker`·`smooth_x/y/p`·`smooth_active` **제거** → `ink` 1개로 통합 |

### 6-2. 서비스 컴포지션 루 — `AppDeps` (완료)

펜 프로파일은 per-session 데이터라 `Materials` 값으로 명시 주입(DI 컨테이너 대상 아님).
**교차 횡단 서비스**(`Clock`·`Logger`)는 `crates/freedf/src/app/deps.rs`의 `AppDeps`로 묶어
컴포지션 루(`main.rs`)에서 한 번 조립해 `FreeDfApp::new(deps)`에 **생성자 주입**합니다.

```v
pub struct AppDeps {
    pub clock: Box<dyn Clock + Send + Sync>,  // 프로덕션 SystemClock
    pub logger: Logger,                        // 프로덕션 to_sink / 테스트 disabled
}
impl AppDeps {
    pub fn compose(clock: Box<dyn Clock + Send + Sync>, logger: Logger) -> Self
}
```

- `FreeDfApp::now_ms()` = `self.clock.now_ms()` — 앱 전역(캔버스 포함)이 **주입된 시계**로
  벽시계를 읽습니다(자유 `now_ms()`는 sync_storage 백그라운드 전용으로만 잔존).
- bake 서비스(`BakeService`)는 이미 **비제네릭(인터페이스 기반)**이며, 세션 설정(메셔)에
  묶여 있어 deps 대신 `FreeDfApp::new`가 메셔를 만든 뒤 조립합니다.
- 테스트는 `AppDeps::compose(Box::new(FakeClock), Logger::disabled())`로 갈아 끼웁니다.

| # | 테스트 | 검증 계약 |
|---|---|---|
| 1 | `live_stroke_incremental_bbox_matches_scan` | 증분 bbox == 전체 스캔 bbox |
| 2 | `live_stroke_frontier_makes_tail_shrink_and_grow` | frontier 전진 시 꼬리 축소, append 시 꼬리 성장 |
| 3 | `live_stroke_fix_last_blocked_at_frontier` | frontier 이전 fix는 no-op, 이후는 반영 |
| 4 | `live_stroke_freeze_detaches_from_live` | freeze 후 라이브 추가해도 커밋본 불변 |
| 5 | `pipeline_down_starts_with_locked_first_point` | down → len 1, 첫 점 폭 잠금 |
| 6 | `pipeline_drag_appends_and_locks_widths` | drag마다 점 추가, 전 점 폭 잠금 |
| 7 | `pipeline_up_preserves_wysiwyg_no_penup_change` | 펜업 시 폭 불변(WYSIWYG), 비활성, id 유지 |
| 8 | `pipeline_second_stroke_starts_fresh` | 새 획은 이전 점을 이어받지 않음 |

실행 출력 (요약):
```
running 8 tests
test pipeline::tests::live_stroke_freeze_detaches_from_live ... ok
...  (7 more ok)
test result: ok. 8 passed; 0 failed
```

---

## 6. 다이어그램

정적(클래스) / 동적(시퀀스) 다이어그램 원천과 렌더 결과:
- `docs/write-pipeline.md` — 설명 + SVG 참조
- `docs/write-pipeline-class.svg` — 정적 클래스(InkPipeline, Association)
- `docs/write-pipeline-sequence.svg` — 동적 시퀀스(down/drag/up + freeze)