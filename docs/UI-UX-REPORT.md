# FreeDF UI/UX 개선 심층 보고서

> **상태**: 분석 전용 보고서 — 코드는 수정하지 않음
> **대상 버전**: v0.1.0 (`crates/freedf` · eframe/egui 0.36 · pdfium-render 0.9.3)
> **작성일**: 2026-09-13
> **범위**: 툴바 계층화 · 정보 그룹핑 · 통합 디버그 패널(진단) 확충

---

## 1. 요약 (Executive Summary)

FreeDF는 캔버스·잉크·도구 기능이 매우 풍부하지만, **UI 표면이 기능 성장 속도를
따라가지 못하고 있는 상태**입니다. 핵심 문제 세 가지를 진단했습니다.

| # | 문제 | 심각도 | 핵심 근거 (코드) |
|---|------|:---:|------|
| 1 | **툴바 1층(Row1) 과밀** — 한 줄에 약 22개 컨트롤이 상주해 계층이 없음 | 🔴 상 | `toolbar/rows.rs::row_top` |
| 2 | **정보 그룹핑 혼선** — 기능이 "자기가 속한 곳"이 아닌 임시 위치에 노출됨 (예: Fast ink noise가 Debug HUD에 기생) | 🔴 상 | `canvas/paint.rs::debug_pen_section` |
| 3 | **디버그 패널이 '입력 튜닝 용도'로만 잘림** — 버전/OS/시스템/진단 내보내기 부재 | 🟠 중 | `app/gamepad.rs::debug_hud_ui` |

가장 큰 구조적 결함은 **문제 1(툴바 계층)** 입니다. 툴바는 "모든 기능의 최상위
진입점" 역할을 해야 하는데, 현재는 기능이 늘어날 때마다 **한 줄에 아이콘을 하나씩
더 붙이는 방식**으로 성장해 "되돌리기/패널 토글/설정/서버/정렬"이 한 줄에 뒤섞여
있습니다.

> **권장 방향**: 툴바를 **그룹 + 오버플로 + 요약 위젯(Settings 다이얼로그 통합)**
> 구조로 재편하고, "토글 스위치" 성격의 설정은 툴바에서 빼서 설정 다이얼로그로
> 옮기며, 디버그 HUD는 **종합 진단(About + 진단 내보내기)** 으로 승격시킵니다.

---

## 2. 현황 진단 (As-Is 분석)

### 2.1 툴바 구조 — 상단 3개 상주 행 + 검색 행

`toolbar/mod.rs::toolbar()`는 `Panel::top("toolbar")` 안에 Row1~Row4를 아래로 쌓습니다.

```
┌──────────────────────────────────────────────────────────────────────────┐
│ Row1 row_top   → 패널/기능 토글 + 명령 + 정렬 + 저장 + 디버그 … (과밀)    │
│ Row2 row_pages → 페이지/캔버스/종이/리프레시                              │
│ Row3 row_tools → 도구 피커/색/두께/스무딩                                 │
│ Row4 search    → Ctrl+F 시에만                                            │
└──────────────────────────────────────────────────────────────────────────┘
```

**Row1(`rows.rs::row_top`) 현재 컨트롤 인벤토리 (왼→오)**

| 구분 | 컨트롤 | 종류 |
|------|--------|------|
| 창 | Hide UI | 명령 |
| 창 | Window Focus | 토글(라벨 버튼) |
| 패널 | Library · Outline · Bookmarks | 패널 토글 3 |
| 패널 | Palette | 패널/기능 토글 |
| 기능 | Dictionary | 기능 토글 |
| 서버 | Media Server | 설정창 열기 토글 |
| 패널 | Media | 패널 토글 |
| 기능 | Macro | 설정창 토글 |
| 입력 | Gamepad | 설정창 토글 |
| 유지보수 | Cache | 메뉴(오버플로 아님) |
| 정렬 | Align Left / Center / Right | 정렬 선택 3 |
| 편집 | Undo · Redo · Clear Page | 명령 3 |
| 저장 | Save Edits · Load Edits | 명령 2 |
| 진단 | Debug HUD | 토글 |

→ **약 22개 시각 요소**가 구분자(separator) 3~4개와 함께 **1행**에 상주.

**Row3(`row_tools`)** 는 도구(스펙트럼) 컨트롤 — 도구 피커·미니 스트로크 미리보기·
폭 슬라이더·색·스무딩·필압 토글 등 —이 매우 길고, **전체가 상주**라 가로 폭을 크게
차지합니다. Row2도 페이지/캔버스/종이/리프레시가 섞여 있습니다.

### 2.2 설정 창이 ~11개로 분산 (모달 확산)

`settings.rs::settings_windows()`가 각각 독립된 플로팅 창을 띄웁니다.

| 창 | 너비 | 리사이즈 | 스크롤 |
|----|:---:|:---:|:---:|
| Ballpen / Fountain pen settings | 400 | ✅ | ✅ |
| Cursor settings | 320 | ❌ | ❌ |
| Paper settings | 400 | ✅ | ✅ |
| Canvas settings | 400 | ❌ | ❌ |
| Color wheel settings | 400 | ❌ | ❌ |
| Edge auto-scroll | 400 | ❌ | ❌ |
| Window Focus | 400 | ❌ | ❌ |
| Insert pages | 400 | ❌ | ❌ |
| Media server | 440 | ❌ | ❌ |
| Macro settings | 480 | ❌ | ❌ |
| Gamepad settings | 400 | ❌ | ❌ |

총 **11개**의 독립 모달입니다. 이들은:

- 툴바의 각 단일 버튼/토글에 1:1로 매핑되어 **툴바 길이를 키우는 원인**이 됨
- 서로 **탭/그룹으로 묶이지 않아** "어디서 무슨 설정을 열었는지" 인지 부담 증가

### 2.3 Debug HUD — 진단이 '입력 보정 도구'로만 쓰임

`gamepad.rs::debug_hud_ui()`는 **한 개의 창**에 두 섹션만 있습니다:

```
Debug HUD 창
├── Pen / Canvas   ← paint.rs::debug_pen_section()  (필압/틸트/속도/폭 + Fast ink noise 토글)
└── Gamepad        ← gamepad_debug_section()          (축/버튼/액션 카운터/이벤트 로그)
```

**부재 항목**:
- **소프트웨어 버전** — `env!("CARGO_PKG_VERSION")` 사용처가 없음
- **OS / 아키텍처** — `std::env::consts::{OS, ARCH}` 및 Windows 빌드 버전 수집 없음
- **시스템(CPU/메모리/디스크/GPU)** — `sysinfo` 크레이트 미도입
- **렌더러/백엔드 정보** — `glow`/`wgpu` 선택, 디스플레이 Hz(`refresh_hz`는 Row2에 존재)
- **DB/서버 연결 상태**, **앱 데이터 경로**, **저장소 크기**, **로그/진단 내보내기** 부재

### 2.4 그룹핑 혼선의 대표 사례 — "Fast ink noise"

- **실제 정의**: `ink.rs::InkGrain::fast_noise` — 펜/만년필 잉크 질감의 시각 근사 토글
- **노출 위치**: `canvas/paint.rs::debug_pen_section()` — **Debug HUD 안의 "Pen / Canvas"
  섹션**에 체크박스로 존재
- **본질**: 이 값은 **펜/만년필 설정(도구)의 잉크 질감 옵션**이지 진단용이 아님
- **영향**:
  - 평소 **Debug HUD를 끄면 이 설정을 열기 위해 HUD를 켜야** 하는 비직관적 워크플로
  - `fast_ink_noise`가 **App 필드 + pen_grain/fountain_grain 두 곳에 동시 반영**되는 전파
    로직이 분산되어 "도구 설정" 관점에서 한 곳에 모아야 관리가 쉬움
  - 사용자가 "이게 뭔데?" 하고 **설정 창을 찾아도 없음** → 기능 발견성 저하

> **패턴 정리**: "임시 장소 + 성능 토글"이 디버그 HUD에 쌓이고, 설정 창은 11개로
> 분산되면서 **"무엇이 어디 숨는지"를 사용자가 추론할 수 없게** 됩니다.

---

## 3. 문제 심층 분석 (Root-Cause)

### 3.1 계층 부재로 인한 툴바 과밀 — 핵심 문제인 이유

한 줄에 22개 아이콘이 문제인 이유는:

1. **인지 부하 과다** — Miller의 7±2 규칙상 단일 그룹 대상이 아니며, 구분자 4개로는
   5개 미만의 하위 그룹으로만 나뉘어 여전히 식별 부담이 큼.
2. **가로 폭 압박** — 아이콘 단추 + 간격당 약 28~40px, 22개면 **~700~900px**에 달해
   11인치 타블릿(1366px)에서 첫 행만으로도 화면 일부를 점유.
3. **기능 상태 확인 비용 증대** — 활성 여부가 토글 하이라이트로만 표시되어,
   다중 토글(패널/기능/설정/진단)이 동시에 존재하면 **무슨 토글이 켜졌는지** 확인이 어려움.
4. **확장성(fit) 없음** — 새 기능이 늘면 그룹핑/정렬 없이 아이콘이 계속 쌓이며,
   향후 기능 추가 시 문제가 지속·심화.

### 3.2 "표시 토글 / 기능 토글 / 명령 / 설정 진입" 구분 부재

현 Row1에는 성격이 다른 4유형이 혼합:

- **표시 토글**: Library / Outline / Bookmarks / Palette / Media / Debug HUD
- **기능 토글(간접)**: Dictionary / Media Server / Macro / Gamepad
- **명령(즉시 실행)**: Hide UI / Undo / Redo / Clear Page / Save / Load / 정렬
- **설정 창 입구**: Window Focus / Cache / (기타)

이들은 UI 의미가 서로 다른데, 전부 **동일한 아이콘 버튼 규격 + 선택 하이라이트**로
렌더되어 "클릭했을 때 무슨 일이 일어날지"를 예측하기 어렵습니다.

### 3.3 진단 도구(디버그)와 성능/미학 설정이 뒤섞임

"입력이 실제로 어떻게 들어오나"는 **진단** 목적이고, "질감 옥타브를 빼서 약 2배 빠르게"는
**성능 튜닝/미학** 목적입니다. 두 목적의 성격이 다른데, 현재 **같은 화면(Debug HUD)
같은 섹션**에서 관리되다 보니:

- 진단을 끄면 성능 토글 접근 경로 자체가 사라짐
- 성능/미학 토글은 진단과 **상호 배타적이지 않아** "환경은 유지하되 진단만 끈다"는
  선택지가 없음

### 3.4 종합 진단·시스템 정보 기능의 부재 원인

- DB·미디어·게임패드·매크로 등 기능은 계속 늘었는데, **앱 전반의 "About/시스템 정보"
  집결점이 없음**
- 버전 상수(`CARGO_PKG_VERSION`)는 정의되어 있으나 앱 UI에서 렌링하지 않음
---

## 4. 해결 방안 (To-Be 설계)

### 4.1 [최우선] 툴바 계층 재편

#### 원칙 (계층 모델)

```
[그룹1] 상태/표시   [그룹2] 명령     [그룹3] 도구·색   [그룹4] 설정   [⋯] 오버플로
 (패널·기능 토글)   (편집/저장)      (캔버스)         (Settings)      (그 외)
```

**목표**: 툴바에서 직접 노출할 것은 **자주 쓰는 것**(도구 / 색 / 폭 / Undo·Redo / 툴
패널 토글 3~5)만 남기고, 나머지는 "설정" 그룹 또는 **오버플로 메뉴(⋯)** 안으로.

**구체 방안 (3안 대비)**

| 안 | 개요 | 장점 | 단점 | 권장 |
|----|------|------|------|:---:|
| **A. 오버플로 + 그룹핑** | Row1을 표시/명령/설정 그룹으로 나누고 초과분을 `⋯` 메뉴(오른쪽 끝)로 | 구현 비용 낮음, 현재 구조 유지 | 오버플로는 발견성이 낮음 | ✅ 초기 단계 |
| **B. 설정 다이얼로그 통합(탭)** | 11개 모달 → 1개 "Settings" 창의 탭으로 | 개념 전면 정돈, 계층 완벽 | 리팩토링 범위 큼 | ★ 최종 |
| **C. 렌더 스트립 개편** | 메뉴·도크·기본 탐색 바 구조(GoodNotes/Notability식) | 가장 익숙한 UX | 가장 큰 전면 리워크 | 후기 로드맵 |

> **권장 로드**: ① 4.1-A로 즉시 부담 완화 → ② 4.1-B로 11개 모달을 단일 설정
> 다이얼로그로 통합 → ③ B가 안정되면 4.4 디버그 패널과 함께 C(렌더 스트립) 추진.

#### 4.1-A 즉시 적용 가능한 Row1 재배치

```
BEFORE (약 22개 1행 상주):
 HideUI │ WinFocus │ Lib Out BM Pal │ Dict Srv Med Mac Gamepad │ Cache │ L/C/R │
 Undo Redo Clear │ Save Load │ DebugHUD

AFTER (그룹핑 + 오버플로):
 [패널] Lib Out BM Pal │ [명령] Undo Redo (⋯ HideUI Clear Save Load) │ [기능] ⋯ │ ⚙설정
```

핵심 변경:

1. **"설정 창 열기" 진입점을 툴바 상단에서 제거** — Dictionary / Media Server / Macro /
   Gamepad / Window Focus / Cache의 상태 토글은 유지하되, **세부 진입은 "⚙ 설정" 그룹·다이얼로그**로.
2. **정렬(L/C/R)은 캔버스 화면 전환 메뉴 또는 우클릭 메뉴로 이동**해 매 행 상주 중단.
3. **명령과 표시 토글을 시각적으로 구분** — 명령은 solid 아이콘, 표시 토글은 음영
   토글(색으로 켜짐 표시)로 렌더.
4. **초과분은 `⋯` 오버플로 + 호버 시 드롭다운**으로.
- 시스템 자동 수집 모듈이 없어 "버그 리포트올 아넬 때 사용자가 일일이 정보를 복사해야 함
#### 4.1-B 설정 다이얼로그 통합 (최종 설계)

11개 모달 → **단일 "Settings" 창 + 좌측 그룹/탭**:

```
Settings
├─ ✏ 그리기(Drawing)   Ballpen · Fountain · Highlighter · Cursor
│                        질감/Fast ink noise 를 "Ink grain"로 이동 · 필압 곡선
├─ 종이(Paper)           종이 스타일 · 캔버스 색 · 줄/격자 색
├─ 페이지(Page)          Insert/Delete 기본값 · Rotate 기본
├─ 색(Color)             Color wheel 팔레트 관리
├─ 입력(Input)           Smoothing · edge auto-scroll · window focus · 펜 틸트/필압
├─ 게임패드(Gamepad)     연결 · 축 매핑
├─ 매크로(Macro)         단축키 · 데스크톱 전환
├─ 서버(Server)          API 주소 · 키 · 테스트/상태
└─ 정보(About)           버전 · OS · 시스템 · 진단 내보내기 ← 4.4와 연동
```

효과:

- 툴바 "설정" 버튼(GEAR) **1개**로 축소
- 사용자 개념 지도("기능이 어디 있는지") 단순화
- 대부분 기존 `form::fieldset` 컴포넌트 재사용 → **구현 비용 낮음**

### 4.2 [그룹핑 혼선 해결] 성능·미학 토글의 소유권 재배치

| 토글/설정 | 현재 위치 | 이동 대상 | 이유 |
|-----------|----------|-----------|------|
| **Fast ink noise** | Debug HUD `debug_pen_section` | **펜/만년필 설정 창 "Ink grain" 필드셋** | 도구별 질감 옵션 — 진단이 아님 |
| **refresh Hz** | Row2 직접 콤보 | **설정 → 입력/표시(디스플레이)** | 하드웨어/디스플레이 설정이지 툴바 단일 조작 대상 아님 |
| **필압/틸트 소스** | Debug HUD에만 노출 | **설정 → 입력**(상태 표시) + HUD 알람 유지 | 원인 파악은 진단, 조정은 설정 |

**원칙**: "값을 관찰하는 것"은 **진단(HUD)**, "값을 바꾸는 것"은 **설정창**에서.
- HUD = 모니터링/진단 전용 (변경 불필요한 실시간 수치 + 타임스탬프)
- 설정창 = 변경 전용 (성능·미학·입력 옵션의 유일한 제어자)

### 4.3 [디버그 패널 확충] 종합 진단 → "정보(About) + 진단 HUD"

`debug_hud_ui`의 두 섹션을 **네 개 섹션**으로 재구성:

| 섹션 | 내용 | 소스 |
|------|------|------|
| **입력(Input)** | 기존 Pen/Canvas + Gamepad (실시간 수치, 이벤트 로그) | `debug_pen_section`, `gamepad_debug_section` |
| **성능(Perf)** | FPS(`unstable_dt`), 프레임 수, 스트로크 점수·렌더 비용, refresh Hz | ctx.input, active_stroke |
| **연결/상태(Health)** | DB/서버 연결, 미디어 서버, 사전 캐시, 저장 중 상태 | server.rs, actions/cache.rs |
| **정보(About: System)** | 버전·OS·하드웨어·렌더러 + 진단 내보내기 | 신규 시스템 모듈(4.4) |
### 4.4 시스템/버전 정보 모듈 (신규 구현 안)

**소프트웨어 버전**
- `env!("CARGO_PKG_VERSION")`·`env!("CARGO_PKG_NAME")`를 **한 곳**에서 조회해 HUD + About 공용
- 빌드 커밋은 빌드 시 `git rev-parse` 주입(예: .cargo 환경변수)으로 제공

**OS/아키텍처**
- `std::env::consts::{OS, ARCH}` + 빌드 아키텍처
- Windows: `windows` crate의 `GetVersionExW`/`RtlGetVersion`으로 빌드 버전 제공
  (`Win32_System_SystemInformation` 기능을 추가해 작은 비용으로)
- Linux: `/etc/os-release` 파싱(std 라이브러리만으로 커버 가능)

**하드웨어/시스템**
- `sysinfo` 크레이트(순수 Rust, 경량) 사용: CPU(모델/코어/사용률/속도),
  메모리(전체/사용/가용), 디스크(앱 데이터 폴더 사용량), 네트워크 정보
- 렌더러: `glow`/`wgpu` 중 어떤 백엔드인지(eframe `Renderer`,`FREEDF_RENDERER` 변수 존재) 표시
- egui `CtxInfo`(스케일/텍스처 크기), 모니터 해상도·Hz

**진단 내보내기(Export)**
- HUD / About 창에 **"Copy diagnostics" / "Export .txt"** 버튼 추가
- 기존 `freedf_pendebug.log`, 게임패드 로그(`gamepad_log_snapshot()`)와 진단 블록
  (버전/OS/하드웨어/렌더러/연결/설정 요약)을 결합해 클립보드/파일로 내보냄
- "버그 리포트에 필요한 정보가 1-클릭으로 동반"되도록 경험 단일화

### 4.5 UI/UX 미세 개선

- **툴바 높이 최소화**: 행 높이를 일관되게, 행 사이 구분선 간격 축소
- **접근성**: `accesskit` feature 활용 — 아이콘 버튼 툴팁 + 키보드 네비게이션 점검
- **토글 상태 단일화**: 설정 창과 HUD에서 같은 토글(`debug_hud` 등)이 항상 동일
  하이라이트로 표시되도록 `..._open` 필드 흐름 정리
- **상태 바(하단)**: 연결 상태·저장 상태·현채 도구를 하단 요약에 표시
---

## 5. 실행 로드맵 (단계별 계획)

| 단계 | 범위 | 예상 효과 | 리소스 |
|:---:|------|----------|:---:|
| **P0 (즉시)** | Row1을 "명령/표시토글/설정" 그룹 + `⋯` 오버플로로 재배치, 정렬 아이콘 그룹 이동 | 행 길이 약 35% 축소 | 낮 |
| **P1** | Fast ink noise를 펜/만년필 "Ink grain"으로 이동, HUD는 실시간 값만 | 발견성·직관성 개선 | 낮 |
| **P2** | 11개 모달 → 단일 "Settings" 탭 통합(4.1-B) | 개념 단순화, 툴바 버튼 11개 축소 | 중 |
| **P3** | 디버그 HUD 4섹션 확충 + About:System 모듈(`sysinfo`) + 진단 내보내기 | 종합 진단 강화 | 중 |
| **P4** | 상태 바 + 렌더 스트립 개편(레이아웃 리워크) | 시각 정돈/만족 | 중~대 |

**의존성**: P3의 시스템 모듈은 P2(설정 About 탭)와 결합해 배치. HW 크레이트는
**optional feature**로 두어 "초경량" 원칙 유지.

---

## 6. 성공 지표 (Success Metrics)

- **툴바 행 길이**: Row1 시각 요소 수 **22 → 9 이하** (그룹 + 오버플로 후 상주 기준)
- **설정 진입 경로**: 11개 모달 → **1개 설정 창**, 툴바의 설정 진입 버튼 **2개(도구 설정 +
  전체 설정) 이하**
- **기능 발견성**: "Fast ink noise를 찾는 시간" 목표 1분 이내(현재는 Debug HUD를 켜야 접근)
- **진단 내보내기**: 버그 리포트에 필요한 정보를 1-클릭으로 동반 가능
- **회귀 보호**: 개편 후 실시간 입력/스트로크 프레임 속도 회손 없음 — 기존 결정성/벤치
  (`docs/OPTIMIZATION.md`) 테스트 유지

---

## 7. 부록 — 조사된 코드 참조

| 항목 | 위치 |
|------|------|
| 툴바 행 렌더 (Row1 과밀 원인) | `crates/freedf/src/app/toolbar/rows.rs::row_top` |
| 툴바 행 조립 | `crates/freedf/src/app/toolbar/mod.rs::toolbar` |
| 설정 모달 11개 | `crates/freedf/src/app/toolbar/settings.rs::settings_windows` |
| Fast ink noise (잘못된 위치) | `crates/freedf/src/app/canvas/paint.rs::debug_pen_section` |
| InkGrain::fast_noise 정의 | `crates/freedf-core/src/ink.rs` |
| 디버그 HUD 창 (2섹션) | `crates/freedf/src/app/gamepad.rs::debug_hud_ui` |
| 디버그 필드 | `crates/freedf/src/app/mod.rs` (`debug_hud`, `fast_ink_noise`) |
| 펜/만년필 도구 설정 | `crates/freedf/src/app/toolbar/settings.rs::pen_settings_ui` 등 |
| 버전 상수 | `Cargo.toml` (v0.1.0) — 현재 UI 미사용 |
| 게임패드 디버그 로그 | `crates/freedf/src/app/gamepad.rs::gamepad_debug_section` |