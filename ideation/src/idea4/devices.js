import { EventKind, Pointer, ControlPhase } from './events.js';

/**
 * 계약 객체 ⑤ — 장치 어댑터 (장치 축).
 *
 * raw 하드웨어 이벤트를 통합 어휘로 번역한다. 툴/워크스페이스를 몰라야 한다
 * (layering 테스트가 이 규칙을 정적으로 검사한다).
 * 능력 협상(압력 없는 마우스 → 기본값)도 여기서 끝난다.
 */

/** 최소 이벤트 소스 — 하드웨어 mock/재생 장치의 공통 모양: on(event, cb) */
export function createEventSource() {
  const handlers = {};
  return {
    on: (ev, cb) => (handlers[ev] ??= []).push(cb),
    fire: (ev, raw) => handlers[ev]?.forEach((cb) => cb(raw)),
  };
}

/**
 * 컨트롤 번역: 원시 (종류, 번호) → 사용자 매핑 → 'action'.
 * 버튼 개수와 무관 — 들어온 (종류, 번호)만 조회한다.
 */
export function attachControls(hub, hw, map) {
  hw.on('control', (c) => {
    const b = map.binding(c.control, c.index);
    if (!b) return; // 미바인딩 — 조용히 무시 (미지원 하드웨어 안전)
    if (b.mode === 'hold') {
      const mode = c.phase === ControlPhase.DOWN ? 'hold-on' : 'hold-off';
      hub.emit({ kind: EventKind.ACTION, source: c.control, key: b.action, mode });
    } else if (c.phase === ControlPhase.DOWN) {
      hub.emit({ kind: EventKind.ACTION, source: c.control, key: b.action });
    }
  });
}

/** 스타일러스: 포인터 패킷 + 측면 버튼(개수 무관). */
export function attachStylus(hub, hw, map) {
  hw.on('packet', (p) => hub.emit(Pointer('pen', p.phase, p.pos, p.pressure, p.tilt)));
  if (map) attachControls(hub, hw, map);
}

/** 마우스: 압력/기울기 없음 → 어댑터가 기본값을 채운다 (능력 협상의 끝). */
export function attachMouse(hub, hw, map) {
  hw.on('down', (p) => hub.emit(Pointer('mouse', 'down', p.pos)));
  hw.on('move', (p) => hub.emit(Pointer('mouse', 'drag', p.pos)));
  hw.on('up', () => hub.emit(Pointer('mouse', 'up')));
  if (map) attachControls(hub, hw, map);
}

/** 태블릿 본체: 익스프레스 키/매크로 — 컨트롤만 (포인터는 펜이 담당). */
export function attachTabletControls(hub, hw, map) {
  attachControls(hub, hw, map);
}
