//! eguidev GUI 자동화 계측 — `dev-automation` 기능이 켜졌을 때만 실제로 동작합니다.
//!
//! FreeDF는 `egui 0.36`을 쓰는데 공식 `eguidev` 크레이트는 아직 `egui 0.35`까지만
//! 발행되어 있습니다. 그래서 `crates/freedf/Cargo.toml`은 리비전을 고정한 포크
//! (`jaywoo0830a/eguidev` 의 `freedf` 브랜치)를 씁니다 — 그 포크는 저자 로컬
//! 경로 의존성을 공개 리비전으로 바꾸고 비공개 `ruau-script-api` 의존성을 걷어낸
//! 것뿐이라, 이 앱이 기대하는 API는 upstream과 동일합니다.
//!
//! 이 모듈은 그 위에 얇은 헬퍼만 얹습니다. 전부 **feature 게이트된 no-op 분기**를
//! 함께 제공하므로 호출부에 `#[cfg]`를 흩뿌릴 필요가 없습니다:
//!
//! - [`attach`] — 프로세스당 한 번, 런타임 서버를 `DevMcp` 핸들에 붙입니다.
//!   `EDEV`가 `EGUIDEV_MCP_ADDR`를 주입하지 않으면 완전히 무해한(inert) 핸들이
//!   돌아오므로, 그냥 `cargo run`한 앱은 평소와 100% 동일하게 동작합니다.
//! - [`tag`] — FreeDF의 커스텀 위젯(`IconButton` 등)을 문자열 id로 등록합니다.
//!   스크립트는 이 id로 클릭/대기/단언을 합니다.
//! - [`tag_response`] — 이미 얻은 [`egui::Response`]를 id로 등록합니다.
//! - [`publish_rect`] — painter로 직접 그린 영역(페이지 캔버스)의 기하를 공개합니다.
//!
//! 자세한 사용법은 `docs/eguidev-automation.md` 참고.

use eframe::egui;

/// 자동화 런타임을 붙인 `DevMcp` 핸들을 만듭니다 (프로세스당 한 번).
///
/// `EDEV`가 `EGUIDEV_MCP_ADDR`를 주입한 실행에서만 서버/스크립트 평가가 켜집니다.
#[cfg(feature = "dev-automation")]
pub(crate) fn attach() -> eguidev::DevMcp {
    eguidev_runtime::attach(eguidev::DevMcp::new())
}

/// 이 프로세스가 EDEV 자동화용으로 시작되었는지.
///
/// 종료 확인 창처럼 **사람의 입력을 기다리는** 흐름은 자동화에서 곧바로
/// 타임아웃(강제 종료)을 만들기 때문에, 이 값으로 그런 흐름을 건너뜁니다.
#[cfg(feature = "dev-automation")]
pub(crate) fn automation_active() -> bool {
    eguidev_runtime::automation_launch()
}

/// 이 프로세스가 EDEV 자동화용으로 시작되었는지 (기능이 꺼져 있으면 항상 false).
#[cfg(not(feature = "dev-automation"))]
pub(crate) fn automation_active() -> bool {
    false
}

/// 커스텀 위젯을 그리면서 결과 `Response`를 문자열 id로 등록합니다.
///
/// `add`가 그린 위젯의 role/label/value/geometry가 매 프레임 스냅샷에 담기고,
/// 스크립트의 `click()` / `wait()` / `expect()` 대상이 됩니다.
#[cfg(feature = "dev-automation")]
pub(crate) fn tag(
    ui: &mut egui::Ui,
    id: impl Into<String>,
    add: impl FnOnce(&mut egui::Ui) -> egui::Response,
) -> egui::Response {
    eguidev::track_widget(ui, id, add)
}

/// 커스텀 위젯을 그리면서 결과 `Response`를 문자열 id로 등록합니다.
#[cfg(not(feature = "dev-automation"))]
pub(crate) fn tag(
    ui: &mut egui::Ui,
    _id: impl Into<String>,
    add: impl FnOnce(&mut egui::Ui) -> egui::Response,
) -> egui::Response {
    add(ui)
}

/// 버튼(선택 상태 없음)을 id·라벨과 함께 등록합니다.
///
/// 역할이 `button`으로 기록되므로 스크립트가 `widgets({ role = "button" })`로
/// 훑거나 라벨로 찾을 수 있습니다.
#[cfg(feature = "dev-automation")]
pub(crate) fn tag_button(
    ui: &egui::Ui,
    id: impl Into<String>,
    label: impl Into<String>,
    response: &egui::Response,
) {
    let meta = eguidev::WidgetMeta {
        role: eguidev::WidgetRoleMeta::Button { selected: None },
        label: Some(label.into()),
        ..base_meta(ui, response)
    };
    eguidev::track_response(id, response, meta);
}

/// 버튼(선택 상태 없음) 등록 — 기능이 꺼져 있으면 no-op.
#[cfg(not(feature = "dev-automation"))]
pub(crate) fn tag_button(
    _ui: &egui::Ui,
    _id: impl Into<String>,
    _label: impl Into<String>,
    _response: &egui::Response,
) {
}

/// 버튼을 **그리면서** id·라벨과 함께 등록합니다 (선택 상태 없음).
///
/// `tag` + `tag_button`을 같은 id로 연달아 부르면 한 프레임에 id가 두 번
/// 등록되어 eguidev가 중복 id 결함으로 잡습니다 — 그래서 그리기와 등록을
/// 한 번에 하는 이 헬퍼를 씁니다.
#[cfg(feature = "dev-automation")]
pub(crate) fn tag_button_with(
    ui: &mut egui::Ui,
    id: impl Into<String>,
    label: impl Into<String>,
    add: impl FnOnce(&mut egui::Ui) -> egui::Response,
) -> egui::Response {
    eguidev::track_widget_with_meta(
        ui,
        id,
        eguidev::WidgetRoleMeta::Button { selected: None },
        Some(label.into()),
        None,
        add,
    )
}

/// 버튼 그리기+등록 — 기능이 꺼져 있으면 그리기만.
#[cfg(not(feature = "dev-automation"))]
pub(crate) fn tag_button_with(
    ui: &mut egui::Ui,
    _id: impl Into<String>,
    _label: impl Into<String>,
    add: impl FnOnce(&mut egui::Ui) -> egui::Response,
) -> egui::Response {
    add(ui)
}

/// **선택 상태를 가진** 버튼(도구 선택기 등)을 등록합니다.
///
/// `selected`는 스크립트의 `expect({ selected = true })`가 보는 값이라, 호출부가
/// 알고 있는 상태를 그대로 넘겨야 합니다(스냅샷은 매 프레임 다시 기록됩니다).
#[cfg(feature = "dev-automation")]
pub(crate) fn tag_selected_button(
    ui: &egui::Ui,
    id: impl Into<String>,
    label: impl Into<String>,
    response: &egui::Response,
    selected: bool,
) {
    let meta = eguidev::WidgetMeta {
        role: eguidev::WidgetRoleMeta::Button {
            selected: Some(selected),
        },
        label: Some(label.into()),
        ..base_meta(ui, response)
    };
    eguidev::track_response(id, response, meta);
}

/// 선택 상태 버튼 등록 — 기능이 꺼져 있으면 no-op.
#[cfg(not(feature = "dev-automation"))]
pub(crate) fn tag_selected_button(
    _ui: &egui::Ui,
    _id: impl Into<String>,
    _label: impl Into<String>,
    _response: &egui::Response,
    _selected: bool,
) {
}

/// 토글(패널 표시/숨김 등)을 id·라벨·현재 값과 함께 등록합니다.
///
/// 역할 `toggle` + `value: bool`로 기록되므로 스크립트는 `selected`/`value`
/// 양쪽으로 상태를 읽을 수 있습니다.
#[cfg(feature = "dev-automation")]
pub(crate) fn tag_toggle(
    ui: &egui::Ui,
    id: impl Into<String>,
    label: impl Into<String>,
    response: &egui::Response,
    value: bool,
) {
    let meta = eguidev::WidgetMeta {
        role: eguidev::WidgetRoleMeta::Plain(eguidev::WidgetRole::Toggle),
        label: Some(label.into()),
        value: Some(eguidev::WidgetValue::Bool(value)),
        ..base_meta(ui, response)
    };
    eguidev::track_response(id, response, meta);
}

/// 토글 등록 — 기능이 꺼져 있으면 no-op.
#[cfg(not(feature = "dev-automation"))]
pub(crate) fn tag_toggle(
    _ui: &egui::Ui,
    _id: impl Into<String>,
    _label: impl Into<String>,
    _response: &egui::Response,
    _value: bool,
) {
}

/// 수동 등록 위젯의 공통 메타 — `visible`은 기본값이 false라 반드시 채웁니다.
#[cfg(feature = "dev-automation")]
fn base_meta(ui: &egui::Ui, response: &egui::Response) -> eguidev::WidgetMeta {
    eguidev::WidgetMeta {
        visible: ui.is_visible() && ui.is_rect_visible(response.rect),
        ..Default::default()
    }
}

/// [`egui::Response`]가 없는 painter 영역(예: 페이지 캔버스)을 문자열 id로 공개합니다.
#[cfg(feature = "dev-automation")]
pub(crate) fn publish_rect(ui: &mut egui::Ui, id: impl Into<String>, rect: egui::Rect) {
    eguidev::publish_rect_meta(
        ui,
        id,
        rect,
        eguidev::WidgetMeta {
            visible: ui.is_rect_visible(rect),
            ..Default::default()
        },
    );
}

/// [`egui::Response`]가 없는 painter 영역(예: 페이지 캔버스)을 문자열 id로 공개합니다.
#[cfg(not(feature = "dev-automation"))]
pub(crate) fn publish_rect(_ui: &mut egui::Ui, _id: impl Into<String>, _rect: egui::Rect) {}
