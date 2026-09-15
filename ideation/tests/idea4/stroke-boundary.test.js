import { describe, expect, it } from 'vitest';
import {
  createHub,
  createEventSource,
  attachStylus,
  createControlMap,
  createWorkspace,
  Pointer,
  Action,
} from '../../src/idea4/index.js';
import { checkWellFormed, commandTypes } from './invariants.js';

/**
 * 아키텍처 설계 테스트 ③ — 획 경계 불변식.
 *
 * "툴 전환은 획 경계에서 일어난다": tap 선택, hold 수정자, 중첩 hold, 심지어
 * 잘못된 순서의 이벤트가 와도 — 각 툴 상태기계는 항상 온전한 down→…→up
 * 스트림만 보고, 커맨드 스트림은 잘-형성 상태를 유지해야 한다.
 */

const setup = (map) => {
  const hub = createHub();
  const ws = createWorkspace(hub);
  const pen = createEventSource();
  attachStylus(hub, pen, map);
  return { ws, pen };
};

describe('획 경계 불변식', () => {
  it('획 도중 tap 으로 툴을 바꾸면 합성 up/down 으로 분할된다', () => {
    const { ws, pen } = setup();
    pen.fire('packet', { phase: 'down', pos: { x: 0, y: 0 }, pressure: 0.5, tilt: 0 });
    pen.fire('packet', { phase: 'drag', pos: { x: 1, y: 0 }, pressure: 0.5, tilt: 0 });

    ws.select('tool:eraser'); // 그리는 중 툴 교체
    pen.fire('packet', { phase: 'drag', pos: { x: 2, y: 0 }, pressure: 0.5, tilt: 0 });
    pen.fire('packet', { phase: 'up', pos: { x: 2, y: 0 }, pressure: 0.5, tilt: 0 });

    expect(commandTypes(ws.commands)).toEqual([
      'begin-stroke', 'extend-stroke', 'end-stroke', // 이전 획 — 경계에서 닫힘
      'erase-at', 'erase-at', 'end-erase', // 새 세션 — 합성 down + drag 후 종료
    ]);
    expect(checkWellFormed(ws.commands)).toBe(true);
  });

  it('중첩 hold 는 LIFO 로 복원된다 (지우개 → 형광펜 → 되감기)', () => {
    const { ws, pen } = setup(
      createControlMap({
        'stylus-button#0': { action: 'tool:eraser', mode: 'hold' },
        'stylus-button#1': { action: 'tool:highlighter', mode: 'hold' },
      }),
    );
    pen.fire('packet', { phase: 'down', pos: { x: 0, y: 0 }, pressure: 0.5, tilt: 0 });

    pen.fire('control', { control: 'stylus-button', index: 0, phase: 'down' }); // eraser
    expect(ws.activeName()).toBe('eraser');
    pen.fire('control', { control: 'stylus-button', index: 1, phase: 'down' }); // highlighter (중첩)
    expect(ws.activeName()).toBe('highlighter');
    expect(ws.heldCount()).toBe(2);

    pen.fire('control', { control: 'stylus-button', index: 0, phase: 'up' }); // LIFO: eraser 복원
    expect(ws.activeName()).toBe('eraser');
    pen.fire('control', { control: 'stylus-button', index: 1, phase: 'up' }); // pen 복원
    expect(ws.activeName()).toBe('pen');
    expect(ws.heldCount()).toBe(0);
  });

  it('어떤 전환 순서가 와도 커맨드 스트림은 잘-형성을 유지한다', () => {
    const { ws, pen } = setup(
      createControlMap({
        'stylus-button#0': { action: 'tool:eraser', mode: 'hold' },
        'stylus-button#1': { action: 'tool:highlighter', mode: 'hold' },
      }),
    );
    // 난잡한 순서: 그리다 → hold → 그리다 → 중첩 hold → 해제 꼬임 → 정리
    pen.fire('packet', { phase: 'down', pos: { x: 0, y: 0 }, pressure: 0.5, tilt: 0 });
    pen.fire('control', { control: 'stylus-button', index: 0, phase: 'down' });
    pen.fire('packet', { phase: 'drag', pos: { x: 1, y: 0 }, pressure: 0.5, tilt: 0 });
    pen.fire('control', { control: 'stylus-button', index: 1, phase: 'down' });
    pen.fire('packet', { phase: 'drag', pos: { x: 2, y: 0 }, pressure: 0.5, tilt: 0 });
    pen.fire('control', { control: 'stylus-button', index: 0, phase: 'up' });
    pen.fire('control', { control: 'stylus-button', index: 1, phase: 'up' });
    pen.fire('packet', { phase: 'drag', pos: { x: 3, y: 0 }, pressure: 0.5, tilt: 0 });
    pen.fire('packet', { phase: 'up', pos: { x: 4, y: 0 }, pressure: 0.5, tilt: 0 });

    expect(checkWellFormed(ws.commands)).toBe(true);
    expect(ws.heldCount()).toBe(0); // 스택도 깨끗하게 비운다
  });

  it('hold-off 가 hold-on 없이 먼저 와도 안전하다 (방어 계약)', () => {
    const { ws, pen } = setup(
      createControlMap({ 'stylus-button#0': { action: 'tool:eraser', mode: 'hold' } }),
    );
    pen.fire('control', { control: 'stylus-button', index: 0, phase: 'up' }); // 유령 hold-off
    expect(ws.activeName()).toBe('pen');
    expect(ws.heldCount()).toBe(0);

    pen.fire('packet', { phase: 'down', pos: { x: 0, y: 0 }, pressure: 0.5, tilt: 0 });
    expect(commandTypes(ws.commands)).toEqual(['begin-stroke']); // 이후 동작도 정상
  });

  it('포인터 세션 없이 hold 만 반복해도 상태가 오염되지 않는다', () => {
    const { ws, pen } = setup(
      createControlMap({ 'stylus-button#0': { action: 'tool:eraser', mode: 'hold' } }),
    );
    for (let i = 0; i < 3; i++) {
      pen.fire('control', { control: 'stylus-button', index: 0, phase: 'down' });
      pen.fire('control', { control: 'stylus-button', index: 0, phase: 'up' });
    }
    expect(ws.heldCount()).toBe(0);
    expect(ws.activeName()).toBe('pen');
    expect(ws.commands).toEqual([]);
  });
});
