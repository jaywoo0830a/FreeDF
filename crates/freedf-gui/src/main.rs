//! FreeDF GUI — eframe 호스트 (얇은 바이너리).
//!
//! 화면/엔진/스타일은 전부 라이브러리(`freedf_gui`, `src/lib.rs`)가 소유한다.
//! 여기에는 eframe 진입점과 프레임 루프만 둔다:
//!
//! 1. `elm_magic::frame(&mut ctx, &ShellProps::default())` — 상태(아레나 슬롯)에서
//!    `Element` 트리를 만들고 (`shell::render_root`)
//! 2. `elm_magic_egui::render_with_palette(...)` — 어댑터가 egui 위젯으로 그려서
//!    클릭/입력을 아레나로 되돌린다.
//!
//! 캔버스(PDF/잉크)는 어댑터 어휘 밖이라 셸 안의 `<Raw>`가 `canvas::paint`로
//! 직접 그린다 (엔진은 `canvas` 모듈이 소유).

use eframe::egui;
use freedf_gui::{canvas, fonts, shell, style};

#[cfg(feature = "dev-automation")]
use freedf_gui::dev;

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
            // 스타일은 전부 elm-magic 0.6 CSS 속성(`src/style.rs`의 `css!` 규칙 +
            // 팔레트 토큰)이다. 설치할 egui Style/Visuals는 **없다** — freedf-theme
            // 의존 0. 다만 elm-magic CSS가 닿지 않는 egui 네이티브 위젯(`<Raw>`
            // 캔버스, `<Input>`, 창 크롬)의 최소 설정만 여기서 넣는다.
            style::install_egui_visuals(&cc.egui_ctx);
            // DPI 디버그 — `FREEDF_GUI_PPP=1.5` 처럼 지정해 Windows 배율을 흉내 낸다.
            if let Ok(ppp) = std::env::var("FREEDF_GUI_PPP") {
                if let Ok(v) = ppp.parse::<f32>() {
                    if v > 0.0 {
                        cc.egui_ctx.set_pixels_per_point(v);
                    }
                }
            }
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
    /// 창 클리어 색 — eframe 기본값은 반투명 근사 검정 `(12,12,12,α180)`이라
    /// elm-magic CSS가 덮지 않는 영역(창 여백)이 검정으로 보인다. CSS 팔레트의
    /// `background` 토큰(루트 `.app`의 `bg`와 같은 값)을 불투명하게 돌려준다.
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        style::clear_color()
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // ── eguidev 자동화 프레임 스코프 ────────────────────────────────
        // EDEV가 시작한 실행(EGUIDEV_MCP_ADDR 주입)이 아니면 사실상 no-op.
        #[cfg(feature = "dev-automation")]
        let devmcp = self.devmcp.clone();

        // 루트 프레임(배경 = CSS `background` 토큰 + 방어 여백)은
        // `shell::render_root` — 배경을 명시적으로 칠해야 창 클리어 색이
        // 드러나지 않는다. 테스트가 같은 경로를 검증한다.
        let body = |ui: &mut egui::Ui, elm: &mut elm_magic::Ctx| {
            shell::render_root(ui, elm);
        };

        #[cfg(feature = "dev-automation")]
        eguidev::frame_scope(&devmcp, ui, "freedf-gui.root", |ui| {
            body(ui, &mut self.elm);
        });

        #[cfg(not(feature = "dev-automation"))]
        body(ui, &mut self.elm);
    }
}
