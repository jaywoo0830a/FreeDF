import { describe, expect, it } from 'vitest';
import { createSessionRouter } from './session-router.test.js';

/**
 * 교육용 스텁 — 2026-09-15 필기 유실 버그를 "두 개의 시계"만 있는 미니 세계로 재현.
 *
 * 버그의 전부는 이 한 문장이다:
 *   "프레스"라는 하나의 사실이 펜(evdev)에게는 즉시 보이지만,
 *   UI(egui)에게는 한 프레임 늦게 보인다 — 그리고 Down 에지는 딱 한 번만 판정된다.
 *
 * ── 핵심 개념: 에지(edge)와 프레임 ─────────────────────────────────────────
 *
 *   에지   = 상태가 **바뀌는 전환점** (안 눌림 → 눌림). 딱 한 번 발생하고 지나간다.
 *            코드: PointerPhase::Down / PointerPhase::Up
 *   레벨   = 상태가 **그런 채 지속되는 구간**, 또는 지속 동안 계속 들어오는 샘플.
 *            코드: PointerPhase::Drag (접촉 중 계속 도착), uiOwns 같은 레벨 질의
 *   프레임 = 이 세상을 **관측하는 유일한 시점**. 세상은 프레임마다 한 번씩만 본다.
 *
 * 이 둘의 연관이 버그의 무대다:
 *   ① 에지는 프레임과 무관하게 발생한다 — 장치 시계는 프레임 사이에서도 흐른다.
 *   ② 그러나 에지를 **관측**할 수 있는 곳은 프레임뿐이다.
 *   ③ 에지는 소모성이다 — 발생한 프레임에서 관측되지 못하면 **영원히 사라진다**.
 *      (레벨은 다음 프레임에 다시 물어볼 수 있지만, 에지는 다시 오지 않는다.)
 *
 *      펜: 누름(에지 발생, 프레임 사이) ─► f1에서 관측 ─► f2에는 이미 존재하지 않음
 *
 * 따라서 에지를 다루는 정책은 이 둘 중 하나여야 한다:
 *   (a) 매 프레임 다시 물어본다      → 에지를 레벨로 강등. 놓침은 없지만
 *                                      "발생 시점"이 아니라 "발견 시점"에 시작 (1막)
 *   (b) 에지를 보존했다가 판정한다   → 세션. 에지의 소모성 자체를 무력화 (3·4막)
 * 리팩터 후 코드는 (a)도 (b)도 아닌 **"에지를 한 번 판정하고 그냥 버린다"**였고,
 * 그 한 번이 레벨 질의(느린 UI 시계)에 막히면 프레스 전체가 사라졌다 (2막).
 *
 * 아래 4막은 같은 프레스(누르고 → x=10,12,14 로 끌고 → 뗀다)를 시대별 설계에
 * 먹여 본다. 읽는 순서대로 따라가면 왜 땜질이 필요했고, 왜 라우터가 답인지 안다.
 *
 *   1막  매 프레임 폴링 (리팩터 전)   → 유실 없음, 대신 늦게 시작
 *   2막  에지 1회 판정   (리팩터 후)  → 프레스 전체가 무음 유실 ★버그★
 *   3막  PendingDown 땜질 (현재 수정) → 시작점은 살지만 승격 전 샘플이 유실
 *   4막  세션 라우터     (제안 설계)  → 온전한 세션
 */

// ── 미니 세계: 펜 스레드와 UI 프레임, 두 개의 시계 ──────────────────────────
//
//  - world.pen(...)     : 펜 스레드가 이벤트를 만든다 (지금 즉시 — 프레임 무관)
//  - world.frame()      : UI 프레임이 돈다
//      1) 이번 프레임에 도착한 펜 이벤트를 소비한다
//      2) 등록된 핸들러(on)를 부른다
//      3) 프레임 끝 훅(onFrameEnd)을 부른다
//  - world.uiOwns       : egui 상태 "캔버스가 이 포인터를 점유했다".
//                         프레스를 lag 프레임 늦게야 안다 (경합의 원점)
function makeWorld({ lag = 1 } = {}) {
  let frameNo = 0;
  let ownsFrom = Infinity; // 이 프레임부터 egui가 점유를 안다
  let contact = false; // 펜이 접촉 중인가 (장치 진실 — 즉시 갱신)
  let curX = 0;
  const queue = []; // { at, kind, x } — at 프레임에 도착
  const handlers = [];
  const frameEndHooks = [];

  return {
    get frameNo() { return frameNo; },
    get uiOwns() { return frameNo >= ownsFrom; }, // 레벨 질의 — 프레임마다 몇 번이든 다시 물을 수 있다 (에지와의 결정적 차이)
    get contact() { return contact; }, // 장치 시계 (빠름)
    get x() { return curX; },
    on(fn) { handlers.push(fn); },
    onFrameEnd(fn) { frameEndHooks.push(fn); },
    /**
     * 펜 스레드 — 에지(Down/Up)와 레벨 샘플(Drag)이 **발생하는** 곳.
     * 실제로는 프레임 사이 어디서든 발생하지만, 이 스텁에서는 "다음 관측
     * 시점에 도착"으로 모델링한다 — 관측(프레임)보다 먼저 일어난다는 것만
     * 중요하다. Down 에지와 동시에 egui의 레벨(ownsFrom)이 lag 프레임 뒤에
     * 켜진다 — 이 시간차가 모든 문제의 씨앗이다.
     */
    pen(kind, x = 0) {
      queue.push({ at: frameNo + 1, kind, x });
      if (kind === 'down') ownsFrom = frameNo + 1 + lag; // egui는 lag 프레임 늦게 안다
    },
    /** UI 프레임 — 에지를 관측할 수 있는 **유일한** 시점. 여기서 못 보면 끝이다. */
    frame() {
      frameNo += 1;
      // 이번 프레임에 도착한 이벤트를 큐에서 **꺼내** 소비한다 (재전달 없음).
      const due = [];
      while (queue.length && queue[0].at <= frameNo) due.push(queue.shift());
      for (const e of due) {
        if (e.kind === 'down') contact = true;
        if (e.kind === 'up') contact = false;
        curX = e.x;
        for (const fn of handlers) fn(e);
      }
      for (const fn of frameEndHooks) fn();
    },
  };
}

/** 표준 프레스: down → drag(12) → drag(14) → up, 프레임 하나씩 띄워서. */
function runPress(world) {
  world.pen('down', 10);
  world.frame();
  world.pen('drag', 12);
  world.frame();
  world.pen('drag', 14);
  world.frame();
  world.pen('up', 14);
  world.frame();
}

describe('0915 버그 케이스 — 두 개의 시계 이야기', () => {
  // ── 1막: 매 프레임 폴링 (리팩터 전 설계) ────────────────────────────────
  it('1막 폴링: 게이트를 매 프레임 다시 물으니 유실은 없다 — 대신 늦게 시작한다', () => {
    const world = makeWorld({ lag: 1 });
    const cmds = [];
    let stroke = null;

    world.on((ev) => {
      if (ev.kind === 'drag' && stroke) cmds.push(['extend', ev.x]);
    });
    world.onFrameEnd(() => {
      // ★ 매 프레임 다시 묻는다: "접촉 중이고, 게이트도 참인가?"
      if (world.contact && world.uiOwns && !stroke) {
        stroke = [];
        cmds.push(['begin', world.x]); // 현재 위치 — x=10은 이미 지났다
      } else if (!world.contact && stroke) {
        cmds.push(['end']);
        stroke = null;
      }
    });

    runPress(world);

    // 획은 살아남는다. 시작이 x=10이 아니라 x=12(한 프레임 늦음)인 게 전부.
    expect(cmds).toEqual([['begin', 12], ['extend', 14], ['end']]);
  });

  // ── 2막: 에지 1회 판정 (리팩터 후 — 버그) ───────────────────────────────
  // 헤더 개념 ③이 그대로 실현되는 장면: 소모성 에지 + 늦게 켜지는 레벨 질의.
  // 판정이 에지와 같은 프레임에서 "한 번" 일어나므로, 관측 시점의 운이 곧 결과다.
  it('2막 에지 1회: Down 에지는 딱 한 번 판정되고, 그 순간 게이트가 늦으면 프레스 전체가 유실된다', () => {
    const world = makeWorld({ lag: 1 });
    const cmds = [];
    let stroke = false;

    world.on((ev) => {
      if (ev.kind === 'down') {
        // ★ 딱 한 번의 판정 기회. 지금 게이트가 거짓이면 영원히 폐기.
        if (world.uiOwns) { cmds.push(['begin', ev.x]); stroke = true; }
        // else: 버려진다 — 이 프레스는 세상에 흔적을 남기지 않는다
      } else if (ev.kind === 'drag') {
        if (stroke) cmds.push(['extend', ev.x]); // 세션이 없으니 조용히 무시됨
      } else if (ev.kind === 'up') {
        if (stroke) cmds.push(['end']);
      }
    });

    runPress(world);

    // Down(f1)에선 게이트가 아직 거짓, 게이트가 참이 된 건 f2 — 이미 늦었다.
    expect(world.uiOwns).toBe(true); // 정보는 결국 도착했다...
    expect(cmds).toEqual([]); // ...하지만 물어볼 에지가 없다. 무음 유실.
  });

  it('2막′ 실측 재현: 펜 7번 눌렀는데 게이트가 늦은 3번이 유실된다 (tmp/0916debug.log)', () => {
    // 로그의 실측: Pen Down 7건 중 3건이 cmds(b/ext/end)=0/0/0.
    // lag=0 (같은 프레임에 둘 다 앎)이면 살고, lag=1이면 죽는다.
    const survives = (lag) => {
      const world = makeWorld({ lag });
      const cmds = [];
      let stroke = false;
      world.on((ev) => {
        if (ev.kind === 'down') { if (world.uiOwns) { cmds.push('begin'); stroke = true; } }
        else if (ev.kind === 'drag' && stroke) cmds.push('extend');
        else if (ev.kind === 'up' && stroke) cmds.push('end');
      });
      runPress(world);
      return cmds.length > 0;
    };

    const lags = [0, 0, 1, 0, 1, 0, 1]; // 7번의 프레스 — 3번만 운이 나빴다
    const lost = lags.filter((l) => !survives(l)).length;
    expect(lost).toBe(3);
  });

  // ── 3막: PendingDown 땜질 (현재 적용된 수정) ────────────────────────────
  it('3막 땜질: 시작점은 복구되지만, 승격 전에 흘러간 Drag 샘플은 되돌릴 수 없다', () => {
    const world = makeWorld({ lag: 1 });
    const cmds = [];
    let stroke = false;
    let pending = null; // 앱이 손으로 든 보류 (input.rs의 PendingDown)

    world.on((ev) => {
      if (ev.kind === 'down') {
        if (world.uiOwns) { cmds.push(['begin', ev.x]); stroke = true; }
        else pending = ev; // 에지는 버리지 않고 보관한다
      } else if (ev.kind === 'drag') {
        // ★ 보류 중인 Drag는 "세션 없음"으로 흘려보내진다 → 유실 (복구 불가)
        if (stroke) cmds.push(['extend', ev.x]);
      } else if (ev.kind === 'up') {
        if (stroke) cmds.push(['end']);
        pending = null; // 보류는 조용히 파괴된다 (빠른 탭 유실의 씨앗)
      }
    });
    world.onFrameEnd(() => {
      // 승격: 게이트가 지금 참이고 접촉 증거가 있으면 원래 접촉점으로 시작
      if (pending && world.uiOwns && world.contact) {
        cmds.push(['begin', pending.x]);
        pending = null;
        stroke = true;
      }
    });

    runPress(world);

    // begin(x=10) — 땜질이 시작점은 살렸다. 하지만 f2의 drag(12)는 이미
    // "세션 없음"으로 흘러가 유실됐다: 획 앞부분이 결손된다.
    expect(cmds).toEqual([['begin', 10], ['extend', 14], ['end']]);
  });

  // ── 4막: 세션 라우터 (제안 설계) ────────────────────────────────────────
  it('4막 라우터(기하 즉담): 판정이 이벤트만 보면 경합 자체가 없다 — 보류조차 필요 없다', () => {
    const world = makeWorld({ lag: 1 }); // UI는 여전히 한 프레임 늦지만...
    const cmds = [];
    const ink = {
      name: 'ink',
      // 캔버스 위인가? = 이벤트의 좌표만으로 즉답 (egui 상태를 묻지 않음)
      admit: () => 'now',
      handle: (evs) => evs.forEach((e) => {
        if (e.phase === 'down') cmds.push(['begin', e.point[0]]);
        else if (e.phase === 'drag') cmds.push(['extend', e.point[0]]);
        else cmds.push(['end']);
      }),
    };
    const router = createSessionRouter({ sinks: [ink] });
    world.on((ev) => router.dispatch(
      { source: 'pen', phase: ev.kind, point: [ev.x, 0], pressure: 0.5 },
      { now: world.frameNo * 16 },
    ));
    world.onFrameEnd(() => router.frame({ now: world.frameNo * 16 }));

    runPress(world);

    // Down 에지가 온 프레임(f1)에 즉시 시작 — 온전한 세션.
    expect(cmds).toEqual([['begin', 10], ['extend', 12], ['extend', 14], ['end']]);
  });

  it('4막′ 라우터(늦는 정책): 그래도 hold 중 Drag는 버퍼되어 온전하게 재생된다', () => {
    const world = makeWorld({ lag: 1 });
    const cmds = [];
    let asked = 0;
    const ink = {
      name: 'ink',
      // 두 판정(Down 프레임 + 첫 프레임)까지 미루는 정책 — hold 중 Drag가
      // 실제로 버퍼되는 상황 (게이트가 두 프레임 늦게 참이 되는 모의)
      admit: () => (++asked <= 2 ? 'hold' : 'now'),
      handle: (evs) => evs.forEach((e) => {
        if (e.phase === 'down') cmds.push(['begin', e.point[0]]);
        else if (e.phase === 'drag') cmds.push(['extend', e.point[0]]);
        else cmds.push(['end']);
      }),
    };
    const router = createSessionRouter({ sinks: [ink] });
    world.on((ev) => router.dispatch(
      { source: 'pen', phase: ev.kind, point: [ev.x, 0], pressure: 0.5 },
      { now: world.frameNo * 16 },
    ));
    world.onFrameEnd(() => router.frame({ now: world.frameNo * 16 }));

    runPress(world);

    // f1: down → hold. f2: drag(12)는 버퍼 (유실 아님!). f2 프레임 끝: 승격 —
    // 라우터가 **버퍼까지 재생**해 [down@10, drag@12]를 한 번에 전달한다.
    // f3: drag(14) 라이브. 땜질과 달리 결손이 없다.
    expect(cmds).toEqual([['begin', 10], ['extend', 12], ['extend', 14], ['end']]);
    // 정산 장부 — 유실이 상태가 아니라 데이터로 관측된다.
    expect(router.ledger()).toEqual([
      { id: 1, source: 'pen', outcome: 'delivered', sink: 'ink', promoted: true },
    ]);
  });
});
