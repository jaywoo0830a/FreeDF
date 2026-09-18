//! PDF 아웃라인(북마크) 트리 모델과 탐색.
//!
//! pdfium에서 뽑아낸 데이터를 넣으면 UI와 테스트가 사용할 수 있는
//! 순수 데이터 구조로 다룹니다.

use serde::{Deserialize, Serialize};

/// 아웃라인 트리의 한 노드.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutlineNode {
    pub title: String,
    /// 대상 페이지 인덱스 (없으면 0 이상이 아닌 값)
    pub page_index: Option<usize>,
    pub children: Vec<OutlineNode>,
}

impl OutlineNode {
    pub fn new(
        title: impl Into<String>,
        page_index: Option<usize>,
        children: Vec<OutlineNode>,
    ) -> Self {
        Self {
            title: title.into(),
            page_index,
            children,
        }
    }

    pub fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }

    /// 자기 자신 포함 하위 노드 개수.
    pub fn total_count(&self) -> usize {
        1 + self.children.iter().map(Self::total_count).sum::<usize>()
    }
}

/// 평탄화된 아웃라인 항목 (깊이 + 노드 참조).
#[derive(Debug, Clone, PartialEq)]
pub struct OutlineEntry<'a> {
    pub depth: usize,
    pub node: &'a OutlineNode,
}

/// 트리를 순회하며 (깊이, 노드) 목록을 반환합니다.
pub fn flatten(roots: &[OutlineNode]) -> Vec<OutlineEntry<'_>> {
    fn walk<'a>(node: &'a OutlineNode, depth: usize, out: &mut Vec<OutlineEntry<'a>>) {
        out.push(OutlineEntry { depth, node });
        for child in &node.children {
            walk(child, depth + 1, out);
        }
    }
    let mut out = Vec::new();
    for root in roots {
        walk(root, 0, &mut out);
    }
    out
}

/// 제목으로 노드를 찾습니다 (첫 번째 일치, 대소문자 무시).
pub fn find_by_title<'a>(roots: &'a [OutlineNode], title: &str) -> Option<&'a OutlineNode> {
    for entry in flatten(roots) {
        if entry.node.title.eq_ignore_ascii_case(title) {
            return Some(entry.node);
        }
    }
    None
}
