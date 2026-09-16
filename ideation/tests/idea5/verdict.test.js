import { describe, expect, it } from 'vitest';
import {
  Level,
  ledgerFacts,
  liveFlat,
  strokeVerdict,
  createRouter,
  Pointer,
} from '../../src/idea5/index.js';

/**
 * C4 — 진단은 하나: 설정 + 측정 + **라우터 장부** (idea5).
 *
 * Rust 대응: `app/canvas/diagnostics.rs` (+ `SessionRouter::summary()`).
 *
 * 지키는 것: 진단이 "설정/장치 사실"을 모르면 **오진**한다 — writing.log 는
 * 필압 민감도가 꺼진 상태를 "OTD 연결/필압 소스 확인"으로 진단했다.
 * 그리고 "입력이 이상하다"와 "세션이 유실됐다"를 장부로 구분한다.
 */
const facts = (over = {}) => ({
  nPt: 20,
  pressureMin: 0.1,
  pressureMax: 0.8,
  widthMin: 0.5,
  widthMax: 3.0,
  halfMin: 0.25,
  halfMax: 1.5,
  unlocked: 0,
  tailChanged: false,
  ...over,
});

const device = (over = {}) => ({
  pressureEnabled: true,
  tiltSupported: true,
  tilt: [3, -4],
  livePressure: 0,
  ...over,
});

describe('idea5 / C4 — 단일 판정', () => {
  it('정상 획은 정상이라고 말한다 (판정이 늘 경고하지 않는다)', () => {
    const v = strokeVerdict({ facts: facts(), device: device(), ledger: ledgerFacts() });
    expect(v.level).toBe(Level.OK);
    expect(v.text).toContain('OK');
    expect(v.line).toContain('tilt_src=device');
  });

  it('설정이 꺼져 있으면 입력을 의심하지 않는다 (오진 회귀 방지)', () => {
    const v = strokeVerdict({
      facts: facts({ pressureMin: 1, pressureMax: 1 }),
      device: device({ pressureEnabled: false }),
      ledger: ledgerFacts(),
    });
    expect(v.level).toBe(Level.NOTE);
    expect(v.text).toContain('설정');
    expect(v.text).not.toContain('OTD');
    // 라이브 경고도 설정을 존중한다.
    expect(liveFlat({ nPt: 20, pressure: [1, 1], widths: [1, 1], device: device({ pressureEnabled: false }) })).toBeNull();
  });

  it('평평함 + 장부 유실(stale)은 입력 문제가 아니라 **유실**로 판정된다', () => {
    const v = strokeVerdict({
      facts: facts({ pressureMin: 0.5, pressureMax: 0.5 }),
      device: device(),
      ledger: ledgerFacts({ delivered: 2, stale: 1 }),
    });
    expect(v.level).toBe(Level.WARN);
    expect(v.text).toContain('stale');
    expect(v.line).toContain('stale=1'); // 장부가 로그 한 줄에 들어간다
  });

  it('장부 유실이 없으면 같은 측정값이 입력 문제로 판정된다 (재료가 판정을 가른다)', () => {
    const v = strokeVerdict({
      facts: facts({ pressureMin: 0.5, pressureMax: 0.5 }),
      device: device(),
      ledger: ledgerFacts({ delivered: 2 }),
    });
    expect(v.text).toContain('OTD');
  });

  it('꼬리 변화는 별도 로그가 아니라 같은 한 줄에 접힌다 (grep 토큰 유지)', () => {
    const v = strokeVerdict({
      facts: facts({ tailChanged: true }),
      device: device(),
      ledger: ledgerFacts(),
    });
    expect(v.level).toBe(Level.WARN);
    expect(v.line.startsWith('PENUP-CHANGED: ')).toBe(true);
    expect(v.line.match(/PENUP-CHANGED/g)).toHaveLength(1);
  });

  it('라우터 장부 요약이 판정 재료로 이어진다 (유실은 데이터로 관측된다)', () => {
    // 열린 세션을 강제로 닫지 않은 상태에서 요약을 읽는다 — 스텁 라우터의 정산.
    const router = createRouter({ sinks: [{ name: 'ink', admit: () => 'now', handle: () => {} }] });
    router.dispatch(Pointer('pen', 'down', [1, 1], 0.5, [0, 0]), { now: 0 });
    const s = router.summary();
    expect(s.delivered).toBe(1);
    expect(s.open).toBe(true);
    const l = ledgerFacts(s);
    expect(l.open).toBe(true);
    expect(l.delivered).toBe(1);
  });
});