import { describe, expect, it } from 'vitest';
import {
  createHub,
  createWorkspace,
  Pointer,
  PointerPhase,
} from '../../src/idea4/index.js';
import { checkWellFormed, commandTypes } from './invariants.js';

/**
 * 아키텍처 설계 테스트 ⑦ — 획 시작 에지 계약 ("에지는 파괴되지 않는다").
 *
 * 2026-09-15 필기 유실 회귀(P1~P3 이식)의 사후 명세다. 근거와 전체 경위는
 * 같은 디렉터리의 README.md (획 시작 에지 계약 — 참고 문서)를 보라.
 *
 * 요지: 커맨드 경로(허브 → 툴 → begin-stroke)는 Down "에지 1회"만 평가한다.
 * 그 한 프레임이 앱 경계의 캔버스 정책 게이트(egui 소유 — 장치 폴링과
 * 다른 스레드/지연)에 막히면, 프레스 전체가 무음 유실된다
 * (tmp/0916debug.log: Pen Down 7건 중 3건이 `cmds(b/ext/end)=0/0/0`).
 *
 * 이 파일의 두 참고 모델은 이식 대상 계약의 명세다:
 *  ① createStrokeGate — 앱 경계(캔버스 정책)의 에지 보류/승격
 *     (Rust: crates/freedf/src/app/canvas/input.rs 의 PendingDown — 적용됨)
 *  ② createPenStreamAdapter — 장치 경계(펜 스트림)의 에지 보존
 *     (Rust: crates/freedf-core/src/input_devices.rs — 아직 미적용, 후속 #4)
 *
 * 참고 모델을 src/idea4 로 승격할 때는 layering.test.js 의 ALLOWED 맵에
 * 의존성을 함께 등록한다.
 */

// ── 참고 모델 ① — 앱 경계 획 게이트 ─────────────────────────────────────
//
// Rust PendingDown(input.rs)과 1:1 대응. 규칙:
//   - 게이트가 거짓인 프레임의 Down 은 **버리지 않고 보류**한다.
//   - 승격 조건(모두 참): 팬 아님 · TTL 안 · 게이트가 지금 참 · 접촉 증거.
//   - 승격 시 **원래 접촉점**을 그대로 쓰고, 이번 프레임 이벤트 **맨 앞**에
//     놓아 begin → extend 순서를 만든다.
//   - 폐기: 같은 소스의 Up · 팬 프레임 · 포커스 유예 · TTL 만료.
// contactEvidence 계산은 호출자(앱)의 몫이다 — egui primary_down 또는
// 같은 소스의 Drag(펜/패드 어댑터는 접촉 중에만 Drag 를 만든다).
export function createStrokeGate(hub, { ttlMs = 250 } = {}) {
  let pending = null; // { ev, at } — 게이트에 막힌 Down 에지
  let now = 0;

  return {
    /** 보관 중인 에지 (진단/증거 판정용). */
    get held() {
      return pending ? pending.ev : null;
    },

    /** 한 프레임 소비 — 반환값: 허브로 나간 포인터 이벤트 (순서 보존). */
    frame({
      now: t,
      gate = false,
      panning = false,
      inGrace = false,
      contactEvidence = true,
      events = [],
    }) {
      now = t;
      const out = [];

      for (const ev of events) {
        if (ev.phase === PointerPhase.DOWN) {
          if (!gate) {
            // 에지는 파괴되지 않는다 — 팬이 프레스를 가져간 경우만 예외.
            if (!panning) pending = { ev, at: t };
            continue;
          }
          pending = null; // 정상 통과한 새 프레스 — 이전 프레스의 보류는 무효
          if (inGrace) continue; // 포커스 제스처 — 의도적 삼킴 (복구 금지)
          out.push(ev);
        } else if (ev.phase === PointerPhase.UP) {
          // 프레스 종료 — 같은 소스의 보류는 승격 기회를 잃는다.
          if (pending && pending.ev.source === ev.source) pending = null;
          out.push(ev);
        } else {
          out.push(ev); // Drag 는 항상 통과 (세션 닫기 보장)
        }
      }

      if (panning || inGrace) {
        pending = null; // 팬/포커스가 이 프레스를 소유했다
      } else if (pending && gate && contactEvidence && now - pending.at <= ttlMs) {
        out.unshift(pending.ev); // 승격 — 맨 앞에 놓아 begin 이 extend 보다 먼저
        pending = null;
      } else if (pending && now - pending.at > ttlMs) {
        pending = null; // 만료 — 이 프레스의 승격 기회 소진 (다른 프레스에 붙지 않게)
      }

      out.forEach((e) => hub.emit(e));
      return out;
    },

    /** 보류 폐기 (진단/수동 제어용). 반환: 폐기된 에지. */
    cancel() {
      const ev = pending ? pending.ev : null;
      pending = null;
      return ev;
    },
  };
}

// ── 참고 모델 ② — 장치 경계 펜 스트림 어댑터 ────────────────────────────
//
// Rust input_devices.rs PenEventAdapter 의 **목표** 계약. 현재 Rust 이식은
// 위치가 없어 이벤트를 못 만들 때도 `prev_contact` 를 갱신해 에지를 영구
// 소비한다 (후속 수정 #4 — README.md 참고). 이 모델은 그 반대:
//   - 접촉 에지(Down/Up) 순간 위치가 없으면 에지를 **보관**하고,
//   - 위치가 도착하는 다음 갱신에서 원래 위상 그대로 내보낸다.
export function createPenStreamAdapter() {
  let prevContact = false;
  let held = null; // { phase } — 위치가 없어 못 낸 접촉 에지

  return {
    /**
     * @param state  펜 스트림 스냅샷 { contact, pressure?, tilt? }
     * @param point  이 순간의 커서 위치 [x, y] — 없으면 null
     * @returns 통합 어휘 포인터 이벤트들 (순서 보존)
     */
    update(state, point) {
      const out = [];
      const pressure = state.pressure ?? 1.0; // 능력 협상 — 어댑터가 기본값을 채운다
      const tilt = Math.hypot(state.tilt?.[0] ?? 0, state.tilt?.[1] ?? 0);

      if (state.contact !== prevContact) {
        const phase = state.contact ? PointerPhase.DOWN : PointerPhase.UP;
        if (point) out.push(Pointer('pen', phase, point, pressure, tilt));
        else held = { phase }; // 에지를 소비하지 않는다 — 위치를 기다린다
      } else if (state.contact && point) {
        out.push(Pointer('pen', PointerPhase.DRAG, point, pressure, tilt));
      }

      if (held && point) {
        out.unshift(Pointer('pen', held.phase, point, pressure, tilt));
        held = null;
      }
      prevContact = state.contact;
      return out;
    },
  };
}

// ── 테스트 리깅 ─────────────────────────────────────────────────────────
const rig = () => {
  const hub = createHub();
  const ws = createWorkspace(hub);
  const gate = createStrokeGate(hub);
  return { hub, ws, gate };
};

const pen = (phase, x, pressure = 0.3) => Pointer('pen', phase, [x, 0], pressure, 0);

describe('획 시작 에지 계약 — 게이트 보류/승격 (참고 모델 ①)', () => {
  it('게이트가 늦게 참이 되어도 프레스 전체가 살아난다 (2026-09-15 회귀)', () => {
    const { ws, gate } = rig();

    // 프레임 1: 접촉 Down 이 도착했지만 egui 캔버스 게이트가 아직 거짓
    // (evdev 폴링이 윈도우 메시지 루프보다 빠른 프레임 — 로그의 유실 서명).
    gate.frame({ now: 1000, gate: false, events: [pen('down', 10)] });
    expect(ws.commands).toEqual([]); // 이 순간까지는 아무 일도 없어도 된다

    // 프레임 2~3: 게이트가 여전히 거짓 — Drag 만 흐른다 (툴은 idle 이라 무시).
    gate.frame({ now: 1010, gate: false, events: [pen('drag', 12)] });
    gate.frame({ now: 1020, gate: false, events: [pen('drag', 14)] });
    expect(ws.commands).toEqual([]);

    // 프레임 4: 게이트가 참 — 보류된 Down 이 승격되어 세션이 열린다.
    gate.frame({ now: 1030, gate: true, events: [pen('drag', 16)] });
    expect(commandTypes(ws.commands)).toEqual(['begin-stroke', 'extend-stroke']);

    // 프레임 5: Up — 세션이 닫힌다.
    gate.frame({ now: 1040, gate: true, events: [pen('up', 18)] });
    expect(commandTypes(ws.commands)).toEqual(['begin-stroke', 'extend-stroke', 'end-stroke']);
    expect(checkWellFormed(ws.commands)).toBe(true);
  });

  it('첫 점은 보류된 Down 의 원래 접촉점이다 — 늦은 시작도 좌표를 잃지 않는다', () => {
    const { ws, gate } = rig();
    gate.frame({ now: 1000, gate: false, events: [pen('down', 10)] });
    gate.frame({ now: 1010, gate: true, events: [pen('drag', 99)] });

    expect(ws.commands[0].type).toBe('begin-stroke');
    expect(ws.commands[0].point).toEqual([10, 0]); // drag 지점(99)이 아니라 접촉점
    expect(ws.commands[0].pressure).toBe(0.3); // 접촉 순간의 필압
  });

  it('보류된 에지가 없으면 절대 승격하지 않는다 — 꼬리 hover 가 점을 연발하지 않는다', () => {
    const { ws, gate } = rig();

    // 정상 프레스 하나 (세션 열림 → 닫힘).
    gate.frame({ now: 1000, gate: true, events: [pen('down', 1)] });
    gate.frame({ now: 1010, gate: true, events: [pen('up', 2)] });
    const after = ws.commands.length;
    expect(after).toBe(2);

    // 펜을 뗀 뒤 같은 프레임에 도착하는 꼬리 hover/에뮬레이션 Drag 들
    // (로그 Up 프레임: mouse=0/14/1 pad=0/14/1). 게이트와 접촉 증거가
    // 모두 참이어도 **보류된 에지가 없으면** 아무 일도 일어나지 않는다 —
    // 되돌린 "힐 로직"(세션 밖 Drag → Down 승격)의 점 연발을 막는 계약.
    gate.frame({
      now: 1020,
      gate: true,
      contactEvidence: true,
      events: [pen('drag', 3), pen('drag', 4), pen('drag', 5)],
    });
    expect(ws.commands.length).toBe(after);
    expect(commandTypes(ws.commands)).toEqual(['begin-stroke', 'end-stroke']);
  });

  it('TTL 을 넘긴 보류는 승격되지 않고 버려진다 — 끝난 프레스가 나중 프레스에 붙지 않는다', () => {
    const { ws, gate } = rig();
    gate.frame({ now: 1000, gate: false, events: [pen('down', 10)] });

    // 251ms 후 — 승격 기회 소진. 이후의 Drag 는 세션을 열지 못한다.
    gate.frame({ now: 1251, gate: true, contactEvidence: true, events: [pen('drag', 12)] });
    expect(ws.commands).toEqual([]);
    expect(gate.held).toBeNull(); // 만료된 보류는 버려진다 (부활 금지)

    // 같은 보류로 다시 승격되지 않는다.
    gate.frame({ now: 1300, gate: true, contactEvidence: true, events: [pen('drag', 14)] });
    expect(ws.commands).toEqual([]);
  });

  it('팬 프레임이 프레스를 소유한다 — 팬 중에는 승격하지 않는다', () => {
    const { ws, gate } = rig();
    gate.frame({ now: 1000, gate: false, panning: false, events: [pen('down', 10)] });
    expect(gate.held).not.toBeNull();

    // 장치 판별이 뒤집혀 이 프레임이 팬 프레임이 됐다 — 잉크 금지, 보류 폐기.
    gate.frame({
      now: 1010,
      gate: true,
      panning: true,
      contactEvidence: true,
      events: [pen('drag', 12)],
    });
    expect(ws.commands).toEqual([]);
    expect(gate.held).toBeNull();

    gate.frame({ now: 1020, gate: true, contactEvidence: true, events: [pen('drag', 14)] });
    expect(ws.commands).toEqual([]); // 팬이 끝나도 되살아나지 않는다
  });

  it('같은 소스의 Up 이 도착하면 보류는 폐기된다', () => {
    const { ws, gate } = rig();
    gate.frame({ now: 1000, gate: false, events: [pen('down', 10)] });
    gate.frame({ now: 1010, gate: false, events: [pen('up', 12)] }); // 프레스 종료
    expect(gate.held).toBeNull();

    gate.frame({ now: 1020, gate: true, contactEvidence: true, events: [pen('drag', 14)] });
    expect(ws.commands).toEqual([]); // 끝난 프레스의 에지로 세션을 열지 않는다
  });

  it('접촉 증거가 없으면 게이트가 참이어도 승격하지 않는다', () => {
    const { ws, gate } = rig();
    gate.frame({ now: 1000, gate: false, events: [pen('down', 10)] });
    // 게이트는 참이지만 이 프레임에 접촉 증거가 없다 (이미 뗀 뒤일 수 있다).
    gate.frame({ now: 1010, gate: true, contactEvidence: false, events: [] });
    expect(gate.held).not.toBeNull(); // 증거가 올 때까지 기다린다

    gate.frame({ now: 1020, gate: true, contactEvidence: true, events: [pen('drag', 12)] });
    expect(commandTypes(ws.commands)).toEqual(['begin-stroke', 'extend-stroke']);
  });
});

describe('획 시작 에지 계약 — 허브 충돌 규칙의 관측성', () => {
  it('펜 점유 중 반대 소스 스트림은 drop 된다 — 반환값으로 관찰 가능해야 한다', () => {
    // 로그: 필기 중 프레임당 hub_drop=12~30 (같은 물리 펜이 evdev 'pen' 과
    // egui 'mouse'/'pad' 두 스트림으로 도착 → 한쪽이 통째로 drop).
    // drop 자체는 설계대로지만, **관측 불가능하면** 유실 원인을 알 수 없다 —
    // 이식은 emit 반환값(또는 누적 카운터)으로 반드시 관찰 가능해야 한다.
    const hub = createHub();
    expect(hub.emit(pen('down', 0))).toBe(true); // 펜이 점유
    expect(hub.emit(Pointer('mouse', 'down', [1, 0]))).toBe(false);
    expect(hub.emit(Pointer('mouse', 'drag', [2, 0]))).toBe(false);
    expect(hub.emit(Pointer('pad', 'drag', [3, 0]))).toBe(false);
    expect(hub.emit(pen('drag', 4))).toBe(true); // 같은 소스는 통과
    expect(hub.emit(pen('up', 5))).toBe(true); // 점유 해제
    expect(hub.emit(Pointer('mouse', 'down', [6, 0]))).toBe(true); // 이제 들어온다
  });
});

describe('획 시작 에지 계약 — 장치 어댑터의 에지 보존 (참고 모델 ②)', () => {
  it('접촉 Down 순간 위치가 없어도 에지는 소실되지 않는다 (이식 계약 #4)', () => {
    const adapter = createPenStreamAdapter();
    const s = { contact: true, pressure: 0.4, tilt: [0, 0] };

    // Down 순간 hover_pos 미확정 (포인터가 윈도우에 아직 안 들어온 프레임).
    // 현재 Rust: 이벤트 0건 + prev_contact 갱신 → Down 에지 **영구 소실**.
    expect(adapter.update(s, null)).toEqual([]);

    // 다음 갱신에서 위치가 도착 — 첫 이벤트는 Drag 가 아니라 **Down** 이어야 한다.
    const evs = adapter.update({ ...s, pressure: 0.5 }, [2, 2]);
    expect(evs.map((e) => e.phase)).toEqual(['down', 'drag']);
    expect(evs[0].point).toEqual([2, 2]);
  });

  it('위치 없는 Up 에지도 보존된다 (이식 계약 #4)', () => {
    const adapter = createPenStreamAdapter();
    expect(
      adapter.update({ contact: true, pressure: 0.4 }, [1, 1]).map((e) => e.phase),
    ).toEqual(['down']);

    // 펜을 뗀 순간 위치가 없었다 → Up 을 잃으면 잉크 세션이 열린 채 남는다
    // (다음 프레스가 같은 획에 이어지는 "획이 연속으로 이어지는" 증상).
    expect(adapter.update({ contact: false, pressure: 0 }, null)).toEqual([]);
    expect(
      adapter.update({ contact: false, pressure: 0 }, [5, 5]).map((e) => e.phase),
    ).toEqual(['up']);
  });

  it('에지를 어댑터가 잃지 않으면, 보류 게이트와 합쳐도 잘-형성이 유지된다', () => {
    // 어댑터(장치 경계) + 게이트(앱 경계)를 통과한 스트림은 어떤 지연 조합에도
    // 온전한 down→…→up 이어야 한다 — 두 경계의 책임이 겹치지 않는다는 증명.
    const hub = createHub();
    const ws = createWorkspace(hub);
    const gate = createStrokeGate(hub);
    const adapter = createPenStreamAdapter();

    const frames = [
      { contact: true, point: null, gate: false }, // Down 에지, 위치 없음
      { contact: true, point: [10, 0], gate: false }, // 위치 도착 (Down 보존됨)
      { contact: true, point: [20, 0], gate: true }, // 게이트가 참 — 승격
      { contact: false, point: [30, 0], gate: true }, // Up
    ];
    for (const [i, f] of frames.entries()) {
      const evs = adapter.update({ contact: f.contact, pressure: 0.4 }, f.point);
      gate.frame({ now: 1000 + i * 10, gate: f.gate, contactEvidence: true, events: evs });
    }

    expect(commandTypes(ws.commands)).toEqual(['begin-stroke', 'extend-stroke', 'end-stroke']);
    expect(checkWellFormed(ws.commands)).toBe(true);
  });
});



