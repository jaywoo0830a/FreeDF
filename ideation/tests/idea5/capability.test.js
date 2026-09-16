import { describe, expect, it } from 'vitest';
import { capabilities, createPenAdapter, cursorAzimuth } from '../../src/idea5/index.js';

/**
 * C3 — "틸트를 보고하는 장치인가"는 **장치가 대답한다** (idea5).
 *
 * Rust 대응: `PenCapabilities`(`pen_input.rs`) → `PenEventAdapter::tilt_supported()`
 * → 렌더 분기(`paint.rs`)가 `pen_monitor.is_some()`(스트림 존재 근사) 대신
 * 능력 질의를 쓴다.
 *
 * 왜 근사가 틀렸나: **압력만 보고하는 펜도 스트림은 있다.** 스트림 존재로
 * 판정하면 그 장치의 틸트는 항상 0(수직)이라 커서가 고정 방향으로 붙는다.
 */
describe('idea5 / C3 — 틸트 능력 협상', () => {
  it('모르면 낙관 — 보고하면 쓴다 (구 동작 보존)', () => {
    const a = createPenAdapter({ caps: capabilities() });
    expect(a.tiltSupported()).toBe(true);
  });

  it('능력 질의는 스트림 존재와 다른 질문이다', () => {
    const a = createPenAdapter({
      caps: capabilities({ hasTilt: false, hasPressure: true }),
    });
    // 스트림은 흐른다 (압력은 온다) — 그런데 틸트 능력은 없다.
    const evs = a.update({ tilt: [0, 0], pressure: 0.8, contact: true }, [1, 1]);
    expect(evs).toHaveLength(1);
    expect(a.tiltSupported()).toBe(false);
  });

  it('능력이 없으면 커서는 손잡이 기본 방위각을 쓴다 (수직에 붙지 않는다)', () => {
    const noTilt = createPenAdapter({ caps: capabilities({ hasTilt: false }) });
    const [azRight] = cursorAzimuth({ adapter: noTilt, leftHanded: false });
    const [azLeft] = cursorAzimuth({ adapter: noTilt, leftHanded: true });
    expect(azRight).toBeCloseTo(-0.6, 6);
    expect(azLeft).toBeLessThan(0); // 왼손잡이는 반대 반평면
    expect(azLeft).toBeCloseTo(-Math.PI + 0.6, 6);
  });

  it('능력이 있으면 장치 방향을 따른다 (벡터 주입 → 방위각)', () => {
    const a = createPenAdapter({ caps: capabilities({ hasTilt: true }) });
    a.setTilt([0, 30]); // 사용자 쪽으로 기울임 → +90°
    const [az] = cursorAzimuth({ adapter: a });
    expect(az).toBeCloseTo(Math.PI / 2, 4);
  });

  it('주입된 틸트는 장치 어휘 범위로 클램프된다', () => {
    const a = createPenAdapter();
    a.setTilt([200, -200]);
    expect(a.tilt()).toEqual([90, -90]);
  });
});