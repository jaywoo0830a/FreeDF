import { describe, expect, it } from 'vitest';
import { target, space, dark, resolve } from '../../src/ui/index.js';
import { button, text } from '../../src/ui/index.js';

/**
 * 토큰 — "컴포넌트는 여기 있는 값만 쓴다" (Rust `ui/tokens.rs` 의 이식 전 계약).
 *
 * 토큰도 **데이터**고 스타일 결합도 **순수 함수**다: 테마 교체는 인자 하나,
 * a11y 검증이 같은 표(target.MIN)를 읽는다 — 규칙이 두 곳에 있지 않다.
 */
describe('ui / tokens — 단일 진실 원천', () => {
  it('타깃 하한 계약 — 24px 미만의 클릭 영역은 만들지 않는다 (a11y.rs 동일 값)', () => {
    expect(target.MIN).toBeGreaterThanOrEqual(24);
    expect(target.TOUCH).toBeGreaterThanOrEqual(target.COMFORT);
    expect(target.COMFORT).toBeGreaterThanOrEqual(target.MIN);
  });

  it('여백은 8px 그리드 리듬이다', () => {
    expect(space(1)).toBe(8);
    expect(space(2)).toBe(16);
  });

  it('resolve — 종류 기본값이 채워지고, 노드 스타일이 이긴다', () => {
    const styled = button('ok', '확인', {}, { style: { minHeight: target.TOUCH } });
    const s = resolve(styled);
    expect(s.minHeight).toBe(target.TOUCH); // 지정값 승
    expect(s.radius).toBe('md'); // 기본값 채움

    const plain = resolve(text('hi'));
    expect(plain).toEqual({ color: 'text', size: 14 });
  });

  it('resolve 는 순수하다 — 입력을 변형하지 않고, 같은 입력은 같은 결과', () => {
    const nodeStyle = { kind: 'button', style: { minHeight: 40 } };
    const frozen = JSON.parse(JSON.stringify(nodeStyle));
    const a = resolve(nodeStyle, dark);
    const b = resolve(nodeStyle, dark);
    expect(a).toEqual(b);
    expect(nodeStyle).toEqual(frozen); // 입력 불변
    expect(a).not.toBe(nodeStyle.style); // 새 객체 (공유 변형 없음)
  });
});