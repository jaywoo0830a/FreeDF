import { describe, expect, it } from 'vitest';
import {
  Kind,
  INTERACTIVE,
  node,
  text,
  box,
  button,
  toggle,
  walk,
  find,
  ids,
  scope,
} from '../../src/ui/index.js';

/**
 * UI 노드 어휘 — 계약 ① "노드는 순수 데이터다".
 *
 * Rust 대응: egui 위젯 호출을 대신할 **트리 데이터**. 렌더러 타입이 여기 없으므로
 * 트리를 만들고 비교하고 검사하는 것이 전부 렌더러 없이 가능하다 — 이것이
 * "UI 를 함수로 테스트한다"의 재료다.
 */
describe('ui / node — 트리 어휘 (잎)', () => {
  it('노드는 평범한 데이터다 — 함수/클래스/렌더러 타입이 없다', () => {
    const n = button('ok', '확인', { tap: { type: 'commit' } });
    expect(n).toEqual({
      kind: 'button',
      id: 'ok',
      role: 'button',
      label: '확인',
      on: { tap: { type: 'commit' } },
      children: [],
    });
    expect(JSON.parse(JSON.stringify(n))).toEqual(n); // 직렬화 가능 = 데이터
  });

  it('생성자가 id 를 만들지 않는다 — id 는 호출부의 공개 계약 (결정성)', () => {
    // 자동 증번 id 가 있으면 같은 state 로 두 번 렌더해도 트리가 달라진다.
    // 그래서 어휘는 id 를 절대 만들지 않는다 (없으면 a11y 가 지적한다).
    const a = text('hello');
    const b = text('hello');
    expect(a).toEqual(b);
    expect(a.id).toBeUndefined();
  });

  it('토글은 값(value)을 안고 있다 — 상태는 데이터로 보인다', () => {
    const n = toggle('pen', '필압', true, { change: (v) => ({ type: 'pen', value: v }) });
    expect(n.value).toBe(true);
    expect(n.on.change(false)).toEqual({ type: 'pen', value: false });
  });

  it('walk/find/ids — 트리 조회는 순수 함수다', () => {
    const tree = box({}, [text('제목', { id: 'title' }), button('ok', '확인')]);
    const kinds = [];
    walk(tree, (n) => kinds.push(n.kind));
    expect(kinds).toEqual([Kind.BOX, Kind.TEXT, Kind.BUTTON]);
    expect(find(tree, 'title')?.content).toBe('제목');
    expect(find(tree, 'nope')).toBeUndefined();
    expect(ids(tree)).toEqual(['title', 'ok']);
  });

  it('scope — id 네임스페이스 + 메시지 스코프 태그 (모듈화의 재료)', () => {
    const scoped = scope('toolbar', box({}, [button('ok', '확인', { tap: { type: 'commit' } })]));
    expect(ids(scoped)).toEqual(['toolbar/ok']);
    // 메시지에 스코프가 태긴다 — 라우팅의 재료 (목적지는 데이터가 안고 온다).
    expect(scoped.children[0].on.tap.scope).toEqual(['toolbar']);
    expect(scoped.children[0].on.tap.type).toBe('commit');
  });

  it('스코프가 겹치면 바깥이 앞선다 — [바깥, ..., 안]', () => {
    const inner = scope('inner', button('ok', '확인', { tap: { type: 'x' } }));
    const outer = scope('outer', box({}, [inner]));
    // box 안의 이미 스코프된 버튼이 다시 스코프된다 (버튼 = outer.children[0]).
    expect(outer.children[0].id).toBe('outer/inner/ok');
    expect(outer.children[0].on.tap.scope).toEqual(['outer', 'inner']);
  });

  it('인터랙티브 종류 집합이 a11y 관문의 대상을 정의한다 (닫힌 유니언)', () => {
    expect([...INTERACTIVE].sort()).toEqual(['button', 'toggle']);
  });
});