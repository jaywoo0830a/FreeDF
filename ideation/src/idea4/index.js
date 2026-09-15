/**
 * 아이디어 #4 계약 객체 배럴 — 아키텍처의 공개 표면.
 *
 * 의존성 그래프 (아래로만):
 *   events (잎, 의존 0)
 *   descriptor (잎, 의존 0)      ← control-map
 *   hub ─ events
 *   devices ─ events                                   [장치 축]
 *   tools (잎, 의존 0)                                 [툴 축]
 *   workspace ─ events, tools
 */
export * from './events.js';
export * from './commands.js';
export * from './descriptor.js';
export * from './control-map.js';
export * from './hub.js';
export * from './devices.js';
export * from './tools.js';
export * from './workspace.js';
export * from './canvas.js';
export * from './tool-package.js';
