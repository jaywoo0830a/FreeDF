//! 시간 공급 — 순수성 원칙의 첫 번째 부품.
//!
//! 잉크 번짐/애니메이션은 시간에 의존하지만, **직접 벽시계를 읽지 않습니다.**
//! 모든 계산은 `now_ms: u64`를 인자로 받고, 벽시계는 [`Clock`] 구현체가
//! 한 번만 읽습니다. 테스트는 [`FakeClock`]으로 결정적 시간을 주입합니다.

use std::sync::atomic::{AtomicU64, Ordering};

/// 단조 증가 시각 공급자 (epoch ms).
pub trait Clock: Send + Sync {
    fn now_ms(&self) -> u64;
}

/// 실제 벽시계 — 앱 실행 경로 전용.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

/// 테스트용 수동 시계 — 테스트가 시간을 완전히 통제합니다.
#[derive(Debug, Default)]
pub struct FakeClock {
    now_ms: AtomicU64,
}

impl FakeClock {
    pub fn new(now_ms: u64) -> Self {
        Self {
            now_ms: AtomicU64::new(now_ms),
        }
    }

    pub fn set(&self, now_ms: u64) {
        self.now_ms.store(now_ms, Ordering::SeqCst);
    }

    pub fn advance(&self, delta_ms: u64) {
        self.now_ms.fetch_add(delta_ms, Ordering::SeqCst);
    }
}

impl Clock for FakeClock {
    fn now_ms(&self) -> u64 {
        self.now_ms.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 계약: FakeClock은 테스트가 시간을 완전히 통제하게 합니다.
    #[test]
    fn fake_clock_is_fully_controllable() {
        let clock = FakeClock::new(1_000);
        assert_eq!(clock.now_ms(), 1_000);
        clock.advance(250);
        assert_eq!(clock.now_ms(), 1_250);
        clock.set(42);
        assert_eq!(clock.now_ms(), 42);
    }
}
