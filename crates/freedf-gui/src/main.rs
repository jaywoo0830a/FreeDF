//! FreeDF GUI — elm-magic으로 처음부터 다시 만드는 UI 셸.
//!
//! 이 크레이트는 **마이그레이션이 아니라 재작성** 실험입니다. 기존 `freedf`
//! 크레이트의 egui 명령형 UI와 달리, 화면은 전부 [`elm_magic::view!`] 컴포넌트
//! (`shell::Shell`)로 선언하고, eframe 호스트는 매 프레임:
//!
//! 1. `elm_magic::frame(&mut ctx, &ShellProps::default())` — 상태(아레나 슬롯)에서
//!    `Element` 트리를 만들고
//! 2. `elm_magic_egui::render(ui, &tree, &mut ctx.arena)` — 어댑터가 egui 위젯으로
//!    그려서 클릭/입력을 아레나로 되돌린다.
//!
//! 캔버스(PDF/잉크)는 어댑터 어휘 밖이므로 `<Raw>` 플레이스홀더로 자리만 잡아
//! 둔다 (v0 스코프 — 앱 셸 우선).

mod shell;


use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 720.0])
            .with_title("FreeDF GUI (elm-magic)"),
        ..Default::default()
    };
    eframe::run_native(
        "freedf-gui",
        options,
        Box::new(|_cc| Ok(Box::new(Host::default()))),
    )
}

/// eframe 호스트 — elm-magic의 상태 아레나(`Ctx`)를 프레임 간에 유지한다.
///
/// 상태는 전부 `Shell` 컴포넌트의 매개변수 슬롯 안에 산다. 호스트는 슬롯을
/// 만지지 않는다(렌더 + 어댑터 전달만) — "상태는 변수, 화면은 함수 본문".
struct Host {
    elm: elm_magic::Ctx,
}

impl Default for Host {
    fn default() -> Self {
        Self { elm: elm_magic::Ctx::default() }
    }
}

impl eframe::App for Host {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        shell::render_shell(ui, &mut self.elm);
    }
}
