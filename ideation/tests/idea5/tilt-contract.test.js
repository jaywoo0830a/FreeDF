import { describe, expect, it } from 'vitest';
import {
  Pointer,
  tiltMagnitude,
  tiltAzimuth,
  NO_TILT,
  createRouter,
  createPenAdapter,
} from '../../src/idea5/index.js';

/**
 * C2 — 틸트 계약: 어휘가 **방향**을 나른다 (크기만 나르던 구 어휘의 상향).
 *
 * Rust 대응: `input_events::PointerEvent.tilt: [f32; 2]` + `tilt_magnitude`/
 * `tilt_azimuth` (파생값), 어댑터가 조건화된 벡터를 그대로 싣는다.
 *
 * 이 파일이 지키는 것: **이벤트가 나르는 틸트 = 소비자가 보는 틸트** — 어휘 밖의
 * 별도 벡터(앱 미러)가 필요 없다.
 */
describe('idea5 / C2 — 틸트 벡터 어휘', () => {
  it('방향을 잃지 않는다 (구 스펙은 숫자 → [n,0] 으로 정규화)', () => {
    const legacy = Pointer('pen', 'down', [0, 0], 0.5, 0.0);
    expect(legacy.tilt).toEqual([0, 0]); // 구 호출도 벡터로

    const v = Pointer('pen', 'down', [0, 0], 0.5, [20, -20]);
    expect(v.tilt).toEqual([20, -20]);
    expect(tiltMagnitude(v.tilt)).toBeCloseTo(Math.hypot(20, 20), 5);
    const [az] = tiltAzimuth(v.tilt);
    expect(az).toBeCloseTo(Math.atan2(-20, 20), 5); // 방향이 계산된다
  });

  it('수직(0 벡터)은 방향 없음 — (0, 1) 로 정의된다', () => {
    expect(tiltAzimuth(NO_TILT)).toEqual([0, 1]);
  });

  it('어댑터가 나른 틸트가 이벤트로 그대로 흐른다 (소비자 미러 없음)', () => {
    const adapter = createPenAdapter();
    adapter.setTilt([12, -7]); // 외부 훅(HID/WM_POINTER) 주입
    const events = adapter.update({ tilt: [12, -7], contact: true }, [5, 5]);
    expect(events[0].tilt).toEqual([12, -7]);
    expect(adapter.tilt()).toEqual(events[0].tilt); // 같은 값 (이중 상태 금지)
  });

  it('라우터/싱크를 지나도 벡터가 훼손되지 않는다', () => {
    const seen = [];
    const sink = {
      name: 'ink',
      admit: () => 'now',
      handle: (evs) => seen.push(...evs),
    };
    const router = createRouter({ sinks: [sink] });
    router.dispatch(Pointer('pen', 'down', [1, 1], 0.7, [30, 40]), { now: 0 });
    router.dispatch(Pointer('pen', 'up', [2, 2], 0.7, [30, 40]), { now: 10 });
    expect(seen).toHaveLength(2);
    expect(seen[0].tilt).toEqual([30, 40]);
    expect(tiltMagnitude(seen[0].tilt)).toBeCloseTo(50, 4); // 3-4-5 삼각형
  });
});