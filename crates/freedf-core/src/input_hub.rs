//! 이벤트 허브 (계약 객체 ④ — ideation `idea4/hub.js`의 이식).
//!
//! "정책이 사는 곳": 포인터 소스 충돌 규칙(**한 번에 한 포인터**)을 소유해서,
//! 툴 상태기계가 두 장치의 뒤섞인 입력이라는 혼란을 아예 받지 않게 한다.
//!
//! JS 프로토타입의 리스너(push) 모델 대신 **큐(pull) 모델**을 쓴다 — 이 앱은
//! egui의 즉시 모드(immediate mode)라 프레임마다 이벤트를 순서대로 소비하는
//! 것이 소유권 규칙(borrow checker)과 잘 맞는다. 순서 보장과 재생 계약은
//! 동일하게 유지된다.
//!
//! drop 계약: [`Hub::emit`]의 반환값(`false` = 규칙에 의해 drop)은 장치
//! 어댑터의 emit 체인에서 관찰되지 않는다 — drop 여부는 다운스트림
//! 커맨드(그리기가 시작됐는가)로 관찰하는 것이 계약이다. (허브가 [`Hub::dropped`]
//! 카운터로 drop 누적을 보관한다 — 진단 계약 4.3.)

use std::collections::VecDeque;

use crate::input_events::{
    InputEvent, PointerPhase, PointerSource, POINTER_SOURCES,
};

/// 통합 입력 허브 — 충돌 규칙 적용 후 순서대로 적재하고, 소비자가 당겨 간다.
#[derive(Debug, Default)]
pub struct Hub {
    queue: VecDeque<InputEvent>,
    /// 현재 포인터를 점유 중인 소스 — Down에 진입, Up에 탈출.
    active_source: Option<PointerSource>,
    /// 충돌 규칙에 의해 drop된 이벤트 누적 (진단 계약 4.3 — 유실 관측).
    dropped_count: usize,
}

impl Hub {
    pub fn new() -> Self {
        Self::default()
    }

    /// 이벤트를 허브에 발행한다.
    ///
    /// 반환값: `true` = 수용(큐에 적재), `false` = 충돌 규칙에 의해 drop.
    /// 어댑터는 이 반환값을 무시한다 (drop 계약 — 모듈 문서 참조).
    pub fn emit(&mut self, event: InputEvent) -> bool {
        if let InputEvent::Pointer(p) = &event {
            if POINTER_SOURCES.contains(&p.source) {
                // 규칙: 한 번에 한 포인터 — 다른 소스의 이벤트는 점유 중엔 drop.
                // (Up도 예외가 아니다 — 엉뚱한 소스의 Up이 점유를 풀지 못하게.)
                if let Some(active) = self.active_source {
                    if active != p.source {
                        self.dropped_count += 1;
                        return false;
                    }
                }
                match p.phase {
                    PointerPhase::Down => self.active_source = Some(p.source),
                    PointerPhase::Up => self.active_source = None,
                    PointerPhase::Drag => {}
                }
            }
        }
        self.queue.push_back(event);
        true
    }

    /// 이번 프레임의 이벤트를 **적재 순서대로** 소비한다.
    pub fn take(&mut self, mut consumer: impl FnMut(InputEvent)) {
        for event in self.queue.drain(..) {
            consumer(event);
        }
    }

    /// 아직 소비되지 않은 이벤트 수 (진단/테스트용).
    pub fn pending(&self) -> usize {
        self.queue.len()
    }

    /// 현재 포인터를 점유 중인 소스 (Down에서 진입, Up에서 탈출).
    ///
    /// 캔버스 정책(팬/드로우, 커서 미리보기)이 "이 프레스의 소유자는 누구인가"를
    /// **추정 대신 묻는** 창구다. 점유 규칙의 소유자가 여기(허브)이므로, 소스
    /// 판정을 egui 이벤트(터치 유무 등)로 다시 추정하는 래치는 땜질이다.
    pub fn active_source(&self) -> Option<PointerSource> {
        self.active_source
    }

    /// 충돌 규칙("한 번에 한 포인터")에 의해 drop된 이벤트 누적 (진단 계약 4.3).
    /// 유실이 장치 축에서 일어났는지를 데이터로 관측하는 창구다.
    pub fn dropped(&self) -> usize {
        self.dropped_count
    }
}

/// 재생(player) 계약 — 하드웨어 없이 통합 이벤트 스크립트만으로 전체 흐름을
/// 구동한다. 장치↔툴 분리가 진짜라는 증거이자, 녹화 파일의 재생 경로다.
pub fn replay(hub: &mut Hub, events: impl IntoIterator<Item = InputEvent>) {
    for event in events {
        hub.emit(event);
    }
}
