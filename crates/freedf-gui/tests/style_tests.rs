//! `style` 모듈 테스트 — `src/style.rs`의 `#[cfg(test)]` 모듈에서 이동했습니다.
//!
//! 하이브리드 규칙을 강제한다: **클래스 이름은 BEM**, 태그는 의미(하위 셀렉터)로만
//! 쓴다. 태그 단독 셀렉터(`Button { … }`)는 금지 — 스타일은 항상 클래스가 소유한다.

use elm_magic::style::{lookup_class, selectors};

/// 등록된 CSS 셀렉터 목록.
///
/// `css!` 규칙은 `.init_array` 생성자로 등록되므로 **이 바이너리가 `style`
/// 모듈을 링크해야** 목록이 채워진다 (테스트 바이너리는 셸을 안 쓰므로
/// 아무것도 참조하지 않으면 통째로 빠진다). `palette()` 호출이 그 참조다.
fn registered_selectors() -> Vec<String> {
    let _ = freedf_gui::style::palette();
    selectors()
}

/// `:hover` 같은 상태 접미사를 떼고 셀렉터 본체만.
fn body(selector: &str) -> &str {
    selector.trim().split(':').next().unwrap_or("").trim()
}

/// 셀렉터를 조각으로 나눈다 — 하위 결합(`.a .b`)과 자식 결합(`.a > .b`).
fn parts(selector: &str) -> Vec<String> {
    body(selector)
        .split(|c: char| c.is_whitespace() || c == '>')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// BEM 이름 판정 — `block`, `block__element`, `block__element--modifier`.
///
/// 소문자/숫자/하이픈만 쓰고, `__`·`--` 구분자 조각은 비어 있으면 안 된다
/// (`.panel_title` 같은 밑줄 하나짜리 이름이 조용히 섞이는 걸 막는다).
fn is_bem(name: &str) -> bool {
    let Some(name) = name.strip_prefix('.') else {
        return false;
    };
    let word = |s: &str| {
        !s.is_empty()
            && !s.starts_with('-')
            && !s.ends_with('-')
            && s.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    };
    let (rest, modifier) = match name.split_once("--") {
        Some((rest, m)) => (rest, Some(m)),
        None => (name, None),
    };
    let (block, element) = match rest.split_once("__") {
        Some((b, e)) => (b, Some(e)),
        None => (rest, None),
    };
    word(block) && element.map(word).unwrap_or(true) && modifier.map(word).unwrap_or(true)
}

/// 의미 태그인가 — 대문자로 시작하는 elm-magic 어휘 태그(`Button` `Text` `Strong` …).
fn is_semantic_tag(part: &str) -> bool {
    part.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && part.chars().all(|c| c.is_ascii_alphanumeric())
}

/// 마크업(셸 + 컴포넌트)이 쓰는 리터럴 `class="…"` 값들.
fn literal_classes(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = src;
    while let Some(i) = rest.find("class=\"") {
        rest = &rest[i + "class=\"".len()..];
        let Some(end) = rest.find('"') else { break };
        out.extend(rest[..end].split_whitespace().map(str::to_string));
        rest = &rest[end..];
    }
    out
}

/// 셀렉터 조각은 BEM 클래스이거나 의미 태그다 (태그 단독 셀렉터 금지).
#[test]
fn selectors_use_bem_classes_with_semantic_tags() {
    let all = registered_selectors();
    assert!(!all.is_empty(), "css! 규칙이 하나도 등록되지 않았다");
    for sel in &all {
        let parts = parts(sel);
        assert!(!parts.is_empty(), "빈 셀렉터: {sel}");
        assert!(
            parts.iter().any(|p| p.starts_with('.')),
            "클래스가 하나도 없는 태그 셀렉터는 금지: {sel}"
        );
        for part in &parts {
            assert!(
                is_bem(part) || is_semantic_tag(part),
                "BEM 클래스도 의미 태그도 아니다: {part} ({sel})"
            );
        }
    }
    // 판정기 자체의 회귀 방지.
    assert!(is_bem(".app") && is_bem(".panel__item") && is_bem(".tabs__item--active"));
    assert!(!is_bem(".panel_title") && !is_bem("Button") && !is_bem(".a__") && !is_bem(".a--"));
    assert!(is_semantic_tag("Button") && !is_semantic_tag("button") && !is_semantic_tag(".btn"));
}

/// 마크업의 모든 class가 CSS에 등록돼 있다 — 요소마다 자기 CSS 이름을 갖는다.
#[test]
fn shell_markup_classes_are_registered() {
    let mut src = String::from(include_str!("../src/shell.rs"));
    src.push_str(include_str!("../src/ui/atoms.rs"));
    src.push_str(include_str!("../src/ui/layout.rs"));
    let classes = literal_classes(&src);
    assert!(classes.len() > 20, "class 파싱이 이상하다: {classes:?}");
    for name in &classes {
        assert!(
            lookup_class(name).is_some(),
            "마크업이 쓰는 `{name}`이 style.rs에 등록되지 않았다"
        );
    }
    // 표현식 class(`class={if …}`)는 리터럴 스캔에 안 잡히므로 명시 검증.
    for name in [
        "btn",
        "btn--on",
        "swatch",
        "swatch--on",
        "tabs__item",
        "tabs__item--active",
    ] {
        assert!(lookup_class(name).is_some(), "동적 class `{name}` 미등록");
    }
}
