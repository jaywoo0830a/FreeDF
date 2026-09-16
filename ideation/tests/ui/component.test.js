import { describe, expect, it } from 'vitest';
import { mount, createHarness, ok, duplicates, defineComponent, find } from '../../src/ui/index.js';
import { box, button, text } from '../../src/ui/index.js';
import { toolbar } from './fixtures.js';

/**
 * 컴포넌트 — 모듈화 계약.
 *
 * 지키는 것: 컴포넌트는 **자기완결**이다 (앱을 모른다). 조립은 기계적이다 —
 * 상태 격리(id 키), 메시지 라우팅(scope 배열), id 네임스페이스(scope 접두).
 * 같은 컴포넌트를 두 번 심어도 a11y 관문이 통과해야 한다 (id 유일성).
 */

/** 두 번째 픽스처 — 같은 모양의 컨트롤을 가진 별개 컴포넌트 (충돌 유도용). */
const palette = defineComponent({
  id: 'palette',
  init: () => ({ color: 'black', strokes: 0 }),
  view: (s) =>
    box({ id: 'root' }, [
      text(`color: ${s.color}`, { id: 'count' }),
      button('stroke', '획 추가', { tap: { type: 'stroke' } }),
    ]),
  update: (s, msg) =>
    msg.type === 'stroke' ? { ...s, strokes: s.strokes + 1 } : s,
});

const app = mount({ components: [toolbar, palette] });
const make = () =>
  createHarness({ state: app.init({}), view: app.view, update: app.update });

describe('ui / component — 조립 계약', () => {
  it('같은 컨트롤 id 를 가진 컴포넌트를 심어도 id 가 유일하다 (스코프 조립)', () => {
    const h = make();
    expect(duplicates(h.tree())).toEqual([]);
    expect(ok(h.tree())).toBe(true); // 조립된 트리가 관문을 통과한다
    expect(h.tree().children.map((c) => c.id)).toEqual(['toolbar/root', 'palette/root']);
  });

  it('상태는 컴포넌트 키로 격리된다 — 서로 못 본다', () => {
    const h = make();
    expect(h.state()).toEqual({
      toolbar: { pressure: true, strokes: 0 },
      palette: { color: 'black', strokes: 0 },
    });
  });

  it('메시지는 데이터가 안고 온 목적지로 간다 — 크로스 유출 없음', () => {
    const h = make();
    h.tap('toolbar/stroke');
    h.tap('toolbar/stroke');
    h.tap('palette/stroke');
    expect(h.state().toolbar.strokes).toBe(2);
    expect(h.state().palette.strokes).toBe(1);
  });

  it('보이는 대로 검증 — 스코프된 id 로 트리를 읽는다', () => {
    const h = make();
    expect(find(h.tree(), 'toolbar/count').content).toBe('strokes: 0');
    expect(find(h.tree(), 'palette/count').content).toBe('color: black');
  });

  it('한 컴포넌트의 렌더가 다른 컴포넌트 트리를 바꾸지 않는다 (결정성 유지)', () => {
    const h = make();
    const paletteBefore = JSON.stringify(find(h.tree(), 'palette'));
    h.tap('toolbar/stroke');
    expect(JSON.stringify(find(h.tree(), 'palette'))).toBe(paletteBefore);
  });
});