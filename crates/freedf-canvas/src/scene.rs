//! 장면 모델 — 스트로크 저장소와 **revision 기반 증분 diff**.
//!
//! 이 모듈은 실전에서 반복 재발하는 두 버그를 타입으로 막습니다:
//! 1. "커밋마다 전체 재구성" (필기 버벅임) → [`SceneStore::changes_since`]로
//!    신규 획만 증분 처리.
//! 2. "낡은 캐시 재사용" (획/undo 실종) → 모든 스냅샷이 [`Revision`]을
//!    달고, 소비자는 rev를 비교해야 합니다.

/// 스트로크 id — 0은 무효 (DB 시퀀스/로컬 풀과 호환).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StrokeId(pub u64);

/// 장면 변경 단조 카운터 — 추가/삭제마다 증가.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Revision(pub u64);

/// 레이어 종류 — 굽기/그리기 순서의 기준.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayerKind {
    Ink,
    Paper,
    SearchHighlight,
}

/// 스트로크의 한 점.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrokePoint {
    pub position: crate::geom::PagePoint,
    /// 필압 0..1.
    pub pressure: f32,
    /// 이 점이 쓰인 시각 (epoch ms) — 번짐 나이 계산의 기준.
    pub t_ms: u64,
    /// 입력 시점에 잠금된 폭 (pt) — 0이면 프로파일 모델로 계산.
    pub width: f32,
}

/// 장면의 스트로크 하나.
#[derive(Debug, Clone, PartialEq)]
pub struct Stroke {
    pub id: StrokeId,
    pub kind: LayerKind,
    /// 도구 종류 — 폭 모델/캡/알파 모델 선택의 기준.
    pub tool: freedf_core::model::ToolType,
    /// [r, g, b, a] — 알파는 잉크 모델이 점별로 조절.
    pub color: [u8; 4],
    /// 기준 두께 (pt) — 실제 폭은 WidthModel이 필압/속도로 변조.
    pub base_width: f32,
    pub points: Vec<StrokePoint>,
    /// 획 시작 시각 (epoch ms) — 0이면 무효, id로 시드를 대체.
    pub created_ms: u64,
}

/// [`Revision`] 구간의 변경 사항 — 굽기 파이프라인의 증분 단위.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Changes {
    /// 이 rev 이후에 추가된 스트로크 (순서 유지).
    pub added: Vec<Stroke>,
    /// 이 rev 이후에 삭제된 스트로크 id.
    pub removed: Vec<StrokeId>,
    pub from_revision: Revision,
    pub to_revision: Revision,
}

impl Changes {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty()
    }
}

/// 굽기 작업에 넘길 장면 스냅샷 — rev를 달고 있어 낡은 스냅샷 감지 가능.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneSnapshot {
    pub revision: Revision,
    pub strokes: Vec<Stroke>,
}

#[derive(Debug, Clone, PartialEq)]
enum SceneOp {
    Add(Stroke),
    Remove(StrokeId),
}

/// 장면 저장소 — 추가/삭제와 rev 기반 diff.
///
/// 스트로크는 **삽입 순서**로 유지됩니다 (증분 append 경계의 근거).
/// 연산 로그(append-only)는 스켈레톤 구현 — 나중에 주기적 압축 가능.
#[derive(Debug, Default)]
pub struct SceneStore {
    strokes: Vec<Stroke>,
    ops: Vec<(Revision, SceneOp)>,
    rev: Revision,
}

impl SceneStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// 현재 revision.
    pub fn rev(&self) -> Revision {
        self.rev
    }

    /// 스트로크 추가 — rev가 1 증가하고 새 rev를 반환.
    pub fn add(&mut self, stroke: Stroke) -> Revision {
        self.rev = Revision(self.rev.0 + 1);
        self.ops
            .push((self.rev, SceneOp::Add(stroke.clone())));
        self.strokes.push(stroke);
        self.rev
    }

    /// 스트로크 삭제 — 있으면 rev가 1 증가하고 삭제된 획을 반환.
    pub fn remove(&mut self, id: StrokeId) -> Option<Stroke> {
        let idx = self.strokes.iter().position(|s| s.id == id)?;
        let removed = self.strokes.remove(idx);
        self.rev = Revision(self.rev.0 + 1);
        self.ops.push((self.rev, SceneOp::Remove(id)));
        Some(removed)
    }

    /// 삽입 순서의 스트로크 목록.
    pub fn strokes(&self) -> &[Stroke] {
        &self.strokes
    }

    /// 굽기용 스냅샷.
    pub fn snapshot(&self) -> SceneSnapshot {
        SceneSnapshot {
            revision: self.rev,
            strokes: self.strokes.clone(),
        }
    }

    /// `revision` 이후의 변경 — 증분 굽기/동기화의 단일 진입점.
    ///
    /// **계약**: 삭제가 하나라도 섞여 있으면 안전을 위해 `added`를 비우고
    /// `removed`만 반환할 수도 있습니다 — 소비자는 `removed`가 비어 있지
    /// 않으면 전체 재구성을 선택해야 합니다 (스켈레톤은 둘 다 채웁니다).
    pub fn changes_since(&self, revision: Revision) -> Changes {
        let mut changes = Changes {
            from_revision: revision,
            to_revision: self.rev,
            ..Changes::default()
        };
        for (rev, op) in &self.ops {
            if *rev <= revision {
                continue;
            }
            match op {
                SceneOp::Add(stroke) => changes.added.push(stroke.clone()),
                SceneOp::Remove(id) => changes.removed.push(*id),
            }
        }
        changes
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::PagePoint;

    fn stroke(id: u64) -> Stroke {
        Stroke {
            id: StrokeId(id),
            kind: LayerKind::Ink,
            tool: freedf_core::model::ToolType::Pen,
            color: [0, 0, 0, 255],
            base_width: 2.0,
            points: vec![StrokePoint {
                position: PagePoint::new(id as f32, 0.0),
                pressure: 0.5,
                t_ms: 0,
                width: 0.0,
            }],
            created_ms: 0,
        }
    }

    /// 계약: add/remove가 rev를 증가시킵니다.
    #[test]
    fn mutations_bump_revision() {
        let mut store = SceneStore::new();
        assert_eq!(store.rev(), Revision(0));
        store.add(stroke(1));
        assert_eq!(store.rev(), Revision(1));
        store.add(stroke(2));
        assert_eq!(store.rev(), Revision(2));
        assert!(store.remove(StrokeId(1)).is_some());
        assert_eq!(store.rev(), Revision(3));
        assert!(store.remove(StrokeId(99)).is_none(), "없는 획 삭제는 rev 불변");
        assert_eq!(store.rev(), Revision(3));
    }

    /// 계약: changes_since는 그 rev 이후의 변경만 반환 — 증분 굽기의 근거.
    #[test]
    fn changes_since_returns_only_newer_ops() {
        let mut store = SceneStore::new();
        store.add(stroke(1));
        let mid = store.add(stroke(2));
        store.add(stroke(3));
        store.remove(StrokeId(1));
        let end = store.rev();

        let from_mid = store.changes_since(mid);
        assert_eq!(from_mid.from_revision, mid);
        assert_eq!(from_mid.to_revision, end);
        assert_eq!(from_mid.added.len(), 1, "mid 이후 추가는 3번 하나");
        assert_eq!(from_mid.added[0].id, StrokeId(3));
        assert_eq!(from_mid.removed, vec![StrokeId(1)], "삭제는 mid 이후");

        let all = store.changes_since(Revision(0));
        assert_eq!(all.added.len(), 3);
        assert_eq!(all.removed, vec![StrokeId(1)]);
    }

    /// 계약: 스냅샷은 rev와 내용이 일치해야 합니다 (낡은 스냅샷 감지 근거).
    #[test]
    fn snapshot_carries_revision_and_strokes() {
        let mut store = SceneStore::new();
        store.add(stroke(1));
        let snap = store.snapshot();
        assert_eq!(snap.revision, store.rev());
        assert_eq!(snap.strokes, store.strokes());
    }
}
