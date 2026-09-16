import { describe, expect, it } from 'vitest';
import { check, ok, duplicates, target } from '../../src/ui/index.js';
import { box, text, button, toggle, scope } from '../../src/ui/index.js';

/**
 * a11y — **단일 관문**을 렌더러 없이 통과시킨다 (Rust `ui/a11y.rs` 의 이상형).
 *
 * 지키는 것: 위반은 예외가 아니라 **데이터**(issues)로 모인다. 그래서
 * `cargo test` 의 헤드리스 검증과 같은 방식으로 — GUI 없이 — 컴포넌트 계약을
 * 전부 검사할 수 있다. 위반 한 건이 "규칙 + id + 수정 힌트"를 안고 있다.
 */
describe('ui / a11y — 단일 관문 (트리 검증)', () => {
  it('이름 없는 컨트롤은 위반이다 — 아이콘만 있는 버튼도 이름이 필요하다', () => {
    const issues = check(box({}, [button('gear', '')]));
    expect(issues).toEqual([
      { rule: 'missing-name', id: 'gear', hint: expect.stringContaining('이름') },
    ]);
  });

  it('id 없는 컨트롤은 위반이다 — 계측/테스트가 같은 id 를 쓴다', () => {
    const issues = check(box({}, [button(undefined, '확인')]));
    expect(issues[0].rule).toBe('missing-id');
    expect(issues).toHaveLength(1);
  });

  it('타깃 크기 하한 — tokens.target.MIN 미만은 위반 (토큰이 규칙의 원천)', () => {
    const issues = check(box({}, [
      button('tiny', '작음', {}, { style: { minHeight: target.MIN - 1 } }),
      button('ok-btn', '큼', {}, { style: { minHeight: target.MIN } }),
    ]));
    expect(issues).toHaveLength(1);
    expect(issues[0]).toMatchObject({ rule: 'small-target', id: 'tiny' });
    expect(issues[0].hint).toContain(String(target.MIN));
  });

  it('중복 id 는 조립 버그의 신호 — 스코프 조립이 이것을 예방한다', () => {
    const bad = box({}, [button('ok', 'A'), button('ok', 'B')]);
    expect(duplicates(bad)).toEqual(['ok']);
    // 조립이 스코프를 쓰면 같은 컴포넌트를 두 번 심어도 id 가 유일하다.
    const good = box({}, [scope('a', button('ok', 'A')), scope('b', button('ok', 'B'))]);
    expect(duplicates(good)).toEqual([]);
    expect(ok(good)).toBe(true);
  });

  it('내용 없는 텍스트도 지적한다 — 실수로 보이게', () => {
    const issues = check(text(undefined, { id: 't' }));
    expect(issues[0].rule).toBe('empty-text');
  });

  it('규칙을 모두 지킨 트리는 통과한다 — 이것이 컴포넌트의 계약 통과 증명', () => {
    const tree = box({ id: 'panel' }, [
      text('strokes: 3', { id: 'count' }),
      toggle('pressure', '필압 민감도', true),
      button('undo', '되돌리기'),
    ]);
    expect(check(tree)).toEqual([]);
  });
});