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
//! - [`actions`] — document & note actions: open / save / export, page CRUD,
//!   rotation, search, bookmarks, undo / redo
//!
//! The child modules each `use super::*;` — they extend `FreeDfApp` with more
//! inherent methods, so call sites keep working exactly as before.

mod actions;
mod canvas;
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
    clamp_line_width, clamp_spacing, paper_dots, paper_lines, PagePaper, PaperSize, PaperStyle,
    PAPER_COLORS, PAPER_LINE, PAPER_LINE_WIDTH_PT, PAPER_WHITE,
};
pub(crate) use freedf_core::pen::{
    base_width_factor, ink_modifier, ink_profile_hint, taper_factors, uses_own_profile,
    uses_taper, ColorFamily, OneEuroFilter, Palette, PressureCurve, TAPER_LEN_PTS,
};
pub(crate) use freedf_core::search::{char_line_highlights, find_matches, TextMatch, TextRun};
pub(crate) use freedf_core::store::AnnotationStore;
pub(crate) use freedf_core::transform::{PageAlign, ViewTransform, MAX_ZOOM, MIN_ZOOM, ZOOM_100_PERCENT};

pub(crate) use crate::export::draw_strokes_on_image;
pub(crate) use crate::pdf::DocumentView;
pub(crate) use crate::recent::{RecentItem, RecentKind, RecentList};
pub(crate) use crate::settings::MAX_FAVORITE_COLORS;
pub(crate) use egui_phosphor_icons::icons;
pub(crate) use pdfium_render::prelude::Pdfium;
use std::collections::HashSet;

/// Canvas margin around the page
const CANVAS_MARGIN: f32 = 16.0;
/// Page top margin
const TOP_MARGIN: f32 = 16.0;
/// Page transition animation duration (seconds)
const PAGE_ANIM_SECS: f32 = 0.28;
/// Window width (points) below which the UI collapses to canvas + palette
/// (Windows split view / narrow multitasking), with a floating control to
/// re-show the full chrome on demand.
const COMPACT_MIN_WIDTH: f32 = 640.0;
/// Smoothing rate (1/second) for animated wheel scroll.
const SCROLL_SMOOTH_RATE: f32 = 14.0;
/// Smoothing rate (1/second) for animated Ctrl+wheel zoom.
const ZOOM_SMOOTH_RATE: f32 = 16.0;

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
pub(crate) struct ActiveStroke {
    tool: ToolType,
    color: [u8; 4],
    width: f32,
    points: Vec<StrokePoint>,
}

impl ActiveStroke {
    fn push(&mut self, point: [f32; 2], pressure: f32) {
        self.points.push(StrokePoint::new(point[0], point[1], pressure));
    }
}

fn tool_label(tool: ToolType) -> &'static str {
    match tool {
        ToolType::Pen => "Pen",
        ToolType::Ballpoint => "Ballpoint",
        ToolType::Fountain => "Fountain",
        ToolType::Highlighter => "Highlighter",
        ToolType::Eraser => "Eraser",
        ToolType::Pan => "Pan",
    }
}

fn tool_icon(tool: ToolType) -> egui_phosphor_icons::Icon {
    match tool {
        ToolType::Pen => icons::PEN,
        ToolType::Ballpoint => icons::PEN_NIB_STRAIGHT,
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
fn toolbar_row<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.horizontal(|ui| add(ui)).inner
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum TextAction {
    NewNote,
    RenameNote,
    OpenPdf,
    SaveAnnotations,
    LoadAnnotations,
    ExportPng,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ConfirmAction {
    DeleteNote,
    /// Library 다중 삭제: 선택된 노트 id + PDF 경로 (PDF는 디스크에서 삭제).
    DeleteLibrary { notes: Vec<u64>, pdfs: Vec<PathBuf> },
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
        }
    }

    fn alert(title: &str, message: &str) -> Self {
        Self {
            kind: ModalKind::Alert {
                title: title.into(),
                message: message.into(),
            },
            text: String::new(),
        }
    }
}

/// 열려 있는 문서 탭의 종류.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabKind {
    /// FreeDF 노트 (id).
    Note(u64),
    /// 외부 PDF 파일 (경로).
    Pdf(PathBuf),
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
    file_path: Option<PathBuf>,
    current_note: Option<u64>,
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
    library_width: f32,
    outline_width: f32,
    tool: ToolType,
    color_family: ColorFamily,
    pen_color: [u8; 4],
    pen_width: f32,
    hi_color: [u8; 4],
    hi_width: f32,
    eraser_radius: f32,
    pressure_enabled: bool,
    pressure_curve: PressureCurve,
    paper_style: PaperStyle,
    paper_color: [u8; 4],
    paper_size: PaperSize,
    /// 줄/격자/점 간격 기본값 (pt)
    paper_spacing: f32,
    /// 줄/격자/점 색/두께 기본값 (pt) — 페이퍼 라인 옵션.
    paper_line_color: [u8; 4],
    paper_line_width: f32,
    /// 사용자 정의 용지 크기 [가로, 세로] (pt, `PaperSize::Custom`일 때)
    custom_paper_size: [f32; 2],
    /// 펜 입력 스무딩 강도 0..1
    smoothing: f32,
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
    // ---------- Notes ----------
    notes: NotesManager,
    current_note: Option<u64>,

    // ---------- Tabs (multiple open documents) ----------
    tabs: Vec<TabEntry>,
    active: usize,

    // ---------- Recent files ----------
    recents: RecentList,
    recent_path: PathBuf,

    // ---------- Document ----------
    document: Option<DocumentView>,
    /// PDFium loaded once at startup; reused for creating blank note PDFs.
    pdfium: Result<Box<Pdfium>, String>,
    file_path: Option<PathBuf>,
    current_page: usize,
    page_size_pts: [f32; 2],
    view: ViewTransform,
    page_align: PageAlign,
    last_canvas: [f32; 2],
    /// Canvas size from the previous frame (detects panel toggles / resizes).
    prev_canvas: [f32; 2],
    pending_fit: Option<FitMode>,
    texture: Option<egui::TextureHandle>,
    render_dirty: bool,
    last_render_zoom: f32,
    last_render_ppp: f32,

    // ---------- Annotations ----------
    store: AnnotationStore,
    history: History,

    // ---------- Tools ----------
    tool: ToolType,
    color_family: ColorFamily,
    pen_color: [u8; 4],
    pen_width: f32,
    hi_color: [u8; 4],
    hi_width: f32,
    eraser_radius: f32,
    pressure_enabled: bool,
    pressure_curve: PressureCurve,
    /// 펜 커서 모양 (펜 도구일 때)
    pen_cursor_style: PenCursorStyle,
    /// 도구 선택기 순서 (드래그 앤 드롭 재정렬)
    tool_order: Vec<ToolType>,
    /// 드래그 앤 드롭 상태 (임시)
    tool_drag: Option<usize>,
    tool_drop: Option<usize>,

    // ---------- Paper (grid / color / size) ----------
    paper_style: PaperStyle,
    paper_color: [u8; 4],
    /// 종이 크기 (새 페이지/노트 기본값)
    paper_size: PaperSize,
    /// 줄/격자/점 간격 기본값 (pt)
    paper_spacing: f32,
    /// 줄/격자/점 색 (RGBA)
    paper_line_color: [u8; 4],
    /// 줄/격자/점 두께 기본값 (pt)
    paper_line_width: f32,
    /// 사용자 정의 용지 크기 [가로, 세로] (pt, `PaperSize::Custom`일 때)
    custom_paper_size: [f32; 2],
    /// 펜 입력 스무딩 강도 0..1
    smoothing: f32,
    /// 줌 잠금 (휠/핀치/단축키 줌 무시)
    zoom_lock: bool,

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
    /// Ctrl+wheel zoom acceleration ramp (0.01 per notch, capped)
    zoom_accel: f32,
    /// Time of the last Ctrl+wheel notch (used to restart the ramp)
    zoom_accel_last: f64,
    /// Animated (eased) zoom target — set by Ctrl+wheel notches; the actual
    /// `view.zoom` glides toward it for a few frames (smooth, no jumps).
    zoom_target: Option<f32>,
    /// Page-space point that stays under the cursor while zoom animates.
    zoom_anchor_page: Option<[f32; 2]>,
    /// Canvas-space cursor position used as the zoom anchor.
    zoom_anchor_ui: Option<[f32; 2]>,
    /// Page change slide animation
    page_anim: Option<PageAnim>,
    /// 다음 페이지 전환을 세로로 할지 (PgUp/PgDn 키가 세팅) — 시작 시 소비
    transition_vertical: bool,
    /// Texture of the outgoing page during a transition
    prev_texture: Option<egui::TextureHandle>,
    /// Page index before the latest page change (drives the animation direction)
    transition_last_page: usize,

    // ---------- Compact (narrow / split-view) mode ----------
    /// While the window is narrow the UI collapses to canvas + palette; set to
    /// `true` to temporarily show the full chrome (tabs/toolbar) again.
    narrow_chrome_expanded: bool,
    /// Manual "focus" mode: hides all toolbars regardless of the window width
    /// (toggled with Ctrl+Shift+M, or from the floating pill). Always shows the
    /// writing palette; the pill restores the chrome.
    manual_minimal: bool,

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
    /// Library / Outline panel widths (tracked per tab & persisted in session)
    library_width: f32,
    outline_width: f32,
    /// Canvas right-side writing-tool / color palette (global pref)
    show_palette: bool,
    /// Frequently-used pen colors (global pref)
    favorite_colors: Vec<[u8; 4]>,
    /// Highlighter snaps to recognized document text (global pref)
    text_highlight_snap: bool,
    /// Library 패널 검색 필터 (일시적)
    library_filter: String,
    /// Library 패널 다중 삭제 선택 상태 (일시적)
    sel_notes: HashSet<u64>,
    sel_pdfs: HashSet<PathBuf>,

    // ---------- Logging / status ----------
    logger: Logger,
    file_name: String,
    status: Option<String>,
    /// (message, time set) so the transient status line auto-clears
    status_since: Option<(String, f64)>,

    // ---------- Default session (global GUI state) ----------
    default_session_path: PathBuf,

    // ---------- CLI startup / new-window ----------
    /// A standalone PDF passed on the command line (`freedf <file>.pdf`);
    /// opened on the very first frame of this window.
    pending_open: Option<PathBuf>,

    // ---------- Fallback dialog ----------
    modal: Option<ModalState>,
    // ---------- Close confirmation ----------
    asking_close: bool,
    quitting: bool,
}

impl FreeDfApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        notes: NotesManager,
        logger: Logger,
        default_session_path: PathBuf,
        pending_open: Option<PathBuf>,
    ) -> Self {
        // Disable egui's built-in Ctrl+scroll zoom folding: it multiplies the
        // zoom by exp(speed * scroll), which jumps ~28% per wheel notch. We do
        // discrete +1% zoom ourselves (see handle_canvas_input), so keep egui's
        // fold a no-op while still allowing real pinch (Event::Zoom).
        cc.egui_ctx.options_mut(|o| o.input_options.scroll_zoom_speed = 0.0);

        let dark = matches!(cc.egui_ctx.theme(), egui::Theme::Dark);
        let theme_pen = if dark {
            [255, 255, 255, 255]
        } else {
            Palette::default_pen()
        };
        let theme_hi = if dark {
            [255, 220, 60, 110]
        } else {
            Palette::default_highlighter()
        };
        // 전역 기본 세션(마지막 펜 색/용지/도구 등)을 복원하고, 없으면 테마 기본값.
        // (이전 버전의 settings.json이 있으면 session.json으로 마이그레이션)
        if !default_session_path.exists() {
            let old = default_session_path.with_file_name("settings.json");
            if old.exists() {
                let s: crate::settings::SessionState = crate::settings::load_json(&old);
                s.save(&default_session_path);
            }
        }
        let s = crate::settings::SessionState::load(&default_session_path);
        let has = default_session_path.exists();
        let pen_color = if has { s.pen_color } else { theme_pen };
        let hi_color = if has { s.hi_color } else { theme_hi };
        let tool = if has { s.tool } else { ToolType::Pen };
        let color_family = if has { s.color_family } else { ColorFamily::Black };
        let pen_width = if has { s.pen_width } else { 2.5 };
        let hi_width = if has { s.hi_width } else { 16.0 };
        let eraser_radius = if has { s.eraser_radius } else { 16.0 };
        let pressure_enabled = if has { s.pressure_enabled } else { true };
        let pressure_curve = if has {
            s.pressure_curve
        } else {
            PressureCurve::default()
        };
        let paper_style = if has { s.paper_style } else { PaperStyle::Blank };
        let paper_color = if has { s.paper_color } else { PAPER_WHITE };
        let paper_size = if has { s.paper_size } else { PaperSize::A4 };
        let paper_spacing = if has {
            clamp_spacing(s.paper_spacing)
        } else {
            24.0
        };
        let paper_line_color = if has { s.paper_line_color } else { PAPER_LINE };
        let paper_line_width = if has {
            clamp_line_width(s.paper_line_width)
        } else {
            PAPER_LINE_WIDTH_PT
        };
        let show_library = if has { s.show_notes } else { true };
        let show_outline = if has { s.show_outline } else { false };
        let library_width = if has { s.library_width } else { 260.0 };
        let outline_width = if has { s.outline_width } else { 240.0 };
        let show_palette = if has { s.show_palette } else { true };
        let mut favorite_colors = if has {
            s.favorite_colors.clone()
        } else {
            crate::settings::SessionState::default().favorite_colors
        };
        // 팔레트는 최대 3색 — 이전 버전 세션의 8색 목록은 잘라냅니다.
        favorite_colors.truncate(MAX_FAVORITE_COLORS);
        if favorite_colors.is_empty() {
            favorite_colors = crate::settings::SessionState::default().favorite_colors;
        }
        let text_highlight_snap = if has { s.text_highlight_snap } else { false };
        let zoom_lock = if has { s.zoom_lock } else { false };
        let smoothing = if has { s.smoothing.clamp(0.0, 1.0) } else { 0.4 };
        let custom_paper_size = if let Some(c) = s.custom_paper_size {
            [
                c[0].clamp(100.0, 2400.0),
                c[1].clamp(100.0, 2400.0),
            ]
        } else {
            PaperSize::A4.size_pts()
        };
        let tool_order = if has {
            s.tool_order.clone()
        } else {
            ToolType::default_order()
        };
        let library_filter = String::new();
        // Recent files live next to the default session file in the app data folder.
        let recent_path = default_session_path
            .parent()
            .map(|p| p.join("recent.json"))
            .unwrap_or_else(|| PathBuf::from("recent.json"));
        let recents = RecentList::load(&recent_path);
        Self {
            notes,
            default_session_path,
            tabs: Vec::new(),
            active: 0,
            recents,
            recent_path,
            current_note: None,
            document: None,
            pdfium: crate::pdf::load_pdfium().map(Box::new),
            file_path: None,
            current_page: 0,
            page_size_pts: PaperSize::A4.size_pts(),
            view: ViewTransform::default(),
            page_align: PageAlign::Center,
            last_canvas: [1280.0, 600.0],
            prev_canvas: [1280.0, 600.0],
            pending_fit: None,
            texture: None,
            render_dirty: true,
            last_render_zoom: 0.0,
            last_render_ppp: 0.0,
            store: AnnotationStore::new(),
            history: History::new(256),
            tool,
            color_family,
            pen_color,
            pen_width,
            hi_color,
            hi_width,
            eraser_radius,
            pressure_enabled,
            pressure_curve,
            pen_cursor_style: PenCursorStyle::Round,
            tool_order,
            tool_drag: None,
            tool_drop: None,
            paper_style,
            paper_color,
            paper_size,
            paper_spacing,
            paper_line_color,
            paper_line_width,
            custom_paper_size,
            smoothing,
            zoom_lock,
            active_stroke: None,
            pan_last: None,
            middle_pan_last: None,
            smooth_x: OneEuroFilter::from_smoothing(0.4),
            smooth_y: OneEuroFilter::from_smoothing(0.4),
            smooth_p: OneEuroFilter::from_smoothing(0.4),
            smooth_active: false,
            scroll_vel: Vec2::ZERO,
            zoom_accel: 0.0,
            zoom_accel_last: 0.0,
            zoom_target: None,
            zoom_anchor_page: None,
            zoom_anchor_ui: None,
            page_anim: None,
            transition_vertical: false,
            prev_texture: None,
            transition_last_page: 0,
            narrow_chrome_expanded: false,
            manual_minimal: false,
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
            library_width,
            outline_width,
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
            modal: None,
            asking_close: false,
            quitting: false,
        }
    }

    /// 전역 기본 세션(마지막 펜 색/용지/도구 등)을 저장해 다음 시작 시 복원합니다.
    fn save_default_session(&self) {
        crate::settings::SessionState {
            page: 0,
            tool: self.tool,
            color_family: self.color_family,
            pen_color: self.pen_color,
            pen_width: self.pen_width,
            hi_color: self.hi_color,
            hi_width: self.hi_width,
            eraser_radius: self.eraser_radius,
            pressure_enabled: self.pressure_enabled,
            pressure_curve: self.pressure_curve,
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
            page_align: self.page_align,
            paper_style: self.paper_style,
            paper_color: self.paper_color,
            paper_size: self.paper_size,
            paper_spacing: self.paper_spacing,
            paper_line_color: self.paper_line_color,
            paper_line_width: self.paper_line_width,
            show_notes: self.show_library,
            show_outline: self.show_outline,
            library_width: self.library_width,
            outline_width: self.outline_width,
            show_palette: self.show_palette,
            favorite_colors: self.favorite_colors.clone(),
            text_highlight_snap: self.text_highlight_snap,
            tool_order: self.tool_order.clone(),
            zoom_lock: self.zoom_lock,
            smoothing: self.smoothing,
            custom_paper_size: Some(self.custom_paper_size),
        }
        .save(&self.default_session_path);
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
                spacing: self.paper_spacing,
                line_color: self.paper_line_color,
                line_width: self.paper_line_width,
            },
        )
    }

    /// 툴바의 용지 기본값을 현재 페이지에 저장하고 다시 그립니다.
    fn apply_paper_to_current_page(&mut self) {
        if let Some(doc) = &self.document {
            if self.current_page < doc.page_count() {
                self.store.set_paper(
                    self.current_page,
                    PagePaper {
                        style: self.paper_style,
                        color: self.paper_color,
                        spacing: self.paper_spacing,
                        line_color: self.paper_line_color,
                        line_width: self.paper_line_width,
                    },
                );
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
            hi_color: self.hi_color,
            hi_width: self.hi_width,
            eraser_radius: self.eraser_radius,
            pressure_enabled: self.pressure_enabled,
            pressure_curve: self.pressure_curve,
            zoom: self.view.zoom,
            pan_x: self.view.pan_x,
            pan_y: self.view.pan_y,
            page_align: self.page_align,
            paper_style: self.paper_style,
            paper_color: self.paper_color,
            paper_size: self.paper_size,
            paper_spacing: self.paper_spacing,
            paper_line_color: self.paper_line_color,
            paper_line_width: self.paper_line_width,
            show_notes: self.show_library,
            show_outline: self.show_outline,
            library_width: self.library_width,
            outline_width: self.outline_width,
            show_palette: self.show_palette,
            favorite_colors: self.favorite_colors.clone(),
            text_highlight_snap: self.text_highlight_snap,
            tool_order: self.tool_order.clone(),
            zoom_lock: self.zoom_lock,
            smoothing: self.smoothing,
            custom_paper_size: Some(self.custom_paper_size),
        }
    }

    /// 현재 문서의 세션 파일 경로 (노트 폴더 또는 PDF 옆 사이드카).
    fn session_path(&self) -> Option<PathBuf> {
        if let Some(id) = self.current_note {
            Some(self.notes.session_path(id))
        } else {
            self.file_path.as_deref().map(session_path_for)
        }
    }

    /// 열려 있는 문서의 GUI 상태를 세션 파일에 저장합니다.
    fn save_session(&self) {
        if self.document.is_none() {
            return;
        }
        if let Some(path) = self.session_path() {
            self.capture_session().save(&path);
        }
    }

    /// 저장된 세션을 현재 문서에 적용합니다. `page_count`는 페이지 상한입니다.
    fn apply_session(&mut self, s: &crate::settings::SessionState, page_count: usize) {
        self.current_page = s.page.min(page_count.saturating_sub(1));
        self.tool = s.tool;
        self.color_family = s.color_family;
        self.pen_color = s.pen_color;
        self.pen_width = s.pen_width.clamp(0.5, 12.0);
        self.hi_color = s.hi_color;
        self.hi_width = s.hi_width.clamp(4.0, 40.0);
        self.eraser_radius = s.eraser_radius.clamp(4.0, 60.0);
        self.pressure_enabled = s.pressure_enabled;
        self.pressure_curve = s.pressure_curve;
        self.page_align = s.page_align;
        self.paper_style = s.paper_style;
        self.paper_color = s.paper_color;
        self.paper_size = s.paper_size;
        self.paper_spacing = clamp_spacing(s.paper_spacing);
        self.paper_line_color = s.paper_line_color;
        self.paper_line_width = clamp_line_width(s.paper_line_width);
        self.zoom_lock = s.zoom_lock;
        self.smoothing = s.smoothing.clamp(0.0, 1.0);
        if let Some(c) = s.custom_paper_size {
            self.custom_paper_size = [c[0].clamp(100.0, 2400.0), c[1].clamp(100.0, 2400.0)];
        }
        self.show_library = s.show_notes;
        self.show_outline = s.show_outline;
        self.library_width = s.library_width.clamp(160.0, 460.0);
        self.outline_width = s.outline_width.clamp(160.0, 460.0);
        if let Some(doc) = &self.document {
            self.page_size_pts = doc.page_size_pts(self.current_page);
            self.view.zoom = s.zoom.clamp(MIN_ZOOM, MAX_ZOOM);
            self.view.pan_x = s.pan_x;
            self.view.pan_y = s.pan_y;
            self.view
                .clamp_pan(self.page_size_pts, self.last_canvas, CANVAS_MARGIN);
        }
        self.render_dirty = true;
        self.search_update();
    }

    // ---------- Tabs (multiple open documents) ----------

    /// 같은 대상(노트 id 또는 파일 경로)이 이미 열려 있으면 탭 인덱스 반환.
    fn note_recent(
        &mut self,
        kind: RecentKind,
        title: String,
        note_id: Option<u64>,
        path: Option<PathBuf>,
    ) {
        let opened_at_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        self.recents
            .touch(RecentItem {
                kind,
                note_id,
                path,
                title,
                opened_at_ms,
            });
        self.recents.save(&self.recent_path);
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
            self.modal = Some(ModalState::ask_text(
                "New Note",
                "Note title:",
                TextAction::NewNote,
            ));
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
        if ctrl && ctx.input(|i| i.key_pressed(egui::Key::E)) {
            self.export_png();
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
            if self.zoom_lock {
                // 진행 중이던 줌 애니메이션은 즉시 종료합니다.
                if let Some(t) = self.zoom_target {
                    self.view.zoom = t.clamp(MIN_ZOOM, MAX_ZOOM);
                    self.render_dirty = true;
                }
                self.zoom_target = None;
                self.zoom_anchor_page = None;
                self.zoom_anchor_ui = None;
            }
            self.save_default_session();
            self.save_session();
        }
        // PgDn / PgUp = 다음/이전 페이지 (스크롤이 아니라 페이지 이동).
        // 노트에서는 PgDn 시 마지막 페이지면 새 페이지를 자동으로 추가합니다.
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
                self.zoom_by(1.25);
            }
            if !self.zoom_lock && ctx.input(|i| i.key_pressed(egui::Key::Minus)) {
                self.zoom_by(1.0 / 1.25);
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

    // ---------- Compact (narrow-window) floating control ----------

    /// 좁은 창(스플릿 뷰)에서 우상단에 뜨는 작은 조종 버튼.
    ///
    /// - 크롬(탭/툴바)이 접혀 있으면 "☰ Show UI"를 눌러 전체 크롬을 잠시
    ///   다시 켜고, 켜져 있으면 "✕"로 다시 접습니다.
    /// - 필기 팔레트(오른쪽 색상 바)도 여기서 켜고 끌 수 있습니다.
    fn compact_pill(&mut self, ctx: &egui::Context, minimal: bool, narrow: bool) {
        egui::Area::new(egui::Id::new("compact_pill"))
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-10.0, 8.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                egui::Frame::window(&ui.style())
                    .inner_margin(egui::Margin::same(6))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let label = if minimal {
                                "☰  Show UI"
                            } else {
                                "✕  Hide UI"
                            };
                            if ui
                                .button(label)
                                .on_hover_text(
                                    "Show/hide all toolbars. The canvas and palette stay \
                                     available. Shortcut: Ctrl+Shift+M",
                                )
                                .clicked()
                            {
                                if minimal {
                                    // 숨김 → 크롬 표시.
                                    self.manual_minimal = false;
                                    self.narrow_chrome_expanded = true;
                                } else if narrow {
                                    // 좁은 자동 모드에서 크롬 표시 → 다시 접기.
                                    self.narrow_chrome_expanded = false;
                                } else {
                                    // 넓은 창에서 크롬 표시 → 수동 최소 모드로.
                                    self.manual_minimal = true;
                                }
                            }
                            if ui
                                .selectable_label(self.show_palette, "Palette")
                                .on_hover_text("Show the writing-tool / color palette")
                                .clicked()
                            {
                                self.show_palette = !self.show_palette;
                                self.save_default_session();
                            }
                        });
                    });
            });
    }

    // ---------- Fallback dialog ----------

    fn fallback_dialog(&mut self, ctx: &egui::Context) {
        let Some(modal) = self.modal.clone() else {
            return;
        };
        let mut text = modal.text.clone();
        let mut ok = false;
        let mut cancel = false;

        match &modal.kind {
            ModalKind::AskText { title, hint, .. } => {
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
        }

        if ok {
            let kind = self.modal.as_ref().map(|m| m.kind.clone());
            self.modal = None;
            if let Some(kind) = kind {
                match kind {
                    ModalKind::AskText { action, .. } if !text.trim().is_empty() => {
                        self.run_text_action(action, text);
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
        // A standalone PDF passed on the command line (`freedf <file>.pdf` —
        // also how "Open in New Window" relaunches) opens once on the first frame.
        if let Some(path) = self.pending_open.take() {
            self.open_pdf(&path);
        }
        self.handle_shortcuts(&ctx);

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

        if !minimal && self.show_library {
            // Per-tab panel id: each tab keeps its own width in egui memory,
            // and the width is read back each frame into `self.library_width`
            // so it survives tab switches and app restarts (via session.json).
            let panel_id = egui::Id::new(("library_panel", self.active));
            let resp = egui::Panel::left(panel_id)
                .resizable(true)
                .default_size(self.library_width)
                .min_size(160.0)
                .max_size(460.0)
                .show(ui, |ui| self.library_panel(ui));
            self.library_width = resp.response.rect.width().clamp(160.0, 460.0);
        }
        if !minimal && self.show_outline {
            let panel_id = egui::Id::new(("outline_panel", self.active));
            let resp = egui::Panel::left(panel_id)
                .resizable(true)
                .default_size(self.outline_width)
                .min_size(160.0)
                .max_size(460.0)
                .show(ui, |ui| self.outline_panel(ui));
            self.outline_width = resp.response.rect.width().clamp(160.0, 460.0);
        }

        egui::CentralPanel::default().show(ui, |ui| {
            self.canvas(ui);
        });

        // 플로팅 제어: 좁은 창이거나 수동 최소 모드일 때만 표시 (복귀 장치).
        if narrow || self.manual_minimal {
            self.compact_pill(&ctx, minimal, narrow);
        }

        self.fallback_dialog(&ctx);

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
                if save {
                    // Re-save everything before quitting.
                    self.autosave();
                    let _ = self.notes.save();
                    self.save_pdf_if_note();
                }
                self.save_default_session();
                self.save_session();
                self.recents.save(&self.recent_path);
                self.quitting = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }

        // High-refresh support: keep repainting while a document is open so
        // pen input and ink rendering stay smooth (120Hz+ displays).
        if self.document.is_some() || self.active_stroke.is_some() {
            ctx.request_repaint();
        }
    }
}

/// Sidecar annotation path for a standalone PDF (`doc.pdf.freedf.json`).
fn annotation_path_for(pdf_path: &Path) -> PathBuf {
    let mut os = pdf_path.as_os_str().to_os_string();
    os.push(".freedf.json");
    PathBuf::from(os)
}

/// Sidecar session path for a standalone PDF (`doc.pdf.session.json`).
fn session_path_for(pdf_path: &Path) -> PathBuf {
    let mut os = pdf_path.as_os_str().to_os_string();
    os.push(".session.json");
    PathBuf::from(os)
}
