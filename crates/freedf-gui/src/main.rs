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

mod canvas;
mod dev;
mod fonts;
mod shell;

#[cfg(test)]
mod services_smoke {
    /// Phase 1 (docs/freedf-gui-migration.md) 연결 확인 — freedf-gui가
    /// freedf-services 계층을 직접 쓸 수 있다 (Phase 2+에서 캔버스/저장소가 이
    /// 경로로 붙는다).
    #[test]
    fn services_available() {
        let cfg = freedf_services::server::MediaServerConfig::default();
        let _ = cfg.normalized_base();
        let _ = freedf_services::storage::app_data_dir();
        assert_eq!(freedf_services::settings::MAX_FAVORITE_COLORS, 8);
        let _ = freedf_services::pdf::MAX_RENDER_DIM;
    }
}



use eframe::egui;

fn main() -> eframe::Result {
    // 저장된 잉크 기본값 복원 (파일 없음/손상이면 조용히 기본값 — docs: 설정 창).
    canvas::load_defaults();
    let options = eframe::NativeOptions {
        // 기본: OpenGL(glow) — freedf와 동일. wgpu(DX12)는 일부 Windows에서
        // 시작 시 0xc0000005 크래시 (워크스페이스 Cargo.toml 주석 참고).
        // `FREEDF_RENDERER=wgpu`로 실행하면 wgpu 백엔드로 시도한다.
        renderer: match std::env::var("FREEDF_RENDERER").as_deref() {
            Ok("wgpu") => eframe::Renderer::Wgpu,
            _ => eframe::Renderer::Glow,
        },
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 720.0])
            .with_title("FreeDF GUI (elm-magic)"),
        ..Default::default()
    };
    eframe::run_native(
        "freedf-gui",
        options,
        Box::new(|cc| {
            fonts::install(&cc.egui_ctx);
            Ok(Box::new(Host::default()))
        }),
    )
}

/// eframe 호스트 — elm-magic의 상태 아레나(`Ctx`)를 프레임 간에 유지한다.
///
/// 상태는 전부 `Shell` 컴포넌트의 매개변수 슬롯 안에 산다. 호스트는 슬롯을
/// 만지지 않는다(렌더 + 어댑터 전달만) — "상태는 변수, 화면은 함수 본문".
struct Host {
    elm: elm_magic::Ctx,
    /// eguidev 자동화 핸들 — `dev-automation` 기능에서만 존재 (기본 빌드 no-op).
    #[cfg(feature = "dev-automation")]
    devmcp: eguidev::DevMcp,
}

impl Default for Host {
    fn default() -> Self {
        Self {
            elm: elm_magic::Ctx::default(),
            #[cfg(feature = "dev-automation")]
            devmcp: dev::attach(),
        }
    }
}

impl eframe::App for Host {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // ── eguidev 자동화 프레임 스코프 ────────────────────────────────
        // EDEV가 시작한 실행(EGUIDEV_MCP_ADDR 주입)이 아니면 사실상 no-op.
        #[cfg(feature = "dev-automation")]
        let devmcp = self.devmcp.clone();

        #[cfg(feature = "dev-automation")]
        eguidev::frame_scope(&devmcp, ui, "freedf-gui.root", |ui| {
            shell::render_shell(ui, &mut self.elm);
        });

        #[cfg(not(feature = "dev-automation"))]
        shell::render_shell(ui, &mut self.elm);
    }
}
