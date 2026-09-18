//! 펜 입력(필압·틸트) — **코어 공급원 배선**만 담당한다.
//!
//! 장치 지식은 전부 `freedf-core`가 소유한다:
//!
//! - `pen_input::spawn_otd_monitor()` — OpenTabletDriver 데몬 RPC 스트림
//!   (헤더 프레이밍 JSON-RPC `.NET` 네임드 파이프/유닉스 소켓 — 전 플랫폼).
//!   데몬이 태블릿을 독점하므로 **장치를 직접 열지 않는다**.
//! - `pen_input::open_best()` — OTD가 없을 때의 evdev 폴백 (리눅스).
//! - `input_devices::PenEventAdapter` — 틸트 노이즈 필터(EMA)와 능력 협상
//!   (압력/틸트를 보고하지 않는 장치는 이 경계가 기본값을 채운다).
//! - `input_events::{tilt_magnitude, tilt_azimuth}` — 틸트 벡터 → 크기/방위각.
//!
//! 여기서 하는 일은 **셋뿐**이다: (1) 두 공급원 중 쓸 수 있는 것을 고르고,
//! (2) 프레임마다 최신 스냅샷을 폴링해 어댑터에 통과시키고, (3) 소비자
//! (캔버스 잉크/커서)가 묻는 파생값을 내준다. 압력 스케일링·필터링을 다시
//! 구현하지 않는다 — 그건 코어가 이미 보증한다.

use freedf_core::input_devices::PenEventAdapter;
use freedf_core::input_events::{tilt_azimuth, tilt_magnitude};
use freedf_core::pen_input::{self, PenButtons, PenCapabilities, PenMonitor, PenState};
use std::sync::mpsc::Receiver;

/// 펜 스트림의 출처 — 진단/상태 표시용 (동작 차이는 없다: 스트림은 하나다).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PenSource {
    /// OpenTabletDriver 데몬 RPC (필압 + 틸트).
    Otd,
    /// evdev 직접 열기 (리눅스 폴백).
    Evdev,
    /// 스트림 없음 — 압력은 명목 1.0, 틸트는 0.
    None,
}

impl PenSource {
    /// 사람이 읽는 라벨 (상태바/진단).
    pub fn label(self) -> &'static str {
        match self {
            PenSource::Otd => "OTD",
            PenSource::Evdev => "evdev",
            PenSource::None => "none",
        }
    }
}

/// 프레임마다 최신 펜 상태를 들고 있는 어댑터 — 코어 공급원의 유일한 소비 창구.
pub struct PenInput {
    /// 활성 펜 스트림 (없으면 "펜 없음" 모드).
    monitor: Option<PenMonitor>,
    source: PenSource,
    /// 틸트 조건화(장치 노이즈 필터) + 능력 협상의 소유자 — `freedf-core`.
    adapter: PenEventAdapter,
    /// 마지막으로 받은 장치 스냅샷 (없으면 `PenState::default()`).
    state: PenState,
    /// 마지막 리포트 도착 시각 (Unix ms) — 근접감(그림자 깊이) 계산용.
    last_ms: Option<u64>,
}

impl Default for PenInput {
    fn default() -> Self {
        Self::attach()
    }
}

impl PenInput {
    /// 사용 가능한 공급원을 **고른다**: OTD 데몬 → evdev → 없음.
    ///
    /// OTD가 떠 있으면 데몬 스트림이 이긴다(태블릿을 독점하므로 evdev로는
    /// 필압/틸트가 안 온다). 준비 판정은 `pen_input::otd_connectable()` —
    /// 데몬이 없을 때 `spawn_otd_monitor()`는 영원히 조용한 스트림만 준다.
    ///
    /// 한계: 선택은 **시작 시 1회**다. 앱이 뜬 뒤에 OTD가 켜지면 자동으로
    /// 갈아타지 않는다(다음 실행에서 붙는다) — 재선택은 이 함수를 다시 부르면 된다.
    pub fn attach() -> Self {
        if pen_input::otd_connectable() {
            if let Some(rx) = pen_input::spawn_otd_monitor() {
                // OTD 리포트는 TiltTabletReport — 능력은 낙관 기본값(보고하면 쓴다).
                return Self::from_receiver(rx, PenCapabilities::UNKNOWN, PenSource::Otd);
            }
        }
        match pen_input::open_best() {
            // 능력은 장치 경계(evdev 열거)가 아는 사실 — 모니터가 함께 나른다.
            Some(m) => {
                let caps = m.capabilities();
                Self::from_monitor(m, caps, PenSource::Evdev)
            }
            None => Self::silent(),
        }
    }

    /// 이미 열린 모니터를 감싼다 (evdev 경로 — 능력은 모니터가 나른다).
    pub fn from_monitor(monitor: PenMonitor, caps: PenCapabilities, source: PenSource) -> Self {
        Self {
            adapter: PenEventAdapter::with_capabilities(caps),
            monitor: Some(monitor),
            source,
            state: PenState::default(),
            last_ms: None,
        }
    }

    /// 수신부를 직접 주입한다 — 테스트/사용자 공급 시임(`pen_input::channel`).
    pub fn from_receiver(rx: Receiver<PenState>, caps: PenCapabilities, source: PenSource) -> Self {
        Self::from_monitor(
            pen_input::from_receiver(rx).with_capabilities(caps),
            caps,
            source,
        )
    }

    /// 펜 스트림 없음 — 압력/틸트는 명목값 (마우스/트랙패드 환경이 정상이다).
    pub fn silent() -> Self {
        Self::from_monitor(
            pen_input::from_receiver(std::sync::mpsc::channel().1),
            PenCapabilities::NONE,
            PenSource::None,
        )
    }

    /// 프레임당 1회 폴링 — 새 리포트가 있었는지 반환한다.
    ///
    /// 위치는 넘기지 않는다(`None`): 위치는 egui 포인터가 나르고, 여기서는
    /// 장치 상태(압력/틸트/접촉)만 조건화한다 (어댑터 계약 그대로).
    pub fn poll(&mut self) -> bool {
        let Some(monitor) = self.monitor.as_mut() else {
            return false;
        };
        let Some(state) = monitor.poll() else {
            return false;
        };
        let _ = self.adapter.update(&state, None);
        self.state = state;
        self.last_ms = Some(crate::canvas::now_ms());
        true
    }

    /// 스트림 출처 (OTD/evdev/없음).
    pub fn source(&self) -> PenSource {
        self.source
    }

    /// 최신 장치 스냅샷 (압력/접촉은 장치가 보고한 raw 값).
    pub fn state(&self) -> PenState {
        self.state
    }

    /// 이 장치가 틸트를 보고하는가 — 커서 각도 분기의 **능력 질의**.
    pub fn tilt_supported(&self) -> bool {
        self.adapter.tilt_supported()
    }

    /// 조건화된 틸트 벡터 (도, ±90) — 코어 어댑터가 소유한 값.
    pub fn tilt(&self) -> [f32; 2] {
        self.adapter.tilt()
    }

    /// 틸트 크기 (도) — 도구 재료(잉크 폭 변화)에 넘기는 값.
    pub fn tilt_magnitude(&self) -> f32 {
        tilt_magnitude(self.tilt())
    }

    /// 틸트 벡터 → 도구 재료용 0..1 크기 (freedf `model_tilt`와 같은 변환).
    pub fn tilt_unit(&self) -> f32 {
        (self.tilt_magnitude() / 90.0).clamp(0.0, 1.0)
    }

    /// (방위각 rad, 기울기 코사인) — 커서 배럴 각도. 틸트 미지원 장치는 `None`.
    pub fn cursor_azimuth(&self) -> Option<(f32, f32)> {
        if !self.tilt_supported() {
            return None;
        }
        Some(tilt_azimuth(self.tilt()))
    }

    /// 펜이 패드에 닿아 있는가.
    pub fn contact(&self) -> bool {
        self.state.contact
    }

    /// 사이드 버튼 상태 (장치가 보고할 때만 의미 있음).
    pub fn buttons(&self) -> PenButtons {
        self.state.buttons
    }

    /// 마지막 리포트 이후 경과 시간(ms) — 리포트가 없으면 `None`.
    pub fn age_ms(&self, now_ms: u64) -> Option<u64> {
        self.last_ms.map(|t| now_ms.saturating_sub(t))
    }

    /// 그리기에 쓸 압력 — 설정이 꺼져 있거나 장치가 보고하지 않으면 명목 1.0.
    ///
    /// 범위 클램프(0..1)는 여기서 한다: 설정/장치가 무엇을 주든
    /// `InkPipeline` 계약(0..1)을 지킨다.
    pub fn pressure(&self, enabled: bool) -> f32 {
        if !enabled {
            return 1.0;
        }
        self.state.pressure.unwrap_or(1.0).clamp(0.0, 1.0)
    }

    /// 커서 "근접감" 0..1 — 입체 그림자의 깊이/거리 (freedf 커서 이식).
    ///
    /// 접촉이면 1.0. 호버는 리포트가 살아 있는 동안(마지막 리포트 후 400ms까지)
    /// 최대값을 유지하다가, 그 뒤 900ms에 걸쳐 0으로 사라진다 — 펜을 떼면
    /// 그림자가 서서히 멀어져 "펜이 종이에서 떨어졌다"가 시각적으로 읽힌다.
    pub fn proximity(&self, now_ms: u64) -> f32 {
        if self.contact() {
            return 1.0;
        }
        let age = self.age_ms(now_ms).unwrap_or(u64::MAX);
        let base = if age < 400 { 0.65 } else { 0.0 };
        let fade = 1.0 - ((age.saturating_sub(400)) as f32 / 900.0).clamp(0.0, 1.0);
        (base + 0.65 * fade).clamp(0.0, 1.0)
    }
}
