//! 입력 소스(펜 스트림) 판정 헬퍼.
//!
//! 펜 스트림(OTD/evdev)의 **최신성**과 접촉 상태만 추적한다 — 장치 종류는
//! 이제 어댑터가 이벤트에 붙여 보내므로(`PointerSource`) 추정하지 않는다.
//! 여기 값은 "펜 스트림이 살아 있는가 / 펜이 닿아 있는가"를 묻는 쪽(엣지 자동
//! 스크롤, 커서 근접감, 세션 문맥의 접촉 증거)이 쓴다.
//!
//! 종전에는 egui 포인터/핀치 이벤트로 마우스·트랙패드 활동까지 추정해 보관했고
//! (`#[allow(dead_code)] // 구조 선제공`), 장치 판별 래치가 이 파일의 값을 다시
//! 조합했다 — 소비자 없는 선제 구조와 추정 래치는 제거됐다 (0916-3 청소).

use freedf_core::pen_input::PenState;

/// 펜 스트림 리포트를 "최근"으로 보는 시간 창 (ms).
const PEN_FRESH_MS: u64 = 2000;
/// 입력 소스 추정 상태 — 매 프레임 [`InputSources::update`]로 갱신한 뒤
/// `is_*_in_use`로 질의합니다.
#[derive(Debug, Default)]
pub(crate) struct InputSources {
    /// 펜 스트림(OTD/evdev) 마지막 리포트 시각(ms). `None` = 한 번도 안 옴.
    last_pen_report_ms: Option<u64>,
    /// 펜 팁이 화면에 닿아 있는지 (스트림의 접촉 상태).
    pen_contact: bool,
}

impl InputSources {
    /// 매 프레임 1회 호출 — egui 입력과 펜 스트림에서 소스별 활동을 기록합니다.
    ///
    /// `pen_latest`: 이번 프레임 폴에서 새로 받은 펜 상태 (없으면 None).
    /// `pen_report_ms`: 스트림의 마지막 리포트 시각 — 폴이 빈 프레임에서도
    /// 이전 시각을 유지하기 위해 앱 필드에서 그대로 전달합니다.
    pub(crate) fn update(
        &mut self,
        pen_latest: Option<&PenState>,
        pen_report_ms: Option<u64>,
        now: u64,
    ) {
        // ① 펜 — 스트림에서 직접 기록.
        if let Some(st) = pen_latest {
            self.last_pen_report_ms = Some(now);
            self.pen_contact = st.contact;
        } else if self.last_pen_report_ms.is_none() {
            self.last_pen_report_ms = pen_report_ms;
        }
    }

    /// 펜으로 커서를 움직이고 있는지 (**스트림 최신성** 기준 — 호버/접촉 무관).
    ///
    /// 스트림이 한 번도 안 온 환경에서는 `false`를 반환합니다 — 그런 환경은
    /// 펜/마우스 구분이 불가능하므로 [`Self::pen_undetectable`]로 따로
    /// 처리하세요 (예: 엣지 자동 스크롤은 구분 불가 시 허용).
    pub(crate) fn is_pen_in_use(&self, now: u64) -> bool {
        self.last_pen_report_ms
            .is_some_and(|t| now.saturating_sub(t) < PEN_FRESH_MS)
    }

    /// 펜 스트림이 한 번도 리포트한 적이 없는 환경인지 (판정 불가).
    pub(crate) fn pen_undetectable(&self) -> bool {
        self.last_pen_report_ms.is_none()
    }

    /// 펜 팁이 화면에 닿아 있는지 (스트림 기준).
    pub(crate) fn pen_contact(&self) -> bool {
        self.pen_contact
    }

}
