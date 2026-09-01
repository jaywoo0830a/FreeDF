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
    pub fn new(title: impl Into<String>, page_index: Option<usize>, children: Vec<OutlineNode>) -> Self {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> Vec<OutlineNode> {
        vec![
            OutlineNode::new(
                "Chapter 1",
                Some(0),
                vec![
                    OutlineNode::new("Section 1.1", Some(0), vec![]),
                    OutlineNode::new("Section 1.2", Some(1), vec![]),
                ],
            ),
            OutlineNode::new("Chapter 2", Some(5), vec![]),
            OutlineNode::new("No destination", None, vec![]),
        ]
    }

    #[test]
    fn flatten_preserves_order_and_depth() {
        let t = tree();
        let flat = flatten(&t);
        let titles: Vec<&str> = flat.iter().map(|e| e.node.title.as_str()).collect();
        assert_eq!(
            titles,
            vec![
                "Chapter 1",
                "Section 1.1",
                "Section 1.2",
                "Chapter 2",
                "No destination"
            ]
        );
        let depths: Vec<usize> = flat.iter().map(|e| e.depth).collect();
        assert_eq!(depths, vec![0, 1, 1, 0, 0]);
    }

    #[test]
    fn total_count_counts_all_nodes() {
        let t = tree();
        let total: usize = t.iter().map(OutlineNode::total_count).sum();
        assert_eq!(total, 5);
    }

    #[test]
    fn find_by_title_is_case_insensitive() {
        let t = tree();
        assert!(find_by_title(&t, "section 1.2").is_some());
        assert!(find_by_title(&t, "Chapter 2").is_some());
        assert!(find_by_title(&t, "missing").is_none());
    }

    #[test]
    fn serialization_round_trip() {
        let t = tree();
        let json = serde_json::to_string(&t).unwrap();
        let back: Vec<OutlineNode> = serde_json::from_str(&json).unwrap();
        assert_eq!(t, back);
    }

    #[test]
    fn leaf_and_count() {
        let leaf = OutlineNode::new("x", Some(0), vec![]);
        assert!(leaf.is_leaf());
        let parent = OutlineNode::new("p", None, vec![leaf]);
        assert!(!parent.is_leaf());
        assert_eq!(parent.total_count(), 2);
    }
}
