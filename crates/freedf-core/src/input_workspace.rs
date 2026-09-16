//! 워크스페이스 (계약 객체 ⑦ — ideation `idea4/workspace.js`의 이식).
//!
//! "정책의 집":
//! ① 활성 툴 선택 (action `"tool:NAME"` — 탭/홀드 모두, 홀드는 LIFO 스택)
//! ② 획 경계 보장 — 그리는 중 툴을 바꾸면 **합성 up/down**으로 이전 툴의
//!    세션을 닫고 새 툴의 세션을 연다 (각 툴은 항상 온전한 스트림만 본다)
//! ③ 즉시 커맨드(undo 등)의 문서 커맨드 통과.
//!
//! 장치 지식 0 — 포인터가 어느 장치에서 왔는지 정책에 쓰지 않는다.
//! pull 모델: 이벤트는 [`Workspace::handle`]로 밀어 넣고, 생산된 커맨드는
//! [`Workspace::take_commands`]로 당겨 간다 (프레임 단위 소비).

use std::collections::VecDeque;

use crate::input_commands::Command;
use crate::input_events::{
    ActionEvent, ActionMode, ActionSource, InputEvent, PointerPhase, PointerSource, NO_TILT,
};
use crate::input_tools::{default_tools, Tool};

const TOOL_PREFIX: &str = "tool:";

pub struct Workspace {
    tools: Vec<Box<dyn Tool>>,
    active: usize,
    /// 포인터 점유 — Down/Up **에지**로만 갱신한다 (Drag로는 바뀌지 않는다:
    /// 호버 이동·부분 필터링된 스트림에서도 세션 추적이 유지되게).
    pointer_down: bool,
    /// 마지막 포인터 위치/소스 — 합성 up/down이 재사용한다.
    last_point: [f32; 2],
    last_source: PointerSource,
    /// hold 수정자 스택 — 홀드 중 홀드(중첩)도 원복 순서가 보장된다.
    held: Vec<usize>,
    commands: VecDeque<Command>,
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new()
    }
}

impl Workspace {
    pub fn new() -> Self {
        Self::with_tools(default_tools())
    }

    pub fn with_tools(tools: Vec<Box<dyn Tool>>) -> Self {
        Self {
            tools,
            active: 0,
            pointer_down: false,
            last_point: [0.0; 2],
            last_source: PointerSource::Pen,
            held: Vec::new(),
            commands: VecDeque::new(),
        }
    }

    /// 통합 이벤트 1건 소비.
    pub fn handle(&mut self, event: &InputEvent) {
        match event {
            InputEvent::Pointer(p) => {
                match p.phase {
                    PointerPhase::Down => self.pointer_down = true,
                    PointerPhase::Up => self.pointer_down = false,
                    PointerPhase::Drag => {}
                }
                self.last_point = p.point;
                self.last_source = p.source;
                self.feed_active(event);
            }
            InputEvent::Action(a) => self.handle_action(a),
            // 원시 컨트롤은 워크스페이스에 들어오지 않는다 — 컨트롤 맵이
            // action으로 번역한 것만 온다 (하드웨어 정체성은 여기까지 안 온다).
            InputEvent::Control(_) => {}
        }
    }

    fn handle_action(&mut self, a: &ActionEvent) {
        match a.mode {
            Some(ActionMode::HoldOn) => {
                self.held.push(self.active);
                let target = self.tool_index(&a.key).unwrap_or(self.active);
                self.switch_to(target);
            }
            Some(ActionMode::HoldOff) => {
                let prev = self.held.pop().unwrap_or(self.active);
                self.switch_to(prev);
            }
            _ => {
                if let Some(idx) = self.tool_index(&a.key) {
                    self.switch_to(idx);
                } else if a.key.starts_with(TOOL_PREFIX) {
                    // 레지스트리에 아직 없는 툴 키 — 조용히 무시한다 (JS 계약과
                    // 동일: switchTo(undefined)는 no-op). 확장은 등록만으로.
                } else if a.key == "undo" {
                    self.commands.push_back(Command::Undo);
                } else {
                    // 즉시 커맨드 통과 — 실행기가 키를 해석한다.
                    self.commands
                        .push_back(Command::Immediate { key: a.key.clone() });
                }
            }
        }
    }

    /// "툴 전환은 획 경계에서 일어난다": 그리는 중이라면 합성 up/down으로
    /// 이전 툴의 세션을 닫고 새 툴의 세션을 연다. 툴 상태기계는 단순함이
    /// 유지된다 (자기 전이 표만 알면 된다).
    fn switch_to(&mut self, target: usize) {
        if target == self.active || target >= self.tools.len() {
            return;
        }
        let mut out: Vec<Command> = Vec::new();
        if self.pointer_down {
            // 합성 경계 이벤트(툴 전환 시 up/down 쌍) — 장치가 없으므로
            // 틸트는 능력 기본값([0,0])이다.
            let up = InputEvent::pointer(
                self.last_source,
                PointerPhase::Up,
                self.last_point,
                1.0,
                NO_TILT,
            );
            self.tools[self.active].handle(&up, &mut |c| out.push(c));
            self.active = target;
            let down = InputEvent::pointer(
                self.last_source,
                PointerPhase::Down,
                self.last_point,
                1.0,
                NO_TILT,
            );
            self.tools[self.active].handle(&down, &mut |c| out.push(c));
        } else {
            self.active = target;
        }
        self.commands.extend(out);
    }

    /// 활성 툴에 이벤트를 먹인다 (생산된 커맨드는 큐로).
    fn feed_active(&mut self, event: &InputEvent) {
        let mut out: Vec<Command> = Vec::new();
        self.tools[self.active].handle(event, &mut |c| out.push(c));
        self.commands.extend(out);
    }

    /// `"tool:NAME"` (또는 `"NAME"`) → 레지스트리 인덱스.
    fn tool_index(&self, key: &str) -> Option<usize> {
        let name = key.strip_prefix(TOOL_PREFIX).unwrap_or(key);
        self.tools.iter().position(|t| t.name() == name)
    }

    /// 생산된 커맨드를 **큐 순서대로** 소비한다.
    pub fn take_commands(&mut self, mut consumer: impl FnMut(Command)) {
        for cmd in self.commands.drain(..) {
            consumer(cmd);
        }
    }

    /// 활성 툴 이름 — 앱이 렌더 상태(ToolType 등)와 동기화할 때 읽는다.
    pub fn active_name(&self) -> &str {
        self.tools[self.active].name()
    }

    /// 홀드 스택 깊이 (진단/테스트용).
    pub fn held_count(&self) -> usize {
        self.held.len()
    }

    /// 프로그래매틱 선택 — 키보드/휠/툴바가 쓰는 공용 입구.
    /// action으로 들어가므로 홀드 중 전환 등 모든 정책이 동일하게 적용된다.
    pub fn select(&mut self, key: &str) {
        self.handle(&InputEvent::action(ActionSource::Keyboard, key, None));
    }

    /// 실행 중 새 툴 등록 (툴 패키지 조립용 — 확장이 프레임워크 수정이 아니다).
    pub fn add_tool(&mut self, tool: Box<dyn Tool>) {
        self.tools.push(tool);
    }

    /// 획 세션 진행 여부 (진단/테스트용).
    pub fn pointer_down(&self) -> bool {
        self.pointer_down
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input_commands::{check_well_formed, command_kinds};
    use crate::input_events::{ControlKind, PointerEvent, NO_TILT};

    fn pointer(phase: PointerPhase, point: [f32; 2]) -> InputEvent {
        InputEvent::Pointer(PointerEvent {
            source: PointerSource::Pen,
            phase,
            point,
            pressure: 0.5,
            tilt: NO_TILT,
        })
    }

    #[test]
    fn pointer_stream_becomes_well_formed_erase_session() {
        let mut ws = Workspace::new();
        ws.select("tool:eraser");
        assert_eq!(ws.active_name(), "eraser");
        ws.handle(&pointer(PointerPhase::Down, [1.0, 1.0]));
        ws.handle(&pointer(PointerPhase::Drag, [2.0, 1.0]));
        ws.handle(&pointer(PointerPhase::Up, [2.0, 1.0]));
        let mut cmds = Vec::new();
        ws.take_commands(|c| cmds.push(c));
        assert_eq!(
            command_kinds(&cmds),
            vec!["erase-at", "erase-at", "end-erase"]
        );
        assert!(check_well_formed(&cmds).is_ok());
    }

    #[test]
    fn switching_mid_stroke_synthesizes_up_down() {
        let mut ws = Workspace::new();
        ws.handle(&pointer(PointerPhase::Down, [1.0, 1.0]));
        ws.handle(&pointer(PointerPhase::Drag, [2.0, 1.0]));
        // 그리는 중 펜 → 지우개: 합성 up/down이 세션을 이어준다.
        ws.select("tool:eraser");
        ws.handle(&pointer(PointerPhase::Drag, [3.0, 1.0]));
        ws.handle(&pointer(PointerPhase::Up, [3.0, 1.0]));
        let mut cmds = Vec::new();
        ws.take_commands(|c| cmds.push(c));
        assert_eq!(
            command_kinds(&cmds),
            vec![
                "begin-stroke",
                "extend-stroke",
                "end-stroke", // 합성 up — 펜 세션 닫힘
                "erase-at",   // 합성 down — 지우개 세션 시작 (begin = erase-at)
                "erase-at",   // 이후 Drag — 같은 erase 세션 (연속 지우기)
                "end-erase",
            ]
        );
        assert!(check_well_formed(&cmds).is_ok());
    }

    #[test]
    fn hold_replaces_tool_and_lifo_restores() {
        let mut ws = Workspace::new();
        ws.select("tool:fountain");
        let hold = |mode| {
            InputEvent::action(
                ActionSource::Control(ControlKind::StylusButton),
                "tool:eraser",
                Some(mode),
            )
        };
        // 홀드 시작 → 지우개로 교체
        ws.handle(&hold(ActionMode::HoldOn));
        assert_eq!(ws.active_name(), "eraser");
        assert_eq!(ws.held_count(), 1);
        // 홀드 중 탭 전환 — LIFO가 아니라 일반 전환 (스택은 그대로)
        ws.select("tool:pen");
        assert_eq!(ws.active_name(), "pen");
        assert_eq!(ws.held_count(), 1);
        // 홀드 해제 → 홀드 직전 툴(fountain)로 복귀
        ws.handle(&hold(ActionMode::HoldOff));
        assert_eq!(ws.active_name(), "fountain");
        assert_eq!(ws.held_count(), 0);
        // 스택이 빈 상태의 HoldOff — 무해 (현재 툴 유지)
        ws.handle(&hold(ActionMode::HoldOff));
        assert_eq!(ws.active_name(), "fountain");
    }

    #[test]
    fn unknown_action_passes_through_as_immediate() {
        let mut ws = Workspace::new();
        ws.handle(&InputEvent::action(
            ActionSource::Keyboard,
            "color-wheel",
            None,
        ));
        let mut cmds = Vec::new();
        ws.take_commands(|c| cmds.push(c));
        assert_eq!(command_kinds(&cmds), vec!["immediate"]);
    }

    #[test]
    fn hover_drags_do_not_trick_session_tracking() {
        let mut ws = Workspace::new();
        // Down 없이 Drag만 흐른 스트림 (캔버스 밖에서 시작한 드래그 등) —
        // 세션 추적은 에지 기반이라 오염되지 않는다.
        ws.handle(&pointer(PointerPhase::Drag, [5.0, 5.0]));
        ws.select("tool:pen");
        ws.handle(&pointer(PointerPhase::Drag, [6.0, 6.0]));
        let mut cmds = Vec::new();
        ws.take_commands(|c| cmds.push(c));
        assert_eq!(command_kinds(&cmds), Vec::<&str>::new());
        assert!(!ws.pointer_down());
    }

    /// 확장성 계약 — 새 툴(Lasso)은 `add_tool` 등록 한 번으로 동작한다.
    /// 프레임워크(워크스페이스/허브/툴 뼈대) 수정 0이 이 테스트로 증명된다.
    #[test]
    fn new_tool_joins_registry_without_framework_changes() {
        use crate::input_events::PointerEvent;
        use crate::input_tools::Emit;

        struct LassoTool {
            session: bool,
        }
        impl Tool for LassoTool {
            fn name(&self) -> &str {
                "lasso"
            }
            fn handle(&mut self, event: &InputEvent, emit: Emit) {
                let InputEvent::Pointer(e) = event else {
                    return;
                };
                match (self.session, e.phase) {
                    (false, PointerPhase::Down) => {
                        self.session = true;
                        emit(Command::Immediate {
                            key: "lasso-begin".into(),
                        });
                    }
                    (true, PointerPhase::Drag) => emit(Command::Immediate {
                        key: "lasso-move".into(),
                    }),
                    (true, PointerPhase::Up) => {
                        self.session = false;
                        emit(Command::Immediate {
                            key: "lasso-end".into(),
                        });
                    }
                    _ => {}
                }
            }
        }

        let mut ws = Workspace::new();
        ws.add_tool(Box::new(LassoTool { session: false }));
        ws.select("tool:lasso");
        assert_eq!(ws.active_name(), "lasso");
        ws.handle(&pointer(PointerPhase::Down, [1.0, 0.0]));
        ws.handle(&pointer(PointerPhase::Drag, [2.0, 0.0]));
        ws.handle(&pointer(PointerPhase::Up, [2.0, 0.0]));
        let mut keys = Vec::new();
        ws.take_commands(|c| match c {
            Command::Immediate { key } => keys.push(key),
            other => panic!("lasso는 문서 세션을 만지지 않는다: {other:?}"),
        });
        assert_eq!(keys, vec!["lasso-begin", "lasso-move", "lasso-end"]);
    }
}
