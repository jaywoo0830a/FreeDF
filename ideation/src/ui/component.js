/**
 * 컴포넌트 — UI 모듈화의 단위 (스텁).
 *
 * 컴포넌트 = 상태(view+update 가 아는 것)와 외형(init/update 가 만드는 것)의
 * **자기완결 묶음**이다. 컴포넌트는 앱을 모른다 — 상태 조각 하나와 메시지 하나만
 * 본다. 조립(스코프/라우팅)은 `mount` 가 기계적으로 한다:
 *
 *   - 상태는 부모 트리의 `comp.id` 키로 격리된다 (서로 못 본다).
 *   - 메시지는 `scope` 배열의 첫 원소로 라우팅된다 (idea5 라우터와 같은 원리:
 *     목적지는 데이터가 안고 온다).
 *   - 렌더된 트리는 `scope(prefix, …)` 로 id 네임스페이스를 얻는다 — 조립해도
 *     id 가 유일하다 (a11y 중복 검사가 증명한다).
 *
 * 컴포넌트가 프레임워크를 몰라도 되는 것이 포인트다 — `cargo`/egui 이식 시에도
 * 이 네 쌍(id/init/view/update)만 타입으로 박제하면 된다.
 */

import { box, scope } from './node.js';

/** 컴포넌트 정의 — 네 쌍 (id, init, view, update). */
export const defineComponent = ({ id, init = () => ({}), view, update }) => ({
  id,
  init,
  view,
  update,
});

/** 컴포넌트들을 한 앱으로 조립 — 하니스(`createHarness`)가 바로 먹는 모양. */
export const mount = ({ components }) => ({
  init: (env) =>
    Object.fromEntries(components.map((c) => [c.id, c.init(env)])),

  view: (state, env) =>
    box(
      { id: 'app' },
      components.map((c) => scope(c.id, c.view(state[c.id], env))),
    ),

  update: (state, msg) => {
    const [head, ...rest] = msg.scope ?? [];
    const comp = components.find((c) => c.id === head);
    if (!comp) {
      // 계약 밖 메시지 — 조용히 버리지만 하니스 장부에는 이미 기록됐다
      // (유실은 상태가 아니라 데이터로 관측된다).
      return state;
    }
    return { ...state, [comp.id]: comp.update(state[comp.id], { ...msg, scope: rest }) };
  },
});