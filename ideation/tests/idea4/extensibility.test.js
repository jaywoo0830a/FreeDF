import { describe, expect, it } from 'vitest';
import {
  createHub,
  createEventSource,
  attachStylus,
  createWorkspace,
  createRecordingSurface,
  createCanvasProjection,
} from '../../src/idea4/index.js';
import { checkWellFormed } from './invariants.js';

/**
 * 아키텍처 설계 테스트 ⑦ — 확장성 계약 (TDD 레드).
 *
 * 질문: "CanvasTool 이 추가/삭제될 때 Canvas 인터페이스도 변해야 한다면,
 *        이 구조는 대비가 되어 있는가?"
 *
 * 레드 단계: 아래 테스트들은 [아직 존재하지 않는 이상적인 인터페이스]를
 * 요구한다 — 소스 수정 없이 전부 실패해야 한다.
 *
 *  1. projection 의 default: 무시 → 미처리 커맨드 즉시 실패 (조용한 데이터 손실 금지)
 *  2. projection 은 닫힌 switch 가 아니라 열린 레지스트리다
 *  3. 툴 패키지는 필요 surface capability 를 선언하고 조립 시 검증된다
 *  4. capability 가 있으면 조립이 성공한다
 *  5. Lasso 툴 패키지가 프레임워크 파일 무수정으로 end-to-end 동작한다
 */

// ── 테스트 전용: Lasso 툴 패키지. 프레임워크 밖(테스트)에서 정의된다는
//    사실 자체가 "확장이 열려 있다"는 증거다.
const lassoPackage = {
  name: 'lasso',
  requires: ['overlay'], // 필요 surface capability 선언
  commandTypes: ['lasso-begin', 'lasso-move', 'lasso-end'],
  createTool: () => {
    let state = 'idle';
    return {
      name: 'lasso',
      handle(e, emit) {
        if (e.kind !== 'pointer') return;
        if (state === 'idle' && e.phase === 'down') {
          state = 'selecting';
          emit({ type: 'lasso-begin', point: e.point });
        } else if (state === 'selecting' && e.phase === 'drag') {
          emit({ type: 'lasso-move', points: [e.point] });
        } else if (state === 'selecting' && e.phase === 'up') {
          state = 'idle';
          emit({ type: 'lasso-end' });
        }
      },
    };
  },
  projections: {
    // 레지스트리 계약: 패키지가 생산하는 모든 커맨드의 번역을 스스로 등록한다.
    'lasso-begin': () => {}, // 세션 시작 — surface 연산 없음
    'lasso-move': (cmd, surface) => surface.overlay({ kind: 'ants', points: cmd.points }),
    'lasso-end': () => {}, // 세션 종료
  },
};

describe('확장성 계약 — 툴 패키지가 프레임워크를 바꾸지 않는다', () => {
  it('1. 미처리 커맨드는 조용히 버려지지 않고 즉시 실패한다', () => {
    const surface = createRecordingSurface();
    const projection = createCanvasProjection({ surface });

    // 현재 구현: default 로 조용히 무시 → throw 되지 않아 실패 (레드)
    expect(() => projection.project([{ type: 'lasso-begin', point: [1, 2] }]))
      .toThrowError(/미처리|unhandled/i);
    expect(surface.ops).toEqual([]); // 조용한 부작용도 없어야 한다
  });

  it('2. projection 은 열린 레지스트리다 — register 로 번역을 추가할 수 있다', () => {
    // capability 모델 정합성: overlay 를 기록하려면 surface 가 그 능력을 선언해야 한다
    const surface = createRecordingSurface({ capabilities: ['overlay'] });
    const projection = createCanvasProjection({ surface });

    // 현재 register 가 없어 실패 (레드)
    projection.register('lasso-move', lassoPackage.projections['lasso-move']);
    projection.project([{ type: 'lasso-move', points: [[3, 4]] }]);
    expect(surface.ops).toEqual([
      { op: 'overlay', shape: { kind: 'ants', points: [[3, 4]] } },
    ]);
  });

  it('3. 툴 패키지는 필요 capability 를 선언하고, 없는 surface 면 조립이 실패한다', async () => {
    // assembleTool 은 아직 없다 — 동적 import 로 확인 (파일 전체가 깨지지 않게)
    const { assembleTool } = await import('../../src/idea4/index.js');
    const surface = createRecordingSurface(); // overlay capability 미장착
    const projection = createCanvasProjection({ surface });
    const ws = createWorkspace(createHub());

    // 현재 undefined → 호출 불가로 실패 (레드)
    expect(() => assembleTool({ ws, projection, surface, pkg: lassoPackage }))
      .toThrowError(/overlay/);
  });

  it('4. capability 가 있으면 조립 성공 — overlay 가 surface 에 노출된다', async () => {
    const { assembleTool } = await import('../../src/idea4/index.js');
    const surface = createRecordingSurface({ capabilities: ['overlay'] });
    const projection = createCanvasProjection({ surface });
    const ws = createWorkspace(createHub());

    assembleTool({ ws, projection, surface, pkg: lassoPackage }); // 안 던진다
    expect(surface.capabilities).toContain('overlay');
    expect(typeof surface.overlay).toBe('function');
  });

  it('5. Lasso end-to-end: 프레임워크 파일 무수정, 패키지 하나로 동작한다', async () => {
    const { assembleTool } = await import('../../src/idea4/index.js');
    const surface = createRecordingSurface({ capabilities: ['overlay'] });
    const projection = createCanvasProjection({ surface });
    const hub = createHub();
    const ws = createWorkspace(hub);
    assembleTool({ ws, projection, surface, pkg: lassoPackage });

    ws.select('tool:lasso');
    const pen = createEventSource();
    attachStylus(hub, pen);
    pen.fire('packet', { phase: 'down', pos: [0, 0], pressure: 0.5, tilt: 0 });
    pen.fire('packet', { phase: 'drag', pos: [1, 0], pressure: 0.5, tilt: 0 });
    pen.fire('packet', { phase: 'up', pos: [1, 0], pressure: 0.5, tilt: 0 });

    // 프레임 사이클: 새로 쌓인 문서 커맨드를 projection 에 투영한다
    const consumed = projection.project(ws.commands);
    expect(consumed).toBe(3);

    // 새 툴의 커맨드가 기존 어휘와 한 스트림에 공존하고 불변식은 유지된다
    expect(ws.commands.map((c) => c.type)).toEqual(['lasso-begin', 'lasso-move', 'lasso-end']);
    expect(checkWellFormed(ws.commands)).toBe(true);
    // 등록된 번역이 overlay 로 기록했다 — 프레임워크 파일 수정 0
    expect(surface.ops).toEqual([
      { op: 'overlay', shape: { kind: 'ants', points: [[1, 0]] } },
    ]);
  });
});
