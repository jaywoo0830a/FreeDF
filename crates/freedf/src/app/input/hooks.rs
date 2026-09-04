//! 입력 소스(펜/마우스/트랙패드) 판정 헬퍼.
//!
//! egui 0.36은 포인터 이벤트에 기기 종류를 노출하지 않으므로, 이 헬퍼는
//! ① 펜 스트림(OTD/evdev)의 최신성, ② egui 포인터 버튼/핀치 이벤트,
//! ③ 소스별 마지막 활동 시각을 조합해 **추정**합니다.
//!
//! 판정 규칙은 이 파일 한 곳에 모여 있습니다 — 나중에 진짜 기기 식별이
//! 가능해지면(Windows WM_POINTER 통합, winit DeviceId 노출 등) 여기만
//! 고치면 됩니다.

use freedf_core::pen_input::PenState;

/// 펜 스트림 리포트를 "최근"으로 보는 시간 창 (ms).
const PEN_FRESH_MS: u64 = 2000;
/// 마우스/트랙패드 활동 판정 창 (ms).
#[allow(dead_code)] // 구조 선제공 — 실제 판정 로직 연결 예정.
const POINTER_FRESH_MS: u64 = 1000;

/// 입력 소스 추정 상태 — 매 프레임 [`InputSources::update`]로 갱신한 뒤
/// `is_*_in_use`로 질의합니다.
#[derive(Debug, Default)]
pub(crate) struct InputSources {
    /// 펜 스트림(OTD/evdev) 마지막 리포트 시각(ms). `None` = 한 번도 안 옴.
    last_pen_report_ms: Option<u64>,
    /// 펜 팁이 화면에 닿아 있는지 (스트림의 접촉 상태).
    pen_contact: bool,
    /// 마우스 활동(포인터 버튼) 마지막 시각(ms).
    last_mouse_activity_ms: Option<u64>,
    /// 트랙패드 활동(핀치 등 제스처) 마지막 시각(ms).
    last_trackpad_activity_ms: Option<u64>,
}

impl InputSources {
    /// 매 프레임 1회 호출 — egui 입력과 펜 스트림에서 소스별 활동을 기록합니다.
    ///
    /// `pen_latest`: 이번 프레임 폴에서 새로 받은 펜 상태 (없으면 None).
    /// `pen_report_ms`: 스트림의 마지막 리포트 시각 — 폴이 빈 프레임에서도
    /// 이전 시각을 유지하기 위해 앱 필드에서 그대로 전달합니다.
    pub(crate) fn update(
        &mut self,
        ctx: &egui::Context,
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

        // ② 마우스 — 펜 스트림이 살아 있지 않은 동안의 포인터 버튼 활동.
        let pen_fresh = self
            .last_pen_report_ms
            .is_some_and(|t| now.saturating_sub(t) < PEN_FRESH_MS);
        let pointer_busy = ctx.input(|i| {
            i.pointer.any_pressed() || i.pointer.any_released() || i.pointer.any_down()
        });
        if pointer_busy && !pen_fresh {
            self.last_mouse_activity_ms = Some(now);
        }

        // ③ 트랙패드 — 핀치(줌) 제스처는 마우스에 없는 신호라 강하게 씁니다.
        //    휠 스크롤은 마우스 휠과 구분이 안 돼 아직 기록하지 않습니다.
        let gesture = ctx.input(|i| {
            i.events
                .iter()
                .any(|e| matches!(e, egui::Event::Zoom(_)))
        });
        if gesture {
            self.last_trackpad_activity_ms = Some(now);
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

    /// 마우스가 최근 활동 중인지 (버튼 조작 기준).
    #[allow(dead_code)] // 구조 선제공 — 실제 판정 로직 연결 예정.
    pub(crate) fn is_mouse_in_use(&self, now: u64) -> bool {
        self.last_mouse_activity_ms
            .is_some_and(|t| now.saturating_sub(t) < POINTER_FRESH_MS)
    }

    /// 트랙패드가 최근 활동 중인지 (핀치/제스처 기준).
    #[allow(dead_code)] // 구조 선제공 — 실제 판정 로직 연결 예정.
    pub(crate) fn is_trackpad_in_use(&self, now: u64) -> bool {
        self.last_trackpad_activity_ms
            .is_some_and(|t| now.saturating_sub(t) < POINTER_FRESH_MS)
    }
}
