# FreeDF UI 컴포넌트 아키텍처 (React/Bootstrap 스타일)

> 목적: 툴바와 UI 전반을 **작은 재사용 컴포넌트**로 조립하는 패턴 확립.
> egui는 Immediate-mode라 React처럼 "가상돔"은 없지만, 아래 원칙이면 같은 이점을 얻습니다:
> **프레젠테이션(props) ↔ 컨테이너(상태 연결) 분리 + 작은 원자 재사용.**

---

## 1. 계층 개념 (Component ↔ React/Bootstrap)

| FreeDF | React | Bootstrap | 역할 |
|--------|-------|-----------|------|
| `crate::ui::layout` | Layout primitives | `.d-flex`, grid, spacer | 배치(배열) 전용, 상태 무관 |
| `crate::ui::components` | Atoms | `.badge`/`.form-text` 등 | 표시용 원자(경량) |
| `crate::ui::ds` | Design-system elements | `.badge`/`.alert`/`.card`/`<kbd>` | 세마틱 톤 기반 요소 |
| `crate::ui::form` | Form controls | `.form-*` | 데이터 입력 빌더(props + 결과) |
| `crate::ui::{icon_button..}` | <Button> | `.btn` | 툴바/버튼 원자 |
| `app::toolbar::rows::*` | Containers | `<Page>` | 상태를 props에 연결해 조립 |
| `app::FreeDfApp` | Root component | App state | 전역 상태 단일 소스 |

원칙: **상태 접근은 컨테이너(rows/panels)에서만**, 표시는 `crate::ui` 컴포넌트가.
`onChange`(`.clicked()`/`.changed()`)는 호출부(컨테이너)가 처리.

---

## 2. 툴바 계층 트리

```
TopBar (Panel::top — app/toolbar/mod.rs::toolbar)
└─ layout::vstack (행 사이 여백)
   ├─ layout::toolbar_row "row1"   ── 파일/패널/편집 (toolbar::rows::row_top)
   │   ├─ group "Window"   Hide UI · Window Focus
   │   ├─ vdivider
   │   ├─ group "Panels"   Library · Outline · Bookmarks · Palette  (toolbar_panel_group)
   │   ├─ vdivider
   │   ├─ group "Edit"     Undo · Redo · Clear Page
   │   ├─ vdivider
   │   ├─ group "File"     Save Edits · Load Edits
   │   └─ vdivider + group "More"  (toolbar_overflow_menu)
   │        └─ menu: Align L/C/R · Dictionary · Media · Media Server ·
   │                 Macro · Gamepad · Cache · Debug HUD
   ├─ layout::hseparator
   ├─ layout::toolbar_row "row2"   ── 페이지/캔버스/종이 (row_pages)
   ├─ layout::hseparator
   ├─ layout::toolbar_row "row3"   ── 도구/색/폭 (row_tools)
   ├─ layout::hseparator
   └─ layout::toolbar_row "search"  (Ctrl+F 시만)
```

**그룹 규칙**: 각 논리 그룹은 `layout::group(gap, …)`으로 묶고, 그룹 사이는
`layout::vdivider`로 구분. 행 사이는 `layout::hseparator`.

---

## 3. 레이아웃 컴포넌트 (`crate::ui::layout`)

| API | 의미 | Bootstrap |
|-----|------|-----------|
| `SP_1..SP_4` (4/8/12/16px) | 8px 그리드 스페이서 | `$spacer` |
| `hstack(ui, gap, \|ui\| …)` | 수평 스택 (flex-row) | `d-flex gap-*` |
| `vstack(ui, gap, \|ui\| …)` | 수직 스택 (flex-col) | `d-flex flex-column` |
| `vdivider(ui)` | 그룹 사이 세로 구분선 | `border-start` |
| `hseparator(ui)` | 행 사이 가로 구분선 | `<hr>` |
| `toolbar_row(ui, id, …)` | 스크롤 툴바 리본 | `.navbar` + overflow |
| `group(ui, gap, …)` | 인라인 그룹 | `.btn-group` |

---

## 4. 범용(원자) 컴포넌트 (`crate::ui::components`)

| API | 의미 | React/Bootstrap |
|-----|------|-----------------|
| `pill(ui, text, selected)` | 상태 배지 | `<span class="badge">` |
| `caption(ui, text)` | 약한 그룹 라벨 | `<small text-muted>` |
| `help(ui, text)` | 컨트롤 아래 도움글 | `<div class="form-text">` |
| `placeholder(ui, text)` | 빈 상태 표시 | empty state |
| `badge(ui, text, Tone)` | 컬러 배지 (Neutral/Primary/Success/Warning/Danger/Info) | `.badge bg-*` |
| `tag(ui, text, Tone) -> bool` | 삭제 가능한 태그 (× 클릭 시 true) | `<span class="badge"> ×` |
| `alert(ui, Tone, title, msg)` | 알림/콜아웃 박스 | `.alert alert-*` |
| `status_dot(ui, Tone, label)` | 상태 점 + 라벨 | `<i class="dot">` |
| `progress(ui, 0..1, Tone)` | 진행 바 | `.progress-bar` |
| `spinner(ui)` | 로딩 스피너 | spinner |
| `kbd(ui, text)` | 키보드 키 | `<kbd>` |
| `code(ui, text, copyable) -> bool` | 코드 스니펫 + 복사 | `<code>` |
| `breadcrumb(ui, &[&str])` | 경로 크럼 | `.breadcrumb` |
| `divider_label(ui, text)` | ── 라벨 ── 구분 | `<hr>` + text |
| `avatar(ui, text, Tone)` | 이니셜 원형 | `<img class="avatar">` |
| `count(ui, n, Tone)` | 숫자 카운트 pill | `.badge` |
| `tabs(ui, &mut idx, &[&str])` | 탭 바 | `.nav-tabs` |
| `card(ui, title?, Tone, \|ui\| …)` | 타이틀 카드 박스 | `.card` |

> 표시 원자는 props-only이며 상태를 건드리지 않습니다. 색 계열(세마틱 톤)은 `Tone`으로 통일.

---

## 5. 기존 재사용 컴포넌트

- `crate::ui::{ icon_button, icon_toggle, icon_select, icon_label, check, slider, action, section, hint }`
- `crate::ui::form::{ group, label, help, fieldset, text, password, textarea, color, range/select/check/switch, number, segmented }`
- `crate::ui::dialog::{ pad, modal, actions }`

---

## 6. 확장 재사용 라이브러리 (신규)

### 3계층 버튼 — `crate::ui::buttons`
| API | 의미 | Bootstrap |
|-----|------|-----------|
| `Button::primary(label).icon(..).hint(..).danger(..).size(..).show(ui)` | 강조 CTA | `.btn-primary` |
| `Button::secondary(label).show(ui)` | 기본/중립 | `.btn-secondary` |
| `Button::ghost(label).show(ui)` | 프레임 없는 subtle | `.btn-ghost` |
| `btn_primary / btn_secondary / btn_ghost(ui, label)` | 숏컷 | — |
> 보조 props: `icon`, `size(Small/Medium/Large)`, `danger`, `selected`, `enabled`, `framed`, `hint`.
> 모두 `egui::Response` 반환 → `if btn.clicked() { .. }`로 상태 연결.

### 범용 컨테이너 — `crate::ui::containers`
| API | 의미 |
|-----|------|
| `container().pad(Margin).margin(..).fill(..).bordered(..).rounded(..).show(ui, \|ui\| …)` | 프레임 박스(마진/패딩/배경/테두리/모서리) |
| `grid(n).gap(x,y).striped(..).min_col_width(..).show(ui, id, \|ui\| …)` | 그리드 |
| `row(ui, Align, gap, \|ui\| …)` / `centered(ui, \|ui\| …)` | 플렉스 수평/중앙 |

### 스크롤바 — `crate::ui::scroll`
`scroll(id).dir(ScrollDir::Vertical|Horizontal|Both).max_height(..).stick_to_bottom(..).show(ui, \|ui\| …)`

### 토스트 — `crate::ui::toast`
- `ToastQueue::new()` → `toasts` 필드로 보관
- `push(kind: Info|Success|Warning|Danger, title, message, now_ms)` → 큐에 추가
- `show(&ctx)` → 매 프레임 호출, **우상단 스택 + 자동 소멸(self-dismiss) + ✕**
- 앱 `mod.rs`에 `toasts` 필드로 배선 완료, 시작 시 환영 토스트 1회 표시.

### 폼 확장 — `crate::ui::form`
- `textarea(ui, &mut v, hint, rows)` — 여러 줄 텍스트
- `segmented(ui, &mut idx, &[&str])` — btn-group 스타일 선택
- (기존) `number/select/check/switch/range/color/text/password` 재사용

---

## 7. 작성 가이드 (새 UI를 만들 때)

1. **배치** → `layout::hstack/vstack/toolbar_row/group/vdivider` + `containers::grid/row`.
2. **버튼** → `buttons::Button` (3계층) / `icon_*`.
3. **표시 원자** → `components` / `form::*`.
4. **상태 연결(onClick)** → 컨테이너(`rows`/`panels`)에서만 `self.*` 접근.
5. **여백/컨테이너** → 8px 그리드(`SP_*`), `container()` 재사용.
6. 새 재사용 부품은 먼저 컴포넌트로 만들고(빌더 props), 사용처에서 조립.

_끝._