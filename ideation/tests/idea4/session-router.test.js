import { describe, expect, it } from 'vitest';
import {
  createHub,
  createWorkspace,
  Action,
  Pointer,
} from '../../src/idea4/index.js';
import { checkWellFormed, commandTypes } from './invariants.js';

/**
 * 아키텍처 설계 테스트 ⑧ — 세션 라우터 ("프레스의 목적지와 완결을 소유하는 객체").
 *
 * 2026-09-15 필기 유실 회귀의 **땜질(PendingDown)을 구조로 바꾸는** 후속 설계다.
 * 같은 디렉터리 README.md 의 "9. 다음 설계" 절과 함께 읽는다.
 *
 * 문제의 뿌리: "이 프레스를 어디로 보내는가"(라우팅 — 순수)와 "지금 그릴
 * 준비가 됐는가"(준비성 — 시간 의존)가 하나의 샘플링 boolean
 * (`response.is_pointer_button_down_on()`)으로 융합된 것. 샘플링은 다른
 * 시계(egui)의 상태를 읽으므로 Down 에지 1회 판정이 경합에 지게 되고,
 * 그 보완책이 앱 경계에 손으로 붙은 보류 상태기계(땜질)가 됐다.
 *
 * 이 파일이 제시하는 이상적인 객체는 세 가지를 **타입으로** 강제한다:
 *  ① 싱크가 볼 수 있는 것은 (세션, 명시 문맥) 뿐 — 암묵적 외부 상태 샘플링이
 *     인터페이스에 존재하지 않는다.
 *  ② 라우터는 시계를 읽지 않는다 — 시간은 인자로만 들어온다 (정적 검사).
 *  ③ 모든 Down 에지는 resolution(장부)으로 정산된다 — 유실은 상태가 아니라
 *     데이터로 관측된다.
 *
 * "실패하는 인터페이스 테스트": 아래 대조군(`it.fails`)은 현재 설계의 축소
 * 모의(샘플링 게이트 + 앱 보류)가 이 계약을 **만족할 수 없음을 증명**하기
 * 위해 실패가 예상되는 테스트다. 목표 스펙은 참조 구현(createSessionRouter)
 * 에 대해 녹색이며, Rust 이식(input.rs)의 목표 계약이 된다.
 */

// ── 이상적인 객체 — 세션 라우터 (참조 구현) ──────────────────────────────
//
// 싱크 인터페이스: { name, admit(session, ctx) → 'now'|'hold'|'refuse', handle(events) }
//   - session: { id, source, down, drags, ageMs } — 판정 재료는 이것뿐
//   - ctx:     { now, evidence }                  — 라우터가 명시 전달하는 문맥
//   - 'hold' 는 세션을 보류시킨다: 라우터가 원래 접촉점과 이후 Drag 를 버퍼하고
//     매 frame 에 다시 admit 을 묻는다. 승인되면 **온전한 세션** [down, …drags]
//     이 한 번에 전달된다 (승인 전 샘플은 유실되지 않는다).
//
// 이 파일의 어떤 함수도 Date.now/performance.now/setTimeout 을 부르지 않는다 —
// 아래 정적 테스트가 검사한다. 시간은 반드시 인자(now)로 들어온다.
export function createSessionRouter({ sinks, ttlMs = 250, onAbandon = 'drop' } = {}) {
  let seq = 0;
  let open = null; // { id, source, down, drags, sink, at, state: 'held' | 'live' }
  const resolutions = []; // 모든 Down 에지의 정산 장부

  const sinkByName = (name) => sinks.find((s) => s.name === name);
  const lastPoint = () =>
    (open.drags.length ? open.drags[open.drags.length - 1].point : open.down.point);
  const sessionOf = (now) => ({
    id: open.id,
    source: open.source,
    down: open.down,
    drags: [...open.drags],
    ageMs: now - open.at,
  });
  /** Down 에지의 운명을 장부에 기록한다. 세션은 계속 열려 있을 수 있다 (승격). */
  const resolve = (outcome, extra = {}) => {
    resolutions.push({ id: open.id, source: open.source, outcome, ...extra });
  };
  const close = () => {
    open = null;
  };

  return {
    /** 현재 열린 세션 스냅샷 (진단용). */
    openSession() {
      return open ? { id: open.id, source: open.source, state: open.state, sink: open.sink } : null;
    },
    /** 지금까지의 정산 장부 — 유실은 여기서 관측된다. */
    ledger() {
      return resolutions.map((r) => ({ ...r }));
    },

    /**
     * 장치 스트림 (에지 보존 어댑터의 출력) 을 소비한다.
     * 반환: 이 이벤트의 정산 항목 { edge, outcome, ... }.
     */
    dispatch(ev, { now = 0 } = {}) {
      if (ev.phase === 'down') {
        if (open) {
          // 업스트림(어댑터) 계약 위반 방어 — 접촉 없는 Down. 열린 세션을
          // 합성 up 으로 닫고(워크스페이스의 획 경계 합성과 같은 원리) 정산한다.
          if (!open.resolved) resolve('replaced', { sink: open.sink });
          sinkByName(open.sink).handle([
            Pointer(open.source, 'up', lastPoint(), open.down.pressure),
          ]);
          close();
        }
        const id = ++seq;
        const session = { id, source: ev.source, down: ev, drags: [], ageMs: 0 };
        for (const sink of sinks) {
          const d = sink.admit(session, { now, evidence: true }); // Down 자체가 접촉 증거
          if (d === 'now') {
            open = { id, source: ev.source, down: ev, drags: [], sink: sink.name, at: now, state: 'live', resolved: true };
            resolve('delivered', { sink: sink.name });
            sink.handle([ev]);
            return { edge: 'down', id, outcome: 'delivered', sink: sink.name, now };
          }
          if (d === 'hold') {
            open = { id, source: ev.source, down: ev, drags: [], sink: sink.name, at: now, state: 'held', resolved: false };
            return { edge: 'down', id, outcome: 'held', sink: sink.name, now };
          }
          // 'refuse' → 다음 싱크 (우선순위는 sinks 배열 순서)
        }
        resolutions.push({ id, source: ev.source, outcome: 'refused', now });
        return { edge: 'down', id, outcome: 'refused', now }; // 닫힌 거절 — 세션 없음
      }

      if (!open) return { edge: ev.phase, outcome: 'unrouted', now }; // 세션 밖 Drag/Up

      if (ev.phase === 'drag') {
        open.drags.push(ev);
        if (open.state === 'held') return { edge: 'drag', id: open.id, outcome: 'buffered', now };
        sinkByName(open.sink).handle([ev]);
        return { edge: 'drag', id: open.id, outcome: 'delivered', sink: open.sink, now };
      }

      if (ev.phase === 'up') {
        // up — 라이브 세션은 즉시 전달하고 닫는다 (Down 에지는 이미 정산됨).
        const id = open.id;
        const name = open.sink;
        if (open.state === 'live') {
          sinkByName(name).handle([ev]);
          close();
          return { edge: 'up', id, outcome: 'delivered', sink: name, now };
        }
        // 보류 세션의 마지막 판정 기회 — 빠른 탭도 유실되지 않는다.
        const session = sessionOf(now);
        if (sinkByName(name).admit(session, { now, evidence: true }) === 'now') {
          sinkByName(name).handle([open.down, ...open.drags, ev]); // 온전한 세션
          resolve('delivered', { sink: name, promoted: true });
          close();
          return { edge: 'up', id, outcome: 'promoted', sink: name, now };
        }
        resolve('cancelled', { sink: name });
        close();
        return { edge: 'up', id, outcome: 'cancelled', now };
      }
      return { edge: ev.phase, outcome: 'unrouted', now };
    },

    /**
     * 프레임마다 — 보류 세션의 재판정 창구 (유일한 시간 진입점).
     * 반환: 정산 항목 | null (보류 없음 또는 계속 보류).
     */
    frame({ now, evidence = true } = {}) {
      if (!open || open.state !== 'held') return null;
      const session = sessionOf(now);
      const name = open.sink;
      const sink = sinkByName(name);
      if (now - open.at > ttlMs) {
        if (onAbandon === 'deliver') {
          sink.handle([open.down, ...open.drags]);
          resolve('delivered', { sink: name, expired: true });
          open.state = 'live'; // 만료라도 전달했으면 라이브 세션으로 계속된다
          open.resolved = true;
          return { edge: 'down', id: session.id, outcome: 'promoted', sink: name, now };
        }
        resolve('expired', { sink: name });
        close();
        return { edge: 'down', id: session.id, outcome: 'expired', now };
      }
      const d = sink.admit(session, { now, evidence });
      if (d === 'now') {
        sink.handle([open.down, ...open.drags]); // 온전한 세션 — 원래 접촉점 + 버퍼 재생
        resolve('delivered', { sink: name, promoted: true });
        open.state = 'live'; // 승격 — 세션은 닫히지 않고 라이브로 계속된다
        open.resolved = true;
        return { edge: 'down', id: session.id, outcome: 'promoted', sink: name, now };
      }
      if (d === 'refuse') {
        // 싱크가 이 프레스를 내려놓았다 (예: 팬이 소유) — 취소로 정산한다.
        resolve('cancelled', { sink: name });
        close();
        return { edge: 'down', id: session.id, outcome: 'cancelled', now };
      }
      return { edge: 'down', id: session.id, outcome: 'holding', now };
    },
  };
}

// ── 대조군 — 현재 설계(Rust input.rs a59a5e3 이전 구조)의 축소 모의 ────────
//
// "게이트는 () → bool 로 암묵 샘플링되고, 보류는 앱이 손으로 들고 있다."
// 세션 라우터의 계약과 비교해 무엇이 표현 불가능한지 보이게 하는 것이 목적.
export function createSampledGateDesign(hub) {
  let pending = null; // 앱이 손으로 든 보류 (땜질)
  let ambientGate = false; // egui 가 소유한 상태 — 앱이 매 프레임 "샘플"한다

  return {
    /** egui 쪽에서 게이트 상태를 갱신 (다른 시계 — 경합의 원점). */
    setAmbientGate(v) {
      ambientGate = v;
    },
    /** 보류된 에지를 몇 개 들고 있는지 (외부에서 관측 불가 — 이게 문제). */
    dispatch(ev, { now = 0 } = {}) {
      if (ev.phase === 'down') {
        if (ambientGate) hub.emit(ev);
        else pending = { ev, at: now };
      } else if (ev.phase === 'up') {
        if (pending && pending.ev.source === ev.source) pending = null; // 보류는 조용히 사라진다
        hub.emit(ev);
      } else {
        hub.emit(ev); // Drag 는 보류와 무관하게 흘러간다 → idle 툴이 무시 → 유실
      }
      if (pending && ambientGate) {
        hub.emit(pending.ev); // 승인 전 Drag 는 이미 사라졌다 — 복구 불가
        pending = null;
      }
      return undefined; // 정산 장부 없음 — 유실이 데이터로 관측되지 않는다
    },
    frame() {
      return undefined;
    },
  };
}

// ── 테스트 리깅 ─────────────────────────────────────────────────────────
const rig = () => {
  const hub = createHub();
  const ws = createWorkspace(hub);
  return { hub, ws };
};

const toHub = (hub) => (evs) => evs.forEach((e) => hub.emit(e));
const pen = (phase, x, pressure = 0.3) => Pointer('pen', phase, [x, 0], pressure, 0);

/** 스크립트된 싱크 — admit 판정을 순서대로 내놓고, 받은 이벤트/판정 인자를 기록한다. */
const scriptedSink = (name, decisions, hub) => {
  const calls = [];
  const received = [];
  const handle = (evs) => {
    received.push(...evs);
    if (hub) toHub(hub)(evs);
  };
  return {
    name,
    calls,
    received,
    admit(session, ctx) {
      calls.push({ session: { ...session, down: { ...session.down } }, ctx: { ...ctx } });
      return decisions.shift() ?? 'refuse';
    },
    handle,
  };
};

/** 이상적인 최종 상태의 잉크 싱크 — 순수 기하만 본다 (시간/외부 상태 0). */
const geometrySink = (name, hub, contains) => ({
  name,
  admit: (session) => (contains(session.down.point) ? 'now' : 'refuse'),
  handle: toHub(hub),
});

/** 접촉 증거(같은 소스의 Drag 버퍼)를 기다리는 잉크 싱크 — hold 가 필요한 이유의 표본. */
const evidenceSink = (hub) => ({
  name: 'ink',
  admit: (session) => (session.drags.length > 0 ? 'now' : 'hold'),
  handle: toHub(hub),
});

/** UI 싱크 — 잉크가 거절한 프레스를 액션으로 소화 (툴바 프레스). */
const toolbarSink = (hub) => ({
  name: 'toolbar',
  admit: () => 'now',
  handle: (evs) => evs.forEach((e) => e.phase === 'down' && hub.emit(Action('toolbar', 'tool:eraser'))),
});

describe('세션 라우터 — 인터페이스 계약 (실패를 표현 불가능하게 만든다)', () => {
  it('싱크가 볼 수 있는 것은 (세션, 명시 문맥) 뿐이다 — 암묵적 외부 상태 샘플링이 없다', () => {
    const { hub } = rig();
    const ink = scriptedSink('ink', ['refuse']);
    const router = createSessionRouter({ sinks: [ink] });

    router.dispatch(pen('down', 10), { now: 1000 });

    expect(ink.calls).toHaveLength(1);
    const { session: argSession, ctx: argCtx } = ink.calls[0];
    // 판정 재료 = 이벤트가 스스로 안고 있는 것 + 라우터가 명시 전달한 문맥.
    // response.dragged() 류의 "지금 다른 시계는 뭐라고 하지?" 질문은
    // 이 시그니처로는 표현 자체가 되지 않는다.
    expect(Object.keys(argSession).sort()).toEqual(['ageMs', 'down', 'drags', 'id', 'source']);
    expect(Object.keys(argCtx).sort()).toEqual(['evidence', 'now']);
  });

  it('라우터는 시계를 읽지 않는다 — 시간은 인자로만 들어온다 (정적 검사)', () => {
    // layering.test.js 와 같은 정적 검사 방식. 구현이 Date.now 등을 부르는
    // 순간 "샘플링"이 다시 태어난다 — 시간은 반드시 dispatch/frame 의 인자.
    const src = createSessionRouter.toString();
    expect(/Date\.now|performance\.now|setTimeout|setInterval/.test(src)).toBe(false);
  });

  it('모든 Down 에지는 정산 장부를 갖는다 — 유실은 상태가 아니라 데이터로 관측된다', () => {
    const { hub, ws } = rig();
    // hold → 승격 → live → 계약 위반(접촉 없는 Down) → 닫힌 거절
    const ink = scriptedSink('ink', ['hold', 'now', 'refuse'], hub);
    const router = createSessionRouter({ sinks: [ink] });

    router.dispatch(pen('down', 10), { now: 1000 }); // hold
    router.dispatch(pen('drag', 12), { now: 1010 }); // 버퍼
    router.frame({ now: 1020 }); // 승격 — [down, drag] 재생
    router.dispatch(pen('drag', 14), { now: 1030 }); // live 전달
    router.dispatch(pen('down', 20), { now: 1040 }); // 위반 — 세션 교체 + 새 Down 은 거절
    router.dispatch(pen('drag', 22), { now: 1050 }); // unrouted
    router.dispatch(pen('up', 24), { now: 1060 }); // unrouted

    const ledger = router.ledger();
    expect(ledger.map((r) => r.outcome)).toEqual(['delivered', 'refused']); // Down 2건 = 정산 2건
    expect(
      ledger.every((r) =>
        ['delivered', 'refused', 'cancelled', 'expired', 'replaced'].includes(r.outcome),
      ),
    ).toBe(true);
    // 세션 교체는 워크스페이스의 획 경계 합성과 같은 원리로 닫힌다 — 잘-형성 유지.
    expect(commandTypes(ws.commands)).toEqual([
      'begin-stroke',
      'extend-stroke',
      'extend-stroke',
      'end-stroke',
    ]);
    expect(checkWellFormed(ws.commands)).toBe(true);
  });
});

describe('세션 라우터 — 라우팅 계약', () => {
  it('Down 은 우선순위대로 단 하나의 싱크로 라우팅된다 (툴바 프레스는 잉크가 아니다)', () => {
    const { hub, ws } = rig();
    // 캔버스 밖 (x<0) — 잉크 거절 → 다음 싱크인 툴바가 즉시 승인.
    const ink = scriptedSink('ink', ['refuse']);
    const received = [];
    const toolbar = {
      name: 'toolbar',
      admit: () => 'now',
      handle: (evs) => {
        received.push(...evs);
        evs.forEach((e) => e.phase === 'down' && hub.emit(Action('toolbar', 'tool:eraser')));
      },
    };
    const router = createSessionRouter({ sinks: [ink, toolbar] });

    router.dispatch(pen('down', -5), { now: 1000 });
    router.dispatch(pen('up', -5), { now: 1010 });

    expect(ink.calls).toHaveLength(1); // 우선순위 1위만 먼저 물었다
    expect(received.map((e) => e.phase)).toEqual(['down', 'up']);
    expect(ws.activeName()).toBe('eraser'); // 액션으로 소화 — 커맨드 스트림 오염 없음
    expect(commandTypes(ws.commands)).toEqual([]);
    expect(router.ledger()[0].outcome).toBe('delivered');
  });

  it('거절은 닫힌 거절이다 — 세션 없는 Drag 가 아무리 와도 세션은 생기지 않는다', () => {
    // 점 연발(힐 로직)의 구조적 차단: 승격의 원점은 언제나 "보류된 Down 에지" —
    // 꼬리 hover Drag 는 unrouted 로 정산될 뿐, 세션을 만들 수 없다.
    const { hub, ws } = rig();
    const ink = scriptedSink('ink', ['refuse']);
    const router = createSessionRouter({ sinks: [ink] });

    router.dispatch(pen('down', -5), { now: 1000 });
    for (let i = 0; i < 14; i++) router.dispatch(pen('drag', i), { now: 1000 + i }); // 꼬리 hover
    router.dispatch(pen('up', -5), { now: 1020 });

    expect(ws.commands).toEqual([]);
    expect(router.ledger()).toEqual([{ id: 1, source: 'pen', outcome: 'refused', now: 1000 }]);
  });

  it('hold 된 세션은 승인 시 원래 접촉점과 버퍼된 Drag 를 함께 재생한다', () => {
    const { hub, ws } = rig();
    const router = createSessionRouter({ sinks: [evidenceSink(hub)] });

    // 게이트가 거짓인 프레임에 Down — 에지는 버려지지 않고 보류된다.
    router.dispatch(pen('down', 10), { now: 1000 });
    router.dispatch(pen('drag', 12), { now: 1010 }); // 버퍼 — 싱크에 새지 않는다
    router.dispatch(pen('drag', 14), { now: 1020 });
    expect(ws.commands).toEqual([]);
    expect(router.openSession().state).toBe('held');

    // 증거가 생겨 승인 — 싱크는 **온전한 세션**을 한 번에 본다.
    router.frame({ now: 1030 });
    expect(commandTypes(ws.commands)).toEqual(['begin-stroke', 'extend-stroke', 'extend-stroke']);
    expect(ws.commands[0].point).toEqual([10, 0]); // 원래 접촉점
    expect(ws.commands[1].point).toEqual([12, 0]); // 버퍼된 첫 샘플 — 유실 없음
    expect(ws.commands[2].point).toEqual([14, 0]);
  });

  it('빠른 탭 — Up 프레임에야 승인되어도 세션은 완결 전달된다', () => {
    const { hub, ws } = rig();
    // 승인이 Up 프레임에야 나오는 싱크 (egui 캔버스가 늦게 소유를 인정하는 상황).
    const ink = scriptedSink('ink', ['hold', 'hold', 'now'], hub);
    const router = createSessionRouter({ sinks: [ink] });

    router.dispatch(pen('down', 10), { now: 1000 }); // hold
    router.frame({ now: 1010 }); // 아직 준비 안 됨 — hold 유지
    router.dispatch(pen('up', 12), { now: 1020 }); // 마지막 판정 기회 — 승인

    // 땜질에서는 이 탭이 사라진다 (Up 이 보류를 조용히 파괴). 라우터는
    // 완결 세션 [down, up] 을 전달해 "점 하나"라도 정산한다.
    expect(commandTypes(ws.commands)).toEqual(['begin-stroke', 'end-stroke']);
    expect(ws.commands[0].point).toEqual([10, 0]);
  });

  it('TTL 을 넘긴 보류는 만료로 정산된다 — 끝난 프레스가 나중 프레스에 붙지 않는다', () => {
    const { hub, ws } = rig();
    const router = createSessionRouter({ sinks: [evidenceSink(hub)] });

    router.dispatch(pen('down', 10), { now: 1000 });
    router.frame({ now: 1000 + 251 }); // 만료
    expect(router.ledger()).toEqual([
      { id: 1, source: 'pen', outcome: 'expired', sink: 'ink' },
    ]);
    expect(router.openSession()).toBeNull();

    router.frame({ now: 1300 }); // 부활 금지
    router.dispatch(pen('drag', 99), { now: 1310 }); // unrouted
    expect(ws.commands).toEqual([]);
  });

  it('hold 중 싱크가 refuse 로 전환하면(팬 소유) 세션은 취소 정산된다', () => {
    const { hub, ws } = rig();
    const ink = scriptedSink('ink', ['hold', 'refuse']); // 판정 뒤집힘 — 팬 프레임
    const router = createSessionRouter({ sinks: [ink] });

    router.dispatch(pen('down', 10), { now: 1000 });
    router.frame({ now: 1010 });

    expect(router.ledger()).toEqual([
      { id: 1, source: 'pen', outcome: 'cancelled', sink: 'ink' },
    ]);
    expect(router.openSession()).toBeNull();
    expect(ws.commands).toEqual([]);
  });

  it('onAbandon: deliver — 만료 세션도 버리지 않고 전달할 수 있다 (정책은 데이터)', () => {
    const { hub, ws } = rig();
    const router = createSessionRouter({ sinks: [evidenceSink(hub)], onAbandon: 'deliver' });

    router.dispatch(pen('down', 10), { now: 1000 });
    router.frame({ now: 1300 }); // TTL 지남 — 그래도 전달

    expect(commandTypes(ws.commands)).toEqual(['begin-stroke']);
    expect(router.ledger()[0].outcome).toBe('delivered');
  });
});

describe('대조군 — 샘플링 게이트 설계는 위 계약을 만족할 수 없다', () => {
  // it.fails: "실패해야 통과"하는 테스트. 현재 구조의 축소 모의가 세션 라우터의
  // 계약을 달성할 수 없음을 실행으로 증명한다 — 구조를 바꾸라는 명세.
  it.fails('승인 전 Drag 는 버퍼되지 않고 유실된다 (획 앞부분 결손)', () => {
    const { hub, ws } = rig();
    const legacy = createSampledGateDesign(hub);

    legacy.setAmbientGate(false);
    legacy.dispatch(pen('down', 10), { now: 1000 }); // 보류
    legacy.dispatch(pen('drag', 12), { now: 1010 }); // idle 툴이 무시 → 유실
    legacy.dispatch(pen('drag', 14), { now: 1020 }); // 유실
    legacy.setAmbientGate(true);
    legacy.dispatch(pen('drag', 16), { now: 1030 }); // 승격 + 이 시점 Drag 만 살아남는다
    legacy.dispatch(pen('up', 18), { now: 1040 });

    // 계약: 온전한 세션 — 승인 전 샘플(12, 14)도 살아있어야 한다.
    expect(commandTypes(ws.commands)).toEqual([
      'begin-stroke',
      'extend-stroke',
      'extend-stroke',
      'extend-stroke',
      'end-stroke',
    ]);
  });

  it.fails('에지 정산(장부)이 관측 불가능하다 — 유실이 데이터로 드러나지 않는다', () => {
    const { hub } = rig();
    const legacy = createSampledGateDesign(hub);

    const r = legacy.dispatch(pen('down', 10), { now: 1000 });
    // 계약: 모든 dispatch 는 정산 항목을 반환한다 (유실 관측의 최소 조건).
    expect(r).toEqual({ edge: 'down', outcome: expect.any(String) });
  });
});
