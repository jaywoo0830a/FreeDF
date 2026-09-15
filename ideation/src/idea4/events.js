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
 */
export const Pointer = (source, phase, point, pressure = 1.0, tilt = 0.0) => ({
  kind: EventKind.POINTER,
  source,
  phase,
  point,
  pressure,
  tilt,
});

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
