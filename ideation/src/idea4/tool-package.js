/**
 * 계약 객체 ⑨ — 툴 패키지 조립 (확장의 단위).
 *
 * 툴 추가에 흩어지던 4가지 — ①상태기계 ②커맨드 어휘 ③projection 번역
 * ④surface 능력 요구 — 를 하나의 패키지(pkg)로 묶어 한 번에 조립한다:
 *
 *  1. pkg.requires ⊆ surface.capabilities 검증 (누락 시 즉시 실패)
 *  2. pkg.projections 를 projection 레지스트리에 등록
 *  3. 툴 상태기계를 워크스페이스에 등록 (ws.select('tool:NAME') 가능)
 *
 * 이 모듈은 아무것도 import 하지 않는다 — 조립은 전달받은 객체들의
 * 계약(메서드)만 사용한다. Rust 의 optional-trait + 핸들러 맵으로 1:1 대응.
 */

const TOOL_KEY_PREFIX = 'tool:';

export function assembleTool({ ws, projection, surface, pkg }) {
  if (!pkg || typeof pkg.name !== 'string') {
    throw new Error('툴 패키지에는 name 이 필요하다');
  }

  // 1) capability 검증 — 요구된 능력이 surface 에 없으면 조립을 거부한다.
  const capabilities = surface.capabilities ?? ['core'];
  const missing = (pkg.requires ?? []).filter((c) => !capabilities.includes(c));
  if (missing.length > 0) {
    throw new Error(
      `surface 가 툴 '${pkg.name}' 의 capability 를 충족하지 않는다: ` +
        `누락=${missing.join(', ')} (보유=${capabilities.join(', ')})`,
    );
  }

  // 2) 번역 등록 — 툴이 자기 커맨드의 projection 을 스스로 가져온다.
  for (const [cmdType, handler] of Object.entries(pkg.projections ?? {})) {
    projection.register(cmdType, handler);
  }

  // 3) 상태기계 등록 — 'tool:NAME' 액션으로 선택 가능해진다.
  const tool = pkg.createTool();
  workspace_tool_check(tool);
  ws.addTool(tool);

  return {
    name: pkg.name,
    selectKey: `${TOOL_KEY_PREFIX}${pkg.name}`,
  };
}

function workspace_tool_check(tool) {
  if (!tool || typeof tool.name !== 'string' || typeof tool.handle !== 'function') {
    throw new Error('툴 계약 위반: { name: string, handle(event, emit) } 이어야 한다');
  }
}
