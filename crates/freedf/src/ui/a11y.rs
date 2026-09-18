//! 접근성 계약 — 모든 인터랙티브 컴포넌트가 통과해야 하는 **단일 관문**.
//!
//! 여기서 강제하는 것:
//! 1. **접근성 이름**: 아이콘만 있는 버튼도 이름을 가져야 합니다
//!    (예전에는 `IconButton::new(icons::GEAR, "")`처럼 빈 라벨이 있었습니다).
//! 2. **최소 타깃 크기**: [`crate::ui::tokens::target::MIN`] 미만이면 위반으로 기록.
//! 3. **계측 id 내장**: 컴포넌트가 스스로 `dev`에 등록합니다 — 호출부가
//!    `dev::tag_*`를 따로 부르다 빠뜨리는 일(→ More 오버레이가 통째로 미계측이던
//!    사고)을 구조적으로 막습니다.
//!
//! 위반은 [`issues`]에 모입니다. 그래서 테스트는 GUI를 띄우지 않고도
//! (`cargo test`의 헤드리스 egui 렌더) 컴포넌트 계약을 검증할 수 있습니다 —
//! 이것이 이 시스템의 **테스트 훅**입니다.

use std::sync::Mutex;

use eframe::egui;

use crate::ui::tokens;

/// 컴포넌트의 접근성 역할 — 계측(eguidev) 역할 문자열로도 쓰입니다.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    /// 눌러서 실행되는 버튼.
    Button,
    /// 켜고 끄는 값 (value 있음).
    Toggle,
    /// 라디오/세그먼트 중 선택된 항목 (selected 있음).
    Radio,
    /// 메뉴 항목 (행 전체가 타깃).
    MenuItem,
    /// 읽기 전용 텍스트.
    Text,
}

impl Role {
    /// eguidev가 이해하는 역할 문자열.
    pub fn report(self) -> &'static str {
        match self {
            Self::Button | Self::MenuItem | Self::Radio => "button",
            Self::Toggle => "toggle",
            Self::Text => "label",
        }
    }
}

/// 컴포넌트 한 개의 계약서(props). 컴포넌트는 이걸 만들어 [`finish`]에 넘깁니다.
#[derive(Clone, Copy)]
pub struct Spec<'a> {
    /// 테스트 훅 — 이 id로 자동화 스크립트가 조작합니다. `None`이면 계측하지 않습니다.
    pub id: Option<&'a str>,
    /// 접근성 이름(필수). 스크린리더/자동화가 읽는 이름입니다.
    pub name: &'a str,
    /// 사용자에게 보여 줄 설명(툴팁).
    pub hint: &'a str,
    pub role: Role,
    /// 이 컴포넌트가 보장해야 하는 최소 타깃 크기(pt).
    pub min_target: f32,
    /// 라디오/메뉴 항목의 선택 상태.
    pub selected: bool,
    /// 토글의 값.
    pub value: Option<bool>,
}

impl<'a> Spec<'a> {
    /// 읽기 전용 텍스트 — 계측하지 않음(장식용).
    pub fn text(name: &'a str) -> Self {
        Self {
            id: None,
            name,
            hint: "",
            role: Role::Text,
            min_target: 0.0,
            selected: false,
            value: None,
        }
    }

    /// 계측 id가 있는 버튼.
    pub fn button(id: &'a str, name: &'a str) -> Self {
        Self {
            id: Some(id),
            name,
            hint: "",
            role: Role::Button,
            min_target: tokens::target::MIN,
            selected: false,
            value: None,
        }
    }

    /// 계측 id가 있는 토글 (현재 값 포함).
    pub fn toggle(id: &'a str, name: &'a str, value: bool) -> Self {
        Self {
            id: Some(id),
            name,
            hint: "",
            role: Role::Toggle,
            min_target: tokens::target::MIN,
            selected: false,
            value: Some(value),
        }
    }

    /// 계측 id가 있는 라디오/세그먼트 항목.
    pub fn radio(id: &'a str, name: &'a str, selected: bool) -> Self {
        Self {
            id: Some(id),
            name,
            hint: "",
            role: Role::Radio,
            min_target: tokens::target::MIN,
            selected,
            value: None,
        }
    }

    /// 계측 id가 있는 읽기 전용 라벨(섹션 제목·화면 제목 등).
    ///
    /// 버튼이 아니므로 최소 타깃은 요구하지 않습니다 — 자동화가 "그 화면에 어떤
    /// 섹션이 있는가"를 읽는 용도입니다.
    pub fn label(id: &'a str, name: &'a str) -> Self {
        Self {
            id: Some(id),
            name,
            hint: "",
            role: Role::Text,
            min_target: 0.0,
            selected: false,
            value: None,
        }
    }

    pub fn hint(mut self, hint: &'a str) -> Self {
        self.hint = hint;
        self
    }

    pub fn min_target(mut self, v: f32) -> Self {
        self.min_target = v;
        self
    }

    pub fn selected(mut self, on: bool) -> Self {
        self.selected = on;
        self
    }
}

/// 계약 위반 종류.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum IssueKind {
    /// 접근성 이름이 비어 있음.
    MissingName,
    /// 최소 타깃 크기 미달.
    SmallTarget,
}

impl IssueKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MissingName => "missing_name",
            Self::SmallTarget => "small_target",
        }
    }
}

/// 계약 위반 한 건.
#[derive(Clone, Debug)]
pub struct Issue {
    pub kind: IssueKind,
    /// 위반한 컴포넌트(계측 id 또는 접근성 이름).
    pub subject: String,
    pub detail: String,
}

static ISSUES: Mutex<Vec<Issue>> = Mutex::new(Vec::new());

/// 계약 테스트는 **전역 레지스트리를 공유**하므로 직렬화가 필요합니다.
/// (`cargo test`는 테스트를 병렬로 돌립니다 — 이 가드 없이는 서로의 위반을 봅니다.)
#[cfg(test)]
static TEST_GUARD: Mutex<()> = Mutex::new(());

/// 계약 테스트용 직렬화 가드. 각 테스트 첫 줄에서 잡으세요.
#[cfg(test)]
pub(crate) fn test_guard() -> std::sync::MutexGuard<'static, ()> {
    TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner())
}

/// 지금까지 수집된 위반 목록(복사본).
pub fn issues() -> Vec<Issue> {
    ISSUES.lock().map(|v| v.clone()).unwrap_or_default()
}

/// 위반 목록을 비웁니다(테스트 시작 시).
pub fn reset_issues() {
    if let Ok(mut v) = ISSUES.lock() {
        v.clear();
    }
}

/// 위반 보고서 텍스트(로그/실패 메시지용).
pub fn report() -> String {
    let list = issues();
    if list.is_empty() {
        return "a11y: 위반 없음".to_string();
    }
    let mut s = format!("a11y: {}건 위반\n", list.len());
    for i in &list {
        s.push_str(&format!(
            "  - [{}] {} — {}\n",
            i.kind.as_str(),
            i.subject,
            i.detail
        ));
    }
    s
}

/// 위반이 있으면 사람이 읽을 수 있는 보고서로 패닉합니다(테스트 훅).
pub fn assert_clean() {
    let list = issues();
    assert!(list.is_empty(), "{}", report());
}

fn push(issue: Issue) {
    let mut emit = false;
    if let Ok(mut v) = ISSUES.lock() {
        // 같은 위반이 프레임마다 쌓이지 않도록 중복을 제거합니다.
        if !v
            .iter()
            .any(|e| e.kind == issue.kind && e.subject == issue.subject)
        {
            v.push(issue);
            emit = true;
        }
    }
    // 자동화 중에는 위반을 즉시 보이게 합니다(에이전트/사람 모두 발견 가능).
    if emit && crate::app::dev::automation_active() {
        eprintln!("[a11y] {}", report().trim_end());
    }
}

/// 계약 검증 + 계측 등록. 모든 인터랙티브 컴포넌트는 이 함수를 지나갑니다.
///
/// * 시각 크기는 컴포넌트가 만들 때부터 보장합니다(여기서 강제로 넓히지 않음 —
///   넓히면 이웃 위젯의 클릭을 가로챌 수 있음). 여기서는 **검증**하고 기록합니다.
/// * `id`가 있으면 역할에 맞는 `dev` 계측을 내장 수행합니다.
pub fn finish(ui: &mut egui::Ui, spec: Spec<'_>, resp: egui::Response) -> egui::Response {
    // 1) 접근성 이름
    if spec.name.trim().is_empty() {
        push(Issue {
            kind: IssueKind::MissingName,
            subject: spec
                .id
                .map(str::to_string)
                .unwrap_or_else(|| "<id 없음>".to_string()),
            detail: "접근성 이름이 비어 있습니다 — 아이콘 전용 컴포넌트는 이름을 넘기세요"
                .to_string(),
        });
    }

    // 2) 최소 타깃
    let r = resp.rect;
    if spec.min_target > 0.0 && (r.width() < spec.min_target || r.height() < spec.min_target) {
        push(Issue {
            kind: IssueKind::SmallTarget,
            subject: spec
                .id
                .map(str::to_string)
                .unwrap_or_else(|| spec.name.to_string()),
            detail: format!(
                "타깃 {:.0}×{:.0}pt < 최소 {:.0}pt",
                r.width(),
                r.height(),
                spec.min_target
            ),
        });
    }

    // 3) 계측 내장 (테스트 훅) — 호출부가 따로 등록할 필요가 없습니다.
    match (spec.id, spec.role) {
        (Some(id), Role::Toggle) => {
            crate::app::dev::tag_toggle(ui, id, spec.name, &resp, spec.value.unwrap_or(false));
        }
        (Some(id), Role::Radio) => {
            crate::app::dev::tag_selected_button(ui, id, spec.name, &resp, spec.selected);
        }
        (Some(id), Role::Button | Role::MenuItem) => {
            crate::app::dev::tag_button(ui, id, spec.name, &resp);
        }
        (Some(id), Role::Text) => {
            crate::app::dev::tag_text(ui, id, spec.name, &resp);
        }
        _ => {}
    }

    // 4) 툴팁 부착(있을 때만)
    if spec.hint.is_empty() {
        resp
    } else {
        resp.on_hover_text(spec.hint)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_helpers_set_role_and_target() {
        let b = Spec::button("a.b", "Save");
        assert_eq!(b.role, Role::Button);
        assert_eq!(b.min_target, tokens::target::MIN);
        assert_eq!(b.role.report(), "button");

        let t = Spec::toggle("a.t", "Panel", true);
        assert_eq!(t.value, Some(true));
        assert_eq!(t.role.report(), "toggle");

        let r = Spec::radio("a.r", "Left", true);
        assert_eq!(r.role, Role::Radio);
        assert!(r.selected);

        assert_eq!(Spec::text("제목").min_target, 0.0);
        assert_eq!(Spec::button("a.b", "Save").hint("설명").hint, "설명");
    }

    #[test]
    fn report_is_human_readable() {
        let _g = test_guard();
        reset_issues();
        push(Issue {
            kind: IssueKind::SmallTarget,
            subject: "menu.x".into(),
            detail: "타깃 16×28pt < 최소 24pt".into(),
        });
        let r = report();
        assert!(r.contains("small_target"), "{r}");
        assert!(r.contains("menu.x"), "{r}");
        reset_issues();
        assert_eq!(report(), "a11y: 위반 없음");
    }

    #[test]
    fn duplicate_issues_are_collapsed() {
        let _g = test_guard();
        reset_issues();
        for _ in 0..3 {
            push(Issue {
                kind: IssueKind::MissingName,
                subject: "x".into(),
                detail: "no name".into(),
            });
        }
        let mine = issues().into_iter().filter(|i| i.subject == "x").count();
        assert_eq!(mine, 1);
        reset_issues();
    }
}
