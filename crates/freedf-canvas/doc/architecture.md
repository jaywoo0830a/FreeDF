# freedf-canvas 아키텍처

> 캔버스(잉크 메시) 렌더링 파이프라인을 담당하는 순수 계산 크레이트.
> IO·GPU·스레드는 전부 경계 밖(앱/워커)에 있고, 이 크레이트는 **결정적 계산만** 합니다.

## 1. 계층 구조

```text
┌─────────────────────────── freedf (앱, egui/pdfium) ───────────────────────────┐
│  입력(펜) · 페이지 텍스처(pdfium) · Surface 구현(egui Painter) · UI 상태        │
└───────────────┬──────────────────────────────────────────┬────────────────────┘
                │ ① 스트로크 추가/삭제                       │ ④ DrawCommand 제출
┌───────────────▼──────────────────────────────────────────▼────────────────────┐
│                          freedf-canvas (순수 계산)                              │
│                                                                                │
│  scene::SceneStore ──▶ changes_since(rev) ──▶ 증분/전체 굽기 결정               │
│  ink::{WidthModel × AlphaModel × GrainSeed} ──▶ CoreRibbonMesher                │
│  bake::{BakeWorker(순수) → BakeService(스레드, try_send/try_recv)}              │
│  surface::{FrameAssembler(순수) → DrawCommand}                                  │
└───────────────┬────────────────────────────────────────────────────────────────┘
                │ ③ StrokeRibbon / Mesh (페이지 좌표)
┌───────────────▼────────────────────────────────────────────────────────────────┐
│                freedf-core (물리 모델 — pen/ink/paper, 순수)                    │
└────────────────────────────────────────────────────────────────────────────────┘
```

- **freedf-canvas는 freedf-core에 의존합니다.** 물리 모델(폭 리본·잉크 스밈·질감)은
  core의 검증된 구현을 재사용하고, canvas는 **파이프라인**(장면 diff·굽기·프레임
  조립)만 소유합니다.
- 순수성 계약은 그대로: 시간은 `now_ms` 인자, 시드는 명시, 전역 상태 없음.
  "무의존성"은 **IO/GPU/스레드 의존 없음**으로 재정의합니다.

## 2. 모듈 책임

| 모듈 | 책임 | 교체 지점(트레이트) |
|---|---|---|
| `clock` | 시간 공급 — 계산은 전부 `now_ms` 인자 | `Clock` |
| `geom` | 페이지 좌표 뉴타입 + 뷰 변환(순수) | — |
| `scene` | 스트로크 저장소, `Revision` 기반 증분 `changes_since` | — |
| `ink` | 폭/알파/질감 모델, `Mesh`, 조합형 `RibbonMesher` | `WidthModel`, `AlphaModel`, `Mesher` |
| `core_mesh` | freedf-core 리본 지오메트리와의 접착 — `halves_for_stroke`, `alphas_for_stroke`, `append_stroke_ribbon`, `CoreRibbonMesher` | — |
| `bake` | 순수 워커 + 논블로킹 서비스 | `BakeWorker` |
| `surface` | 프레임 조립(순수) + 그리기 대상 | `Surface` |

## 3. 무블록 계약 (UI 스레드)

1. `BakeService`의 공개 메서드는 `try_send`/`try_recv`뿐 — `request`는 진행 중
   `Busy`, `poll`은 완료분만. 블로킹 `recv`는 워커 스레드 내부에만 존재.
2. 굽기는 페이지 좌표 메시를 만들고, 팬/줌은 그리기 단계 `Transform`으로만 적용
   → **팬만 바뀌면 재굽기 없음** (앱 쪽에서 정점 이동/변환 캐시로 구현).

## 4. 앱(freedf) 마이그레이션 매핑

| 앱 기존 코드 | freedf-canvas 대응 |
|---|---|
| `canvas/mod.rs` `append_ribbon`(egui) | `Mesh` + `append_stroke_ribbon` (페이지 좌표) |
| `canvas/mod.rs` `stroke_halves` | `core_mesh::halves_for_stroke` |
| `canvas/paint.rs` 알파 합성(soak×grain) | `core_mesh::alphas_for_stroke` |
| `canvas/paint.rs` `build_ink_mesh`/`append_ink_strokes` | `bake::BakeWorker` (전체/증분) + `Mesh::append` |
| `canvas/mod.rs` `ink_baked_rev`/`ink_baked_count` | `scene::SceneStore::changes_since` 계약 |
| egui 변환/드로우 | `surface::{FrameAssembler, Surface}` — egui 어댑터는 앱에 유지 |
| 페이지 텍스처(pdfium)/입력/커서 | **마이그레이션 안 함** — 순수 크레이트 경계 밖 |

## 5. 마이그레이션 로드맵

- [x] 1단계: 인터페이스 + 계약 테스트 (골격)
- [x] 2단계: `core_mesh` — freedf-core 리본 접착 (실제 지오메트리, 앱과 동일 출력)
- [x] 3단계: 앱의 병합 잉크 메시를 canvas `Mesh`로 전환 (egui 변환은 경계 어댑터
      `canvas_mesh_to_egui` + 팬/줌 변환 캐시 — 페이지 좌표 굽기로 `ink_baked_pan` 제거)
- [~] 4단계: `SceneStore` 채택 — 굽기 요청이 `SceneSnapshot`을 사용(부분 완료).
      증분 판정 자체는 아직 앱의 `ink_baked_rev`/`ink_baked_count` —
      `changes_since`로 대체하면 단일 진입점으로 통일.
- [x] 5단계: `BakeService`로 **전체 재굽기를 백그라운드 스레드로 분리**
      (UI는 try_send/try_recv만 — `InkBakeWorker`가 RwLock 메셔 스냅샷 사용,
      도착 시 추가 획 증분 병합, 페이지/세대/줌 불일치면 폐기 후 재요청)
- [x] 6단계: 활성 획 경로도 canvas `Mesh` 경유로 통일 (`append_ribbon` egui 레거시 제거)

## 6. 테스트 전략

- 순수 워커는 스레드 없이 `now_ms` 고정으로 결정적 검증.
- `FakeClock`/`RecordingSurface`/게이트 워커로 타이밍·IO 제거.
- 계약 테스트: rev diff 증분, 좌표 왕복, 메시 well-formed, 무블록 서비스.
