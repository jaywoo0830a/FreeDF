//! 툴 상태기계 (툴 축 — 계약 객체 ⑥, ideation `idea4/tools.js`의 이식).
//!
//! 툴 계약: 포인터 이벤트만 소비하고 action은 무시한다 (툴 선택은
//! 워크스페이스 몫). 항상 온전한 down→…→up 스트림을 받는다고 가정한다
//! (전환은 워크스페이스가 합성 up/down으로 보장). 문서 커맨드만 생산한다 —
//! 렌더/캔버스/장치 지식 0.
//!
//! import는 [`crate::input_commands`](출력 어휘)와 [`crate::input_events`]
//! (입력 어휘)뿐이다.

use crate::input_commands::Command;
use crate::input_events::{InputEvent, PointerEvent, PointerPhase};

/// 커맨드 방출구 — 툴은 컬렉션을 모른다.
pub type Emit<'a> = &'a mut dyn FnMut(Command);

/// 전이 표의 한 칸 — 포인터 이벤트를 커맨드로 번역하는 순수 함수.
type Transition = Box<dyn Fn(&PointerEvent, Emit)>;

/// 툴 상태기계 계약.
pub trait Tool {
    /// 워크스페이스 레지스트리 키 (`"tool:NAME"` 액션의 NAME).
    fn name(&self) -> &str;
    fn handle(&mut self, event: &InputEvent, emit: Emit);
}

/// 공유 상태기계 뼈대 — 툴은 "전이 표"만 적는다 (JS `pointerTool`).
///
/// idle에서 Down이면 세션 시작, active에서 Drag면 확장, active에서 Up이면
/// 세션 종료. 그 외 전이는 모두 무시 (잃어버린 Up에도 안전 — 세션은
/// 워크스페이스의 합성 up/down으로 닫힌다).
pub struct PointerTool {
    name: &'static str,
    active: bool,
    begin: Transition,
    extend: Transition,
    end: Transition,
}

impl PointerTool {
    pub fn new(
        name: &'static str,
        begin: impl Fn(&PointerEvent, Emit) + 'static,
        extend: impl Fn(&PointerEvent, Emit) + 'static,
        end: impl Fn(&PointerEvent, Emit) + 'static,
    ) -> Self {
        Self {
            name,
            active: false,
            begin: Box::new(begin),
            extend: Box::new(extend),
            end: Box::new(end),
        }
    }
}

impl Tool for PointerTool {
    fn name(&self) -> &str {
        self.name
    }

    fn handle(&mut self, event: &InputEvent, emit: Emit) {
        let InputEvent::Pointer(e) = event else {
            return; // action/control은 워크스페이스/컨트롤 맵 몫
        };
        match (self.active, e.phase) {
            (false, PointerPhase::Down) => {
                self.active = true;
                (self.begin)(e, emit);
            }
            (true, PointerPhase::Drag) => (self.extend)(e, emit),
            (true, PointerPhase::Up) => {
                self.active = false;
                (self.end)(e, emit);
            }
            _ => {}
        }
    }
}

/// 볼펜 — 필압 그대로 (두께 확정은 파이프라인/머셔 몫).
pub fn pen_tool() -> PointerTool {
    PointerTool::new(
        "pen",
        |e, emit| {
            emit(Command::BeginStroke {
                tool: "pen".into(),
                point: e.point,
                pressure: e.pressure,
            })
        },
        |e, emit| {
            emit(Command::ExtendStroke {
                point: e.point,
                pressure: e.pressure,
            })
        },
        |_e, emit| emit(Command::EndStroke),
    )
}

/// 만년필 — 볼펜과 동일한 스트림, 재료(프로필)만 다르다.
pub fn fountain_tool() -> PointerTool {
    PointerTool::new(
        "fountain",
        |e, emit| {
            emit(Command::BeginStroke {
                tool: "fountain".into(),
                point: e.point,
                pressure: e.pressure,
            })
        },
        |e, emit| {
            emit(Command::ExtendStroke {
                point: e.point,
                pressure: e.pressure,
            })
        },
        |_e, emit| emit(Command::EndStroke),
    )
}

/// 형광펜 — 앱의 기존 동작을 보존하기 위해 필압을 그대로 통과시킨다
/// (JS 프로토타입의 "고정 1.0"과 다른 점 — 투명 강조선의 두께는
/// 실행기/머셔 정책이 결정한다).
pub fn highlighter_tool() -> PointerTool {
    PointerTool::new(
        "highlighter",
        |e, emit| {
            emit(Command::BeginStroke {
                tool: "highlighter".into(),
                point: e.point,
                pressure: e.pressure,
            })
        },
        |e, emit| {
            emit(Command::ExtendStroke {
                point: e.point,
                pressure: e.pressure,
            })
        },
        |_e, emit| emit(Command::EndStroke),
    )
}

/// 지우개 — 같은 전이 표, 다른 출력 어휘 (erase 세션).
pub fn eraser_tool() -> PointerTool {
    PointerTool::new(
        "eraser",
        |e, emit| emit(Command::EraseAt { point: e.point }),
        |e, emit| emit(Command::EraseAt { point: e.point }),
        |_e, emit| emit(Command::EndErase),
    )
}

/// 팬(탐색) — 문서 커맨드를 생산하지 않는 툴. 레지스트리에 있어야 "활성 툴"
/// 상태가 하나로 유지된다 (실제 팬 이동은 앱의 뷰 정책이 담당한다).
pub fn pan_tool() -> PointerTool {
    PointerTool::new("pan", |_e, _emit| {}, |_e, _emit| {}, |_e, _emit| {})
}

/// 기본 툴 레지스트리 — 워크스페이스가 action 키(`"tool:NAME"`)로 조회한다.
pub fn default_tools() -> Vec<Box<dyn Tool>> {
    vec![
        Box::new(pen_tool()),
        Box::new(fountain_tool()),
        Box::new(highlighter_tool()),
        Box::new(eraser_tool()),
        Box::new(pan_tool()),
    ]
}
