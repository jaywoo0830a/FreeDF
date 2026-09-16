import { describe, expect, it } from 'vitest';
import { Pointer, createRouter, WheelHit } from '../../src/idea5/index.js';
import { createWheelSink, wheelGeom, canvasSinks } from '../../src/idea5/index.js';

/**
 * C1 — 오버레이 탭 판정을 **싱크**로 승격 (idea5).
 *
 * Rust 대응: `app/input/wheel_sink.rs` (+ `CanvasSinks` 우선순위 복합).
 *
 * 지키는 것: 같은 프레스가 잉크와 **같은 시계·같은 기하**로 판정된다 —
 * egui 원시 이벤트 재판정(다른 시계)도, 그 뒤처리(삼킴 표식)도 없다.
 */
const geom = (center = [100, 100]) =>
  wheelGeom({ center, backR: 56, ringR: 34, swatchR: 12, centerR: 15, ringLen: 4 });

const inkSink = (canvas = [0, 0, 1000, 1000]) => {
  const events = [];
  return {
    name: 'ink',
    events,
    admit(session) {
      const [x, y] = session.down.point;
      const inside =
        x >= canvas[0] && y >= canvas[1] && x <= canvas[2] && y <= canvas[3];
      return inside ? 'now' : 'refuse';
    },
    handle: (evs) => events.push(...evs),
  };
};

const build = ({ open }) => {
  const wheel = createWheelSink();
  wheel.setGeometry(geom());
  wheel.setOpen(open);
  const ink = inkSink();
  const sinks = canvasSinks({ wheel, ink });
  return { wheel, ink, router: createRouter({ sinks: [sinks] }) };
};

describe('idea5 / C1 — 휠 싱크', () => {
  it('기하 히트테스트는 순수 함수 (egui/시계 없음)', () => {
    const g = geom();
    expect(g.hit([100, 100]).kind).toBe(WheelHit.CENTER);
    for (let i = 0; i < 4; i += 1) {
      expect(g.hit(g.swatchPos(i))).toEqual({ kind: WheelHit.SWATCH, index: i });
    }
    expect(g.hit([100, 300]).kind).toBe(WheelHit.OUTSIDE);
  });

  it('열려 있으면 프레스는 휠 소유 — 잉크는 받지 않는다', () => {
    const { wheel, ink, router } = build({ open: true });
    const p = wheelGeom({ center: [100, 100], ringLen: 4 }).swatchPos(0);
    const rep = router.dispatch(Pointer('pen', 'down', p, 0.5, [0, 0]), { now: 0 });
    expect(rep).toMatchObject({ outcome: 'admitted', sink: 'canvas' });
    router.dispatch(Pointer('pen', 'up', p, 0.5, [0, 0]), { now: 10 });
    expect(wheel.drainIntents()).toEqual([
      { type: 'pick-swatch', index: 0 },
      { type: 'close' },
    ]);
    expect(ink.events).toHaveLength(0); // 같은 프레스가 잉크로 새지 않는다
  });

  it('닫혀 있으면 같은 프레스가 잉크로 간다 (우선순위가 상태에 따라 뒤집힌다)', () => {
    const { wheel, ink, router } = build({ open: false });
    router.dispatch(Pointer('pen', 'down', [100, 100], 0.5, [0, 0]), { now: 0 });
    router.dispatch(Pointer('pen', 'up', [100, 100], 0.5, [0, 0]), { now: 10 });
    expect(wheel.drainIntents()).toEqual([]);
    expect(ink.events).toHaveLength(2);
  });

  it('바깥 탭은 닫기만 — 점(유령)이 생기지 않는다', () => {
    const { wheel, ink, router } = build({ open: true });
    router.dispatch(Pointer('pen', 'down', [900, 900], 0.5, [0, 0]), { now: 0 });
    router.dispatch(Pointer('pen', 'up', [900, 900], 0.5, [0, 0]), { now: 10 });
    expect(wheel.drainIntents()).toEqual([{ type: 'close' }]);
    expect(ink.events).toHaveLength(0);
  });

  it('중앙 탭 = 지우개 (의도로만 남긴다 — 싱크는 앱 상태를 모른다)', () => {
    const { wheel, router } = build({ open: true });
    router.dispatch(Pointer('pen', 'down', [100, 100], 0.5, [0, 0]), { now: 0 });
    expect(wheel.drainIntents()).toEqual([
      { type: 'select-tool', tool: 'eraser' },
      { type: 'close' },
    ]);
  });

  it('휠 프레스 중에도 컨트롤(펜 버튼) 흐름은 살아 있다', () => {
    // 정적 확인 — 컨트롤은 포인터 어휘가 아니므로 이 복합을 거치지 않는다.
    const { router } = build({ open: true });
    const control = { kind: 'control', control: 'stylus-button', index: 1, phase: 'down' };
    expect(router.dispatch(control, { now: 0 }).outcome).toBe('ignored');
    expect(router.openSession()).toBeNull(); // 컨트롤은 세션을 열지 않는다
  });
});