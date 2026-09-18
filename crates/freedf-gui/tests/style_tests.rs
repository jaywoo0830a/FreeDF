//! `style` 모듈 테스트 — `src/style.rs`의 `#[cfg(test)]` 모듈에서 이동했습니다.

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

/// `shell.rs`의 리터럴 `class="…"` 값들 (표현식 class는 별도 검증).
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

/// 태그 셀렉터 금지 — 스타일은 클래스로만 붙인다 (셸 전체가 BEM).
#[test]
fn no_tag_selectors_registered() {
    let all = registered_selectors();
    assert!(!all.is_empty(), "css! 규칙이 하나도 등록되지 않았다");
    for sel in &all {
        assert!(
            body(sel).starts_with('.'),
            "태그 셀렉터는 금지 — 클래스(BEM)만 쓴다: {sel}"
        );
    }
}

/// 등록된 셀렉터는 전부 BEM 이름이다.
#[test]
fn selectors_are_bem() {
    for sel in &registered_selectors() {
        let base = body(sel);
        assert!(is_bem(base), "BEM 이름이 아니다: {sel}");
    }
    // 판정기 자체의 회귀 방지.
    assert!(is_bem(".app") && is_bem(".panel__item") && is_bem(".tabs__item--active"));
    assert!(!is_bem(".panel_title") && !is_bem("Button") && !is_bem(".a__") && !is_bem(".a--"));
}

/// 마크업의 모든 class가 CSS에 등록돼 있다 — 요소마다 자기 CSS 이름을 갖는다.
#[test]
fn shell_markup_classes_are_registered() {
    let classes = literal_classes(include_str!("../src/shell.rs"));
    assert!(classes.len() > 30, "class 파싱이 이상하다: {classes:?}");
    for name in &classes {
        assert!(
            lookup_class(name).is_some(),
            "shell.rs가 쓰는 `{name}`이 style.rs에 등록되지 않았다"
        );
    }
    // 표현식 class(`class={if …}`)는 리터럴 스캔에 안 잡히므로 명시 검증.
    for name in [
        "tabs__item",
        "tabs__item--active",
        "modal__preset",
        "modal__preset--active",
    ] {
        assert!(lookup_class(name).is_some(), "동적 class `{name}` 미등록");
    }
}
