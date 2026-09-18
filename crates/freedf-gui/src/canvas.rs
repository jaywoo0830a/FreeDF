//! 캔버스 — `<Raw>` 경계 뒤의 명령형 egui 렌더/입력 (Phase 2~3,
//! docs/freedf-gui-migration.md).
//!
//! **탭 = 문서**. 캔버스 엔진이 문서(`Doc`: 저장소·페이지·뷰·PDF)를 소유하고,
//! 위젯 트리(elm-magic)는 탭 이름/활성만 읽어 가며 커맨드(`canvas::add_tab()` 등)
//! 로 엔진을 조작한다. 엔진 상태는 UI 스레드 스레드 로컬 — 잉크 메시/PDF 텍스처는
//! Clone 슬롯에 담을 수 없고, `<Raw>` 클로저가 호스트 상태에 접근할 방법이 없기
//! 때문이다 (egui UI는 단일 스레드라 스레드 로컬이 안전하다).
//!
//! 잉크 렌더는 freedf의 검증된 파이프라인과 같은 부품을 쓴다: **입력**은
//! `freedf-core`의 [`InkPipeline`](freedf_core::pipeline::InkPipeline)
//! (`down`/`drag`/`up` — 1€ 필터 + 인과적 선폭 확정 + 동결 커밋), **출력**은
//! `freedf-canvas`의 `halves_for_stroke` + `append_stroke_ribbon` +
//! `alphas_for_stroke` (freedf `app/canvas/paint.rs`와 동일 생성기), 페이지↔화면
//! 변환은 `ViewTransform`, 스트로크/북마크 저장은 `freedf-core::store::AnnotationStore`.
//!
//! 즉 이 파일은 두 인터페이스를 **배선**할 뿐, 잉크 물리/지오메트리를 직접
//! 계산하지 않는다 (도구별 분기도 없다 — 재료는 `Materials::for_tool`이 결정).
//!
//! v1 한계: 펜 압력은 egui 포인터 이벤트에 없어(마우스/단순 펜) 명목 1.0 —
//! 실제 필압 어댑터는 Phase 4(freedf `app/input` 이식)에서 붙는다. 지우개는
//! 아직 세션이 없다(`Materials::for_tool`이 재료를 주지 않는다) — v1에서는
//! 잉크를 남기지 않는다. 프레임마다 메시 재굽기(획 수가 커지면 freedf의
//! BakeService 이식).

use eframe::egui;
use freedf_canvas::{PagePoint, ViewTransform};
use freedf_core::model::{Stroke, ToolType};
use freedf_core::pen::Materials;
use freedf_core::pipeline::InkPipeline;
use freedf_core::store::AnnotationStore;
use freedf_core::transform::{MAX_ZOOM, MIN_ZOOM};
use freedf_services::pdf::{load_pdfium, DocumentView, Pdfium};
use std::cell::RefCell;
use std::path::Path;

/// 기본 페이지 크기 (A4 portrait, pt).
pub const BLANK_PAGE: [f32; 2] = [595.0, 842.0];
/// 줌 버튼 1회 배율.
const ZOOM_STEP: f32 = 1.25;
/// PDF 페이지 텍스처 렌더 폭 (px) — v1은 단일 해상도.
const PDF_TEX_WIDTH: f32 = 1400.0;
/// 스무딩 강도 — freedf 설정 기본값과 같다(OFF). 켜면 1€ 필터가 점을 다듬는다.
const SMOOTHING: f32 = 0.0;
/// 새 점을 받아들이는 최소 화면 이동(px). 같은 자리의 반복 샘플(정지 중 프레임)과
/// 부화소 떨림을 걸러 리본에 길이 0 세그먼트가 쌓이지 않게 한다.
const MIN_STEP_PX: f32 = 0.75;

/// 탭 하나 = 문서 하나 — 저장소·페이지·뷰·PDF를 독립으로 가진다.
pub(crate) struct Doc {
    pub id: u64,
    pub name: String,
    pub store: AnnotationStore,
    pub page: usize,
    pub page_size: [f32; 2],
    pub view: ViewTransform,
    pub pdf: Option<DocumentView>,
    /// (텍스처, 렌더된 페이지) — 표시는 page_size × zoom, uv 전체.
    page_tex: Option<(egui::TextureHandle, usize)>,
    /// 마지막으로 렌더를 **시도한** 페이지 — 실패해도 프레임마다 재시도하지
    /// 않는다 (렌더 실패 시 매 프레임 블로킹 렌더 반복 버그 방지).
    tex_attempted: Option<usize>,
}

impl Doc {
    fn blank(id: u64, name: String) -> Self {
        Self {
            id,
            name,
            store: AnnotationStore::new(),
            page: 0,
            page_size: BLANK_PAGE,
            view: ViewTransform::default(),
            pdf: None,
            page_tex: None,
            tex_attempted: None,
        }
    }

    fn from_pdf(id: u64, name: String, pdf: DocumentView) -> Self {
        let page_size = pdf.page_size_pts(0);
        Self {
            id,
            name,
            store: AnnotationStore::new(),
            page: 0,
            page_size,
            view: ViewTransform::default(),
            pdf: Some(pdf),
            page_tex: None,
            tex_attempted: None,
        }
    }

    /// 페이지 화면 사각형 — origin(캔버스 원점) + **뷰 변환 전체**(팬/줌).
    ///
    /// 팬을 빼면 종이/PDF/히트테스트가 잉크 메시(`canvas_mesh_to_egui`)와
    /// 어긋난다 — 팬 뒤에 "보이는 종이"에 대고 눌러도 잉크가 다른 자리에 남는다.
    fn page_rect(&self, origin: egui::Pos2) -> egui::Rect {
        egui::Rect::from_min_size(
            origin + egui::vec2(self.view.pan_x, self.view.pan_y),
            egui::vec2(
                self.page_size[0] * self.view.zoom,
                self.page_size[1] * self.view.zoom,
            ),
        )
    }

    /// 페이지 원점 — 뷰포트 안에서 페이지를 가로 중앙 정렬, 세로는 상단 여백.
    fn page_origin(&self, rect: egui::Rect) -> egui::Pos2 {
        let w = self.page_size[0] * self.view.zoom;
        let x = rect.left() + ((rect.width() - w) * 0.5).max(8.0);
        egui::pos2(x, rect.top() + 12.0)
    }

    /// 페이지 중심 (화면 px, origin 상대) — 줌 버튼의 고정점. 팬도 반영한다
    /// (팬한 상태에서 줌해도 화면 중앙의 페이지 점이 고정).
    fn last_page_center_px(&self) -> [f32; 2] {
        [
            self.page_size[0] * self.view.zoom * 0.5 + self.view.pan_x,
            self.page_size[1] * self.view.zoom * 0.5 + self.view.pan_y,
        ]
    }
}

/// 캔버스 엔진 — 위젯 트리 바깥의 명령형 상태 (문서 목록 + 활성 문서).
pub(crate) struct Canvas {
    /// 탭 = 문서 (테스트에서도 읽을 수 있게 crate 공개).
    pub(crate) docs: Vec<Doc>,
    /// 활성 문서 인덱스 (docs 내) — docs는 절대 비지 않는다.
    active: usize,
    next_doc_id: u64,
    pub(crate) tool: ToolType,
    pub(crate) color: [u8; 4],
    pub(crate) width: f32,
    /// 진행 중 획 — **코어의 `InkPipeline`** (1€ 필터 + 인과적 선폭 확정 +
    /// 동결 커밋). 점/폭/시각은 전부 이 객체가 소유하고, 여기서는 down/drag/up만
    /// 부른다 (도구별 분기 없음 — 재료는 `Materials::for_tool`).
    ink: Option<InkPipeline>,
    /// 오른쪽 버튼 팬 진행 중 — **캔버스 안에서 눌렀을 때만** 켜진다
    /// (캔버스 밖에서 누른 드래그가 팬으로 새는 버그 방지).
    pan_active: bool,
    /// 모달이 열려 있는 동안 캔버스 포인터 입력을 무시 (셸이 매 프레임 동기화).
    input_enabled: bool,
    /// 마지막 프레임의 포인터 위치 — 팬(오른쪽 버튼 드래그) 델타 계산용.
    last_pointer: Option<egui::Pos2>,
    pdfium: Option<Pdfium>,
    pub pdf_error: Option<String>,
    /// 토스트 — (메시지, 표시 시작 시각). 시간 만료는 엔진이 소유 (`toast()`).
    toast: Option<(String, std::time::Instant)>,
    mesher: freedf_canvas::CoreRibbonMesher,
}

impl Default for Canvas {
    fn default() -> Self {
        Self {
            docs: vec![Doc::blank(0, String::from("Untitled"))],
            active: 0,
            next_doc_id: 1,
            tool: ToolType::Pen,
            color: [26, 26, 28, 255],
            width: 2.0,
            ink: None,
            pan_active: false,
            input_enabled: true,
            last_pointer: None,
            pdfium: None,
            pdf_error: None,
            toast: None,
            mesher: freedf_canvas::CoreRibbonMesher {
                materials: Materials::new(Default::default(), Default::default()),
                pen_soak: Default::default(),
                fountain_soak: Default::default(),
                pen_grain: Default::default(),
                fountain_grain: Default::default(),
                tilt_magnitude: 0.0,
                feather_pt: 1.0,
            },
        }
    }
}

thread_local! {
    static ENGINE: RefCell<Canvas> = RefCell::new(Canvas::default());
}

/// 엔진 접근 — UI 스레드 전용 (위 `<Raw>` 경계 참고).
pub(crate) fn with<R>(f: impl FnOnce(&mut Canvas) -> R) -> R {
    ENGINE.with(|c| f(&mut c.borrow_mut()))
}

impl Canvas {
    /// 활성 문서 (항상 존재 — docs는 절대 비지 않는다).
    pub(crate) fn doc(&mut self) -> &mut Doc {
        &mut self.docs[self.active]
    }

    /// 활성 문서 읽기 전용 — 렌더 경로용.
    fn doc_ref(&self) -> &Doc {
        &self.docs[self.active]
    }

    /// 진행 중 획을 **코어에서 동결**(마지막 폭 확정 + 불변 `Stroke`)해 활성
    /// 문서에 기록 (탭 전환/닫기 전에도 호출된다).
    ///
    /// 점 하나짜리 탭도 점으로 남긴다 — 펜을 짧게 찍는 입력이 사라지지 않게
    /// (구 구현은 `points.len() >= 2`를 요구해 탭이 통째로 버려졌다).
    fn finish_active(&mut self) {
        let Some(mut pipeline) = self.ink.take() else {
            return;
        };
        let Some(stroke) = pipeline.up(0) else {
            return;
        };
        if stroke.points.is_empty() {
            return;
        }
        let doc = self.doc();
        doc.store.add_stroke(
            doc.page,
            stroke.tool,
            stroke.color,
            stroke.width,
            stroke.points,
        );
    }

    /// 진행 중 획을 버린다 (문서를 닫을 때 등 — 잘못된 문서에 기록 방지).
    fn drop_active(&mut self) {
        self.ink = None;
    }
}

// ── elm 핸들러용 커맨드 ─────────────────────────────────────────────

pub fn zoom_in() {
    with(|c| {
        let center = c.doc().last_page_center_px();
        let view = std::mem::take(&mut c.doc().view);
        c.doc().view = zoom_at_view(view, center, ZOOM_STEP);
    });
}

pub fn zoom_out() {
    with(|c| {
        let center = c.doc().last_page_center_px();
        let view = std::mem::take(&mut c.doc().view);
        c.doc().view = zoom_at_view(view, center, 1.0 / ZOOM_STEP);
    });
}

/// 줌 1 + 팬 원위치.
pub fn zoom_fit() {
    with(|c| c.doc().view = ViewTransform::default());
}

/// 현재 페이지의 잉크 전체 제거 (확인 모달 경유).
pub fn clear_ink() {
    with(|c| {
        let page = c.doc().page;
        let ids: Vec<u64> = c
            .doc()
            .store
            .strokes_on(page)
            .iter()
            .map(|s| s.id)
            .collect();
        c.doc().store.remove_strokes(page, &ids);
        c.toast_now(String::from("페이지 잉크를 지웠습니다"));
    });
}

/// 다음 PDF 페이지로 (PDF가 없으면 no-op).
pub fn page_next() {
    with(|c| {
        let (page, count) = {
            let doc = c.doc();
            let count = doc.pdf.as_ref().map(|d| d.page_count()).unwrap_or(0);
            (doc.page, count)
        };
        if page + 1 < count {
            let doc = c.doc();
            doc.page = page + 1;
            doc.page_tex = None;
            doc.tex_attempted = None;
        }
    });
}

/// 이전 PDF 페이지로 (PDF가 없거나 첫 페이지면 no-op).
pub fn page_prev() {
    with(|c| {
        let can = c.doc().page > 0 && c.doc().pdf.is_some();
        if can {
            let doc = c.doc();
            doc.page -= 1;
            doc.page_tex = None;
            doc.tex_attempted = None;
        }
    });
}

/// 북마크 토글 (현재 페이지).
pub fn toggle_bookmark() {
    with(|c| {
        let page = c.doc().page;
        c.doc().store.toggle_bookmark(page);
        let marked = c.doc().store.bookmarks().contains(&page);
        c.toast_now(if marked {
            format!("북마크 추가: {page}페이지")
        } else {
            format!("북마크 제거: {page}페이지")
        });
    });
}

/// 북마크된 페이지 목록 (오름차순 — store가 정렬해 둔다).
pub fn bookmark_list() -> Vec<usize> {
    with(|c| c.doc().store.bookmarks().to_vec())
}

// ── 토스트 — 시간 기반 자동 만료 알림 (엔진 소유, 셸은 읽기만) ─────────

/// 토스트 표시 시간 (초).
const TOAST_SECS: u64 = 3;

impl Canvas {
    /// 토스트를 지금 시각으로 표시한다.
    fn toast_now(&mut self, msg: String) {
        self.toast = Some((msg, std::time::Instant::now()));
    }
}

/// 토스트 표시 — `TOAST_SECS`초 뒤 자동으로 사라진다.
pub fn show_toast(msg: impl Into<String>) {
    with(|c| c.toast_now(msg.into()));
}

/// 아직 유효한 토스트 메시지 (만료 시 None) — 셸이 매 프레임 읽는다.
pub fn toast() -> Option<String> {
    with(|c| match &c.toast {
        Some((msg, at)) if at.elapsed().as_secs() < TOAST_SECS => Some(msg.clone()),
        _ => None,
    })
}

// ── 잉크 리본 — 도구/색상/굵기 (문자열 기반 커맨드 — view! 이벤트 단순화) ──

/// 도구 선택 (Pen/Fountain/Highlighter/Eraser — 그 외 값은 Pen).
pub fn select_tool(name: &str) {
    with(|c| {
        c.tool = match name {
            "Fountain" => ToolType::Fountain,
            "Highlighter" => ToolType::Highlighter,
            "Eraser" => ToolType::Eraser,
            _ => ToolType::Pen,
        };
    });
}

/// 현재 도구 이름 (리본 활성 표시용).
pub fn tool_name() -> String {
    with(|c| {
        String::from(match c.tool {
            ToolType::Pen => "Pen",
            ToolType::Fountain => "Fountain",
            ToolType::Highlighter => "Highlighter",
            ToolType::Eraser => "Eraser",
            ToolType::Pan => "Pan",
        })
    })
}

/// 즐겨찾기 색상 선택 (Black/Red/Blue — settings 서비스 기본 팔레트와 동일).
pub fn select_color(name: &str) {
    let rgba: [u8; 4] = match name {
        "Red" => [255, 71, 66, 255],
        "Blue" => [72, 166, 235, 255],
        _ => [26, 26, 28, 255],
    };
    with(|c| c.color = rgba);
}

/// 현재 색상 이름 (팔레트에 없으면 "Custom").
pub fn color_name() -> String {
    with(|c| match c.color {
        [255, 71, 66, 255] => String::from("Red"),
        [72, 166, 235, 255] => String::from("Blue"),
        [26, 26, 28, 255] => String::from("Black"),
        _ => String::from("Custom"),
    })
}

/// 굵기 프리셋 (pt) — Thin/Medium/Thick.
pub fn select_width(name: &str) {
    let w = match name {
        "Thin" => 1.5,
        "Thick" => 4.0,
        _ => 2.5,
    };
    with(|c| c.width = w);
}

/// 현재 굵기 프리셋 이름 (프리셋이 아니면 "Medium").
pub fn width_name() -> String {
    with(|c| {
        if (c.width - 1.5).abs() < 0.01 {
            String::from("Thin")
        } else if (c.width - 4.0).abs() < 0.01 {
            String::from("Thick")
        } else {
            String::from("Medium")
        }
    })
}

// ── 설정 — 잉크 기본값 저장/복원 (파일 백엔드: app_data_dir의 JSON) ──────

/// 잉크 기본값 — 리본의 **표시 이름**을 그대로 저장한다. 복원은
/// `select_*` 커맨드를 거치므로 알 수 없는 값은 자동으로 기본값 폴백된다.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct InkDefaults {
    pub tool: String,
    pub color: String,
    pub width: String,
}

/// 설정 파일 경로 — `<app_data_dir>/gui-ink-defaults.json`.
/// (Windows: `%LOCALAPPDATA%\FreeDF`, Linux/macOS: `~/.local/share/freedf`)
pub(crate) fn settings_path() -> std::path::PathBuf {
    freedf_services::storage::app_data_dir().join("gui-ink-defaults.json")
}

/// 현재 리본 상태를 기본값 파일로 저장한다 (성공/실패 토스트).
pub fn save_defaults() {
    let path = settings_path();
    let result = save_defaults_to(&path);
    with(|c| {
        c.toast_now(match result {
            Ok(()) => String::from("잉크 기본값을 저장했습니다"),
            Err(e) => format!("기본값 저장 실패: {e}"),
        });
    });
}

/// 지정한 경로에 현재 잉크 기본값을 쓴다 (테스트 가능한 순수 경로 버전).
pub(crate) fn save_defaults_to(path: &std::path::Path) -> Result<(), String> {
    let defaults = InkDefaults {
        tool: tool_name(),
        color: color_name(),
        width: width_name(),
    };
    let json = serde_json::to_string_pretty(&defaults).map_err(|e| e.to_string())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, json).map_err(|e| e.to_string())
}

/// 앱 시작 시 잉크 기본값을 복원한다 — 실패(파일 없음/손상)는 조용히 기본값.
/// main에서 한 번 호출 (기본 Canvas는 순수하게 유지 — 테스트 오염 방지).
pub fn load_defaults() {
    let _ = load_defaults_from(&settings_path());
}

/// 지정한 경로에서 잉크 기본값을 읽어 적용한다 (테스트 가능한 순수 경로 버전).
pub(crate) fn load_defaults_from(path: &std::path::Path) -> Result<InkDefaults, String> {
    let json = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let defaults: InkDefaults = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    // 적용은 select_* 커맨드 경로 — 알 수 없는 이름은 기본값 폴백이 이미 내장.
    select_tool(&defaults.tool);
    select_color(&defaults.color);
    select_width(&defaults.width);
    Ok(defaults)
}

/// 북마크/아웃라인 클릭 → 해당 페이지로 점프.
pub fn go_to_page(page: usize) {
    with(|c| {
        let doc = c.doc();
        let max = doc.pdf.as_ref().map(|d| d.page_count()).unwrap_or(1);
        if page < max {
            doc.page = page;
            doc.page_tex = None;
            doc.tex_attempted = None;
        }
    });
}

/// 새 탭(빈 문서) 추가 — 활성 탭이 된다.
pub fn add_tab(name: String) {
    with(|c| {
        let id = c.next_doc_id;
        c.next_doc_id += 1;
        c.docs.push(Doc::blank(id, name));
        c.active = c.docs.len() - 1;
    });
}

/// 활성 탭 닫기 — 마지막 탭은 닫지 않고 새 빈 문서로 리셋한다
/// (freedf의 "No documents" 빈 상태는 v2 과제).
pub fn close_tab() {
    with(|c| {
        c.drop_active();
        if c.docs.len() > 1 {
            c.docs.remove(c.active);
            c.active = c.active.min(c.docs.len() - 1);
        } else {
            let id = c.docs[0].id;
            c.docs[0] = Doc::blank(id, String::from("Untitled"));
        }
    });
}

/// 탭 선택 (id 기준 — 이름은 중복될 수 있다).
pub fn select_tab(id: u64) {
    with(|c| {
        if let Some(i) = c.docs.iter().position(|d| d.id == id) {
            c.drop_active();
            c.active = i;
        }
    });
}

/// (id, 이름) 목록 — 셸 탭 스트립이 읽어 간다.
pub fn tab_names() -> Vec<(u64, String)> {
    with(|c| c.docs.iter().map(|d| (d.id, d.name.clone())).collect())
}

/// 활성 탭 id.
pub fn active_tab_id() -> u64 {
    with(|c| c.docs[c.active].id)
}

/// 북마크/아웃라인 패널용 항목 — 제목에는 깊이만큼 들여쓰기가 **미리 반영**된다
/// (문자열 보간 안에서 repeat 식을 쓰면 format string이 깨지므로 여기서 계산).
///
/// `Clone`: elm-magic 0.5.0은 지역 컬렉션의 `.iter().map(..)`을
/// `(x).clone().into_iter().map(..)`으로 전개한다(지역 변수 재사용 보존) —
/// 목록이 `Clone`이어야 `{if ..}` 안에서 `.iter()`를 쓸 수 있다.
#[derive(Clone)]
pub(crate) struct OutlineEntry {
    pub title: String,
    pub page: usize,
}

/// PDF 북마크 트리를 패널 렌더용으로 평탄화한다 (순수 함수 — 테스트 대상).
fn flatten_outline(
    nodes: &[freedf_core::outline::OutlineNode],
    depth: usize,
    out: &mut Vec<OutlineEntry>,
) {
    for n in nodes {
        out.push(OutlineEntry {
            title: format!("{}{}", "  ".repeat(depth), n.title),
            page: n.page_index.unwrap_or(0),
        });
        flatten_outline(&n.children, depth + 1, out);
    }
}

/// 활성 문서의 아웃라인 (PDF가 없으면 빈 목록).
pub fn outline_list() -> Vec<OutlineEntry> {
    with(|c| {
        let Some(pdf) = &c.doc().pdf else {
            return Vec::new();
        };
        let mut out = Vec::new();
        flatten_outline(&pdf.outline(), 0, &mut out);
        out
    })
}

/// PDF 열기 — **새 탭(문서)** 으로 연다. 실패(엔진 부재/파일 오류)는
/// `pdf_error`로 상태바에 표시되고 문서는 추가되지 않는다.
pub fn open_pdf(path: String) {
    with(|c| {
        if c.pdfium.is_none() {
            match load_pdfium() {
                Ok(p) => c.pdfium = Some(p),
                Err(e) => {
                    c.pdf_error = Some(format!("PDF 엔진 로드 실패: {e}"));
                    return;
                }
            }
        }
        let pdfium = c.pdfium.as_ref().unwrap();
        match DocumentView::open(pdfium, Path::new(&path)) {
            Ok(doc_view) => {
                c.pdf_error = None;
                let name = Path::new(&path)
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| String::from("PDF"));
                let id = c.next_doc_id;
                c.next_doc_id += 1;
                let doc = Doc::from_pdf(id, name.clone(), doc_view);
                c.docs.push(doc);
                c.active = c.docs.len() - 1;
                c.toast_now(format!("PDF 열기: {name}"));
            }
            Err(e) => {
                c.pdf_error = Some(format!("Could not open PDF: {e}"));
                c.toast_now(String::from("PDF 열기 실패 — 상태바 확인"));
            }
        }
    });
}

/// 모달 열림 여부를 반영하고 상태바 문자열을 돌려준다 — 셸이 매 프레임 호출한다.
/// 모달이 열려 있는 동안 캔버스 포인터 입력을 무시해 모달 뒤 잉크를 방지한다.
pub fn sync_and_status(modal_open: bool) -> String {
    with(|c| {
        c.input_enabled = !modal_open;
        // pdf_error를 먼저 소유권으로 꺼내 doc 가용 대여와 충돌하지 않게 한다.
        let err = c.pdf_error.clone();
        let doc = c.doc();
        let zoom_pct = (doc.view.zoom * 100.0).round() as i32;
        let strokes = doc.store.stroke_count_on(doc.page);
        let bookmarks = doc.store.bookmarks().len();
        let base = if let Some(err) = err {
            format!("{err} · ")
        } else if doc.pdf.is_some() {
            let count = doc.pdf.as_ref().map(|d| d.page_count()).unwrap_or(0);
            format!("PDF {}/{} · ", doc.page + 1, count)
        } else {
            format!(
                "빈 페이지 {}×{}pt · ",
                doc.page_size[0] as i32, doc.page_size[1] as i32
            )
        };
        format!("{base}줌 {zoom_pct}% · 획 {strokes} · 북마크 {bookmarks}")
    })
}

/// `<Raw>` 진입점.
pub fn paint(ui: &mut egui::Ui) {
    with(|c| c.paint_ui(ui));
}

// ── 렌더/입력 (egui) ───────────────────────────────────────────────

impl Canvas {
    fn paint_ui(&mut self, ui: &mut egui::Ui) {
        let avail = ui.available_size();
        let (rect, _resp) = ui.allocate_exact_size(avail, egui::Sense::click_and_drag());
        let painter = ui.painter_at(rect);

        // 캔버스 바탕 — 스테이지 색도 CSS 팔레트 토큰에서 온다(`style::stage_color`
        // = `surface_alt`). 칠하지 않으면 창 클리어 색이 드러나 검정으로 보인다.
        let stage_bg = crate::style::stage_color();
        painter.rect_filled(rect, 0.0, stage_bg);

        // eguidev 계약 — 캔버스 기하 공개 (docs/eguidev-automation.md).
        crate::dev::publish_rect(ui, "canvas.surface", rect);

        // PDF 텍스처 (필요하면 이번 프레임에 렌더 — pdfium이 있을 때만).
        self.ensure_page_texture(ui.ctx());

        let origin = self.doc().page_origin(rect);
        self.draw_paper(&painter, origin);
        self.draw_pdf(&painter, origin);
        self.paint_committed(&painter, origin);
        self.paint_active(&painter, origin);
        self.handle_input(ui, rect, origin);
        self.last_pointer = ui.input(|i| i.pointer.latest_pos());
    }

    fn draw_paper(&self, painter: &egui::Painter, origin: egui::Pos2) {
        let rect = self.doc_ref().page_rect(origin);
        // 종이 경계/그림자 색도 팔레트 토큰(`border`) — 캔버스는 CSS 밖(painter)이라
        // style.rs가 색을 제공하고 여기서는 옮기기만 한다.
        let border = crate::style::page_border_color();
        painter.rect_filled(rect.expand(1.0), 2.0, border);
        painter.rect_filled(rect, 0.0, egui::Color32::WHITE);
        painter.rect_stroke(rect, 0.0, (1.0, border), egui::StrokeKind::Outside);
    }

    fn draw_pdf(&self, painter: &egui::Painter, origin: egui::Pos2) {
        if let Some((tex, _page)) = &self.doc_ref().page_tex {
            painter.image(
                tex.id(),
                self.doc_ref().page_rect(origin),
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
    }

    /// 정착된 획 전체를 병합 메시 하나로 재굽기해 그린다 (v1 — 프레임마다
    /// 재굽기; freedf의 백그라운드 BakeService는 획 수가 커지면 이식).
    fn paint_committed(&mut self, painter: &egui::Painter, origin: egui::Pos2) {
        let now = now_ms();
        let (zoom, pan_x, pan_y, page) = {
            let doc = self.doc_ref();
            (doc.view.zoom, doc.view.pan_x, doc.view.pan_y, doc.page)
        };
        self.mesher.feather_pt = 1.0 / zoom.max(1e-3);
        let mut mesh = freedf_canvas::Mesh::default();
        for s in self.doc_ref().store.strokes_on(page) {
            self.mesher.append_stroke(&mut mesh, &canvas_stroke(s), now);
        }
        if !mesh.vertices.is_empty() {
            painter.add(egui::Shape::mesh(canvas_mesh_to_egui(
                &mesh, origin, zoom, pan_x, pan_y,
            )));
        }
    }

    /// 진행 중 획(코어 `LiveStroke`)을 리본으로 그린다 — **커밋과 같은 생성기**를
    /// 쓰므로 펜을 떼는 순간 그림이 바뀌지 않는다 (WYSIWYG).
    fn paint_active(&mut self, painter: &egui::Painter, origin: egui::Pos2) {
        let Some(live) = self.ink.as_ref().and_then(|p| p.live()) else {
            return;
        };
        if live.points.is_empty() {
            return;
        }
        let created_ms = live
            .points
            .first()
            .map(|p| p.t_ms)
            .filter(|t| *t > 0)
            .unwrap_or_else(now_ms);
        let cs = freedf_canvas::Stroke {
            id: freedf_canvas::StrokeId(0),
            kind: freedf_canvas::LayerKind::Ink,
            tool: live.tool,
            color: live.color,
            base_width: live.width,
            points: live
                .points
                .iter()
                .map(|p| freedf_canvas::StrokePoint {
                    position: freedf_canvas::PagePoint::new(p.x, p.y),
                    pressure: p.pressure,
                    t_ms: p.t_ms,
                    width: p.width,
                })
                .collect(),
            created_ms,
        };
        let (zoom, pan_x, pan_y) = {
            let doc = self.doc_ref();
            (doc.view.zoom, doc.view.pan_x, doc.view.pan_y)
        };
        self.mesher.feather_pt = 1.0 / zoom.max(1e-3);
        let mut mesh = freedf_canvas::Mesh::default();
        self.mesher.append_stroke(&mut mesh, &cs, now_ms());
        if !mesh.vertices.is_empty() {
            painter.add(egui::Shape::mesh(canvas_mesh_to_egui(
                &mesh, origin, zoom, pan_x, pan_y,
            )));
        }
    }

    fn handle_input(&mut self, ui: &mut egui::Ui, rect: egui::Rect, origin: egui::Pos2) {
        let (
            pos,
            time,
            primary_down,
            primary_pressed,
            primary_released,
            secondary_down,
            secondary_pressed,
            secondary_released,
            scroll,
        ) = ui.input(|i| {
            (
                i.pointer.latest_pos(),
                i.time,
                i.pointer.primary_down(),
                i.pointer.primary_pressed(),
                i.pointer.primary_released(),
                i.pointer.secondary_down(),
                i.pointer.secondary_pressed(),
                i.pointer.secondary_released(),
                i.smooth_scroll_delta,
            )
        });
        let page = self.doc().page_rect(origin);

        // ── 가로채기 방지 ── 모달 창이 열려 있으면(셸이 `sync_and_status`로
        // 알려준다) 캔버스 입력을 무시한다 — 모달 뒤에서 잉크가 그려지는
        // 버그 방지. (egui `hovered()`는 헤드리스/신규 인터랙션 모델에서
        // 불안정해 이 플래그로 대체했다.)
        let enabled = self.input_enabled;

        // ── 휠 줌 (포인터 아래 점 고정) ──
        if scroll.y != 0.0 && enabled {
            if let Some(pos) = pos {
                if rect.contains(pos) {
                    let rel = pos - origin;
                    let view = std::mem::take(&mut self.doc().view);
                    self.doc().view = zoom_at_view(view, [rel.x, rel.y], (scroll.y * 0.002).exp());
                }
            }
        }

        // ── 오른쪽 버튼 드래그 = 팬 (캔버스 안에서 눌렀을 때만 시작) ──
        if secondary_pressed && enabled {
            if pos.map(|p| page.contains(p)).unwrap_or(false) {
                self.pan_active = true;
            }
        }
        if secondary_released || !secondary_down {
            self.pan_active = false;
        }
        if self.pan_active {
            if let (Some(pos), Some(last)) = (pos, self.last_pointer) {
                let doc = self.doc();
                doc.view.pan_x += pos.x - last.x;
                doc.view.pan_y += pos.y - last.y;
            }
        }

        // ── 왼쪽 버튼 = 잉크 (시작은 페이지 안에서만) ──
        //
        // 세션 수명은 **코어 `InkPipeline`**이 소유한다: down(시작) → drag(꼬리)
        // → up(동결 커밋). 여기서는 화면→페이지 변환과 "페이지 안" 판정만 한다.
        let in_page = pos.map(|p| page.contains(p)).unwrap_or(false);
        if primary_pressed && enabled && in_page && is_ink_tool(self.tool) {
            if let Some(pos) = pos {
                let p = self.page_pos(pos, origin);
                let mut pipeline = InkPipeline::new(self.mesher.materials, self.width, SMOOTHING);
                // 압력은 egui 포인터에 없다(마우스/단순 펜) — 명목 1.0.
                pipeline.down(self.tool, self.color, p.x, p.y, 1.0, time, now_ms(), 0.0);
                self.ink = Some(pipeline);
            }
        } else if primary_down && self.ink.is_some() && in_page {
            if let Some(pos) = pos {
                let zoom = self.doc_ref().view.zoom;
                let p = self.page_pos(pos, origin);
                // 같은 자리 반복 샘플/부화소 떨림은 버린다 (길이 0 세그먼트 방지).
                let moved = self
                    .ink
                    .as_ref()
                    .and_then(|pipe| pipe.live())
                    .and_then(|live| live.points.last())
                    .map(|last| {
                        let dx = p.x - last.x;
                        let dy = p.y - last.y;
                        (dx * dx + dy * dy).sqrt() * zoom > MIN_STEP_PX
                    })
                    .unwrap_or(false);
                if moved {
                    if let Some(pipe) = self.ink.as_mut() {
                        pipe.drag(p.x, p.y, 1.0, time, now_ms());
                    }
                }
            }
        }
        if primary_released || (self.ink.is_some() && !primary_down) {
            self.finish_active();
        }
    }

    /// 화면 좌표 → 활성 문서 페이지 pt 좌표 (뷰 변환 = 줌/팬).
    fn page_pos(&self, pos: egui::Pos2, origin: egui::Pos2) -> PagePoint {
        let rel = pos - origin;
        let doc = self.docs.get(self.active).expect("활성 문서");
        doc.view.view_to_page(PagePoint::new(rel.x, rel.y))
    }

    /// 현재 페이지 텍스처가 없으면(또는 페이지가 바뀌었으면) 렌더한다.
    /// 렌더에 **실패한 페이지는 프레임마다 재시도하지 않는다** — 실패한 채로
    /// 두면 매 프레임 블로킹 렌더가 반복되어 프리즈가 난다.
    fn ensure_page_texture(&mut self, ctx: &egui::Context) {
        if self.pdfium.is_none() {
            return;
        }
        let doc = self.doc();
        if doc.pdf.is_none() {
            return;
        }
        if let Some((_, rendered_page)) = &doc.page_tex {
            if *rendered_page == doc.page {
                return;
            }
        }
        if doc.tex_attempted == Some(doc.page) {
            return;
        }
        doc.tex_attempted = Some(doc.page);
        let page = doc.page;
        let Some(pdf) = &doc.pdf else { return };
        match pdf.render_page(page, PDF_TEX_WIDTH, freedf_services::pdf::MAX_RENDER_DIM) {
            Ok(rp) => {
                let img = egui::ColorImage::from_rgba_unmultiplied([rp.width, rp.height], &rp.rgba);
                let tex = ctx.load_texture("pdf_page", img, egui::TextureOptions::LINEAR);
                self.doc().page_tex = Some((tex, page));
            }
            Err(e) => {
                self.pdf_error = Some(e);
            }
        }
    }
}

/// 포인터 아래 페이지 점을 고정하며 줌 (순수 함수 — 테스트 대상).
fn zoom_at_view(view: ViewTransform, pointer_px: [f32; 2], factor: f32) -> ViewTransform {
    let page = view.view_to_page(PagePoint::new(pointer_px[0], pointer_px[1]));
    let mut v = ViewTransform {
        zoom: (view.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM),
        ..view
    };
    let back = v.page_to_view(page);
    v.pan_x += pointer_px[0] - back.x;
    v.pan_y += pointer_px[1] - back.y;
    v
}

/// 잉크를 남기는 도구인가 — 코어의 재료 팩토리(`Materials::for_tool`)가
/// 스트로크 재료를 주는 도구만 해당한다 (`Eraser`/`Pan`은 `None`).
///
/// freedf-gui에는 아직 지우개 세션이 없으므로(Phase 4 이식) 지우개로 눌러도
/// 잉크를 남기지 않는다 — 재료 없는 도구로 `InkPipeline::down`을 부르면
/// 코어가 패닉한다 (`WidthLocker::new`의 재료 `expect`).
fn is_ink_tool(tool: ToolType) -> bool {
    matches!(
        tool,
        ToolType::Pen | ToolType::Fountain | ToolType::Highlighter
    )
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// freedf-core 스트로크 → freedf-canvas 스트로크 (freedf `canvas_stroke`와 동일).
fn canvas_stroke(s: &Stroke) -> freedf_canvas::Stroke {
    freedf_canvas::Stroke {
        id: freedf_canvas::StrokeId(s.id),
        kind: freedf_canvas::LayerKind::Ink,
        tool: s.tool,
        color: s.color,
        base_width: s.width,
        points: s
            .points
            .iter()
            .map(|p| freedf_canvas::StrokePoint {
                position: freedf_canvas::PagePoint::new(p.x, p.y),
                pressure: p.pressure,
                t_ms: p.t_ms,
                width: p.width,
            })
            .collect(),
        created_ms: s.created_ms,
    }
}

/// 페이지 좌표로 구워진 메시 → 화면 좌표 egui 메시
/// (freedf `app/canvas/mod.rs::canvas_mesh_to_egui`와 동일 변환).
fn canvas_mesh_to_egui(
    mesh: &freedf_canvas::Mesh,
    origin: egui::Pos2,
    zoom: f32,
    pan_x: f32,
    pan_y: f32,
) -> egui::Mesh {
    let mut out = egui::Mesh::default();
    for (p, c) in mesh.vertices.iter().zip(&mesh.colors) {
        let x = origin.x + p[0] * zoom + pan_x;
        let y = origin.y + p[1] * zoom + pan_y;
        let a = (c[3] * 255.0).clamp(0.0, 255.0) as u8;
        let col = egui::Color32::from_rgba_unmultiplied(
            (c[0] * 255.0).clamp(0.0, 255.0) as u8,
            (c[1] * 255.0).clamp(0.0, 255.0) as u8,
            (c[2] * 255.0).clamp(0.0, 255.0) as u8,
            a,
        );
        out.vertices
            .push(egui::epaint::Vertex::untextured(egui::pos2(x, y), col));
    }
    out.indices.extend_from_slice(&mesh.indices);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use freedf_core::model::StrokePoint;

    /// 헤드리스 egui 프레임 하나 — `paint`를 `CentralPanel`에 직접 그린다
    /// (셸 경유 테스트는 각자 셸을 그린다).
    fn paint_frame(ctx: &egui::Context, events: Vec<egui::Event>) {
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };
        input.events = events;
        let mut out = ctx.run_ui(input, |ctx| {
            egui::CentralPanel::default().show(ctx, paint);
        });
        out.textures_delta.clear();
    }

    /// 좌클릭 한 번의 press/release 이벤트.
    fn press(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        }
    }

    #[test]
    fn zoom_at_keeps_pointer_page_point_fixed() {
        let view = ViewTransform::new(1.0, 0.0, 0.0);
        let pointer = [300.0_f32, 400.0];
        let page_pt = view.view_to_page(PagePoint::new(pointer[0], pointer[1]));
        let zoomed = zoom_at_view(view, pointer, 1.5);
        let still = zoomed.view_to_page(PagePoint::new(pointer[0], pointer[1]));
        assert!((page_pt.x - still.x).abs() < 1e-4);
        assert!((page_pt.y - still.y).abs() < 1e-4);
        assert!((zoomed.zoom - 1.5).abs() < 1e-6);
    }

    #[test]
    fn zoom_clamped_to_core_bounds() {
        let zoomed = zoom_at_view(ViewTransform::new(MAX_ZOOM, 0.0, 0.0), [0.0, 0.0], 2.0);
        assert!((zoomed.zoom - MAX_ZOOM).abs() < 1e-6);
        let zoomed = zoom_at_view(ViewTransform::new(MIN_ZOOM, 0.0, 0.0), [0.0, 0.0], 0.1);
        assert!((zoomed.zoom - MIN_ZOOM).abs() < 1e-6);
    }

    /// 헤드리스 egui에 실제 포인터 이벤트를 주입해 잉크가 저장소에 기록되는지
    /// 검증 (freedf-canvas 어댑터 테스트와 동일한 headless 방식).
    #[test]
    fn ink_stroke_lands_in_store() {
        with(|c| *c = Canvas::default());

        let ctx = egui::Context::default();
        let base = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let frame = |events: Vec<egui::Event>| {
            let mut input = base();
            input.events = events;
            let mut out = ctx.run_ui(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| paint(ui));
            });
            // headless: 폰트 아틀라스 델타 소비 (어댑터 테스트와 동일).
            out.textures_delta.clear();
        };

        frame(vec![
            egui::Event::PointerMoved(egui::pos2(300.0, 300.0)),
            press(egui::pos2(300.0, 300.0), true),
        ]);
        // 프레임당 최신 포인터 위치 1개가 샘플링된다 (60fps 실사용과 동일).
        frame(vec![egui::Event::PointerMoved(egui::pos2(350.0, 340.0))]);
        frame(vec![egui::Event::PointerMoved(egui::pos2(400.0, 380.0))]);
        frame(vec![egui::Event::PointerMoved(egui::pos2(450.0, 420.0))]);
        frame(vec![press(egui::pos2(450.0, 420.0), false)]);

        with(|c| {
            assert_eq!(
                c.docs[0].store.total_stroke_count(),
                1,
                "획이 저장소에 기록되어야 한다"
            );
            let pts = &c.docs[0].store.strokes_on(0)[0].points;
            assert!(pts.len() >= 4, "4점 이상: {}", pts.len());
            assert!(pts[0].x < pts.last().unwrap().x);
            assert!(pts[0].y < pts.last().unwrap().y);
            assert!(pts[0].x > 0.0 && pts[0].x < BLANK_PAGE[0]);
            assert!(pts[0].y > 0.0 && pts[0].y < BLANK_PAGE[1]);
        });
    }

    /// 펜을 짧게 찍은 탭(한 프레임 안 press+release)도 **점 하나짜리 획**으로
    /// 남는다 — 구 구현은 2점 미만을 버려 탭이 통째로 사라졌다.
    #[test]
    fn single_point_tap_commits_a_dot() {
        with(|c| *c = Canvas::default());
        let ctx = egui::Context::default();
        let at = egui::pos2(300.0, 300.0);
        paint_frame(
            &ctx,
            vec![
                egui::Event::PointerMoved(at),
                press(at, true),
                press(at, false),
            ],
        );
        with(|c| {
            assert_eq!(
                c.docs[0].store.total_stroke_count(),
                1,
                "탭도 점으로 남아야 한다"
            );
            assert_eq!(c.docs[0].store.strokes_on(0)[0].points.len(), 1);
        });
    }

    /// 팬(오른쪽 드래그) 뒤에도 **누른 자리**에 잉크가 남는다 — 종이/히트테스트/
    /// 메시가 같은 뷰 변환(팬 포함)을 쓴다.
    ///
    /// 불변식으로 검증한다: 팬 (dx, dy)는 페이지를 화면에서 (dx, dy)만큼 옮기므로,
    /// 같은 페이지 점은 팬 전에는 P, 팬 후에는 P+(dx, dy)에서 눌린다.
    #[test]
    fn pan_keeps_press_position_in_sync() {
        let ctx = egui::Context::default();
        let tap = |at: egui::Pos2| {
            paint_frame(
                &ctx,
                vec![
                    egui::Event::PointerMoved(at),
                    press(at, true),
                    press(at, false),
                ],
            );
        };
        let only_point =
            |page: usize| with(|c| c.docs[0].store.strokes_on(page)[0].points[0].clone());

        // ① 팬 없이 찍은 페이지 점.
        with(|c| *c = Canvas::default());
        let base = egui::pos2(350.0, 330.0);
        tap(base);
        let before = only_point(0);

        // ② 오른쪽 드래그로 팬 (+50, +30) — 같은 페이지 점을 노려 찍는다.
        with(|c| *c = Canvas::default());
        let right = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: Default::default(),
        };
        let from = egui::pos2(300.0, 300.0);
        paint_frame(
            &ctx,
            vec![egui::Event::PointerMoved(from), right(from, true)],
        );
        paint_frame(
            &ctx,
            vec![egui::Event::PointerMoved(egui::pos2(350.0, 330.0))],
        );
        paint_frame(&ctx, vec![right(egui::pos2(350.0, 330.0), false)]);
        with(|c| {
            assert_eq!(c.docs[0].view.pan_x, 50.0);
            assert_eq!(c.docs[0].view.pan_y, 30.0);
        });
        tap(base + egui::vec2(50.0, 30.0));
        let after = only_point(0);

        assert!(
            (before.x - after.x).abs() < 0.01,
            "x가 어긋난다: {} vs {}",
            before.x,
            after.x
        );
        assert!(
            (before.y - after.y).abs() < 0.01,
            "y가 어긋난다: {} vs {}",
            before.y,
            after.y
        );
    }

    /// 지우개는 아직 지우기 세션이 없다 — 눌러도 패닉하지 않고 잉크를 남기지
    /// 않는다 (코어 `Materials::for_tool(Eraser)`가 재료를 주지 않는다).
    #[test]
    fn eraser_press_leaves_no_ink() {
        with(|c| *c = Canvas::default());
        select_tool("Eraser");
        let ctx = egui::Context::default();
        let start = egui::pos2(300.0, 300.0);
        paint_frame(
            &ctx,
            vec![egui::Event::PointerMoved(start), press(start, true)],
        );
        paint_frame(
            &ctx,
            vec![egui::Event::PointerMoved(egui::pos2(340.0, 330.0))],
        );
        paint_frame(&ctx, vec![press(egui::pos2(340.0, 330.0), false)]);
        with(|c| {
            assert_eq!(
                c.docs[0].store.total_stroke_count(),
                0,
                "지우개는 잉크를 남기지 않는다"
            );
        });
    }

    /// 실제 앱 경로(elm-magic 셸 안의 `<Raw>` 캔버스)로도 획이 기록되는지.
    ///
    /// `paint`를 직접 부르는 테스트는 셸의 배치/입력 상호작용을 우회한다 —
    /// 여기서는 셸 전체를 그려 캔버스가 실제로 놓이는 자리에서 입력이 닿는지 본다.
    #[test]
    fn ink_stroke_lands_through_shell_raw() {
        with(|c| *c = Canvas::default());

        let mut elm = elm_magic::Ctx::default();
        let ctx = egui::Context::default();
        let base = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1100.0, 720.0),
            )),
            ..Default::default()
        };
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let frame = |elm: &mut elm_magic::Ctx, events: Vec<egui::Event>| {
            let mut input = base();
            input.events = events;
            let mut out = ctx.run_ui(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    crate::shell::render_shell(ui, &mut *elm);
                });
            });
            out.textures_delta.clear();
        };

        // 셸을 두 프레임 그려 캔버스 자리를 확정한 뒤, 그 안쪽에 펜 드래그를 준다.
        frame(&mut elm, vec![]);
        frame(&mut elm, vec![]);
        let start = egui::pos2(500.0, 400.0);
        frame(
            &mut elm,
            vec![egui::Event::PointerMoved(start), press(start, true)],
        );
        for i in 1..=4 {
            let p = egui::pos2(500.0 + 20.0 * i as f32, 400.0 + 10.0 * i as f32);
            frame(&mut elm, vec![egui::Event::PointerMoved(p)]);
        }
        let end = egui::pos2(600.0, 460.0);
        frame(&mut elm, vec![press(end, false)]);

        with(|c| {
            assert_eq!(
                c.docs[0].store.total_stroke_count(),
                1,
                "셸 안의 캔버스에도 획이 기록되어야 한다"
            );
        });
    }

    /// 커밋된 획이 실제로 **화면 도형**으로 나오는지 (잉크 렌더 회귀 방지).
    /// 입력이 아니라 렌더 경로만 본다 — 저장소에 획을 직접 넣고 한 프레임 그린 뒤
    /// 프레임 출력에 정점을 가진 메시가 있는지 확인한다.
    #[test]
    fn committed_stroke_renders_mesh() {
        with(|c| {
            *c = Canvas::default();
            let pts = vec![
                StrokePoint {
                    x: 100.0,
                    y: 100.0,
                    pressure: 1.0,
                    t_ms: now_ms(),
                    width: 2.0,
                },
                StrokePoint {
                    x: 200.0,
                    y: 160.0,
                    pressure: 1.0,
                    t_ms: now_ms(),
                    width: 2.0,
                },
            ];
            c.doc()
                .store
                .add_stroke(0, ToolType::Pen, [26, 26, 28, 255], 2.5, pts);
        });

        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| paint(ui));
        });
        out.textures_delta.clear();

        let mut mesh_vertices = 0usize;
        for clipped in &out.shapes {
            if let egui::Shape::Mesh(m) = &clipped.shape {
                mesh_vertices += m.vertices.len();
            }
        }
        assert!(mesh_vertices > 0, "커밋된 획이 메시로 그려져야 한다");
    }

    #[test]
    fn clear_ink_empties_page() {
        with(|c| {
            *c = Canvas::default();
            let pts = vec![
                StrokePoint {
                    x: 10.0,
                    y: 10.0,
                    pressure: 1.0,
                    t_ms: now_ms(),
                    width: 2.0,
                },
                StrokePoint {
                    x: 40.0,
                    y: 40.0,
                    pressure: 1.0,
                    t_ms: now_ms(),
                    width: 2.0,
                },
            ];
            c.doc()
                .store
                .add_stroke(0, ToolType::Pen, [0, 0, 0, 255], 2.0, pts);
            assert_eq!(c.doc().store.total_stroke_count(), 1);
        });
        clear_ink();
        with(|c| assert_eq!(c.doc().store.total_stroke_count(), 0));
    }

    #[test]
    fn page_nav_without_pdf_is_noop() {
        with(|c| *c = Canvas::default());
        page_next();
        page_prev();
        with(|c| {
            assert_eq!(c.doc().page, 0);
            assert!(c.doc().pdf.is_none());
            assert!(c.pdf_error.is_none());
        });
    }

    #[test]
    fn open_pdf_without_engine_records_error_and_adds_no_doc() {
        with(|c| *c = Canvas::default());
        open_pdf("/nonexistent/no-such-file.pdf".to_string());
        with(|c| {
            assert!(c
                .pdf_error
                .as_deref()
                .map(|e| e.contains("PDF"))
                .unwrap_or(false));
            assert_eq!(c.docs.len(), 1);
        });
    }

    #[test]
    fn tab_lifecycle_add_select_close() {
        with(|c| *c = Canvas::default());
        add_tab("Sketch 2".to_string());
        add_tab("Sketch 3".to_string());
        with(|c| {
            assert_eq!(c.docs.len(), 3);
            assert_eq!(c.docs[c.active].name, "Sketch 3", "새 탭이 활성이어야 한다");
        });
        // 이름이 같은 탭도 id로 구분해 선택한다 — 첫 "Untitled"의 id를 찾아 선택.
        let untitled_id = tab_names()
            .into_iter()
            .find(|(_, name)| name == "Untitled")
            .map(|(id, _)| id)
            .expect("Untitled 탭");
        select_tab(untitled_id);
        with(|c| assert_eq!(c.docs[c.active].name, "Untitled"));
        // 닫기: 활성 문서 제거, 남은 문서로 활성 이동.
        close_tab();
        with(|c| {
            assert_eq!(c.docs.len(), 2);
            assert_eq!(c.docs[c.active].name, "Sketch 2");
        });
        close_tab();
        // 마지막 남은 탭(Sketch 3)에 획을 추가한다.
        with(|c| {
            assert_eq!(c.docs.len(), 1);
            assert_eq!(c.docs[c.active].name, "Sketch 3");
            let pts = vec![
                StrokePoint {
                    x: 1.0,
                    y: 1.0,
                    pressure: 1.0,
                    t_ms: 0,
                    width: 2.0,
                },
                StrokePoint {
                    x: 9.0,
                    y: 9.0,
                    pressure: 1.0,
                    t_ms: 0,
                    width: 2.0,
                },
            ];
            c.doc()
                .store
                .add_stroke(0, ToolType::Pen, [0, 0, 0, 255], 2.0, pts);
        });
        // 마지막 탭을 닫으면 닫지 않고 **빈 문서로 리셋** (획도 사라진다).
        close_tab();
        with(|c| {
            assert_eq!(c.docs.len(), 1);
            assert_eq!(c.docs[0].name, "Untitled");
            assert_eq!(c.docs[0].store.total_stroke_count(), 0, "리셋된 빈 문서");
        });
    }

    #[test]
    fn ink_is_isolated_per_tab() {
        with(|c| {
            *c = Canvas::default();
            let pts = vec![
                StrokePoint {
                    x: 1.0,
                    y: 1.0,
                    pressure: 1.0,
                    t_ms: 0,
                    width: 2.0,
                },
                StrokePoint {
                    x: 9.0,
                    y: 9.0,
                    pressure: 1.0,
                    t_ms: 0,
                    width: 2.0,
                },
            ];
            c.doc()
                .store
                .add_stroke(0, ToolType::Pen, [0, 0, 0, 255], 2.0, pts);
        });
        add_tab("Second".to_string()); // 새 문서로 전환됨
        with(|c| {
            // 문서별 저장소 분리 — 새 문서에는 획이 없고 첫 문서에만 있다.
            assert_eq!(c.docs[0].store.total_stroke_count(), 1);
            assert_eq!(c.docs[1].store.total_stroke_count(), 0);
        });
    }

    #[test]
    fn bookmarks_toggle_and_list() {
        with(|c| *c = Canvas::default());
        assert!(bookmark_list().is_empty());
        toggle_bookmark();
        assert_eq!(bookmark_list(), vec![0]);
        go_to_page(0);
        toggle_bookmark();
        assert!(bookmark_list().is_empty());
    }

    #[test]
    fn outline_flattens_tree_with_indent() {
        // PDF 없이도 평탄화 순수 함수를 검증한다 (깊이 들여쓰기 + 순서).
        use freedf_core::outline::OutlineNode;
        let tree = vec![OutlineNode::new(
            "Ch1",
            Some(0),
            vec![
                OutlineNode::new("1.1", Some(1), vec![]),
                OutlineNode::new(
                    "1.2",
                    Some(2),
                    vec![OutlineNode::new("1.2.1", Some(3), vec![])],
                ),
            ],
        )];
        let mut out = Vec::new();
        flatten_outline(&tree, 0, &mut out);
        assert_eq!(out.len(), 4);
        assert_eq!(out[0].title, "Ch1");
        assert_eq!(out[0].page, 0);
        assert_eq!(out[1].title, "  1.1");
        assert_eq!(out[1].page, 1);
        assert_eq!(out[3].title, "    1.2.1");
        assert_eq!(out[3].page, 3);
    }

    #[test]
    fn outline_list_without_pdf_is_empty() {
        with(|c| *c = Canvas::default());
        assert!(outline_list().is_empty());
    }

    #[test]
    fn ribbon_selects_tool_color_width() {
        with(|c| *c = Canvas::default());
        assert_eq!(tool_name(), "Pen");
        assert_eq!(color_name(), "Black");
        assert_eq!(width_name(), "Medium");
        select_tool("Highlighter");
        assert_eq!(tool_name(), "Highlighter");
        select_tool("Fountain");
        assert_eq!(tool_name(), "Fountain");
        select_tool("Eraser");
        assert_eq!(tool_name(), "Eraser");
        select_tool("Pen");
        assert_eq!(tool_name(), "Pen");
        select_color("Red");
        assert_eq!(color_name(), "Red");
        select_color("Blue");
        assert_eq!(color_name(), "Blue");
        select_color("Black");
        assert_eq!(color_name(), "Black");
        select_width("Thin");
        assert_eq!(width_name(), "Thin");
        select_width("Thick");
        assert_eq!(width_name(), "Thick");
        // 알 수 없는 값 — 프리셋 기본값으로 폴백.
        select_tool("Nonsense");
        assert_eq!(tool_name(), "Pen");
        select_color("Nonsense");
        assert_eq!(color_name(), "Black");
        select_width("Nonsense");
        assert_eq!(width_name(), "Medium");
    }

    #[test]
    fn toast_expires_after_delay() {
        with(|c| *c = Canvas::default());
        assert!(toast().is_none());
        show_toast("hello");
        assert_eq!(toast().as_deref(), Some("hello"));
        // 표시 시작 시각을 만료 시각 이전으로 되돌려 시간 경과를 시뮬레이션.
        with(|c| {
            if let Some((_, at)) = c.toast.as_mut() {
                *at -= std::time::Duration::from_secs(TOAST_SECS + 1);
            }
        });
        assert!(toast().is_none());
    }

    #[test]
    fn actions_raise_toasts() {
        with(|c| *c = Canvas::default());
        toggle_bookmark();
        assert!(toast().unwrap().contains("북마크 추가"));
        toggle_bookmark();
        assert!(toast().unwrap().contains("북마크 제거"));
        clear_ink();
        assert!(toast().unwrap().contains("잉크"));
    }

    #[test]
    fn ink_defaults_roundtrip_and_fallback() {
        let path =
            std::env::temp_dir().join(format!("freedf-gui-test-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        // 1) 현재 상태 저장 → 엔진을 다르게 바꾸고 → 복원이 되돌린다.
        with(|c| *c = Canvas::default());
        select_tool("Highlighter");
        select_color("Red");
        select_width("Thick");
        save_defaults_to(&path).expect("save");
        with(|c| *c = Canvas::default());
        assert_eq!(tool_name(), "Pen");
        let restored = load_defaults_from(&path).expect("load");
        assert_eq!(restored.tool, "Highlighter");
        assert_eq!(restored.color, "Red");
        assert_eq!(restored.width, "Thick");
        assert_eq!(tool_name(), "Highlighter");
        assert_eq!(color_name(), "Red");
        assert_eq!(width_name(), "Thick");
        // 2) 손상된 파일 — 오류를 돌려주고 엔진은 그대로 (조용한 폴백은 load_defaults 몫).
        std::fs::write(&path, "not json").unwrap();
        assert!(load_defaults_from(&path).is_err());
        assert_eq!(tool_name(), "Highlighter");
        // 3) 파일에 알 수 없는 이름 — select_* 폴백으로 기본값 적용.
        std::fs::write(&path, r#"{"tool":"Warp","color":"Neon","width":"Huge"}"#).unwrap();
        let fallback = load_defaults_from(&path).expect("parse");
        assert_eq!(fallback.tool, "Warp"); // 저장 값은 그대로 (기록 보존)
        assert_eq!(tool_name(), "Pen"); // 적용은 폴백
        assert_eq!(color_name(), "Black");
        assert_eq!(width_name(), "Medium");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn modal_blocks_canvas_input() {
        with(|c| *c = Canvas::default());
        sync_and_status(true); // 모달 열림 — 캔버스 입력 차단

        let ctx = egui::Context::default();
        let base = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };
        let press = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let frame = |events: Vec<egui::Event>| {
            let mut input = base();
            input.events = events;
            let mut out = ctx.run_ui(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| paint(ui));
            });
            out.textures_delta.clear();
        };

        frame(vec![
            egui::Event::PointerMoved(egui::pos2(300.0, 300.0)),
            press(egui::pos2(300.0, 300.0), true),
        ]);
        frame(vec![egui::Event::PointerMoved(egui::pos2(350.0, 340.0))]);
        frame(vec![press(egui::pos2(350.0, 340.0), false)]);
        with(|c| {
            assert_eq!(
                c.docs[0].store.total_stroke_count(),
                0,
                "모달 중에는 획이 없어야 한다"
            )
        });

        sync_and_status(false); // 모달 닫힘 — 이제 그려진다
        frame(vec![
            egui::Event::PointerMoved(egui::pos2(300.0, 300.0)),
            press(egui::pos2(300.0, 300.0), true),
        ]);
        frame(vec![egui::Event::PointerMoved(egui::pos2(350.0, 340.0))]);
        frame(vec![press(egui::pos2(350.0, 340.0), false)]);
        with(|c| {
            assert_eq!(
                c.docs[0].store.total_stroke_count(),
                1,
                "모달이 닫히면 그려진다"
            )
        });
    }
}
