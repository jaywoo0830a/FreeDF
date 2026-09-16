import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';

/**
 * UI 아키텍처 정적 검사 — ① 의존성 규칙, ② 순수성 규칙.
 *
 * idea4 의 layering.test.js 와 같은 방식이다: 규칙을 테스트로 박제해서,
 * 위반하는 코드가 "나중에 리뷰에서 걸리는" 게 아니라 **돌리는 순간** 깨진다.
 *
 * ② 순수성이 특히 중요하다: 렌더(`view`)가 Date.now/Math.random 을 부르는 순간
 * "같은 state → 같은 트리" 계약이 깨지고 스냅샷 테스트·계측·a11y 검증이 전부
 * 무의미해진다. 시간/난수는 반드시 env 로 들어온다 (하니스의 tick).
 */

const ALLOWED = {
  'node.js': [], // 잎 — 트리 어휘 (모든 층이 의존)
  'tokens.js': [], // 잎 — 토큰 데이터 (a11y 가 같은 표를 읽는다)
  'a11y.js': ['node.js', 'tokens.js'],
  'harness.js': ['node.js'],
  'component.js': ['node.js'],
  'index.js': ['node.js', 'tokens.js', 'a11y.js', 'harness.js', 'component.js'],
};

const base = new URL('../../src/ui/', import.meta.url);

const sourceOf = (file) => readFileSync(new URL(file, base), 'utf8');
const importsOf = (file) =>
  [...sourceOf(file).matchAll(/from\s+'\.\/([^']+)'/g)].map((m) => m[1]);

describe('ui 아키텍처: 의존성 규칙', () => {
  for (const [file, allowed] of Object.entries(ALLOWED)) {
    it(`${file} 는 허용 목록 [${allowed.join(', ') || '없음'}] 만 import 한다`, () => {
      const bad = importsOf(file).filter((m) => !allowed.includes(m));
      expect(bad).toEqual([]);
    });
  }

  it('모든 모듈이 실제로 존재한다 (계약 객체 전체 + 배럴)', () => {
    for (const file of Object.keys(ALLOWED)) {
      expect(() => sourceOf(file)).not.toThrow();
    }
  });

  it('잎(node.js, tokens.js)은 의존성이 0이다 — 어휘와 데이터는 누구 위에도 서지 않는다', () => {
    expect(importsOf('node.js')).toEqual([]);
    expect(importsOf('tokens.js')).toEqual([]);
  });

  it('렌더 축은 입력 축을 몰라도 된다 — UI 어휘는 events 를 import 하지 않는다', () => {
    // UI 는 메시지를 **생산**할 뿐, 입력 축의 구체 어휘(events/hub)를 모른다.
    // 연결은 앱 경계(메시지 → InputEvent/Command 번역)가 한다.
    for (const file of Object.keys(ALLOWED)) {
      expect(importsOf(file)).not.toContain('../idea4/events.js');
    }
  });
});

describe('ui 아키텍처: 순수성 규칙 (정적 검사)', () => {
  const FORBIDDEN = [
    'Date.now',
    'performance.now',
    'new Date(',
    'Math.random',
    'setTimeout',
    'setInterval',
  ];

  for (const file of Object.keys(ALLOWED)) {
    it(`${file} 은 숨은 시계/난수를 부르지 않는다 — 시간은 env, 난수는 주입`, () => {
      const src = sourceOf(file);
      const hits = FORBIDDEN.filter((f) => src.includes(f));
      expect(hits).toEqual([]);
    });
  }

  it('DOM/전역 참조 금지 — 렌더러도 브라우저도 아닌 순수 계층이다', () => {
    for (const file of Object.keys(ALLOWED)) {
      const src = sourceOf(file);
      expect(src.includes('document.')).toBe(false);
      expect(src.includes('window.')).toBe(false);
    }
  });
});