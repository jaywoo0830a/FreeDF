/**
 * 테스트 공용 픽스처 — 실제 앱의 축소판 (툴바: 필압 토글 + 획 카운터).
 *
 * 컴포넌트는 앱을 모른다: 상태 조각 하나, 메시지 하나만 본다. 조립/스코프/
 * 라우팅은 `mount` 가 한다 (tests/ui/component.test.js 가 증명).
 */
import { defineComponent } from '../../src/ui/index.js';
import { box, toggle, button, text } from '../../src/ui/index.js';

export const toolbar = defineComponent({
  id: 'toolbar',
  init: () => ({ pressure: true, strokes: 0 }),

  view: (s) =>
    box({ id: 'root' }, [
      text(`strokes: ${s.strokes}`, { id: 'count' }),
      toggle('pressure', '필압 민감도', s.pressure, {
        change: (next) => ({ type: 'pressure', value: next }),
      }),
      button('stroke', '획 추가', { tap: { type: 'stroke' } }),
    ]),

  update: (s, msg) => {
    switch (msg.type) {
      case 'pressure':
        return { ...s, pressure: msg.value };
      case 'stroke':
        return { ...s, strokes: s.strokes + 1 };
      default:
        return s; // 모르는 메시지 — 상태 불변 (하니스 장부에는 기록된다)
    }
  },
});