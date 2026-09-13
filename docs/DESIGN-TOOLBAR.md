# 툴바 재설계 청사진 — "하나의 액션 = 한 곳" 원칙

> 이 문서는 FreeDF 툴바를 **원시 egui 프리미티브 위의 React식 선언형 조립**으로
> 재구성하기 위한 설계 결정(Design Authority)입니다. 여러 세션에 걸쳐 점진적으로
> 검증하며 이행합니다.

## 1. 설계 원칙

### 1-1. 하나의 액션은 한 곳 (Single Home)
어떤 액션이든 툴바에 **정확히 한 번만** 나타납니다.
- 같은 토글/버튼이 여러 행에 중복 등장하지 않는다.
- "More" 오버플로 메뉴는 본 툴바에 없는 **차등 액션**만 담는다
  (본 툴바 내용의 복사본 금지).

### 1-2. 자주 쓰는 액션만 2중 배치 (Palette Pair)
잦은 액션에만 **상주 슬롯 + 팔레트/우클릭/단축키**의 2중 접근을 허용합니다.
- 예: 잉크색 토글(상주 + 색상 팔레트), 페이지 삽입(상주 + 우클릭), Undo(상주 + Ctrl+Z).
- 잦지 않은 액션은 단일 홈 + 필요한 경우 "More" 한 곳에만.

### 1-3. 선언형 조립 (Declarative Composition)
버튼을 매번 베이프라이트로 쌓지 않습니다. 액션은 스펙으로 선언하고,
**액션 바 팩토리**(`ui::actionbar`)가 렌더/클릭 판정을 담당합니다.

```ignore
// 상태(FreeDfApp) ↔ 스펙 연결만 남는다.
let hit = ActionBar::new()
    .add(undo, icons::ARROW_COUNTER_CLOCKWISE, "Undo").hint("Undo (Ctrl+Z)")
    .add(redo, icons::ARROW_CLOCKWISE, "Redo").hint("Redo (Ctrl+Y)")
    .show(ui);
```

### 1-4. 원시 프리미티브 계층
팩토리는 egui 원시(`Button`/`Toggle`/`checkbox`) 또는 `ui::icon_button` 위에만
얹습니다. 도메인 계층(row 함수)은 팩토리/컴포넌트에만 의존합니다.

## 2. 계층 구조

```
egui 원시 (Button/Toggle/ScrollArea/…)
      │
ui::icon_button · icon_toggle · icon_select · layout::group …
      │
ui::actionbar::ActionBar        ← 선언형 액션 바 (React <ActionBar/>)
      │
app::toolbar::rows::row_*      ← 상태(FreeDfApp) ↔ 스펙 연결만
app::toolbar::settings::*      ← 설정 스펙 (grid/card/alert)
```

## 3. 현재 행 → 목표 그룹

| 행 | 현재 | 목표(특이사항) |
|----|------|----------------|
| Row1 | Hide UI · Focus · [패널 토글] · Undo/Redo/Clear · Save/Load · More | `ActionBar`로 Undo/Redo/Clear·Save/Load 전환. 패널 토글은 단일 홈(3+1) |
| Row2 | Page(삽입/회전/삭제) · Canvas · Wheel · Paper | 삽입=상주, 회전=`More`로 이관(단일 홈) |
| Row3 | 검색 | 상태유지 |
| 설정 | 펜/만년필/휠/캔버스/종이/서버 | `grid`/`card`/`alert` 키트 적용, 탭 구성 |

## 4. 이행 순서 (세션 계획)

1. ✅ `ui::actionbar::ActionBar` 신설 + Row1의 Undo/Redo/Clear에 적용
2. ✅ `ActionKind::Toggle`(상태 바인딩)·`Select`(라디오) 추가 →
   패널 토글(Library/Outline/Bookmarks/Palette)·정렬 라디오를 스펙화
3. ✅ Row1 Save/Load·Row2 페이지 그룹(Insert+Delete)을 `ActionBar`로 전환.
   Rotate는 메뉴로 단일 홈 유지 — 중복 Delete 제거
4. 설정 창 `grid`/`card`/`alert`/`tabs` 재구성
5. 중복 액션 정리(More 오버플로 dedupe) — 종료 조건 확인

## 5. 종료 조건
- 각 액션이 툴바에 정확히 한 홈(자주 쓰는 것만 2중) — grep으로 중복 버튼 0.
- 모든 액션 렌더가 `ui::` 계층(원시/팩토리/컴포넌트)에만 의존 — `egui::Button` 직접 호출 0.
- `cargo build` 경고 0 · `cargo test -p freedf-core` 전체 통과.