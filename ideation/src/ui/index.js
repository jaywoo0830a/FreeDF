/**
 * 아이디어: UI — 함수형/테스트 가능한 UI 인터페이스 (스텁).
 *
 * 입력 축(idea4/5)이 증명한 것을 UI 로 일반화한 계약 모음이다:
 *   두 시계 문제 → "프레임은 순수 함수 패스다"
 *   이벤트는 데이터 → "상호작용은 메시지다"
 *   에지는 파괴되지 않는다 → "모든 메시지는 장부로 관측된다"
 *   계측 id 는 공개 계약 → "id 는 노드가 안고 있다"
 *
 * 의존성 그래프 (아래로만):
 *   node    (잎, 의존 0)          — 트리 어휘
 *   tokens  (잎, 의존 0)          — 토큰 데이터 + 순수 스타일 결합
 *   a11y    ─ node, tokens        — 단일 관문 (트리 검증)
 *   harness ─ node                — 헤드리스 런타임 (순수성 경계)
 *   component ─ node              — 모듈화 단위 + 조립
 */

export * from './node.js';
export * from './tokens.js';
export * from './a11y.js';
export * from './harness.js';
export * from './component.js';