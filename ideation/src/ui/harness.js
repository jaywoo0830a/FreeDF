/**
 * 하니스 — 렌더러 없이 UI 를 돌리는 **헤드리스 런타임** (유일한 순수성 경계).
 *
 * Rust 대응: egui 즉시 모드 프레임의 이상형. 즉시 모드는 "그리면서 상태를 읽고
 * 고치는" 스타일이라 프레임 결과가 외부와 결합된다(두 시계 문제의 UI 버전).
 * 여기서는 프레임을 **순수 함수 패스**로 강제한다:
 *
 *     view(state, env) → tree        (렌더 — 출력만, 상태 변경 없음)
 *     update(state, msg) → state'    (갱신 — 메시지 데이터만, 렌더 없음)
 *
 * 계약:
 *  ① 렌더는 같은 state/env 로 두 번 돌리면 **같은 트리**다 (결정적 — id 자동
 *     증번·난수·시계 참조 금지, 정적 검사가 지킨다).
 *  ② 시간은 env.now 로 **명시 전달**된다 — `tick(ms)` 로 하니스가 움직인다.
 *     숨은 시계를 부르는 UI 는 이 하니스로 테스트할 수 없다 (그래서 안 쓰게 된다
 *     — 아래 layering.test.js 의 순수성 정적 검사가 지킨다).
 *  ③ 모든 상호작용은 **장부(ledger)** 로 관측된다 — 유실은 상태가 아니라
 *     데이터다 (입력 축의 SessionRouter 장부와 같은 원리).
 *  ④ `tap(id)` 는 **렌더 결과의 트리**에서 메시지를 꺼낸다 — 보이는 대로
 *     동작한다는 것(= 사용자 계약)을 테스트가 그대로 검증한다.
 */

import { find } from './node.js';

export const createHarness = ({ state, view, update, env = {} }) => {
  let current = state;
  let now = 0;
  const ledger = [];

  const envNow = () => ({ ...env, now });

  const dispatch = (msg) => {
    if (msg === undefined) return undefined;
    ledger.push({ msg, at: now });
    current = update(current, msg);
    return msg;
  };

  return {
    /** 현재 상태 (읽기 전용 스냅샷 — 수정은 update 경로로만). */
    state: () => current,
    /** 순수 렌더 — 몇 번을 불러도 같은 트리, 상태를 변형하지 않는다. */
    tree: () => view(current, envNow()),
    /** 시간 이동 — 유일한 시간 원천 (숨은 시계 없음). */
    tick: (ms) => {
      now += ms;
      return now;
    },
    /** 메시지 투입 — update 만 통과한다 (렌더를 건드리지 않는다). */
    dispatch,
    /** 보이는 대로 탭하기 — 트리에서 id 를 찾아 `on.tap` 메시지를 투입한다. */
    tap(id) {
      const n = find(this.tree(), id);
      if (!n) throw new Error(`계약 위반: 트리에 id 없음 — ${id}`);
      const msg = n.on?.tap;
      if (!msg) throw new Error(`계약 위반: ${id} 는 탭 메시지가 없다`);
      return dispatch(msg);
    },
    /** 값 컨트롤 — `on.change(next)` (토글 등). */
    change(id, next) {
      const n = find(this.tree(), id);
      if (!n) throw new Error(`계약 위반: 트리에 id 없음 — ${id}`);
      const msg = n.on?.change?.(next);
      if (!msg) throw new Error(`계약 위반: ${id} 는 change 메시지가 없다`);
      return dispatch(msg);
    },
    /** 장부 — 모든 메시지는 데이터로 관측된다 (복사본). */
    ledger: () => structuredClone(ledger),
  };
};