//! CanvasSurface 포트 — 문서 커맨드 → 캔버스 출력 계약 (계약 객체 ⑧).
//!
//! ideation `idea4/canvas.js`의 이식. 형제 모듈 [`crate::surface`]의
//! `Surface`(굽힌 프레임 → GPU 제출)와 다른 층위다: 이 포트는 **문서 커맨드의
//! 캔버스 부수효과**(라이브 세션/커밋/무효화)를 받는 출력 경계다.
//!
//! - **그리기 전용, 질의 0** — 모든 연산이 void다. 캔버스는 되묻지 않는다.
//!   (입력 질의는 별개의 Geometry port — 앱에서는 뷰 변환(`view_to_page`)이 담당)
//! - **capability**: 코어('core') 연산 외의 능력('overlay' 등)은 선언한
//!   구현에만 존재한다 — 포트를 키우는 대신 능력으로 분화 (신 인터페이스 방지).
//!   [`CanvasSurface::has_capability`]로 선언 여부를 데이터로 조회한다.
//! - [`CanvasRecorder`]는 녹음 스텁 = 포트 계약의 명세다. 스텁이 통과하는
//!   시퀀스가 곧 계약이다.
//!
//! 코어 연산: begin_live → draw_live_tail* → end_live(+committed) → invalidate
//!
//! 좌표 공간: 포트는 자기 공간(앱 백엔드 = 페이지 좌표)의 데이터를 받는다.
//! 커맨드가 경계 좌표(스크린)로 도착하는 앱에서는 백엔드가 sense 정규화한다
//! (canvas.js ① Geometry port의 몫).

/// 코어 능력 상수 — 툴 패키지 requires 검증이 이 문자열을 공유한다.
pub const CAP_CORE: &str = "core";
/// 오버레이 능력 상수.
pub const CAP_OVERLAY: &str = "overlay";

/// 라이브 세션의 머리 — 툴 + 시작점 + 필압. 색/두께 같은 **외형**은 백엔드가
/// 문서 상태(현재 스타일)에서 유도한다 (포트는 되묻지 않는 대신 외형을 몰라도 된다).
#[derive(Debug, Clone, PartialEq)]
pub struct StrokeHead {
    pub tool: String,
    pub point: [f32; 2],
    pub pressure: f32,
}

/// 라이브 꼬리의 새 점 하나 (O(Δ) — 새 점만 흘린다).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LivePoint {
    pub pos: [f32; 2],
    pub pressure: f32,
}

/// 커밋 마커 — 방금 확정된 획의 정체. 실제 메시는 백엔드가 문서(rev-diff)에서
/// 굽는다 (JS `drawCommitted({ tool })`의 마커 계약과 동일).
#[derive(Debug, Clone, PartialEq)]
pub struct Committed {
    pub tool: String,
}

/// 무효화 영역 — 캐시/재굽기의 부수효과 지시.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Region {
    /// 페이지 전체 재생성 (undo 등 구조 변경).
    Page,
    /// 이 지점 주변 잉크가 변경됐다 (지우개). `radius`는 힌트 — 백엔드가
    /// 실제 반경(도구 설정)을 알고 있으면 그것을 따른다.
    Circle { center: [f32; 2], radius: f32 },
}

/// 오버레이 도형 — 'overlay' 능력의 페이로드 (선택 개미선 등). 페이지 좌표.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OverlayShape {
    Rect {
        min: [f32; 2],
        max: [f32; 2],
        color: [u8; 4],
    },
    Circle {
        center: [f32; 2],
        radius: f32,
        color: [u8; 4],
    },
}

/// 캔버스 출력 포트 — 코어 4연산 + 능력 조회. 구현은 절대 이 호출로 되묻지
/// 않는다 (모든 메서드가 void).
pub trait CanvasSurface {
    /// 노출된 능력 목록 — 툴 패키지의 requires 검증이 이 데이터를 읽는다.
    fn capabilities(&self) -> &[&'static str] {
        &[CAP_CORE]
    }

    /// 능력 선언 조회 (확장 툴이 코어 밖 연산을 쓰기 전에 확인한다).
    fn has_capability(&self, cap: &str) -> bool {
        self.capabilities().contains(&cap)
    }

    /// 라이브 세션 시작 (펜이 닿았다).
    fn begin_live(&mut self, id: u64, head: &StrokeHead);
    /// 진행 중 획에 새 점들 (O(Δ) — tail에는 새 점만).
    fn draw_live_tail(&mut self, id: u64, tail: &[LivePoint]);
    /// 라이브 세션 종료 + 확정 마커.
    fn end_live(&mut self, id: u64, committed: &Committed);
    /// 기존 잉크가 변경됐다 (지우개/undo — 렌더 캐시 부수효과).
    fn invalidate(&mut self, region: Region);
}

/// 'overlay' 능력 — 선언한 구현만 제공하는 확장 출력. 코어 트레잇을 키우지
/// 않고 능력 트레잇으로 분화한다 (신 인터페이스 방지).
pub trait OverlayCapability: CanvasSurface {
    fn overlay(&mut self, shape: &OverlayShape);
}

// ---------- 녹음 스텁 = 포트 계약의 명세 ----------

/// 포트가 생산한 연산 기록 — 테스트/리플레이의 원료.
#[derive(Debug, Clone, PartialEq)]
pub enum CanvasOp {
    BeginLive { id: u64, head: StrokeHead },
    DrawLiveTail { id: u64, points: usize },
    EndLive { id: u64, committed: Committed },
    Invalidate { region: Region },
    Overlay { shape: OverlayShape },
}

impl CanvasOp {
    /// 진단/테스트용 연산 이름 (JS 스텁의 `op` 문자열과 동일한 케이스).
    pub fn kind(&self) -> &'static str {
        match self {
            CanvasOp::BeginLive { .. } => "beginLive",
            CanvasOp::DrawLiveTail { .. } => "drawLiveTail",
            CanvasOp::EndLive { .. } => "endLive",
            CanvasOp::Invalidate { .. } => "invalidate",
            CanvasOp::Overlay { .. } => "overlay",
        }
    }
}

/// 녹음 스텁 — 스텁 구현이 곧 포트 계약의 명세다.
#[derive(Debug, Default, Clone)]
pub struct CanvasRecorder {
    caps: Vec<&'static str>,
    /// 기록된 연산 — void 계약: 그리기 전용, 상태를 되묻지 않는다.
    pub ops: Vec<CanvasOp>,
}

impl CanvasRecorder {
    pub fn new() -> Self {
        Self {
            caps: vec![CAP_CORE],
            ops: Vec::new(),
        }
    }

    /// 'overlay' 능력을 선언한 스텁 — 능력이 있어야 메서드가 의미 있다.
    pub fn with_overlay() -> Self {
        Self {
            caps: vec![CAP_CORE, CAP_OVERLAY],
            ops: Vec::new(),
        }
    }

    /// 기록된 연산의 이름 목록 (테스트 단정용).
    pub fn op_kinds(&self) -> Vec<&'static str> {
        self.ops.iter().map(|o| o.kind()).collect()
    }
}

impl CanvasSurface for CanvasRecorder {
    fn capabilities(&self) -> &[&'static str] {
        &self.caps
    }

    fn begin_live(&mut self, id: u64, head: &StrokeHead) {
        self.ops.push(CanvasOp::BeginLive {
            id,
            head: head.clone(),
        });
    }

    fn draw_live_tail(&mut self, id: u64, tail: &[LivePoint]) {
        self.ops.push(CanvasOp::DrawLiveTail {
            id,
            points: tail.len(),
        });
    }

    fn end_live(&mut self, id: u64, committed: &Committed) {
        self.ops.push(CanvasOp::EndLive {
            id,
            committed: committed.clone(),
        });
    }

    fn invalidate(&mut self, region: Region) {
        self.ops.push(CanvasOp::Invalidate { region });
    }
}

impl OverlayCapability for CanvasRecorder {
    fn overlay(&mut self, shape: &OverlayShape) {
        self.ops.push(CanvasOp::Overlay { shape: *shape });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn head() -> StrokeHead {
        StrokeHead {
            tool: "pen".into(),
            point: [1.0, 2.0],
            pressure: 0.5,
        }
    }

    /// 계약: 라이브 세션은 begin → tail* → end(+commit) 순서다.
    #[test]
    fn live_session_records_in_contract_order() {
        let mut s = CanvasRecorder::new();
        assert_eq!(s.capabilities(), &[CAP_CORE]);
        s.begin_live(1, &head());
        s.draw_live_tail(
            1,
            &[LivePoint {
                pos: [3.0, 2.0],
                pressure: 0.6,
            }],
        );
        s.end_live(
            1,
            &Committed {
                tool: "pen".into(),
            },
        );
        assert_eq!(s.op_kinds(), vec!["beginLive", "drawLiveTail", "endLive"]);
        // tail은 새 점만 — O(Δ) 계약.
        assert_eq!(s.ops[1], CanvasOp::DrawLiveTail { id: 1, points: 1 });
    }

    /// 계약: invalidate는 영역을 데이터로 지시한다 (지우개=원, undo=페이지).
    #[test]
    fn invalidate_carries_region_data() {
        let mut s = CanvasRecorder::new();
        s.invalidate(Region::Circle {
            center: [5.0, 5.0],
            radius: 8.0,
        });
        s.invalidate(Region::Page);
        assert_eq!(
            s.ops,
            vec![
                CanvasOp::Invalidate {
                    region: Region::Circle {
                        center: [5.0, 5.0],
                        radius: 8.0
                    }
                },
                CanvasOp::Invalidate { region: Region::Page },
            ]
        );
    }

    /// 계약: 'overlay' 능력은 선언한 구현에만 존재한다.
    #[test]
    fn overlay_capability_is_declared_not_inherited() {
        let mut core = CanvasRecorder::new();
        assert!(!core.has_capability(CAP_OVERLAY));
        let mut ov = CanvasRecorder::with_overlay();
        assert!(ov.has_capability(CAP_OVERLAY));
        ov.overlay(&OverlayShape::Circle {
            center: [0.0, 0.0],
            radius: 3.0,
            color: [0, 0, 0, 255],
        });
        assert_eq!(ov.op_kinds(), vec!["overlay"]);
    }
}
