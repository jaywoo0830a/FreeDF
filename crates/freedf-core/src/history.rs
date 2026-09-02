//! 실행취소(undo)/다시실행(redo) 이력.
//!
//! 전체 저장소 스냅샷 대신 **diff(Edit)** 를 쌓아 두어 메모리와 복사 비용을
//! 아끼고, `AnnotationStore::apply_edit` / `apply_inverse` 로 되돌립니다.

use crate::model::{PageIndex, Stroke};
use serde::{Deserialize, Serialize};

/// 저장소에 적용할 수 있는 단일 변경 단위.
///
/// 두 가지뿐이므로 undo/redo가 매우 단순합니다:
/// - 추가(그리기) ↔ 제거(지우개/전체 지우기)가 서로의 역연산
/// - "페이지 전체 지우기"는 `RemoveStrokes`(모든 스트로크)로 표현
///
/// FreeDF v2: DB의 `doc_edits` 테이블(영속 편집 저널)에도 같은 구조로 저장되어
/// 앱 재시작 후 undo 스택을 복원합니다.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Edit {
    /// 스트로크 추가 (지우개/전체 지우기의 undo 복원에도 사용).
    AddStrokes { page: PageIndex, strokes: Vec<Stroke> },
    /// 스트로크 제거 (지우개 한 번 = 하나의 undo 단계, 페이지 전체 지우기도 동일).
    RemoveStrokes { page: PageIndex, strokes: Vec<Stroke> },
}

impl Edit {
    /// 이 변경을 되돌리는 역연산.
    pub fn inverse(&self) -> Edit {
        match self {
            Edit::AddStrokes { page, strokes } => Edit::RemoveStrokes {
                page: *page,
                strokes: strokes.clone(),
            },
            Edit::RemoveStrokes { page, strokes } => Edit::AddStrokes {
                page: *page,
                strokes: strokes.clone(),
            },
        }
    }
}

/// undo/redo 스택 (내부 표현: 역연산과 원래 연산을 함께 저장하지 않고
/// 각 방향의 `Edit`을 별도 스택에 쌓음).
#[derive(Debug, Clone)]
pub struct History {
    undo: Vec<Edit>,
    redo: Vec<Edit>,
    limit: usize,
}

impl Default for History {
    fn default() -> Self {
        Self::new(256)
    }
}

impl History {
    pub fn new(limit: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            limit,
        }
    }

    /// 새 변경을 기록합니다. 새 변경이 생기면 redo 스택은 초기화됩니다.
    pub fn push(&mut self, edit: Edit) {
        self.redo.clear();
        self.undo.push(edit);
        if self.undo.len() > self.limit {
            let excess = self.undo.len() - self.limit;
            self.undo.drain(0..excess);
        }
    }

    /// 실행취소: 되돌리기 위해 **역연산**을 반환하고 redo 스택에 쌓습니다.
    pub fn undo(&mut self) -> Option<Edit> {
        let edit = self.undo.pop()?;
        self.redo.push(edit.clone());
        Some(edit.inverse())
    }

    /// 다시실행: 원래 연산을 반환하고 undo 스택에 되돌려 놓습니다.
    pub fn redo(&mut self) -> Option<Edit> {
        let edit = self.redo.pop()?;
        self.undo.push(edit.clone());
        Some(edit)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stroke(id: u64) -> Stroke {
        Stroke {
            created_ms: 0,
            id,
            tool: crate::model::ToolType::Pen,
            color: [0, 0, 0, 255],
            width: 2.0,
            points: vec![crate::model::StrokePoint::new(1.0, 2.0, 0.5)],
        }
    }

    fn add_edit(page: PageIndex, strokes: Vec<Stroke>) -> Edit {
        Edit::AddStrokes { page, strokes }
    }

    #[test]
    fn push_clears_redo() {
        let mut h = History::new(10);
        h.push(add_edit(0, vec![stroke(1)]));
        let _ = h.undo();
        assert!(h.can_redo());
        h.push(add_edit(0, vec![stroke(2)]));
        assert!(!h.can_redo());
        assert_eq!(h.undo_len(), 1);
    }

    #[test]
    fn undo_redo_cycle() {
        let mut h = History::new(10);
        h.push(add_edit(0, vec![stroke(1)]));
        assert!(h.can_undo());

        let inverse = h.undo().expect("undo");
        assert!(matches!(inverse, Edit::RemoveStrokes { .. }));
        assert!(!h.can_undo());
        assert!(h.can_redo());

        let original = h.redo().expect("redo");
        assert!(matches!(original, Edit::AddStrokes { .. }));
        assert!(h.can_undo());
        assert!(!h.can_redo());
    }

    #[test]
    fn undo_on_empty_is_none() {
        let mut h = History::new(10);
        assert!(h.undo().is_none());
        assert!(h.redo().is_none());
    }

    #[test]
    fn limit_trims_oldest() {
        let mut h = History::new(3);
        for i in 0..5 {
            h.push(add_edit(0, vec![stroke(i)]));
        }
        assert_eq!(h.undo_len(), 3);
        // 가장 오래된 0,1은 잘려나가고 2,3,4가 남음
        let first = h.undo().expect("undo");
        match first {
            Edit::RemoveStrokes { strokes, .. } => assert_eq!(strokes[0].id, 4),
            _ => panic!("invalid edit"),
        }
    }

    #[test]
    fn clear_resets() {
        let mut h = History::new(10);
        h.push(add_edit(0, vec![stroke(1)]));
        h.clear();
        assert!(!h.can_undo() && !h.can_redo());
        assert_eq!(h.undo_len(), 0);
    }

    #[test]
    fn inverse_of_inverse_returns_original() {
        let edit = Edit::RemoveStrokes {
            page: 3,
            strokes: vec![stroke(1), stroke(2)],
        };
        let restored = edit.inverse().inverse();
        assert_eq!(edit, restored);
    }
}
