//! 세션 라우터 — "프레스의 목적지와 완결을 소유하는 객체" (아키텍처 설계 ⑧).
//!
//! ideation `tests/idea4/session-router.test.js`의 참조 구현(`createSessionRouter`)
//! 을 Rust로 이식한 것 — 2026-09-15 필기 유실 회귀의 땜질(`PendingDown`)을
//! 구조로 바꾼 최종 형태다 (`session-router-migration.md` 2.2 단계).
//!
//! 계약 — 타입으로 강제한다:
//! ① 싱크가 볼 수 있는 것은 ([`SessionView`], [`Ctx`]) 뿐 — "지금 다른 시계는
//!    뭐라고 하지?" 질의(예: egui `response.is_pointer_button_down_on()`)는 이
//!    시그니처로는 표현 자체가 되지 않는다.
//! ② 라우터는 시계를 읽지 않는다 — 시간은 [`Ctx::now_ms`] 인자로만 들어온다.
//! ③ 모든 Down 에지는 [`Resolution`] 장부로 정산된다 — 유실은 상태가 아니라
//!    데이터로 관측된다.
//! ④ 교차 소스는 이 객체의 계약 밖이다 — "한 번에 한 포인터"는 상류(허브)의
//!    점유 규칙이 소유하고, 라우터는 그 아래에서 **한 소스의 온전한 스트림**만
//!    본다. 다른 소스의 이벤트는 세션도 장부도 오염시키지 않고
//!    [`Outcome::Foreign`]으로 드러난다.
//!
//! egui 의존 0의 순수 상태기계 — 유닛 테스트가 JS 스펙과 1:1로 대응한다
//! (`session-router-migration.md`의 매핑표).

use freedf_core::input_events::{PointerEvent, PointerPhase, PointerSource};

/// 보류된 Down 에지(세션)의 기본 최대 수명 (ms). 이보다 오래 보류한 세션은
/// 승격하지 않고 만료로 정산한다 — 이미 끝난 프레스가 나중 프레스에 붙는 것을
/// 막는다. 땜질 시절의 `PENDING_DOWN_TTL_MS` 값을 이어받는다.
pub(crate) const SESSION_TTL_MS: u64 = 250;

/// 라이브 세션 워치독 창 (ms). Up 에지가 유실된 세션이 이만큼 **조용하고**
/// 접촉 증거도 없으면 합성 up 으로 닫는다. 펜을 대고 멈춰 있는 동안은 egui가
/// 프레스를 인지하고 있으므로(증거 있음) 닫히지 않는다 — 두 조건이 모두
/// 필요하다: ① 이벤트가 끊겼다 ② 증거가 없다. 한 프레임의 시계 지연(경합)은
/// 창 안에 묻힌다 — 레벨 상태를 한 프레임 읽는 것과는 질이 다른 안전망이다.
pub(crate) const SESSION_STALE_MS: u64 = 500;

/// 싱크의 승인 결정 — 즉답한다.
#[allow(dead_code)] // Hold 는 생산 싱크(기하 즉담)가 쓰지 않는다 — 스펙/안전망 계약
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Decision {
    /// 승인 — 세션을 곧바로 라이브로 연다.
    Now,
    /// 보류 — 라우터가 원래 접촉점과 이후 Drag를 버퍼하고 매 프레임 다시 묻는다.
    Hold,
    /// 거절 — 이 싱크는 이 프레스의 목적지가 아니다.
    Refuse,
}

/// 라우터가 싱크에 명시 전달하는 문맥. 앱이 프레임마다 계산해 채운다 —
/// 싱크가 egui/시계를 몰래 샘플링할 통로가 인터페이스에 없다 (계약 ①②).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Ctx {
    /// 이번 프레임의 시각 — 보류 TTL/세션 나이 판정의 유일한 시간 원천.
    pub now_ms: u64,
    /// 이번 프레임에 접촉 증거가 있었는가 (egui 프레스 인지 / 펜이 표면에 닿아
    /// 있음 / 같은 소스 Drag). "증거 없는 프레임에서는 승격하지 않는다" 정책의
    /// 재료 — 꼬리 hover 방지. 라이브 세션 워치독도 이 값을 요구한다.
    pub evidence: bool,
    /// 이번 프레임이 팬에 의해 소유되는가 — 잉크 금지.
    pub panning: bool,
    /// 포커스 유예 중 — 유예 프레스는 삼켜져야 한다 (의도적 삼킴).
    pub focus_grace: bool,
    /// 포커스 없음 + 아직 획득 안 함 — 이 Down이 포커스 획득 제스처다.
    pub focus_grab_pending: bool,
}

/// 싱크가 보는 세션 뷰 — 판정 재료는 이것뿐 (계약 ①). 접근 전용 스냅샷이다.
/// (일부 필드는 생산 싱크가 쓰지 않지만 스펙 계약의 판정 재료다.)
#[allow(dead_code)]
pub(crate) struct SessionView<'a> {
    pub id: u64,
    pub source: PointerSource,
    pub down: &'a PointerEvent,
    /// 보류 중 버퍼된 Drag — hold의 존재 이유. 승인 시 온전히 함께 재생된다.
    pub drags: &'a [PointerEvent],
    pub age_ms: u64,
}

/// 라우팅 대상 싱크 — 판정([`Sink::admit`])과 소비([`Sink::handle`])가 분리된다.
pub(crate) trait Sink {
    fn name(&self) -> &'static str;
    /// Down 에지의 목적지 판정. 세션/문맥 외에 아무것도 보지 못한다 (계약 ①).
    fn admit(&mut self, session: &SessionView, ctx: &Ctx) -> Decision;
    /// 이벤트 소비. Down→Drag→Up 순서 보존(및 보류 승격 시 `[down, …drags]`
    /// 온전한 세션 재생)은 라우터의 책임 — 싱크는 받은 순서대로만 소비한다.
    fn handle(&mut self, evs: &[PointerEvent]);
}

/// 이벤트 1건의 정산 항목 — [`SessionRouter::dispatch`]/[`SessionRouter::frame`]
/// 이 반환한다. 유실이 "상태"가 아니라 "데이터"로 관측되는 최소 단위 (계약 ③).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// Down 즉시 승인 — 세션 라이브.
    Admitted,
    /// Down 보류 — 승인 대기.
    Holding,
    /// 보류 세션의 Drag — 버퍼됨 (싱크에 아직 새지 않는다).
    Buffered,
    /// 모든 싱크가 거절 — 닫힌 거절. 세션은 생기지 않는다.
    Refused,
    /// 세션 밖 Drag/Up — 목적지 없음. (툴이 무시했을 이벤트.)
    Unrouted,
    /// 교차 소스 — 계약 밖 (상류 허브의 점유 규칙이 걸러야 한다).
    Foreign,
    /// 계약 위반(접촉 없는 Down) — 열린 세션을 합성 up으로 닫고 새 Down은 거절.
    Replaced,
    /// 보류 TTL 만료 — 끝난 프레스가 나중 프레스에 붙지 않는다.
    Expired,
    /// 라이브 세션 워치독 — Up 에지가 유실된 채 기기가 접촉을 보고하지 않는다.
    /// 합성 up 으로 정상 경로에서 닫는다 (스펙 확장: JS 참조 구현의 frame()은
    /// 보류 세션만 다뤘다 — 라이브 세션의 Up 유실은 실기에서만 생기는 축).
    Stale,
    /// 보류가 취소로 정산 (싱크 refuse 전환 / Up 프레임 미승인).
    Cancelled,
    /// 보류가 승격 — 온전한 세션 `[down, …drags(, up)]` 이 한 번에 전달됐다.
    Promoted,
    /// 라이브 세션의 Drag/Up 전달 (또는 만료 정책 deliver — 장부의 `expired` 참조).
    Delivered,
}

impl Outcome {
    /// 진단용 케밥 케이스 — JS 스펙의 `outcome` 문자열과 동일하다.
    #[allow(dead_code)] // 스펙 계약 표기 — 로그는 Report 필드로 발행한다
    pub fn as_str(&self) -> &'static str {
        match self {
            Outcome::Admitted | Outcome::Delivered => "delivered",
            Outcome::Holding => "holding",
            Outcome::Buffered => "buffered",
            Outcome::Refused => "refused",
            Outcome::Unrouted => "unrouted",
            Outcome::Foreign => "foreign",
            Outcome::Replaced => "replaced",
            Outcome::Expired => "expired",
            Outcome::Stale => "stale",
            Outcome::Cancelled => "cancelled",
            Outcome::Promoted => "promoted",
        }
    }
}

/// [`SessionRouter::dispatch`]/[`SessionRouter::frame`]의 반환 보고.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Report {
    pub edge: PointerPhase,
    /// 세션 id — 세션과 무관한 결과(Unrouted/Foreign)는 None.
    pub id: Option<u64>,
    pub source: Option<PointerSource>,
    pub outcome: Outcome,
    /// 목적지 싱크 이름.
    pub sink: Option<&'static str>,
    pub now_ms: u64,
}

/// 장부 행 — Down 에지 하나당 최종 정산 하나 (계약 ③). 터미널 결과:
/// Delivered | Refused | Cancelled | Expired | Replaced.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Resolution {
    pub id: u64,
    pub source: PointerSource,
    pub outcome: Outcome,
    pub sink: Option<&'static str>,
    /// 보류 후 승격으로 전달됐는가 (지연 복구의 관측점 — 땜질의 STROKE-RECOVER).
    pub promoted: bool,
    /// TTL 만료 후에도 전달됐는가 (만료 정책 deliver).
    pub expired: bool,
    pub now_ms: u64,
}

impl Resolution {
    /// 정산 불변식 — 모든 장부 행은 터미널이어야 한다 (테스트가 검사).
    #[allow(dead_code)] // 스펙 API — 유닛 테스트가 계약을 검사한다
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.outcome,
            Outcome::Delivered
                | Outcome::Refused
                | Outcome::Cancelled
                | Outcome::Expired
                | Outcome::Stale
                | Outcome::Replaced
        )
    }
}

/// 장부 요약 — 진단(단일 판정)의 재료. 유실은 상태가 아니라 **데이터**로
/// 관측된다 (계약 ③): 필기 진단은 이 수치를 설정/측정값과 합쳐 한 판정을 낸다.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct LedgerSummary {
    /// 승인(Down 즉시/승격 포함) — 정상 경로로 세션이 열린 수.
    pub delivered: usize,
    /// 닫힌 거절 — 캔버스 밖/팬/포커스 제스처 등 (세션이 열리지 않았다).
    pub refused: usize,
    /// 보류 승격 — 늦게 승인된 프레스 (지연 복구 관측점).
    pub promoted: usize,
    /// 보류 TTL 만료 — 끝난 프레스가 나중 프레스에 붙지 않았다.
    pub expired: usize,
    /// 라이브 워치독 — Up 에지 유실을 합성 up 으로 닫았다 (필기 중단의 흔적).
    pub stale: usize,
    /// 보류 취소 (팬/싱크 거절 전환).
    pub cancelled: usize,
    /// 계약 위반 교체 (접촉 없이 Down).
    pub replaced: usize,
    /// 지금 열려 있는 세션이 있는가.
    pub open: bool,
}

impl<S: Sink> SessionRouter<S> {
    /// 장부 요약 — 진단이 "왜 이 획이 이상한가"를 라우터 사실과 함께 판단한다.
    pub fn summary(&self) -> LedgerSummary {
        let mut s = LedgerSummary {
            open: self.open.is_some(),
            stale: self.stale_count,
            ..LedgerSummary::default()
        };
        for r in &self.resolutions {
            match r.outcome {
                Outcome::Delivered => {
                    s.delivered += 1;
                    if r.promoted {
                        s.promoted += 1;
                    }
                }
                Outcome::Refused => s.refused += 1,
                Outcome::Expired => s.expired += 1,
                Outcome::Stale => s.stale += 1,
                Outcome::Cancelled => s.cancelled += 1,
                Outcome::Replaced => s.replaced += 1,
                _ => {}
            }
        }
        s
    }
}

/// 만료 정책 — 정책은 데이터다 (JS 스펙의 `onAbandon`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AbandonPolicy {
    /// 만료 세션을 버린다 (기본).
    Drop,
    /// 만료돼도 버퍼를 전달한다 — 만료 세션도 버리지 않는 정책.
    Deliver,
}

/// 트레이트 객체 배열로 라우터를 쓰기 위한 위임 impl (서로 다른 싱크 타입).
impl Sink for Box<dyn Sink> {
    fn name(&self) -> &'static str {
        (**self).name()
    }
    fn admit(&mut self, session: &SessionView, ctx: &Ctx) -> Decision {
        (**self).admit(session, ctx)
    }
    fn handle(&mut self, evs: &[PointerEvent]) {
        (**self).handle(evs)
    }
}

/// 열린 세션의 상태.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionState {
    Live,
    Held,
}

struct OpenSession {
    id: u64,
    source: PointerSource,
    down: PointerEvent,
    /// 보류 중 버퍼된 Drag — 승인 시 온전한 세션으로 재생된다.
    drags: Vec<PointerEvent>,
    sink: &'static str,
    at: u64,
    /// 이 세션에 마지막으로 이벤트가 온 시각 — 라이브 워치독의 재료.
    last_event_ms: u64,
    state: SessionState,
    /// 장부 정산이 이미 됐는가 (라이브 세션은 Down 승인 시점에 정산된다).
    resolved: bool,
}

/// 열린 세션 스냅샷 (진단용).
#[allow(dead_code)] // 진단/스펙 API — 유닛 테스트가 계약을 검사한다
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SessionSnapshot {
    pub id: u64,
    pub source: PointerSource,
    pub held: bool,
    pub sink: &'static str,
}

/// 세션 라우터. `S`는 싱크 타입 — 생산 배선은 잉크 싱크 하나,
/// 테스트는 스크립트된 싱크(또는 `Box<dyn Sink>`)를 쓴다.
pub(crate) struct SessionRouter<S: Sink> {
    sinks: Vec<S>,
    ttl_ms: u64,
    /// 라이브 세션 워치독 창 — Up 유실 보험 (#[SESSION_STALE_MS]).
    stale_ms: u64,
    on_abandon: AbandonPolicy,
    seq: u64,
    open: Option<OpenSession>,
    resolutions: Vec<Resolution>,
    /// 워치독이 합성 up 으로 닫은 횟수 — 장부에 새 행을 쓰지 않는 사건이라
    /// (그 세션의 Down 은 이미 delivered 로 정산됐다) 여기서 따로 센다.
    /// 진단(단일 판정)이 "이 획의 Up 이 유실됐다"를 데이터로 관측하는 창구.
    stale_count: usize,
}

impl<S: Sink> SessionRouter<S> {
    pub fn new(sinks: Vec<S>) -> Self {
        Self::with_policy(sinks, SESSION_TTL_MS, AbandonPolicy::Drop)
    }

    pub fn with_policy(sinks: Vec<S>, ttl_ms: u64, on_abandon: AbandonPolicy) -> Self {
        Self {
            sinks,
            ttl_ms,
            stale_ms: SESSION_STALE_MS,
            on_abandon,
            seq: 0,
            open: None,
            resolutions: Vec::new(),
            stale_count: 0,
        }
    }

    /// 라이브 세션 워치독 창 바꾸기 — 정책 노브가 필요해지면 여기로.
    #[allow(dead_code)] // 기본값(SESSION_STALE_MS)을 쓴다 — 테스트/설정 확장점
    pub fn with_stale_ms(mut self, stale_ms: u64) -> Self {
        self.stale_ms = stale_ms;
        self
    }

    /// 지금까지의 정산 장부 — 유실은 여기서 관측된다 (계약 ③).
    #[allow(dead_code)] // 진단/스펙 API — 유닛 테스트가 계약을 검사한다
    pub fn ledger(&self) -> &[Resolution] {
        &self.resolutions
    }

    /// 열린 세션 스냅샷 (진단용).
    #[allow(dead_code)] // 진단/스펙 API — 유닛 테스트가 계약을 검사한다
    pub fn open_session(&self) -> Option<SessionSnapshot> {
        self.open.as_ref().map(|o| SessionSnapshot {
            id: o.id,
            source: o.source,
            held: o.state == SessionState::Held,
            sink: o.sink,
        })
    }

    /// 싱크 목록 (프레임 배선이 기하/정책을 주입할 때 쓴다).
    pub fn sinks_mut(&mut self) -> &mut [S] {
        &mut self.sinks
    }

    fn sink_by_name(&mut self, name: &str) -> &mut S {
        self.sinks
            .iter_mut()
            .find(|s| s.name() == name)
            .unwrap_or_else(|| panic!("등록되지 않은 싱크: {name}"))
    }

    fn record(&mut self, r: Resolution) {
        self.resolutions.push(r);
    }

    fn foreign(&self, edge: PointerPhase, source: PointerSource, now: u64) -> Report {
        Report {
            edge,
            id: None,
            source: Some(source),
            outcome: Outcome::Foreign,
            sink: None,
            now_ms: now,
        }
    }

    fn unrouted(&self, edge: PointerPhase, source: PointerSource, now: u64) -> Report {
        Report {
            edge,
            id: None,
            source: Some(source),
            outcome: Outcome::Unrouted,
            sink: None,
            now_ms: now,
        }
    }

    /// 장치 스트림(에지 보존 어댑터의 출력)을 소비한다 — 프레스의 목적지와
    /// 완결을 여기서 결정한다.
    pub fn dispatch(&mut self, ev: &PointerEvent, ctx: &Ctx) -> Report {
        let now = ctx.now_ms;

        if ev.phase == PointerPhase::Down {
            return self.dispatch_down(ev, ctx);
        }

        // ── Drag / Up ────────────────────────────────────────────────────
        let Some(open) = self.open.as_ref() else {
            return self.unrouted(ev.phase, ev.source, now); // 세션 밖 Drag/Up
        };
        if open.source != ev.source {
            // 교차 소스 샘플 — 세션을 오염시키지 않는다 (다른 소스의 Drag 가
            // 잉크 획에 섞이는 것을 구조적으로 차단; 상류 점유 규칙의 몫).
            return self.foreign(ev.phase, ev.source, now);
        }

        if ev.phase == PointerPhase::Drag {
            let (id, name, source, held) = {
                let open = self.open.as_mut().unwrap();
                open.drags.push(*ev);
                open.last_event_ms = now; // 워치독 재료 — 이벤트가 왔으니 살아 있다
                (open.id, open.sink, open.source, open.state == SessionState::Held)
            };
            if held {
                return Report {
                    edge: PointerPhase::Drag,
                    id: Some(id),
                    source: Some(source),
                    outcome: Outcome::Buffered,
                    sink: Some(name),
                    now_ms: now,
                };
            }
            self.sink_by_name(name).handle(std::slice::from_ref(ev));
            return Report {
                edge: PointerPhase::Drag,
                id: Some(id),
                source: Some(source),
                outcome: Outcome::Delivered,
                sink: Some(name),
                now_ms: now,
            };
        }

        // ── Up ───────────────────────────────────────────────────────────
        // 라이브 세션은 즉시 전달하고 닫는다 (Down 에지는 이미 정산됨).
        let open = self.open.take().expect("세션 존재는 위에서 확인했다");
        let id = open.id;
        let name = open.sink;
        let source = open.source;
        if open.state == SessionState::Live {
            self.sink_by_name(name).handle(std::slice::from_ref(ev));
            return Report {
                edge: PointerPhase::Up,
                id: Some(id),
                source: Some(source),
                outcome: Outcome::Delivered,
                sink: Some(name),
                now_ms: now,
            };
        }
        // 보류 세션의 마지막 판정 기회 — 빠른 탭도 유실되지 않는다.
        let session = SessionView {
            id,
            source,
            down: &open.down,
            drags: &open.drags,
            age_ms: now.saturating_sub(open.at),
        };
        if self.sink_by_name(name).admit(&session, ctx) == Decision::Now {
            let mut evs = open.drags.clone();
            evs.insert(0, open.down);
            evs.push(*ev); // 온전한 세션 [down, …drags, up]
            self.sink_by_name(name).handle(&evs);
            self.record(Resolution {
                id,
                source,
                outcome: Outcome::Delivered,
                sink: Some(name),
                promoted: true,
                expired: false,
                now_ms: now,
            });
            return Report {
                edge: PointerPhase::Up,
                id: Some(id),
                source: Some(source),
                outcome: Outcome::Promoted,
                sink: Some(name),
                now_ms: now,
            };
        }
        self.record(Resolution {
            id,
            source,
            outcome: Outcome::Cancelled,
            sink: Some(name),
            promoted: false,
            expired: false,
            now_ms: now,
        });
        Report {
            edge: PointerPhase::Up,
            id: Some(id),
            source: Some(source),
            outcome: Outcome::Cancelled,
            sink: Some(name),
            now_ms: now,
        }
    }
}

impl<S: Sink> SessionRouter<S> {
    fn dispatch_down(&mut self, ev: &PointerEvent, ctx: &Ctx) -> Report {
        let now = ctx.now_ms;
        // 교차 소스 — 라우터의 계약 밖이다 (계약 ④). "한 번에 한 포인터"는
        // 상류(허브)의 점유 규칙이 소유하고, 실제 배치에서 라우터는 그 아래에
        // 놓인다. 여기서는 배선 실수를 조용히 흡수하는 대신 드러난다 — 세션도
        // 장부도 오염하지 않는다.
        if self.open.as_ref().is_some_and(|o| o.source != ev.source) {
            return self.foreign(PointerPhase::Down, ev.source, now);
        }
        if let Some(open) = self.open.take() {
            // 같은 소스의 Down — 업스트림(어댑터) 계약 위반이다 (접촉이 아직
            // 열려 있는데 Down?). 열린 세션을 합성 up 으로 닫고(워크스페이스의
            // 획 경계 합성과 같은 원리) 정산한다.
            if !open.resolved {
                self.record(Resolution {
                    id: open.id,
                    source: open.source,
                    outcome: Outcome::Replaced,
                    sink: Some(open.sink),
                    promoted: false,
                    expired: false,
                    now_ms: now,
                });
            }
            let last = open
                .drags
                .last()
                .map(|d| d.point)
                .unwrap_or(open.down.point);
            let synth = PointerEvent {
                source: open.source,
                phase: PointerPhase::Up,
                point: last,
                pressure: open.down.pressure,
                tilt: open.down.tilt,
            };
            self.sink_by_name(open.sink).handle(&[synth]);
        }

        self.seq += 1;
        let id = self.seq;
        // Down 자체가 접촉 증거 — 라우팅 판정에 전달하는 문맥은 evidence=참이다
        // (JS 스펙: `sink.admit(session, { now, evidence: true })`).
        let down_ctx = Ctx {
            evidence: true,
            ..*ctx
        };
        // 우선순위는 sinks 배열 순서다 — 앞선 싱크가 즉답할 때까지 묻는다.
        for i in 0..self.sinks.len() {
            let session = SessionView {
                id,
                source: ev.source,
                down: ev,
                drags: &[],
                age_ms: 0,
            };
            match self.sinks[i].admit(&session, &down_ctx) {
                Decision::Now => {
                    // Down 자체가 접촉 증거 — 즉답을 존중해 곧바로 라이브.
                    let name = self.sinks[i].name();
                    self.open = Some(OpenSession {
                        id,
                        source: ev.source,
                        down: *ev,
                        drags: Vec::new(),
                        sink: name,
                        at: now,
                        last_event_ms: now,
                        state: SessionState::Live,
                        resolved: true,
                    });
                    self.record(Resolution {
                        id,
                        source: ev.source,
                        outcome: Outcome::Delivered,
                        sink: Some(name),
                        promoted: false,
                        expired: false,
                        now_ms: now,
                    });
                    self.sinks[i].handle(std::slice::from_ref(ev));
                    return Report {
                        edge: PointerPhase::Down,
                        id: Some(id),
                        source: Some(ev.source),
                        outcome: Outcome::Admitted,
                        sink: Some(name),
                        now_ms: now,
                    };
                }
                Decision::Hold => {
                    let name = self.sinks[i].name();
                    self.open = Some(OpenSession {
                        id,
                        source: ev.source,
                        down: *ev,
                        drags: Vec::new(),
                        sink: name,
                        at: now,
                        last_event_ms: now,
                        state: SessionState::Held,
                        resolved: false,
                    });
                    return Report {
                        edge: PointerPhase::Down,
                        id: Some(id),
                        source: Some(ev.source),
                        outcome: Outcome::Holding,
                        sink: Some(name),
                        now_ms: now,
                    };
                }
                Decision::Refuse => continue, // 다음 싱크
            }
        }
        // 닫힌 거절 — 세션 없음. 뒤따르는 Drag/Up 은 unrouted 다 (점 연발 차단).
        self.record(Resolution {
            id,
            source: ev.source,
            outcome: Outcome::Refused,
            sink: None,
            promoted: false,
            expired: false,
            now_ms: now,
        });
        Report {
            edge: PointerPhase::Down,
            id: Some(id),
            source: Some(ev.source),
            outcome: Outcome::Refused,
            sink: None,
            now_ms: now,
        }
    }
}

impl<S: Sink> SessionRouter<S> {
    /// 프레임마다 — 라우터의 **유일한 시간 진입점**.
    ///
    /// ① 보류 세션 재판정 (승격/만료/취소).
    /// ② 라이브 세션 워치독 — Up 에지 유실 보험 (스펙 확장).
    ///
    /// 반환: 정산 보고 | None (할 일 없음).
    pub fn frame(&mut self, ctx: &Ctx) -> Option<Report> {
        let open = self.open.as_ref()?;
        match open.state {
            SessionState::Live => self.frame_live(ctx),
            SessionState::Held => self.frame_held(ctx),
        }
    }

    /// 라이브 세션 워치독 — Up 에지가 유실된 채 기기가 접촉을 보고하지 않으면
    /// (증거 없음 + stale_ms 조용함) **합성 up** 을 싱크에 전달해 정상 경로로
    /// 닫는다. 툴 세션이 영원히 열려 있는 상태(그 뒤의 모든 Drag 가 옛 획에
    /// 붙는 상태)를 구조적으로 막는다.
    ///
    /// 한 프레임의 시계 지연(0916 경합: egui가 한 프레임 늦게 아는 것)은 창 안에
    /// 묻힌다 — 레벨 상태를 한 프레임 읽어 획을 자르는 것과는 질이 다르다.
    fn frame_live(&mut self, ctx: &Ctx) -> Option<Report> {
        let now = ctx.now_ms;
        let open = self.open.as_ref()?;
        if ctx.evidence || now.saturating_sub(open.last_event_ms) <= self.stale_ms {
            return None;
        }
        let id = open.id;
        let name = open.sink;
        let source = open.source;
        let open = self.open.take().unwrap();
        let last = open
            .drags
            .last()
            .map(|d| d.point)
            .unwrap_or(open.down.point);
        let synth = PointerEvent {
            source: open.source,
            phase: PointerPhase::Up,
            point: last,
            pressure: open.down.pressure,
            tilt: open.down.tilt,
        };
        self.sink_by_name(name).handle(&[synth]);
        self.stale_count += 1;
        // 장부에는 **새 행을 쓰지 않는다**: 이 세션의 Down 에지는 승인 시점에
        // 이미 `delivered` 로 정산됐다 (계약 ③ — Down 에지 하나당 정산 하나).
        // 워치독은 세션을 닫는 사건이고, 관측 채널은 이 Report(→ 로그)다.
        Some(Report {
            edge: PointerPhase::Up,
            id: Some(id),
            source: Some(source),
            outcome: Outcome::Stale,
            sink: Some(name),
            now_ms: now,
        })
    }

    /// 보류 세션 재판정 (TTL 만료 / 승격 / 취소).
    fn frame_held(&mut self, ctx: &Ctx) -> Option<Report> {
        let open = self.open.as_ref()?;
        let id = open.id;
        let source = open.source;
        let name = open.sink;
        let at = open.at;
        let now = ctx.now_ms;

        if now.saturating_sub(at) > self.ttl_ms {
            // 만료 — 정책은 데이터다.
            let open = self.open.take().unwrap();
            if self.on_abandon == AbandonPolicy::Deliver {
                // 원래 접촉점 + 버퍼 — 버리지 않고 전달한다.
                let mut evs = open.drags;
                evs.insert(0, open.down);
                self.sink_by_name(name).handle(&evs);
                self.record(Resolution {
                    id,
                    source,
                    outcome: Outcome::Delivered,
                    sink: Some(name),
                    promoted: false,
                    expired: true,
                    now_ms: now,
                });
                return Some(Report {
                    edge: PointerPhase::Down,
                    id: Some(id),
                    source: Some(source),
                    outcome: Outcome::Promoted,
                    sink: Some(name),
                    now_ms: now,
                });
            }
            self.record(Resolution {
                id,
                source,
                outcome: Outcome::Expired,
                sink: Some(name),
                promoted: false,
                expired: false,
                now_ms: now,
            });
            return Some(Report {
                edge: PointerPhase::Down,
                id: Some(id),
                source: Some(source),
                outcome: Outcome::Expired,
                sink: Some(name),
                now_ms: now,
            });
        }

        let (down, drags) = {
            let o = self.open.as_ref().unwrap();
            (o.down, o.drags.clone())
        };
        let session = SessionView {
            id,
            source,
            down: &down,
            drags: &drags,
            age_ms: now.saturating_sub(at),
        };
        match self.sink_by_name(name).admit(&session, ctx) {
            Decision::Now => {
                // 온전한 세션 — 원래 접촉점 + 버퍼 재생 (유실 없음).
                let mut evs = drags;
                evs.insert(0, down);
                self.sink_by_name(name).handle(&evs);
                let o = self.open.as_mut().unwrap();
                o.state = SessionState::Live; // 승격 — 세션은 닫히지 않고 라이브로 계속
                o.resolved = true;
                self.record(Resolution {
                    id,
                    source,
                    outcome: Outcome::Delivered,
                    sink: Some(name),
                    promoted: true,
                    expired: false,
                    now_ms: now,
                });
                Some(Report {
                    edge: PointerPhase::Down,
                    id: Some(id),
                    source: Some(source),
                    outcome: Outcome::Promoted,
                    sink: Some(name),
                    now_ms: now,
                })
            }
            Decision::Refuse => {
                // 싱크가 이 프레스를 내려놓았다 (예: 팬이 소유) — 취소 정산.
                self.open = None;
                self.record(Resolution {
                    id,
                    source,
                    outcome: Outcome::Cancelled,
                    sink: Some(name),
                    promoted: false,
                    expired: false,
                    now_ms: now,
                });
                Some(Report {
                    edge: PointerPhase::Down,
                    id: Some(id),
                    source: Some(source),
                    outcome: Outcome::Cancelled,
                    sink: Some(name),
                    now_ms: now,
                })
            }
            Decision::Hold => Some(Report {
                edge: PointerPhase::Down,
                id: Some(id),
                source: Some(source),
                outcome: Outcome::Holding,
                sink: Some(name),
                now_ms: now,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use freedf_core::input_commands::{check_well_formed, Command};
    use freedf_core::input_events::{ActionSource, InputEvent, NO_TILT};
    use freedf_core::input_workspace::Workspace;
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;

    fn pen(phase: PointerPhase, x: f32) -> PointerEvent {
        PointerEvent {
            source: PointerSource::Pen,
            phase,
            point: [x, 0.0],
            pressure: 0.3,
            tilt: NO_TILT,
        }
    }

    fn pad(phase: PointerPhase, x: f32) -> PointerEvent {
        PointerEvent {
            source: PointerSource::Pad,
            phase,
            point: [x, 0.0],
            pressure: 1.0,
            tilt: NO_TILT,
        }
    }

    fn ctx(now: u64, evidence: bool) -> Ctx {
        Ctx {
            now_ms: now,
            evidence,
            panning: false,
            focus_grace: false,
            focus_grab_pending: false,
        }
    }

    fn kinds(cmds: &[Command]) -> Vec<&'static str> {
        cmds.iter().map(|c| c.kind()).collect()
    }

    /// 세션 커맨드의 점 (begin/extend).
    fn point_of(c: &Command) -> [f32; 2] {
        match c {
            Command::BeginStroke { point, .. } | Command::ExtendStroke { point, .. } => *point,
            other => panic!("점 없는 커맨드: {:?}", other.kind()),
        }
    }

    /// 이벤트를 받아 워크스페이스로 흘려보내는 공통 처리 —
    /// JS 리깅의 `toHub(hub)`(허브 경유 → 워크스페이스 커맨드)에 해당한다.
    fn consume(ws: &mut Workspace, commands: &mut Vec<Command>, evs: &[PointerEvent]) {
        for e in evs {
            ws.handle(&InputEvent::Pointer(*e));
        }
        ws.take_commands(|c| commands.push(c));
    }

    /// 스크립트된 싱크 — admit 판정을 순서대로 내놓고, 받은 이벤트/판정 인자를 기록한다.
    struct ScriptedSink {
        name: &'static str,
        decisions: VecDeque<Decision>,
        received: Vec<PointerEvent>,
        /// admit 호출 기록 — (세션 id, age_ms, evidence, 버퍼된 drags 수)
        calls: Vec<(u64, u64, bool, usize)>,
        ws: Workspace,
        commands: Vec<Command>,
    }

    impl ScriptedSink {
        fn new(name: &'static str, decisions: &[Decision]) -> Self {
            Self {
                name,
                decisions: decisions.iter().copied().collect(),
                received: Vec::new(),
                calls: Vec::new(),
                ws: Workspace::new(),
                commands: Vec::new(),
            }
        }
    }

    impl Sink for ScriptedSink {
        fn name(&self) -> &'static str {
            self.name
        }
        fn admit(&mut self, s: &SessionView, ctx: &Ctx) -> Decision {
            self.calls
                .push((s.id, s.age_ms, ctx.evidence, s.drags.len()));
            self.decisions.pop_front().unwrap_or(Decision::Refuse)
        }
        fn handle(&mut self, evs: &[PointerEvent]) {
            self.received.extend_from_slice(evs);
            consume(&mut self.ws, &mut self.commands, evs);
        }
    }

    /// 테스트에서 싱크를 라우터에 넣은 뒤에도 관측하려면 공유가 필요하다 —
    /// `Shared<T>` 는 `T: Sink` 를 그대로 위임한다.
    struct Shared<T>(Rc<RefCell<T>>);

    impl<T: Sink + 'static> Shared<T> {
        /// 라우터에는 트레이트 객체로 넣는다 — 서로 다른 싱크 타입을 한 배열에.
        fn boxed(self) -> Box<dyn Sink> {
            Box::new(self)
        }
    }

    impl<T: Sink> Sink for Shared<T> {
        fn name(&self) -> &'static str {
            self.0.borrow().name()
        }
        fn admit(&mut self, s: &SessionView, ctx: &Ctx) -> Decision {
            self.0.borrow_mut().admit(s, ctx)
        }
        fn handle(&mut self, evs: &[PointerEvent]) {
            self.0.borrow_mut().handle(evs)
        }
    }

    /// `Rc<RefCell<T>>` 싱크를 라우터용 트레이트 객체로 포장한다.
    fn shared<T: Sink + 'static>(t: Rc<RefCell<T>>) -> Box<dyn Sink> {
        Shared(t).boxed()
    }

    /// 접촉 증거(같은 소스의 Drag 버퍼)를 기다리는 잉크 싱크 — hold가 필요한 이유의 표본.
    struct EvidenceSink {
        ws: Workspace,
        commands: Vec<Command>,
    }

    impl Sink for EvidenceSink {
        fn name(&self) -> &'static str {
            "ink"
        }
        fn admit(&mut self, s: &SessionView, _ctx: &Ctx) -> Decision {
            if s.drags.is_empty() {
                Decision::Hold
            } else {
                Decision::Now
            }
        }
        fn handle(&mut self, evs: &[PointerEvent]) {
            consume(&mut self.ws, &mut self.commands, evs);
        }
    }

    /// 이상적인 최종 상태의 잉크 싱크 — 순수 기하만 본다 (시간/외부 상태 0).
    struct GeometrySink {
        ws: Workspace,
        commands: Vec<Command>,
    }

    impl Sink for GeometrySink {
        fn name(&self) -> &'static str {
            "ink"
        }
        fn admit(&mut self, s: &SessionView, _ctx: &Ctx) -> Decision {
            // 캔버스 기하: x >= 0 가 캔버스 안이라는 축소 모의.
            if s.down.point[0] >= 0.0 {
                Decision::Now
            } else {
                Decision::Refuse
            }
        }
        fn handle(&mut self, evs: &[PointerEvent]) {
            consume(&mut self.ws, &mut self.commands, evs);
        }
    }

    /// UI 싱크 — 잉크가 거절한 프레스를 액션으로 소화 (툴바 프레스).
    struct ToolbarSink {
        ws: Workspace,
        commands: Vec<Command>,
    }

    impl Sink for ToolbarSink {
        fn name(&self) -> &'static str {
            "toolbar"
        }
        fn admit(&mut self, _s: &SessionView, _ctx: &Ctx) -> Decision {
            Decision::Now
        }
        fn handle(&mut self, evs: &[PointerEvent]) {
            for e in evs {
                if e.phase == PointerPhase::Down {
                    self.ws
                        .handle(&InputEvent::action(ActionSource::Keyboard, "toolbar", None));
                }
            }
            self.ws.take_commands(|c| self.commands.push(c));
        }
    }

    // ── 인터페이스 계약 (JS 스펙 describe 1) ─────────────────────────────

    /// 싱크가 볼 수 있는 것은 (세션, 명시 문맥) 뿐이다.
    #[test]
    fn sink_sees_only_session_and_ctx_no_implicit_sampling() {
        let ink = Rc::new(RefCell::new(ScriptedSink::new("ink", &[Decision::Refuse])));
        let mut router = SessionRouter::new(vec![shared(ink.clone())]);

        router.dispatch(&pen(PointerPhase::Down, 10.0), &ctx(1000, false));

        // 판정 재료 = 이벤트가 스스로 안고 있는 것 + 라우터가 명시 전달한 문맥.
        // `response.dragged()` 류의 "지금 다른 시계는 뭐라고 하지?" 질문은
        // SessionView/Ctx 시그니처로는 표현 자체가 되지 않는다 (타입이 계약).
        let ink = ink.borrow();
        assert_eq!(ink.calls.len(), 1);
        let (id, age_ms, evidence, drags) = ink.calls[0];
        assert_eq!(id, 1);
        assert_eq!(age_ms, 0);
        assert!(evidence, "Down 프레임의 evidence 는 참이다 — Down 자체가 증거");
        assert_eq!(drags, 0);
    }

    /// 라우터는 시계를 읽지 않는다 — 시간은 인자로만 들어온다 (정적 검사).
    /// 구현이 시계를 부르는 순간 "샘플링"이 다시 태어난다.
    #[test]
    fn router_never_reads_the_clock() {
        // 파일 전체(구현+테스트)에 시계 접근이 없어야 한다. 리터럴이 이 테스트
        // 파일에도 등장하지 않게 concat 으로 조립한다.
        let now_api = concat!("::now");
        let instant = concat!("Instant");
        let system = concat!("SystemTime");
        let src = include_str!("session_router.rs");
        assert!(!src.contains(&format!("{instant}{now_api}")));
        assert!(!src.contains(&format!("{system}{now_api}")));
    }

    /// 모든 Down 에지는 정산 장부를 갖는다 — 유실은 상태가 아니라 데이터로 관측.
    #[test]
    fn every_down_edge_is_settled_in_the_ledger() {
        // hold → 승격 → live → 계약 위반(접촉 없는 Down) → 닫힌 거절
        let ink = Rc::new(RefCell::new(ScriptedSink::new(
            "ink",
            &[Decision::Hold, Decision::Now, Decision::Refuse],
        )));
        let mut router = SessionRouter::new(vec![shared(ink.clone())]);

        router.dispatch(&pen(PointerPhase::Down, 10.0), &ctx(1000, true)); // hold
        router.dispatch(&pen(PointerPhase::Drag, 12.0), &ctx(1010, true)); // 버퍼
        router.frame(&ctx(1020, true)); // 승격 — [down, drag] 재생
        router.dispatch(&pen(PointerPhase::Drag, 14.0), &ctx(1030, true)); // live 전달
        router.dispatch(&pen(PointerPhase::Down, 20.0), &ctx(1040, true)); // 위반 — 세션 교체
        router.dispatch(&pen(PointerPhase::Drag, 22.0), &ctx(1050, true)); // unrouted
        router.dispatch(&pen(PointerPhase::Up, 24.0), &ctx(1060, true)); // unrouted

        let ledger = router.ledger();
        let outcomes: Vec<_> = ledger.iter().map(|r| r.outcome).collect();
        assert_eq!(outcomes, vec![Outcome::Delivered, Outcome::Refused]); // Down 2건 = 정산 2건
        assert!(ledger.iter().all(|r| r.is_terminal()));

        // 세션 교체는 워크스페이스의 획 경계 합성과 같은 원리로 닫힌다 — 잘-형성 유지.
        let ink = ink.borrow();
        assert_eq!(
            kinds(&ink.commands),
            vec!["begin-stroke", "extend-stroke", "extend-stroke", "end-stroke"]
        );
        assert!(check_well_formed(&ink.commands).is_ok());
    }

    // ── 라우팅 계약 (JS 스펙 describe 2) ─────────────────────────────────

    /// Down 은 우선순위대로 단 하나의 싱크로 라우팅된다 (툴바 프레스는 잉크가 아니다).
    #[test]
    fn down_reaches_exactly_one_sink_by_priority() {
        let toolbar = Rc::new(RefCell::new(ToolbarSink {
            ws: Workspace::new(),
            commands: Vec::new(),
        }));
        let ink = Rc::new(RefCell::new(ScriptedSink::new("ink", &[Decision::Refuse])));
        let mut router = SessionRouter::new(vec![shared(toolbar.clone()), shared(ink.clone())]);

        let rep = router.dispatch(&pen(PointerPhase::Down, 10.0), &ctx(1000, true));

        // 우선순위 배열의 앞선 싱크(toolbar)가 즉답 — 잉크는 묻지도 않는다.
        assert_eq!(rep.outcome, Outcome::Admitted);
        assert_eq!(rep.sink, Some("toolbar"));
        assert!(ink.borrow().calls.is_empty(), "앞선 싱크가 승인하면 뒤는 묻지 않는다");
        assert_eq!(router.ledger()[0].sink, Some("toolbar"));
    }

    /// 교차 소스는 라우터가 대신 끊거나 섞지 않는다 — 점유 규칙은 상류(허브)의 몫.
    #[test]
    fn foreign_source_never_taints_or_closes_the_session() {
        let ink = Rc::new(RefCell::new(ScriptedSink::new("ink", &[Decision::Now])));
        let mut router = SessionRouter::new(vec![shared(ink.clone())]);

        router.dispatch(&pen(PointerPhase::Down, 10.0), &ctx(1000, true)); // pen live
        // 펜 세션이 살아 있는 동안 패드가 프레스 — 라우터는 끊지 않고 드러낸다.
        assert_eq!(
            router.dispatch(&pad(PointerPhase::Down, 5.0), &ctx(1010, true)).outcome,
            Outcome::Foreign
        );
        assert_eq!(
            router.dispatch(&pad(PointerPhase::Drag, 6.0), &ctx(1020, true)).outcome,
            Outcome::Foreign
        );
        assert_eq!(
            router.dispatch(&pad(PointerPhase::Up, 6.0), &ctx(1030, true)).outcome,
            Outcome::Foreign
        );
        assert_eq!(router.open_session().unwrap().source, PointerSource::Pen); // 세션 무사

        router.dispatch(&pen(PointerPhase::Drag, 12.0), &ctx(1040, true)); // 펜 스트림은 계속
        router.dispatch(&pen(PointerPhase::Up, 14.0), &ctx(1050, true));

        let ink = ink.borrow();
        assert_eq!(
            kinds(&ink.commands),
            vec!["begin-stroke", "extend-stroke", "end-stroke"]
        );
        // 패드 샘플(6)이 획에 섞이지 않았다.
        assert_eq!(ink.received.iter().map(|e| e.point).collect::<Vec<_>>(), vec![
            [10.0, 0.0],
            [12.0, 0.0],
            [14.0, 0.0]
        ]);
        assert_eq!(router.ledger().len(), 1); // foreign Down 은 상류(허브 drop)가 정산하는 영역
    }

    /// 증거 문맥은 프레임에서 명시적으로 흘러온다 — 증거 없는 프레임에서는 승격하지 않는다.
    #[test]
    fn evidence_flows_through_ctx_explicitly() {
        /// "증거가 있는 프레임에만 승인" 정책 — 꼬리 hover 승격 방지의 표본.
        /// 증거를 몰래 샘플링하지 않고 ctx 로 받는다는 점이 계약의 요점이다.
        struct AskSink {
            asks: u32,
            ws: Workspace,
            commands: Vec<Command>,
        }
        impl Sink for AskSink {
            fn name(&self) -> &'static str {
                "ink"
            }
            fn admit(&mut self, _s: &SessionView, ctx: &Ctx) -> Decision {
                self.asks += 1;
                if self.asks == 1 {
                    Decision::Hold
                } else if ctx.evidence {
                    Decision::Now
                } else {
                    Decision::Hold
                }
            }
            fn handle(&mut self, evs: &[PointerEvent]) {
                consume(&mut self.ws, &mut self.commands, evs);
            }
        }
        let ink = Rc::new(RefCell::new(AskSink {
            asks: 0,
            ws: Workspace::new(),
            commands: Vec::new(),
        }));
        let mut router = SessionRouter::new(vec![shared(ink.clone())]);

        router.dispatch(&pen(PointerPhase::Down, 10.0), &ctx(1000, true)); // hold
        router.frame(&ctx(1010, false)); // 증거 없는 프레임 — hold 유지
        assert!(router.open_session().unwrap().held);
        router.frame(&ctx(1020, true)); // 증거 도착 — 승격

        let ink = ink.borrow();
        assert_eq!(kinds(&ink.commands), vec!["begin-stroke"]);
        let r = &router.ledger()[0];
        assert_eq!(r.outcome, Outcome::Delivered);
        assert!(r.promoted);
    }

    /// 최종 상태 — 순수 기하 싱크에서는 hold/승격이 아예 발생하지 않는다
    /// (땜질이 필요 없어지는 세상 — 마이그레이션 완료 형태).
    #[test]
    fn pure_geometry_sink_never_holds() {
        let ink = Rc::new(RefCell::new(GeometrySink {
            ws: Workspace::new(),
            commands: Vec::new(),
        }));
        let mut router = SessionRouter::new(vec![shared(ink.clone())]);

        router.dispatch(&pen(PointerPhase::Down, -5.0), &ctx(900, true)); // 캔버스 밖 → 닫힌 거절
        router.dispatch(&pen(PointerPhase::Up, -5.0), &ctx(910, true));
        router.dispatch(&pen(PointerPhase::Down, 10.0), &ctx(1000, true)); // 안 → 즉시 라이브
        router.dispatch(&pen(PointerPhase::Drag, 12.0), &ctx(1010, true));
        router.dispatch(&pen(PointerPhase::Up, 14.0), &ctx(1020, true));

        // 보류가 없다 — frame 은 할 일이 없다 (판정 지연 0프레임의 근거).
        assert!(router.frame(&ctx(1100, true)).is_none());
        let ink = ink.borrow();
        assert_eq!(
            kinds(&ink.commands),
            vec!["begin-stroke", "extend-stroke", "end-stroke"]
        );
        let outcomes: Vec<_> = router.ledger().iter().map(|r| r.outcome).collect();
        assert_eq!(outcomes, vec![Outcome::Refused, Outcome::Delivered]);
    }

    /// 거절은 닫힌 거절이다 — 세션 없는 Drag 가 아무리 와도 세션은 생기지 않는다.
    #[test]
    fn refused_down_never_becomes_a_session() {
        // 점 연발(힐 로직)의 구조적 차단: 승격의 원점은 언제나 "보류된 Down 에지" —
        // 꼬리 hover Drag 는 unrouted 로 정산될 뿐, 세션을 만들 수 없다.
        let ink = Rc::new(RefCell::new(ScriptedSink::new("ink", &[Decision::Refuse])));
        let mut router = SessionRouter::new(vec![shared(ink.clone())]);

        router.dispatch(&pen(PointerPhase::Down, -5.0), &ctx(1000, true));
        for i in 0..14 {
            router.dispatch(&pen(PointerPhase::Drag, i as f32), &ctx(1000 + i, true)); // 꼬리 hover
        }
        router.dispatch(&pen(PointerPhase::Up, -5.0), &ctx(1020, true));

        assert!(ink.borrow().commands.is_empty());
        let ledger = router.ledger();
        assert_eq!(ledger.len(), 1);
        assert_eq!(ledger[0].outcome, Outcome::Refused);
        assert_eq!(ledger[0].now_ms, 1000);
    }

    /// hold 된 세션은 승인 시 원래 접촉점과 버퍼된 Drag 를 함께 재생한다.
    #[test]
    fn promotion_replays_down_and_buffered_drags() {
        let ink = Rc::new(RefCell::new(EvidenceSink {
            ws: Workspace::new(),
            commands: Vec::new(),
        }));
        let mut router = SessionRouter::new(vec![shared(ink.clone())]);

        // 게이트(증거)가 거짓인 프레임에 Down — 에지는 버려지지 않고 보류된다.
        router.dispatch(&pen(PointerPhase::Down, 10.0), &ctx(1000, true));
        router.dispatch(&pen(PointerPhase::Drag, 12.0), &ctx(1010, true)); // 버퍼 — 싱크에 새지 않는다
        router.dispatch(&pen(PointerPhase::Drag, 14.0), &ctx(1020, true));
        assert!(ink.borrow().commands.is_empty());
        assert!(router.open_session().unwrap().held);

        // 증거가 생겨 승인 — 싱크는 **온전한 세션**을 한 번에 본다.
        router.frame(&ctx(1030, true));
        let ink = ink.borrow();
        assert_eq!(
            kinds(&ink.commands),
            vec!["begin-stroke", "extend-stroke", "extend-stroke"]
        );
        assert_eq!(point_of(&ink.commands[0]), [10.0, 0.0], "원래 접촉점");
        assert_eq!(point_of(&ink.commands[1]), [12.0, 0.0], "버퍼된 첫 샘플 — 유실 없음");
        assert_eq!(point_of(&ink.commands[2]), [14.0, 0.0]);
    }

    /// 빠른 탭 — Up 프레임에야 승인되어도 세션은 완결 전달된다.
    #[test]
    fn fast_tap_survives_late_admission_on_up_frame() {
        // 승인이 Up 프레임에야 나오는 싱크 (egui 캔버스가 늦게 소유를 인정하는 상황).
        let ink = Rc::new(RefCell::new(ScriptedSink::new(
            "ink",
            &[Decision::Hold, Decision::Hold, Decision::Now],
        )));
        let mut router = SessionRouter::new(vec![shared(ink.clone())]);

        router.dispatch(&pen(PointerPhase::Down, 10.0), &ctx(1000, true)); // hold
        router.frame(&ctx(1010, true)); // 아직 준비 안 됨 — hold 유지
        router.dispatch(&pen(PointerPhase::Up, 12.0), &ctx(1020, true)); // 마지막 판정 기회 — 승인

        // 땜질에서는 이 탭이 사라진다 (Up 이 보류를 조용히 파괴). 라우터는
        // 완결 세션 [down, up] 을 전달해 "점 하나"라도 정산한다.
        let ink = ink.borrow();
        assert_eq!(kinds(&ink.commands), vec!["begin-stroke", "end-stroke"]);
        assert_eq!(point_of(&ink.commands[0]), [10.0, 0.0]);
    }

    /// 0916-2 실기 회귀(점 부족)의 계약 고정 — **egui가 한 프레임 늦게 아는 것
    /// (evidence=false)만으로는 라이브 세션이 끝나지 않는다.**
    ///
    /// 종전 앱 보험은 egui 레벨(`primary_down`)을 읽어 "버튼이 올라왔다"고
    /// 판단했고, 게이트가 순수 기하로 바뀐 뒤에는 펜 Down이 egui보다 먼저 온
    /// 프레임을 가짜 Up 으로 오독해 획을 1점으로 잘랐다 (writing.log).
    #[test]
    fn lagging_egui_frame_never_kills_a_live_session() {
        let ink = Rc::new(RefCell::new(ScriptedSink::new("ink", &[Decision::Now])));
        let mut router = SessionRouter::new(vec![shared(ink.clone())]);

        // 펜 Down 이 egui보다 먼저 도착한 프레임 — evidence=false.
        router.dispatch(&pen(PointerPhase::Down, 10.0), &ctx(1000, false));
        assert!(!router.open_session().unwrap().held, "기하 즉답 — 즉시 라이브");

        // egui가 따라잡기 전 프레임들 — 세션은 그대로 살아 있어야 한다.
        assert!(router.frame(&ctx(1016, false)).is_none());
        assert!(router.frame(&ctx(1032, false)).is_none());
        assert!(router.open_session().is_some(), "시계 지연은 종료가 아니다");

        // 이어지는 Drag 는 정상 전달 — 획이 온전히 이어진다.
        router.dispatch(&pen(PointerPhase::Drag, 12.0), &ctx(1040, false));
        router.dispatch(&pen(PointerPhase::Up, 14.0), &ctx(1050, false));

        let ink = ink.borrow();
        assert_eq!(
            kinds(&ink.commands),
            vec!["begin-stroke", "extend-stroke", "end-stroke"],
            "1점 점이 아니라 온전한 획"
        );
        assert_eq!(router.ledger().len(), 1, "Down 1건 = 정산 1건");
    }

    /// Up 에지 유실 보험 — 이벤트가 끊기고 접촉 증거도 없는 세션은 stale 창
    /// 뒤에 **합성 up** 으로 정상 경로에서 닫힌다 (툴 세션이 영원히 열려
    /// 있지 않다 = 이후 Drag 가 옛 획에 붙지 않는다).
    #[test]
    fn lost_up_edge_is_closed_by_the_live_watchdog() {
        let ink = Rc::new(RefCell::new(ScriptedSink::new("ink", &[Decision::Now])));
        let mut router = SessionRouter::new(vec![shared(ink.clone())]);

        router.dispatch(&pen(PointerPhase::Down, 10.0), &ctx(1000, true));
        router.dispatch(&pen(PointerPhase::Drag, 12.0), &ctx(1010, true));

        // 창 경계 안 — 아직 닫지 않는다.
        assert!(router.frame(&ctx(1010 + SESSION_STALE_MS, false)).is_none());
        assert!(router.open_session().is_some());

        // 창 밖 + 증거 없음 → 워치독 정산.
        let rep = router.frame(&ctx(1010 + SESSION_STALE_MS + 1, false)).expect("워치독 정산");
        assert_eq!(rep.outcome, Outcome::Stale);
        assert!(router.open_session().is_none());

        // 장부 불변식 — Down 에지 하나당 정산 하나. 워치독은 새 행을 쓰지 않는다
        // (Down 에지는 승인 시점에 이미 delivered 로 정산됐다).
        let ledger = router.ledger();
        assert_eq!(ledger.len(), 1);
        assert_eq!(ledger[0].outcome, Outcome::Delivered);
        assert!(ledger[0].is_terminal());

        // 합성 up 이 싱크에 전달됐다 — 툴 세션이 정상 경로로 닫힌다.
        let ink = ink.borrow();
        assert_eq!(
            kinds(&ink.commands),
            vec!["begin-stroke", "extend-stroke", "end-stroke"]
        );
        assert_eq!(ink.received.last().unwrap().phase, PointerPhase::Up);
    }

    /// 펜을 대고 멈춰 있어도(이벤트 끊김) 접촉 증거가 있는 동안은 닫지 않는다 —
    /// 워치독은 "조용함"만으로 발동하지 않는다 (두 조건 모두 필요).
    /// 생산 배선에서 이 증거에는 **하드웨어 접촉**(펜 tip/필압>0, evdev와 같은
    /// 시계)과 egui의 프레스 인지가 모두 포함된다.
    #[test]
    fn quiet_but_evidenced_session_stays_open() {
        let ink = Rc::new(RefCell::new(ScriptedSink::new("ink", &[Decision::Now])));
        let mut router = SessionRouter::new(vec![shared(ink.clone())]);

        router.dispatch(&pen(PointerPhase::Down, 10.0), &ctx(1000, true));
        // 이벤트는 끊겼지만 egui가 프레스를 계속 알고 있다.
        assert!(router.frame(&ctx(5000, true)).is_none());
        assert!(router.open_session().is_some(), "멈춰 있는 펜은 획의 끝이 아니다");
        assert_eq!(router.ledger().len(), 1);
    }

    /// TTL 을 넘긴 보류는 만료로 정산된다 — 끝난 프레스가 나중 프레스에 붙지 않는다.
    #[test]
    fn expired_hold_is_settled_and_not_revived() {
        let ink = Rc::new(RefCell::new(EvidenceSink {
            ws: Workspace::new(),
            commands: Vec::new(),
        }));
        let mut router = SessionRouter::new(vec![shared(ink.clone())]);

        router.dispatch(&pen(PointerPhase::Down, 10.0), &ctx(1000, true));
        router.frame(&ctx(1000 + 251, true)); // 만료
        let ledger = router.ledger();
        assert_eq!(ledger.len(), 1);
        assert_eq!(ledger[0].outcome, Outcome::Expired);
        assert_eq!(ledger[0].sink, Some("ink"));
        assert!(router.open_session().is_none());

        router.frame(&ctx(1300, true)); // 부활 금지
        router.dispatch(&pen(PointerPhase::Drag, 99.0), &ctx(1310, true)); // unrouted
        assert!(ink.borrow().commands.is_empty());
    }

    /// hold 중 싱크가 refuse 로 전환하면(팬 소유) 세션은 취소 정산된다.
    #[test]
    fn hold_turned_refuse_cancels_the_session() {
        let ink = Rc::new(RefCell::new(ScriptedSink::new(
            "ink",
            &[Decision::Hold, Decision::Refuse], // 판정 뒤집힘 — 팬 프레임
        )));
        let mut router = SessionRouter::new(vec![shared(ink.clone())]);

        router.dispatch(&pen(PointerPhase::Down, 10.0), &ctx(1000, true));
        router.frame(&ctx(1010, true)); // 싱크가 refuse 로 전환

        let ledger = router.ledger();
        assert_eq!(ledger.len(), 1);
        assert_eq!(ledger[0].outcome, Outcome::Cancelled);
        assert_eq!(ledger[0].sink, Some("ink"));
        assert!(router.open_session().is_none());
        assert!(ink.borrow().commands.is_empty());
    }

    /// onAbandon: deliver — 만료 세션도 버리지 않고 전달할 수 있다 (정책은 데이터).
    #[test]
    fn abandon_policy_is_data() {
        let ink = Rc::new(RefCell::new(EvidenceSink {
            ws: Workspace::new(),
            commands: Vec::new(),
        }));
        let mut router = SessionRouter::with_policy(
            vec![shared(ink.clone())],
            SESSION_TTL_MS,
            AbandonPolicy::Deliver,
        );

        router.dispatch(&pen(PointerPhase::Down, 10.0), &ctx(1000, true));
        router.frame(&ctx(1300, true)); // TTL 지남 — 그래도 전달

        assert_eq!(kinds(&ink.borrow().commands), vec!["begin-stroke"]);
        assert_eq!(router.ledger()[0].outcome, Outcome::Delivered);
        assert!(router.ledger()[0].expired);
    }
}

