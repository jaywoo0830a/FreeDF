import { EventKind } from './events.js';
import { CommandType } from './commands.js';

/**
 * 계약 객체 ⑧ — 캔버스 경계 (projection).
 *
 * "캔버스 객체와의 상호작용"은 셋으로 쪼개지고, 각각 다른 계층이 소유한다:
 *
 *  1. 입력 질의 — Geometry port (읽기 전용 좌표 변환).
 *     sense 정규화 단계(createPageSpaceStage)만 사용한다. 툴은 페이지 좌표만
 *     본다 — 화면 좌표는 이 경계에서 끝난다.
 *  2. 출력 — CanvasSurface port (그리기 전용).
 *     createCanvasProjection 이 유일한 호출자다. 툴/허브/워크스페이스는
 *     캔버스의 존재를 모른다.
 *  3. 캐시/무효화 — 커맨드의 부수효과로 projection 이 지시한다
 *     (지우개 → 영역 invalidate, undo → 페이지 무효화).
 *
 * egui Painter 같은 실제 캔버스 객체는 이 포트 뒤에 숨는다 — 스텁 구현이
 * 곧 포트 계약의 명세다.
 */

// ---------- ① Geometry port (입력 질의 — 읽기 전용) ----------

/** 스텁 구현: 실제로는 crates/freedf-core/src/transform.rs 가 이 계약을 채운다. */
export function createGeometryStub({ zoom = 1, origin = [0, 0] } = {}) {
  const [ox, oy] = origin;
  return {
    zoom,
    origin,
    toPage: ([x, y]) => [(x - ox) / zoom, (y - oy) / zoom],
    toWindow: ([x, y]) => [x * zoom + ox, y * zoom + oy],
  };
}

/**
 * 정규화 단계 — 어댑터가 hub 대신 이 stage 로 emit 하면, 워크스페이스가
 * 보는 포인터는 이미 페이지 좌표다. 툴은 화면 좌표를 영원히 모른다.
 */
export function createPageSpaceStage(hub, geometry) {
  return {
    on: (l) => hub.on(l),
    emit(event) {
      if (event.kind === EventKind.POINTER && Array.isArray(event.point)) {
        hub.emit({ ...event, point: geometry.toPage(event.point) });
      } else {
        hub.emit(event);
      }
    },
  };
}

// ---------- ②③ CanvasSurface port (출력 — 그리기 전용) ----------

/**
 * 녹음 스텁 = 포트 계약의 명세. 코어 연산 5개:
 *   beginLive → drawLiveTail* → endLive → drawCommitted
 *   invalidate (지우개 영역 / 페이지 무효화)
 * 질의 메서드가 하나도 없다는 점이 핵심 — 캔버스는 되묻지 않는 출력 port 다.
 *
 * capability: 코어('core') 외의 능력('overlay' 등)은 선언된 경우에만 메서드가
 * 존재한다. 포트를 키우는 대신 능력으로 분화 — 신(神) 인터페이스 방지.
 */
export function createRecordingSurface({ capabilities = ['core'] } = {}) {
  const ops = [];
  const record = (op) => {
    ops.push(op); // void — 포트 계약: 그리기 전용, 상태를 되묻지 않는다
  };
  const surface = {
    /** 노출된 능력 목록 — 툴 패키지의 requires 검증이 이 데이터를 읽는다. */
    capabilities: [...capabilities],
    ops,
    beginLive: (id, style) => record({ op: 'beginLive', id, style }),
    drawLiveTail: (id, tail) => record({ op: 'drawLiveTail', id, tail, points: tail.length }),
    endLive: (id) => record({ op: 'endLive', id }),
    drawCommitted: (mesh) => record({ op: 'drawCommitted', mesh }),
    invalidate: (region) => record({ op: 'invalidate', region }),
  };
  if (surface.capabilities.includes('overlay')) {
    surface.overlay = (shape) => record({ op: 'overlay', shape }); // 선택 오버레이 (marching ants 등)
  }
  return surface;
}

/**
 * projection adapter — surface 의 유일한 호출자.
 * 프레임마다 새로 쌓인 문서 커맨드만 소비한다 (내부 cursor — O(Δ) 계약).
 */
export function createCanvasProjection({ surface } = {}) {
  let nextId = 1;
  let liveId = null;
  let liveTool = null; // begin 에서 기억 — end-stroke 커맨드에는 툴이 없다
  let cursor = 0;

  // 코어 번역 6개 — 레지스트리에 기본 등록. (툴 패키지는 register 로 추가한다)
  const handlers = new Map();
  handlers.set(CommandType.BEGIN_STROKE, (cmd) => {
    liveId = nextId++;
    liveTool = cmd.tool;
    surface.beginLive(liveId, { tool: cmd.tool, point: cmd.point, pressure: cmd.pressure });
  });
  handlers.set(CommandType.EXTEND_STROKE, (cmd) => {
    if (liveId !== null) surface.drawLiveTail(liveId, [cmd.point]); // tail = 새 점만 — O(Δ)
  });
  handlers.set(CommandType.END_STROKE, () => {
    if (liveId !== null) {
      surface.endLive(liveId);
      surface.drawCommitted({ tool: liveTool }); // 확정 메시는 캐시 대상 (불변)
      liveId = null;
      liveTool = null;
    }
  });
  handlers.set(CommandType.ERASE_AT, (cmd) => {
    surface.invalidate({ center: cmd.point, radius: 8 }); // 기존 잉크 변경 → 영역 재생성
  });
  handlers.set(CommandType.END_ERASE, () => {
    /* 무효화는 erase-at 때 이미 끝남 */
  });
  handlers.set(CommandType.UNDO, () => surface.invalidate('page')); // 페이지 전체 재생성

  return {
    /**
     * 열린 번역 레지스트리 — 툴 패키지가 자기 커맨드의 projection 을 스스로
     * 등록한다. 새 툴이 와도 이 switch/레지스트리 코어는 수정되지 않는다.
     */
    register(cmdType, handler) {
      handlers.set(cmdType, handler);
    },

    /** 새 커맨드만 투영하고 소비 개수를 반환한다. */
    project(commands) {
      let consumed = 0;
      for (; cursor < commands.length; cursor++, consumed++) {
        const cmd = commands[cursor];
        const handler = handlers.get(cmd.type);
        if (!handler) {
          // 조용한 데이터 손실 금지 — 알 수 없는 커맨드는 즉시 실패한다
          // (idea #2 의 완전 매칭 계약과 동일한 원칙).
          throw new Error(
            `미처리(unhandled) 커맨드: '${cmd.type}' — projection 핸들러가 등록되지 않았다`,
          );
        }
        handler(cmd, surface); // 핸들러 서명: (cmd, surface) — surface 능력을 스스로 사용
      }
      return consumed;
    },
  };
}
