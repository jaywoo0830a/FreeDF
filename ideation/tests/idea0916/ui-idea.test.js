// ============================================================
// my-ui 합성 버전 — 사용자 관점 테스트 모음 (단일 파일)
// 구현 없음. 계약만 테스트로 드러낸다.
// 테스트 유틸은 라이브러리가 제공한다고 가정:
//   mount, findByTestId, findByText, findAllByTestId,
//   fire, focus, flush, structuredClone
// ============================================================

import { describe, expect, it, vi } from 'vitest';

// ============================================================
// 1. 자식 컴포넌트 단독 테스트 — Counter
// ============================================================

describe('Counter — 단독으로 쓸 수 있다', () => {
  const mountCounter = (props = { start: 0 }) => mount(Counter, props);

  it('props.start로 초기 상태를 정한다', () => {
    const app = mountCounter({ start: 10 });
    expect(findByText(app.view(), '10')).toBeDefined();
  });

  it('+ 버튼을 누르면 자기 상태만 바뀐다', () => {
    const app = mountCounter({ start: 0 });
    fire(findByTestId(app.view(), 'inc'));
    expect(findByText(app.view(), '1')).toBeDefined();
  });

  it('- 버튼을 누르면 1 줄어든다', () => {
    const app = mountCounter({ start: 3 });
    fire(findByTestId(app.view(), 'dec'));
    expect(findByText(app.view(), '2')).toBeDefined();
  });

  it('자기 메시지는 자기 것만 안다', () => {
    const app = mountCounter({ start: 0 });
    expect(app.messages()).toEqual(
      expect.arrayContaining([{ type: 'Inc' }, { type: 'Dec' }]),
    );
  });
});


// ============================================================
// 2. 자식이 부모에게 알리는 컴포넌트 — CounterWithNotify
// ============================================================

describe('CounterWithNotify — 이벤트를 위로 올린다', () => {
  it('변경될 때마다 onChange가 호출된다', () => {
    const onChange = vi.fn();
    const app = mount(CounterWithNotify, { start: 0, onChange });

    fire(findByTestId(app.view(), 'inc'));
    fire(findByTestId(app.view(), 'inc'));

    expect(onChange).toHaveBeenCalledTimes(2);
    expect(onChange).toHaveBeenLastCalledWith(2);
  });

  it('이벤트는 자기 상태가 아니라 콜백으로 나간다', () => {
    const onChange = vi.fn();
    const app = mount(CounterWithNotify, { start: 0, onChange });

    fire(findByTestId(app.view(), 'inc'));

    expect(app.model().count).toBe(1);
    expect(onChange).toHaveBeenCalledWith(1);
  });
});


// ============================================================
// 3. 리스트 아이템 — 자식 메시지를 부모가 소화
// ============================================================

describe('TodoItem — 상태 없는 뷰, 메시지만 낸다', () => {
  it('텍스트를 그대로 보여준다', () => {
    const app = mount(TodoItem, { id: 1, text: '밥 먹기', done: false });
    expect(app.view()).toContainText('밥 먹기');
  });

  it('체크박스를 누르면 Toggle 메시지를 낸다', () => {
    const app = mount(TodoItem, { id: 7, text: 'x', done: false });
    fire(findByTestId(app.view(), 'toggle'));
    expect(app.lastMsg()).toEqual({ type: 'Toggle', id: 7 });
  });

  it('삭제 버튼은 Remove 메시지를 낸다', () => {
    const app = mount(TodoItem, { id: 7, text: 'x', done: false });
    fire(findByTestId(app.view(), 'remove'));
    expect(app.lastMsg()).toEqual({ type: 'Remove', id: 7 });
  });

  it('done 상태가 스타일에 반영된다', () => {
    const a = mount(TodoItem, { id: 1, text: 'x', done: false });
    const b = mount(TodoItem, { id: 1, text: 'x', done: true });
    expect(a.view()).not.toEqual(b.view());
  });
});


// ============================================================
// 4. 부모가 자식을 map으로 합성 — TodoList
// ============================================================

describe('TodoList — 자식들을 map으로 합성', () => {
  const mountList = (todos = []) => mount(TodoList, { initial: todos });

  it('자식 개수만큼 아이템을 그린다', () => {
    const app = mountList([
      { id: 1, text: 'a', done: false },
      { id: 2, text: 'b', done: false },
    ]);
    expect(findAllByTestId(app.view(), 'todo-item')).toHaveLength(2);
  });

  it('입력 후 Add로 아이템이 추가된다', () => {
    const app = mountList([]);
    fire(findByTestId(app.view(), 'input'), { value: '우유 사기' });
    fire(findByTestId(app.view(), 'add'));
    expect(findAllByTestId(app.view(), 'todo-item')).toHaveLength(1);
  });

  it('자식이 낸 Toggle이 부모 상태를 바꾼다', () => {
    const app = mountList([{ id: 1, text: 'a', done: false }]);
    fire(findByTestId(app.view(), 'toggle'));
    expect(app.model().todos[0].done).toBe(true);
  });

  it('자식이 낸 Remove가 리스트에서 사라진다', () => {
    const app = mountList([
      { id: 1, text: 'a', done: false },
      { id: 2, text: 'b', done: false },
    ]);
    fire(findAllByTestId(app.view(), 'remove')[0]);
    expect(app.model().todos).toHaveLength(1);
    expect(app.model().todos[0].id).toBe(2);
  });

  it('부모가 자식 메시지를 map으로 감싼다', () => {
    const app = mountList([{ id: 1, text: 'a', done: false }]);
    const child = app.focus('todo-item:0');
    fire(findByTestId(child.view(), 'toggle'));
    expect(app.lastMsg()).toEqual({
      type: 'Item',
      index: 0,
      msg: { type: 'Toggle', id: 1 },
    });
  });
});


// ============================================================
// 5. 부모가 자식 여러 개를 조합 — App
// ============================================================

describe('App — 이종 컴포넌트 합성', () => {
  const mountApp = () => mount(App, {});

  it('Counter와 TodoList가 함께 그려진다', () => {
    const app = mountApp();
    expect(findByTestId(app.view(), 'counter')).toBeDefined();
    expect(findByTestId(app.view(), 'todo-list')).toBeDefined();
  });

  it('Counter만 조작해도 TodoList 상태는 그대로다', () => {
    const app = mountApp();
    fire(findByTestId(app.view(), 'add'), { value: 'a' });
    fire(findByTestId(app.view(), 'add'));

    const before = app.focus('todo-list').model();

    fire(findByTestId(app.view(), 'inc'));
    fire(findByTestId(app.view(), 'inc'));

    expect(app.focus('todo-list').model()).toEqual(before);
  });

  it('자식 상태는 부모 모델에 새어들지 않는다', () => {
    const app = mountApp();
    fire(findByTestId(app.view(), 'inc'));
    fire(findByTestId(app.view(), 'inc'));

    expect(app.model()).not.toHaveProperty('count');
    expect(app.focus('counter').model().count).toBe(2);
  });

  it('자식 두 곳의 메시지가 서로 섞이지 않는다', () => {
    const app = mountApp();
    app.focus('counter').dispatch({ type: 'Inc' });
    app.focus('todo-list').dispatch({ type: 'Add', text: 'x' });

    expect(app.focus('counter').model().count).toBe(1);
    expect(app.focus('todo-list').model().todos).toHaveLength(1);
  });
});


// ============================================================
// 6. 자식의 효과가 부모를 통해 흐른다
// ============================================================

describe('Cmd 합성 — 자식의 효과가 부모로 흐른다', () => {
  it('자식의 Fetch가 부모 런타임에서 실행된다', async () => {
    const api = { load: vi.fn().mockResolvedValue({ items: ['a', 'b'] }) };
    const app = mount(App, { api });

    fire(findByTestId(app.view(), 'todo-fetch'));

    expect(app.focus('todo-list').model().loading).toBe(true);
    await flush();

    expect(api.load).toHaveBeenCalledOnce();
    expect(app.focus('todo-list').model().todos).toHaveLength(2);
  });

  it('자식의 효과는 부모가 flush하기 전까지 실행되지 않는다', () => {
    const api = { load: vi.fn() };
    const app = mount(App, { api });

    fire(findByTestId(app.view(), 'todo-fetch'));

    expect(api.load).not.toHaveBeenCalled();
  });

  it('자식 여러 개가 낸 Cmd가 배치로 합쳐진다', async () => {
    const api = { load: vi.fn().mockResolvedValue({ items: [] }) };
    const app = mount(App, { api });

    app.focus('counter').dispatch({ type: 'Sync' });
    app.focus('todo-list').dispatch({ type: 'Sync' });

    expect(app.pendingCmds()).toHaveLength(2);
    await flush();
    expect(app.pendingCmds()).toHaveLength(0);
  });
});


// ============================================================
// 7. 자식 교체 / 조건부 렌더
// ============================================================

describe('조건부 자식 — 키가 바뀌면 상태가 초기화된다', () => {
  it('탭을 바꾸면 이전 탭 자식 상태는 사라진다', () => {
    const app = mount(App, {});
    app.focus('counter').dispatch({ type: 'Inc' });
    app.focus('counter').dispatch({ type: 'Inc' });
    expect(app.focus('counter').model().count).toBe(2);

    fire(findByTestId(app.view(), 'tab-todos'));
    fire(findByTestId(app.view(), 'tab-counter'));

    expect(app.focus('counter').model().count).toBe(0);
  });

  it('같은 키를 유지하면 상태가 보존된다', () => {
    const app = mount(App, {});
    app.focus('counter').dispatch({ type: 'Inc' });

    fire(findByTestId(app.view(), 'toggle-theme'));
    fire(findByTestId(app.view(), 'toggle-theme'));

    expect(app.focus('counter').model().count).toBe(1);
  });
});


// ============================================================
// 8. 컴포넌트 계약 자체를 테스트 (회귀 방지)
// ============================================================

describe('컴포넌트 계약', () => {
  it.each([
    [Counter, { start: 0 }],
    [TodoList, { initial: [] }],
    [TodoItem, { id: 1, text: 'x', done: false }],
  ])('%s는 결정적이다', (Comp, props) => {
    const a = mount(Comp, props);
    const b = mount(Comp, props);
    expect(a.view()).toEqual(b.view());
  });

  it.each([
    [Counter, { start: 0 }],
    [TodoList, { initial: [] }],
  ])('%s의 update는 원본을 변형하지 않는다', (Comp, props) => {
    const app = mount(Comp, props);
    const before = JSON.stringify(app.model());
    app.dispatchAny();
    expect(JSON.stringify(app.model())).not.toBe(before);
    expect(JSON.stringify(props)).not.toContain('undefined');
  });

  it.each([
    [Counter, { start: 0 }],
    [TodoList, { initial: [] }],
  ])('%s의 view는 플랫폼 중립적이다', (Comp, props) => {
    const el = mount(Comp, props).view();
    expect(() => structuredClone(el)).not.toThrow();
  });
});


// ============================================================
// 9. 에러 / 경계 케이스 (계약 위반 시 알려준다)
// ============================================================

describe('계약 위반 감지', () => {
  it('자식 키가 중복되면 경고한다', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    mount(App, { forceDuplicateKeys: true });
    expect(warn).toHaveBeenCalledWith(
      expect.stringContaining('duplicate key'),
    );
    warn.mockRestore();
  });

  it('map 누락된 자식 메시지가 오면 에러를 낸다', () => {
    const app = mount(App, {});
    expect(() => app.focus('todo-list').dispatch({ type: 'Alien' }))
      .toThrow(/unhandled child msg/i);
  });

  it('flush 없이 pending Cmd가 남아 있으면 알려준다', () => {
    const app = mount(App, {});
    fire(findByTestId(app.view(), 'todo-fetch'));
    expect(app.pendingCmds().length).toBeGreaterThan(0);
    expect(app.warnings()).toContain('unflushed-cmd');
  });
});


// ============================================================
// 사용자 계약 요약 (테스트가 말하는 것)
// ------------------------------------------------------------
// - 컴포넌트는 mount(Comp, props)로 단독 테스트 가능해야 한다.
// - 부모는 자식을 map으로 합성하고, 그 결과가 lastMsg()에 보여야 한다.
// - app.focus(key)로 자식에게 내려가 자식만 조작·검증할 수 있어야 한다.
// - 자식 상태는 부모 모델에 새지 않고, 키가 바뀌면 초기화되어야 한다.
// - 자식의 Cmd는 부모 런타임으로 올라와 배치로 실행되며,
//   flush() 전에는 부작용이 없어야 한다.
// - 모든 컴포넌트는 결정적이고, update는 원본을 변형하지 않으며,
//   view는 직렬화 가능해야 한다.
// - 상태 없는 뷰 컴포넌트는 lastMsg()로 메시지만 검증하면 끝난다.
// ============================================================
