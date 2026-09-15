import { EventKind, ActionMode } from './events.js';
import { defaultTools } from './tools.js';

/**
 * 계약 객체 ⑦ — 워크스페이스 (정책의 집).
 *
 * 소관: ① 활성 툴 선택 (action 'tool:NAME' — tap/hold 모두)
 *      ② 획 경계 보장 (전환 시 합성 up/down — 각 툴은 항상 온전한 스트림만 본다)
 *      ③ 즉시 커맨드(undo 등) 통과.
 * 장치 지식 0 — 포인터가 어느 장치에서 왔는지 구분하지 않는다.
 */

const TOOL_PREFIX = 'tool:';

export function createWorkspace(hub, { tools = defaultTools() } = {}) {
  let active = tools.pen;
  let pointerDown = false;
  let lastPoint = null;
  const held = []; // hold 수정자 LIFO 스택 (중첩 hold 지원)
  const commands = [];
  const emit = (cmd) => commands.push(cmd);

  // "툴 전환은 획 경계에서 일어난다": 그리는 중이라면 합성 up/down 으로
  // 이전 툴의 세션을 닫고 새 툴의 세션을 연다. 툴 상태기계는 단순함이 유지된다.
  const switchTo = (tool, via) => {
    if (!tool || tool === active) return;
    if (pointerDown) {
      active.handle({ kind: EventKind.POINTER, phase: 'up', point: lastPoint, source: via }, emit);
      active = tool;
      active.handle({ kind: EventKind.POINTER, phase: 'down', point: lastPoint, source: via }, emit);
    } else {
      active = tool;
    }
  };

  const toolOf = (key) => tools[key.slice(TOOL_PREFIX.length)];

  hub.on((e) => {
    if (e.kind === EventKind.POINTER) {
      pointerDown = e.phase !== 'up';
      lastPoint = e.point ?? lastPoint;
      active.handle(e, emit);
      return;
    }
    if (e.mode === ActionMode.HOLD_ON) {
      held.push(active);
      switchTo(toolOf(e.key) ?? active, e.source);
    } else if (e.mode === ActionMode.HOLD_OFF) {
      switchTo(held.pop() ?? active, e.source);
    } else if (typeof e.key === 'string' && e.key.startsWith(TOOL_PREFIX)) {
      switchTo(toolOf(e.key), e.source);
    } else {
      commands.push({ type: e.key }); // undo, next-page 등 즉시 커맨드
    }
  });

  return {
    commands,
    activeName: () => active.name,
    heldCount: () => held.length,
    select: (key) => hub.emit({ kind: EventKind.ACTION, source: 'keyboard', key }),
    /** 툴 패키지 조립용 — 실행 중 새 툴을 등록한다 (tool-package.js 가 호출). */
    addTool(tool) {
      tools[tool.name] = tool;
    },
  };
}
