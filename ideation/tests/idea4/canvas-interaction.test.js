import { describe, expect, it } from 'vitest';
import {
  createHub,
  createEventSource,
  attachStylus,
  createWorkspace,
  createGeometryStub,
  createPageSpaceStage,
  createRecordingSurface,
  createCanvasProjection,
} from '../../src/idea4/index.js';
import { checkWellFormed, commandTypes } from './invariants.js';

/**
 * 아키텍처 설계 테스트 ⑥ — 캔버스 상호작용 계층 (스텁 테스트).
 *
 * 질문: "캔버스 객체와의 상호작용은 어느 계층에서 하는가?"
 * 답: 셋으로 쪼개진다 — 그리고 이 스텁 테스트들이 그 계약 자체다.
 *
 *  1. 입력 질의 (화면→페이지)  = Geometry port, sense 정규화 단계만 사용
 *  2. 출력 (그리기)            = CanvasSurface port, projection 이 유일한 호출자
 *  3. 캐시/무효화              = 커맨드 부수효과로 projection 이 지시
 *
 * createRecordingSurface() 는 테스트 더블이자 포트 계약의 명세다.
 */

describe('캔버스 상호작용 계층 — 스텁 테스트', () => {
  it('① 입력 질의: 화면 좌표는 정규화 단계에서 페이지 좌표로 — 툴은 화면을 모른다', () => {
    const geometry = createGeometryStub({ zoom: 2, origin: [50, 100] });
    const hub = createHub();
    const stage = createPageSpaceStage(hub, geometry); // 어댑터는 hub 대신 stage 로 emit
    const ws = createWorkspace(hub); // 워크스페이스는 페이지 좌표만 본다
    const tablet = createEventSource();
    attachStylus(stage, tablet);

    tablet.fire('packet', { phase: 'down', pos: [150, 300], pressure: 0.5, tilt: 0 }); // 윈도우 좌표

    expect(ws.commands[0].point).toEqual([50, 100]); // (150-50)/2, (300-100)/2 — 페이지 좌표로 도착
  });

  it('① Geometry port 는 왕복 변환이 항등이다 — 읽기 전용이라도 정확해야 한다', () => {
    const geometry = createGeometryStub({ zoom: 1.5, origin: [30, -20] });
    const page = geometry.toPage([123.5, -77.25]);
    expect(geometry.toWindow(page)).toEqual([123.5, -77.25]);
  });

  it('② 출력: projection 이 surface 의 유일한 호출자 — 라이브 프리뷰는 tail만 (O(Δ))', () => {
    const surface = createRecordingSurface();
    const projection = createCanvasProjection({ surface });
    const hub = createHub();
    const ws = createWorkspace(hub);
    const pen = createEventSource();
    attachStylus(hub, pen);

    pen.fire('packet', { phase: 'down', pos: [0, 0], pressure: 0.5, tilt: 0 });
    pen.fire('packet', { phase: 'drag', pos: [1, 0], pressure: 0.5, tilt: 0 });
    pen.fire('packet', { phase: 'drag', pos: [2, 0], pressure: 0.5, tilt: 0 });

    const consumed = projection.project(ws.commands);
    expect(consumed).toBe(3); // 새 커맨드만 소비 — 커서가 O(Δ) 를 보장
    expect(surface.ops.map((o) => o.op)).toEqual(['beginLive', 'drawLiveTail', 'drawLiveTail']);
    expect(surface.ops[1].tail).toEqual([[1, 0]]); // 새 점 1개만 전달 — 이전 점 재처리 없음
    expect(surface.ops[2].tail).toEqual([[2, 0]]);
  });
  it('② 커밋: end-stroke → 라이브 제거 + 확정 메시 추가 (불변 → 캐시 대상)', () => {
    const surface = createRecordingSurface();
    const projection = createCanvasProjection({ surface });
    const hub = createHub();
    const ws = createWorkspace(hub);
    const pen = createEventSource();
    attachStylus(hub, pen);

    pen.fire('packet', { phase: 'down', pos: [0, 0], pressure: 0.5, tilt: 0 });
    pen.fire('packet', { phase: 'up', pos: [1, 0], pressure: 0.5, tilt: 0 });
    projection.project(ws.commands);

    expect(surface.ops.map((o) => o.op)).toEqual(['beginLive', 'endLive', 'drawCommitted']);
    expect(surface.ops[2].mesh).toEqual({ tool: 'pen' });
  });

  it('③ 지우개: erase-at 는 invalidate 로 번역된다 — 라이브/커밋 연산이 없다', () => {
    const surface = createRecordingSurface();
    const projection = createCanvasProjection({ surface });
    const hub = createHub();
    const ws = createWorkspace(hub);
    ws.select('tool:eraser');
    const pen = createEventSource();
    attachStylus(hub, pen);

    pen.fire('packet', { phase: 'down', pos: [5, 5], pressure: 0.5, tilt: 0 });
    pen.fire('packet', { phase: 'up', pos: [5, 5], pressure: 0.5, tilt: 0 });
    projection.project(ws.commands);

    expect(surface.ops.every((o) => o.op === 'invalidate')).toBe(true);
    expect(surface.ops[0].region).toEqual({ center: [5, 5], radius: 8 });
  });

  it('③ undo: 페이지 무효화 — 라이브/커밋 재생성 없이 재렌더만 지시', () => {
    const surface = createRecordingSurface();
    const projection = createCanvasProjection({ surface });
    projection.project([{ type: 'undo' }]);
    expect(surface.ops).toEqual([{ op: 'invalidate', region: 'page' }]);
  });

  it('surface port 는 그리기 전용이다 — 문서 상태를 되묻는 질의가 하나도 없다', () => {
    const surface = createRecordingSurface();
    const methods = Object.keys(surface)
      .filter((k) => typeof surface[k] === 'function')
      .sort();
    expect(methods).toEqual(['beginLive', 'drawCommitted', 'drawLiveTail', 'endLive', 'invalidate']);
    // capabilities 는 메서드가 아니라 데이터다 — 능력 선언이 포트를 키우지 않는다
    expect(surface.capabilities).toEqual(['core']);
    // 전부 void 연산 — 반환값으로 상태를 얻을 방법이 없다 (읽기 port 와 완전히 분리)
    expect(surface.beginLive(1, {})).toBeUndefined();
    expect(surface.invalidate('page')).toBeUndefined();
  });

  it('capability 는 선언된 경우에만 메서드가 존재한다 (신 인터페이스 방지)', () => {
    const core = createRecordingSurface();
    expect(core.overlay).toBeUndefined(); // 선언 없음 → 메서드 없음
    const withOverlay = createRecordingSurface({ capabilities: ['core', 'overlay'] });
    expect(typeof withOverlay.overlay).toBe('function');
  });

  it('end-to-end: 윈도우 좌표로 들어와 페이지 좌표로 그려진다 (두 port 가 만나는 지점)', () => {
    const surface = createRecordingSurface();
    const geometry = createGeometryStub({ zoom: 2, origin: [50, 100] });
    const hub = createHub();
    const stage = createPageSpaceStage(hub, geometry);
    const ws = createWorkspace(hub);
    const projection = createCanvasProjection({ surface });
    const tablet = createEventSource();
    attachStylus(stage, tablet);

    tablet.fire('packet', { phase: 'down', pos: [150, 300], pressure: 0.5, tilt: 0 });
    tablet.fire('packet', { phase: 'drag', pos: [154, 306], pressure: 0.7, tilt: 0 });
    tablet.fire('packet', { phase: 'up', pos: [154, 306], pressure: 0.7, tilt: 0 });
    projection.project(ws.commands);

    // 커맨드 스트림 자체는 잘-형성 — 캔버스가 끼어도 문서 의미론은 오염되지 않는다
    expect(checkWellFormed(ws.commands)).toBe(true);
    expect(commandTypes(ws.commands)).toEqual(['begin-stroke', 'extend-stroke', 'end-stroke']);
    // 그려진 것은 페이지 좌표: (154-50)/2, (306-100)/2 = [52, 103]
    expect(surface.ops.find((o) => o.op === 'drawLiveTail').tail).toEqual([[52, 103]]);
    expect(surface.ops.at(-1).op).toBe('drawCommitted');
  });

  it('projection 을 두 번 호출해도 커서가 중복 소비하지 않는다 (프레임 사이클 계약)', () => {
    const surface = createRecordingSurface();
    const projection = createCanvasProjection({ surface });
    const hub = createHub();
    const ws = createWorkspace(hub);
    const pen = createEventSource();
    attachStylus(hub, pen);

    pen.fire('packet', { phase: 'down', pos: [0, 0], pressure: 0.5, tilt: 0 });
    expect(projection.project(ws.commands)).toBe(1);

    pen.fire('packet', { phase: 'up', pos: [1, 0], pressure: 0.5, tilt: 0 });
    expect(projection.project(ws.commands)).toBe(1); // 새 커맨드만

    expect(projection.project(ws.commands)).toBe(0); // 새것이 없으면 0
    expect(surface.ops.map((o) => o.op)).toEqual(['beginLive', 'endLive', 'drawCommitted']);
  });
});
