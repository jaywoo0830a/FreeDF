//! Sizing scale — 모든 UI 치수를 여기 규약(helper/token)으로만 찍도록 강제합니다.
//!
//! # 치수 규약 (Size Rule)
//! 기반 단위 **1rem = 16px**. 치수는 반드시 아래 3계층 중 하나로 표현한다.
//!
//! | 계층 | 계단 | 규칙 | 용도 |
//! |------|------|------|------|
//! | 소형 `qrem(n)` | 4px (0.25rem) | n×4 | gap / padding / inset |
//! | **중형 `hrem(n)`** | **8px (0.5rem)** | **n×8 — 짝수만** | **컨트롤 높이·폭 등 "24, 32, 40…" 짝수 규격** |
//! | 대형 `rem(n)` | 16px (1rem) | n×16 | 창 / 패널 / 오버레이 크기 |
//!
//! **짝수 규약**: 중형은 8px 계단의 **짝수** 값만 허용 —
//! `S_24 · S_32 · S_40 · S_48 · S_64 …` (토큰). 26/30 같은 비정형 짝수는 금지.
//!
//! ```ignore
//! let w = crate::ui::scale::rem(44);    // 704px — 대형 (1rem 계단)
//! let h = crate::ui::scale::hrem(5);    //  40px — 중형 짝수 규격 (0.5rem 계단)
//! let g = crate::ui::scale::qrem(2);    //   8px — 소형 (0.25rem 계단)
//! ```

#![allow(dead_code)] // 상수/유틸은 필요 시 호출자가 재사용 (선재 위치).

/// 1rem = 16px (기반 단위).
pub const REM: f32 = 16.0;
/// 0.25rem = 4px (소형 최소 스텝).
pub const QR: f32 = REM / 4.0;
/// 0.5rem = 8px (중형 짝수 규격의 기본 계단).
pub const HALF: f32 = REM / 2.0;

/// 대형 치수 — **1rem(16px) 배수**로 강제. `rem(44)` = 704px.
pub const fn rem(n: i32) -> f32 {
    (n * 16) as f32
}

/// 소형 치수 — **0.25rem(4px) 배수**로 강제. `qrem(52)` = 208px.
pub const fn qrem(n: i32) -> f32 {
    (n * 4) as f32
}

/// 중형 치수 — **0.5rem(8px) 짝수 계단**으로 강제. `hrem(5)` = 40px.
/// "24·32·40·48…" 짝수 규격(비정형 짝수 금지)의 생성기.
pub const fn hrem(n: i32) -> f32 {
    (n * 8) as f32
}

/// 창/패널 같은 고정 크기를 **1rem 배수로 정규화**해 내림합니다.
/// (입력을 16px 단위로 내림해 항상 그리드에 안착 — 대형 위젯 전용.)
pub fn rem_floor(px: f32) -> f32 {
    ((px / REM) as i32) as f32 * REM
}

// ── 중형 짝수 규약 토큰 (정거장): 이 외의 중형 값은 넣지 않는다 ──
/// 24px = 1.5rem — 소형 컨트롤 높이 (S_24).
pub const S_24: f32 = 24.0;
/// 32px = 2rem — 표준 버튼/입력 높이 (S_32).
pub const S_32: f32 = 32.0;
/// 40px = 2.5rem — 넉넉한 터치/아이콘 (S_40).
pub const S_40: f32 = 40.0;
/// 48px = 3rem — 대형 터치 대상 (S_48).
pub const S_48: f32 = 48.0;
/// 56px = 3.5rem (S_56).
pub const S_56: f32 = 56.0;
/// 64px = 4rem — 섬네일/헤더 (S_64).
pub const S_64: f32 = 64.0;
/// 72px = 4.5rem (S_72).
pub const S_72: f32 = 72.0;
/// 80px = 5rem — 넓은 입력/미디어 썸네일 (S_80).
pub const S_80: f32 = 80.0;
/// 88px = 5.5rem (S_88).
pub const S_88: f32 = 88.0;
/// 96px = 6rem — 미디어/스와치 크기 (S_96).
pub const S_96: f32 = 96.0;
/// 104px = 6.5rem (S_104).
pub const S_104: f32 = 104.0;
/// 112px = 7rem — 큰 미리보기 (S_112).
pub const S_112: f32 = 112.0;
/// 120px = 7.5rem — 큰 패널/미리보기 (S_120).
pub const S_120: f32 = 120.0;
/// 128px = 8rem — 썸네일 그리드 (S_128).
pub const S_128: f32 = 128.0;
/// 134px — 상한 토큰 (요청: 최대 크기 정거장).
pub const S_134: f32 = 134.0;