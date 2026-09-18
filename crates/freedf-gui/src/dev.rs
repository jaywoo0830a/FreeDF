//! eguidev GUI 자동화 계측 — freedf `app/dev.rs`와 동일한 얇은 헬퍼 패턴.
//!
//! `dev-automation` 기능이 켜진 빌드에서만 실제로 동작하고, 그 외에는 전부
//! no-op입니다 (호출부에 `#[cfg]`를 흩뿌리지 않기 위한 통로):
//!
//! - [`attach`] — `EGUIDEV_MCP_ADDR`이 주입된 실행에서만 자동화 서버가 켜진다.
//! - [`tag_button`] — 어댑터(`elm_magic_egui`)가 그린 버튼을 계약 id로 등록.
//! - [`publish_rect`] — painter 영역(캔버스)의 기하를 공개.
//!
//! freedf-gui의 계약 id 규칙: 루트 프레임 스코프는 `freedf-gui.root`,
//! 어댑터 버튼은 `gui.<라벨 슬러그>` (같은 라벨이 한 프레임에 두 번 이상
//! 나오면 `.<n>` 접미사 — eguidev 중복 id 결함 방지), 캔버스는 `canvas.surface`.

use eframe::egui;

/// 자동화 런타임을 붙인 `DevMcp` 핸들 (프로세스당 한 번).
///
/// `pub`인 이유: 바이너리(`src/main.rs`)는 **다른 크레이트**라 `pub(crate)`가
/// 보이지 않는다 (실측 E0603 — `dev-automation` 빌드가 통째로 깨졌다).
#[cfg(feature = "dev-automation")]
pub fn attach() -> eguidev::DevMcp {
    eguidev_runtime::attach(eguidev::DevMcp::new())
}

/// 이 프로세스가 EDEV 자동화용으로 시작되었는지 (기능이 꺼져 있으면 항상 false).
///
/// 아직 freedf-gui에는 자동화 중 생략해야 할 흐름(종료 확인 등)이 없어 미사용 —
/// freedf `app/dev.rs`와 동일한 헬퍼 세트를 유지하기 위해 둔다.
#[cfg(feature = "dev-automation")]
#[allow(dead_code)]
pub(crate) fn automation_active() -> bool {
    eguidev_runtime::automation_launch()
}

/// 이 프로세스가 EDEV 자동화용으로 시작되었는지 (기능이 꺼져 있으면 항상 false).
#[cfg(not(feature = "dev-automation"))]
#[allow(dead_code)] // freedf `app/dev.rs`와 동일한 헬퍼 세트 — 사용처가 생길 때까지 예약.
pub(crate) fn automation_active() -> bool {
    false
}

/// 어댑터가 그린 버튼을 id·라벨과 함께 등록 (역할 `button`).
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

#[cfg(not(feature = "dev-automation"))]
pub(crate) fn tag_button(
    _ui: &egui::Ui,
    _id: impl Into<String>,
    _label: impl Into<String>,
    _response: &egui::Response,
) {
}

/// [`egui::Response`]가 없는 painter 영역(캔버스)을 문자열 id로 공개합니다.
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

#[cfg(not(feature = "dev-automation"))]
pub(crate) fn publish_rect(_ui: &mut egui::Ui, _id: impl Into<String>, _rect: egui::Rect) {}

/// 수동 등록 위젯의 공통 메타 — `visible`은 기본값이 false라 반드시 채웁니다.
#[cfg(feature = "dev-automation")]
fn base_meta(ui: &egui::Ui, response: &egui::Response) -> eguidev::WidgetMeta {
    eguidev::WidgetMeta {
        visible: ui.is_visible() && ui.is_rect_visible(response.rect),
        ..Default::default()
    }
}
