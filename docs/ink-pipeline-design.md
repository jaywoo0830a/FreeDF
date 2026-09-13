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
pub struct LiveStroke {
    tool: ToolType
    color: [u8; 4]
    width: f32
    points: Vec<StrokePoint>
    mesh_done: usize   // frontier — 이 인덱스 앞은 확정 구간
    bbox: Option<[f32; 4]>   // 증분 경계 상자
}

impl LiveStroke {
    pub fn begin(tool, color, width) -> Self
    pub fn len(&self) -> usize
    pub fn is_empty(&self) -> bool
    pub fn last(&self) -> Option<StrokePoint>        // 복사
    pub fn append(&mut self, p) -> usize             // append-only + bbox O(1)
    pub fn fix_last(&mut self, p)                    // frontier 이전은 no-op(불변 보호)
    pub fn points(&self) -> &[StrokePoint]           // 전체(읽기)
    pub fn mesh_done(&self) -> usize
    pub fn tail(&self) -> &[StrokePoint]             // frontier 이후 꼬리(증분 메시 입력)
    pub fn mark_meshed(&mut self)                    // frontier = len
    pub fn bbox(&self) -> Option<[f32; 4]>
    pub fn freeze(&mut self, id, created_ms) -> Stroke  // 불변 커밋본
}
```

### `InkPipeline` — 필터·락커·진행 획 조율 (down/drag/up)

```v
pub struct InkPipeline {
    ball: BallPenProfile
    fountain: FountainProfile
    max_width_pt: f32
    smoothing: f32
    filter_x / filter_y / filter_p: Option<OneEuroFilter>
    locker: Option<WidthLocker>
    live: Option<LiveStroke>
}

impl InkPipeline {
    pub fn new(ball, fountain, max_width_pt, smoothing) -> Self
    pub fn set_smoothing(&mut self, s: f32)
    pub fn smoothing(&self) -> f32
    pub fn is_drawing(&self) -> bool
    pub fn live(&self) -> Option<&LiveStroke>

    // 외부로 노출되는 호출은 3개뿐 — 요구된 "호출 시퀀스 압축"
    pub fn down(&mut self, tool, color, x, y, pressure, t: f64, t_ms: u64) -> StrokePoint
    pub fn drag(&mut self, x, y, pressure, t: f64, t_ms: u64) -> Option<StrokePoint>
    pub fn up(&mut self, id: u64) -> Option<Stroke>   // 마지막 폭 확정 → freeze → 비활성

    // 내부
    fn filter_lock(&mut self, x, y, pressure, t, t_ms) -> StrokePoint
}
```

`drag()` 1회 = `[필터(x/y/p) → WidthLocker.push(이전 점 폭 확정) → LiveStroke.append]`.

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
구현으로 **GREEN**. 최종 `cargo test -p freedf-core` 200개 전부 통과.

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