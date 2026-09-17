//! `outline` 모듈 단위 테스트 — `src/outline.rs`의 `#[cfg(test)]` 모듈에서 이동했습니다.

use freedf_core::outline::*;

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
