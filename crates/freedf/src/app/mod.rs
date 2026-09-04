//! FreeDF main app: PDF viewer canvas + drawing-pad annotation + notes/outline/search.
//!
//! English-only UI. Screen coordinates map 1:1 to egui's canvas space; the canvas
//! top-left equals `response.rect.min`, and page <-> view coordinates are handled
//! by `freedf_core::transform::ViewTransform`.
//!
//! # Layout (layered refactor)
//!
//! This file (`app/mod.rs`) is the root of the `app` module: it owns the app
//! **state** (`FreeDfApp` struct), the `eframe::App` glue, session persistence,
//! shared helper functions and the small types used everywhere. The bulk of the
//! behavior lives in focused submodules so no single file stays unwieldy:
//!
//! - [`tabs`] — tab strip UI + tab lifecycle (open / switch / close) + detach
//!   to a separate OS window
//! - [`toolbar`] — three-tier toolbar, drawing-tool picker (drag to reorder),
//!   per-tool settings
//! - [`panels`] — Library (Notes / PDFs / Recents) and Outline side panels
//! - [`canvas`] — page canvas: pan / zoom / drawing input, page painting,
//!   text-aware highlights, palette & navigation overlays, custom cursors
//! - [`actions`] — document & note actions: open / save, page CRUD,
//!   rotation, search, bookmarks, undo / redo
//!
//! The child modules each `use super::*;` — they extend `FreeDfApp` with more
//! inherent methods, so call sites keep working exactly as before.

mod actions;
pub(crate) mod canvas;
mod dictionary;
mod input;
mod panels;
mod tabs;
mod toolbar;

pub(crate) use std::path::{Path, PathBuf};

pub(crate) use eframe::egui;
pub(crate) use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};

pub(crate) use freedf_core::history::{Edit, History};
pub(crate) use freedf_core::logging::{AppEvent, Logger};
pub(crate) use freedf_core::model::{PageIndex, StrokePoint, ToolType};
pub(crate) use freedf_core::notes::NotesManager;
pub(crate) use freedf_core::outline::{flatten, OutlineNode};
pub(crate) use freedf_core::paper::{
    clamp_line_width, clamp_spacing, paper_dots, paper_lines_rotated, PagePaper, PaperSize, PaperStyle,
    PaperStyleSettings, PAPER_COLORS, PAPER_WHITE,
};
pub(crate) use freedf_core::pen::{
    BallPenProfile, ColorFamily, FountainProfile, InkSoak, OneEuroFilter, Palette,
};
pub(crate) use freedf_core::ink::{combine_saturation, stroke_ink_lr, InkGrain};
pub(crate) use freedf_core::search::{find_matches, TextMatch, TextRun};
pub(crate) use freedf_core::text::char_line_highlights;

/// 현재 시각 (유닉스 epoch ms) — 잉크 번짐 나이 계산용.
pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 현재 시각 (초, f64) — 로딩 경과 시간 표시용.
fn now_secs() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}
pub(crate) use freedf_core::store::AnnotationStore;
pub(crate) use freedf_core::transform::{PageAlign, ViewTransform, MAX_ZOOM, MIN_ZOOM, ZOOM_100_PERCENT};

pub(crate) use crate::storage::{DocRow, SharedStorage, StorageBackend};
pub(crate) use dictionary::Dictionary;
pub(crate) use crate::pdf::DocumentView;
pub(crate) use crate::recent::{RecentItem, RecentKind, RecentList};
pub(crate) use crate::server::{MediaClient, MediaObject, MediaServerConfig};
pub(crate) use crate::settings::MAX_FAVORITE_COLORS;
pub(crate) use egui_phosphor_icons::icons;
pub(crate) use pdfium_render::prelude::Pdfium;
use std::collections::HashSet;

/// Canvas margin around the page
const CANVAS_MARGIN: f32 = 16.0;
/// Page top margin
const TOP_MARGIN: f32 = 16.0;
/// 스트로크 id 풀 배치 크기 — 한 번의 원격 왕복으로 이만큼씩 미리 받아둡니다.
/// (풀 절반 이하에서 미리 보충 예약 — 소진 시 UI 왕복 없음)
const STROKE_ID_POOL_BATCH: usize = 256;
/// Page transition animation duration (seconds)
const PAGE_ANIM_SECS: f32 = 0.28;
/// Window width (points) below which the UI collapses to canvas + palette
/// (Windows split view / narrow multitasking), with a floating control to
/// re-show the full chrome on demand.
const COMPACT_MIN_WIDTH: f32 = 640.0;
/// Smoothing rate (1/second) for animated wheel scroll.
const SCROLL_SMOOTH_RATE: f32 = 14.0;
/// 줌 한 스텝 = **5%**. PDF 렌더러 특성상 연속(애니메이션) 줌은 매 프레임
/// 재래스터로 렉이 걸리므로, 모든 줌 입력(버튼/Ctrl+휠/핀치/단축키)을
/// 이 고정 스텝으로 양자화해 한 번에 적용합니다.
const ZOOM_STEP: f32 = 1.05;

/// Fit mode
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum FitMode {
    /// Fit page width
    Width,
    /// Fit page height
    Height,
}

/// In-progress page transition (slide).
pub(crate) struct PageAnim {
    /// 0.0 (start) .. 1.0 (done)
    progress: f32,
    /// +1.0 = next page (slides in from the right), -1.0 = previous (from the left)
    direction: f32,
    /// 세로(위/아래) 전환 여부 — PgUp/PgDn 키 전용. false면 기존 가로 슬라이드.
    vertical: bool,
}

/// A stroke currently being drawn
#[derive(Clone)]
pub(crate) struct ActiveStroke {
    tool: ToolType,
    color: [u8; 4],
    width: f32,
    points: Vec<StrokePoint>,
}

impl ActiveStroke {
    fn push(&mut self, point: [f32; 2], pressure: f32, t_ms: u64) {
        self.points
            .push(StrokePoint::with_time(point[0], point[1], pressure, t_ms));
    }
}

fn tool_label(tool: ToolType) -> &'static str {
    match tool {
        ToolType::Pen => "Pen",
        ToolType::Fountain => "Fountain",
        ToolType::Highlighter => "Highlighter",
        ToolType::Eraser => "Eraser",
        ToolType::Pan => "Pan",
    }
}

impl FreeDfApp {
    /// 스토어를 통째로 교체합니다 (문서 열기/탭 전환) — 세대를 올려 이전
    /// 문서의 병합 잉크 메시가 재사용되지 않게 합니다.
    pub(crate) fn set_store(&mut self, store: AnnotationStore) {
        self.store_generation = self.store_generation.wrapping_add(1);
        self.ink_mesh = None;
        self.ink_next_settle_ms = u64::MAX;
        self.store = store;
    }
}

fn tool_icon(tool: ToolType) -> egui_phosphor_icons::Icon {
    match tool {
        ToolType::Pen => icons::PEN,
        ToolType::Fountain => icons::PEN_NIB,
        ToolType::Highlighter => icons::MARKER_CIRCLE,
        ToolType::Eraser => icons::ERASER,
        ToolType::Pan => icons::HAND,
    }
}

/// Builds a WidgetText: a Phosphor icon glyph followed by a label, both in the
/// current theme's text color. The icon uses the Phosphor font family so the
/// glyph renders correctly; the label uses the UI font. This gives each button
/// a recognizable icon *and* a text label (WCAG: text alternative + contrast).
fn icon_text(ui: &egui::Ui, label: &str, ic: egui_phosphor_icons::Icon) -> egui::WidgetText {
    let color = ui.visuals().text_color();
    let mut job = egui::text::LayoutJob::default();
    job.append(
        ic.0,
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::new(16.0, egui::FontFamily::Name("phosphor-regular".into())),
            color,
            ..Default::default()
        },
    );
    if !label.is_empty() {
        job.append(
            &format!("  {label}"),
            0.0,
            egui::TextFormat {
                font_id: egui::FontId::proportional(14.0),
                color,
                ..Default::default()
            },
        );
    }
    job.into()
}

/// 아이콘 글리프와 제목을 **하나의 라벨**로 합칩니다 — 아이콘이 따로
/// 덩그러니 남지 않고 제목과 한 몸으로 보이게 합니다.
fn overlay_title(ui: &egui::Ui, icon: egui_phosphor_icons::Icon, title: &str) -> egui::WidgetText {
    let color = ui.visuals().text_color();
    let mut job = egui::text::LayoutJob::default();
    job.append(
        icon.0,
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::new(16.0, egui::FontFamily::Name("phosphor-regular".into())),
            color,
            ..Default::default()
        },
    );
    job.append(
        &format!("  {title}"),
        0.0,
        egui::TextFormat {
            font_id: egui::FontId::new(15.0, egui::FontFamily::Proportional),
            color,
            ..Default::default()
        },
    );
    job.into()
}

/// Library/Outline/Bookmarks **공용 오버레이 헤더** — 한 컨테이너 안에
/// 아이콘+제목(강조)+개수(약하게)를 왼쪽에, 닫기(✕)를 오른쪽 끝에 배치해
/// 균형 잡힌 한 줄로 만듭니다. 닫기를 누르면 true를 반환합니다.
fn overlay_header(
    ui: &mut egui::Ui,
    icon: egui_phosphor_icons::Icon,
    title: &str,
    count: &str,
    close_hint: &str,
) -> bool {
    let mut close = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 0.0);
        ui.label(overlay_title(ui, icon, title));
        ui.label(egui::RichText::new(count).weak().small());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .button(icon_text(ui, "", icons::X))
                .on_hover_text(close_hint)
                .clicked()
            {
                close = true;
            }
        });
    });
    ui.separator();
    close
}

/// 라이브러리 패널의 목록 행. `selected`면 강조 배경 + 테두리, 호버 시 배경.
/// 오른쪽에 약한 회색 `meta`(예: "3p", "PDF")를 붙입니다. 클릭하면 true.
fn library_row(ui: &mut egui::Ui, selected: bool, title: &str, meta: &str) -> bool {
    let height = 26.0;
    let width = ui.available_width();
    let (rect, resp) =
        ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
    let visuals = ui.visuals();
    let bg = if selected {
        visuals.selection.bg_fill
    } else if resp.hovered() {
        visuals.widgets.hovered.weak_bg_fill
    } else {
        egui::Color32::TRANSPARENT
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 6.0, bg);
    if selected {
        painter.rect_stroke(
            rect,
            6.0,
            egui::Stroke::new(1.0, visuals.selection.stroke.color),
            egui::StrokeKind::Inside,
        );
    }
    // 오른쪽 메타 폭 계산
    let meta_w = if meta.is_empty() {
        0.0
    } else {
        painter
            .layout_no_wrap(
                meta.to_string(),
                egui::FontId::proportional(11.0),
                egui::Color32::WHITE,
            )
            .rect
            .width()
    };
    // 제목: 메타와 겹치지 않게 잘라낸다.
    let max_title_w = (rect.width() - 20.0 - meta_w - 8.0).max(24.0);
    let mut t = title.to_string();
    {
        let font = egui::FontId::proportional(14.0);
        let w_of = |s: &str| {
            painter
                .layout_no_wrap(s.to_string(), font.clone(), egui::Color32::WHITE)
                .rect
                .width()
        };
        if w_of(&t) > max_title_w {
            while !t.is_empty() {
                let mut cand = t.clone();
                cand.pop();
                let cw = w_of(&format!("{cand}…"));
                if cw <= max_title_w {
                    t = format!("{cand}…");
                    break;
                }
                t = cand;
            }
        }
    }
    painter.text(
        egui::pos2(rect.left() + 10.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        t,
        egui::FontId::proportional(14.0),
        visuals.text_color(),
    );
    if !meta.is_empty() {
        painter.text(
            egui::pos2(rect.right() - 10.0, rect.center().y),
            egui::Align2::RIGHT_CENTER,
            meta,
            egui::FontId::proportional(11.0),
            visuals.weak_text_color(),
        );
    }
    resp.clicked()
}

/// Renders a left-aligned row of controls in the toolbar.
/// 내용이 창 폭을 넘으면 가로 스크롤로 접근할 수 있게 합니다 (툴바 항목이
/// 화면 밖으로 잘려 "보이지 않는 버튼"이 생기지 않도록 — 예: Color wheel).
fn toolbar_row<R>(ui: &mut egui::Ui, salt: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::ScrollArea::horizontal()
        .id_salt(("toolbar_row", salt))
        .auto_shrink([false, true])
        .show(ui, |ui| ui.horizontal(|ui| add(ui)).inner)
        .inner
}

/// 동그란 색상 스와치를 그립니다. `selected`면 강조 링, 아니면 옅은 테두리.
/// `id_salt`는 호출 지점마다 고유해야 합니다 (예: 인덱스 포함).
/// 반환된 `Response`로 클릭/우클릭을 처리합니다.
fn color_circle_swatch(
    ui: &mut egui::Ui,
    id_salt: impl egui::AsIdSalt,
    color: Color32,
    selected: bool,
) -> egui::Response {
    let size = egui::vec2(24.0, 24.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter();
    let center = rect.center();
    let r = 9.0;
    painter.circle_filled(center, r, color);
    let ring = if selected {
        ui.visuals().selection.stroke.color
    } else {
        Color32::from_gray(110)
    };
    painter.circle_stroke(center, r, egui::Stroke::new(if selected { 2.0 } else { 1.0 }, ring));
    ui.interact(rect, ui.id().with(id_salt), egui::Sense::click())
}

// ---------- Fallback dialogs (non-Windows / when no native dialog) ----------

/// 새 노트 페이지 수 프리셋 (1장 ~ 대량 노트).
const NOTE_PAGE_PRESETS: &[usize] = &[1, 100, 200, 300, 500, 1000];

// ---------- 새 창(--doc) 시작 순서 (순수 상태 머신 — 테스트 대상) ----------

/// `freedf --doc`으로 시작한 새 창이 DB 문서를 여는 순서 결정.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StartupOpenAction {
    /// 저장된 URL로 자동 연결을 시작합니다.
    Connect,
    /// 대기 (연결 시도 중).
    Wait,
    /// 문서를 엽니다.
    Open(i64),
}

/// (문서 요청 없음) → Wait. (연결 안 됨 + 시도 중 아님) → Connect,
/// (시도 중) → Wait, (연결됨) → Open. 새 창 프로세스는 연결 없이 시작하므로
/// 연결 전에 문서를 열면 "Document not found"가 나는 것을 막습니다.
pub(crate) fn startup_open_step(
    pending_doc: Option<i64>,
    db_connected: bool,
    connecting: bool,
) -> StartupOpenAction {
    match pending_doc {
        None => StartupOpenAction::Wait,
        Some(doc_id) => {
            if db_connected {
                StartupOpenAction::Open(doc_id)
            } else if connecting {
                StartupOpenAction::Wait
            } else {
                StartupOpenAction::Connect
            }
        }
    }
}

/// 펜 사이드 버튼(휠 토글)의 **창 간 격리** — 두 창이 같은 펜 장치를
/// 공유하므로, 포커스된 창만 반응합니다 (배경 창의 휠이 동시에 열리는 버그 방지).
/// `focused == None`(미확인)이면 반응하지 않습니다.
pub(crate) fn wheel_toggle_allowed(window_focused: Option<bool>) -> bool {
    window_focused == Some(true)
}

/// Window Focus의 대기 시간 판정 — 커서가 `dwell_sec` 이상 창 위에 머물렀는지.
/// (0초면 즉시, `since_ms == 0`이면 아직 머물지 않음.)
pub(crate) fn dwell_focus_due(now_ms: u64, since_ms: u64, dwell_sec: f32) -> bool {
    since_ms != 0 && now_ms.saturating_sub(since_ms) >= (dwell_sec.clamp(0.0, 5.0) * 1000.0) as u64
}

/// 커서 표시 히스테리시스 1프레임 — (새 카운터, 새 표시 상태) 반환.
/// 같은 `want`가 `stable_frames` 연속일 때만 표시 상태가 want를 따라갑니다
/// (상태가 프레임마다 뒤집혀도 깜빡임/시스템 커서와의 겹침이 없음).
pub(crate) fn cursor_hysteresis(
    prev_want: bool,
    want: bool,
    counter: u32,
    shown: bool,
    stable_frames: u32,
) -> (u32, bool) {
    // 같은 want면 누적, 바뀌면 새 실행의 1번째 프레임으로 시작.
    let counter = if want == prev_want {
        (counter + 1).min(stable_frames)
    } else {
        1
    };
    let shown = if counter >= stable_frames { want } else { shown };
    (counter, shown)
}

/// 사이드 패널 종류 — Library / Outline / Bookmarks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PanelKind {
    Library,
    Outline,
    Bookmarks,
}

/// Library / Outline / Bookmarks의 **상호 베타성** — `panel`을 켜면 나머지는
/// 꺼집니다. Hide UI 트리거와 툴바 Row1 토글 양쪽에서 동일하게 사용합니다.
/// 반환: [library, outline, bookmarks].
pub(crate) fn exclusive_panel_on(panel: PanelKind) -> [bool; 3] {
    match panel {
        PanelKind::Library => [true, false, false],
        PanelKind::Outline => [false, true, false],
        PanelKind::Bookmarks => [false, false, true],
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum TextAction {
    NewNote,
    RenameNote,
    /// 외부 PDF 경로 입력 폴백 — 비 Windows에서만 구성됨.
    #[cfg_attr(target_os = "windows", allow(dead_code))]
    OpenPdf,
    /// 미디어(녹음) 업로드 — 비 Windows에서 경로 입력 폴백.
    #[cfg_attr(target_os = "windows", allow(dead_code))]
    UploadMedia,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ConfirmAction {
    DeleteNote,
    /// Library 다중 삭제 (documents.id 기준).
    DeleteLibrary { notes: Vec<i64>, pdfs: Vec<i64> },
}

/// 새 빈 페이지를 삽입하는 위치/방식.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum InsertTarget {
    /// 현재 페이지의 크기/용지를 그대로 써서 바로 다음에 삽입
    FromCurrent,
    /// 문서 맨 앞(0번)에 삽입
    AtVeryFront,
    /// 문서 맨 끝에 삽입
    AtVeryBack,
    /// 현재 페이지 앞에 삽입
    BeforeCurrent,
    /// 현재 페이지 뒤에 삽입
    AfterCurrent,
}

#[derive(Debug, Clone)]
pub(crate) enum ModalKind {
    AskText {
        title: String,
        hint: String,
        action: TextAction,
    },
    Confirm {
        title: String,
        message: String,
        action: ConfirmAction,
    },
    /// Non-blocking error popup.
    Alert {
        title: String,
        message: String,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct ModalState {
    kind: ModalKind,
    text: String,
    /// 새 노트 페이지 수 프리셋 선택 (NewNote 전용).
    pages: usize,
}

impl ModalState {
    fn ask_text(title: &str, hint: &str, action: TextAction) -> Self {
        Self {
            kind: ModalKind::AskText {
                title: title.into(),
                hint: hint.into(),
                action,
            },
            text: String::new(),
            pages: 1,
        }
    }

    /// 새 노트 전용 — 페이지 수 프리셋(기본 100장) 선택 UI가 함께 표시됩니다.
    fn ask_new_note() -> Self {
        Self {
            kind: ModalKind::AskText {
                title: "New Note".into(),
                hint: "Note title:".into(),
                action: TextAction::NewNote,
            },
            text: String::new(),
            pages: 100,
        }
    }

    fn confirm(title: &str, message: &str, action: ConfirmAction) -> Self {
        Self {
            kind: ModalKind::Confirm {
                title: title.into(),
                message: message.into(),
                action,
            },
            text: String::new(),
            pages: 1,
        }
    }

    fn alert(title: &str, message: &str) -> Self {
        Self {
            kind: ModalKind::Alert {
                title: title.into(),
                message: message.into(),
            },
            text: String::new(),
            pages: 1,
        }
    }
}

/// 문서 열기의 DB 부분 결과 (백그라운드 로더 — UI를 막지 않음).
pub(crate) struct LoaderBundle {
    pub doc_id: i64,
    pub is_note: bool,
    pub row: DocRow,
    pub pdf_bytes: Vec<u8>,
    pub store: AnnotationStore,
    pub edits: Vec<Edit>,
    pub session: Option<serde_json::Value>,
}

/// 미디어 작업 결과.
pub(crate) enum MediaOutcome {
    Listed(Vec<MediaObject>),
    Uploaded(MediaObject),
    Deleted,
}

/// 문서 열기 로더 진행 메시지 — 단계(무슨 데이터를 가져오는지) + 완료.
pub(crate) enum LoaderMsg {
    Stage(String),
    Done(Result<LoaderBundle, String>),
}

/// 백그라운드 저장 진행 메시지 — 단계(무슨 패킷을 보내는지) + 완료.
pub(crate) enum SaveMsg {
    Stage(String),
    Done(Result<(), String>),
}

/// 라이브러리 삭제(원격 DB 왕복) 작업 — 로컬 상태는 즉시 반영하고
/// 원격 삭제만 백그라운드로 보냅니다 (UI 스레드 블로킹 방지).
pub(crate) enum LibraryJob {
    DeleteNote { doc_id: i64, title: String },
    DeletePdf { doc_id: i64, name: String },
}

/// 라이브러리 삭제 작업 결과 (순서대로 도착).
pub(crate) enum LibraryOutcome {
    Done(LibraryJob, Result<(), String>),
}

/// 외부 PDF import(파일 읽기 → DB 업로드) 진행 — 완료되면 `open_document`로 연결.
pub(crate) enum PdfImportMsg {
    Done(Result<i64, String>),
}

/// 열려 있는 문서 탭의 종류 — 둘 다 DB의 `documents.id`입니다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabKind {
    /// FreeDF 노트 (documents.kind = 'note').
    Note(i64),
    /// 외부 PDF (documents.kind = 'pdf').
    Pdf(i64),
}

/// 하나의 열린 문서에 대한 전체 상태.
///
/// 앱의 활성 문서 상태(`self.document`, `self.store`, …)는 항상
/// `tabs[active]`와 동기화됩니다. 활성 탭은 `document`가 `None`이고
/// (실제 핸들은 `self.document`에 있음), 비활성 탭은 `document`가 `Some`입니다.
/// 탭 전환 시 capture/restore로 상태를 주고받습니다.
pub struct TabEntry {
    kind: TabKind,
    label: String,
    current_note: Option<i64>,
    /// 활성 탭이 아니면 실제 문서 핸들 보관.
    document: Option<DocumentView>,
    current_page: usize,
    page_size_pts: [f32; 2],
    view: ViewTransform,
    page_align: PageAlign,
    store: AnnotationStore,
    history: History,
    search_query: String,
    search_matches: Vec<TextMatch>,
    search_current: Option<usize>,
    outline: Vec<OutlineNode>,
    outline_loaded: bool,
    // ---------- Per-tab UI state (independent on switch) ----------
    show_library: bool,
    show_outline: bool,
    show_search: bool,
    tool: ToolType,
    color_family: ColorFamily,
    pen_color: [u8; 4],
    pen_width: f32,
    fountain_color: [u8; 4],
    fountain_width: f32,
    hi_color: [u8; 4],
    hi_width: f32,
    eraser_radius: f32,
    pressure_enabled: bool,
    /// 일반 펜(볼펜/젤펜) 물리 모델 프로파일.
    pen_profile: BallPenProfile,
    paper_style: PaperStyle,
    paper_color: [u8; 4],
    paper_size: PaperSize,
    /// 캔버스(페이지 뒤 서라운드) 배경색 — 탭별 상태.
    canvas_color: [u8; 4],
    /// 스타일별(Ruled/Grid/Dotted) 줄/점 세부설정 프리셋 — 각 스타일 독립.
    paper_style_settings: PaperStyleSettings,
    /// 사용자 정의 용지 크기 [가로, 세로] (pt, `PaperSize::Custom`일 때)
    custom_paper_size: [f32; 2],
    /// 펜 입력 스무딩 강도 0..1
    smoothing: f32,
    /// 스무딩 사용 여부 (기본 off)
    smoothing_enabled: bool,
    /// 잉크 스밈(진해짐) — 볼펜 (은은하게)
    pen_soak: InkSoak,
    /// 잉크 스밈(진해짐) — 만년필
    fountain_soak: InkSoak,
    /// 일반 펜(볼펜) 잉크 질감
    pen_grain: InkGrain,
    /// 만년필 잉크 질감
    fountain_grain: InkGrain,
    /// 만년필 물리 모델 프로파일
    fountain_profile: FountainProfile,
    /// 줌 잠금 (휠/핀치/단축키 줌 무시)
    zoom_lock: bool,
}

/// 펜 커서 모양.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PenCursorStyle {
    /// 작은 점 (기존)
    Dot,
    /// 펜 색/굵기를 미리보는 둥근 원
    Round,
}

/// 현재 입력 장치. egui 0.36은 이벤트에 장치 필드가 없어 `Event::Touch`
/// (Windows Ink 펜) 유무로 판별합니다 — 펜이면 잉크, 아니면 팬(기본).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InputDevice {
    Pen,
    Mouse,
}

impl PenCursorStyle {
    fn label(self) -> &'static str {
        match self {
            PenCursorStyle::Dot => "Dot",
            PenCursorStyle::Round => "Round",
        }
    }

    fn all() -> [PenCursorStyle; 2] {
        [PenCursorStyle::Round, PenCursorStyle::Dot]
    }
}

pub struct FreeDfApp {
    // ---------- Notes (in-memory cache of documents.kind='note') ----------
    notes: NotesManager,
    current_note: Option<i64>,

    // ---------- Tabs (multiple open documents) ----------
    tabs: Vec<TabEntry>,
    active: usize,

    // ---------- Recent files (in-memory cache of recents table) ----------
    recents: RecentList,

    // ---------- Storage backend ----------
    /// 저장소 백엔드(트레이트 객체) — UI는 `StorageBackend`만 바라보고,
    /// 실제 구현은 `storage::from_server_config`(Sync v3 서버)가 선택합니다.
    db: std::sync::Arc<dyn StorageBackend>,

    // ---------- Document ----------
    document: Option<DocumentView>,
    /// 현재 열린 문서의 DB id (documents.id).
    doc_id: Option<i64>,
    /// PDFium loaded once at startup; reused for creating blank note PDFs.
    pdfium: Result<Box<Pdfium>, String>,
    current_page: usize,
    page_size_pts: [f32; 2],
    view: ViewTransform,
    page_align: PageAlign,
    last_canvas: [f32; 2],
    /// Canvas size from the previous frame (detects panel toggles / resizes).
    prev_canvas: [f32; 2],
    /// Canvas 좌상단(origin) 직전 프레임 좌표 — 리사이즈 시 화면 고정 보정용.
    prev_canvas_origin: egui::Pos2,
    pending_fit: Option<FitMode>,
    texture: Option<egui::TextureHandle>,
    render_dirty: bool,
    last_render_zoom: f32,
    last_render_ppp: f32,

    // ---------- Annotations ----------
    store: AnnotationStore,
    history: History,
    /// 원격 왕복을 줄이기 위한 스트로크 id 풀 [next, end) — 스트로크마다
    /// DB 시퀀스를 부르지 않고 미리 받아 둡니다.
    stroke_id_pool: (u64, u64),
    /// 백그라운드에서 미리 받아 둔 id 묶음 수신 채널 (UI 스레드 왕복 제거).
    stroke_id_refill: Option<std::sync::mpsc::Receiver<Vec<i64>>>,

    // ---------- Tools ----------
    tool: ToolType,
    color_family: ColorFamily,
    pen_color: [u8; 4],
    pen_width: f32,
    /// 만년필 전용 색/두께 — 볼펜과 완전히 독립.
    fountain_color: [u8; 4],
    fountain_width: f32,
    hi_color: [u8; 4],
    hi_width: f32,
    eraser_radius: f32,
    pressure_enabled: bool,
    /// 디버그 HUD(실시간 입력값 오버레이) 표시 여부.
    debug_hud: bool,
    /// 왼손잡이 여부 — 펜 커서 배럴 방향을 왼쪽 반평면으로 제한.
    left_handed: bool,
    /// 일반 펜(볼펜/젤펜) 물리 모델 프로파일.
    pen_profile: BallPenProfile,
    /// 펜 커서 모양 (펜 도구일 때)
    pen_cursor_style: PenCursorStyle,
    /// 도구 선택기 순서 (드래그 앤 드롭 재정렬)
    tool_order: Vec<ToolType>,
    /// 드래그 앤 드롭 상태 (임시)
    tool_drag: Option<usize>,
    tool_drop: Option<usize>,
    /// 도구별 세부 설정 플로팅 창 표시 여부 (툴바 Settings 버튼, 임시)
    tool_settings_open: bool,
    /// Paper 세부 설정 플로팅 창 표시 여부 (임시)
    paper_settings_open: bool,
    /// Canvas(서라운드 배경색) 설정 플로팅 창 표시 여부 (임시)
    canvas_settings_open: bool,
    /// Color wheel(원형 팔레트 색 지정) 설정 플로팅 창 표시 여부 (임시)
    wheel_settings_open: bool,
    /// Insert Page 플로팅 창 표시 여부 (임시 — 메뉴 대신 창이라 타이핑 유지)
    insert_page_open: bool,
    /// 마지막으로 감지된 입력 장치 (펜/마우스)
    input_device: InputDevice,
    /// 마지막 Windows Ink 터치 시각 (초) — 펜→마우스 전환 유예 판정용.
    last_touch_time: Option<f64>,
    /// 마우스/트랙패드로도 잉크를 그릴지 (기본 off — 펜 전용 필기)
    mouse_draws: bool,
    /// 펜 입력 스무딩(안정화) 사용 여부 (기본 off — OTD 등 드라이버로
    /// 안정화하는 환경에서 꺼둠)
    smoothing_enabled: bool,
    /// 잉크 스밈(진해짐) — 볼펜 (은은하게, 기본 활성)
    pen_soak: InkSoak,
    /// 잉크 스밈(진해짐) — 만년필 (뚜렷하게, 기본 활성)
    fountain_soak: InkSoak,
    /// 일반 펜(볼펜) 잉크 질감 (입체적 불균일 — 흐름/위킹/뭉침/레일로드)
    pen_grain: InkGrain,
    /// 만년필 잉크 질감 — 볼펜과 완전히 독립
    fountain_grain: InkGrain,
    /// 만년필 물리 모델 프로파일 (필압 × 속도 × 기울기).
    fountain_profile: FountainProfile,
    /// 현재 펜 기울기 벡터 [tilt_x, tilt_y] (도, ±90). egui/winit이
    /// 노출하지 않아 기본 [0,0] — HID/WM_POINTER 훅에서 `set_pen_tilt`로 주입.
    pen_tilt: [f32; 2],
    /// 진행 중 스트로크의 선폭 확정기 — 점이 입력되는 즉시 폭을 잠급니다
    /// (펜을 뗀 뒤 폭이 변하지 않음).
    width_locker: Option<freedf_core::pen::WidthLocker>,
    /// evdev 펜 모니터 — egui가 노출하지 않는 **틸트/필압** 자동 공급원
    /// (Linux 전용, 장치가 없으면 None).
    pen_monitor: Option<freedf_core::pen_input::PenMonitor>,
    /// evdev에서 직접 읽은 최신 필압 (없으면 egui Touch force 사용).
    live_pressure: Option<f32>,
    /// 펜 사이드 버튼 현재 상태 (OTD/evdev 스트림) — 팔레트 토글 등에 사용.
    pen_buttons: freedf_core::pen_input::PenButtons,
    /// OTD/evdev 펜 스트림이 마지막으로 도착한 시각 (ms) — 진단용.
    last_pen_state_ms: Option<u64>,
    /// 마지막 획의 진단 판정 문구 (Debug HUD 표시용).
    pen_verdict: Option<String>,
    /// LIVE-FLAT 경고 로그 스로틀 (ms).
    pen_flat_log_ms: u64,
    /// 방금 끝난 획의 id — 병합 메시에서 그 획의 정착 렌더 폭을 대조 로그로
    /// 남기기 위한 표식 (진단용).
    last_finished_id: Option<u64>,
    /// LIFT-CUT 로그가 이번 획에서 이미 나왔는지 (스팸 방지).
    lift_cut_logged: bool,
    /// 페이지의 완성 획 전부를 담은 병합 잉크 메시 (드로우 콜 1개).
    ink_mesh: Option<std::sync::Arc<egui::Mesh>>,
    /// 병합 메시가 만들어진 시점의 (페이지, 스토어 버전, 세대, 줌, 잉크 설정).
    /// 메시는 **항상 애니메이션 오프셋 없이**(origin 기준) 구워지고,
    /// 페이지 전환/팬 중에는 그리기 시점에 정점만 평행 이동한 사본을 씁니다.
    /// (pan은 키에 없음 — 팬만 바뀌면 재구성 대신 정점 이동으로 처리)
    ink_key: (
        usize,
        u64,
        u64,
        f32,
        InkSoak,
        InkSoak,
        BallPenProfile,
        FountainProfile,
        InkGrain,
        InkGrain,
    ),
    /// 병합 메시가 구워진 시점의 pan — 팬만 바뀌면 재구성 없이 정점 이동.
    ink_baked_pan: (f32, f32),
    /// 병합 메시를 만든 시각 (ms).
    ink_built_at: u64,
    /// 다음 블리드 정착 시각 (젊은 후광 동안 매 프레임 재구성).
    ink_next_settle_ms: u64,
    /// 스토어 교체(문서 열기/탭 전환)마다 증가 — 캐시 키 충돌 방지.
    store_generation: u64,
    /// 진행 중 획의 캐시된 렌더 메시 (본체+후광 합본) — **100ms 스로틀**로
    /// 재구성하고, 그 사이엔 이 메시를 그대로 다시 그립니다.
    /// (빌드 시각 ms, 점 수, 뷰/설정 키, 메시)
    active_mesh: Option<(
        u64,
        usize,
        (
            f32,
            f32,
            f32,
            f32,
            f32,
            InkSoak,
            InkSoak,
            BallPenProfile,
            FountainProfile,
            InkGrain,
            InkGrain,
        ),
        std::sync::Arc<egui::Mesh>,
    )>,

    // ---------- Paper (grid / color / size) ----------
    paper_style: PaperStyle,
    paper_color: [u8; 4],
    /// 종이 질감 — 페이지 위에 은은한 섬유 노이즈.
    paper_texture: bool,
    /// 종이 질감 강도 0..1.
    paper_texture_strength: f32,
    /// 종이 질감 초보자 프리셋 단계 0..=4 (Lowest..Highest).
    paper_texture_level: u8,
    /// Custom — 상세 표면/조명 값 직접 조절.
    paper_texture_custom: bool,
    /// 종이 표면 물리 모델 (요철·조명·반사율) — 문서 §6.
    paper_surface: freedf_core::paper::PaperSurfaceSettings,
    /// 종이 질감 노이즈 텍스처 캐시 (강도·배경색·표면 설정 변경 시 재생성).
    paper_noise_tex: Option<egui::TextureHandle>,
    paper_noise_cfg: Option<(f32, [u8; 3], freedf_core::paper::PaperSurfaceSettings)>,
    /// 종이 크기 (새 페이지/노트 기본값)
    paper_size: PaperSize,
    /// 캔버스(페이지 뒤 서라운드) 배경색.
    canvas_color: [u8; 4],
    /// 스타일별(Ruled/Grid/Dotted) 줄/점 세부설정 — 각 스타일 독립.
    paper_style_settings: PaperStyleSettings,
    /// Paper 설정 창 "범위 적용" 임시 입력 (1-based 페이지 번호).
    paper_range_from: usize,
    paper_range_to: usize,
    /// Insert Page 메뉴의 페이지 개수 입력 (임시 — 입력이 유지되도록 필드에 둠).
    insert_page_count: usize,
    /// Insert Page 메뉴 숫자 입력칸의 편집 중 텍스트 (포커스 유지용).
    insert_page_text: String,
    /// Insert Page 숫자 입력칸 포커스 여부 (편집 중 텍스트 덮어쓰기 방지).
    insert_page_focus: bool,
    /// Color wheel 설정 창에서 새 색을 조합 중인 RGB 값 (임시).
    wheel_pick_color: [u8; 4],
    /// 펜 사이드 버튼 1로 여는 원형 색상 팔레트(굿노트식) 표시 여부 (임시).
    color_wheel_open: bool,
    /// 원형 팔레트가 열린 시각 (ms) — 일정 시간 입력이 없으면 자동 닫힘.
    color_wheel_opened_at: u64,
    /// 원형 팔레트가 열린 위치 (캔버스 좌표) — 펜 위치, 클램프 후 사용.
    color_wheel_anchor: [f32; 2],
    /// 원형 팔레트를 닫은 바깥 탭의 릴리스(점)를 한 번만 무시하는 표식.
    wheel_swallow_click: bool,
    /// 사용자 정의 용지 크기 [가로, 세로] (pt, `PaperSize::Custom`일 때)
    custom_paper_size: [f32; 2],
    /// 펜 입력 스무딩 강도 0..1
    smoothing: f32,
    /// 줌 잠금 (휠/핀치/단축키 줌 무시)
    zoom_lock: bool,
    /// 엣지 자동 스크롤 (커서가 캔버스 가장자리 근처 → 그 방향으로 자동 패닝)
    edge_autoscroll: bool,
    /// 엣지 자동 스크롤이 펜으로 커서를 움직일 때만(호버/접촉) 반응할지 —
    /// 단순 마우스/트랙패드 호버는 무시. 펜 스트림이 없으면 구분 불가라 허용.
    edge_autoscroll_pen_only: bool,
    /// 입력 소스(펜/마우스/트랙패드) 판정 상태 — 매 프레임 canvas()에서 갱신.
    input_sources: input::InputSources,
    /// 엣지 반응 영역 폭 (화면 px)
    edge_zone: f32,
    /// 엣지 방향별 최대 속도 [왼쪽, 오른쪽, 위, 아래] (화면 px/s)
    edge_speeds: [f32; 4],
    /// 엣지 자동 스크롤 설정 창 표시 여부
    edge_scroll_settings_open: bool,
    /// 최소 모드 좌측(Library/Outline/Bookmarks) 컨테이너 접힘 여부 (일시적)
    minimal_sections_collapsed: bool,
    /// 최소 모드 우측(Palette/Show UI) 컨테이너 접힘 여부 (일시적)
    minimal_chrome_collapsed: bool,
    /// 커서가 창 위에서 움직이면 이 창을 포커스할지 (스플릿 뷰, 창마다 독립)
    window_focus_on_move: bool,
    /// Window Focus 지연(초) — 커서가 창 위에 이 시간 이상 머물면 포커스
    window_focus_dwell_sec: f32,
    /// 커서가 이 창 위에 머물기 시작한 시각(ms) — 0이면 없음 (런타임 전용)
    window_hover_since_ms: u64,
    /// Window Focus 설정 창 표시 여부
    window_focus_settings_open: bool,
    /// 페이지(문서) 바깥으로 더 패닝할 수 있는 여유 (화면 px)
    edge_overscroll: f32,
    /// 엣지 스크롤 "숨쉬는" 글로우 표시 여부
    edge_pulse: bool,
    /// 엣지 스크롤 방향별 반응 지연(초) [왼쪽, 오른쪽, 위, 아래]
    edge_delays: [f32; 4],
    /// 방향별 가장자리 진입 시각(ms) — 반응 지연 램프용 (런타임 전용)
    edge_zone_enter_ms: [u64; 4],
    /// 이번 프레임 방향별 글로우 강도 [왼쪽, 오른쪽, 위, 아래] (런타임 전용)
    edge_glow: [f32; 4],
    /// 커스텀 커서 표시 여부 (히스테리시스 — 상태 깜빡임 방지)
    cursor_custom_shown: bool,
    /// 이전 프레임의 표시 희망(want) — 히스테리시스 비교 기준
    cursor_prev_want: bool,
    /// 커스텀 커서 상태 연속 프레임 카운터
    cursor_custom_counter: u32,

    // ---------- Input ----------
    active_stroke: Option<ActiveStroke>,
    pan_last: Option<Pos2>,
    middle_pan_last: Option<Pos2>,
    /// 펜 입력 스무딩 필터 (x/y/필압 채널, 스트로크 시작 시 리셋)
    smooth_x: OneEuroFilter,
    smooth_y: OneEuroFilter,
    smooth_p: OneEuroFilter,
    /// 현재 스트로크가 스무딩 필터를 사용 중인지
    smooth_active: bool,
    /// Trackpad/wheel momentum (points/sec) for inertial panning
    scroll_vel: Vec2,
    /// Page change slide animation
    page_anim: Option<PageAnim>,
    /// 다음 페이지 전환을 세로로 할지 (PgUp/PgDn 키가 세팅) — 시작 시 소비
    transition_vertical: bool,
    /// Texture of the outgoing page during a transition
    prev_texture: Option<egui::TextureHandle>,
    /// Page index before the latest page change (drives the animation direction)
    transition_last_page: usize,
    /// 다음/이전 페이지를 미리 렌더한 텍스처 (페이지 전환을 부드럽게)
    prefetch: Option<(usize, f32, egui::TextureHandle)>,
    /// 프리페치가 필요한지 (페이지 변경/애니메이션 종료 시 세팅)
    prefetch_pending: bool,

    /// 단어 사전 오버레이
    dictionary: Dictionary,

    // ---------- Compact (narrow / split-view) mode ----------
    /// While the window is narrow the UI collapses to canvas + palette; set to
    /// `true` to temporarily show the full chrome (tabs/toolbar) again.
    narrow_chrome_expanded: bool,
    /// Manual "focus" mode: hides all toolbars regardless of the window width
    /// (toggled with Ctrl+Shift+M, or from the floating pill). Always shows the
    /// writing palette; the pill restores the chrome.
    manual_minimal: bool,
    /// 스플릿 뷰 포커스 제스처: 포커스가 없던 동안 이미 Focus를 요청했는지
    /// (펜 호버 시 한 번만 요청 — 계속 뺏아오는 것을 방지).
    focus_grabbed: bool,
    /// 직전 프레임의 뷰포트 포커스 상태 — 이번 프레임에 false→true로
    /// 바뀌었는지(첫 탭이 포커스를 만든 것인지) 판별용.
    prev_viewport_focused: Option<bool>,
    /// 포커스가 프레스와 동시에 잡힌 직후의 잉크 유예 만료 시각 (ms).
    focus_grace_until_ms: Option<u64>,
    /// 포커스 요청으로 삼킨 프레스의 릴리스(점)를 한 번만 무시하는 표식.
    focus_swallow_next_click: bool,

    // ---------- Search ----------
    search_query: String,
    search_runs: Vec<TextRun>,
    search_matches: Vec<TextMatch>,
    search_current: Option<usize>,
    /// Search row visible only while Ctrl+F was pressed.
    show_search: bool,
    /// Request focus on the search box next frame.
    focus_search: bool,

    // ---------- Outline ----------
    outline: Vec<OutlineNode>,
    outline_loaded: bool,

    // ---------- Panels ----------
    show_library: bool,
    show_outline: bool,
    /// 최소(포커스) 모드 오버레이의 Bookmarks 섹션 표시 여부 (일시적).
    show_bookmarks: bool,
    /// Canvas right-side writing-tool / color palette (global pref)
    show_palette: bool,
    /// Frequently-used pen colors (global pref)
    favorite_colors: Vec<[u8; 4]>,
    /// Highlighter snaps to recognized document text (global pref)
    text_highlight_snap: bool,
    /// Library 패널 검색 필터 (일시적)
    library_filter: String,
    /// Library 패널 다중 삭제 선택 상태 (일시적, documents.id 기준)
    sel_notes: HashSet<i64>,
    sel_pdfs: HashSet<i64>,

    // ---------- Logging / status ----------
    logger: Logger,
    file_name: String,
    status: Option<String>,
    /// (message, time set) so the transient status line auto-clears
    status_since: Option<(String, f64)>,

    // ---------- CLI startup / new-window ----------
    /// A standalone PDF passed on the command line (`freedf <file.pdf>`);
    /// imported and opened on the very first frame of this window.
    pending_open: Option<PathBuf>,
    /// DB document id passed on the command line (`freedf --doc <id>`);
    /// opened on the very first frame of this window.
    pending_doc: Option<i64>,

    // ---------- Fallback dialog ----------
    modal: Option<ModalState>,
    // ---------- Sync server connection (first-run setup) ----------
    /// Sync v3 서버 연결 여부 — 연결 전엔 `DisconnectedStorage` 폴백 백엔드.
    db_connected: bool,
    /// 첫 실행 대화상자 표시 여부 (서버 연결 후에도 저장 확인을 위해 유지).
    setup_open: bool,
    /// 마지막 서버 연결 시도 결과 (성공 여부, 메시지).
    connect_status: Option<(bool, String)>,
    /// 백그라운드 연결 시도 수신 채널 (+자동 시작 여부).
    pending_connect: Option<(
        std::sync::mpsc::Receiver<Result<SharedStorage, String>>,
        bool,
    )>,
    // ---------- Loading / background ops ----------
    /// 진행 중인 배경 작업 표시 (스피너+진행바 오버레이). 완료 시 None.
    loading: Option<String>,
    /// 로딩 시작 시각 (경과 시간 표시용).
    loading_started: Option<f64>,
    /// 문서 열기 로더 수신 채널 (단계/완료 메시지).
    loader_rx: Option<std::sync::mpsc::Receiver<LoaderMsg>>,
    /// 미디어 작업(목록/업로드/삭제) 수신 채널.
    media_rx: Option<std::sync::mpsc::Receiver<Result<MediaOutcome, String>>>,
    /// 백그라운드 저장 진행 채널 (단계/완료).
    save_rx: Option<std::sync::mpsc::Receiver<SaveMsg>>,
    /// 라이브러리 삭제(노트/PDF) 백그라운드 작업 채널 — 여러 배치가 동시에
    /// 떠 있을 수 있어 벡터로 관리합니다.
    library_rx: Vec<std::sync::mpsc::Receiver<LibraryOutcome>>,
    /// 외부 PDF import(파일 읽기 + DB 업로드) 백그라운드 작업 채널.
    pdf_import_rx: Option<std::sync::mpsc::Receiver<PdfImportMsg>>,
    /// 저장 완료 후 앱 종료 예정 여부 (Save & Quit).
    pending_quit: bool,
    // ---------- Media server ----------
    /// 미디어(녹음) 서버 연결 설정 — `server.json`에서 런타임 로드.
    media_config: MediaServerConfig,
    /// 미디어 서버 설정 창 표시 여부 (툴바 Server 버튼).
    server_settings_open: bool,
    /// 설정 창의 마지막 테스트/저장 결과 (성공 여부, 메시지).
    server_msg: Option<(bool, String)>,
    // ---------- Media (audio recordings) ----------
    /// 녹음 패널 표시 여부 (툴바 Media 버튼).
    show_media: bool,
    /// 현재 문서의 미디어 목록 캐시.
    media_items: Vec<MediaObject>,
    /// `media_items`를 로드한 문서 id (문서가 바뀌면 재로드).
    media_loaded_for: Option<i64>,
    /// 패널 안 마지막 작업 결과 메시지.
    media_status: Option<String>,
    // ---------- Close confirmation ----------
    asking_close: bool,
    quitting: bool,
}

impl FreeDfApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        db: std::sync::Arc<dyn StorageBackend>,
        db_connected: bool,
        connect_error: Option<String>,
        logger: Logger,
        pending_open: Option<PathBuf>,
        pending_doc: Option<i64>,
    ) -> Self {
        // Disable egui's built-in Ctrl+scroll zoom folding: it multiplies the
        // zoom by exp(speed * scroll), which jumps ~28% per wheel notch. We do
        // discrete 5% zoom ourselves (see handle_canvas_input), so keep egui's
        // fold a no-op while still allowing real pinch (Event::Zoom).
        cc.egui_ctx.options_mut(|o| o.input_options.scroll_zoom_speed = 0.0);

        // 기본 펜/만년필 색은 항상 진한 검정 (다크 테마에서도 흰색이 아님).
        let theme_pen = Palette::default_pen();
        let theme_hi = Palette::default_highlighter();

        // 전역 기본 세션 → DB app_state('session'). 없으면 테마 기본값.
        let (s, has) = match db.get_app_state("session") {
            Some(v) => (crate::settings::SessionState::from_json_value(v), true),
            None => (crate::settings::SessionState::default(), false),
        };
        let pen_color = if has { s.pen_color } else { theme_pen };
        let hi_color = if has { s.hi_color } else { theme_hi };
        let fountain_color = if has { s.fountain_color } else { theme_pen };
        let tool = if has { s.tool } else { ToolType::Pen };
        let color_family = if has { s.color_family } else { ColorFamily::Black };
        let pen_width = if has { s.pen_width } else { 2.0 };
        let fountain_width = if has { s.fountain_width } else { 2.0 };
        let hi_width = if has { s.hi_width } else { 16.0 };
        let eraser_radius = if has { s.eraser_radius } else { 16.0 };
        let pressure_enabled = if has { s.pressure_enabled } else { true };
        let debug_hud = if has { s.debug_hud } else { false };
        let left_handed = if has { s.left_handed } else { false };
        // 펜 입력 공급원 — Windows는 OTD 데몬 IPC(틸트·필압), Linux는 evdev.
        #[cfg(target_os = "windows")]
        let pen_monitor = freedf_core::pen_input::spawn_otd_monitor()
            .map(freedf_core::pen_input::from_receiver);
        #[cfg(not(target_os = "windows"))]
        let pen_monitor = freedf_core::pen_input::open_best();
        let pen_profile = if has { s.pen_profile } else { BallPenProfile::default() };
        let paper_style = if has { s.paper_style } else { PaperStyle::Blank };
        let paper_color = if has { s.paper_color } else { PAPER_WHITE };
        let paper_texture = if has { s.paper_texture } else { true };
        let mut paper_texture_strength = if has {
            s.paper_texture_strength.clamp(0.0, 1.0)
        } else {
            0.25
        };
        let mut paper_surface = if has {
            s.paper_surface
        } else {
            freedf_core::paper::PaperSurfaceSettings::default()
        };
        let paper_texture_level = if has { s.paper_texture_level.min(4) } else { 2 };
        let paper_texture_custom = if has { s.paper_texture_custom } else { false };
        // Custom이 꺼져 있으면 프리셋 단계가 강도·표면 값을 지배합니다.
        if !paper_texture_custom {
            let (ps, surf) = freedf_core::paper::paper_texture_preset(paper_texture_level);
            paper_texture_strength = ps;
            paper_surface = surf;
        }
        let canvas_color = if has {
            s.canvas_color
        } else {
            crate::theme::nord::semantic::PAGE_SURROUND.to_array()
        };
        let paper_size = if has { s.paper_size } else { PaperSize::A4 };
        let paper_style_settings = if has {
            s.paper_style_settings
        } else {
            PaperStyleSettings::default()
        };
        let show_library = if has { s.show_notes } else { true };
        let show_outline = if has { s.show_outline } else { false };
        let show_palette = if has { s.show_palette } else { true };
        let mut favorite_colors = if has {
            s.favorite_colors.clone()
        } else {
            crate::settings::SessionState::default().favorite_colors
        };
        // 팔레트는 최대 3색.
        favorite_colors.truncate(MAX_FAVORITE_COLORS);
        if favorite_colors.is_empty() {
            favorite_colors = crate::settings::SessionState::default().favorite_colors;
        }
        let text_highlight_snap = if has { s.text_highlight_snap } else { false };
        let zoom_lock = if has { s.zoom_lock } else { false };
        let smoothing = if has { s.smoothing.clamp(0.0, 1.0) } else { 0.4 };
        let smoothing_enabled = if has { s.smoothing_enabled } else { false };
        let pen_soak = if has { s.pen_soak } else { InkSoak::ballpoint_default() };
        let fountain_soak = if has {
            s.fountain_soak
        } else {
            InkSoak::fountain_default()
        };
        let pen_grain = if has { s.pen_grain } else { InkGrain::default() };
        let fountain_grain = if has {
            s.fountain_grain
        } else {
            InkGrain::default()
        };
        let fountain_profile = if has {
            s.fountain_profile
        } else {
            FountainProfile::default()
        };
        let mouse_draws = if has { s.mouse_draws } else { false };
        let dictionary_enabled = if has { s.dictionary_enabled } else { false };
        let edge_autoscroll = if has { s.edge_autoscroll } else { false };
        let edge_autoscroll_pen_only = if has { s.edge_autoscroll_pen_only } else { true };
        let edge_zone = if has { s.edge_zone.clamp(8.0, 300.0) } else { 72.0 };
        let edge_speeds = if has {
            [
                s.edge_speeds[0].clamp(20.0, 4000.0),
                s.edge_speeds[1].clamp(20.0, 4000.0),
                s.edge_speeds[2].clamp(20.0, 4000.0),
                s.edge_speeds[3].clamp(20.0, 4000.0),
            ]
        } else {
            [480.0; 4]
        };
        let window_focus_on_move = if has { s.window_focus_on_move } else { false };
        let window_focus_dwell_sec = if has {
            s.window_focus_dwell_sec.clamp(0.0, 5.0)
        } else {
            0.5
        };
        let edge_overscroll = if has { s.edge_overscroll.clamp(0.0, 2000.0) } else { 64.0 };
        let edge_pulse = if has { s.edge_pulse } else { true };
        let edge_delays = if has {
            [
                s.edge_delays[0].clamp(0.0, 3.0),
                s.edge_delays[1].clamp(0.0, 3.0),
                s.edge_delays[2].clamp(0.0, 3.0),
                s.edge_delays[3].clamp(0.0, 3.0),
            ]
        } else {
            [0.5; 4]
        };
        let custom_paper_size = if let Some(c) = s.custom_paper_size {
            [c[0].clamp(100.0, 2400.0), c[1].clamp(100.0, 2400.0)]
        } else {
            PaperSize::A4.size_pts()
        };
        let tool_order = if has {
            s.tool_order.clone()
        } else {
            ToolType::default_order()
        };
        // 저장된 순서에 새로 추가된 도구(예: 만년필)가 없으면 기본 위치에 보충.
        let mut tool_order = tool_order;
        for t in ToolType::default_order() {
            if !tool_order.contains(&t) {
                tool_order.push(t);
            }
        }
        let library_filter = String::new();

        // 미디어 서버 연결 설정 — 빌드타임이 아니라 `server.json`에서 런타임 로드.
        let media_config = MediaServerConfig::load(&MediaServerConfig::config_path());

        // 노트 캐시 + 최근 목록 캐시 → DB에서 로드.
        let notes = NotesManager::from_metas(
            db.list_notes()
                .into_iter()
                .map(|d| freedf_core::notes::NoteMeta {
                    id: d.id as u64,
                    title: d.title,
                    created_at_ms: d.created_at.max(0) as u128,
                    updated_at_ms: d.updated_at.max(0) as u128,
                    page_count: d.page_count.max(0) as usize,
                })
                .collect(),
        );
        let recents = RecentList {
            items: db
                .load_recents()
                .into_iter()
                .map(|r| RecentItem {
                    kind: if r.kind == "note" { RecentKind::Note } else { RecentKind::File },
                    doc_id: Some(r.doc_id),
                    note_id: if r.kind == "note" { Some(r.doc_id as u64) } else { None },
                    path: r.origin_path.map(PathBuf::from),
                    title: r.title,
                    opened_at_ms: r.opened_at.max(0) as u128,
                })
                .collect(),
        };

        Self {
            notes,
            tabs: Vec::new(),
            active: 0,
            recents,
            db,
            current_note: None,
            document: None,
            pdfium: crate::pdf::load_pdfium().map(Box::new),
            doc_id: None,
            current_page: 0,
            page_size_pts: PaperSize::A4.size_pts(),
            view: ViewTransform::default(),
            page_align: PageAlign::Center,
            last_canvas: [1280.0, 600.0],
            prev_canvas: [1280.0, 600.0],
            prev_canvas_origin: egui::Pos2::ZERO,
            pending_fit: None,
            texture: None,
            render_dirty: true,
            last_render_zoom: 0.0,
            last_render_ppp: 0.0,
            store: AnnotationStore::new(),
            history: History::new(256),
            stroke_id_pool: (0, 0),
            stroke_id_refill: None,
            tool,
            color_family,
            pen_color,
            pen_width,
            fountain_color,
            fountain_width,
            hi_color,
            hi_width,
            eraser_radius,
            pressure_enabled,
            pen_profile,
            pen_cursor_style: PenCursorStyle::Round,
            tool_order,
            tool_drag: None,
            tool_drop: None,
            tool_settings_open: false,
            paper_settings_open: false,
            canvas_settings_open: false,
            wheel_settings_open: false,
            insert_page_open: false,
            input_device: InputDevice::Mouse,
            last_touch_time: None,
            mouse_draws,
            smoothing_enabled,
            pen_soak,
            fountain_soak,
            pen_grain,
            fountain_grain,
            fountain_profile,
            pen_tilt: [0.0, 0.0],
            debug_hud,
            left_handed,
            width_locker: None,
            pen_monitor,
            live_pressure: None,
            pen_buttons: Default::default(),
            input_sources: input::InputSources::default(),
            last_pen_state_ms: None,
            pen_verdict: None,
            pen_flat_log_ms: 0,
            last_finished_id: None,
            lift_cut_logged: false,
            ink_mesh: None,
            ink_key: (
                0,
                0,
                0,
                0.0,
                InkSoak::default(),
                InkSoak::default(),
                BallPenProfile::default(),
                FountainProfile::default(),
                InkGrain::default(),
                InkGrain::default(),
            ),
            ink_baked_pan: (0.0, 0.0),
            ink_built_at: 0,
            ink_next_settle_ms: u64::MAX,
            store_generation: 0,
            active_mesh: None,
            paper_style,
            paper_color,
            paper_texture,
            paper_texture_strength,
            paper_texture_level,
            paper_texture_custom,
            paper_surface,
            paper_noise_tex: None,
            paper_noise_cfg: None,
            paper_size,
            canvas_color,
            paper_style_settings,
            paper_range_from: 0,
            paper_range_to: 0,
            insert_page_count: 1,
            insert_page_text: String::from("1"),
            insert_page_focus: false,
            wheel_pick_color: [200, 40, 40, 255],
            color_wheel_open: false,
            color_wheel_opened_at: 0,
            color_wheel_anchor: [0.0, 0.0],
            wheel_swallow_click: false,
            custom_paper_size,
            smoothing,
            zoom_lock,
            edge_autoscroll,
            edge_autoscroll_pen_only,
            edge_zone,
            edge_speeds,
            edge_scroll_settings_open: false,
            minimal_sections_collapsed: false,
            minimal_chrome_collapsed: false,
            window_focus_on_move,
            window_focus_dwell_sec,
            window_hover_since_ms: 0,
            window_focus_settings_open: false,
            edge_overscroll,
            edge_pulse,
            edge_delays,
            edge_zone_enter_ms: [0; 4],
            edge_glow: [0.0; 4],
            cursor_custom_shown: false,
            cursor_prev_want: false,
            cursor_custom_counter: 0,
            active_stroke: None,
            pan_last: None,
            middle_pan_last: None,
            smooth_x: OneEuroFilter::from_smoothing(0.4),
            smooth_y: OneEuroFilter::from_smoothing(0.4),
            smooth_p: OneEuroFilter::from_smoothing(0.4),
            smooth_active: false,
            scroll_vel: Vec2::ZERO,
            page_anim: None,
            transition_vertical: false,
            prev_texture: None,
            transition_last_page: 0,
            prefetch: None,
            prefetch_pending: false,
            dictionary: Dictionary {
                enabled: dictionary_enabled,
                ..Default::default()
            },
            narrow_chrome_expanded: false,
            manual_minimal: false,
            focus_grabbed: false,
            prev_viewport_focused: None,
            focus_grace_until_ms: None,
            focus_swallow_next_click: false,
            search_query: String::new(),
            search_runs: Vec::new(),
            search_matches: Vec::new(),
            search_current: None,
            show_search: false,
            focus_search: false,
            outline: Vec::new(),
            outline_loaded: false,
            show_library,
            show_outline,
            show_bookmarks: false,
            show_palette,
            favorite_colors,
            text_highlight_snap,
            library_filter,
            sel_notes: HashSet::new(),
            sel_pdfs: HashSet::new(),
            logger,
            file_name: String::new(),
            status: None,
            status_since: None,
            pending_open,
            pending_doc,
            modal: None,
            db_connected,
            setup_open: !db_connected,
            connect_status: connect_error.map(|e| (false, e)),
            pending_connect: None,
            loading: None,
            loading_started: None,
            loader_rx: None,
            media_rx: None,
            save_rx: None,
            library_rx: Vec::new(),
            pdf_import_rx: None,
            pending_quit: false,
            media_config,
            server_settings_open: false,
            server_msg: None,
            show_media: false,
            media_items: Vec::new(),
            media_loaded_for: None,
            media_status: None,
            asking_close: false,
            quitting: false,
        }
    }

    /// 플랫폼 훅: 펜 기울기 벡터를 주입합니다 (도, 각 축 ±90).
    /// egui/winit은 기울기를 노출하지 않으므로 기본 [0,0]입니다 —
    /// WM_POINTER(POINTER_PEN_INFO.tiltX/tiltY) 또는 HID(X/Y Tilt usage)에서
    /// 읽은 값을 여기로 넣으면 만년필 모델이 기울기를 반영합니다.
    #[allow(dead_code)] // HID/WM_POINTER 훅이 붙을 때까지 미사용.
    pub(crate) fn set_pen_tilt(&mut self, tilt_x: f32, tilt_y: f32) {
        self.pen_tilt = [tilt_x.clamp(-90.0, 90.0), tilt_y.clamp(-90.0, 90.0)];
    }

    /// 전역 기본 세션(마지막 펜 색/용지/도구 등)을 저장해 다음 시작 시 복원합니다.
    fn save_default_session(&self) {
        let state = crate::settings::SessionState {
            page: 0,
            tool: self.tool,
            color_family: self.color_family,
            pen_color: self.pen_color,
            pen_width: self.pen_width,
            fountain_color: self.fountain_color,
            fountain_width: self.fountain_width,
            hi_color: self.hi_color,
            hi_width: self.hi_width,
            eraser_radius: self.eraser_radius,
            pressure_enabled: self.pressure_enabled,
            debug_hud: self.debug_hud,
            left_handed: self.left_handed,
            pen_profile: self.pen_profile,
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            page_align: self.page_align,
            paper_style: self.paper_style,
            paper_color: self.paper_color,
            paper_texture: self.paper_texture,
            paper_texture_strength: self.paper_texture_strength,
            paper_texture_level: self.paper_texture_level,
            paper_texture_custom: self.paper_texture_custom,
            paper_surface: self.paper_surface,
            paper_size: self.paper_size,
            canvas_color: self.canvas_color,
            paper_style_settings: self.paper_style_settings,
            show_notes: self.show_library,
            show_outline: self.show_outline,
            show_palette: self.show_palette,
            favorite_colors: self.favorite_colors.clone(),
            text_highlight_snap: self.text_highlight_snap,
            tool_order: self.tool_order.clone(),
            zoom_lock: self.zoom_lock,
            edge_autoscroll: self.edge_autoscroll,
            edge_autoscroll_pen_only: self.edge_autoscroll_pen_only,
            edge_zone: self.edge_zone,
            edge_speeds: self.edge_speeds,
            window_focus_on_move: self.window_focus_on_move,
            window_focus_dwell_sec: self.window_focus_dwell_sec,
            edge_overscroll: self.edge_overscroll,
            edge_pulse: self.edge_pulse,
            edge_delays: self.edge_delays,
            smoothing: self.smoothing,
            smoothing_enabled: self.smoothing_enabled,
            pen_soak: self.pen_soak,
            fountain_soak: self.fountain_soak,
            pen_grain: self.pen_grain,
            fountain_grain: self.fountain_grain,
            fountain_profile: self.fountain_profile,
            custom_paper_size: Some(self.custom_paper_size),
            mouse_draws: self.mouse_draws,
            dictionary_enabled: self.dictionary.enabled,
        };
        self.db.set_app_state("session", &state.to_json_value());
    }

    /// 풀 보충을 백그라운드 스레드로 예약 — 원격 왕복이 UI 스레드를 막지 않음.
    fn request_stroke_pool_refill(&mut self) {
        if self.stroke_id_refill.is_some() {
            return;
        }
        let db = self.db.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        self.stroke_id_refill = Some(rx);
        std::thread::spawn(move || {
            let _ = tx.send(db.alloc_stroke_ids(STROKE_ID_POOL_BATCH));
        });
    }

    /// 백그라운드로 도착한 id 묶음을 매 프레임 흡수 — 풀을 미리 채워
    /// 소진 시에도 UI 스레드에서 DB를 호출할 일이 없습니다.
    fn poll_stroke_refill(&mut self) {
        let Some(rx) = self.stroke_id_refill.take() else {
            return;
        };
        match rx.try_recv() {
            Ok(ids) => {
                if !ids.is_empty() {
                    let first = ids[0] as u64;
                    let last = ids[ids.len() - 1] as u64;
                    self.stroke_id_pool = (first, last + 1);
                }
                if self.stroke_id_pool.1 - self.stroke_id_pool.0
                    < (STROKE_ID_POOL_BATCH as u64) / 2
                {
                    self.request_stroke_pool_refill();
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.stroke_id_refill = Some(rx); // 아직 — 다음 프레임에 다시.
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {}
        }
    }

    /// 다음 스트로크 id n개를 반환 — **UI 스레드에서 DB를 절대 호출하지 않음**.
    /// 풀이 마르면 백그라운드 보충이 도착할 때까지만 소비하고, 없으면 빈 목록
    /// (호출자가 로컬 id 폴백 — 저장은 다음 resync에서 반영).
    fn next_stroke_ids(&mut self, n: usize) -> Vec<i64> {
        let mut out = Vec::with_capacity(n);
        while out.len() < n {
            if self.stroke_id_pool.0 >= self.stroke_id_pool.1 {
                // 백그라운드 보충 결과가 도착했으면 소비 (왕복 없음).
                if let Some(rx) = self.stroke_id_refill.take() {
                    match rx.try_recv() {
                        Ok(ids) => {
                            if !ids.is_empty() {
                                let first = ids[0] as u64;
                                let last = ids[ids.len() - 1] as u64;
                                self.stroke_id_pool = (first, last + 1);
                            }
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => {
                            self.stroke_id_refill = Some(rx); // 아직 — 다음에 다시.
                        }
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {}
                    }
                }
            }
            if self.stroke_id_pool.0 >= self.stroke_id_pool.1 {
                // 동기 폴백 없음 — UI 멈춤 방지가 우선. 연결이 늦거나 끊긴
                // 경우 호출자가 로컬 id(add_stroke)로 그립니다.
                break;
            }
            let take = (n - out.len())
                .min((self.stroke_id_pool.1 - self.stroke_id_pool.0) as usize);
            for _ in 0..take {
                out.push(self.stroke_id_pool.0 as i64);
                self.stroke_id_pool.0 += 1;
            }
        }
        // 절반 미만이면 미리 보충 예약 (다음 소진 때 왕복 없도록).
        if self.stroke_id_pool.1 - self.stroke_id_pool.0 < (STROKE_ID_POOL_BATCH as u64) / 2 {
            self.request_stroke_pool_refill();
        }
        out
    }

    /// Sync v3 서버 연결 시도 (백그라운드 — UI를 블로킹하지 않음).
    /// 서버 설정(server.json)을 저장하고 백엔드를 `SyncStorage`로 교체합니다.
    /// 도중에 다른 서버로 전환하면 열린 문서(이전 서버 소속)를 닫습니다.
    fn try_connect_server(&mut self, auto: bool) {
        let base = self.media_config.base_url.trim().trim_end_matches('/').to_string();
        if base.is_empty() {
            self.connect_status = Some((false, "Enter the server URL first.".into()));
            return;
        }
        if self.pending_connect.is_some() {
            return; // 이미 시도 중.
        }
        self.connect_status = None;
        let config = self.media_config.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        self.pending_connect = Some((rx, auto));
        std::thread::spawn(move || {
            let storage = crate::storage::from_server_config(&config);
            let result = if storage.ping() {
                Ok(storage)
            } else {
                Err("Cannot reach the server (GET /health failed). \
                     Check the URL and API key."
                    .into())
            };
            let _ = tx.send(result);
        });
    }

    /// 백그라운드 연결 결과 수신 (매 프레임 호출).
    fn poll_connect_result(&mut self) {
        let Some((rx, auto)) = self.pending_connect.take() else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(db)) => {
                let switching = self.db_connected;
                self.db = db;
                self.db_connected = true;
                self.media_config.enabled = true;
                let _ = self.media_config.save(&MediaServerConfig::config_path());
                self.connect_status = Some((true, "Connected.".into()));
                if switching {
                    self.close_all_documents();
                }
                self.reload_library_from_db();
                self.request_stroke_pool_refill();
                if auto {
                    self.setup_open = false;
                }
            }
            Ok(Err(e)) => {
                self.db_connected = false;
                self.connect_status = Some((false, e));
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.pending_connect = Some((rx, auto)); // 아직 — 다음 프레임에 다시.
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.connect_status =
                    Some((false, "Connection attempt was interrupted.".into()));
            }
        }
    }

    /// 도중에 DB를 바꾸거나 연결을 끊을 때 — 열린 문서는 이전 DB 소속이므로
    /// 모두 닫습니다.
    fn close_all_documents(&mut self) {
        while !self.tabs.is_empty() {
            self.close_tab(0);
        }
        if self.document.is_some() {
            self.close_document();
        }
    }

    /// 연결 성공 직후 라이브러리/최근 목록을 DB에서 다시 채웁니다.
    fn reload_library_from_db(&mut self) {
        self.notes = NotesManager::from_metas(
            self.db
                .list_notes()
                .into_iter()
                .map(|d| freedf_core::notes::NoteMeta {
                    id: d.id as u64,
                    title: d.title,
                    created_at_ms: d.created_at.max(0) as u128,
                    updated_at_ms: d.updated_at.max(0) as u128,
                    page_count: d.page_count.max(0) as usize,
                })
                .collect(),
        );
        self.recents = RecentList {
            items: self
                .db
                .load_recents()
                .into_iter()
                .map(|r| RecentItem {
                    kind: if r.kind == "note" {
                        RecentKind::Note
                    } else {
                        RecentKind::File
                    },
                    doc_id: Some(r.doc_id),
                    note_id: if r.kind == "note" {
                        Some(r.doc_id as u64)
                    } else {
                        None
                    },
                    path: r.origin_path.map(PathBuf::from),
                    title: r.title,
                    opened_at_ms: r.opened_at.max(0) as u128,
                })
                .collect(),
        };
    }

    /// First-run setup dialog.
    ///
    /// 서버에 연결되기 전에는 닫을 수 없습니다 — 연결 후에는
    /// "Done"으로 닫습니다.
    fn connection_dialog(&mut self, ctx: &egui::Context) {
        if !self.setup_open {
            return;
        }
        let forced = !self.db_connected;
        let mut open = self.setup_open;
        let mut done = false;
        let mut window = egui::Window::new("FreeDF — first run setup")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .default_width(560.0);
        if !forced {
            window = window.open(&mut open);
        }
        window.show(ctx, |ui| {
            ui.label(egui::RichText::new("Sync server (v3)").strong());
            if forced {
                ui.label(
                    "Enter the FreeDF server address — all documents are stored \
                     through it (Sync v3 snapshots + media):",
                );
            }
            ui.horizontal(|ui| {
                ui.label("Server URL");
                ui.add(
                    egui::TextEdit::singleline(&mut self.media_config.base_url)
                        .hint_text("https://your-server.example.com")
                        .desired_width(330.0),
                );
            });
            ui.horizontal(|ui| {
                ui.label("API key");
                ui.add(
                    egui::TextEdit::singleline(&mut self.media_config.api_key)
                        .password(true)
                        .desired_width(330.0),
                );
            });
            ui.horizontal(|ui| {
                let label = if self.db_connected {
                    "Reconnect"
                } else {
                    "Connect"
                };
                if ui.button(label).clicked() {
                    self.try_connect_server(false);
                }
                if self.db_connected {
                    ui.label(egui::RichText::new("✓ connected").weak());
                }
            });
            if self.pending_connect.is_some() {
                ui.label(egui::RichText::new("Connecting…").weak());
            } else if let Some((ok, msg)) = &self.connect_status {
                let color = if *ok {
                    ui.visuals().hyperlink_color
                } else {
                    ui.visuals().error_fg_color
                };
                ui.colored_label(color, msg);
            }
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(
                    "The server hosts Sync v3 (document snapshots) and media \
                     (recordings) behind one address and API key.",
                )
                .weak(),
            );
            if !forced {
                ui.add_space(8.0);
                if ui.button("Done").clicked() {
                    done = true;
                }
            }
        });
        self.setup_open = open && !done;
    }

    /// 새 페이지/노트에 쓸 물리적 크기 (pt). `PaperSize::Custom`이면 사용자 정의 치수.
    pub(crate) fn new_page_size_pts(&self) -> [f32; 2] {
        if self.paper_size == PaperSize::Custom {
            self.custom_paper_size
        } else {
            self.paper_size.size_pts()
        }
    }

    /// 현재 페이지의 용지 설정 (저장된 값, 없으면 툴바 기본값).
    fn current_page_paper(&self) -> PagePaper {
        self.store.paper_on_or(
            self.current_page,
            PagePaper {
                style: self.paper_style,
                color: self.paper_color,
            },
        )
    }

    /// 툴바의 용지 기본값(스타일+색)을 현재 페이지에 저장하고 다시 그립니다.
    fn apply_paper_to_current_page(&mut self) {
        if let Some(doc) = &self.document {
            if self.current_page < doc.page_count() {
                self.store.set_paper(
                    self.current_page,
                    PagePaper {
                        style: self.paper_style,
                        color: self.paper_color,
                    },
                );
                // DB pages 행에도 즉시 반영.
                if let Some(doc_id) = self.doc_id {
                    self.db.upsert_page(
                        doc_id,
                        self.current_page as i32,
                        &self.current_page_paper(),
                        self.store.is_bookmarked(self.current_page),
                    );
                }
                self.render_dirty = true;
            }
        }
    }

    // ---------- Session (per-document GUI state) ----------

    /// 현재 GUI 상태를 세션 구조체로 캡처합니다.
    fn capture_session(&self) -> crate::settings::SessionState {
        crate::settings::SessionState {
            page: self.current_page,
            tool: self.tool,
            color_family: self.color_family,
            pen_color: self.pen_color,
            pen_width: self.pen_width,
            fountain_color: self.fountain_color,
            fountain_width: self.fountain_width,
            hi_color: self.hi_color,
            hi_width: self.hi_width,
            eraser_radius: self.eraser_radius,
            pressure_enabled: self.pressure_enabled,
            debug_hud: self.debug_hud,
            left_handed: self.left_handed,
            pen_profile: self.pen_profile,
            zoom: self.view.zoom,
            pan_x: self.view.pan_x,
            pan_y: self.view.pan_y,
            page_align: self.page_align,
            paper_style: self.paper_style,
            paper_color: self.paper_color,
            paper_texture: self.paper_texture,
            paper_texture_strength: self.paper_texture_strength,
            paper_texture_level: self.paper_texture_level,
            paper_texture_custom: self.paper_texture_custom,
            paper_surface: self.paper_surface,
            paper_size: self.paper_size,
            canvas_color: self.canvas_color,
            paper_style_settings: self.paper_style_settings,
            show_notes: self.show_library,
            show_outline: self.show_outline,
            show_palette: self.show_palette,
            favorite_colors: self.favorite_colors.clone(),
            text_highlight_snap: self.text_highlight_snap,
            tool_order: self.tool_order.clone(),
            zoom_lock: self.zoom_lock,
            edge_autoscroll: self.edge_autoscroll,
            edge_autoscroll_pen_only: self.edge_autoscroll_pen_only,
            edge_zone: self.edge_zone,
            edge_speeds: self.edge_speeds,
            window_focus_on_move: self.window_focus_on_move,
            window_focus_dwell_sec: self.window_focus_dwell_sec,
            edge_overscroll: self.edge_overscroll,
            edge_pulse: self.edge_pulse,
            edge_delays: self.edge_delays,
            smoothing: self.smoothing,
            smoothing_enabled: self.smoothing_enabled,
            pen_soak: self.pen_soak,
            fountain_soak: self.fountain_soak,
            pen_grain: self.pen_grain,
            fountain_grain: self.fountain_grain,
            fountain_profile: self.fountain_profile,
            custom_paper_size: Some(self.custom_paper_size),
            mouse_draws: self.mouse_draws,
            dictionary_enabled: self.dictionary.enabled,
        }
    }

    /// 현재 문서의 GUI 상태를 DB `sessions` 테이블에 저장합니다.
    fn save_session(&self) {
        if self.document.is_none() {
            return;
        }
        if let Some(doc_id) = self.doc_id {
            self.db
                .upsert_session(doc_id, &self.capture_session().to_json_value());
        }
    }

    /// 편집을 메모리 히스토리에 쌓고 DB 저널(`doc_edits`)에도 기록합니다.
    /// (재시작 후 undo 스택 복원의 근거 — 그리기/지우기/전체 지우기마다 호출)
    pub(crate) fn push_history(&mut self, edit: Edit) {
        self.history.push(edit.clone());
        if let Some(doc_id) = self.doc_id {
            // CachingBackend에서는 write-behind 큐에 쌓여 원격 왕복이 없습니다.
            self.db.log_edit(doc_id, &edit);
        }
    }

    /// 백그라운드에서 이미 가져온 편집 저널로 undo 스택을 복원합니다.
    pub(crate) fn restore_history_from_edits(&mut self, edits: &[Edit]) {
        self.history = History::new(256);
        for edit in edits {
            self.history.push(edit.clone());
        }
    }

    /// 문서 열기 로더 결과 수신 (매 프레임 호출 — UI를 막지 않음).
    fn poll_loader(&mut self) {
        let Some(rx) = self.loader_rx.take() else {
            return;
        };
        match rx.try_recv() {
            Ok(LoaderMsg::Stage(msg)) => {
                self.begin_loading(msg);
                self.loader_rx = Some(rx);
            }
            Ok(LoaderMsg::Done(Ok(bundle))) => {
                self.end_loading();
                self.finish_document_open(bundle);
            }
            Ok(LoaderMsg::Done(Err(e))) => {
                self.end_loading();
                self.show_error(e);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.loader_rx = Some(rx); // 아직 — 다음 프레임에 다시.
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.end_loading();
                self.show_error("Document loading was interrupted.".into());
            }
        }
    }

    /// 백그라운드 저장 진행 수신 (단계 갱신 + 완료 처리).
    fn poll_save(&mut self, ctx: &egui::Context) {
        let Some(rx) = self.save_rx.take() else {
            return;
        };
        match rx.try_recv() {
            Ok(SaveMsg::Stage(msg)) => {
                self.begin_loading(msg);
                self.save_rx = Some(rx);
            }
            Ok(SaveMsg::Done(Ok(()))) => {
                self.end_loading();
                self.status = Some("Saved to database.".into());
                if self.pending_quit {
                    self.pending_quit = false;
                    self.quitting = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            Ok(SaveMsg::Done(Err(e))) => {
                self.end_loading();
                self.status = Some(format!("Save failed: {e}"));
                self.pending_quit = false;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.save_rx = Some(rx);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.end_loading();
                self.status = Some("Save was interrupted.".into());
                self.pending_quit = false;
            }
        }
    }

    /// 배경 작업 시작 표시 (오버레이 + 경과 시간).
    fn begin_loading(&mut self, msg: impl Into<String>) {
        self.loading = Some(msg.into());
        self.loading_started = Some(now_secs());
    }

    fn end_loading(&mut self) {
        self.loading = None;
        self.loading_started = None;
    }

    /// 미디어 작업 결과 수신 (매 프레임 호출).
    fn poll_media(&mut self) {
        let Some(rx) = self.media_rx.take() else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(outcome)) => {
                self.end_loading();
                match outcome {
                    MediaOutcome::Listed(items) => {
                        self.media_status = if items.is_empty() {
                            Some("No recordings yet.".into())
                        } else {
                            None
                        };
                        self.media_items = items;
                    }
                    MediaOutcome::Uploaded(obj) => {
                        self.media_status = Some(format!("Uploaded {}", obj.name));
                        self.media_list_job(); // 목록 재조회.
                    }
                    MediaOutcome::Deleted => {
                        self.media_status = Some("Deleted.".into());
                        self.media_list_job();
                    }
                }
            }
            Ok(Err(e)) => {
                self.end_loading();
                self.media_status = Some(e);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.media_rx = Some(rx);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.end_loading();
                self.media_status = Some("Media operation was interrupted.".into());
            }
        }
    }

    /// 라이브러리 삭제(원격 DB 왕복)를 백그라운드 스레드로 보냅니다.
    /// 로컬 상태는 이미 반영된 뒤이므로 UI는 즉시 반응합니다.
    pub(crate) fn spawn_library_delete(&mut self, jobs: Vec<LibraryJob>) {
        let db = self.db.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for job in jobs {
                let doc_id = match &job {
                    LibraryJob::DeleteNote { doc_id, .. } | LibraryJob::DeletePdf { doc_id, .. } => {
                        *doc_id
                    }
                };
                let result = db.delete_document(doc_id).map_err(|e| e.to_string());
                if tx.send(LibraryOutcome::Done(job, result)).is_err() {
                    break; // 소비자 종료.
                }
            }
        });
        self.library_rx.push(rx);
    }

    /// 라이브러리 삭제 작업 결과 수신 (매 프레임 호출).
    fn poll_library(&mut self) {
        if self.library_rx.is_empty() {
            return;
        }
        let mut done = Vec::new();
        let mut keep = Vec::new();
        for rx in self.library_rx.drain(..) {
            match rx.try_recv() {
                Ok(msg) => done.push(msg),
                Err(std::sync::mpsc::TryRecvError::Empty) => keep.push(rx),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {}
            }
        }
        self.library_rx = keep;
        for msg in done {
            match msg {
                LibraryOutcome::Done(LibraryJob::DeleteNote { doc_id, title }, Ok(())) => {
                    self.logger.log(AppEvent::NoteDeleted {
                        note_id: doc_id as u64,
                        title,
                    });
                }
                LibraryOutcome::Done(LibraryJob::DeletePdf { name, .. }, Ok(())) => {
                    self.logger.log(AppEvent::PdfDeleted { path: name });
                }
                LibraryOutcome::Done(_, Err(e)) => {
                    self.status = Some(format!("Delete failed: {e}"));
                }
            }
        }
    }

    /// 외부 PDF import(파일 읽기 + DB 업로드) 결과 수신 — 완료되면 엽니다.
    fn poll_pdf_import(&mut self) {
        let Some(rx) = self.pdf_import_rx.take() else {
            return;
        };
        match rx.try_recv() {
            Ok(PdfImportMsg::Done(Ok(doc_id))) => {
                self.end_loading();
                self.open_document(doc_id);
            }
            Ok(PdfImportMsg::Done(Err(e))) => {
                self.end_loading();
                self.show_error(e);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.pdf_import_rx = Some(rx); // 아직 — 다음 프레임에 다시.
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.end_loading();
                self.show_error("PDF import was interrupted.".into());
            }
        }
    }

    /// 진행 중 오버레이 — 스피너 + 단계 메시지 + 경과 시간 + 진행바.
    fn loading_overlay(&self, ctx: &egui::Context) {
        let Some(msg) = self.loading.clone() else {
            return;
        };
        let t = ctx.input(|i| i.time);
        let frac = ((t * 0.7) % 1.0) as f32;
        let elapsed = self.loading_started.map(|s| (now_secs() - s).max(0.0)).unwrap_or(0.0);
        egui::Area::new(egui::Id::new("loading_overlay"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, -60.0])
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(&msg);
                    });
                    ui.horizontal(|ui| {
                        ui.add(egui::ProgressBar::new(frac).desired_width(220.0).show_percentage());
                        ui.label(egui::RichText::new(format!("{elapsed:.1}s")).weak());
                    });
                });
            });
        ctx.request_repaint();
    }

    /// 초보자 프리셋을 현재 단계 값으로 적용합니다 (단계 변경·Custom 해제 시).
    fn apply_paper_texture_preset(&mut self) {
        let (strength, surface) =
            freedf_core::paper::paper_texture_preset(self.paper_texture_level);
        self.paper_texture_strength = strength;
        self.paper_surface = surface;
    }

    /// 저장된 세션을 현재 문서에 적용합니다. `page_count`는 페이지 상한입니다.
    fn apply_session(&mut self, s: &crate::settings::SessionState, page_count: usize) {
        self.current_page = s.page.min(page_count.saturating_sub(1));
        self.tool = s.tool;
        self.color_family = s.color_family;
        self.pen_color = s.pen_color;
        self.pen_width = s.pen_width.clamp(0.5, 12.0);
        self.fountain_color = s.fountain_color;
        self.fountain_width = s.fountain_width.clamp(0.5, 12.0);
        self.hi_color = s.hi_color;
        self.hi_width = s.hi_width.clamp(4.0, 40.0);
        self.eraser_radius = s.eraser_radius.clamp(4.0, 60.0);
        self.pressure_enabled = s.pressure_enabled;
        self.debug_hud = s.debug_hud;
        self.left_handed = s.left_handed;
        self.pen_profile = s.pen_profile;
        self.fountain_profile = s.fountain_profile;
        self.page_align = s.page_align;
        self.paper_style = s.paper_style;
        self.paper_color = s.paper_color;
        self.paper_texture = s.paper_texture;
        self.paper_texture_strength = s.paper_texture_strength.clamp(0.0, 1.0);
        self.paper_texture_level = s.paper_texture_level.min(4);
        self.paper_texture_custom = s.paper_texture_custom;
        self.paper_surface = s.paper_surface;
        if !self.paper_texture_custom {
            self.apply_paper_texture_preset();
        }
        self.paper_size = s.paper_size;
        self.canvas_color = s.canvas_color;
        self.paper_style_settings = s.paper_style_settings;
        self.zoom_lock = s.zoom_lock;
        self.smoothing = s.smoothing.clamp(0.0, 1.0);
        self.smoothing_enabled = s.smoothing_enabled;
        self.pen_soak = s.pen_soak;
        self.fountain_soak = s.fountain_soak;
        self.pen_grain = s.pen_grain;
        self.fountain_grain = s.fountain_grain;
        self.mouse_draws = s.mouse_draws;
        self.dictionary.enabled = s.dictionary_enabled;
        if let Some(c) = s.custom_paper_size {
            self.custom_paper_size = [c[0].clamp(100.0, 2400.0), c[1].clamp(100.0, 2400.0)];
        }
        self.show_library = s.show_notes;
        self.show_outline = s.show_outline;
        // Library / Outline / Bookmarks 상호 베타 — 구버전 세션에서 둘 이상
        // 켜져 있어도 하나만 남깁니다.
        if self.show_library || self.show_outline {
            self.show_bookmarks = false;
        }
        if let Some(doc) = &self.document {
            self.page_size_pts = doc.page_size_pts(self.current_page);
            self.view.zoom = s.zoom.clamp(MIN_ZOOM, MAX_ZOOM);
            self.view.pan_x = s.pan_x;
            self.view.pan_y = s.pan_y;
            self.view
                .clamp_pan(self.page_size_pts, self.last_canvas, self.edge_overscroll);
        }
        self.render_dirty = true;
        self.search_update();
    }

    // ---------- Tabs (multiple open documents) ----------

    /// 같은 대상(노트 id 또는 파일 경로)이 이미 열려 있으면 탭 인덱스 반환.
    /// 최근 항목 기록: 메모리 캐시 + DB `recents` 테이블.
    fn note_recent(&mut self, kind: RecentKind, title: String, doc_id: i64, path: Option<PathBuf>) {
        let opened_at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        self.recents.touch(RecentItem {
            kind,
            doc_id: Some(doc_id),
            note_id: (kind == RecentKind::Note).then_some(doc_id as u64),
            path,
            title: title.clone(),
            opened_at_ms,
        });
        let kind_str = if kind == RecentKind::Note { "note" } else { "pdf" };
        self.db.touch_recent(kind_str, doc_id, &title);
    }

    // ---------- Bookmarks ----------

    /// 현재 페이지 북마크 토글 (애노테이션에 저장).
    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let now = ui.ctx().input(|i| i.time);
        let status = self.status.clone();
        match &status {
            None => self.status_since = None,
            Some(msg) => {
                // Restart the timer whenever the message text changes.
                let restart = match &self.status_since {
                    Some((prev, _)) => prev != msg,
                    None => true,
                };
                let since = if restart {
                    now
                } else {
                    self.status_since.as_ref().unwrap().1
                };
                self.status_since = Some((msg.clone(), since));
                if now - since > 5.0 {
                    self.status = None;
                    self.status_since = None;
                    return;
                }
                egui::Panel::bottom("status").show(ui, |ui| {
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(msg));
                    });
                    ui.add_space(2.0);
                });
            }
        }
    }

    // ---------- UI: canvas ----------

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let ctrl = ctx.input(|i| i.modifiers.command);
        let shift = ctx.input(|i| i.modifiers.shift);

        if ctrl && ctx.input(|i| i.key_pressed(egui::Key::O)) {
            self.open_file_dialog();
        }
        if ctrl && ctx.input(|i| i.key_pressed(egui::Key::N)) {
            self.modal = Some(ModalState::ask_new_note());
        }
        if ctrl && ctx.input(|i| i.key_pressed(egui::Key::Z)) {
            if shift {
                self.redo();
            } else {
                self.undo();
            }
        }
        if ctrl && ctx.input(|i| i.key_pressed(egui::Key::Y)) {
            self.redo();
        }
        if ctrl && ctx.input(|i| i.key_pressed(egui::Key::S)) {
            self.save_annotations();
        }
        if ctrl && shift && ctx.input(|i| i.key_pressed(egui::Key::M)) {
            // 최소(포커스) 모드: 화면 크기와 무관하게 모든 툴바를 숨깁니다.
            // 팔레트는 항상 켜 두고, 우상단 pill(☰ Show UI)로 복귀합니다.
            self.manual_minimal = !self.manual_minimal;
            self.narrow_chrome_expanded = false;
            self.show_palette = true;
            self.save_default_session();
        }
        if ctrl && ctx.input(|i| i.key_pressed(egui::Key::F)) {
            // Toggle the search row; Ctrl+F shows it and focuses the box.
            self.show_search = !self.show_search;
            if self.show_search {
                self.focus_search = true;
                self.search_update();
            } else {
                self.search_clear();
            }
        }
        if ctrl && ctx.input(|i| i.key_pressed(egui::Key::L)) {
            // 줌 잠금 토글 — 실수로 확대/축소되는 것을 막습니다.
            self.zoom_lock = !self.zoom_lock;
            self.save_default_session();
            self.save_session();
        }
        // PgDn / PgUp = 다음/이전 페이지 (스크롤이 아니라 페이지 이동).
        // 마지막 페이지에서는 더 이상 넘어가지 않습니다 (새 페이지 자동 추가 없음).
        // 텍스트 입력 중(검색창/제목)에는 가로채지 않습니다.
        let typing = ctx.egui_wants_keyboard_input();
        if !typing {
            if ctx.input(|i| i.key_pressed(egui::Key::PageDown)) {
                // 브라우저식: 한 뷰포트만 아래로, 끝나면 다음 페이지.
                // (실제 페이지 전환이면 세로 애니메이션)
                self.transition_vertical = true;
                self.page_key(true);
                self.transition_vertical = false; // (스크롤만 했다면 누수 방지)
            }
            if ctx.input(|i| i.key_pressed(egui::Key::PageUp)) {
                self.transition_vertical = true;
                self.page_key(false);
                self.transition_vertical = false;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
                self.next_page();
            }
            if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
                self.prev_page();
            }
            if !self.zoom_lock
                && ctx.input(|i| i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals))
            {
                self.zoom_by(ZOOM_STEP);
            }
            if !self.zoom_lock && ctx.input(|i| i.key_pressed(egui::Key::Minus)) {
                self.zoom_by(1.0 / ZOOM_STEP);
            }
            // Tool shortcuts
            if ctx.input(|i| i.key_pressed(egui::Key::P)) {
                self.tool = ToolType::Pen;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::H)) {
                self.tool = ToolType::Highlighter;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::E)) && !ctrl {
                self.tool = ToolType::Eraser;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::V)) {
                self.tool = ToolType::Pan;
            }
        }
    }

    // ---------- Minimal-mode floating containers ----------

    /// 최소(포커스) 모드의 **좌측 컨테이너** — Library / Outline / Bookmarks
    /// **트리거만** 담습니다. 콘텐츠는 컨테이너 바깥의 독립 오버레이로
    /// 표시됩니다 (`minimal_library_overlay` 등).
    ///
    /// 세 섹션은 **상호 베타적**입니다: 하나를 켜면 나머지는 꺼집니다.
    /// 창은 빈 공간을 잡고 드래그해 이동할 수 있고, 접기 버튼으로 작은
    /// ▤ 버튼 하나만 남길 수 있습니다.
    fn minimal_sections(&mut self, ctx: &egui::Context) {
        let fill = crate::theme::nord::semantic::overlay_bg();
        let stroke = crate::theme::nord::semantic::OVERLAY_BORDER;
        egui::Window::new("minimal_sections")
            .title_bar(false)
            .movable(true)
            .resizable(false)
            .anchor(egui::Align2::LEFT_TOP, egui::vec2(8.0, 8.0))
            .frame(
                egui::Frame::new()
                    .fill(fill)
                    .stroke(egui::Stroke::new(1.0, stroke))
                    .corner_radius(10.0)
                    .inner_margin(egui::Margin::same(6)),
            )
            .show(ctx, |ui| {
                if self.minimal_sections_collapsed {
                    // 접힘: 작은 펼치기 아이콘 버튼 하나만 남습니다.
                    if ui
                        .button(icon_text(ui, "", icons::ARROWS_OUT))
                        .on_hover_text("Expand — Library / Outline / Bookmarks")
                        .clicked()
                    {
                        self.minimal_sections_collapsed = false;
                    }
                    return;
                }
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(
                            self.show_library,
                            icon_text(ui, "Library", icons::NOTEBOOK),
                        )
                        .on_hover_text("Library (notes, PDFs, recents) — exclusive")
                        .clicked()
                    {
                        self.show_library = !self.show_library;
                        if self.show_library {
                            [self.show_library, self.show_outline, self.show_bookmarks] =
                                exclusive_panel_on(PanelKind::Library);
                        }
                        self.save_session();
                    }
                    if ui
                        .selectable_label(
                            self.show_outline,
                            icon_text(ui, "Outline", icons::LIST_BULLETS),
                        )
                        .on_hover_text("Outline — exclusive")
                        .clicked()
                    {
                        self.show_outline = !self.show_outline;
                        if self.show_outline {
                            [self.show_library, self.show_outline, self.show_bookmarks] =
                                exclusive_panel_on(PanelKind::Outline);
                        }
                        self.save_session();
                    }
                    if ui
                        .selectable_label(
                            self.show_bookmarks,
                            icon_text(ui, "Bookmarks", icons::BOOKMARKS_SIMPLE),
                        )
                        .on_hover_text("Bookmarked pages — click to jump — exclusive")
                        .clicked()
                    {
                        self.show_bookmarks = !self.show_bookmarks;
                        if self.show_bookmarks {
                            [self.show_library, self.show_outline, self.show_bookmarks] =
                                exclusive_panel_on(PanelKind::Bookmarks);
                        }
                    }
                    if ui
                        .button(icon_text(ui, "", icons::MINUS))
                        .on_hover_text("Collapse to a small button")
                        .clicked()
                    {
                        self.minimal_sections_collapsed = true;
                    }
                });
            });
    }

    /// Library 콘텐츠 오버레이 — **절대 폭(520pt)의 독립 플로팅 창**.
    /// Hide UI / Show UI 공용. `x, top`은 첫 표시 위치이며, 이후에는
    /// 사용자가 드래그한 위치가 기억됩니다.
    fn library_overlay(&mut self, ctx: &egui::Context, x: f32, top: f32) {
        let fill = crate::theme::nord::semantic::overlay_bg();
        let stroke = crate::theme::nord::semantic::OVERLAY_BORDER;
        egui::Window::new("library_overlay")
            .title_bar(false)
            .movable(true)
            .resizable(false)
            .default_pos(egui::pos2(x, top))
            .frame(
                egui::Frame::new()
                    .fill(fill)
                    .stroke(egui::Stroke::new(1.0, stroke))
                    .corner_radius(10.0)
                    .inner_margin(egui::Margin::same(8)),
            )
            .show(ctx, |ui| {
                ui.set_width(520.0);
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
                // 헤더: 아이콘+제목+개수+닫기를 한 컨테이너로 (공용 헬퍼).
                let total = self.notes.list().len() + self.recents.sorted().len();
                if overlay_header(
                    ui,
                    icons::NOTEBOOK,
                    "Library",
                    &format!("{total} items"),
                    "Close Library",
                ) {
                    self.show_library = false;
                }
                // 콘텐츠가 아무리 넓어도 창 폭(520pt)이 늘어나지 않게 상한 고정 —
                // 닫기 버튼이 항상 오른쪽 구석에 붙습니다.
                egui::ScrollArea::vertical()
                    .id_salt("library_overlay_scroll")
                    .max_height(520.0)
                    .max_width(504.0)
                    .show(ui, |ui| self.library_panel(ui));
            });
    }

    /// Outline 콘텐츠 오버레이 — 절대 폭(460pt)의 독립 플로팅 창. Hide UI/Show UI 공용.
    fn outline_overlay(&mut self, ctx: &egui::Context, x: f32, top: f32) {
        let fill = crate::theme::nord::semantic::overlay_bg();
        let stroke = crate::theme::nord::semantic::OVERLAY_BORDER;
        egui::Window::new("outline_overlay")
            .title_bar(false)
            .movable(true)
            .resizable(false)
            .default_pos(egui::pos2(x, top))
            .frame(
                egui::Frame::new()
                    .fill(fill)
                    .stroke(egui::Stroke::new(1.0, stroke))
                    .corner_radius(10.0)
                    .inner_margin(egui::Margin::same(8)),
            )
            .show(ctx, |ui| {
                ui.set_width(460.0);
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
                // 헤더: 아이콘+제목+개수+닫기를 한 컨테이너로 (공용 헬퍼).
                // 개수가 첫 프레임부터 정확하도록 패널 표시 시점에 목차를 로드.
                self.load_outline_if_needed();
                let count = self.outline.len();
                if overlay_header(
                    ui,
                    icons::LIST_BULLETS,
                    "Outline",
                    &format!("{count} entries"),
                    "Close Outline",
                ) {
                    self.show_outline = false;
                }
                // 깊은 계층 때문에 창 폭이 늘어나 닫기 버튼이 밀리는 문제를 막기
                // 위해 콘텐츠 폭 상한을 창 폭(460pt)으로 고정합니다.
                egui::ScrollArea::vertical()
                    .id_salt("outline_overlay_scroll")
                    .max_height(520.0)
                    .max_width(444.0)
                    .show(ui, |ui| self.outline_panel(ui));
            });
    }

    /// Bookmarks 콘텐츠 오버레이 — 절대 폭(420pt)의 독립 플로팅 창. Hide UI/Show UI 공용.
    fn bookmarks_overlay(&mut self, ctx: &egui::Context, x: f32, top: f32) {
        let fill = crate::theme::nord::semantic::overlay_bg();
        let stroke = crate::theme::nord::semantic::OVERLAY_BORDER;
        let pages: Vec<PageIndex> = self.store.bookmarks().to_vec();
        egui::Window::new("bookmarks_overlay")
            .title_bar(false)
            .movable(true)
            .resizable(false)
            .default_pos(egui::pos2(x, top))
            .frame(
                egui::Frame::new()
                    .fill(fill)
                    .stroke(egui::Stroke::new(1.0, stroke))
                    .corner_radius(10.0)
                    .inner_margin(egui::Margin::same(8)),
            )
            .show(ctx, |ui| {
                ui.set_width(420.0);
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
                // 헤더: 아이콘+제목+개수+닫기를 한 컨테이너로 (공용 헬퍼).
                if overlay_header(
                    ui,
                    icons::BOOKMARKS_SIMPLE,
                    "Bookmarks",
                    &format!("{} bookmarks", pages.len()),
                    "Close Bookmarks",
                ) {
                    self.show_bookmarks = false;
                }
                egui::ScrollArea::vertical()
                    .id_salt("bookmarks_overlay_scroll")
                    .max_height(420.0)
                    .max_width(404.0)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(6.0, 2.0);
                        if pages.is_empty() {
                            ui.add_space(2.0);
                            ui.horizontal(|ui| {
                                ui.add_space(6.0);
                                ui.label(
                                    egui::RichText::new("No bookmarks yet").weak().small(),
                                );
                            });
                        } else {
                            // 계층 2: 행 — 호버 강조 + 클릭으로 이동.
                            for p in pages {
                                if library_row(ui, false, &format!("Page {}", p + 1), "") {
                                    self.goto_page(p);
                                }
                            }
                            ui.add_space(4.0);
                            ui.separator();
                            if ui
                                .button("Clear all bookmarks")
                                .on_hover_text("Remove every bookmark from this document")
                                .clicked()
                            {
                                self.clear_bookmarks();
                            }
                        }
                    });
            });
    }

    /// 최소(포커스) 모드의 **우측 컨테이너** — Palette / Show UI.
    /// 접으면 Palette 토글이 숨고 Show UI 버튼만 남습니다.
    fn minimal_chrome_controls(&mut self, ctx: &egui::Context) {
        let fill = crate::theme::nord::semantic::overlay_bg();
        let stroke = crate::theme::nord::semantic::OVERLAY_BORDER;
        egui::Window::new("minimal_chrome_controls")
            .title_bar(false)
            .movable(true)
            .resizable(false)
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-10.0, 8.0))
            .frame(
                egui::Frame::new()
                    .fill(fill)
                    .stroke(egui::Stroke::new(1.0, stroke))
                    .corner_radius(10.0)
                    .inner_margin(egui::Margin::same(6)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if self.minimal_chrome_collapsed {
                        // 접힘: 작은 Show UI(모서리 모으기)와 펼치기 아이콘만 남습니다.
                        if ui
                            .button(icon_text(ui, "", icons::CORNERS_IN))
                            .on_hover_text("Show all toolbars again (Ctrl+Shift+M)")
                            .clicked()
                        {
                            self.manual_minimal = false;
                            self.narrow_chrome_expanded = true;
                        }
                        if ui
                            .button(icon_text(ui, "", icons::ARROWS_OUT))
                            .on_hover_text("Show the Palette toggle")
                            .clicked()
                        {
                            self.minimal_chrome_collapsed = false;
                        }
                        return;
                    }
                    if ui
                        .selectable_label(
                            self.show_palette,
                            icon_text(ui, "Palette", icons::PALETTE),
                        )
                        .on_hover_text("Writing-tool color palette (right side of canvas)")
                        .clicked()
                    {
                        self.show_palette = !self.show_palette;
                        self.save_default_session();
                    }
                    if ui
                        .selectable_label(
                            self.edge_autoscroll,
                            icon_text(ui, "Auto Scroll", icons::ARROWS_OUT_CARDINAL),
                        )
                        .on_hover_text(
                            "Edge auto-scroll — cursor near the canvas edge pans the view.\n\
                             (zone / speeds / overscroll: Edge Auto Scroll settings in the toolbar)",
                        )
                        .clicked()
                    {
                        self.edge_autoscroll = !self.edge_autoscroll;
                        self.save_default_session();
                    }
                    if ui
                        .button(icon_text(ui, "Show UI", icons::CORNERS_IN))
                        .on_hover_text(
                            "Show all toolbars again.\n\
                             Shortcut: Ctrl+Shift+M",
                        )
                        .clicked()
                    {
                        self.manual_minimal = false;
                        self.narrow_chrome_expanded = true;
                    }
                    if ui
                        .button(icon_text(ui, "", icons::MINUS))
                        .on_hover_text("Collapse to a small button")
                        .clicked()
                    {
                        self.minimal_chrome_collapsed = true;
                    }
                });
            });
    }

    // ---------- Fallback dialog ----------

    fn fallback_dialog(&mut self, ctx: &egui::Context) {
        let Some(modal) = self.modal.clone() else {
            return;
        };
        let mut text = modal.text.clone();
        let mut pages = modal.pages;
        let mut ok = false;
        let mut cancel = false;

        match &modal.kind {
            ModalKind::AskText { title, hint, action } => {
                egui::Window::new(title)
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.label(hint);
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut text)
                                .hint_text("Type here...")
                                .desired_width(360.0),
                        );
                        if *action == TextAction::NewNote {
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                ui.label("Pages:");
                                egui::ComboBox::from_id_salt("note_pages")
                                    .selected_text(format!("{pages}"))
                                    .show_ui(ui, |ui| {
                                        for p in NOTE_PAGE_PRESETS {
                                            ui.selectable_value(
                                                &mut pages,
                                                *p,
                                                format!("{p} pages"),
                                            );
                                        }
                                    });
                            });
                        }
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ok = ui.button("OK").clicked();
                            cancel = ui.button("Cancel").clicked();
                        });
                        if resp.lost_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                            ok = true;
                        }
                    });
            }
            ModalKind::Confirm { title, message, .. } => {
                egui::Window::new(title)
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.label(message);
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ok = ui.button("Delete").clicked();
                            cancel = ui.button("Cancel").clicked();
                        });
                    });
            }
            ModalKind::Alert { title, message } => {
                egui::Window::new(title)
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.label(message);
                        ui.add_space(6.0);
                        if ui.button("OK").clicked() {
                            ok = true;
                        }
                        if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                            ok = true;
                        }
                    });
            }
        }

        // Keep text updated while typing
        if let Some(m) = &mut self.modal {
            m.text = text.clone();
            m.pages = pages;
        }

        if ok {
            let kind = self.modal.as_ref().map(|m| m.kind.clone());
            self.modal = None;
            if let Some(kind) = kind {
                match kind {
                    ModalKind::AskText { action, .. } if !text.trim().is_empty() => {
                        self.run_text_action(action, text, pages);
                    }
                    ModalKind::Confirm { action, .. } => self.run_confirm_action(action, text),
                    _ => {}
                }
            }
        } else if cancel {
            self.modal = None;
        }
    }
}

impl eframe::App for FreeDfApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // 펜 진단 로그는 Debug HUD가 켜져 있을 때만 (평소 I/O 비용 0).
        canvas::set_pen_trace(self.debug_hud);
        // 백그라운드 DB 연결 결과 수신 (연결 중이면 계속 갱신 요청).
        self.poll_connect_result();
        self.poll_loader();
        self.poll_media();
        self.poll_library();
        self.poll_pdf_import();
        self.poll_save(&ctx);
        self.poll_stroke_refill();
        if self.pending_connect.is_some()
            || self.loader_rx.is_some()
            || self.media_rx.is_some()
            || !self.library_rx.is_empty()
            || self.pdf_import_rx.is_some()
            || self.save_rx.is_some()
            || self.stroke_id_refill.is_some()
        {
            ctx.request_repaint();
        }
        // CLI 시작 인자: `freedf <file.pdf>`은 import 후 열기,
        // `freedf --doc <id>`는 DB의 문서 id로 열기("새 창" 분리 시 사용).
        if let Some(path) = self.pending_open.take() {
            self.open_pdf(&path);
        }
        // 새 창 프로세스는 DB 연결 없이 시작하므로, 먼저 저장된 URL로
        // **자동 연결**한 뒤 연결이 완료된 다음에만 문서를 엽니다
        // (연결 전 열기 → "Document not found" 방지). 순수 판정은
        // `startup_open_step` — 테스트로 검증.
        match startup_open_step(
            self.pending_doc,
            self.db_connected,
            self.pending_connect.is_some(),
        ) {
            StartupOpenAction::Connect => self.try_connect_server(true),
            StartupOpenAction::Open(doc_id) => {
                self.pending_doc = None;
                self.open_document(doc_id);
            }
            StartupOpenAction::Wait => {}
        }
        self.handle_shortcuts(&ctx);

        // ── 스플릿 뷰: 커서가 이 창 위에서 일정 시간(dwell) 활성화되면
        //    이 창에 포커스 — 0초면 즉시, 그 이상이면 머문 시간 기준.
        if self.window_focus_on_move
            && ctx.input(|i| i.pointer.hover_pos().is_some())
            && ctx.input(|i| i.viewport().focused == Some(false))
        {
            let now = now_ms();
            if self.window_hover_since_ms == 0 {
                self.window_hover_since_ms = now;
            }
            if dwell_focus_due(now, self.window_hover_since_ms, self.window_focus_dwell_sec) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            }
        } else {
            self.window_hover_since_ms = 0;
        }

        // 좁은 창에서는 캔버스 + 팔레트만 남기고 나머지는 자동으로 숨깁니다.
        // 판정 기준: (1) 창 폭이 모니터(뷰포트) 폭의 **절반 이하**일 때
        // (Windows 스플릿 뷰 = 정확히 절반) 또는 (2) 절대 폭이 너무 작을 때.
        // 우상단의 작은 조종 버튼(compact_pill)으로 전체 크롬을 다시 켭니다.
        let window_w = ctx.viewport_rect().width();
        let monitor_w = ctx.input(|i| i.viewport().monitor_size.map(|m| m.x));
        let narrow = match monitor_w {
            Some(mw) => {
                // 모니터 폭의 절반, 또는 최소 절대 폭 중 큰 쪽 이하.
                window_w <= (mw * 0.5).max(COMPACT_MIN_WIDTH) + 1.0
            }
            None => window_w < COMPACT_MIN_WIDTH,
        };
        if !narrow {
            // 자동 접힘은 창이 다시 넓어지면 해제하되, 수동 최소 모드는 유지.
            self.narrow_chrome_expanded = false;
        }
        // 자동: 좁은 창 & 아직 크롬을 다시 켜지 않았을 때 (시작 화면 제외).
        let auto_minimal = narrow && !self.narrow_chrome_expanded && !self.tabs.is_empty();
        // 수동: 화면 크기와 무관하게 Ctrl+Shift+M(또는 플로팅 버튼)으로
        // 모든 툴바를 숨깁니다 (팔레트는 유지).
        let minimal = self.manual_minimal || auto_minimal;

        if !minimal {
            self.tabs_bar(ui);
            self.toolbar(ui);
            self.status_bar(ui);
        }

        // ── Library / Outline / Bookmarks — 절대적 사이드 오버레이 ──
        // 패널 레이아웃(폭 비율)을 차지하지 않는 플로팅 창. Hide UI와
        // Show UI 모두에서 동일하게 사용하며, 드래그로 자유 배치 + ✕로 닫기.
        let overlay_top = ui.available_rect_before_wrap().top().max(8.0);
        let mut ox = 8.0f32;
        if self.show_library {
            self.library_overlay(&ctx, ox, overlay_top);
            ox += 528.0;
        }
        if self.show_outline {
            self.outline_overlay(&ctx, ox, overlay_top);
            ox += 468.0;
        }
        if self.show_bookmarks {
            self.bookmarks_overlay(&ctx, ox, overlay_top);
        }
        if !minimal && self.show_media {
            let panel_id = egui::Id::new(("media_panel", self.active));
            egui::Panel::left(panel_id)
                .resizable(true)
                .default_size(300.0)
                .min_size(200.0)
                .max_size(460.0)
                .show(ui, |ui| self.media_panel(ui));
        }

        egui::CentralPanel::default().show(ui, |ui| {
            self.canvas(ui);
        });

        // 플로팅 복귀 장치: 크롬이 숨겨진(최소) 모드에서만 표시.
        // 크롬이 보일 때의 Show/Hide UI 토글은 툴바 Row1이 담당합니다.
        if minimal {
            self.minimal_sections(&ctx);
            self.minimal_chrome_controls(&ctx);
        }

        self.connection_dialog(&ctx);
        self.fallback_dialog(&ctx);
        self.loading_overlay(&ctx);

        // Close confirmation: ask whether to save before quitting.
        let close_requested = ctx.input(|i| i.viewport().close_requested());
        if close_requested && !self.quitting {
            if !self.asking_close {
                self.asking_close = true;
            }
            // Cancel the native close and show our own confirmation dialog.
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        if self.asking_close {
            let mut decision: Option<bool> = None;
            egui::Window::new("Save before quitting?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(&ctx, |ui| {
                    ui.label("Save your current work before quitting?");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Save & Quit").clicked() {
                            decision = Some(true);
                        }
                        if ui.button("Quit").clicked() {
                            decision = Some(false);
                        }
                        if ui.button("Cancel").clicked() {
                            self.asking_close = false;
                        }
                    });
                });
            if let Some(save) = decision {
                self.asking_close = false;
                if save {
                    // 백그라운드 저장 — 완료되면 poll_save가 창을 닫습니다.
                    self.save_default_session();
                    self.save_session();
                    self.pending_quit = true;
                    self.flush_current_document();
                } else {
                    self.save_default_session();
                    self.save_session();
                    self.quitting = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }

        // High-refresh support: keep repainting while a document is open so
        // pen input and ink rendering stay smooth (120Hz+ displays).
        if self.document.is_some() || self.active_stroke.is_some() {
            ctx.request_repaint();
        }
    }
}

#[cfg(test)]
mod window_isolation_tests {
    use super::*;

    #[test]
    fn new_window_with_doc_connects_first() {
        // 문서 요청 + 연결 안 됨 + 시도 중 아님 → 먼저 자동 연결.
        assert_eq!(
            startup_open_step(Some(7), false, false),
            StartupOpenAction::Connect
        );
    }

    #[test]
    fn new_window_waits_while_connecting() {
        // 연결 시도 중에는 기다렸다가 연결 완료 후 열립니다.
        assert_eq!(
            startup_open_step(Some(7), false, true),
            StartupOpenAction::Wait
        );
    }

    #[test]
    fn new_window_opens_once_connected() {
        // 연결 완료 → 문서 id 7 열기.
        assert_eq!(
            startup_open_step(Some(7), true, false),
            StartupOpenAction::Open(7)
        );
    }

    #[test]
    fn no_request_does_nothing() {
        // --doc 없이 시작한 창은 아무것도 하지 않습니다.
        assert_eq!(startup_open_step(None, true, false), StartupOpenAction::Wait);
        assert_eq!(startup_open_step(None, false, false), StartupOpenAction::Wait);
    }

    #[test]
    fn unfocused_window_ignores_pen_button() {
        // 배경 창은 휠을 열지 않습니다 (같은 펜 장치를 공유해도).
        assert!(!wheel_toggle_allowed(None));
        assert!(!wheel_toggle_allowed(Some(false)));
        // 포커스된 창만 반응.
        assert!(wheel_toggle_allowed(Some(true)));
    }

    #[test]
    fn dwell_zero_focuses_immediately() {
        // 0초 지연 = 머물기 시작한 순간 포커스.
        assert!(dwell_focus_due(1000, 1000, 0.0));
    }

    #[test]
    fn dwell_waits_for_configured_time() {
        // 1초 지연 — 500ms는 부족, 1000ms면 충분.
        assert!(!dwell_focus_due(1500, 1000, 1.0));
        assert!(dwell_focus_due(2000, 1000, 1.0));
    }

    #[test]
    fn dwell_not_hovering_never_focuses() {
        // since_ms == 0 (아직 머물지 않음) → 항상 false.
        assert!(!dwell_focus_due(9999, 0, 0.0));
    }

    #[test]
    fn cursor_appears_after_three_stable_frames() {
        // want=true가 3프레임 연속이면 커서가 나타납니다.
        let (c1, s1) = cursor_hysteresis(false, true, 0, false, 3);
        assert_eq!((c1, s1), (1, false));
        let (c2, s2) = cursor_hysteresis(true, true, 1, false, 3);
        assert_eq!((c2, s2), (2, false));
        let (c3, s3) = cursor_hysteresis(true, true, 2, false, 3);
        assert_eq!((c3, s3), (3, true));
    }

    #[test]
    fn cursor_flip_resets_counter_and_keeps_state() {
        // want가 뒤집히면 새 실행의 1번째 프레임, 표시 상태는 유지.
        let (c, s) = cursor_hysteresis(true, false, 3, true, 3);
        assert_eq!((c, s), (1, true));
    }

    #[test]
    fn cursor_hides_after_three_stable_frames() {
        // want=false가 3프레임 연속이면 커서가 사라집니다.
        let (c1, s1) = cursor_hysteresis(true, false, 3, true, 3);
        let (c2, s2) = cursor_hysteresis(false, false, c1, s1, 3);
        let (c3, s3) = cursor_hysteresis(false, false, c2, s2, 3);
        assert_eq!((c3, s3), (3, false));
    }

    #[test]
    fn panels_are_mutually_exclusive() {
        // 어디서든 Library/Outline/Bookmarks 중 하나만 켜집니다.
        assert_eq!(
            exclusive_panel_on(PanelKind::Library),
            [true, false, false]
        );
        assert_eq!(
            exclusive_panel_on(PanelKind::Outline),
            [false, true, false]
        );
        assert_eq!(
            exclusive_panel_on(PanelKind::Bookmarks),
            [false, false, true]
        );
    }
}
