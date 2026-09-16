//! 필기 진단 — **단일 판정** (C4).
//!
//! 종전에는 진단이 여러 곳에 흩어져 있었다: 획 종료 판정(`stroke end` +
//! `PENUP-CHANGED`)은 `ink.rs`, 프레임 단위 평평 경고(`LIVE-FLAT`)는 `paint.rs`,
//! 세션 유실(`STROKE-DROP`)은 라우터 로그. 각자 다른 재료만 보므로 **"설정이
//! 꺼져 있어서 평평하다"와 "입력이 유실되어 평평하다"를 구분하지 못했다**
//! (`writing.log`의 오진: 필압 끔 상태를 "OTD 연결 확인"으로 진단).
//!
//! 이제 재료는 세 종류뿐이고, 판정은 **한 함수**가 낸다:
//! - 설정/장치 사실 ([`DeviceFacts`]) — 필압 민감도, 틸트 능력, 라이브 압력
//! - 측정 사실 ([`StrokeFacts`]) — 압력/폭/절반폭 통계, 잠금, 펜업 꼬리 변화
//! - 라우터 장부 ([`LedgerFacts`]) — 유실/강제 종료/승격은 **데이터**다 (계약 ③)
//!
//! 순수 함수 — 시간/egui/장치를 읽지 않는다 (테스트가 스펙과 1:1).

/// 한 획의 측정 사실 (판정 재료 — 통계만).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct StrokeFacts {
    /// 점 수 (1 = 탭).
    pub n_pt: usize,
    pub pressure_min: f32,
    pub pressure_max: f32,
    pub width_min: f32,
    pub width_max: f32,
    /// 렌더에 쓰이는 절반 폭 (실제 폭).
    pub half_min: f32,
    pub half_max: f32,
    /// 폭이 0(잠기지 않음)인 점 수.
    pub unlocked: usize,
    /// 펜을 떼는 순간(동결) 폭 데이터가 바뀌었는가 — 구 `PENUP-CHANGED` 관측.
    pub tail_changed: bool,
}

/// 설정/장치 사실 (판정 재료).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DeviceFacts {
    /// 사용자 설정: 필압 민감도.
    pub pressure_enabled: bool,
    /// 장치 능력: 틸트를 보고하는가 (C3 — 스트림 존재로 근사하지 않는다).
    pub tilt_supported: bool,
    pub tilt: [f32; 2],
    /// 마지막 펜 스트림 압력 (None = 스트림 없음).
    pub live_pressure: Option<f32>,
}

/// 라우터 장부 사실 (판정 재료) — 세션 라우터의 요약에서 온다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct LedgerFacts {
    pub delivered: usize,
    pub refused: usize,
    pub promoted: usize,
    pub expired: usize,
    pub stale: usize,
    pub cancelled: usize,
    pub replaced: usize,
    pub open: bool,
}

impl From<super::super::input::session_router::LedgerSummary> for LedgerFacts {
    fn from(s: super::super::input::session_router::LedgerSummary) -> Self {
        Self {
            delivered: s.delivered,
            refused: s.refused,
            promoted: s.promoted,
            expired: s.expired,
            stale: s.stale,
            cancelled: s.cancelled,
            replaced: s.replaced,
            open: s.open,
        }
    }
}

impl LedgerFacts {
    /// 유실/이상 정산이 있었는가.
    pub fn has_loss(&self) -> bool {
        self.expired > 0 || self.stale > 0 || self.replaced > 0
    }

    /// 사람이 읽는 요약 — 로그 한 줄에 들어가는 형태.
    pub fn label(&self) -> String {
        format!(
            "ledger[d={} r={} p={} x={} stale={} c={} rep={} open={}]",
            self.delivered,
            self.refused,
            self.promoted,
            self.expired,
            self.stale,
            self.cancelled,
            self.replaced,
            self.open
        )
    }
}

/// 판정 등급.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Level {
    /// 정상 (또는 설정상 정상).
    Ok,
    /// 정보 — 이상은 아니지만 봐야 하는 상태 (탭/짧은 획).
    Note,
    /// 경고 — 조사가 필요한 상태 (유실/모델 문제).
    Warn,
}

/// 단일 판정.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Verdict {
    pub level: Level,
    /// HUD 문구 (짧다).
    pub text: String,
    /// 로그 한 줄 — 형식은 구 로그와 호환 (grep 토큰 유지).
    pub line: String,
}

/// 획 종료 판정 — 설정/측정/장부를 **한 판정**으로 합친다.
///
/// 사다리 순서가 곧 인과 순서다: ① 설정이 꺼져 있으면 폭이 평평한 것이 정상이다
/// (입력을 의심하지 않는다) → ② 탭/짧은 획 → ③ 유실 동반 평평 → ④ 순수 입력
/// 문제 → ⑤ 잠금/모델 문제.
pub(crate) fn stroke_verdict(
    tool: &str,
    f: &StrokeFacts,
    dev: &DeviceFacts,
    ledger: &LedgerFacts,
) -> Verdict {
    let mut level = Level::Ok;
    let text: String = if !dev.pressure_enabled {
        // 설정이 꺼져 있으면 폭이 필압과 무관한 것이 **정상**이다
        // (writing.log 오진의 교훈 — 진단이 설정을 모르면 입력을 의심한다).
        level = Level::Note;
        "필압 꺼짐 (설정) — 폭은 필압과 무관".into()
    } else if f.n_pt == 1 {
        level = Level::Note;
        "탭 (1점 — 정상)".into()
    } else if f.n_pt < 8 {
        level = Level::Note;
        "점 부족 — 짧은 획".into()
    } else if f.pressure_max - f.pressure_min < 0.05 {
        level = Level::Warn;
        if ledger.has_loss() {
            "필압 일정 + 세션 유실(stale/expired) — Up 에지 유실 의심".into()
        } else {
            "필압 일정 → 입력 문제 (OTD 연결/필압 소스 확인)".into()
        }
    } else if f.unlocked > 0 {
        level = Level::Warn;
        "폭 잠금 안 됨 → locker 버그".into()
    } else if f.half_max - f.half_min < 0.02 {
        level = Level::Warn;
        "필압은 변하는데 렌더 폭 고정 → 모델/바닥값 버그".into()
    } else {
        "OK — 렌더 폭 변화 정상".into()
    };

    // 꼬리 변화(구 PENUP-CHANGED) — 판정에 접어 넣는다 (별도 로그/판정 아님).
    let prefix = if f.tail_changed {
        level = Level::Warn;
        "PENUP-CHANGED: "
    } else {
        ""
    };
    let line = format!(
        "{prefix}stroke end: tool={tool} n={} pressure=[{:.3}..{:.3}] width=[{:.3}..{:.3}] half=[{:.3}..{:.3}] unlocked={} live_pressure={:?} tilt=[{:+.0},{:+.0}] tilt_src={} {} → {}",
        f.n_pt,
        f.pressure_min,
        f.pressure_max,
        f.width_min,
        f.width_max,
        f.half_min,
        f.half_max,
        f.unlocked,
        dev.live_pressure,
        dev.tilt[0],
        dev.tilt[1],
        if dev.tilt_supported { "device" } else { "hand" },
        ledger.label(),
        text
    );
    Verdict { level, text, line }
}

/// 진행 중 획의 프레임 단위 평평 경고 (구 `LIVE-FLAT`) — 조건과 문구를 여기서
/// 단일 소유한다 (`paint.rs`는 스로틀과 로그만).
///
/// 반환: 경고 문구 | None (평평하지 않거나 표본이 부족하거나 설정이 꺼져 있다).
pub(crate) fn live_flat(
    n_pt: usize,
    pressure: (f32, f32),
    widths: (f32, f32),
    dev: &DeviceFacts,
) -> Option<String> {
    if n_pt < 8 {
        return None;
    }
    if widths.1 - widths.0 >= 0.05 {
        return None;
    }
    // 설정이 꺼져 있으면 평평한 폭은 정상 — 경고하지 않는다 (오진 방지).
    if !dev.pressure_enabled {
        return None;
    }
    Some(format!(
        "LIVE-FLAT: n={n_pt} widths=[{:.3}..{:.3}] pressure=[{:.3}..{:.3}] live_pressure={:?} (판정: {})",
        widths.0,
        widths.1,
        pressure.0,
        pressure.1,
        dev.live_pressure,
        if pressure.1 - pressure.0 < 0.05 {
            "필압 일정 — 입력/설정 확인"
        } else {
            "필압은 변하는데 렌더 폭 고정 — 모델 확인"
        }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> StrokeFacts {
        StrokeFacts {
            n_pt: 20,
            pressure_min: 0.1,
            pressure_max: 0.8,
            width_min: 0.5,
            width_max: 3.0,
            half_min: 0.25,
            half_max: 1.5,
            unlocked: 0,
            tail_changed: false,
        }
    }

    fn device() -> DeviceFacts {
        DeviceFacts {
            pressure_enabled: true,
            tilt_supported: true,
            tilt: [3.0, -4.0],
            live_pressure: Some(0.0),
        }
    }

    /// 설정이 꺼져 있으면 평평한 폭은 **정상** — 입력을 의심하지 않는다.
    /// (writing.log 오진의 회귀 방지.)
    #[test]
    fn pressure_disabled_is_not_an_input_problem() {
        let mut dev = device();
        dev.pressure_enabled = false;
        let f = StrokeFacts {
            pressure_min: 1.0,
            pressure_max: 1.0,
            ..facts()
        };
        let v = stroke_verdict("Pen", &f, &dev, &LedgerFacts::default());
        assert_eq!(v.level, Level::Note);
        assert!(v.text.contains("설정"), "설정 사실이 판정에 들어간다: {}", v.text);
        assert!(!v.text.contains("OTD"), "오진 금지");
        // 라이브 경고도 설정을 존중한다.
        assert!(live_flat(20, (1.0, 1.0), (1.0, 1.0), &dev).is_none());
    }

    /// 유실(stale)이 동반된 평평함은 입력 문제가 아니라 **유실**로 판정된다.
    #[test]
    fn flat_with_ledger_loss_points_at_the_router() {
        let f = StrokeFacts {
            pressure_min: 0.5,
            pressure_max: 0.5,
            ..facts()
        };
        let ledger = LedgerFacts {
            delivered: 2,
            stale: 1,
            ..LedgerFacts::default()
        };
        let v = stroke_verdict("Pen", &f, &device(), &ledger);
        assert_eq!(v.level, Level::Warn);
        assert!(v.text.contains("stale"), "장부 사실이 판정에 들어간다: {}", v.text);
        assert!(v.line.contains("stale=1"), "로그 한 줄에 장부 요약: {}", v.line);
    }

    /// 꼬리 변화는 별도 로그가 아니라 **같은 한 줄**에 접혀 들어간다 (grep 토큰 유지).
    #[test]
    fn tail_change_folds_into_the_single_line() {
        let f = StrokeFacts {
            tail_changed: true,
            ..facts()
        };
        let v = stroke_verdict("Pen", &f, &device(), &LedgerFacts::default());
        assert_eq!(v.level, Level::Warn);
        assert!(v.line.starts_with("PENUP-CHANGED: "), "grep 토큰 유지: {}", v.line);
        assert_eq!(v.line.matches("PENUP-CHANGED").count(), 1, "한 줄에 한 번");
    }

    /// 정상 획은 정상이라고 말한다 (판정이 늘 경고를 내지 않는다).
    #[test]
    fn healthy_stroke_is_reported_ok() {
        let v = stroke_verdict("Pen", &facts(), &device(), &LedgerFacts::default());
        assert_eq!(v.level, Level::Ok);
        assert!(v.text.contains("OK"));
        assert!(v.line.contains("tilt_src=device"), "능력 사실도 함께 남는다");
    }

    /// 장부 요약 → 판정 재료 변환은 라우터 사실을 보존한다.
    #[test]
    fn ledger_summary_maps_into_facts() {
        use super::super::super::input::session_router::LedgerSummary;
        let s = LedgerSummary {
            delivered: 3,
            refused: 1,
            promoted: 1,
            expired: 1,
            stale: 0,
            cancelled: 0,
            replaced: 0,
            open: true,
        };
        let f = LedgerFacts::from(s);
        assert_eq!(f.delivered, 3);
        assert!(f.open);
        assert!(f.has_loss(), "만료는 유실이다");
        assert!(f.label().contains("x=1"));
    }
}
