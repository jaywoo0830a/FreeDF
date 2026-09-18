//! 캔버스 — `<Raw>` 경계 뒤의 명령형 egui 렌더/입력 (Phase 2~3).
//!
//! 잉크 파이프라인 규약은 `docs/ink-pipeline-design.md`, 계측 id 계약은
//! `docs/eguidev-automation.md`를 본다.
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
//! 지우개·실행취소/다시실행·문서 저장/불러오기도 전부 **코어 API 배선**이다:
//! 지우기는 `AnnotationStore::erase_at`, 이력은 `History` + `Edit`
//! (`AnnotationStore::apply_edit`), 저장/불러오기는
//! `AnnotationStore::to_json`/`from_json`. 이 파일이 자체 상태기계를 만들지 않는다.
//!
//! 펜 필압/틸트는 [`crate::pen::PenInput`]이 **코어 공급원**(OTD 데몬 RPC →
//! evdev → 스트림 없음)에서 폴링해 `InkPipeline`/메셔/커서로 흘려보낸다.
//! 도구별 커서 스프라이트는 [`crate::cursor`], 즐겨찾기 색 해석은
//! [`crate::palette`]가 소유한다 — 이 파일은 배선만 한다.
//!
//! v1 한계: 마우스로도 그린다(freedf의 `mouse_draws` 정책은 아직 없다).
//! 프레임마다 메시 재굽기(획 수가 커지면 freedf의 BakeService 이식).

use crate::cursor::{self, CursorSpec, CURSOR_STABLE_FRAMES};
use crate::palette;
use crate::pen::PenInput;
use eframe::egui;
use freedf_canvas::{PagePoint, ViewTransform};
use freedf_core::history::{Edit, History};
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
/// 지우개 반경 (화면 px) — 페이지 반경은 줌으로 나눠 쓴다 (freedf와 동일 규약).
pub const ERASER_RADIUS_PX: f32 = 12.0;
/// 스무딩 프리셋 — `(이름, 1€ 필터 강도)`. 강도는 `OneEuroFilter::from_smoothing`
/// 입력으로, freedf 설정의 `SmoothingStrength`(0..1, 기본 0.4)와 같은 값 공간이다.
///
/// 이름은 리본의 굵기 프리셋(`Thin`/`Medium`/`Thick`)과 **겹치지 않게** 고른다 —
/// elm-magic 셸의 클릭 대상은 라벨이라 같은 라벨이 둘이면 클릭이 모호해진다.
pub const SMOOTHING_PRESETS: [(&str, f32); 4] = [
    ("Off", 0.0),
    ("Light", 0.25),
    ("Normal", 0.4),
    ("Strong", 0.7),
];
/// 스무딩 기본값 — freedf 설정 기본과 같다(OFF: 외부 드라이버 안정화와 충돌 방지).
const SMOOTHING_DEFAULT: f32 = 0.0;
/// 새 점을 받아들이는 최소 화면 이동(px). 같은 자리의 반복 샘플(정지 중 프레임)과
/// 부화소 떨림을 걸러 리본에 길이 0 세그먼트가 쌓이지 않게 한다.
const MIN_STEP_PX: f32 = 0.75;

/// 탭 하나 = 문서 하나 — 저장소·페이지·뷰·PDF·이력을 독립으로 가진다.
pub struct Doc {
    pub id: u64,
    pub name: String,
    pub store: AnnotationStore,
    /// 문서별 실행취소/다시실행 이력 — 코어 `History`가 소유한다 (`Edit` diff 스택).
    pub history: History,
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
            history: History::default(),
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
            history: History::default(),
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
pub struct Canvas {
    /// 탭 = 문서 (테스트/자동화에서도 읽는다).
    pub docs: Vec<Doc>,
    /// 활성 문서 인덱스 (docs 내) — docs는 절대 비지 않는다.
    pub active: usize,
    next_doc_id: u64,
    pub tool: ToolType,
    pub color: [u8; 4],
    pub width: f32,
    /// 스무딩 강도(0..1) — 새 획마다 코어 `InkPipeline`에 넘긴다.
    pub smoothing: f32,
    /// 진행 중 획 — **코어의 `InkPipeline`** (1€ 필터 + 인과적 선폭 확정 +
    /// 동결 커밋). 점/폭/시각은 전부 이 객체가 소유하고, 여기서는 down/drag/up만
    /// 부른다 (도구별 분기 없음 — 재료는 `Materials::for_tool`).
    ink: Option<InkPipeline>,
    /// 지우기 세션 진행 중 — 이번 프레스에서 지운 획들(코어 `erase_at` 반환값).
    /// 펜을 떼면 **하나의 undo 단계**(`Edit::RemoveStrokes`)로 확정된다.
    erasing: Option<Vec<Stroke>>,
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
    pub toast: Option<(String, std::time::Instant)>,
    mesher: freedf_canvas::CoreRibbonMesher,
    /// 펜 입력(필압/틸트) — 코어 공급원(OTD 데몬 RPC → evdev → 없음) 배선.
    pub pen: PenInput,
    /// 필압 반영 여부 (freedf `ToolState::pressure_enabled`에 대응하는 GUI 토글).
    pub pressure_enabled: bool,
    /// 자주 쓰는 색 팔레트 — `freedf-services::settings` 기본값에서 시작한다
    /// (스와치 라벨은 `palette::label`, 해석은 `palette::parse`).
    pub favorites: Vec<[u8; 4]>,
    /// 커서 크기 배율 (freedf `global.cursor_scale`, 0.5..2.0).
    pub cursor_scale: f32,
    /// 왼손잡이 — 펜 커서 배럴이 왼쪽 반평면만 가리킨다.
    pub left_handed: bool,
    /// 커서 표시 히스테리시스 상태 — (직전 want, 연속 프레임, 표시 여부).
    cursor_want: bool,
    cursor_frames: u32,
    cursor_shown: bool,
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
            smoothing: SMOOTHING_DEFAULT,
            ink: None,
            erasing: None,
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
            // 펜 공급원 선택/폴링은 `PenInput`이 소유한다 (OTD → evdev → 없음).
            pen: PenInput::attach(),
            pressure_enabled: true,
            favorites: palette::defaults(),
            // 커서 배율/손잡이 기본값도 settings 서비스가 소유한 값에서 가져온다.
            cursor_scale: freedf_services::settings::GlobalState::default().cursor_scale,
            left_handed: freedf_services::settings::GlobalState::default().left_handed,
            cursor_want: false,
            cursor_frames: 0,
            cursor_shown: false,
        }
    }
}

thread_local! {
    static ENGINE: RefCell<Canvas> = RefCell::new(Canvas::default());
}

/// 엔진 접근 — UI 스레드 전용 (위 `<Raw>` 경계 참고).
pub fn with<R>(f: impl FnOnce(&mut Canvas) -> R) -> R {
    ENGINE.with(|c| f(&mut c.borrow_mut()))
}

impl Canvas {
    /// 활성 문서 (항상 존재 — docs는 절대 비지 않는다).
    pub fn doc(&mut self) -> &mut Doc {
        &mut self.docs[self.active]
    }

    /// 메셔에 넘어간 틸트 크기 (0..1) — 펜 배선 진단/자동화용 읽기 창구.
    pub fn tilt_magnitude(&self) -> f32 {
        self.mesher.tilt_magnitude
    }

    /// 활성 문서 읽기 전용 — 렌더 경로용.
    fn doc_ref(&self) -> &Doc {
        &self.docs[self.active]
    }

    /// 진행 중 획을 **코어에서 동결**(마지막 폭 확정 + 불변 `Stroke`)해 활성
    /// 문서에 기록하고, 이력에 `Edit::AddStrokes`를 쌓는다 (탭 전환/닫기 전에도
    /// 호출된다).
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
        let page = doc.page;
        let id = doc
            .store
            .add_stroke(page, stroke.tool, stroke.color, stroke.width, stroke.points);
        // 이력에는 **저장소가 부여한 id를 가진** 스트로크가 들어가야 undo/redo의
        // RemoveStrokes가 같은 대상을 가리킨다.
        if let Some(committed) = doc.store.stroke(page, id).cloned() {
            doc.history.push(Edit::AddStrokes {
                page,
                strokes: vec![committed],
            });
        }
    }

    /// 진행 중인 입력(획/지우기)을 버린다 — 문서를 닫을 때 등 (잘못된 문서에
    /// 기록 방지).
    fn drop_active(&mut self) {
        self.ink = None;
        self.erasing = None;
    }

    /// 지우개 한 점 — 코어 `AnnotationStore::erase_at`을 그대로 쓴다.
    /// 이번 세션에서 새로 지워진 획은 모아 두었다가 펜을 뗄 때 한 단계로 기록한다.
    fn erase_at(&mut self, pos: egui::Pos2, origin: egui::Pos2) {
        if self.erasing.is_none() {
            return;
        }
        let p = self.page_pos(pos, origin);
        let zoom = self.doc_ref().view.zoom.max(1e-3);
        let radius = ERASER_RADIUS_PX / zoom; // 화면 px → 페이지 pt
        let page = self.doc().page;
        let removed = self.doc().store.erase_at(page, [p.x, p.y], radius);
        if let Some(erased) = self.erasing.as_mut() {
            erased.extend(removed);
        }
    }

    /// 지우기 세션 종료 — 모아 둔 획들을 **하나의 undo 단계**로 기록한다.
    fn finish_erase(&mut self) {
        let Some(erased) = self.erasing.take() else {
            return;
        };
        if erased.is_empty() {
            return;
        }
        let doc = self.doc();
        let page = doc.page;
        doc.history.push(Edit::RemoveStrokes {
            page,
            strokes: erased,
        });
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
///
/// 제거된 획을 **하나의 `Edit::RemoveStrokes`**로 이력에 남긴다 (undo 가능).
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
        let removed = c.doc().store.remove_strokes(page, &ids);
        if !removed.is_empty() {
            c.doc().history.push(Edit::RemoveStrokes {
                page,
                strokes: removed,
            });
        }
        c.toast_now(String::from("페이지 잉크를 지웠습니다"));
    });
}

// ── 실행취소/다시실행 — 코어 `History` + `AnnotationStore::apply_edit` ──

/// 실행취소: 이력의 역연산을 저장소에 적용한다 (활성 문서 기준).
pub fn undo() {
    with(|c| {
        let doc = c.doc();
        let Some(edit) = doc.history.undo() else {
            c.toast_now(String::from("되돌릴 작업이 없습니다"));
            return;
        };
        doc.store.apply_edit(&edit);
        c.toast_now(String::from("실행취소"));
    });
}

/// 다시실행: undo로 되돌린 연산을 다시 적용한다.
pub fn redo() {
    with(|c| {
        let doc = c.doc();
        let Some(edit) = doc.history.redo() else {
            c.toast_now(String::from("다시 실행할 작업이 없습니다"));
            return;
        };
        doc.store.apply_edit(&edit);
        c.toast_now(String::from("다시 실행"));
    });
}

/// 되돌릴 작업이 있는지 (툴바 활성 표시용).
pub fn can_undo() -> bool {
    with(|c| c.doc().history.can_undo())
}

/// 다시 실행할 작업이 있는지.
pub fn can_redo() -> bool {
    with(|c| c.doc().history.can_redo())
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
pub const TOAST_SECS: u64 = 3;

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

/// 즐겨찾기 색상 선택 — 이름(Black/Red/Blue) / `#RRGGBB` / 스와치 라벨("Swatch 2").
/// 알 수 없는 값은 팔레트 첫 색(기본 Black)으로 폴백한다 (설정 파일 호환).
pub fn select_color(name: &str) {
    let color = resolve_color(name);
    with(|c| c.color = color);
}

/// 문자열 → 색: 이름/HEX → 스와치 라벨 → 팔레트 첫 색.
fn resolve_color(name: &str) -> [u8; 4] {
    if let Some(color) = palette::parse(name) {
        return color;
    }
    if let Some(i) = palette::label_index(name) {
        if let Some(color) = with(|c| c.favorites.get(i).copied()) {
            return color;
        }
    }
    with(|c| c.favorites.first().copied()).unwrap_or([26, 26, 28, 255])
}

/// 현재 색상 이름 — 기본 3색은 이름, 그 외는 `#RRGGBB` (`palette::name`).
pub fn color_name() -> String {
    with(|c| palette::name(c.color))
}

/// 스와치 색 목록 — 셸 리본이 매 프레임 읽는다 (settings 기본 팔레트에서 시작).
pub fn palette_colors() -> Vec<[u8; 4]> {
    with(|c| c.favorites.clone())
}

/// 스와치 라벨 = 계약 id의 근거 (`Swatch 1` → `gui.swatch_1`).
pub fn swatch_label(index: usize) -> String {
    palette::label(index)
}

/// 스와치 클릭 → 그 색 선택 (범위 밖 인덱스는 무시).
pub fn select_swatch(index: usize) {
    with(|c| {
        if let Some(color) = c.favorites.get(index).copied() {
            c.color = color;
        }
    });
}

/// 현재 색이 팔레트의 몇 번째인가 (활성 표시용, 없으면 `None`).
pub fn color_swatch_index() -> Option<usize> {
    with(|c| palette::index_of(&c.favorites, c.color))
}

/// 즐겨찾기 색 목록 복원 — 해석 불가한 항목은 버리고, 비면 기본 팔레트.
pub fn set_favorites(names: &[String]) {
    let colors: Vec<[u8; 4]> = names.iter().filter_map(|n| palette::parse(n)).collect();
    with(|c| c.favorites = palette::normalize(colors));
}

/// 필압 반영 토글 (freedf `ToolState::pressure_enabled`에 대응).
pub fn toggle_pressure() {
    with(|c| c.pressure_enabled = !c.pressure_enabled);
}

/// 필압 반영을 직접 지정 (설정 복원).
pub fn set_pressure(on: bool) {
    with(|c| c.pressure_enabled = on);
}

/// 필압 반영 여부 (리본/설정 표시용).
pub fn pressure_enabled() -> bool {
    with(|c| c.pressure_enabled)
}

/// 펜 스트림 출처 라벨 ("OTD"/"evdev"/"none") — 상태/설정 표시용.
pub fn pen_source() -> String {
    with(|c| String::from(c.pen.source().label()))
}

/// 펜 장치가 틸트를 보고하는가 (설정 창 진단 — 스트림 존재와는 다른 질문).
pub fn pen_tilt_supported() -> bool {
    with(|c| c.pen.tilt_supported())
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

/// 스무딩 프리셋 강도 (알 수 없는 이름은 기본값).
fn smoothing_value(name: &str) -> f32 {
    SMOOTHING_PRESETS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, v)| *v)
        .unwrap_or(SMOOTHING_DEFAULT)
}

/// 스무딩 프리셋 이름 (강도가 프리셋과 다르면 "Custom").
pub fn smoothing_name_for(value: f32) -> String {
    SMOOTHING_PRESETS
        .iter()
        .find(|(_, v)| (v - value).abs() < 1e-3)
        .map(|(n, _)| String::from(*n))
        .unwrap_or_else(|| String::from("Custom"))
}

/// 스무딩 선택 (Off/Light/Medium/Strong — 그 외 값은 Off).
/// 강도는 **코어 `InkPipeline`**에 그대로 넘어간다 (1€ 필터).
pub fn select_smoothing(name: &str) {
    let value = smoothing_value(name);
    with(|c| c.smoothing = value);
}

/// 현재 스무딩 프리셋 이름 (설정 창 표시용).
pub fn smoothing_name() -> String {
    with(|c| smoothing_name_for(c.smoothing))
}

// ── 문서 저장/불러오기 — 코어 `AnnotationStore` JSON ────────────────────
//
// freedf는 서버(StorageBackend)에 저장하지만, freedf-gui v1은 코어의
// `to_json`/`from_json`을 써서 **활성 문서의 주석**을 app_data_dir의 파일로
// 왕복한다 (저장소 계층 이식 전의 최소 경로).

/// 활성 문서의 주석 파일 경로 — `<app_data_dir>/gui-doc-<id>.json`.
pub fn doc_path(id: u64) -> std::path::PathBuf {
    freedf_services::storage::app_data_dir().join(format!("gui-doc-{id}.json"))
}

/// 활성 문서의 주석을 지정 경로에 저장한다 (테스트 가능한 순수 경로 버전).
pub fn save_doc_to(path: &std::path::Path) -> Result<(), String> {
    let json = with(|c| c.doc().store.to_json());
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, json).map_err(|e| e.to_string())
}

/// 지정 경로의 주석을 **활성 문서에** 불러온다 (이력은 초기화 — 새 문서 기준).
pub fn load_doc_from(path: &std::path::Path) -> Result<(), String> {
    let json = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let store = AnnotationStore::from_json(&json).map_err(|e| e.to_string())?;
    with(|c| {
        let doc = c.doc();
        doc.store = store;
        doc.history.clear();
        doc.page_tex = None;
        doc.tex_attempted = None;
    });
    Ok(())
}

/// 활성 문서를 기본 경로(`doc_path(id)`)에 저장 (성공/실패 토스트).
pub fn save_edits() {
    let path = with(|c| doc_path(c.doc().id));
    let result = save_doc_to(&path);
    with(|c| {
        c.toast_now(match result {
            Ok(()) => String::from("문서를 저장했습니다"),
            Err(e) => format!("저장 실패: {e}"),
        });
    });
}

/// 활성 문서를 기본 경로에서 불러온다 (성공/실패 토스트).
pub fn load_edits() {
    let path = with(|c| doc_path(c.doc().id));
    let result = load_doc_from(&path);
    with(|c| {
        c.toast_now(match result {
            Ok(()) => String::from("문서를 불러왔습니다"),
            Err(e) => format!("불러오기 실패: {e}"),
        });
    });
}

// ── 설정 — 잉크 기본값 저장/복원 (파일 백엔드: app_data_dir의 JSON) ──────

/// 잉크 기본값 — 리본의 **표시 이름**을 그대로 저장한다. 복원은
/// `select_*` 커맨드를 거치므로 알 수 없는 값은 자동으로 기본값 폴백된다.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct InkDefaults {
    pub tool: String,
    pub color: String,
    pub width: String,
    /// 스무딩 프리셋 이름 — 이전 형식 파일에는 없다(기본값 사용).
    #[serde(default)]
    pub smoothing: String,
    /// 필압 반영 여부 — 이전 형식 파일에는 없다(기본 켜짐).
    #[serde(default = "default_true")]
    pub pressure: bool,
    /// 즐겨찾기 색 목록(이름 또는 `#RRGGBB`) — 이전 형식에는 없다(기본 팔레트).
    #[serde(default)]
    pub favorites: Vec<String>,
}

/// serde 기본값 — `InkDefaults::pressure`.
fn default_true() -> bool {
    true
}

/// 설정 파일 경로 — `<app_data_dir>/gui-ink-defaults.json`.
/// (Windows: `%LOCALAPPDATA%\FreeDF`, Linux/macOS: `~/.local/share/freedf`)
pub fn settings_path() -> std::path::PathBuf {
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
pub fn save_defaults_to(path: &std::path::Path) -> Result<(), String> {
    let defaults = InkDefaults {
        tool: tool_name(),
        color: color_name(),
        width: width_name(),
        smoothing: smoothing_name(),
        pressure: pressure_enabled(),
        favorites: palette_colors().iter().map(|c| palette::name(*c)).collect(),
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
pub fn load_defaults_from(path: &std::path::Path) -> Result<InkDefaults, String> {
    let json = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let defaults: InkDefaults = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    // 팔레트를 **먼저** 복원한다 — 색 이름/스와치 라벨 해석의 기준이 팔레트라서
    // (팔레트가 바뀐 뒤에 색을 골라야 "기본 팔레트 첫 색" 폴백이 맞는다).
    set_favorites(&defaults.favorites);
    set_pressure(defaults.pressure);
    // 적용은 select_* 커맨드 경로 — 알 수 없는 이름은 기본값 폴백이 이미 내장.
    select_tool(&defaults.tool);
    select_color(&defaults.color);
    select_width(&defaults.width);
    // 스무딩은 빈 문자열(이전 형식)이면 기본값 유지.
    if !defaults.smoothing.is_empty() {
        select_smoothing(&defaults.smoothing);
    }
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
pub struct OutlineEntry {
    pub title: String,
    pub page: usize,
}

/// PDF 북마크 트리를 패널 렌더용으로 평탄화한다 (순수 함수 — 테스트 대상).
pub fn flatten_outline(
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
        let pen = String::from(c.pen.source().label());
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
        format!("{base}줌 {zoom_pct}% · 획 {strokes} · 북마크 {bookmarks} · 펜 {pen}")
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

        // 펜 스트림 폴링 (프레임당 1회) → 메셔 틸트/압력의 원천.
        self.pen.poll();
        self.mesher.tilt_magnitude = self.pen.tilt_unit();

        // PDF 텍스처 (필요하면 이번 프레임에 렌더 — pdfium이 있을 때만).
        self.ensure_page_texture(ui.ctx());

        let origin = self.doc().page_origin(rect);
        self.draw_paper(&painter, origin);
        self.draw_pdf(&painter, origin);
        self.paint_committed(&painter, origin);
        self.paint_active(&painter, origin);
        self.handle_input(ui, rect, origin);
        // 도구별 커서 — 캔버스 위에서만 시스템 커서를 숨기고 직접 그린다.
        self.paint_cursor(ui, &painter, rect);
        self.last_pointer = ui.input(|i| i.pointer.latest_pos());
    }

    /// 도구별 커서 스프라이트 (freedf `paint_custom_cursor` 이식).
    ///
    /// 캔버스 안에 포인터가 있으면 시스템 커서를 숨기고(`CursorIcon::None`)
    /// 직접 그린다. 표시 여부는 3프레임 히스테리시스 — 캔버스 경계를 드나들 때
    /// 시스템 커서와 겹쳐 깜빡이지 않는다. 캔버스 밖으로 나가면 상태를 리셋해
    /// 다음 진입이 즉시 반영되게 한다.
    fn paint_cursor(&mut self, ui: &mut egui::Ui, painter: &egui::Painter, rect: egui::Rect) {
        let (pos, time) = ui.input(|i| (i.pointer.latest_pos(), i.time));
        // 커서는 입력이 살아 있을 때만(모달/자동화 캡처 중에는 OS 커서를 만지지 않는다).
        let want = self.input_enabled && pos.map(|p| rect.contains(p)).unwrap_or(false);
        if !want {
            // 캔버스 밖/입력 차단 — 상태 리셋 + 시스템 커서 복원.
            self.cursor_want = false;
            self.cursor_frames = 0;
            self.cursor_shown = false;
            return;
        }
        let (frames, shown) = cursor::hysteresis(
            self.cursor_want,
            want,
            self.cursor_frames,
            self.cursor_shown,
            CURSOR_STABLE_FRAMES,
        );
        self.cursor_want = want;
        self.cursor_frames = frames;
        self.cursor_shown = shown;
        if !shown {
            return;
        }
        let Some(pos) = pos else { return };
        // 도구는 CursorSpec이 나른다 — 스프라이트는 cursor.rs가 소유.
        let spec = CursorSpec {
            tool: self.tool,
            color: self.color,
            width_pt: self.width,
            zoom: self.doc_ref().view.zoom,
            eraser_radius_px: ERASER_RADIUS_PX,
            // 능력 질의 — 틸트 미지원 장치는 None (손잡이 기본 방위각 폴백).
            tilt: self.pen.cursor_azimuth(),
            left_handed: self.left_handed,
            scale: self.cursor_scale.clamp(0.5, 2.0),
            proximity: self.pen.proximity(now_ms()),
        };
        cursor::paint(painter, pos, time as f32, &spec);
        // 스프라이트가 커서 노릇을 하므로 OS 커서는 숨긴다.
        ui.ctx().set_cursor_icon(egui::CursorIcon::None);
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

        // ── 왼쪽 버튼 = 잉크/지우개 (시작은 페이지 안에서만) ──
        //
        // 세션 수명은 **코어**가 소유한다: 잉크는 `InkPipeline`(down→drag→up),
        // 지우개는 `AnnotationStore::erase_at`. 여기서는 화면→페이지 변환과
        // "페이지 안" 판정만 한다 (도구 판정도 코어의 `ToolType::is_ink`).
        let in_page = pos.map(|p| page.contains(p)).unwrap_or(false);
        if primary_pressed && enabled && in_page {
            if let Some(p) = pos {
                if self.tool.is_ink() {
                    let page_pt = self.page_pos(p, origin);
                    let mut pipeline =
                        InkPipeline::new(self.mesher.materials, self.width, self.smoothing);
                    // 압력/틸트는 **펜 장치가 보고한 값**(OTD RPC/evdev) — 스트림이
                    // 없으면 압력 1.0/틸트 0 (마우스/트랙패드 환경이 정상이다).
                    let pressure = self.pen.pressure(self.pressure_enabled);
                    let tilt = self.pen.tilt_magnitude();
                    pipeline.down(
                        self.tool,
                        self.color,
                        page_pt.x,
                        page_pt.y,
                        pressure,
                        time,
                        now_ms(),
                        tilt,
                    );
                    self.ink = Some(pipeline);
                } else if self.tool == ToolType::Eraser {
                    self.erasing = Some(Vec::new());
                    self.erase_at(p, origin);
                }
            }
        } else if primary_down && in_page {
            if let Some(p) = pos {
                if self.erasing.is_some() {
                    self.erase_at(p, origin);
                } else if self.ink.is_some() {
                    let zoom = self.doc_ref().view.zoom;
                    let page_pt = self.page_pos(p, origin);
                    // 같은 자리 반복 샘플/부화소 떨림은 버린다 (길이 0 세그먼트 방지).
                    let moved = self
                        .ink
                        .as_ref()
                        .and_then(|pipe| pipe.live())
                        .and_then(|live| live.points.last())
                        .map(|last| {
                            let dx = page_pt.x - last.x;
                            let dy = page_pt.y - last.y;
                            (dx * dx + dy * dy).sqrt() * zoom > MIN_STEP_PX
                        })
                        .unwrap_or(false);
                    if moved {
                        let pressure = self.pen.pressure(self.pressure_enabled);
                        if let Some(pipe) = self.ink.as_mut() {
                            pipe.drag(page_pt.x, page_pt.y, pressure, time, now_ms());
                        }
                    }
                }
            }
        }
        if primary_released || !primary_down {
            self.finish_active();
            self.finish_erase();
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
pub fn zoom_at_view(view: ViewTransform, pointer_px: [f32; 2], factor: f32) -> ViewTransform {
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

/// 현재 시각 (유닉스 epoch ms) — 점 시각/블리드 나이 계산용 (테스트에서도 쓴다).
pub fn now_ms() -> u64 {
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
