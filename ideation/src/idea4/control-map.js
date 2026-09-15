import { controlKey } from './descriptor.js';

/**
 * 계약 객체 ③ — 사용자 매핑 테이블.
 *
 * 버튼의 "의미"는 하드웨어가 아니라 이 데이터가 결정한다.
 * 순수 데이터(직렬화 가능)여야 한다 — 설정 파일에 그대로 저장되는 형태.
 */

export const BindingMode = { TAP: 'tap', HOLD: 'hold' };

export function createControlMap(bindings = {}) {
  return {
    bindings,
    /** 매핑 없는 컨트롤은 null — 호출자는 조용히 무시한다 (미지원 하드웨어 안전). */
    binding: (control, index) => bindings[controlKey(control, index)] ?? null,
  };
}
