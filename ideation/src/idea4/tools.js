import { CommandType } from './commands.js';

export { CommandType };

/**
 * 계약 객체 ⑥ — 툴 상태기계 (툴 축).
 *
 * 툴 계약: { name: string, handle(event, emit) }
 *  - 'pointer' 이벤트만 소비하고 'action' 은 무시한다 (툴 선택은 워크스페이스 몫).
 *  - 항상 온전한 down→…→up 스트림을 받는다고 가정한다 (전환은 워크스페이스가 보장).
 *  - 문서 커맨드만 생산한다 — 렌더/캔버스/장치 지식 0 (캔버스는 projection 몫).
 *
 * import 는 commands.js(출력 어휘) 하나뿐이다 (layering 테스트가 검사).
 */

const EventKind = { POINTER: 'pointer' };

/** 툴 = 전이 표. down/drag/up 에 무엇을 할지만 적는다 — 상태기계 뼈대는 공유된다. */
function pointerTool({ name, begin, extend, end }) {
  let state = 'idle';
  return {
    name,
    handle(e, emit) {
      if (e.kind !== EventKind.POINTER) return;
      if (state === 'idle' && e.phase === 'down') {
        state = 'active';
        begin(e, emit);
      } else if (state === 'active' && e.phase === 'drag') {
        extend(e, emit);
      } else if (state === 'active' && e.phase === 'up') {
        state = 'idle';
        end(e, emit);
      }
    },
  };
}

export const createPenTool = () =>
  pointerTool({
    name: 'pen',
    begin: (e, emit) => emit({ type: 'begin-stroke', tool: 'pen', point: e.point, pressure: e.pressure }),
    extend: (e, emit) => emit({ type: 'extend-stroke', point: e.point, pressure: e.pressure }),
    end: (_e, emit) => emit({ type: 'end-stroke' }),
  });

export const createEraserTool = () =>
  pointerTool({
    name: 'eraser',
    begin: (e, emit) => emit({ type: 'erase-at', point: e.point }),
    extend: (e, emit) => emit({ type: 'erase-at', point: e.point }),
    end: (_e, emit) => emit({ type: 'end-erase' }),
  });

export const createHighlighterTool = () =>
  pointerTool({
    name: 'highlighter',
    begin: (e, emit) => emit({ type: 'begin-stroke', tool: 'highlighter', point: e.point, pressure: 1.0 }),
    extend: (e, emit) => emit({ type: 'extend-stroke', point: e.point, pressure: 1.0 }),
    end: (_e, emit) => emit({ type: 'end-stroke' }),
  });

/** 기본 툴 레지스트리 — 워크스페이스가 action 키('tool:NAME')로 조회한다. */
export const defaultTools = () => ({
  pen: createPenTool(),
  eraser: createEraserTool(),
  highlighter: createHighlighterTool(),
});
