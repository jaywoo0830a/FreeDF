import { EventKind, POINTER_SOURCES } from './events.js';

/**
 * 계약 객체 ④ — 이벤트 허브.
 *
 * "정책이 사는 곳": ① 포인터 소스 충돌 규칙(한 번에 한 포인터 — 툴 상태기계는
 * 이런 혼란을 아예 받지 않는다) ② 재생(player) 계약.
 */

export function createHub({ pointerSources = POINTER_SOURCES } = {}) {
  const listeners = [];
  let activeSource = null;

  return {
    on(listener) {
      listeners.push(listener);
    },

    /**
     * 반환값: true=전파됨, false=정책에 의해 drop.
     * 단, 장치 어댑터의 fire 체인 뒤에서는 이 반환값이 관찰되지 않는다 —
     * drop 여부는 다운스트림 커맨드로 관찰하는 것이 계약이다.
     */
    emit(event) {
      if (event.kind === EventKind.POINTER && pointerSources.includes(event.source)) {
        if (activeSource && activeSource !== event.source) return false;
        if (event.phase === 'down') activeSource = event.source;
        if (event.phase === 'up') activeSource = null;
      }
      listeners.forEach((l) => l(event));
      return true;
    },
  };
}

/**
 * 재생(player) 계약 — 하드웨어 없이 통합 이벤트 스크립트만으로 전체 흐름을
 * 구동한다. 장치↔툴 분리가 진짜라는 증거이자 recording/player 의 모양.
 */
export const replay = (hub, events) => events.forEach((e) => hub.emit(e));
