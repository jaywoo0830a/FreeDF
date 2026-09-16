# UI — 함수형/테스트 가능한 UI 인터페이스 (스텁 스펙)

> Rust 쪽 `crates/freedf/src/ui/`(tokens·a11y·kit)이 이미 갖고 있는 규칙들을
> **함수형 UI**로 일반화한 이상형 인터페이스다. 참조 구현은 [`../../src/ui/`](../../src/ui/),
> 이 디렉터리의 테스트가 실행 스펙이다 (흐름 파악용 — 엄격함보다 계약이 목적).
>
> 입력 축(idea4/5)에서 증명한 원칙을 그대로 UI 로 옮긴다:

| 입력 축에서 배운 것 | UI 계약 |
|---|---|
| 두 개의 시계가 모든 경쟁 버그의 원인 | **프레임은 순수 함수 패스**다: `view(state, env) → tree`. 렌더 중 상태를 읽고 고치는 즉시 모드 스타일 금지 |
| 이벤트는 데이터 (어휘) | **상호작용은 메시지**다: `on: { tap: msg }` — 위젯이 앱 상태를 직접 고치지 않는다 |
| 에지는 파괴되지 않는다 (장부) | **모든 메시지는 장부로 관측된다** — 하니스 `ledger()` |
| 계측 id 는 공개 계약 | **id 는 노드가 안고 있다** — a11y·자동화·테스트가 같은 id 를 쓴다. 생성자가 자동으로 만들지 않는다(결정성) |
| 시간은 인자로만 (정적 검사) | 시간/난수는 `env` 로만 — 정적 검사가 `Date.now`/`Math.random` 을 박제 |

## 모듈 지도 (의존성은 아래로만)

| 모듈 | 역할 | Rust 대응 |
|---|---|---|
| [`node.js`](../../src/ui/node.js) | 트리 어휘(잎): `text/box/button/toggle`, `walk/find/ids`, **`scope`**(id 네임스페이스 + 메시지 태그), `snapshot` | (신규) 렌더 출력 데이터 — egui 호출 대체물 |
| [`tokens.js`](../../src/ui/tokens.js) | 토큰 데이터(잎): `target.MIN` 하한, 8px 그리드, `resolve()` 순수 스타일 결합 | `ui/tokens.rs` ("매직 넘버 금지") |
| [`a11y.js`](../../src/ui/a11y.js) | **단일 관문**: `check(tree) → issues[]` (이름/계측 id/타깃 크기/유일성) — 위반은 데이터 | `ui/a11y.rs` (issues 수집 = 테스트 훅) |
| [`harness.js`](../../src/ui/harness.js) | 헤드리스 런타임: `tree()/tap(id)/change(id,v)/tick(ms)/ledger()` | egui 프레임의 이상형 + eguidev 계측의 테스트 접점 |
| [`component.js`](../../src/ui/component.js) | 모듈화 단위: `defineComponent({id,init,view,update})`, `mount`(상태 격리 + 메시지 라우팅 + id 스코프) | (신규) 화면/패널 단위 조립 |

## 실행 스펙

| 파일 | 증명하는 것 |
|---|---|
| [`node.test.js`](./node.test.js) | 노드는 데이터 · id 는 계약 · 스코프 조립 ([바깥,…,안] 순서) |
| [`tokens.test.js`](./tokens.test.js) | 타깃 하한 · 순수 스타일 결합 (입력 불변) |
| [`a11y.test.js`](./a11y.test.js) | 위반이 데이터로 모인다 · 조립해도 관문 통과 |
| [`harness.test.js`](./harness.test.js) | 렌더 순수성 · 탭→메시지→상태→렌더 · 명시적 시간 · 장부 · 계약 위반은 드러난다 |
| [`component.test.js`](./component.test.js) | 상태 격리 · 메시지 라우팅(크로스 유출 없음) · 같은 컴포넌트 2회 심기 |
| [`layering.test.js`](./layering.test.js) | 의존성 규칙 + **순수성 정적 검사**(숨은 시계/난수/DOM 금지) |

## 흐름 한 눈에

```
state ── view(state, env) ──▶ tree(순수 데이터) ──▶ a11y.check(tree) → issues[]
  ▲                                                    │
  │                                                 tap(id) / change(id, v)
  │                                                    ▼  메시지 (scope 태그)
  └── update(state, msg) ◀── 하니스 장부(ledger) ◀──────┘
```

## 설계 메모 — 이 스텁이 남긴 결정들

1. **렌더는 매번 새 트리**를 돌려준다(공유 변형 없음). 비교는 `snapshot(tree)` —
   `on` 클로저는 값이 아니라 계약이라 스냅샷에서 떨어진다.
2. **`on` 의 값은 메시지(데이터) 또는 빌더(함수)** 다. 스코프는 객체를 재태깅하고
   빌더는 실행 결과를 재태깅한다 — 몇 겹을 조립해도 메시지에 목적지가 남는다.
3. **컴포넌트는 앱을 모른다**: 상태 조각 하나, 메시지 하나만 본다. 조립
   (상태 격리·라우팅·스코프)은 `mount` 가 기계적으로 한다 — 이 네 쌍
   (id/init/view/update)을 Rust 타입으로 박제하면 이식된다.
4. **휠 기하의 교훈 재적용**: 렌더가 그리는 기하와 판정이 보는 기하가 같아야
   한다 → a11y 검증이 `tokens` 를 직접 읽는다 (규칙의 이중 소유 금지).

## 다음 단계 후보 (아직 스텁에 없음)

- `layout`: 측정 → rect 순수 기하 (WheelGeom 처럼 "렌더 = 판정"이 되도록).
- `diff/patch`: 트리 비교 — egui 즉시 모드 위에서 최소 변경만 재적용하는 경계.
- 포커스/키보드 순환, 가상화 목록, i18n(문자열도 데이터), 테마 전환 검증.
- eguidev 연결: `ids(tree)` ↔ 계측 id 표 자동 대조 (계약 드리프트 감지).
