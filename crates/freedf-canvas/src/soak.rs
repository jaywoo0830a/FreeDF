//! 잉크 스밈(진해짐) — **젊은 획 추적 계약** (순수 상태 머신).
//!
//! 병합 잉크 메시 파이프라인의 북킹 규칙을 순수 데이터로 분리한 모듈입니다.
//! 시간은 항상 `now_ms` 인자로 주입되고, 정착 판정은 호출자가 함수로
//! 넘기므로 egui/스레드/시계 없이 계약 테스트가 가능합니다.
//!
//! ## 불변식
//!
//! - 병합 메시에는 **정착된 획만** 들어 있습니다 — `settled`가 그 수.
//! - 스밈이 진행 중인 획은 `young` 목록에 있고, 오버레이가 매 프레임
//!   현재 나이로 재굽습니다.
//! - 스토어의 획은 **삽입 순서** `Vec`이라, 신규 획은 항상
//!   `[settled + young.len() ..]` 꼬리에만 나타납니다 (undo 재삽입도 tail).
//! - `rev`는 마지막으로 반영한 스토어 rev — 달라졌고 개수가 줄었으면
//!   삭제로 보고 전체 재구성을 요청합니다.
//!
//! ## 결정 규칙 (전부 순수 함수)
//!
//! ```text
//! new_from(store)   — 꼬리에 새 획이 왔는지 (시작 인덱스)
//! add(page, ...)    — 새 획을 젊은 목록에 추가 (페이지가 다르면 초기화)
//! sweep(settle,now) — 정착된 젊은 획을 꺼냄 (병합 메시로 이동할 목록)
//! deleted(store)    — 삭제/축소 발생 (전체 재구성 필요)
//! resync(store,..)  — 전체 재굽기 설치 후 집합 재계산
//! ```

use crate::scene::{Revision, Stroke};

/// 지원하는 모니터 주사율 프리셋 (Hz) — 낮음 → 높음 순.
pub const REFRESH_PRESETS: [u32; 4] = [60, 120, 144, 240];

/// 주사율 프리셋 → 잉크 파이프라인 페이싱 파라미터 (계약 테스트로 고정).
///
/// 주사율이 높을수록 **더 많은 연산**으로 더 부드러운 필기감을 만듭니다:
/// - `active_geom_ms` — 진행 중 획 재구성 스로틀 (주사율과 같은 주기).
/// - `soak_scale` — 스밈 정착 시간 배율. 커지면 같은 진해짐이 더 많은
///   프레임에 걸쳐 그라데이션되므로 고주사율에서 더 촘촘하게 보입니다.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InkPacing {
    /// 진행 중 획 지오메트리 재구성 스로틀 (ms).
    pub active_geom_ms: u64,
    /// 스밈(진해짐) 정착 시간 배율 (60Hz = 1.0 기준).
    pub soak_scale: f32,
}

/// 주사율 프리셋의 페이싱 파라미터 (60Hz가 기존 동작과 같은 기준선).
pub fn ink_pacing_for(hz: u32) -> InkPacing {
    match hz {
        120 => InkPacing {
            active_geom_ms: 8,
            soak_scale: 1.15,
        },
        144 => InkPacing {
            active_geom_ms: 7,
            soak_scale: 1.25,
        },
        h if h >= 240 => InkPacing {
            active_geom_ms: 4,
            soak_scale: 1.5,
        },
        _ => InkPacing {
            active_geom_ms: 16,
            soak_scale: 1.0,
        },
    }
}

/// 가장 가까운 지원 주사율 프리셋으로 스냅 (저장된 값 보정용).
pub fn snap_refresh_hz(hz: u32) -> u32 {
    REFRESH_PRESETS
        .iter()
        .copied()
        .min_by_key(|p| p.abs_diff(hz))
        .unwrap_or(60)
}

/// 스밈 정착 추적기 — 앱(`freedf`)의 병합 메시와 스토어 사이의 조정자.
#[derive(Debug, Clone, Default)]
pub struct InkSettling {
    /// 스밈이 아직 진행 중인 획 (삽입 순서).
    pub young: Vec<Stroke>,
    /// 병합 메시에 들어간(정착된) 획 수.
    pub settled: usize,
    /// 마지막으로 반영한 스토어 rev.
    pub rev: Revision,
    /// 젊은 목록이 속한 페이지 — 다른 페이지면 무효.
    pub page: Option<usize>,
}

impl InkSettling {
    /// 빈 추적기 — 아직 아무 rev도 반영하지 않은 상태.
    pub fn new() -> Self {
        Self {
            rev: Revision(u64::MAX),
            ..Self::default()
        }
    }

    /// 전체 초기화 (문서 교체/캐시 비움 등 — 병합 메시도 함께 버려야 함).
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// 스토어에 새로 추가된 획의 시작 인덱스 (`strokes[from..]`이 신규 획).
    /// rev가 같으면 `None` (이미 반영).
    pub fn new_from(&self, page: usize, store_count: usize, store_rev: u64) -> Option<usize> {
        if store_rev == self.rev.0 || self.page != Some(page) {
            return None;
        }
        let expected = self.settled + self.young.len();
        (store_count > expected).then_some(expected)
    }

    /// 방금 끝난 획들을 젊은 목록에 추가합니다. 페이지가 바뀌었으면
    /// 이전 페이지의 젊은 획을 버리고 새로 시작합니다.
    pub fn add(&mut self, page: usize, strokes: Vec<Stroke>, store_rev: u64) {
        if self.page != Some(page) {
            self.young.clear();
            self.settled = 0;
            self.page = Some(page);
        }
        self.young.extend(strokes);
        self.rev = Revision(store_rev);
    }

    /// 정착된 젊은 획을 꺼냅니다 — 반환된 획을 병합 메시에 append하고
    /// `settled`는 내부에서 증가합니다. (`settle`은 `u64::MAX` = 정착 완료)
    pub fn sweep(&mut self, settle: impl Fn(&Stroke) -> u64, _now: u64) -> Vec<Stroke> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < self.young.len() {
            if settle(&self.young[i]) == u64::MAX {
                out.push(self.young.remove(i));
            } else {
                i += 1;
            }
        }
        self.settled += out.len();
        out
    }

    /// 삭제/축소 발생 — rev가 바뀌었는데 스토어 개수가 기대치 이하이면
    /// 전체 재구성이 필요합니다 (병합 메시에 없는 지워진 획이 남음).
    pub fn deleted(&self, page: usize, store_count: usize, store_rev: u64) -> bool {
        self.page == Some(page)
            && store_rev != self.rev.0
            && store_count <= self.settled + self.young.len()
    }

    /// 전체 재굽기 설치 후 — 정착/젊은 집합을 스토어 기준으로 재계산합니다.
    /// (`young`은 호출자가 스토어에서 `next_settle != MAX`인 획으로 채움)
    pub fn resync(&mut self, page: usize, store_count: usize, store_rev: u64, young: Vec<Stroke>) {
        self.page = Some(page);
        self.rev = Revision(store_rev);
        self.settled = store_count.saturating_sub(young.len());
        self.young = young;
    }
}
