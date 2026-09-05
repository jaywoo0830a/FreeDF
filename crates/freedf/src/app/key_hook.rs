//! 매크로 출력 파이프라인 — egui 키 이벤트(입력) + enigo 주입(출력).
//!
//! 배경: 전역 LL 훅(직접 구현·rdev 모두)은 **화상 키보드(TabTip 등)의
//! 문자키를 받지 못합니다** — 그 입력은 훅 체인을 우회해 창에 직접 전달됩니다
//! (실측: 화살표만 훅에 도달했고, 문자키는 egui 이벤트로만 도달 — a/s 탭
//! 키가 동작한 것이 증거). 따라서 입력은 **egui 이벤트**를 쓰고, 출력만
//! enigo로 보냅니다 (화상·물리 키보드 모두 동작).
//!
//! - 페이지 z/x → 앱 내부 next/prev_page 직접 호출 (주입 불필요)
//! - 데스크탑 q/w → enigo로 Win↓→Ctrl↓→←/→→Ctrl↑→Win↑ 주입
//! - 탭 a/s → 앱 내부 next_tab/prev_tab (기존과 동일)
//!
//! 게이트는 `handle_shortcuts`가 담당합니다 (타이핑 중/수정키 조합 무시).
//! 이 모듈에는 디버그 로그(패널 표시)와 enigo 전송만 남습니다.

use std::collections::VecDeque;
use std::time::Instant;

/// 매크로 진단 로그 (최근 200줄, [경과초] 접두) — Macro 설정 창의
/// 디버그 패널이 표시합니다.
static HOOK_LOG: std::sync::Mutex<VecDeque<String>> = std::sync::Mutex::new(VecDeque::new());
const HOOK_LOG_MAX: usize = 200;

/// 데스크탑 매크로를 쓸 수 있는지 (enigo는 Windows 전용).
pub(crate) fn pipeline_enabled() -> bool {
    cfg!(target_os = "windows")
}

pub(crate) fn hook_log(msg: impl std::fmt::Display) {
    static T0: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    let t = T0.get_or_init(Instant::now).elapsed().as_secs_f64();
    if let Ok(mut q) = HOOK_LOG.lock() {
        while q.len() >= HOOK_LOG_MAX {
            q.pop_front();
        }
        q.push_back(format!("[{t:>8.3}s] {msg}"));
    }
}

pub(crate) fn hook_log_snapshot() -> Vec<String> {
    HOOK_LOG
        .lock()
        .map(|q| q.iter().cloned().collect())
        .unwrap_or_default()
}

pub(crate) fn hook_log_clear() {
    if let Ok(mut q) = HOOK_LOG.lock() {
        q.clear();
    }
}

/// Win↓ → Ctrl↓ → ←/→ 탭 → Ctrl↑ → Win↑ — 검증된 순서로 enigo 주입.
/// egui 단축키 처리(UI 스레드)에서 직접 호출합니다 — 사용자가 검증한
/// 예제와 같은 컨텍스트(메인 스레드)입니다.
#[cfg(target_os = "windows")]
pub(crate) fn send_desktop(prev: bool) {
    use enigo::{Direction, Key, Keyboard};
    let mut enigo = match enigo::Enigo::new(&enigo::Settings::default()) {
        Ok(e) => e,
        Err(e) => {
            hook_log(format!("enigo init FAILED: {e:?}"));
            return;
        }
    };
    let arrow = if prev { Key::LeftArrow } else { Key::RightArrow };
    let steps: [(&str, Key, Direction); 5] = [
        ("win↓", Key::Meta, Direction::Press),
        ("ctrl↓", Key::Control, Direction::Press),
        ("arrow", arrow, Direction::Click),
        ("ctrl↑", Key::Control, Direction::Release),
        ("win↑", Key::Meta, Direction::Release),
    ];
    let mut out = String::new();
    for (label, k, dir) in steps {
        match enigo.key(k, dir) {
            Ok(()) => out.push_str(label),
            Err(e) => out.push_str(&format!("{label}!({e:?})")),
        }
        out.push(' ');
    }
    hook_log(format!("desktop prev={prev} [{out}]"));
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn send_desktop(prev: bool) {
    hook_log(format!("desktop prev={prev} — disabled (Windows only)"));
}
