/**
 * 디자인 토큰 — 컴포넌트 시스템의 단일 진실 원천 (데이터).
 *
 * Rust 대응: `crates/freedf/src/ui/tokens.rs` — "컴포넌트는 여기 있는 값만 쓴다
 * (매직 넘버 금지)" 규칙의 이식 전 계약. 토큰도 **데이터**다: 테마를 갈아끼우는
 * 것은 함수 인자 하나고, 검증(a11y)이 같은 표를 읽는다.
 */

/** 인터랙티브 타깃(터치/클릭 영역) 하한 — 접근성 계약 (px, 1rem=16 기준). */
export const target = {
  /** 절대 하한 (1.5rem). 이보다 작은 클릭 영역은 만들지 않는다. */
  MIN: 24,
  /** 데스크톱 기본 — 대부분의 아이콘 버튼. */
  COMFORT: 28,
  /** 태블릿/터치 기본 — 손가락 입력이 주가 되는 컨트롤. */
  TOUCH: 32,
  /** 목록/메뉴 행 높이 — 행 전체가 타깃일 때. */
  ROW: 28,
};

/** 여백 리듬 — 8px 그리드의 배수 단위. */
export const space = (step) => step * 8;

/** 기본 테마 (스텁 — 의미 토큰만, 원시 팔레트 금지 규칙의 자리 표시). */
export const dark = {
  colors: {
    bg: '#2e3440',
    surface: '#3b4252',
    text: '#eceff4',
    accent: '#88c0d0',
    danger: '#bf616a',
  },
  radius: { sm: 4, md: 8 },
  font: { size: 14, strong: 16 },
};

/** 종류별 기본 스타일 — 컴포넌트가 반복하지 않게 하는 기본값 표. */
const defaults = {
  text: { color: 'text', size: 14 },
  box: { pad: 1 },
  button: { minHeight: target.COMFORT, radius: 'md' },
  toggle: { minHeight: target.COMFORT, radius: 'sm' },
};

/**
 * 스타일 결합 — **순수 함수**: (노드 스타일, 테마) → 확정 스타일.
 *
 * 노드가 지정한 값이 이기고, 나머지는 종류 기본값이 채운다. 입력을 변형하지
 * 않는다(불변) — 같은 입력은 항상 같은 결과. 렌더러는 이 결과만 그리면 된다.
 */
export const resolve = (nodeStyle, theme = dark) => {
  const base = defaults[nodeStyle.kind] ?? {};
  return { ...base, ...nodeStyle.style };
};
