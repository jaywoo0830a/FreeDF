//! 페이지별 주석(스트로크) 저장소와 지우개 히트 테스트, JSON 직렬화.

use crate::history::Edit;
use crate::model::{PageIndex, Stroke, StrokePoint, ToolType};
use crate::paper::PagePaper;
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
/// `paper`는 페이지별 용지 설정(그리드/색)으로, 페이지마다 독립적입니다.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AnnotationStore {
    pub(crate) pages: BTreeMap<PageIndex, PageAnnotations>,
    pub(crate) paper: BTreeMap<PageIndex, PagePaper>,
    /// 사용자 북마크 페이지 (정렬 유지).
    pub(crate) bookmarks: Vec<PageIndex>,
    next_stroke_id: u64,
    /// 스트로크 변경마다 증가하는 수정 버전 — 렌더러가 병합 잉크 메시를
    /// 다시 만들 시점을 판별하는 데 사용합니다 (내용이 같으면 재구성 없음).
    #[serde(default)]
    rev: u64,
}

impl AnnotationStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// 스트로크 데이터 수정 버전 (추가/삭제/변경마다 증가).
    pub fn rev(&self) -> u64 {
        self.rev
    }

    fn bump_rev(&mut self) {
        self.rev = self.rev.wrapping_add(1);
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

    /// 페이지의 용지 설정 (없으면 None).
    pub fn paper_on(&self, page_index: PageIndex) -> Option<PagePaper> {
        self.paper.get(&page_index).copied()
    }

    /// 페이지의 용지 설정. 없으면 `default`를 반환합니다.
    pub fn paper_on_or(&self, page_index: PageIndex, default: PagePaper) -> PagePaper {
        self.paper_on(page_index).unwrap_or(default)
    }

    /// 페이지의 용지 설정을 저장합니다.
    pub fn set_paper(&mut self, page_index: PageIndex, paper: PagePaper) {
        self.paper.insert(page_index, paper);
    }

    /// 북마크된 페이지 목록 (정렬).
    pub fn bookmarks(&self) -> &[PageIndex] {
        &self.bookmarks
    }

    /// 페이지가 북마크되어 있는지.
    pub fn is_bookmarked(&self, page_index: PageIndex) -> bool {
        self.bookmarks.contains(&page_index)
    }

    /// 페이지 북마크 토글. 추가되면 true, 해제되면 false.
    pub fn toggle_bookmark(&mut self, page_index: PageIndex) -> bool {
        if let Some(pos) = self.bookmarks.iter().position(|p| *p == page_index) {
            self.bookmarks.remove(pos);
            false
        } else {
            self.bookmarks.push(page_index);
            self.bookmarks.sort_unstable();
            true
        }
    }

    /// 모든 북마크 제거.
    pub fn clear_bookmarks(&mut self) {
        self.bookmarks.clear();
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
            created_ms: 0,
        };
        self.ensure_page(page_index).strokes.push(stroke);
        self.bump_rev();
        id
    }

    /// 이미 ID가 부여된 스트로크들을 추가 (undo/redo 재적용용).
    pub fn add_strokes(&mut self, page_index: PageIndex, strokes: Vec<Stroke>) {
        let max_id = strokes.iter().map(|s| s.id).max().map(|v| v + 1).unwrap_or(0);
        self.next_stroke_id = self.next_stroke_id.max(max_id);
        let page = self.ensure_page(page_index);
        page.strokes.extend(strokes);
        self.bump_rev();
    }

    /// DB 시퀀스에서 미리 할당받은 id로 스트로크 하나를 추가합니다.
    /// (id를 먼저 알아야 DB 행과 히스토리가 같은 id를 공유할 수 있습니다)
    pub fn add_stroke_with_id(
        &mut self,
        page_index: PageIndex,
        id: u64,
        tool: ToolType,
        color: [u8; 4],
        width: f32,
        points: Vec<StrokePoint>,
    ) {
        self.next_stroke_id = self.next_stroke_id.max(id + 1);
        self.ensure_page(page_index).strokes.push(Stroke {
            id,
            tool,
            color,
            width,
            points,
            created_ms: 0,
        });
        self.bump_rev();
    }

    /// 임시 로컬 id를 DB의 최종 id로 교체합니다.
    pub fn assign_stroke_id(&mut self, page_index: PageIndex, from: u64, to: u64) {
        if from == to {
            return;
        }
        if let Some(page) = self.pages.get_mut(&page_index) {
            for s in &mut page.strokes {
                if s.id == from {
                    s.id = to;
                }
            }
        }
        self.next_stroke_id = self.next_stroke_id.max(to + 1);
        self.bump_rev();
    }

    /// 스트로크의 생성 시각(epoch ms)을 기록합니다 — 잉크 번짐(블리드)의
    /// 나이 계산에 사용됩니다.
    pub fn set_stroke_created_ms(&mut self, page_index: PageIndex, id: u64, ms: u64) -> bool {
        if let Some(page) = self.pages.get_mut(&page_index) {
            if let Some(s) = page.strokes.iter_mut().find(|s| s.id == id) {
                s.created_ms = ms;
                self.bump_rev();
                return true;
            }
        }
        false
    }

    /// 페이지별 용지 설정 전체를 순회합니다 (DB pages 테이블 동기화용).
    pub fn paper_entries(&self) -> impl Iterator<Item = (PageIndex, PagePaper)> + '_ {
        self.paper.iter().map(|(k, v)| (*k, *v))
    }

    /// 페이지에서 스트로크 하나 제거.
    pub fn remove_stroke(&mut self, page_index: PageIndex, stroke_id: u64) -> Option<Stroke> {
        let page = self.pages.get_mut(&page_index)?;
        let index = page.strokes.iter().position(|s| s.id == stroke_id)?;
        let removed = page.strokes.remove(index);
        self.bump_rev();
        Some(removed)
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
        if !removed.is_empty() {
            self.bump_rev();
        }
        removed
    }

    /// 페이지의 모든 스트로크 제거. 제거된 목록을 반환합니다.
    pub fn clear_page(&mut self, page_index: PageIndex) -> Vec<Stroke> {
        let removed = self
            .pages
            .remove(&page_index)
            .map(|p| p.strokes)
            .unwrap_or_default();
        if !removed.is_empty() {
            self.bump_rev();
        }
        removed
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

    /// 페이지의 모든 스트로크 점을 시계/반시계 90° 회전합니다.
    /// (페이지 표시 방향이 바뀌므로 주석도 같은 회전을 적용해 화면 위치를 유지)
    ///
    /// 변환: 페이지 좌표계(원점 좌상단, y 아래로 증가)에서
    /// - 시계방향: `(x, y) → (height - y, x)`
    /// - 반시계: `(x, y) → (y, width - x)`
    pub fn rotate_strokes_on(
        &mut self,
        page_index: PageIndex,
        width: f32,
        height: f32,
        clockwise: bool,
    ) {
        if let Some(page) = self.pages.get_mut(&page_index) {
            for stroke in &mut page.strokes {
                for p in &mut stroke.points {
                    let (nx, ny) = if clockwise {
                        (height - p.y, p.x)
                    } else {
                        (p.y, width - p.x)
                    };
                    p.x = nx;
                    p.y = ny;
                }
            }
            self.bump_rev();
        }
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
    ///
    /// 실패 원본은 `serde_json::Error` 그대로이며, 크레이트 경계에서
    /// [`crate::error::Error::Json`]로 수렴됩니다.
    pub fn from_json(s: &str) -> crate::error::Result<Self> {
        Ok(serde_json::from_str(s)?)
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
