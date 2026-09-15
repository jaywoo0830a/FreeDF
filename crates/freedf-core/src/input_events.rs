//! 통합 입력 이벤트 어휘 (계약 객체 ① — ideation `idea4/events.js`의 이식).
//!
//! 장치 축(어댑터/허브)과 툴 축(툴 상태기계)이 공유하는 **유일한** 모듈이다.
//! 이 모듈은 아무것도 의존하지 않는 잎(leaf)이어야 한다 — 여기에 무엇을
//! 추가할 때는 "장치인가, 툴인가, 문서인가"를 먼저 묻고, 장치와 툴이
//! 동시에 알아야 하는 것만 들어온다.
//!
//! 어휘 규칙 (idea #2/#3/#4에서 확정):
//! - 포인터의 압력/기울기는 **항상 존재**한다. 기본값 채움(능력 협상)은
//!   장치 어댑터의 책임이고, 툴은 fallback을 모른다.
//! - 하드웨어 컨트롤의 정체성은 (종류, 번호) 쌍이다. 버튼 "개수"는 어휘에
//!   없다 — 펜에 버튼이 0개든 5개든, 패드에 매크로 키가 몇 개든 이 어휘는
//!   변하지 않는다.
//! - 논리 동작(action)의 키는 열린 문자열이다 — 툴 레지스트리(예:
//!   `"tool:pen"`, `"undo"`)가 이 네임스페이스를 소유한다.

/// 이벤트 대분류 — [`InputEvent::kind`]가 반환한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventKind {
    /// 포인터 (펜/마우스/패드) — 그리기·탐색의 원료.
    Pointer,
    /// 논리 동작 (툴 선택, undo 등) — 사용자 매핑/단축키의 결과물.
    Action,
    /// 원시 하드웨어 컨트롤 — 사용자 매핑으로 번역되기 **전** 상태.
    Control,
}

/// 포인터 소스 — 허브 충돌 규칙("한 번에 한 포인터")의 대상이 되는 소스들.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PointerSource {
    /// 태블릿 스타일러스 (evdev/OTD 스트림).
    Pen,
    /// 마우스 (egui 포인터).
    Mouse,
    /// 터치/트랙패드.
    Pad,
    /// 태블릿 본체 (포인터는 펜이 담당 — 컨트롤용 예약).
    Tablet,
}

/// 충돌 규칙의 대상이 되는 포인터 소스 전체.
pub const POINTER_SOURCES: &[PointerSource] = &[
    PointerSource::Pen,
    PointerSource::Mouse,
    PointerSource::Pad,
    PointerSource::Tablet,
];

/// 포인터 위상 — 다운/드래그/업 3단계 (획 경계는 워크스페이스가 합성한다).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PointerPhase {
    Down,
    Drag,
    Up,
}

/// 하드웨어 컨트롤 종류 — (종류, 번호) 쌍의 "종류".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ControlKind {
    /// 스타일러스 측면 버튼.
    StylusButton,
    /// 태블릿 본체 익스프레스 키/매크로 키.
    ExpressKey,
}

/// 컨트롤 위상.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ControlPhase {
    Down,
    Up,
}

/// action 모드 — 탭(즉시 실행) 또는 홀드(누르는 동안 툴 교체).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActionMode {
    Tap,
    HoldOn,
    HoldOff,
}

/// action의 발행자 — 키보드 또는 (번역된) 하드웨어 컨트롤.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ActionSource {
    Keyboard,
    Control(ControlKind),
}

/// 포인터 이벤트. 압력/기울기는 어댑터가 기본값을 채워 "항상 존재"한다.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointerEvent {
    pub source: PointerSource,
    pub phase: PointerPhase,
    /// 경계 좌표 (어댑터가 놓는 공간 — 앱에서는 캔버스 UI 좌표).
    /// sense-normalization(페이지 좌표 변환)은 소비자 쪽 지오메트리 포트가 한다.
    pub point: [f32; 2],
    /// 필압 0..1 (미지원 장치는 어댑터가 1.0으로 채움).
    pub pressure: f32,
    /// 기울기 크기 (도, 0..=90). 미지원 장치는 0.
    pub tilt: f32,
}

/// 논리 동작 이벤트 — `key`는 툴 레지스트리 네임스페이스의 열린 문자열.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionEvent {
    pub source: ActionSource,
    pub key: String,
    /// 홀드 매핑일 때만 Some — 탭 매핑은 None.
    pub mode: Option<ActionMode>,
}

/// 원시 하드웨어 컨트롤 이벤트 — (종류, 번호) 쌍. 번역 전 상태.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ControlEvent {
    pub control: ControlKind,
    /// 1부터 시작하는 번호 (장치 문서의 버튼 번호와 맞춘다).
    pub index: u8,
    pub phase: ControlPhase,
}

/// 통합 입력 이벤트 — 닫힌 판별 유니온. Rust enum이 곧 "전체 매칭" 계약이다:
/// 소비자는 `match`로 모든 변형을 다뤄야 하고, 새 변형 추가는 컴파일 오류로
/// 모든 소비자에게 알려진다.
#[derive(Debug, Clone, PartialEq)]
pub enum InputEvent {
    Pointer(PointerEvent),
    Action(ActionEvent),
    Control(ControlEvent),
}

impl InputEvent {
    /// 편의 생성자 — JS 프로토타입의 `Pointer()` 팩터리에 대응.
    pub fn pointer(
        source: PointerSource,
        phase: PointerPhase,
        point: [f32; 2],
        pressure: f32,
        tilt: f32,
    ) -> Self {
        InputEvent::Pointer(PointerEvent {
            source,
            phase,
            point,
            pressure,
            tilt,
        })
    }

    /// 편의 생성자 — `Action(source, key, mode)`에 대응.
    pub fn action(source: ActionSource, key: impl Into<String>, mode: Option<ActionMode>) -> Self {
        InputEvent::Action(ActionEvent {
            source,
            key: key.into(),
            mode,
        })
    }

    /// 편의 생성자 — `RawControl(control, index, phase)`에 대응.
    pub fn control(control: ControlKind, index: u8, phase: ControlPhase) -> Self {
        InputEvent::Control(ControlEvent {
            control,
            index,
            phase,
        })
    }

    /// 대분류 조회 — `kind` 문자열 비교(어휘의 구식 검사) 대신 enum으로.
    pub fn kind(&self) -> EventKind {
        match self {
            InputEvent::Pointer(_) => EventKind::Pointer,
            InputEvent::Action(_) => EventKind::Action,
            InputEvent::Control(_) => EventKind::Control,
        }
    }
}
