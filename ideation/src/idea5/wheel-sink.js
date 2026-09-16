/**
 * C1 — 원형 팔레트(휠)를 **싱크**로 승격 (idea5).
 *
 * Rust 이식: `crates/freedf/src/app/input/wheel_sink.rs`.
 *
 * 문제: 오버레이가 egui 원시 이벤트(`frame_tap_pos`)를 다시 읽어 탭을 판정했다.
 * 같은 프레스를 잉크와 **다른 시계**로 해석하는 것이고, 그 뒤처리로 삼킴 표식과
 * 적체된 이벤트(유령 점)가 따라왔다.
 *
 * 계약:
 *  - 히트테스트는 **순수 기하** (`wheelGeom().hit(point)`) — 렌더가 그린 기하와
 *    같은 값을 앱이 주입한다.
 *  - 싱크는 앱 상태를 모른다: 판정 결과는 **의도**(intent)로 남긴다.
 *  - 열려 있는 동안 프레스는 휠 소유 (우선순위 1) — 잉크 세션은 열리지 않는다.
 */

/** 탭이 휠의 어디에 닿았는지. */
export const WheelHit = {
  CENTER: 'center',
  SWATCH: 'swatch',
  BACKPLATE: 'backplate',
  OUTSIDE: 'outside',
};

/** 휠 기하 — 히트테스트 수학의 유일한 소유자 (egui 타입 없음). */
export const wheelGeom = ({
  center = [0, 0],
  backR = 56,
  ringR = 34,
  swatchR = 12,
  centerR = 15,
  ringLen = 0,
} = {}) => {
  const dist = (p) => Math.hypot(p[0] - center[0], p[1] - center[1]);
  const swatchPos = (i) => {
    if (!ringLen) return center;
    // 12시 방향부터 시계 방향.
    const a = -Math.PI / 2 + (2 * Math.PI * i) / ringLen;
    return [center[0] + Math.cos(a) * ringR, center[1] + Math.sin(a) * ringR];
  };
  return {
    center,
    backR,
    ringR,
    swatchR,
    centerR,
    ringLen,
    swatchPos,
    hit(p) {
      if (dist(p) <= centerR) return { kind: WheelHit.CENTER };
      if (dist(p) > backR) return { kind: WheelHit.OUTSIDE };
      for (let i = 0; i < ringLen; i += 1) {
        const s = swatchPos(i);
        if (Math.hypot(p[0] - s[0], p[1] - s[1]) <= swatchR + 3) {
          return { kind: WheelHit.SWATCH, index: i };
        }
      }
      return { kind: WheelHit.BACKPLATE };
    },
  };
};

/** 휠 싱크 — 앱이 기하/열림 상태를 주입하고, 의도를 회수한다. */
export const createWheelSink = () => {
  let open = false;
  let geom = null;
  const intents = [];
  return {
    name: 'wheel',
    setOpen(v) {
      open = v;
    },
    setGeometry(g) {
      geom = g;
    },
    drainIntents() {
      return intents.splice(0, intents.length);
    },
    admit() {
      // 열려 있으면 **모든** 프레스가 휠 것이다 (안/밖은 handle 이 판정).
      return open ? 'now' : 'refuse';
    },
    handle(events) {
      const down = events.find((e) => e.phase === 'down');
      if (!down) return;
      if (!geom) {
        intents.push({ type: 'close' });
        return;
      }
      const hit = geom.hit(down.point);
      if (hit.kind === WheelHit.CENTER) {
        intents.push({ type: 'select-tool', tool: 'eraser' });
        intents.push({ type: 'close' });
      } else if (hit.kind === WheelHit.SWATCH) {
        intents.push({ type: 'pick-swatch', index: hit.index });
        intents.push({ type: 'close' });
      } else {
        // 뒷판/바깥 = 닫기만 (잉크 없음).
        intents.push({ type: 'close' });
      }
    },
  };
};

/** 캔버스 목적지 복합 — 우선순위의 소유자 (휠 → 잉크). */
export const canvasSinks = ({ wheel, ink }) => {
  let target = null;
  return {
    name: 'canvas',
    lastAdmitted: () => target,
    admit(session, ctx) {
      if (wheel.admit(session, ctx) === 'now') {
        target = 'wheel';
        return 'now';
      }
      if (ink.admit(session, ctx) === 'now') {
        target = 'ink';
        return 'now';
      }
      target = null;
      return 'refuse';
    },
    handle(events) {
      (target === 'wheel' ? wheel : ink).handle(events);
    },
  };
};