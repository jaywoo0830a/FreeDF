/**
 * UI 노드 어휘 (잎 — 의존성 0).
 *
 * UI 아키텍처의 **공유 어휘**다: 렌더가 생산하고, 검증(a11y)/계측(자동화)/
 * 테스트(하니스)가 소비한다. idea4 의 `events.js` 와 같은 자리다 — 누구 위에도
 * 서지 않는다.
 *
 * 계약 (입력 축에서 검증된 원칙의 UI 버전):
 *  ① 노드는 **순수 데이터**다 — 렌더러 타입(egui 등)이 여기 없다. 그래서
 *     렌더러 없이 트리를 만들고, 비교하고, 검사할 수 있다.
 *  ② 상호작용은 **메시지(데이터)** 다 — `on: { tap: {...} }`. 위젯이 앱 상태를
 *     직접 고치지 않는다 (즉시 모드의 "그리면서 고치기"와 정반대).
 *  ③ `id` 는 **공개 계약**이다 — a11y 검증·계측(자동화)·테스트가 같은 id 를 쓴다.
 *     생성자가 id 를 **자동으로 붙이지 않는다**: 자동 증번 id 는 렌더를
 *     비결정적으로 만들고(같은 state → 다른 트리) 계약이 사라진다.
 *     id 를 붙이는 책임은 호출부(컴포넌트)에, 조립 시 충돌은 `scope` 가 막는다.
 */

/** 노드 종류 — 닫힌 판별 유니언. 소비자는 전부 다뤄야 한다 (새 종류 = 전파 계약). */
export const Kind = {
  TEXT: 'text',
  BOX: 'box',
  BUTTON: 'button',
  TOGGLE: 'toggle',
};

/** 인터랙티브 종류 — id/이름/타깃 크기 계약의 대상 (a11y 단일 관문). */
export const INTERACTIVE = new Set([Kind.BUTTON, Kind.TOGGLE]);

/** 범용 노드 생성 — 어휘의 원형. props 는 데이터 그대로 실린다. */
export const node = (kind, props = {}, children = []) => ({
  kind,
  ...props,
  children,
});

/** 읽기 전용 텍스트. */
export const text = (content, props = {}) => node(Kind.TEXT, { ...props, content });

/** 레이아웃 컨테이너 (기본: 세로 쌓기). */
export const box = (props = {}, children = []) => node(Kind.BOX, props, children);

/** 버튼 — `on.tap` 메시지를 남긴다. id/label 은 계약 (a11y 가 강제). */
export const button = (id, label, on = {}, props = {}) =>
  node(Kind.BUTTON, { id, role: 'button', label, on, ...props });

/** 토글 — 값(value)을 가지는 컨트롤. `on.change(next)` 로 다음 값을 남긴다. */
export const toggle = (id, label, value, on = {}, props = {}) =>
  node(Kind.TOGGLE, { id, role: 'toggle', label, value, on, ...props });

/**
 * 구조 스냅샷 — 트리의 **데이터 등가물** (함수는 값이 아니라 계약이므로 떨어진다).
 *
 * 렌더는 매번 새 `on` 클로저를 만들 수 있다(순수성에는 문제없다 — 같은 입력에
 * 같은 **데이터**를 내면 된다). 비교/스냅샷 테스트는 이 함수의 결과로 한다:
 * 같은 state 로 렌더한 두 트리의 snapshot 은 반드시 같다.
 */
export const snapshot = (tree) => JSON.parse(JSON.stringify(tree));

/** 트리 순회 — 전위(pre-order). path 는 children 인덱스 경로. */
export const walk = (tree, fn, path = []) => {
  fn(tree, path);
  (tree.children ?? []).forEach((child, i) => walk(child, fn, [...path, i]));
};

/** id 로 노드 찾기 (없으면 undefined). */
export const find = (tree, id) => {
  let hit;
  walk(tree, (n) => {
    if (hit === undefined && n.id === id) hit = n;
  });
  return hit;
};

/** 트리의 모든 id (순서 보존). */
export const ids = (tree) => {
  const out = [];
  walk(tree, (n) => {
    if (n.id !== undefined) out.push(n.id);
  });
  return out;
};

/**
 * 모듈화 — 하위 트리의 **id 네임스페이스** + 메시지 **스코프 태그**.
 *
 * 컴포넌트는 자기 안에서 짧은 id(`"ok"`)와 지역 메시지를 쓰고, 조립하는 쪽이
 * `scope('toolbar', tree)` 로 충돌 없는 트리를 만든다. 스코프가 겹치면
 * `msg.scope = [바깥, ..., 안]` (라우팅은 첫 원소).
 */
export const scope = (prefix, tree) => {
  // 주의: spread 순서 — scope 를 나중에 써야 겹친 스코프가 보존된다.
  // `on` 의 값은 메시지(데이터)이거나 메시지 **빌더**(함수, 토글의 change 등)다 —
  // 빌더는 실행 결과를 다시 태깅한다 (스코프가 몇 겹이든 메시지에 남는다).
  const retag = (msg) =>
    typeof msg === 'function'
      ? (arg) => retag(msg(arg))
      : { ...msg, scope: [prefix, ...(msg.scope ?? [])] };
  const go = (n) => ({
    ...n,
    ...(n.id !== undefined ? { id: `${prefix}/${n.id}` } : {}),
    ...(n.on !== undefined
      ? { on: Object.fromEntries(Object.entries(n.on).map(([k, m]) => [k, retag(m)])) }
      : {}),
    children: (n.children ?? []).map(go),
  });
  return go(tree);
};