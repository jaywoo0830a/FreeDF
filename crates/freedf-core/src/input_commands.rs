//! 문서 커맨드 어휘 (계약 객체 ⓪ — ideation `idea4/commands.js`의 이식).
//!
//! 툴이 생산하고 문서/투영(projection)이 소비하는 출력 어휘. 툴 축
//! ([`crate::input_tools`])과 캔버스 경계(앱의 커맨드 실행기)가 공유하는
//! 것은 이 enum뿐이다 — 어느 쪽도 서로를 몰라야 한다.
//!
//! 세션 불변식: begin → extend* → end, erase-at* → end-erase. 검사기는
//! [`check_well_formed`] — 어떤 장치/툴 조합을 흘려도 이 불변식이 깨지면
//! 아키텍처 계약 위반이다 (ideation `invariants.js`의 포트).

use serde::{Deserialize, Serialize};

/// 문서 커맨드 — 툴 상태기계가 포인터 이벤트를 번역한 결과물.
///
/// 좌표는 툴이 받은 그대로의 경계 좌표(스크린/UI 공간)다. 페이지 좌표로의
/// sense-normalization은 소비자(앱 실행기)의 지오메트리 몫이다.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// 잉크 세션 시작 — `tool`은 워크스페이스 툴 레지스트리의 이름.
    BeginStroke {
        tool: String,
        point: [f32; 2],
        pressure: f32,
    },
    /// 진행 중 획에 점 추가 (O(Δ) — 새 점만).
    ExtendStroke { point: [f32; 2], pressure: f32 },
    /// 잉크 세션 종료 — 획 확정(커밋).
    EndStroke,
    /// 지우개 세션 — 반경 소비자 결정 (툴은 지식 0).
    EraseAt { point: [f32; 2] },
    /// 지우개 세션 종료.
    EndErase,
    /// 실행취소.
    Undo,
    /// 즉시 커맨드 (워크스페이스가 모르는 action key의 통과) — "color-wheel",
    /// "next-page" 등. 실행기가 네임스페이스를 해석한다.
    Immediate { key: String },
}

impl Command {
    /// 진단/로그용 타입 이름 (JS의 `c.type` 문자열과 동일한 케밥 케이스).
    pub fn kind(&self) -> &'static str {
        match self {
            Command::BeginStroke { .. } => "begin-stroke",
            Command::ExtendStroke { .. } => "extend-stroke",
            Command::EndStroke => "end-stroke",
            Command::EraseAt { .. } => "erase-at",
            Command::EndErase => "end-erase",
            Command::Undo => "undo",
            Command::Immediate { .. } => "immediate",
        }
    }
}

/// 커맨드 스트림의 잘-형성(well-formed) 검사 — 세션 불변식.
///
/// 반환: `Ok(())` 또는 첫 위반의 설명. `Immediate`/`Undo`는 언제나 허용.
pub fn check_well_formed(stream: &[Command]) -> Result<(), String> {
    #[derive(PartialEq, Clone, Copy, Debug)]
    enum Mode {
        Stroke,
        Erase,
    }
    let mut mode: Option<Mode> = None;
    let mut begins = 0usize;
    let mut ends = 0usize;
    let mut erase_opens = 0usize;
    let mut erase_closes = 0usize;

    for c in stream {
        match c {
            Command::BeginStroke { .. } => {
                if mode.is_some() {
                    return Err(format!("세션 중복 begin (현재 {:?})", mode));
                }
                mode = Some(Mode::Stroke);
                begins += 1;
            }
            Command::ExtendStroke { .. } => {
                if mode != Some(Mode::Stroke) {
                    return Err("획 세션 밖 extend-stroke".into());
                }
            }
            Command::EndStroke => {
                if mode != Some(Mode::Stroke) {
                    return Err("획 세션 밖 end-stroke".into());
                }
                mode = None;
                ends += 1;
            }
            Command::EraseAt { .. } => {
                if mode == Some(Mode::Stroke) {
                    return Err("획 진행 중 erase-at".into());
                }
                if mode != Some(Mode::Erase) {
                    mode = Some(Mode::Erase);
                    erase_opens += 1;
                } // erase 세션 중 추가 erase-at — 같은 세션 (연속 지우기)
            }
            Command::EndErase => {
                if mode != Some(Mode::Erase) {
                    return Err("erase 세션 밖 end-erase".into());
                }
                mode = None;
                erase_closes += 1;
            }
            Command::Undo | Command::Immediate { .. } => {}
        }
    }

    if let Some(m) = mode {
        return Err(format!("닫히지 않은 세션: {m:?}"));
    }
    if begins != ends {
        return Err(format!("begin/end 불일치: {begins}/{ends}"));
    }
    if erase_opens != erase_closes {
        return Err(format!("erase 세션 불일치: {erase_opens}/{erase_closes}"));
    }
    Ok(())
}

/// 스트림의 커맨드 종류 목록 (진단용 — JS `commandTypes`에 대응).
pub fn command_kinds(stream: &[Command]) -> Vec<&'static str> {
    stream.iter().map(|c| c.kind()).collect()
}

/// 직렬화 가능 마커 — 커맨드 자체는 세션 기록(녹화/재생)의 원료가 된다.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct CommandRecord {
    kind: String,
}
