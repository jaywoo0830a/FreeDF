import { describe, expect, it } from 'vitest';
import { readFileSync } from 'node:fs';

/**
 * 아키텍처 설계 테스트 ① — 의존성 규칙 (정적 검사).
 *
 * "의존성은 아래로만 흐른다"는 규칙을 실행 가능한 테스트로 박제한다.
 * 예: 장치 축(devices.js)이 툴(tools.js)을 import 하는 순간 이 테스트가 깨진다.
 * Rust 이식 시에도 같은 규칙을 크레이트 의존성으로 강제할 수 있다.
 */

const ALLOWED = {
  'events.js': [], // 잎 — 모든 층이 의존하는 유일한 공유물
  'commands.js': [], // 잎 — 출력 어휘 (툴이 생산, projection 이 소비)
  'descriptor.js': [], // 잎
  'control-map.js': ['descriptor.js'],
  'hub.js': ['events.js'],
  'devices.js': ['events.js'], // 장치 축: 툴/워크스페이스/캔버스를 몰라야 한다
  'tools.js': ['commands.js'], // 툴 축: 허브/장치/워크스페이스/캔버스를 몰라야 한다
  'workspace.js': ['events.js', 'tools.js'],
  'canvas.js': ['events.js', 'commands.js'], // 캔버스 경계: projection 이 surface 의 유일한 호출자
  'tool-package.js': [], // 조립 계약 — 전달받은 객체의 메서드만 사용 (import 0)
  'index.js': [
    'events.js',
    'commands.js',
    'descriptor.js',
    'control-map.js',
    'hub.js',
    'devices.js',
    'tools.js',
    'workspace.js',
    'canvas.js',
    'tool-package.js',
  ],
};

const base = new URL('../../src/idea4/', import.meta.url);

const importsOf = (file) =>
  [...readFileSync(new URL(file, base), 'utf8').matchAll(/from\s+'\.\/([^']+)'/g)].map((m) => m[1]);

describe('아키텍처 계약: 의존성 규칙', () => {
  for (const [file, allowed] of Object.entries(ALLOWED)) {
    it(`${file} 는 허용 목록 [${allowed.join(', ') || '없음'}] 만 import 한다`, () => {
      const bad = importsOf(file).filter((m) => !allowed.includes(m));
      expect(bad).toEqual([]);
    });
  }

  it('잎(events.js)은 의존성이 0이다 — 어휘는 누구 위에도 서지 않는다', () => {
    expect(importsOf('events.js')).toEqual([]);
  });

  it('장치 축은 툴 축을 알지 못한다 (idea #3 분리 계약)', () => {
    const actual = importsOf('devices.js');
    expect(actual).not.toContain('tools.js');
    expect(actual).not.toContain('workspace.js');
  });

  it('툴 축은 장치/허브를 알지 못한다 — 이벤트만 받아 커맨드를 뱉는다', () => {
    const actual = importsOf('tools.js');
    expect(actual).not.toContain('hub.js');
    expect(actual).not.toContain('devices.js');
    expect(actual).not.toContain('workspace.js');
  });

  it('캔버스 port 는 projection 만 만진다 — 툴/허브/장치/워크스페이스 금지', () => {
    for (const file of ['devices.js', 'tools.js', 'hub.js', 'workspace.js']) {
      expect(importsOf(file)).not.toContain('canvas.js');
    }
  });

  it('모든 모듈이 실제로 존재한다 (계약 객체 전체 + 배럴)', () => {
    for (const file of Object.keys(ALLOWED)) {
      expect(() => readFileSync(new URL(file, base))).not.toThrow();
    }
  });
});
