//! 프로젝션 어댑터 — 문서 커맨드 → [`CanvasSurface`] 연산 번역 (계약 객체 ⑧ ②③).
//!
//! ideation `idea4/canvas.js` `createCanvasProjection`의 이식. surface의
//! **유일한 호출자**다 — 툴/허브/워크스페이스는 캔버스의 존재를 모른다.
//!
//! - **열린 레지스트리**: 코어 번역이 기본 등록되고, 툴 패키지는
//!   [`Projection::register`]로 자기 커맨드의 projection을 스스로 등록한다.
//!   새 툴이 와도 레지스트리 코어는 수정되지 않는다.
//! - **조용한 데이터 손실 금지**: 핸들러 없는 커맨드는 즉시 실패한다
//!   (idea #2의 완전 매칭 계약과 같은 원칙).
//! - **O(Δ)**: 호출자가 새 커맨드만 넘긴다 — 워크스페이스 큐 drain이 그 역할.
//!
//! 핸들러 시그니처: `(cmd, surface)` — surface 능력을 스스로 사용한다.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::*;
use freedf_canvas::canvas_port::{
    CanvasSurface, Committed, LivePoint, Region, StrokeHead,
};
use freedf_core::input_commands::Command;

/// 라이브 세션 상태 — begin에서 기억한 것을 end에서 쓴다 (end-stroke 커맨드에는
/// 툴이 없다). 핸들러들이 공유한다 (JS 클로저 스코프의 Rc<RefCell> 대응).
#[derive(Default)]
struct LiveState {
    next_id: u64,
    live_id: Option<u64>,
    live_tool: Option<String>,
}

pub(crate) type Handler = Box<dyn FnMut(&Command, &mut dyn CanvasSurface)>;

pub(crate) struct Projection {
    live: Rc<RefCell<LiveState>>,
    handlers: HashMap<String, Handler>,
}

impl Default for Projection {
    fn default() -> Self {
        Self::with_core()
    }
}

impl Projection {
    /// 핸들러 없는 빈 레지스트리 (테스트/완전 수동 구성용).
    pub fn empty() -> Self {
        Self {
            live: Rc::new(RefCell::new(LiveState::default())),
            handlers: HashMap::new(),
        }
    }

    /// 열린 번역 레지스트리 — 새 커맨드 종류의 projection을 등록/교체한다.
    pub fn register(&mut self, kind: &str, handler: Handler) {
        self.handlers.insert(kind.to_string(), handler);
    }

    /// 주어진 커맨드를 모두 투영하고 소비 개수를 반환한다.
    pub fn project(
        &mut self,
        commands: &[Command],
        surface: &mut dyn CanvasSurface,
    ) -> Result<usize, String> {
        let mut consumed = 0;
        for cmd in commands {
            let kind = cmd.kind();
            let Some(handler) = self.handlers.get_mut(kind) else {
                // 조용한 데이터 손실 금지 — 알 수 없는 커맨드는 즉시 실패한다.
                return Err(format!(
                    "미처리(unhandled) 커맨드: '{kind}' — projection 핸들러가 등록되지 않았다"
                ));
            };
            handler(cmd, surface);
            consumed += 1;
        }
        Ok(consumed)
    }
}

impl Projection {
    /// 코어 번역 — 레지스트리에 기본 등록된다 (JS와 동일한 6개 + immediate).
    /// 레지스트리 코어는 새 툴이 와도 수정되지 않는다 — 툴은 register로.
    pub fn with_core() -> Self {
        let mut p = Self::empty();
        let live = p.live.clone();

        // begin-stroke: 새 라이브 세션 — id 발급 + 툴 기억.
        p.register(
            "begin-stroke",
            Box::new(move |cmd, surface| {
                let Command::BeginStroke {
                    tool,
                    point,
                    pressure,
                } = cmd
                else {
                    return;
                };
                let mut st = live.borrow_mut();
                st.next_id += 1;
                let id = st.next_id;
                st.live_id = Some(id);
                st.live_tool = Some(tool.clone());
                surface.begin_live(
                    id,
                    &StrokeHead {
                        tool: tool.clone(),
                        point: *point,
                        pressure: *pressure,
                    },
                );
            }),
        );

        // extend-stroke: tail = 새 점만 — O(Δ).
        let live = p.live.clone();
        p.register(
            "extend-stroke",
            Box::new(move |cmd, surface| {
                let Command::ExtendStroke { point, pressure } = cmd else {
                    return;
                };
                if let Some(id) = live.borrow().live_id {
                    surface.draw_live_tail(
                        id,
                        &[LivePoint {
                            pos: *point,
                            pressure: *pressure,
                        }],
                    );
                }
            }),
        );

        // end-stroke: 세션 종료 + 커밋 마커 (확정 메시는 캐시 대상 — 불변).
        let live = p.live.clone();
        p.register(
            "end-stroke",
            Box::new(move |_cmd, surface| {
                let mut st = live.borrow_mut();
                if let Some(id) = st.live_id.take() {
                    let tool = st.live_tool.take().unwrap_or_default();
                    surface.end_live(id, &Committed { tool });
                }
            }),
        );

        // erase-at: 기존 잉크 변경 → 영역 재생성 (반경 8은 힌트 — 실제 지우개
        // 반경은 백엔드가 알고 있다; JS 코어 번역과 동일).
        p.register(
            "erase-at",
            Box::new(|cmd, surface| {
                let Command::EraseAt { point } = cmd else {
                    return;
                };
                surface.invalidate(Region::Circle {
                    center: *point,
                    radius: 8.0,
                });
            }),
        );

        // end-erase: 무효화는 erase-at 때 이미 끝남.
        p.register("end-erase", Box::new(|_cmd, _surface| {}));

        // undo: 페이지 전체 재생성.
        p.register(
            "undo",
            Box::new(|_cmd, surface| surface.invalidate(Region::Page)),
        );

        // immediate: 실행기가 이미 UI 액션을 처리했다 — 렌더 부수효과 없음.
        // (앱 특화: JS는 이런 키가 없고 알 수 없는 타입이면 throw한다.)
        p.register("immediate", Box::new(|_cmd, _surface| {}));

        p
    }
}

/// 앱 백엔드 — [`CanvasSurface`]를 렌더 캐시 위에 구현한다.
///
/// 이 앱의 렌더는 **rev-diff 기반**이다 (커밋→젊은 획 흡수, 삭제/구조 변경→
/// 전체 재굽기가 paint 루프의 store rev 감시로 이미 이뤄진다). 그래서 코어
/// 연산 대부분은 확인용이지만, void 계약(질의 없음)과 "커맨드의 렌더
/// 부수효과는 이 포트로만 선언된다"는 원칙은 그대로 유지된다.
impl CanvasSurface for FreeDfApp {
    fn capabilities(&self) -> &'static [&'static str] {
        // 현재 코어만 노출 — 선택 툴(개미선 등)이 오면 "overlay"를 선언하고
        // [`OverlayCapability`]를 구현한다 (능력은 선언해서 얻는다).
        &["core"]
    }

    fn begin_live(&mut self, _id: u64, _head: &StrokeHead) {
        // 새 라이브 세션 — 이전 세션의 진행 메시 잔재를 남기지 않는다.
        self.active_mesh = None;
    }

    fn draw_live_tail(&mut self, _id: u64, _tail: &[LivePoint]) {
        // 진행 메시는 paint 루프가 active_stroke 미러를 점 수 키로 재구성한다
        // (주사율 프리셋 스로틀 캐시) — 새 점은 키에 반영되므로 별도 무효화
        // 불필요. (그림 자체는 InkPipeline이 점/폭을 생산한다 — 문서 실행 몫.)
    }

    fn end_live(&mut self, _id: u64, _committed: &Committed) {
        // 진행 메시 캐시 정리. 확정 렌더는 store rev-diff가 젊은 목록으로
        // 흡수한다 (paint 루프 `add_ink_young`).
        self.active_mesh = None;
    }

    fn invalidate(&mut self, region: Region) {
        match region {
            Region::Page => {
                // 페이지 전체 재생성 — egui 변환 캐시를 비워 낡은 화면을 막는다.
                // (구조 재굽기 자체는 store_generation/rev-diff가 담당)
                self.ink_egui_mesh = None;
                self.ink_egui_key = None;
            }
            Region::Circle { .. } => {
                // 지우개 영역 — 삭제는 store에 반영됐고 병합 메시 재굽기는
                // `ink_settling.deleted()`/rev-diff가 담당한다 (확인용 연산).
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use freedf_canvas::canvas_port::{CanvasOp, CanvasRecorder};
    use freedf_core::input_events::{InputEvent, PointerEvent, PointerPhase, PointerSource};
    use freedf_core::input_workspace::Workspace;

    fn pointer(phase: PointerPhase, point: [f32; 2]) -> InputEvent {
        InputEvent::Pointer(PointerEvent {
            source: PointerSource::Pen,
            phase,
            point,
            pressure: 0.5,
            tilt: 0.0,
        })
    }

    /// 코어 번역: 획 세션 → beginLive → drawLiveTail* → endLive.
    #[test]
    fn stroke_session_projects_to_live_ops() {
        let mut p = Projection::with_core();
        let mut s = CanvasRecorder::new();
        let cmds = vec![
            Command::BeginStroke {
                tool: "pen".into(),
                point: [1.0, 1.0],
                pressure: 0.5,
            },
            Command::ExtendStroke {
                point: [2.0, 1.0],
                pressure: 0.6,
            },
            Command::EndStroke,
        ];
        let n = p.project(&cmds, &mut s).expect("모두 처리");
        assert_eq!(n, 3);
        assert_eq!(s.op_kinds(), vec!["beginLive", "drawLiveTail", "endLive"]);
        // begin의 머리와 end의 커밋 마커가 같은 툴 — begin에서 기억한다.
        assert!(matches!(
            &s.ops[0],
            CanvasOp::BeginLive { head, .. } if head.tool == "pen"
        ));
        assert!(matches!(
            &s.ops[2],
            CanvasOp::EndLive { committed, .. } if committed.tool == "pen"
        ));
    }

    /// 코어 번역: erase-at → 원형 무효화, undo → 페이지 무효화.
    #[test]
    fn erase_and_undo_project_to_invalidate() {
        let mut p = Projection::with_core();
        let mut s = CanvasRecorder::new();
        let cmds = vec![
            Command::EraseAt {
                point: [4.0, 4.0],
            },
            Command::EndErase,
            Command::Undo,
        ];
        p.project(&cmds, &mut s).expect("모두 처리");
        assert_eq!(s.op_kinds(), vec!["invalidate", "invalidate"]);
        assert!(matches!(
            &s.ops[0],
            CanvasOp::Invalidate {
                region: Region::Circle {
                    center: [4.0, 4.0],
                    ..
                }
            }
        ));
        assert!(matches!(
            &s.ops[1],
            CanvasOp::Invalidate { region: Region::Page }
        ));
    }

    /// 워크스페이스의 합성 up/down(획 경계 전환)이 projection에서도
    /// **두 개의 온전한 세션 흐름**으로 보인다 — 스트림 어느 층에서나 같은
    /// 불변식이 유지된다 (ideation stroke-boundary 테스트의 투영 버전).
    #[test]
    fn mid_stroke_switch_projects_cleanly() {
        let mut ws = Workspace::new();
        ws.handle(&pointer(PointerPhase::Down, [1.0, 1.0]));
        ws.select("tool:eraser"); // 그리는 중 전환 — 합성 up/down
        ws.handle(&pointer(PointerPhase::Up, [2.0, 1.0]));

        let mut cmds = Vec::new();
        ws.take_commands(|c| cmds.push(c));
        assert!(freedf_core::input_commands::check_well_formed(&cmds).is_ok());

        let mut p = Projection::with_core();
        let mut s = CanvasRecorder::new();
        p.project(&cmds, &mut s).expect("모두 처리");
        assert_eq!(
            s.op_kinds(),
            vec![
                "beginLive",  // 펜 세션
                "endLive",    // 합성 up — 닫힘
                "invalidate", // 합성 down(eraser begin = erase-at) → 무효화
            ] // 이후 Up = end-erase — 무효화는 erase-at 때 끝남 (코어 번역)
        );
    }

    /// 확장 계약: register로 커맨드 번역을 **교체/추가**할 수 있다 — 레지스트리
    /// 코어는 수정되지 않는다.
    #[test]
    fn registry_is_open_for_extension() {
        let mut p = Projection::with_core();
        let seen = Rc::new(RefCell::new(Vec::<String>::new()));
        let seen2 = seen.clone();
        p.register(
            "immediate",
            Box::new(move |cmd, _surface| {
                if let Command::Immediate { key } = cmd {
                    seen2.borrow_mut().push(key.clone());
                }
            }),
        );
        let mut s = CanvasRecorder::new();
        p.project(
            &[Command::Immediate {
                key: "color-wheel".into(),
            }],
            &mut s,
        )
        .expect("모두 처리");
        assert_eq!(*seen.borrow(), vec!["color-wheel".to_string()]);
    }

    /// 조용한 데이터 손실 금지 — 핸들러 없는 커맨드는 즉시 실패한다.
    #[test]
    fn unhandled_command_fails_loudly() {
        let mut p = Projection::empty();
        let mut s = CanvasRecorder::new();
        let err = p
            .project(
                &[Command::BeginStroke {
                    tool: "pen".into(),
                    point: [0.0, 0.0],
                    pressure: 1.0,
                }],
                &mut s,
            )
            .expect_err("핸들러 없음 → Err");
        assert!(err.contains("begin-stroke"));
        assert!(s.ops.is_empty(), "실패한 커맨드는 투영되지 않는다");
    }
}
