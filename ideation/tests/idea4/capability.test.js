import { describe, expect, it } from 'vitest';
import {
  createHub,
  createEventSource,
  attachStylus,
  attachTabletControls,
  createControlMap,
  deviceDescriptor,
  enumerateControls,
  createWorkspace,
} from '../../src/idea4/index.js';
import { checkWellFormed, commandTypes } from './invariants.js';

/**
 * 아키텍처 설계 테스트 ⑤ — 가변 컨트롤 (idea #4 계약의 응용 확인).
 *
 * 펜 버튼 0/2/5개, 태블릿 익스프레스 키, 매핑-as-데이터, hold 수정자.
 * 처리 코드는 개수를 전혀 모른다 — 정체성은 (종류, 번호) 쌍뿐이다.
 */

describe('가변 컨트롤', () => {
  it('버튼 0/2/5개 펜이 같은 파이프라인으로 동작한다', () => {
    const map = createControlMap({
      'stylus-button#0': { action: 'tool:eraser', mode: 'hold' },
      'stylus-button#1': { action: 'undo', mode: 'tap' },
    });
    const desc2 = deviceDescriptor({ stylusButtons: 2 });
    const desc5 = deviceDescriptor({ stylusButtons: 5, tilt: true });

    expect(enumerateControls(deviceDescriptor())).toEqual([]); // 0버튼 — 컨트롤 자체가 없음
    expect(enumerateControls(desc2)).toHaveLength(2);
    expect(enumerateControls(desc5)).toHaveLength(5);

    const hub = createHub();
    const seen = [];
    hub.on((e) => seen.push(e));
    const five = createEventSource();
    attachStylus(hub, five, map);

    five.fire('control', { control: 'stylus-button', index: 0, phase: 'down' });
    five.fire('control', { control: 'stylus-button', index: 0, phase: 'up' });
    five.fire('control', { control: 'stylus-button', index: 1, phase: 'down' });
    five.fire('control', { control: 'stylus-button', index: 4, phase: 'down' }); // 미바인딩 — 무시

    expect(seen).toEqual([
      { kind: 'action', source: 'stylus-button', key: 'tool:eraser', mode: 'hold-on' },
      { kind: 'action', source: 'stylus-button', key: 'tool:eraser', mode: 'hold-off' },
      { kind: 'action', source: 'stylus-button', key: 'undo' },
    ]);
  });

  it('매핑은 데이터다 — 같은 하드웨어, 다른 설정 파일', () => {
    const hub = createHub();
    const seen = [];
    hub.on((e) => seen.push(e));
    const pen = createEventSource();

    attachStylus(hub, pen, createControlMap({
      'stylus-button#0': { action: 'tool:eraser', mode: 'hold' },
    }));
    pen.fire('control', { control: 'stylus-button', index: 0, phase: 'down' });
    expect(seen.at(-1).key).toBe('tool:eraser');

    attachStylus(hub, pen, createControlMap({
      'stylus-button#0': { action: 'undo', mode: 'tap' },
    }));
    pen.fire('control', { control: 'stylus-button', index: 0, phase: 'down' });
    expect(seen.at(-1).key).toBe('undo'); // 코드 변경 0
  });

  it('태블릿 익스프레스 키: 일부만 바인딩, 미바인딩은 무시된다', () => {
    const hub = createHub();
    const ws = createWorkspace(hub);
    const pad = createEventSource();
    attachTabletControls(hub, pad, createControlMap({
      'express-key#2': { action: 'undo', mode: 'tap' },
      'express-key#3': { action: 'tool:pen', mode: 'tap' },
      'express-key#7': { action: 'tool:eraser', mode: 'tap' },
    }));

    pad.fire('control', { control: 'express-key', index: 3, phase: 'down' });
    expect(ws.activeName()).toBe('pen');
    pad.fire('control', { control: 'express-key', index: 7, phase: 'down' });
    expect(ws.activeName()).toBe('eraser');
    pad.fire('control', { control: 'express-key', index: 2, phase: 'down' });
    pad.fire('control', { control: 'express-key', index: 5, phase: 'down' }); // 무시

    expect(ws.commands).toEqual([{ type: 'undo' }]);
  });

  it('hold 수정자 end-to-end: 배럴 버튼 누르는 동안만 지우개 (획 경계 분할)', () => {
    const hub = createHub();
    const ws = createWorkspace(hub);
    const pen = createEventSource();
    attachStylus(hub, pen, createControlMap({
      'stylus-button#0': { action: 'tool:eraser', mode: 'hold' },
    }));

    pen.fire('packet', { phase: 'down', pos: { x: 0, y: 0 }, pressure: 0.5, tilt: 0 });
    pen.fire('packet', { phase: 'drag', pos: { x: 1, y: 0 }, pressure: 0.5, tilt: 0 });
    pen.fire('control', { control: 'stylus-button', index: 0, phase: 'down' });
    pen.fire('packet', { phase: 'drag', pos: { x: 2, y: 0 }, pressure: 0.5, tilt: 0 });
    pen.fire('control', { control: 'stylus-button', index: 0, phase: 'up' });
    pen.fire('packet', { phase: 'drag', pos: { x: 3, y: 0 }, pressure: 0.5, tilt: 0 });
    pen.fire('packet', { phase: 'up', pos: { x: 3, y: 0 }, pressure: 0.5, tilt: 0 });

    expect(commandTypes(ws.commands)).toEqual([
      'begin-stroke', 'extend-stroke', 'end-stroke', // 펜 획 1 (분할)
      'erase-at', 'erase-at', 'end-erase', // 지우개
      'begin-stroke', 'extend-stroke', 'end-stroke', // 펜 획 2
    ]);
    expect(checkWellFormed(ws.commands)).toBe(true);
  });
});
