/**
 * 아이디어 #5 — 계약 상향 (idea4 계약의 **변경분**을 담는 배럴).
 *
 * idea4 가 "게이트를 땜질에서 구조로" 바꿨다면, idea5 는 그 위에서 **남은
 * 땜질을 계약으로 승격**한 변경분이다 (Rust `crates/freedf` 의 C1~C4):
 *
 * | # | 변경 | 계약이 달라진 곳 |
 * |---|---|---|
 * | C1 | 오버레이 탭 판정이 egui 원시 이벤트 → **라우터 싱크**(이벤트 기하) | `wheel-sink.js` |
 * | C2 | 틸트가 크기(float) → **벡터**(방향 포함) | `../idea4/events.js` (어휘) |
 * | C3 | "틸트를 보고하는 장치인가"가 스트림 존재 근사 → **능력 질의** | `pen.js` |
 * | C4 | 흩어진 진단 3종 → 설정+측정+장부의 **단일 판정** | `verdict.js` |
 *
 * 어휘(events)와 툴/워크스페이스는 idea4 의 것이 그대로 단일 소유자다 —
 * 여기서는 **바뀐 것만** 스텁으로 세운다 (엄격함보다 흐름 파악이 목적).
 * 실행 스펙: `tests/idea5/*.test.js`.
 */

// C2 — 어휘는 하나다: idea4 쪽 스펙이 벡터로 상향됐다 (여기서 재수출).
export {
  Pointer,
  tiltVector,
  tiltMagnitude,
  tiltAzimuth,
  NO_TILT,
} from '../idea4/events.js';

export * from './router.js';
export * from './wheel-sink.js';
export * from './pen.js';
export * from './verdict.js';