/**
 * 계약 객체 ⓪ — 문서 커맨드 어휘 (잎).
 *
 * 툴이 생산하고 문서/투영(projection)이 소비하는 출력 어휘.
 * 툴 축(tools.js)과 캔버스 경계(canvas.js)가 공유하는 것은 이 상수뿐이다 —
 * 어느 쪽도 서로를 import 하지 않는다.
 */

export const CommandType = {
  BEGIN_STROKE: 'begin-stroke',
  EXTEND_STROKE: 'extend-stroke',
  END_STROKE: 'end-stroke',
  ERASE_AT: 'erase-at',
  END_ERASE: 'end-erase',
  UNDO: 'undo',
};
