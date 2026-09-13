# OPTIMIZATION — live 잉크 렌더 성능: 예상 벤치마크

> 이 문서는 **성능 개선 전략**(전체 재구성 축소 + Seq-Call 축소 + 근사/비트 계산 →
> O(n) 수렴)의 **예상(추정) 벤치마크**입니다.
> 수치는 좋은 하드에서의 **공학 추정치** — P0 실측으로 교정해야 합니다.
> 관련 구현 위치: `freedf-core/src/ink.rs`(그레인/노이즈), `freedf-core/src/pen.rs`
> (`stroke_ribbon_lr`, 폭 계산), `freedf-canvas/src/core_mesh.rs`(live/굽기 메시),
> `freedf/src/app/canvas/paint.rs`(`active_mesh` 스로틀 캐시).

---

## 1. 방법론 (P0 — 계측 우선)

- 마이크로벤치: 고정 점열(N=1k/10k)을 meshing하는 **순수 함수 시간**을
  `std::time::Instant`로 반복 측정(중앙값). UI 스레드 로드와 분리해 **캔버스 순수 시간**만 측정.
- **측정 단위**: `ns/point`(점당 나노초). **할당 수**는 별도 카운터.
- P0 벤치 훅: `core_mesh.rs`의 `#[ignore]` 테스트 (`bench_live_render_cost_*`).
- 이 문서의 %는 **하한~상한 추정** — 실측 후 교정.

### 가정
- 대화형 그리기 중 **성장 경로**(점 2~5개 추가 후 스로틀 재구성)가 주 병목. n=200~600, k=2~5/틱.
- 팬/줌·커밋 시의 전체 재구성은 보조.
- 기존 `active_mesh` **스로틀 캐시는 유지** (동일 점수+뷰 = 0워크는 이미 확보).

---

## 2. 현재 (baseline) — 스로틀 재구성의 점당 비용

| 항목 | 점당 비용 |
|---|---|
| `halves_for_stroke` (locked) | ~1 곱 + max (소폭) |
| 어댑터 `map→collect Vec<StrokePoint>` ×2 | 할당 2 + 필드 복사 |
| `stroke_space` (lens/speeds/us) | sqrt 2·div 1 + Vec 3 |
| 밀도 `density`×2 → `ink_field` → `value_noise` 2회/채널 | **해시 ~16회/점** + exp/powi |
| 리본 `stroke_ribbon_lr` + egui 변환 | sqrt·분할 + Vec 3 + 변환 |
| **합계** | **한 재구성 ≈ 5~7 O(n) 패스 + 힙 할당 ≥7개** |

---

## 3. 예상 절감 (추정 범위 — P0로 확정)

| 단계 | 할 일 | 전체 재구성 CPU | 그리기 성장 경로 |
|---|---|---|---|
| **P1** | frontier `tail()` 증분 append + `active_stroke` 클론 제거 | (전체는 유지) | **85~98%↓** — O(n) 전체 → O(k), 매 프레임 알파는 young window O(S) |
| **P3-퓨즈** | halves+알파+리본 1패스 + 용량 reserve + 스크래치 재사용, 할당 7→1~2 | **30~50%↓** · 할당 **80~90%↓** | 동일 |
| **P3-근사/비트** | 노이즈 타일(해시 제거)·`sat_at`/`combine_saturation`/`effective_pressure` LUT·절대 호 길이 그레인 | **추가 20~40%↓** (단일 레버 = 점당 ~16 해시 제거) | 동일 |
| **합산 (P1+P2+P3)** | | **50~65%↓** | **90~99%↓** |

핵심 요약 추정:
- 점당 해시/할당 제거로 **전체 재구성 CPU 50~65%** 감소.
- 할당 수 **≥7개 → 1~2개** (약 **80~90%** 감소).
- 상호작용(그리기) 중 프레임 단위 재계산 **O(n) → O(k)** 로 **90% 이상** 수렴.
- 단일 최대 레버 = **`value_noise` 점당 ~16회 해시 → 타일 인덱싱**
  (노이즈가 재구성 CPU의 약 40~55% 차지 가정).

---

## 4. 벤치 표 (baseline은 P0 실측 — debug 빌드, release는 더 빠름)

baseline은 이 문서용으로 추가한 `#[ignore]` 벤치 `bench_live_render_cost_per_point`
(`cargo test -p freedf-canvas -- --ignored --nocapture bench_live_render`)로 측정한
실측치입니다 — `CoreRibbonMesher::append_stroke` = halves + alphas(그레인/노이즈 포함) + 리본 전체 경로.

| 시나리오 | baseline(실측) | 동작보존 후(실측) | P3 근사(추정) | 예상 비율 |
|---|---|---|---|---|
| N=1k 점 meshing (전체 재구성) | 560 µs / 560 ns/pt | **548 µs (−2.1%)** | ~225 µs | ≈ **−60%** |
| N=10k 점 meshing | 5.93 ms / 593 ns/pt | **5.59 ms (−5.7%)** | ~2.4 ms | ≈ **−60%** |
| 성장 k=3 (n=300 중) | ~168 µs (환산) | ~164 µs (환산) | ~1.7 µs (tail 3점) | ≈ **−99%** |
| 재구성당 힙 할당 | 7+ | ~6 (메시 예약) | 1~2 | **−80~90%** |

> n=1k·10k는 정밀도가 좋아 실측값 그대로. 성장 행은 ns/pt × N 환산입니다.
> **동작보존 후(실측)** = `density_lr`(좌우 x-파트 공유) + 메시 용량 예약. 같은 입력이
> 정확히 같은 출력(parity 테스트 `density_lr_matches_two_density_calls`로 고정) — **−2~6%**.
> **P3 근사(추정)** = 노이즈 타일·LUT·단일 패스·비트 플래그 — 출력이 달라지는(시각) 근사라
> GUI 검증이 필요해 **아직 미구현**. 구현 후 동일 벤치로 이 표를 교체합니다.
> debug 프로파일이라 절대값이 크며, 앱(release)에선 상수배 줄어듭니다.

---

## 5. 안전망 (수치를 믿을 수 있게)

- **결정성**: 같은 점열 → 항상 같은 출력 (`value_noise_is_deterministic`,
  `no_popping`, `stroke_ink_lr_*` 테스트 유지).
- **렌더==커밋(WYSIWYG)**: live와 굽기(`CoreRibbonMesher`/`halves_for_stroke`
  `/`alphas_for_stroke`)가 **같은 모델을 공유** → 근사는 굽기 경로와 함께 교체하고 parity 테스트로 고정.
- **성능 회귀 가드**: P3 후에도 단일 패스·할당 수가 고정되도록 벤치를 커밋해 보호.

---

## 6. 구현 단계

- **P0** 계측: `#[ignore]` 벤치(`N=1k/10k`)로 ns/point·할당 수 기록 → baseline.
- **P1** frontier `tail()` 증분 append + `active_stroke` 클론 제거 (가장 큰 프레임 이득).
- **P2** 절대 호 길이 그레인 — u-정규화 의존 제거 → 알파도 O(k) 증분.
- **P3** 노이즈 타일·LUT·단일 패스·스크래치·비트 플래그 → 점당 상수 축소.
  - *(부분 구현)* **`InkGrain.fast_noise`**(기본 false) 토글 — 켜면 고주파 위킹 옥타브를
    생략해 질감 계산을 절반으로 줄입니다. **Debug HUD 체크박스**로 라이브 비교 가능.
    결정성 · 범위(0.30..1.60) · no-popping 테스트로 보호. 어떤 게 자연스러운지 눈으로
    보고 기본값을 정하세요.
- **P4** 굽기/young overlay 동일화 + (선택) input 1€ 필터 `dt→alpha` 정리.
