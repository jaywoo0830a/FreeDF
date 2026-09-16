/**
 * C2/C3 — 틸트는 어휘가 나르고(벡터), 능력은 장치가 대답한다 (idea5).
 *
 * Rust 이식: `crates/freedf-core/src/input_devices.rs` (어댑터),
 * `crates/freedf-core/src/pen_input.rs` (`PenCapabilities`),
 * `crates/freedf-core/src/input_events.rs` (틸트 벡터/방위각).
 */

import { Pointer, tiltAzimuth, NO_TILT } from '../idea4/events.js';

/** 장치 능력 — 능력 협상의 입력. 모르면 낙관(보고하면 쓴다). */
export const capabilities = ({ hasTilt = true, hasPressure = true } = {}) => ({
  hasTilt,
  hasPressure,
});

/**
 * 펜 스트림 어댑터 **스텁** — 장치 상태의 소유자.
 *
 * 계약:
 *  - 틸트 벡터를 조건화(노이즈 필터)해 보관하고, 그 값을 이벤트에 **그대로** 싣는다
 *    (이벤트가 나르는 틸트 = 렌더가 보는 틸트 — 이중 상태 금지).
 *  - 능력은 어댑터가 대답한다 (`tiltSupported`) — 스트림 존재로 근사하지 않는다.
 */
export const createPenAdapter = ({ caps = capabilities() } = {}) => {
  let tilt = NO_TILT;
  let contact = false;
  return {
    capabilities: () => caps,
    tiltSupported: () => caps.hasTilt,
    tilt: () => tilt,
    /** 외부 훅(HID/WM_POINTER)의 틸트 주입 — 장치 상태의 소유자를 거친다. */
    setTilt(v) {
      tilt = [Math.max(-90, Math.min(90, v[0])), Math.max(-90, Math.min(90, v[1]))];
    },
    /** 장치 스냅샷 1건 → 통합 이벤트들 (접촉 에지 보존은 스텁에서 생략). */
    update({ tilt: rawTilt = NO_TILT, pressure = 1, contact: c = false }, point) {
      tilt = [rawTilt[0], rawTilt[1]];
      const out = [];
      if (point) {
        const phase = c === contact ? 'drag' : c ? 'down' : 'up';
        if (phase === 'down' || phase === 'up') contact = c;
        out.push(Pointer('pen', phase, point, pressure, tilt));
      }
      return out;
    },
  };
};

/** 렌더 커서의 방위각 — **능력 질의**로 분기한다 (스트림 존재 근사 금지). */
export const cursorAzimuth = ({ adapter, leftHanded = false, defaultRight = -0.6 }) => {
  if (adapter.tiltSupported()) {
    const [az, cosPitch] = tiltAzimuth(adapter.tilt());
    // 수직(0 벡터)일 때는 방향이 없다 — 손잡이 기본값으로 되돌린다.
    if (Math.hypot(...adapter.tilt()) < 1e-3) {
      return [leftHanded ? -Math.PI - defaultRight : defaultRight, 1];
    }
    return [az, cosPitch];
  }
  return [leftHanded ? -Math.PI - defaultRight : defaultRight, 1];
};