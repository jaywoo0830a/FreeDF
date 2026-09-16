/**
 * 세션 라우터 **스텁** (idea5 전용).
 *
 * idea4 의 참조 구현(createSessionRouter)은 보류/승격/TTL/장부 정산까지 갖는다.
 * idea5 는 그 위 계약(C1~C4)만 보면 되므로 여기서는 **최소 골격**만 세운다:
 * 싱크 우선순위로 Down 을 라우팅하고, Up 으로 세션을 닫고, 정산은 개수만 센다.
 *
 * 그래도 지켜야 하는 계약은 그대로다:
 *  - 싱크는 (세션, 명시 문맥)만 본다 — 라우터 밖 상태를 샘플링하지 않는다.
 *  - 시간은 인자(now)로만 들어온다 (Date.now 금지).
 *  - Down 하나당 정산 하나 (여기서는 outcome 이름만 기록).
 */
export function createRouter({ sinks = [] } = {}) {
  let seq = 0;
  let open = null; // { id, source, down, sink, state, lastEventMs }
  const ledger = []; // { id, source, outcome, sink }

  const sinkByName = (name) => sinks.find((s) => s.name === name);

  const resolve = (id, source, outcome, sink) =>
    ledger.push({ id, source, outcome, sink });

  return {
    sinks,
    ledger,
    openSession: () =>
      open ? { id: open.id, source: open.source, sink: open.sink } : null,
    summary: () => ({
      delivered: ledger.filter((r) => r.outcome === 'delivered').length,
      refused: ledger.filter((r) => r.outcome === 'refused').length,
      open: !!open,
    }),

    dispatch(ev, ctx = {}) {
      if (ev.kind !== 'pointer') return { outcome: 'ignored' };
      const now = ctx.now ?? 0;

      if (ev.phase === 'down') {
        if (open) return { outcome: 'foreign' }; // 스텁: 중첩 Down 은 계약 밖
        seq += 1;
        const session = { id: seq, source: ev.source, down: ev, drags: [], ageMs: 0 };
        for (const s of sinks) {
          if (s.admit(session, { ...ctx, evidence: true }) !== 'now') continue;
          open = { id: seq, source: ev.source, sink: s.name, lastEventMs: now };
          resolve(seq, ev.source, 'delivered', s.name);
          s.handle([ev]);
          return { outcome: 'admitted', sink: s.name, id: seq };
        }
        resolve(seq, ev.source, 'refused', null);
        return { outcome: 'refused', id: seq };
      }

      if (!open) return { outcome: 'unrouted' };
      if (open.source !== ev.source) return { outcome: 'foreign' };
      open.lastEventMs = now;
      const sink = sinkByName(open.sink);
      sink.handle([ev]);
      if (ev.phase === 'up') open = null;
      return { outcome: 'delivered', sink: sink.name, id: null };
    },
  };
}