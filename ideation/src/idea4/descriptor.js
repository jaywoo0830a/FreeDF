/**
 * 계약 객체 ② — 장치 기술자(descriptor).
 *
 * 하드웨어 차이(버튼 개수, 압력/기울기 지원)는 코드가 아니라 데이터다.
 * 설정 UI 와 기본 프로필은 기술자를 읽어 만들어진다.
 */

export function deviceDescriptor({
  stylusButtons = 0,
  expressKeys = 0,
  pressure = false,
  tilt = false,
} = {}) {
  return { stylusButtons, expressKeys, pressure, tilt };
}

/** 매핑 키 규칙 — 컨트롤 열거와 사용자 바인딩이 이 규칙 하나를 공유한다. */
export const controlKey = (control, index) => `${control}#${index}`;

/** 기술자 → 설정 UI 가 다룰 컨트롤 목록. */
export function enumerateControls(desc) {
  const controls = [];
  for (let i = 0; i < desc.stylusButtons; i++) {
    controls.push({ control: 'stylus-button', index: i });
  }
  for (let i = 0; i < desc.expressKeys; i++) {
    controls.push({ control: 'express-key', index: i });
  }
  return controls;
}
