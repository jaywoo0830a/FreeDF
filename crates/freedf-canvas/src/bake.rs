//! 굽기 파이프라인 — 워커(순수) + 서비스(스레드, **논블로킹**).
//!
//! 계층 분리:
//! - [`BakeWorker`] — 순수 굽기. `now_ms`를 인자로 받아 결정적, 테스트는
//!   스레드 없이 직접 호출합니다.
//! - [`BakeService`] — 워커를 백그라운드 스레드로 감쌉니다. **UI 스레드
//!   무블록 계약**: 공개 메서드에 블로킹 `recv`/`join`이 없습니다. 제출은
//!   unbounded `try_send` 계열, 수신은 `try_recv` 폴링뿐입니다.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use crate::ink::{bake_strokes, Mesh, Mesher};
use crate::scene::{SceneSnapshot, Revision};

/// 굽기 파라미터 — 바뀌면 재굽기가 필요한 설정 스냅샷 (지금은 줌만).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BakeParams {
    pub zoom: f32,
}

impl Default for BakeParams {
    fn default() -> Self {
        Self { zoom: 1.0 }
    }
}

/// 굽힌 페이지 — UI 스레드가 소비하는 결과물. rev를 달아 낡은 결과 감지.
#[derive(Debug, Clone, PartialEq)]
pub struct BakedPage {
    pub revision: Revision,
    pub params: BakeParams,
    /// 페이지 좌표 메시 (줌은 그리기 단계의 Transform이 적용).
    pub mesh: Mesh,
}

/// 굽기 오류.
///
/// `Busy`는 **재시도 가능** 오류 — 호출자(UI 스레드)는 다음 프레임에
/// 다시 제출하면 됩니다. `WorkerStopped`는 재시작이 필요한 치명적 오류.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BakeError {
    /// 이미 굽기가 진행 중 — 재시도는 다음 프레임에.
    #[error("이미 굽기가 진행 중입니다 (다음 프레임에 재시도)")]
    Busy,
    /// 워커 스레드가 종료됨.
    #[error("워커 스레드가 종료되었습니다")]
    WorkerStopped,
}

/// 굽기 실행 주체 — 스레드 없이도 호출 가능해야 테스트가 순수해집니다.
pub trait BakeWorker: Send {
    fn bake(&self, snapshot: SceneSnapshot, params: BakeParams, now_ms: u64) -> BakedPage;
}

/// 기본 워커 — 모든 스트로크를 메셔로 굽습니다 (전체 굽기).
/// 증분 굽기(Changes 적용)는 같은 트레이트의 다른 구현체로 조합합니다.
pub struct SimpleWorker<M: Mesher> {
    mesher: M,
}

impl<M: Mesher> SimpleWorker<M> {
    pub fn new(mesher: M) -> Self {
        Self { mesher }
    }
}

impl<M: Mesher> BakeWorker for SimpleWorker<M> {
    fn bake(&self, snapshot: SceneSnapshot, params: BakeParams, now_ms: u64) -> BakedPage {
        BakedPage {
            revision: snapshot.revision,
            params,
            mesh: bake_strokes(&self.mesher, &snapshot.strokes, now_ms),
        }
    }
}

struct Job {
    snapshot: SceneSnapshot,
    params: BakeParams,
    now_ms: u64,
}

/// 논블로킹 굽기 서비스.
///
/// **계약**:
/// - `request`는 진행 중이면 즉시 `Busy` (큐잉/대기 없음).
/// - `poll`은 `try_recv` — 완료된 결과가 없으면 즉시 `None`.
/// - `busy`는 원자 플래그 — 진행 중 여부를 언제든 읽을 수 있음.
/// - 블로킹 `recv`는 워커 스레드 내부(`start`)에만 존재.
pub struct BakeService {
    job_tx: Option<Sender<Job>>,
    result_rx: Receiver<Result<BakedPage, BakeError>>,
    in_flight: Arc<AtomicBool>,
    /// 워커를 계속 살려 두는 레퍼런스 (스레드도 자신의 Arc 클론을 가짐).
    _worker: Arc<Box<dyn BakeWorker + Send + Sync + 'static>>,
}

impl BakeService {
    /// 워커 스레드를 띄우고 서비스를 반환 (블로킹 없음 — 스레드만 생성).
    pub fn start(worker: Box<dyn BakeWorker + Send + Sync + 'static>) -> Self {
        let worker = Arc::new(worker);
        let (job_tx, job_rx) = mpsc::channel::<Job>();
        let (result_tx, result_rx) = mpsc::channel();
        let in_flight = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&in_flight);
        let worker_ref = Arc::clone(&worker);
        std::thread::spawn(move || {
            // 이 recv는 워커 스레드 안 — UI 스레드가 호출할 수 없습니다.
            while let Ok(job) = job_rx.recv() {
                let page = worker_ref.bake(job.snapshot, job.params, job.now_ms);
                flag.store(false, Ordering::Release);
                if result_tx.send(Ok(page)).is_err() {
                    break; // 소비자 종료.
                }
            }
            let _ = result_tx.send(Err(BakeError::WorkerStopped));
        });
        Self {
            job_tx: Some(job_tx),
            result_rx,
            in_flight,
            _worker: worker,
        }
    }

    /// 굽기 제출 — 진행 중이면 `Err(Busy)`, 항상 즉시 반환.
    pub fn request(
        &self,
        snapshot: SceneSnapshot,
        params: BakeParams,
        now_ms: u64,
    ) -> Result<(), BakeError> {
        if self.in_flight.swap(true, Ordering::AcqRel) {
            return Err(BakeError::Busy);
        }
        let sent = self
            .job_tx
            .as_ref()
            .ok_or(BakeError::WorkerStopped)?
            .send(Job {
                snapshot,
                params,
                now_ms,
            });
        if sent.is_err() {
            self.in_flight.store(false, Ordering::Release);
            return Err(BakeError::WorkerStopped);
        }
        Ok(())
    }

    /// 결과 폴링 — 매 프레임 호출. 완료된 것이 없으면 `None`.
    pub fn poll(&self) -> Option<Result<BakedPage, BakeError>> {
        self.result_rx.try_recv().ok()
    }

    /// 진행 중인 굽기가 있는지.
    pub fn busy(&self) -> bool {
        self.in_flight.load(Ordering::Acquire)
    }
}
