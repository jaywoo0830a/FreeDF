//! `input_workspace` 모듈 단위 테스트 — `src/input_workspace.rs`의 `#[cfg(test)]` 모듈에서 이동했습니다.

use freedf_core::input_commands::Command;
use freedf_core::input_commands::{check_well_formed, command_kinds};
use freedf_core::input_events::{
    ActionMode, ActionSource, ControlKind, InputEvent, PointerEvent, PointerPhase, PointerSource,
    NO_TILT,
};
use freedf_core::input_tools::Tool;
use freedf_core::input_workspace::*;

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
    use freedf_core::input_tools::Emit;

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
