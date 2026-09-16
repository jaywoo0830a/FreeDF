import { describe, expect, it } from 'vitest';
import { createHarness, mount, snapshot, find } from '../../src/ui/index.js';
import { toolbar } from './fixtures.js';

/**
 * 하니스 — 렌더러 없이 UI 흐름 전체를 검증한다.
 *
 * 흐름: state → view(트리) → (탭 = 메시지) → update → state' → view' …
 *
 * 지키는 것:
 *  ① 렌더는 순수하다 — 같은 state/env 로 두 번 돌리면 같은 **데이터** 트리
 *     (snapshot 동일; `on` 클로저는 매번 새로 만들어도 된다 — 값이 아니라 계약).
 *  ② 시간은 env.now 로 명시 전달된다 (tick — 숨은 시계 없음).
 *  ③ 모든 메시지는 장부로 관측된다 (유실은 데이터).
 *  ④ tap(id) 은 **보이는 트리**에서 메시지를 꺼낸다 — 테스트가 사용자 계약을
 *     그대로 검증한다 (id 만 알면 된다: 자동화/계측과 같은 접근).
 *
 * 픽스처는 조립된 앱(`mount`)을 쓴다 — 실제 앱의 모양이기 때문이다. 컴포넌트는
 * 스코프 전 id(`stroke`)로, 조립 후는 네임스페이스 id(`toolbar/stroke`)로 본다.
 */
const app = mount({ components: [toolbar] });
const make = () =>
  createHarness({ state: app.init({}), view: app.view, update: app.update });

describe('ui / harness — 헤드리스 런타임', () => {
  it('렌더는 순수하다 — 같은 상태로 두 번 돌려도 같은 트리, 상태는 불변', () => {
    const h = make();
    const before = JSON.stringify(h.state());
    const a = h.tree();
    const b = h.tree();
    expect(snapshot(a)).toEqual(snapshot(b)); // 결정적
    expect(a).not.toBe(b); // 하지만 매번 새 트리 (공유 변형 없음)
    expect(JSON.stringify(h.state())).toBe(before); // 렌더가 상태를 고치지 않는다
  });

  it('탭 → 메시지 → 상태 → 렌더 — 보이는 대로 동작한다', () => {
    const h = make();
    h.tap('toolbar/stroke');
    h.tap('toolbar/stroke');
    expect(h.state().toolbar.strokes).toBe(2);
    // 렌더 결과에도 반영된다 (사용자가 보는 것 = 상태).
    expect(find(h.tree(), 'toolbar/count').content).toBe('strokes: 2');
  });

  it('값 컨트롤 — change 로 다음 값을 보낸다 (토글)', () => {
    const h = make();
    expect(find(h.tree(), 'toolbar/pressure').value).toBe(true);
    h.change('toolbar/pressure', false);
    expect(h.state().toolbar.pressure).toBe(false);
    expect(find(h.tree(), 'toolbar/pressure').value).toBe(false);
  });

  it('시간은 명시적이다 — tick 만이 시간을 움직인다', () => {
    const h = make();
    h.tick(16);
    h.tick(16);
    h.tap('toolbar/stroke');
    // 장부의 메시지는 **스코프 태그를 안고 있다** — 목적지가 데이터로 남는다.
    expect(h.ledger()).toEqual([
      { msg: { type: 'stroke', scope: ['toolbar'] }, at: 32 },
    ]);
  });

  it('모든 메시지는 장부로 관측된다 — 상태를 못 바꾼 메시지도 기록은 남는다', () => {
    const h = make();
    h.dispatch({ type: 'unknown-thing' }); // 컴포넌트가 모르는 메시지
    expect(h.state().toolbar.strokes).toBe(0); // 상태 불변
    expect(h.ledger()).toHaveLength(1); // 하지만 유실이 아니라 **기록**으로 남는다
    expect(h.ledger()[0].msg.type).toBe('unknown-thing');
  });

  it('계약 위반은 드러난다 — 없는 id, 메시지 없는 컨트롤', () => {
    const h = make();
    expect(() => h.tap('nope')).toThrow(/nope/);
    // 텍스트 노드는 탭 메시지가 없다 (인터랙티브가 아니다).
    expect(() => h.tap('toolbar/count')).toThrow(/탭 메시지가 없다/);
  });

  it('컴포넌트 단독도 같은 계약으로 돌아간다 — 조립이 스코프를 입힐 뿐이다', () => {
    // 스코프 전: 컴포넌트는 자기 짧은 id 로 테스트한다.
    const h = createHarness({
      state: toolbar.init({}),
      view: toolbar.view,
      update: toolbar.update,
    });
    h.tap('stroke');
    expect(h.state().strokes).toBe(1);
    expect(find(h.tree(), 'count').content).toBe('strokes: 1');
  });
});