//! React식 선언형 액션 바 — `<ActionBar actions={[…]} onHit={…}/>`.
//!
//! 반복되는 "아이콘+라벨 버튼/토글/라디오를 한 줄에" 패턴을 **스펙 목록**으로 선언하고,
//! 렌더/클릭 판정은 원시 프리미티브(`icon_button`/`icon_toggle`/`icon_select`) 위의
//! 이 팩토리가 담당합니다. 도메인 계층은 반환된 `Option<usize>`만 보고 분기합니다.
//!
//! ```ignore
//! let undo = 0; let redo = 1;
//! match ActionBar::new()
//!     .add(undo, icons::ARROW_CLOCKWISE, "Undo").hint("Undo (Ctrl+Z)")
//!     .add(redo, icons::ARROW_CLOCKWISE, "Redo").hint("Redo (Ctrl+Y)")
//!     .show(ui)
//! {
//!     Some(undo) => self.undo(),
//!     Some(redo) => self.redo(),
//!     _ => (),
//! };
//! ```
//!
//! 설계 원칙 "하나의 액션 = 한 곳": 각 `id`는 액션의 **단일 홈**이며 툴바에서 중복
//! 등장을 허용하지 않습니다(자주 쓰는 액션만 팔레트/단축키로 2중). `Toggle`/`Select`는
//! React의 제어 컴포넌트처럼 **상태에 바인딩**되며, 클릭 시 상태 갱신은 egui가 합니다.

use super::*;
use egui_phosphor_icons::Icon;

/// 액션 한 칸의 종류 — 렌더링 방식을 결정합니다.
enum Kind<'a> {
    /// 평범한 액션 버튼 (`ui::icon_button`).
    Button,
    /// 상태에 바인딩된 토글 (`ui::icon_toggle`) — 값은 egui가 직접 갱신.
    Toggle(&'a mut bool),
    /// 라디오 선택 (`ui::icon_select`) — `selected`는 이벤트 시점 스냅샷.
    Select(bool),
}

/// [`ActionBar`]에 넣을 버튼 한 칸.
struct Action<'a> {
    id: usize,
    kind: Kind<'a>,
    icon: Icon,
    label: &'a str,
    hint: &'a str,
    enabled: bool,
    frame: bool,
}

/// [`Action`] 배열의 선언형 컨테이너 — `new()` → `add*(..)…` → `show(ui)`.
pub(crate) struct ActionBar<'a> {
    items: Vec<Action<'a>>,
}

impl<'a> ActionBar<'a> {
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// 평범한 액션 버튼 한 칸을 추가합니다(`id`는 호출자가 부여한 고유 토큰).
    pub fn add(mut self, id: usize, icon: Icon, label: &'a str) -> Self {
        self.items.push(Action {
            id,
            kind: Kind::Button,
            icon,
            label,
            hint: "",
            enabled: true,
            frame: true,
        });
        self
    }

    /// 상태에 바인딩된 토글 한 칸을 추가합니다(값은 egui가 직접 갱신).
    pub fn add_toggle(
        mut self,
        id: usize,
        icon: Icon,
        label: &'a str,
        on: &'a mut bool,
    ) -> Self {
        self.items.push(Action {
            id,
            kind: Kind::Toggle(on),
            icon,
            label,
            hint: "",
            enabled: true,
            frame: true,
        });
        self
    }

    /// 라디오 선택 한 칸을 추가합니다(배경 선택 하이라이트).
    pub fn add_select(
        mut self,
        id: usize,
        icon: Icon,
        label: &'a str,
        selected: bool,
    ) -> Self {
        self.items.push(Action {
            id,
            kind: Kind::Select(selected),
            icon,
            label,
            hint: "",
            enabled: true,
            frame: true,
        });
        self
    }

    pub fn hint(mut self, hint: &'a str) -> Self {
        let top = self.items.len() - 1;
        self.items[top].hint = hint;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        let top = self.items.len() - 1;
        self.items[top].enabled = enabled;
        self
    }

    #[allow(dead_code)] // 자주 쓰는 액션의 팔레트 2중 배치에서 사용.
    pub fn selected(mut self, selected: bool) -> Self {
        let top = self.items.len() - 1;
        self.items[top].kind = Kind::Select(selected);
        self
    }

    #[allow(dead_code)] // 프레임 없는 텍스트/팔레트 스타일 버튼에서 사용.
    pub fn frame(mut self, frame: bool) -> Self {
        let top = self.items.len() - 1;
        self.items[top].frame = frame;
        self
    }

    /// 스펙을 한 줄로 렌더하고, 클릭된 액션의 `id`를 반환합니다(없으면 None).
    pub fn show(self, ui: &mut egui::Ui) -> Option<usize> {
        for it in self.items {
            let resp = match it.kind {
                Kind::Button => icon_button(
                    ui,
                    IconButton::new(it.icon, it.label)
                        .hint(it.hint)
                        .enabled(it.enabled)
                        .frame(it.frame),
                ),
                Kind::Toggle(on) => icon_toggle(ui, on, it.icon, it.label, it.hint),
                Kind::Select(sel) => icon_select(ui, sel, it.icon, it.label, it.hint),
            };
            if resp.clicked() {
                return Some(it.id);
            }
        }
        None
    }
}