import { describe, expect, it } from 'vitest';
import {
  createHub,
  replay,
  createControlMap,
  deviceDescriptor,
  enumerateControls,
  controlKey,
  createEventSource,
  createPenTool,
  createEraserTool,
  createHighlighterTool,
  createWorkspace,
  Pointer,
} from '../../src/idea4/index.js';

/**
 * 아키텍처 설계 테스트 ② — 계약 객체 표면.
 * 각 객체가 문서화된 API 를 정확히 노출하는지 검사한다.
 */

describe('계약 객체 표면', () => {
  it('허브: on/emit 을 노출하고, 충돌 규칙에 따라 emit 반환값이 달라진다', () => {
    const hub = createHub();
    expect(typeof hub.on).toBe('function');
    expect(typeof hub.emit).toBe('function');

    expect(hub.emit(Pointer('pen', 'down', { x: 0, y: 0 }))).toBe(true);
    expect(hub.emit(Pointer('mouse', 'down', { x: 1, y: 0 }))).toBe(false); // 충돌 drop
    hub.emit(Pointer('pen', 'up'));
    expect(hub.emit(Pointer('mouse', 'down', { x: 1, y: 0 }))).toBe(true); // 해제 후 통과
  });

  it('기술자: 열거 결과가 매핑 키 규칙과 정확히 일치한다', () => {
    const desc = deviceDescriptor({ stylusButtons: 2, expressKeys: 8 });
    const controls = enumerateControls(desc);
    expect(controls).toHaveLength(10);
    for (const c of controls) {
      expect(controlKey(c.control, c.index)).toMatch(/^(stylus-button|express-key)#\d+$/);
    }
  });

  it('매핑: 없는 키는 null, 있는 키는 {action, mode} — 미지원 하드웨어가 안전하다', () => {
    const map = createControlMap({ 'stylus-button#0': { action: 'tool:eraser', mode: 'hold' } });
    expect(map.binding('stylus-button', 0)).toEqual({ action: 'tool:eraser', mode: 'hold' });
    expect(map.binding('stylus-button', 4)).toBeNull();
    expect(map.binding('express-key', 0)).toBeNull();
  });

  it('툴 계약 { name, handle } — 참조 구현 3개가 모두 충족한다', () => {
    for (const tool of [createPenTool(), createEraserTool(), createHighlighterTool()]) {
      expect(typeof tool.name).toBe('string');
      expect(typeof tool.handle).toBe('function');
    }
  });

  it('툴은 action 을 무시하고 pointer 만 소비한다 (계약의 핵심 조항)', () => {
    const commands = [];
    const pen = createPenTool();
    pen.handle({ kind: 'action', source: 'keyboard', key: 'tool:pen' }, (c) => commands.push(c));
    expect(commands).toEqual([]); // action 이 커맨드를 만들지 않는다
    pen.handle(Pointer('pen', 'down', { x: 0, y: 0 }), (c) => commands.push(c));
    expect(commands).toHaveLength(1);
  });

  it('워크스페이스 계약: commands / activeName / select', () => {
    const ws = createWorkspace(createHub());
    expect(Array.isArray(ws.commands)).toBe(true);
    expect(ws.activeName()).toBe('pen'); // 초기 툴
    expect(typeof ws.select).toBe('function');
  });

  it('재생 계약: replay(hub, script) 로 하드웨어 없이 흐름을 구동한다', () => {
    const hub = createHub();
    const ws = createWorkspace(hub);
    replay(hub, [
      Pointer('tablet', 'down', { x: 0, y: 0 }, 0.5),
      Pointer('tablet', 'up'),
    ]);
    expect(ws.commandTypes?.() ?? ws.commands.map((c) => c.type)).toEqual(['begin-stroke', 'end-stroke']);
  });

  it('이벤트 소스 계약: on/fire — 어댑터가 붙을 수 있는 최소 모양', () => {
    const src = createEventSource();
    const seen = [];
    src.on('packet', (p) => seen.push(p));
    src.fire('packet', { phase: 'down' });
    src.fire('없는이벤트', {}); // 없는 이벤트도 안전
    expect(seen).toEqual([{ phase: 'down' }]);
  });
});
