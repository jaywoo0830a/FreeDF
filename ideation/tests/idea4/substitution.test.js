import { describe, expect, it } from 'vitest';
import {
  createHub,
  createEventSource,
  attachStylus,
  attachMouse,
  createWorkspace,
  replay,
  Pointer,
} from '../../src/idea4/index.js';
import { checkWellFormed, commandTypes } from './invariants.js';

/**
 * 아키텍처 설계 테스트 ④ — 교체 가능성 (substitution).
 *
 * idea #3 의 핵심 계약: 장치 축과 툴 축은 직교한다.
 * 장치 × 툴 전체 매트릭스에서 같은 제스처가 잘-형성 커맨드를 만들고,
 * 장치가 달라도 커맨드 "형태"는 동일해야 한다.
 */

const GESTURE = [
  ['down', { x: 0, y: 0 }],
  ['drag', { x: 1, y: 1 }],
  ['drag', { x: 2, y: 1 }],
  ['up', { x: 2, y: 1 }],
];

/** 장치 축 — 같은 제스처를 서로 다른 하드웨어로 주입한다 */
const deviceSetups = {
  tablet: (hub) => {
    const hw = createEventSource();
    attachStylus(hub, hw); // 압력/기울기 있음
    return (phase, point) => hw.fire('packet', { phase, pos: point, pressure: 0.6, tilt: 0.1 });
  },
  mouse: (hub) => {
    const hw = createEventSource();
    attachMouse(hub, hw); // 압력/기울기 없음 → 어댑터가 기본값 채움
    return (phase, point) => {
      if (phase === 'down') hw.fire('down', { pos: point });
      else if (phase === 'drag') hw.fire('move', { pos: point });
      else hw.fire('up', {});
    };
  },
  replay: (hub) => {
    // 하드웨어 없음 — 통합 이벤트 스크립트를 직접 재생 (recording/player 계약)
    return (phase, point) =>
      replay(hub, [Pointer('tablet', phase, point, 0.9, 0.2)]);
  },
};

const runGesture = (deviceName, tool) => {
  const hub = createHub();
  const ws = createWorkspace(hub);
  const send = deviceSetups[deviceName](hub);
  ws.select(`tool:${tool}`);
  for (const [phase, point] of GESTURE) send(phase, point);
  return ws;
};

describe('교체 가능성: 장치 × 툴 매트릭스', () => {
  const devices = ['tablet', 'mouse', 'replay'];
  const tools = ['pen', 'eraser'];

  for (const deviceName of devices) {
    for (const tool of tools) {
      it(`${deviceName} → ${tool}: 잘-형성 커맨드 스트림`, () => {
        const ws = runGesture(deviceName, tool);
        expect(checkWellFormed(ws.commands)).toBe(true);

        if (tool === 'pen') {
          expect(commandTypes(ws.commands)).toEqual([
            'begin-stroke', 'extend-stroke', 'extend-stroke', 'end-stroke',
          ]);
          expect(ws.commands[0].tool).toBe('pen'); // 선택한 툴이 실제로 반영됨
        } else {
          expect(commandTypes(ws.commands)).toEqual([
            'erase-at', 'erase-at', 'erase-at', 'end-erase',
          ]);
        }
      });
    }
  }

  it('장치가 달라도 커맨드 "형태"는 동일하다 (압력 유무는 어댑터가 흡수)', () => {
    const tablet = runGesture('tablet', 'pen').commands;
    const mouse = runGesture('mouse', 'pen').commands;
    const replayed = runGesture('replay', 'pen').commands;

    expect(commandTypes(mouse)).toEqual(commandTypes(tablet));
    expect(commandTypes(replayed)).toEqual(commandTypes(tablet));
    // 다른 점은 오직 능력값: 태블릿은 필압이 살아있고, 마우스는 기본값이다
    expect(tablet[0].pressure).toBe(0.6);
    expect(mouse[0].pressure).toBe(1.0);
  });

  it('재생 스크립트 = 녹화 파일의 모양 — 장치 없이도 완전한 획이 된다', () => {
    const ws = runGesture('replay', 'pen');
    expect(ws.commands[0]).toEqual({
      type: 'begin-stroke', tool: 'pen', point: { x: 0, y: 0 }, pressure: 0.9,
    });
  });
});
