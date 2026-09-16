/**
 * 계약 객체 ① — 통합 이벤트 어휘 (잎).
 *
 * 이 모듈은 아무것도 import 하지 않는다. 장치 축과 툴 축이 공유하는
 * 유일한 것은 이 어휘이다 (idea #2/#3/#4 에서 확정된 계약).
 */

export const EventKind = {
  POINTER: 'pointer',
  ACTION: 'action',
  CONTROL: 'control',
};

export const PointerPhase = { DOWN: 'down', DRAG: 'drag', UP: 'up' };

/** 포인터 소스 — 허브 충돌 규칙("한 번에 한 포인터")의 대상이 되는 소스들 */
export const POINTER_SOURCES = ['pen', 'mouse', 'pad', 'tablet'];

/**
 * 하드웨어 컨트롤 종류. 버튼 "개수"는 어휘에 없다 — 정체성은 (종류, 번호) 쌍.
 * 펜에 버튼이 0개든 5개든, 패드에 매크로 키가 몇 개든 이 어휘는 변하지 않는다.
 */
export const ControlKind = {
  STYLUS_BUTTON: 'stylus-button',
  EXPRESS_KEY: 'express-key',
};

export const ControlPhase = { DOWN: 'down', UP: 'up' };

/** action 의 모드 — tap(즉시 실행), hold(누르는 동안 툴 교체) */
export const ActionMode = { TAP: 'tap', HOLD_ON: 'hold-on', HOLD_OFF: 'hold-off' };

/**
 * 포인터 이벤트. 압력/기울기는 장치 어댑터가 기본값을 채워 "항상 존재"한다
 * (능력 협상은 장치 쪽 책임 — 툴은 fallback 을 모른다).
 *
 * 기울기는 **벡터**(도, 각 축 ±90)다: 크기 = 기울임 각(pitch), 방향 = 방위각
 * (azimuth). 크기만 나르던 구 어휘는 방향을 표현할 수 없어 소비자가 어휘 밖의
 * 별도 벡터를 들고 다녀야 했다 — idea #5(C2)에서 벡터로 확정됐고, 이 스펙은
 * 구 호출(숫자)을 [n, 0]으로 **정규화**해 받아들인다 (마이그레이션 호환).
 */
export const Pointer = (source, phase, point, pressure = 1.0, tilt = 0.0) => ({
  kind: EventKind.POINTER,
  source,
  phase,
  point,
  pressure,
  tilt: tiltVector(tilt),
});

/** 틸트를 벡터로 정규화 — 숫자(구 스펙)는 [n, 0]으로 (방향 없음 = +x). */
export const tiltVector = (t) =>
  Array.isArray(t) ? [t[0] ?? 0, t[1] ?? 0] : [t ?? 0, 0];

/** 틸트 벡터 → 크기 (도). */
export const tiltMagnitude = (t) => {
  const [x, y] = tiltVector(t);
  return Math.hypot(x, y);
};

/** 틸트 벡터 → (방위각 rad, cos pitch). 수직(0 벡터)이면 (0, 1). */
export const tiltAzimuth = (t) => {
  const [x, y] = tiltVector(t);
  const mag = Math.hypot(x, y);
  if (mag < 1e-3) return [0, 1];
  return [Math.atan2(y, x), Math.cos((Math.min(mag, 90) * Math.PI) / 180)];
};

/** 기울기를 보고하지 않는 장치의 기본 틸트 벡터 (능력 협상의 결과값). */
export const NO_TILT = [0, 0];

/** 논리 동작 — 툴 선택, undo 등. mode 는 hold 수정자일 때만 설정된다. */
export const Action = (source, key, mode) => ({
  kind: EventKind.ACTION,
  source,
  key,
  ...(mode ? { mode } : {}),
});

/** 원시 하드웨어 컨트롤 이벤트 — 사용자 매핑으로 번역되기 전 상태. */
export const RawControl = (control, index, phase) => ({
  kind: EventKind.CONTROL,
  control,
  index,
  phase,
});
