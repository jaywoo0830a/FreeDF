//! `bake` 모듈 단위 테스트 — `src/bake.rs`의 `#[cfg(test)]` 모듈에서 이동했습니다.

use freedf_canvas::bake::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use freedf_canvas::ink::Mesh;
use freedf_canvas::scene::SceneSnapshot;
use freedf_canvas::geom::PagePoint;
use freedf_canvas::ink::BallWidth;
use freedf_canvas::scene::{LayerKind, SceneStore, StrokeId, StrokePoint, Stroke};
use freedf_canvas::ink::SoakAlpha;

fn stroke(id: u64) -> Stroke {
    Stroke {
        id: StrokeId(id),
        kind: LayerKind::Ink,
        tool: freedf_core::model::ToolType::Pen,
        color: [0, 0, 0, 255],
        base_width: 2.0,
        points: vec![
            StrokePoint {
                position: PagePoint::new(0.0, 0.0),
                pressure: 1.0,
                t_ms: 0,
                width: 0.0,
            },
            StrokePoint {
                position: PagePoint::new(10.0, 0.0),
                pressure: 1.0,
                t_ms: 10,
                width: 0.0,
            },
        ],
        created_ms: 0,
    }
}

/// 계약: 순수 워커는 스레드 없이 결정적으로 굽습니다.
#[test]
fn simple_worker_bakes_all_strokes_purely() {
    let worker = SimpleWorker::new(freedf_canvas::ink::RibbonMesher::new(
        BallWidth,
        SoakAlpha::default(),
    ));
    let mut store = SceneStore::new();
    store.add(stroke(1));
    store.add(stroke(2));
    let page = worker.bake(store.snapshot(), BakeParams::default(), 100);
    assert_eq!(page.revision, store.rev());
    assert!(page.mesh.is_well_formed());
    assert_eq!(page.mesh.vertices.len(), 8, "세그먼트 2개 × 사각형");
}

/// 계약: 진행 중 제출은 Busy, 완료 후 poll이 결과를 전달합니다.
/// 타이밍 의존 제거 — 워커는 테스트가 문을 열 때만 완료합니다.
#[test]
fn service_is_non_blocking_and_delivers() {
    struct ManualWorker {
        gate: Arc<AtomicBool>,
    }
    impl BakeWorker for ManualWorker {
        fn bake(&self, snapshot: SceneSnapshot, params: BakeParams, _now: u64) -> BakedPage {
            // 테스트가 gate를 열 때까지 대기 (결정적).
            while !self.gate.load(Ordering::Acquire) {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            BakedPage {
                revision: snapshot.revision,
                params,
                mesh: Mesh::default(),
            }
        }
    }
    let gate = Arc::new(AtomicBool::new(false));
    let service =
        BakeService::start(Box::new(ManualWorker { gate: Arc::clone(&gate) }));

    let mut store = SceneStore::new();
    store.add(stroke(1));
    let snapshot = store.snapshot();

    service
        .request(snapshot.clone(), BakeParams::default(), 0)
        .expect("첫 제출");
    assert!(service.busy(), "진행 중이어야 함");
    assert!(
        service.request(snapshot, BakeParams::default(), 0).is_err(),
        "진행 중 재제출은 Busy"
    );
    assert!(service.poll().is_none(), "완료 전 poll은 None");

    gate.store(true, Ordering::Release);
    // 완료 대기 — 테스트 코드만 블로킹 허용 (UI 스레드가 아님).
    let mut got = None;
    for _ in 0..1_000 {
        got = service.poll();
        if got.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let page = got.expect("poll 결과").expect("굽기 성공");
    assert_eq!(page.revision, store.rev());
    assert!(!service.busy());
}
