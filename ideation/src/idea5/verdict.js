/**
 * C4 — 진단은 **하나**다: 설정 + 측정 + 라우터 장부 (idea5).
 *
 * Rust 이식: `crates/freedf/src/app/canvas/diagnostics.rs`.
 *
 * 문제: 진단이 세 곳에 흩어져 있었다 (`stroke end`/`PENUP-CHANGED`/`LIVE-FLAT`).
 * 각자 다른 재료만 봐서 **"설정이 꺼져 있어 평평하다"와 "입력이 유실되어
 * 평평하다"를 구분하지 못했다** (writing.log 오진).
 *
 * 계약: 재료는 세 종류뿐이고, 판정은 한 함수가 낸다. 로그도 한 줄이다
 * (구 grep 토큰은 접어 넣어 유지: `PENUP-CHANGED` 접두, `LIVE-FLAT` 메시지).
 */

export const Level = { OK: 'ok', NOTE: 'note', WARN: 'warn' };

export const ledgerFacts = (summary = {}) => ({
  delivered: summary.delivered ?? 0,
  refused: summary.refused ?? 0,
  promoted: summary.promoted ?? 0,
  expired: summary.expired ?? 0,
  stale: summary.stale ?? 0,
  cancelled: summary.cancelled ?? 0,
  replaced: summary.replaced ?? 0,
  open: !!summary.open,
});

export const hasLoss = (l) => l.expired > 0 || l.stale > 0 || l.replaced > 0;

export const ledgerLabel = (l) =>
  `ledger[d=${l.delivered} r=${l.refused} p=${l.promoted} x=${l.expired} ` +
  `stale=${l.stale} c=${l.cancelled} rep=${l.replaced} open=${l.open}]`;

/**
 * 획 종료 판정 — 사다리 순서가 곧 인과 순서다:
 * ① 설정 꺼짐(정상) ② 탭 ③ 짧은 획 ④ 유실 동반 평평 ⑤ 입력 문제 ⑥ 잠금/모델.
 */
export const strokeVerdict = ({ tool = 'Pen', facts, device, ledger = ledgerFacts() }) => {
  let level = Level.OK;
  let text;
  if (!device.pressureEnabled) {
    level = Level.NOTE;
    text = '필압 꺼짐 (설정) — 폭은 필압과 무관';
  } else if (facts.nPt === 1) {
    level = Level.NOTE;
    text = '탭 (1점 — 정상)';
  } else if (facts.nPt < 8) {
    level = Level.NOTE;
    text = '점 부족 — 짧은 획';
  } else if (facts.pressureMax - facts.pressureMin < 0.05) {
    level = Level.WARN;
    text = hasLoss(ledger)
      ? '필압 일정 + 세션 유실(stale/expired) — Up 에지 유실 의심'
      : '필압 일정 → 입력 문제 (OTD 연결/필압 소스 확인)';
  } else if (facts.unlocked > 0) {
    level = Level.WARN;
    text = '폭 잠금 안 됨 → locker 버그';
  } else if (facts.halfMax - facts.halfMin < 0.02) {
    level = Level.WARN;
    text = '필압은 변하는데 렌더 폭 고정 → 모델/바닥값 버그';
  } else {
    text = 'OK — 렌더 폭 변화 정상';
  }

  // 꼬리 변화(구 PENUP-CHANGED) — 별도 로그가 아니라 같은 한 줄에 접힌다.
  let prefix = '';
  if (facts.tailChanged) {
    level = Level.WARN;
    prefix = 'PENUP-CHANGED: ';
  }

  const f = (v) => v.toFixed(3);
  const line =
    `${prefix}stroke end: tool=${tool} n=${facts.nPt} ` +
    `pressure=[${f(facts.pressureMin)}..${f(facts.pressureMax)}] ` +
    `width=[${f(facts.widthMin)}..${f(facts.widthMax)}] ` +
    `half=[${f(facts.halfMin)}..${f(facts.halfMax)}] unlocked=${facts.unlocked} ` +
    `live_pressure=${device.livePressure} tilt=[${device.tilt[0].toFixed(1)},${device.tilt[1].toFixed(1)}] ` +
    `tilt_src=${device.tiltSupported ? 'device' : 'hand'} ${ledgerLabel(ledger)} → ${text}`;

  return { level, text, line };
};

/** 프레임 단위 평평 경고 (구 LIVE-FLAT) — 조건/문구의 단일 소유자. */
export const liveFlat = ({ nPt, pressure, widths, device }) => {
  if (nPt < 8) return null;
  if (widths[1] - widths[0] >= 0.05) return null;
  if (!device.pressureEnabled) return null; // 설정이 꺼져 있으면 정상 — 경고 금지
  const verdict =
    pressure[1] - pressure[0] < 0.05
      ? '필압 일정 — 입력/설정 확인'
      : '필압은 변하는데 렌더 폭 고정 — 모델 확인';
  return `LIVE-FLAT: n=${nPt} widths=[${widths[0].toFixed(3)}..${widths[1].toFixed(3)}] ` +
    `pressure=[${pressure[0].toFixed(3)}..${pressure[1].toFixed(3)}] ` +
    `live_pressure=${device.livePressure} (판정: ${verdict})`;
};