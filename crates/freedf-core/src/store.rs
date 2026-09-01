//! 페이지별 주석(스트로크) 저장소와 지우개 히트 테스트, JSON 직렬화.

use crate::history::Edit;
use crate::model::{PageIndex, Stroke, StrokePoint, ToolType};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 한 페이지에 그려진 모든 스트로크.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PageAnnotations {
    pub page_index: PageIndex,
    pub strokes: Vec<Stroke>,
}

/// 문서 전체의 주석을 페이지별로 보관.
///
/// 스트로크는 ID 부여 후 페이지 좌표계로 저장되며,
/// `Edit`(history)와 함께 사용하면 실행취소/다시실행이 가능합니다.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AnnotationStore {
    pub(crate) pages: BTreeMap<PageIndex, PageAnnotations>,
    next_stroke_id: u64,
}

impl AnnotationStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// 주석이 존재하는 페이지 수.
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// 모든 페이지의 주석을 반복합니다.
    pub fn pages(&self) -> impl Iterator<Item = &PageAnnotations> {
        self.pages.values()
    }

    /// 페이지의 스트로크 목록 (없으면 빈 슬라이스).
    pub fn strokes_on(&self, page_index: PageIndex) -> &[Stroke] {
        self.pages
            .get(&page_index)
            .map(|p| p.strokes.as_slice())
            .unwrap_or(&[])
    }

    /// 페이지의 스트로크 개수.
    pub fn stroke_count_on(&self, page_index: PageIndex) -> usize {
        self.strokes_on(page_index).len()
    }

    /// 전체 스트로크 개수.
    pub fn total_stroke_count(&self) -> usize {
        self.pages.values().map(|p| p.strokes.len()).sum()
    }

    /// 스트로크 ID 조회.
    pub fn stroke(&self, page_index: PageIndex, stroke_id: u64) -> Option<&Stroke> {
        self.strokes_on(page_index).iter().find(|s| s.id == stroke_id)
    }

    /// 새 스트로크 추가. 고유 ID를 부여해 반환합니다.
    pub fn add_stroke(
        &mut self,
        page_index: PageIndex,
        tool: ToolType,
        color: [u8; 4],
        width: f32,
        points: Vec<StrokePoint>,
    ) -> u64 {
        let id = self.next_stroke_id;
        self.next_stroke_id += 1;
        let stroke = Stroke {
            id,
            tool,
            color,
            width,
            points,
        };
        self.ensure_page(page_index).strokes.push(stroke);
        id
    }

    /// 이미 ID가 부여된 스트로크들을 추가 (undo/redo 재적용용).
    pub fn add_strokes(&mut self, page_index: PageIndex, strokes: Vec<Stroke>) {
        let max_id = strokes.iter().map(|s| s.id).max().map(|v| v + 1).unwrap_or(0);
        self.next_stroke_id = self.next_stroke_id.max(max_id);
        let page = self.ensure_page(page_index);
        page.strokes.extend(strokes);
    }

    /// 페이지에서 스트로크 하나 제거.
    pub fn remove_stroke(&mut self, page_index: PageIndex, stroke_id: u64) -> Option<Stroke> {
        let page = self.pages.get_mut(&page_index)?;
        let index = page.strokes.iter().position(|s| s.id == stroke_id)?;
        Some(page.strokes.remove(index))
    }

    /// 페이지에서 여러 스트로크를 일괄 제거하고, 제거된 목록을 반환합니다.
    pub fn remove_strokes(&mut self, page_index: PageIndex, ids: &[u64]) -> Vec<Stroke> {
        let mut removed = Vec::new();
        if let Some(page) = self.pages.get_mut(&page_index) {
            page.strokes.retain(|s| {
                if ids.contains(&s.id) {
                    removed.push(s.clone());
                    false
                } else {
                    true
                }
            });
        }
        removed
    }

    /// 페이지의 모든 스트로크 제거. 제거된 목록을 반환합니다.
    pub fn clear_page(&mut self, page_index: PageIndex) -> Vec<Stroke> {
        self.pages
            .remove(&page_index)
            .map(|p| p.strokes)
            .unwrap_or_default()
    }

    /// `point`(페이지 좌표) 반경 `radius` 안에 닿는 스트로크 ID 목록.
    pub fn hit_test(&self, page_index: PageIndex, point: [f32; 2], radius: f32) -> Vec<u64> {
        self.strokes_on(page_index)
            .iter()
            .filter(|s| s.any_point_within(point, radius))
            .map(|s| s.id)
            .collect()
    }

    /// 지우개 동작: `point` 반경 `radius` 안의 스트로크를 모두 제거.
    pub fn erase_at(&mut self, page_index: PageIndex, point: [f32; 2], radius: f32) -> Vec<Stroke> {
        let ids = self.hit_test(page_index, point, radius);
        self.remove_strokes(page_index, &ids)
    }

    /// `Edit`(history)를 현재 저장소에 적용합니다.
    pub fn apply_edit(&mut self, edit: &Edit) {
        match edit {
            Edit::AddStrokes { page, strokes } => {
                self.add_strokes(*page, strokes.clone());
            }
            Edit::RemoveStrokes { page, strokes } => {
                let ids: Vec<u64> = strokes.iter().map(|s| s.id).collect();
                self.remove_strokes(*page, &ids);
            }
        }
    }

    /// `Edit`의 역연산을 적용합니다 (undo).
    pub fn apply_inverse(&mut self, edit: &Edit) {
        self.apply_edit(&edit.inverse());
    }

    /// JSON 직렬화.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("AnnotationStore serialization failed")
    }

    /// JSON 역직렬화.
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }

    pub(crate) fn ensure_page(&mut self, page_index: PageIndex) -> &mut PageAnnotations {
        self.pages
            .entry(page_index)
            .or_insert_with(|| PageAnnotations {
                page_index,
                strokes: Vec::new(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_points() -> Vec<StrokePoint> {
        vec![
            StrokePoint::new(0.0, 0.0, 0.5),
            StrokePoint::new(100.0, 50.0, 0.8),
        ]
    }

    #[test]
    fn add_stroke_gives_unique_increasing_ids() {
        let mut store = AnnotationStore::new();
        let a = store.add_stroke(0, ToolType::Pen, [0, 0, 0, 255], 2.0, sample_points());
        let b = store.add_stroke(0, ToolType::Highlighter, [255, 255, 0, 90], 14.0, sample_points());
        assert_eq!(a, 0);
        assert_eq!(b, 1);
        assert_eq!(store.stroke_count_on(0), 2);
        assert_eq!(store.total_stroke_count(), 2);
    }

    #[test]
    fn strokes_are_per_page() {
        let mut store = AnnotationStore::new();
        store.add_stroke(0, ToolType::Pen, [0, 0, 0, 255], 2.0, sample_points());
        store.add_stroke(2, ToolType::Pen, [0, 0, 0, 255], 2.0, sample_points());
        assert_eq!(store.stroke_count_on(0), 1);
        assert_eq!(store.stroke_count_on(1), 0);
        assert_eq!(store.stroke_count_on(2), 1);
        assert_eq!(store.page_count(), 2);
    }

    #[test]
    fn remove_stroke_returns_and_removes() {
        let mut store = AnnotationStore::new();
        let id = store.add_stroke(0, ToolType::Pen, [0, 0, 0, 255], 2.0, sample_points());
        let removed = store.remove_stroke(0, id);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().id, id);
        assert_eq!(store.stroke_count_on(0), 0);
        assert!(store.remove_stroke(0, 999).is_none());
    }

    #[test]
    fn erase_at_removes_intersecting_only() {
        let mut store = AnnotationStore::new();
        let near = store.add_stroke(
            0,
            ToolType::Pen,
            [0, 0, 0, 255],
            2.0,
            vec![StrokePoint::new(10.0, 10.0, 0.5)],
        );
        let far = store.add_stroke(
            0,
            ToolType::Pen,
            [0, 0, 0, 255],
            2.0,
            vec![StrokePoint::new(500.0, 500.0, 0.5)],
        );
        let removed = store.erase_at(0, [10.0, 10.0], 20.0);
        let removed_ids: Vec<u64> = removed.iter().map(|s| s.id).collect();
        assert_eq!(removed_ids, vec![near]);
        assert!(store.stroke(0, near).is_none());
        assert!(store.stroke(0, far).is_some());
    }

    #[test]
    fn erase_respects_radius() {
        let mut store = AnnotationStore::new();
        let id = store.add_stroke(
            0,
            ToolType::Pen,
            [0, 0, 0, 255],
            2.0,
            vec![StrokePoint::new(10.0, 10.0, 0.5)],
        );
        // 반지름이 닿지 않으면 지워지지 않음
        let removed = store.erase_at(0, [11.0, 10.0], 0.5);
        assert!(removed.is_empty());
        assert_eq!(store.stroke_count_on(0), 1);
        // 정확히 점 위를 지우면 반지름이 매우 작아도 지워짐 (정확한 히트)
        let removed = store.erase_at(0, [10.0, 10.0], 0.001);
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].id, id);
        assert_eq!(store.stroke_count_on(0), 0);
    }

    #[test]
    fn clear_page_removes_all() {
        let mut store = AnnotationStore::new();
        store.add_stroke(0, ToolType::Pen, [0, 0, 0, 255], 2.0, sample_points());
        store.add_stroke(0, ToolType::Pen, [0, 0, 0, 255], 2.0, sample_points());
        let cleared = store.clear_page(0);
        assert_eq!(cleared.len(), 2);
        assert_eq!(store.stroke_count_on(0), 0);
    }

    #[test]
    fn json_round_trip_preserves_data() {
        let mut store = AnnotationStore::new();
        store.add_stroke(0, ToolType::Pen, [10, 20, 30, 255], 2.0, sample_points());
        store.add_stroke(1, ToolType::Highlighter, [200, 100, 0, 90], 12.0, sample_points());
        let json = store.to_json();
        let restored = AnnotationStore::from_json(&json).expect("JSON 파싱 실패");
        assert_eq!(restored, store);
        assert_eq!(restored.strokes_on(0)[0].color, [10, 20, 30, 255]);
        assert_eq!(restored.strokes_on(1)[0].tool, ToolType::Highlighter);
    }

    #[test]
    fn json_parse_error_is_reported() {
        assert!(AnnotationStore::from_json("not json").is_err());
    }

    #[test]
    fn ids_stay_monotonic_after_remove_and_add() {
        let mut store = AnnotationStore::new();
        let id = store.add_stroke(0, ToolType::Pen, [0, 0, 0, 255], 2.0, sample_points());
        store.remove_stroke(0, id);
        let next = store.add_stroke(0, ToolType::Pen, [0, 0, 0, 255], 2.0, sample_points());
        assert!(next > id, "ID는 단조 증가해야 함");
    }

    #[test]
    fn apply_edit_add_and_remove() {
        let mut store = AnnotationStore::new();
        let id = store.add_stroke(0, ToolType::Pen, [0, 0, 0, 255], 2.0, sample_points());
        let stroke = store.stroke(0, id).cloned().unwrap();

        let edit = Edit::RemoveStrokes {
            page: 0,
            strokes: vec![stroke.clone()],
        };
        store.apply_edit(&edit);
        assert_eq!(store.stroke_count_on(0), 0);

        store.apply_inverse(&edit);
        assert_eq!(store.stroke_count_on(0), 1);
        assert_eq!(store.stroke(0, id), Some(&stroke));
    }
}
