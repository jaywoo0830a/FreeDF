# FreeDF UI 시스템 — 토큰 · 키트 · 접근성 계약 · 갤러리

이 문서는 FreeDF의 UI를 **일관되게(design)**, **접근 가능하게(a11y)**,
**검증 가능하게(testability)** 만드는 4개 계층을 설명합니다.

```
tokens.rs   ── 숫자는 여기에만 있다 (간격·반지름·글자·획·타깃·상태색)
   ↓
a11y.rs     ── 모든 컴포넌트가 통과해야 하는 계약 (Role/Spec/Issue + finish)
   ↓
kit.rs      ── 계약을 통과한 컴포넌트 (Button · IconButton · Row · Toggle · Segmented)
   ↓
gallery.rs  ── 컴포넌트를 모든 변형/상태로 한 화면에 렌더 (시각 리뷰 + 자동 스캔)
```

## 1. 토큰 — `crates/freedf/src/ui/tokens.rs`

UI 코드에 매직 넘버를 쓰지 않습니다. 값은 토큰에서만 나옵니다.

| 토큰 | 뜻 | 값 |
|---|---|---|
| `target::MIN` | 클릭 가능한 최소 타깃 | 1.5rem (24px) |
| `target::COMFORT` | 기본(마우스) 타깃 하한 | 1.75rem (28px) |
| `target::TOUCH` | 터치/펜 타깃 하한 | 2rem (32px) |
| `scale::S_*` | **×1.5 모듈러 정거장** — S_12 → S_16 → S_24 → S_36 → S_52 → S_80 → S_120 → S_180 → S_272 → S_408 | rem 기반 (0.25rem 스냅) |
| `space::*` | 2 · 4 · 8 · 12 · 16 간격 램프 (rem 표현) | `XS`…`XL` |
| `margin::*` | 프레임 내부 마진 — BADGE/KBD/CARD/DIALOG/CONTAINER/CHIP/OVERLAY | — |
| `radius::*` | 0.25/0.375/0.5rem 반지름 | `SM/MD/LG` |
| `font::*` | SMALL/BODY/TITLE/HEADING 크기 | — |
| `stroke::*` | HAIRLINE/NORMAL/EMPHASIS | — |
| `State` | enabled/hovered/active/selected/disabled 판정 헬퍼 | — |
| `expand_to_target(rect, min)` | 보이는 크기는 그대로, **클릭 영역만** 키움 | — |

**치수 규약**: 컴포넌트 실제 크기는 `scale`의 모듈러 정거장(0.75rem에서 ×1.5,
0.25rem 스냅) 중 `target` 하한을 만족하는 가장 작은 값. 표준 컨트롤 높이는
S_36(2.25rem) — 행·툴바·버튼·아이콘 버튼이 모두 같은 리듬을 공유합니다
(`smoketest/30_more_menu.luau`가 행 높이 균일을 회귀 검증).

`expand_to_target`이 중요합니다: 아이콘이 16px이어도 타깃은 `target::MIN` 이상이
됩니다(보이는 크기와 누를 수 있는 크기의 분리).

## 2. 접근성 계약 — `crates/freedf/src/ui/a11y.rs`

컴포넌트는 그리기 직전에 [`Spec`]을 만들고, 응답을 [`finish`]로 감쌉니다.

```rust
let resp = ui.add(egui::Button::new("Clear"));
let resp = a11y::finish(ui, Spec::button("menu.maintenance.clear", "Clear cache"), resp);
```

`finish`가 **한 번에** 하는 일:

1. **최소 타깃 보장** — `interact_rect`가 `target::MIN`보다 작으면 넓힘.
2. **역할/이름/상태 보고** — `role`(button/icon_button/toggle/radio/row/segment…),
   접근성 이름, `selected`/`value`.
3. **계측(id) 등록** — `dev-automation` 빌드에서 `dev::tag_*` 호출이 여기에
   **내장**됩니다. 호출부가 따로 태그하지 않으므로 \"계측을 빠뜨린 화면\"이
   생길 수 없습니다.
4. **계약 위반 수집** — 타깃 미달, 이름 없음, 중복 id 등은 [`issues()`]에 쌓이고
   갤러리 화면에 빨간 글씨로 표시됩니다.

### 규칙

* **한 프레임에 같은 id를 두 번 등록하지 마세요** — eguidev가 계측 결함으로 잡고
  자동화를 멈춥니다(`a11y`가 중복도 위반으로 수집).
* 라벨은 바꿔도 **id는 유지**합니다(자동화 스크립트가 의존하는 공개 계약).
  고정 목록은 [`docs/eguidev-automation.md`](eguidev-automation.md).
* 아이콘만 보이는 컨트롤은 **반드시 이름을 지정**하세요
  (`IconButton::new(icon, "Find")`, `Segment::icon_only(id, "Align left", icon)`).
* 테스트는 `a11y::assert_clean()`(테스트 빌드 전용)으로 위반 0을 강제합니다.

## 3. 키트 — `crates/freedf/src/ui/kit.rs`

| 컴포넌트 | 쓰는 곳 | 계약 |
|---|---|---|
| `Button` | 일반 명령 | `primary`(주 행동, 화면당 1개) · `secondary` · `ghost` · `danger` · `selected(bool)` · `enabled(bool)` · `icon(..)` · `size(Size)` |
| `IconButton` | 아이콘 전용 | 이름 필수, 28pt 기본 |
| `Row` | 메뉴/목록 행 | `action` · `toggle` · `radio` · `trailing_width(..)`+`show(ui, |ui| …)` · `enabled(bool)` — 행 전체가 타깃, 좌측 아이콘 레일 공유 |
| `Toggle` | 인라인 스위치 | 값은 `&mut bool`로만 (상태 반영이 자동으로 참이 됨) |
| `Segmented` + `Segment` | 배타 선택 | `icon_only(id, name, icon)`로 좁은 슬롯에 아이콘만 — 툴팁/이름은 유지 |
| `section_label` | 섹션 구분 | — |
| `segment_group_width(n)` | n개 아이콘 세그먼트가 차지할 **정확한 폭** | 트레일링 슬롯 예약용 |

### 크기 등급

`Size::Small ≥ 24` · `Size::Medium ≥ 28` · `Size::Touch ≥ 32`.
등급 검증은 헤드리스 테스트와 `smoketest/40_ui_gallery.luau`가 함께 합니다.

### 좁은 슬롯 규칙 (실측으로 얻은 교훈)

`Row`의 트레일링 슬롯 폭은 `trailing_width(..)`로 **예약**합니다. 슬롯 안의
컴포넌트가 예약 폭을 넘기면 egui 팝업이 그만큼 **커지고**, 그 뒤의 행들이 넓어진
폭을 따라가 레이아웃이 연쇄로 어긋납니다(실측: 정렬 세그먼트가 라벨 때문에
91~114px로 부풀어 팝업이 304 → 522px로 커짐). 그래서 좁은 슬롯에는
`Segment::icon_only` + `segment_group_width(n)`을 쓰세요.

## 4. 갤러리 — `crates/freedf/src/ui/gallery.rs`

`dev-automation` 빌드에서만 존재합니다(제품 UI 아님). 여는 방법:

```bash
# 자동화/CI (테스트 훅) — 시작할 때 갤러리가 열립니다
FREEDF_UI_GALLERY=1 cargo run -p freedf --features dev-automation

# 사람이 보는 경로: More 메뉴 → Dev → "UI gallery"
```

### 왜 환경변수 경로가 따로 있나

메뉴 항목은 egui **팝업** 안에 있습니다. 실측 결과 팝업 내부 위젯은
eguidev가 interaction-ready로 판정하지 않아 자동화 클릭 대상이 **아닙니다**
(팝업 밖 위젯은 정상). 그래서 계약 스캔은 `FREEDF_UI_GALLERY=1`로 창을 직접
열고, 메뉴 항목은 사람용 입구로 남겨 둡니다.

## 5. 검증 (한 명령, 순차 실행)

```bash
./scripts/test-all.sh          # 헤드리스 계약 → 스모크 → 갤러리 스캔 → 기본 프로필 컴파일
./scripts/test-all.sh --verbose
```

| 단계 | 무엇을 | 어디서 |
|---|---|---|
| 1 | `kit`/`a11y`/토큰 계약 (76개) | `cargo test -p freedf --features dev-automation --bin freedf` |
| 2 | 실제 앱 회귀 (런치·도구·More 메뉴·갤러리) | `scripts/edev-run.sh smoke` |
| 3 | 갤러리 계약 스캔 + 스크린샷 | `scripts/ui-gallery-check.sh` |
| 4 | 기본 프로필 컴파일(기능 꺼짐) | `cargo check -p freedf` |

**순차 실행은 강제입니다.** `edev`는 실제 창/디스플레이를 쓰므로 동시 실행이
서로를 깨뜨립니다. `scripts/edev-run.sh`가 `flock`으로 직렬화하므로 어떤 호출
순서로도 안전합니다.

시각 리뷰는 `tmp/eguidev-screenshots/`에 떨어진 JPEG를 직접 읽어서 합니다
(추측 금지, 캡처 근거 — `.clinerules/eguidev-visual-review.md`).

## 6. 남은 마이그레이션 (후속 작업)

| 대상 | 현황 | 다음 단계 |
|---|---|---|
| `More` 오버레이 (`ribbon::overflow_menu`) | ✅ `kit` 완전 이관 | — |
| 갤러리 · 계약 | ✅ `kit` + `a11y::finish` | — |
| 툴바 아이콘 버튼 (`ribbon`의 `icon_button`/`icon_toggle`/`icon_label`, Row 1~3) | ⚠️ 원시 `ui::icon_*` + `dev::tag_*` 수동 계측 | `kit::IconButton`/`Toggle`/`Button`으로 이관 (계약 자동 적용) |
| `ui/buttons.rs` · `ui/components.rs` · `ui/form.rs` | ⚠️ 평행 API 잔존 | 호출부를 `kit`으로 옮긴 뒤 삭제 |

이관 전에도 **계약은 유지**됩니다: 원시 컴포넌트는 `dev::tag_*`로 계측하고
`smoketest/10_launch.luau`가 존재·타깃을 검증합니다.

