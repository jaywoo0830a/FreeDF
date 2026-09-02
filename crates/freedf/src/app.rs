//! FreeDF main app: PDF viewer canvas + drawing-pad annotation + notes/outline/search.
//!
//! English-only UI. Screen coordinates map 1:1 to egui's canvas space; the canvas
//! top-left equals `response.rect.min`, and page <-> view coordinates are handled
//! by `freedf_core::transform::ViewTransform`.

use std::path::{Path, PathBuf};

use eframe::egui;
use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};

use freedf_core::history::{Edit, History};
use freedf_core::logging::{AppEvent, Logger};
use freedf_core::model::{PageIndex, StrokePoint, ToolType};
use freedf_core::notes::NotesManager;
use freedf_core::outline::{flatten, OutlineNode};
use freedf_core::paper::{
    clamp_spacing, paper_dots, paper_lines, PagePaper, PaperSize, PaperStyle, PAPER_COLORS,
    PAPER_WHITE,
};
use freedf_core::pen::{ColorFamily, Palette, PressureCurve};
use freedf_core::search::{find_matches, text_line_highlights, TextMatch, TextRun};
use freedf_core::store::AnnotationStore;
use freedf_core::transform::{PageAlign, ViewTransform, MAX_ZOOM, MIN_ZOOM, ZOOM_100_PERCENT};

use crate::export::draw_strokes_on_image;
use crate::pdf::DocumentView;
use crate::recent::{RecentItem, RecentKind, RecentList};
use egui_phosphor_icons::icons;
use pdfium_render::prelude::Pdfium;

/// Canvas margin around the page
const CANVAS_MARGIN: f32 = 16.0;
/// Page top margin
const TOP_MARGIN: f32 = 16.0;
/// Page transition animation duration (seconds)
const PAGE_ANIM_SECS: f32 = 0.28;
/// Scroll momentum decay (1/second)
const SCROLL_DECAY: f32 = 6.0;

/// Fit mode
#[derive(Debug, Clone, Copy, PartialEq)]
enum FitMode {
    /// Fit page width
    Width,
    /// Fit page height
    Height,
}

/// In-progress page transition (slide).
struct PageAnim {
    /// 0.0 (start) .. 1.0 (done)
    progress: f32,
    /// +1.0 = next page (slides in from the right), -1.0 = previous (from the left)
    direction: f32,
}

/// A stroke currently being drawn
struct ActiveStroke {
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
        ToolType::Highlighter => "Highlighter",
        ToolType::Eraser => "Eraser",
        ToolType::Pan => "Pan",
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
enum TextAction {
    NewNote,
    RenameNote,
    OpenPdf,
    SaveAnnotations,
    LoadAnnotations,
    ExportPng,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ConfirmAction {
    DeleteNote,
}

/// 새 빈 페이지를 삽입하는 위치/방식.
#[derive(Debug, Clone, Copy, PartialEq)]
enum InsertTarget {
    /// 현재 페이지의 크기/용지를 그대로 써서 바로 다음에 삽입
    FromCurrent,
    /// 문서 맨 앞(0번)에 삽입
    FrontBegin,
    /// 문서 맨 끝에 삽입
    FrontEnd,
    /// 현재 페이지 앞에 삽입
    BeforeCurrent,
    /// 현재 페이지 뒤에 삽입
    AfterCurrent,
}

#[derive(Debug, Clone)]
enum ModalKind {
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
struct ModalState {
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

    // ---------- Paper (grid / color / size) ----------
    paper_style: PaperStyle,
    paper_color: [u8; 4],
    /// 종이 크기 (새 페이지/노트 기본값)
    paper_size: PaperSize,
    /// 줄/격자/점 간격 기본값 (pt)
    paper_spacing: f32,

    // ---------- Input ----------
    active_stroke: Option<ActiveStroke>,
    pan_last: Option<Pos2>,
    middle_pan_last: Option<Pos2>,
    /// Trackpad/wheel momentum (points/sec) for inertial panning
    scroll_vel: Vec2,
    /// Ctrl+wheel zoom acceleration ramp (0.01 per notch, capped)
    zoom_accel: f32,
    /// Time of the last Ctrl+wheel notch (used to restart the ramp)
    zoom_accel_last: f64,
    /// Page change slide animation
    page_anim: Option<PageAnim>,
    /// Texture of the outgoing page during a transition
    prev_texture: Option<egui::TextureHandle>,
    /// Page index before the latest page change (drives the animation direction)
    transition_last_page: usize,

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

    // ---------- Logging / status ----------
    logger: Logger,
    file_name: String,
    status: Option<String>,
    /// (message, time set) so the transient status line auto-clears
    status_since: Option<(String, f64)>,

    // ---------- Default session (global GUI state) ----------
    default_session_path: PathBuf,

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
        let show_library = if has { s.show_notes } else { true };
        let show_outline = if has { s.show_outline } else { false };
        let library_width = if has { s.library_width } else { 260.0 };
        let outline_width = if has { s.outline_width } else { 240.0 };
        let show_palette = if has { s.show_palette } else { true };
        let favorite_colors = if has {
            s.favorite_colors.clone()
        } else {
            crate::settings::SessionState::default().favorite_colors
        };
        let text_highlight_snap = if has { s.text_highlight_snap } else { true };
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
            paper_style,
            paper_color,
            paper_size,
            paper_spacing,
            active_stroke: None,
            pan_last: None,
            middle_pan_last: None,
            scroll_vel: Vec2::ZERO,
            zoom_accel: 0.0,
            zoom_accel_last: 0.0,
            page_anim: None,
            prev_texture: None,
            transition_last_page: 0,
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
            logger,
            file_name: String::new(),
            status: None,
            status_since: None,
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
            show_notes: self.show_library,
            show_outline: self.show_outline,
            library_width: self.library_width,
            outline_width: self.outline_width,
            show_palette: self.show_palette,
            favorite_colors: self.favorite_colors.clone(),
            text_highlight_snap: self.text_highlight_snap,
        }
        .save(&self.default_session_path);
    }

    /// 현재 페이지의 용지 설정 (저장된 값, 없으면 툴바 기본값).
    fn current_page_paper(&self) -> PagePaper {
        self.store.paper_on_or(
            self.current_page,
            PagePaper {
                style: self.paper_style,
                color: self.paper_color,
                spacing: self.paper_spacing,
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
            show_notes: self.show_library,
            show_outline: self.show_outline,
            library_width: self.library_width,
            outline_width: self.outline_width,
            show_palette: self.show_palette,
            favorite_colors: self.favorite_colors.clone(),
            text_highlight_snap: self.text_highlight_snap,
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
    fn find_tab(&self, kind: &TabKind) -> Option<usize> {
        self.tabs.iter().position(|t| &t.kind == kind)
    }

    /// 현재 활성 문서 상태를 `tabs[idx]`에 복사해 둡니다.
    fn capture_into(&mut self, idx: usize) {
        let Some(tab) = self.tabs.get_mut(idx) else {
            return;
        };
        tab.label = self.file_name.clone();
        tab.file_path = self.file_path.clone();
        tab.current_note = self.current_note;
        tab.document = self.document.take();
        tab.current_page = self.current_page;
        tab.page_size_pts = self.page_size_pts;
        tab.view = self.view;
        tab.page_align = self.page_align;
        tab.store = std::mem::take(&mut self.store);
        tab.history = std::mem::take(&mut self.history);
        tab.search_query = std::mem::take(&mut self.search_query);
        tab.search_matches = std::mem::take(&mut self.search_matches);
        tab.search_current = self.search_current.take();
        tab.outline = std::mem::take(&mut self.outline);
        tab.outline_loaded = self.outline_loaded;
        // Per-tab UI state.
        tab.show_library = self.show_library;
        tab.show_outline = self.show_outline;
        tab.show_search = self.show_search;
        tab.library_width = self.library_width;
        tab.outline_width = self.outline_width;
        tab.tool = self.tool;
        tab.color_family = self.color_family;
        tab.pen_color = self.pen_color;
        tab.pen_width = self.pen_width;
        tab.hi_color = self.hi_color;
        tab.hi_width = self.hi_width;
        tab.eraser_radius = self.eraser_radius;
        tab.pressure_enabled = self.pressure_enabled;
        tab.pressure_curve = self.pressure_curve;
        tab.paper_style = self.paper_style;
        tab.paper_color = self.paper_color;
        tab.paper_size = self.paper_size;
        tab.paper_spacing = self.paper_spacing;
    }

    /// `tabs[idx]`의 상태를 활성 문서로 복원합니다. (활성 탭의 document는 None이 됨)
    fn restore_from(&mut self, idx: usize) {
        let (
            label,
            file_path,
            current_note,
            document,
            current_page,
            page_size_pts,
            view,
            page_align,
            store,
            history,
            search_query,
            search_matches,
            search_current,
            outline,
            outline_loaded,
            show_library,
            show_outline,
            show_search,
            library_width,
            outline_width,
            tool,
            color_family,
            pen_color,
            pen_width,
            hi_color,
            hi_width,
            eraser_radius,
            pressure_enabled,
            pressure_curve,
            paper_style,
            paper_color,
            paper_size,
            paper_spacing,
        ) = {
            let tab = self.tabs.get_mut(idx).expect("tab index in range");
            (
                std::mem::take(&mut tab.label),
                tab.file_path.clone(),
                tab.current_note,
                tab.document.take(),
                tab.current_page,
                tab.page_size_pts,
                tab.view,
                tab.page_align,
                std::mem::take(&mut tab.store),
                std::mem::take(&mut tab.history),
                std::mem::take(&mut tab.search_query),
                std::mem::take(&mut tab.search_matches),
                tab.search_current.take(),
                std::mem::take(&mut tab.outline),
                tab.outline_loaded,
                tab.show_library,
                tab.show_outline,
                tab.show_search,
                tab.library_width,
                tab.outline_width,
                tab.tool,
                tab.color_family,
                tab.pen_color,
                tab.pen_width,
                tab.hi_color,
                tab.hi_width,
                tab.eraser_radius,
                tab.pressure_enabled,
                tab.pressure_curve,
                tab.paper_style,
                tab.paper_color,
                tab.paper_size,
                tab.paper_spacing,
            )
        };
        // 일시적인 렌더/입력 상태 초기화.
        self.texture = None;
        self.render_dirty = true;
        self.pending_fit = None;
        self.page_anim = None;
        self.prev_texture = None;
        self.active_stroke = None;
        self.pan_last = None;
        self.middle_pan_last = None;
        self.scroll_vel = Vec2::ZERO;
        self.transition_last_page = current_page;
        self.file_name = label;
        self.file_path = file_path;
        self.current_note = current_note;
        self.document = document;
        self.current_page = current_page;
        self.page_size_pts = page_size_pts;
        self.view = view;
        self.page_align = page_align;
        self.store = store;
        self.history = history;
        self.search_query = search_query;
        self.search_matches = search_matches;
        self.search_current = search_current;
        self.outline = outline;
        self.outline_loaded = outline_loaded;
        // Per-tab UI state (panels, tools, paper).
        self.show_library = show_library;
        self.show_outline = show_outline;
        self.show_search = show_search;
        self.library_width = library_width;
        self.outline_width = outline_width;
        self.tool = tool;
        self.color_family = color_family;
        self.pen_color = pen_color;
        self.pen_width = pen_width;
        self.hi_color = hi_color;
        self.hi_width = hi_width;
        self.eraser_radius = eraser_radius;
        self.pressure_enabled = pressure_enabled;
        self.pressure_curve = pressure_curve;
        self.paper_style = paper_style;
        self.paper_color = paper_color;
        self.paper_size = paper_size;
        self.paper_spacing = paper_spacing;
        self.search_runs = Vec::new();
        self.status = None;
        self.search_update();
    }

    /// 활성 탭을 `idx`로 전환합니다.
    fn switch_tab(&mut self, idx: usize) {
        if idx >= self.tabs.len() || idx == self.active {
            return;
        }
        self.save_session();
        self.capture_into(self.active);
        self.restore_from(idx);
        self.active = idx;
    }

    /// 탭을 닫습니다. 활성 탭이면 인접 탭으로 전환합니다.
    fn close_tab(&mut self, idx: usize) {
        if idx >= self.tabs.len() {
            return;
        }
        if idx == self.active {
            self.close_document();
            self.tabs.remove(idx);
            if self.tabs.is_empty() {
                return;
            }
            let new_active = idx.min(self.tabs.len() - 1);
            self.restore_from(new_active);
            self.active = new_active;
        } else {
            self.tabs.remove(idx);
            if idx < self.active {
                self.active -= 1;
            }
        }
    }

    /// 현재 활성 문서를 새 탭으로 추가합니다 (document는 self에 남아 활성 상태).
    fn add_current_as_tab(&mut self, kind: TabKind) {
        let label = self.file_name.clone();
        // 활성 탭의 실제 데이터는 self에 유지합니다 (document/store/…).
        // 탭 항목에는 전환 시 capture_into가 채워 넣으므로 빈 값으로 둡니다.
        let tab = TabEntry {
            kind,
            label,
            file_path: self.file_path.clone(),
            current_note: self.current_note,
            document: None,
            current_page: self.current_page,
            page_size_pts: self.page_size_pts,
            view: self.view,
            page_align: self.page_align,
            store: AnnotationStore::new(),
            history: History::new(256),
            search_query: String::new(),
            search_matches: Vec::new(),
            search_current: None,
            outline: Vec::new(),
            outline_loaded: false,
            show_library: self.show_library,
            show_outline: self.show_outline,
            show_search: self.show_search,
            library_width: self.library_width,
            outline_width: self.outline_width,
            tool: self.tool,
            color_family: self.color_family,
            pen_color: self.pen_color,
            pen_width: self.pen_width,
            hi_color: self.hi_color,
            hi_width: self.hi_width,
            eraser_radius: self.eraser_radius,
            pressure_enabled: self.pressure_enabled,
            pressure_curve: self.pressure_curve,
            paper_style: self.paper_style,
            paper_color: self.paper_color,
            paper_size: self.paper_size,
            paper_spacing: self.paper_spacing,
        };
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
    }

    // ---------- Recent files ----------

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
    fn toggle_bookmark(&mut self, page: PageIndex) {
        self.store.toggle_bookmark(page);
        self.persist_bookmarks();
    }

    /// 모든 북마크 제거.
    fn clear_bookmarks(&mut self) {
        self.store.clear_bookmarks();
        self.persist_bookmarks();
    }

    /// 북마크를 디스크에 반영합니다 (노트는 자동저장, 일반 PDF는 사이드카).
    fn persist_bookmarks(&mut self) {
        if self.current_note.is_some() {
            self.autosave();
        } else if let Some(path) = self.file_path.clone() {
            let ann_path = annotation_path_for(&path);
            let json = self.store.to_json();
            let _ = std::fs::write(&ann_path, json);
        }
        self.save_session();
    }

    // ---------- Notes ----------

    /// Shows an error both in the status bar and as a popup alert.
    fn show_error(&mut self, msg: String) {
        self.status = Some(msg.clone());
        self.modal = Some(ModalState::alert("Error", &msg));
    }

    /// Creates a note's blank PDF using the PDFium instance cached at startup.
    fn create_blank_pdf_for_note(&self, path: &Path) -> Result<(), String> {
        match &self.pdfium {
            Ok(p) => DocumentView::create_blank_pdf_with(p, path, self.paper_size.size_pts()),
            Err(e) => Err(e.clone()),
        }
    }

    /// Returns a reference to the PDFium instance cached at startup.
    /// pdfium-render only allows one initialization per process, so everything
    /// must reuse this single instance (never call `load_pdfium` again).
    fn pdfium(&self) -> Result<&Pdfium, String> {
        self.pdfium.as_ref().map(|b| b.as_ref()).map_err(|e| e.clone())
    }

    fn create_note_action(&mut self, title: &str) {
        match self.notes.create_note(title) {
            Ok(meta) => {
                let pdf_path = self.notes.pdf_path(meta.id);
                if let Err(e) = self.create_blank_pdf_for_note(&pdf_path) {
                    self.show_error(e);
                    return;
                }
                let _ = self.notes.set_page_count(meta.id, 1);
                self.logger.log(AppEvent::NoteCreated {
                    note_id: meta.id,
                    title: meta.title.clone(),
                });
                self.open_note(meta.id);
            }
            Err(e) => self.status = Some(format!("Could not create note: {e}")),
        }
    }

    fn rename_note_action(&mut self, id: u64, title: &str) {
        let old = self
            .notes
            .get(id)
            .map(|m| m.title.clone())
            .unwrap_or_default();
        match self.notes.rename_note(id, title) {
            Ok(()) => {
                self.logger.log(AppEvent::NoteRenamed {
                    note_id: id,
                    from: old,
                    to: title.to_string(),
                });
                if self.current_note == Some(id) {
                    self.file_name = title.to_string();
                }
                let _ = self.notes.save();
            }
            Err(e) => self.status = Some(format!("Rename failed: {e}")),
        }
    }

    fn delete_note_action(&mut self, id: u64) {
        let title = self
            .notes
            .get(id)
            .map(|m| m.title.clone())
            .unwrap_or_default();
        match self.notes.delete_note(id) {
            Ok(()) => {
                self.logger.log(AppEvent::NoteDeleted { note_id: id, title });
                // 열려 있는 탭이면 닫고, 아니면 문서 상태 정리.
                if let Some(idx) = self.find_tab(&TabKind::Note(id)) {
                    self.close_tab(idx);
                } else if self.current_note == Some(id) {
                    self.close_document();
                }
                // 최근 목록에서도 제거.
                self.recents
                    .items
                    .retain(|r| !(r.kind == RecentKind::Note && r.note_id == Some(id)));
                self.recents.save(&self.recent_path);
                let _ = self.notes.save();
            }
            Err(e) => self.status = Some(format!("Delete failed: {e}")),
        }
    }

    fn close_document(&mut self) {
        self.document = None;
        self.current_note = None;
        self.texture = None;
        self.store = AnnotationStore::new();
        self.history = History::new(256);
        self.active_stroke = None;
        self.search_matches = Vec::new();
        self.search_current = None;
        self.outline = Vec::new();
        self.outline_loaded = false;
        self.file_name = String::new();
        self.file_path = None;
        self.scroll_vel = Vec2::ZERO;
        self.page_anim = None;
        self.prev_texture = None;
        self.transition_last_page = 0;
        self.status = None;
    }

    fn open_note(&mut self, id: u64) {
        let Some(meta) = self.notes.get(id).cloned() else {
            return;
        };
        // 이미 열려 있으면 해당 탭으로 전환만 합니다.
        if let Some(idx) = self.find_tab(&TabKind::Note(id)) {
            self.switch_tab(idx);
            return;
        }
        // 현재 활성 문서 상태를 탭에 보존하고 새 문서를 엽니다.
        self.save_session();
        if self.document.is_some() {
            self.capture_into(self.active);
        }
        let pdf_path = self.notes.pdf_path(id);
        if !pdf_path.exists() {
            if let Err(e) = self.create_blank_pdf_for_note(&pdf_path) {
                self.show_error(e);
                return;
            }
        }
        let ann_path = self.notes.annotations_path(id);
        let store = if ann_path.exists() {
            std::fs::read_to_string(&ann_path)
                .ok()
                .and_then(|t| AnnotationStore::from_json(&t).ok())
                .unwrap_or_default()
        } else {
            AnnotationStore::new()
        };
        let opened = self.pdfium().and_then(|p| DocumentView::open(p, &pdf_path));
        match opened {
            Ok(doc) => {
                self.current_note = Some(id);
                self.current_page = 0;
                self.page_size_pts = doc.page_size_pts(0);
                self.file_name = meta.title.clone();
                self.file_path = Some(pdf_path);
                self.document = Some(doc);
                self.store = store;
                self.history = History::new(256);
                self.active_stroke = None;
                self.pan_last = None;
                self.middle_pan_last = None;
                self.render_dirty = true;
                self.pending_fit = Some(FitMode::Width);
                self.outline_loaded = false;
                self.outline = Vec::new();
                self.search_matches = Vec::new();
                self.search_current = None;
                self.scroll_vel = Vec2::ZERO;
                self.page_anim = None;
                self.prev_texture = None;
                self.transition_last_page = 0;
                self.status = None;
                let page_count = self.document.as_ref().map(|d| d.page_count()).unwrap_or(0);
                // 마지막 세션(페이지/도구/펜/줌 등)을 복원합니다.
                let session_path = self.notes.session_path(id);
                if session_path.exists() {
                    let session = crate::settings::SessionState::load(&session_path);
                    self.apply_session(&session, page_count);
                    self.pending_fit = None;
                }
                self.logger.log(AppEvent::NoteOpened {
                    note_id: id,
                    title: meta.title.clone(),
                    page_count,
                });
                self.load_outline_if_needed();
                self.add_current_as_tab(TabKind::Note(id));
                self.note_recent(RecentKind::Note, meta.title.clone(), Some(id), None);
            }
            Err(e) => self.show_error(e),
        }
    }

    // ---------- Standalone PDF (Open button) ----------

    fn open_file_dialog(&mut self) {
        #[cfg(target_os = "windows")]
        {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("PDF files", &["pdf"])
                .pick_file()
            {
                self.open_pdf(&path);
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            self.modal = Some(ModalState::ask_text(
                "Open PDF",
                "Enter the PDF file path (e.g. C:/Users/me/doc.pdf)",
                TextAction::OpenPdf,
            ));
        }
    }

    fn open_pdf(&mut self, path: &Path) {
        // 이미 열려 있으면 해당 탭으로 전환만 합니다.
        if let Some(idx) = self.find_tab(&TabKind::Pdf(path.to_path_buf())) {
            self.switch_tab(idx);
            return;
        }
        // 현재 활성 문서 상태를 탭에 보존하고 새 문서를 엽니다.
        self.save_session();
        if self.document.is_some() {
            self.capture_into(self.active);
        }
        let opened = self.pdfium().and_then(|p| DocumentView::open(p, path));
        match opened {
            Ok(doc) => {
                self.current_note = None;
                self.current_page = 0;
                self.page_size_pts = doc.page_size_pts(0);
                self.file_name = doc.file_name.clone();
                self.file_path = Some(path.to_path_buf());
                self.document = Some(doc);
                self.store = AnnotationStore::new();
                self.history = History::new(256);
                self.active_stroke = None;
                self.render_dirty = true;
                self.pending_fit = Some(FitMode::Width);
                self.outline_loaded = false;
                self.outline = Vec::new();
                self.search_matches = Vec::new();
                self.search_current = None;
                self.scroll_vel = Vec2::ZERO;
                self.page_anim = None;
                self.prev_texture = None;
                self.transition_last_page = 0;
                self.status = None;

                // Auto-load a sidecar annotation file if present
                let ann_path = annotation_path_for(path);
                if ann_path.exists() {
                    if let Ok(text) = std::fs::read_to_string(&ann_path) {
                        if let Ok(store) = AnnotationStore::from_json(&text) {
                            self.store = store;
                            self.status = Some(format!(
                                "Loaded sidecar annotations: {}",
                                ann_path.file_name().unwrap_or_default().to_string_lossy()
                            ));
                        }
                    }
                }
                // 마지막 세션(페이지/도구/펜/줌 등)을 복원합니다.
                let page_count = self.document.as_ref().map(|d| d.page_count()).unwrap_or(0);
                let session_path = session_path_for(path);
                if session_path.exists() {
                    let session = crate::settings::SessionState::load(&session_path);
                    self.apply_session(&session, page_count);
                    self.pending_fit = None;
                }
                self.load_outline_if_needed();
                self.add_current_as_tab(TabKind::Pdf(path.to_path_buf()));
                self.note_recent(
                    RecentKind::File,
                    self.file_name.clone(),
                    None,
                    Some(path.to_path_buf()),
                );
            }
            Err(e) => self.show_error(e),
        }
    }

    // ---------- Pages ----------

    fn next_page(&mut self) {
        if let Some(doc) = &self.document {
            if self.current_page + 1 < doc.page_count() {
                self.current_page += 1;
                self.on_page_changed();
            }
        }
    }

    fn prev_page(&mut self) {
        if self.current_page > 0 {
            self.current_page -= 1;
            self.on_page_changed();
        }
    }

    fn goto_page(&mut self, index: PageIndex) {
        if let Some(doc) = &self.document {
            if index < doc.page_count() {
                self.current_page = index;
                self.on_page_changed();
            }
        }
    }

    fn on_page_changed(&mut self) {
        let from = self.transition_last_page;
        self.active_stroke = None;
        self.pan_last = None;
        self.middle_pan_last = None;
        self.scroll_vel = Vec2::ZERO;
        if let Some(doc) = &self.document {
            self.page_size_pts = doc.page_size_pts(self.current_page);
        }
        self.render_dirty = true;
        // Keep the current zoom across page changes; just re-align the new page
        // (instead of resetting the zoom to fit-width).
        self.view
            .align_page(self.page_size_pts, self.last_canvas, TOP_MARGIN, self.page_align);
        self.search_update();
        if let Some(doc) = &self.document {
            self.logger.log(AppEvent::PageChanged {
                page: self.current_page,
                total: doc.page_count(),
            });
        }
        self.start_page_anim(from, self.current_page);
        self.transition_last_page = self.current_page;
        // 현재 페이지/줌 상태를 세션에 기록합니다.
        self.save_session();
    }

    /// Captures the outgoing page texture and starts a slide transition.
    fn start_page_anim(&mut self, from: usize, to: usize) {
        if from == to {
            return;
        }
        if self.texture.is_none() {
            return;
        }
        // The current texture still holds the old page; keep it for the outgoing
        // frame and force a fresh render for the new page.
        self.prev_texture = self.texture.take();
        self.render_dirty = true;
        self.page_anim = Some(PageAnim {
            progress: 0.0,
            direction: if to > from { 1.0 } else { -1.0 },
        });
    }

    /// 새 빈 페이지를 삽입합니다. `target`에 따라 위치/크기/용지가 달라집니다.
    fn insert_page_action(&mut self, target: InsertTarget) {
        let Some(doc) = &mut self.document else {
            return;
        };
        let total = doc.page_count();
        if total == 0 {
            return;
        }
        let default_paper = PagePaper {
            style: self.paper_style,
            color: self.paper_color,
            spacing: self.paper_spacing,
        };
        let default_size = self.paper_size.size_pts();
        let (idx, size, paper) = match target {
            // 현재 페이지의 크기/용지를 그대로 써서 바로 다음에 삽입.
            InsertTarget::FromCurrent => {
                let size = doc.page_size_pts(self.current_page);
                let paper = self
                    .store
                    .paper_on_or(self.current_page, default_paper);
                (self.current_page + 1, size, paper)
            }
            InsertTarget::FrontBegin => (0, default_size, default_paper),
            InsertTarget::FrontEnd => (total, default_size, default_paper),
            InsertTarget::BeforeCurrent => (self.current_page, default_size, default_paper),
            InsertTarget::AfterCurrent => (self.current_page + 1, default_size, default_paper),
        };
        if let Err(e) = doc.insert_page_at(idx, size) {
            self.status = Some(e);
            return;
        }
        self.store.insert_page(idx);
        self.store.set_paper(idx, paper);
        self.current_page = idx;
        let total = doc.page_count();
        self.logger.log(AppEvent::PageAdded { page: idx, total });
        self.on_page_changed();
        self.autosave();
        self.save_pdf_if_note();
        self.sync_note_meta();
    }

    /// 현재 페이지를 시계/반시계 90° 회전합니다 (주석도 함께 회전).
    fn rotate_page_action(&mut self, clockwise: bool) {
        let Some(doc) = &mut self.document else {
            return;
        };
        let idx = self.current_page;
        if idx >= doc.page_count() {
            return;
        }
        let [w, h] = doc.page_size_pts(idx); // 회전 전 표시 크기
        if let Err(e) = doc.rotate_page(idx, clockwise) {
            self.status = Some(e);
            return;
        }
        self.store.rotate_strokes_on(idx, w, h, clockwise);
        self.page_size_pts = doc.page_size_pts(idx);
        let total = doc.page_count();
        self.logger
            .log(AppEvent::PageRotated { page: idx, total, clockwise });
        self.on_page_changed();
        self.autosave();
        self.save_pdf_if_note();
        self.sync_note_meta();
    }

    /// 문서의 모든 페이지를 시계/반시계 90° 회전합니다 (주석도 함께).
    fn rotate_all_pages_action(&mut self, clockwise: bool) {
        let Some(doc) = &mut self.document else {
            return;
        };
        let count = doc.page_count();
        if count == 0 {
            return;
        }
        // 각 페이지의 회전 전 표시 크기 스냅샷.
        let sizes: Vec<[f32; 2]> = (0..count).map(|i| doc.page_size_pts(i)).collect();
        if let Err(e) = doc.rotate_all_pages(clockwise) {
            self.status = Some(e);
            return;
        }
        for i in 0..count {
            self.store.rotate_strokes_on(i, sizes[i][0], sizes[i][1], clockwise);
        }
        self.page_size_pts = doc.page_size_pts(self.current_page);
        self.logger.log(AppEvent::PageRotated {
            page: self.current_page,
            total: count,
            clockwise,
        });
        self.on_page_changed();
        self.autosave();
        self.save_pdf_if_note();
        self.sync_note_meta();
    }

    fn delete_page_action(&mut self) {
        let Some(doc) = &mut self.document else {
            return;
        };
        if doc.page_count() <= 1 {
            self.status = Some("Cannot delete the last remaining page.".to_string());
            return;
        }
        let idx = self.current_page;
        if let Err(e) = doc.delete_page(idx) {
            self.status = Some(e);
            return;
        }
        let total = doc.page_count();
        self.store.remove_page(idx);
        if self.current_page >= total {
            self.current_page = total.saturating_sub(1);
        }
        self.logger.log(AppEvent::PageDeleted {
            page: idx,
            total,
        });
        self.on_page_changed();
        self.autosave();
        self.save_pdf_if_note();
        self.sync_note_meta();
    }

    // ---------- Zoom / fit ----------

    fn zoom_by(&mut self, factor: f32) {
        let anchor = [self.last_canvas[0] * 0.5, self.last_canvas[1] * 0.5];
        self.view.zoom_at(anchor, factor, MIN_ZOOM, MAX_ZOOM);
        self.render_dirty = true;
        self.save_session();
    }

    fn fit_width(&mut self) {
        self.pending_fit = Some(FitMode::Width);
    }

    fn fit_height(&mut self) {
        self.pending_fit = Some(FitMode::Height);
    }

    /// Applies a pending fit once the canvas size is known.
    fn apply_pending_fit(&mut self, canvas: [f32; 2]) {
        let Some(mode) = self.pending_fit else {
            return;
        };
        self.pending_fit = None;
        if canvas[0] <= 1.0 || canvas[1] <= 1.0 {
            return;
        }
        match mode {
            FitMode::Width => {
                self.view.zoom =
                    ViewTransform::fit_width_zoom(self.page_size_pts[0], canvas[0], CANVAS_MARGIN);
            }
            FitMode::Height => {
                self.view.zoom =
                    ViewTransform::fit_height_zoom(self.page_size_pts[1], canvas[1], CANVAS_MARGIN);
            }
        }
        self.view
            .align_page(self.page_size_pts, canvas, TOP_MARGIN, self.page_align);
        self.render_dirty = true;
        self.save_session();
    }

    /// Re-applies the current horizontal alignment without changing the zoom.
    fn realign(&mut self) {
        if self.document.is_none() {
            return;
        }
        self.view
            .align_page(self.page_size_pts, self.last_canvas, TOP_MARGIN, self.page_align);
        self.save_session();
    }

    // ---------- Undo / redo / clear ----------

    fn undo(&mut self) {
        if let Some(edit) = self.history.undo() {
            self.store.apply_edit(&edit);
            self.logger.log(AppEvent::UndoRedo {
                kind: "undo".to_string(),
            });
            self.autosave();
        }
    }

    fn redo(&mut self) {
        if let Some(edit) = self.history.redo() {
            self.store.apply_edit(&edit);
            self.logger.log(AppEvent::UndoRedo {
                kind: "redo".to_string(),
            });
            self.autosave();
        }
    }

    fn clear_page(&mut self) {
        let removed = self.store.clear_page(self.current_page);
        if !removed.is_empty() {
            self.history.push(Edit::RemoveStrokes {
                page: self.current_page,
                strokes: removed.clone(),
            });
            self.logger.log(AppEvent::StrokeErased {
                page: self.current_page,
                strokes: removed.len(),
            });
            self.autosave();
        }
    }

    // ---------- Drawing ----------

    fn current_drawing_style(&self) -> ([u8; 4], f32) {
        match self.tool {
            ToolType::Pen => (self.pen_color, self.pen_width),
            ToolType::Highlighter => (self.hi_color, self.hi_width),
            _ => ([0, 0, 0, 255], 2.0),
        }
    }

    /// Pen pressure from touch events (Windows Ink). egui reports force via
    /// `Event::Touch { force: Some(f) }`; falls back to full pressure for mouse.
    fn sample_pressure(&self, ctx: &egui::Context) -> f32 {
        if !self.pressure_enabled {
            return 1.0;
        }
        let force: Option<f32> = ctx.input(|i| {
            i.events
                .iter()
                .rev()
                .find_map(|e| match e {
                    egui::Event::Touch { force: Some(f), .. } => Some(*f),
                    _ => None,
                })
        });
        force.map(|f| f.clamp(0.0, 1.0)).unwrap_or(1.0)
    }

    fn finish_stroke(&mut self) {
        if let Some(active) = self.active_stroke.take() {
            if active.points.is_empty() {
                return;
            }
            // 하이라이터 + 텍스트 인식 모드면 스와이프가 닿은 문서 텍스트 위로
            // 깔끔한 하이라이트를 만들어 저장하고, 원본 자유선은 버립니다.
            if active.tool == ToolType::Highlighter
                && self.text_highlight_snap
                && self.document.is_some()
                && self.add_text_highlights(&active)
            {
                return;
            }
            let id = self.store.add_stroke(
                self.current_page,
                active.tool,
                active.color,
                active.width,
                active.points,
            );
            if let Some(stroke) = self.store.stroke(self.current_page, id).cloned() {
                self.history.push(Edit::AddStrokes {
                    page: self.current_page,
                    strokes: vec![stroke.clone()],
                });
                self.logger.log(AppEvent::StrokeAdded {
                    page: self.current_page,
                    points: stroke.points.len(),
                    tool: tool_label(active.tool).to_string(),
                    width: active.width,
                });
            }
            self.autosave();
        }
    }

    /// 스트로크가 닿은 텍스트 줄 위로 하이라이트 사각형 스트로크를 추가합니다.
    /// 성공(텍스트 하이라이트를 만든 경우)하면 `true`를 반환합니다.
    fn add_text_highlights(&mut self, active: &ActiveStroke) -> bool {
        let Some(doc) = &self.document else {
            return false;
        };
        let (mut x0, mut y0, mut x1, mut y1) =
            (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in &active.points {
            x0 = x0.min(p.x);
            y0 = y0.min(p.y);
            x1 = x1.max(p.x);
            y1 = y1.max(p.y);
        }
        if x1 < x0 || y1 < y0 {
            return false;
        }
        let runs = if self.search_runs.is_empty() {
            doc.page_text_runs(self.current_page).unwrap_or_default()
        } else {
            self.search_runs.clone()
        };
        let rects = text_line_highlights(&runs, [x0, y0, x1, y1], 3.0);
        if rects.is_empty() {
            return false;
        }
        let mut strokes = Vec::new();
        for r in rects {
            let line_h = (r[3] - r[1]).max(2.0);
            let yc = (r[1] + r[3]) * 0.5;
            // 두께가 정확히 줄 높이가 되도록(비율 1.0) 필압 역산.
            let pressure = self.pressure_curve.pressure_of(line_h, line_h);
            let id = self.store.add_stroke(
                self.current_page,
                ToolType::Highlighter,
                active.color,
                line_h,
                vec![
                    StrokePoint::new(r[0], yc, pressure),
                    StrokePoint::new(r[2], yc, pressure),
                ],
            );
            if let Some(st) = self.store.stroke(self.current_page, id).cloned() {
                strokes.push(st);
            }
        }
        if strokes.is_empty() {
            return false;
        }
        self.history.push(Edit::AddStrokes {
            page: self.current_page,
            strokes: strokes.clone(),
        });
        self.logger.log(AppEvent::StrokeAdded {
            page: self.current_page,
            points: strokes.len() * 2,
            tool: "Highlighter".to_string(),
            width: active.width,
        });
        self.autosave();
        true
    }

    fn commit_dot(&mut self, point: [f32; 2], pressure: f32) {
        let (color, width) = self.current_drawing_style();
        self.active_stroke = Some(ActiveStroke {
            tool: self.tool,
            color,
            width,
            points: vec![StrokePoint::new(point[0], point[1], pressure)],
        });
        self.finish_stroke();
    }

    // ---------- Texture rendering ----------

    fn ensure_texture(&mut self, ctx: &egui::Context) {
        let Some(doc) = &self.document else {
            return;
        };
        let ppp = ctx.pixels_per_point();
        let target_w = self.page_size_pts[0] * self.view.zoom * ppp;
        let needs_render = self.render_dirty
            || self.texture.is_none()
            || (self.last_render_zoom - self.view.zoom).abs() / self.view.zoom.max(1e-3) > 0.15
            || (self.last_render_ppp - ppp).abs() > 0.01;

        if !needs_render {
            return;
        }

        match doc.render_page(self.current_page, target_w, 4096.0 * ppp) {
            Ok(rendered) => {
                let img = egui::ColorImage::from_rgba_unmultiplied(
                    [rendered.width, rendered.height],
                    &rendered.rgba,
                );
                if let Some(t) = self.texture.as_mut() {
                    t.set(img, egui::TextureOptions::LINEAR);
                } else {
                    self.texture =
                        Some(ctx.load_texture("page", img, egui::TextureOptions::LINEAR));
                }
                self.last_render_zoom = self.view.zoom;
                self.last_render_ppp = ppp;
                self.render_dirty = false;
            }
            Err(e) => self.status = Some(format!("Render error: {e}")),
        }
    }

    // ---------- Search ----------

    fn search_update(&mut self) {
        let Some(doc) = &self.document else {
            return;
        };
        self.search_runs = doc.page_text_runs(self.current_page).unwrap_or_default();
        self.search_matches = find_matches(&self.search_runs, &self.search_query);
        self.search_current = if self.search_matches.is_empty() {
            None
        } else {
            Some(0)
        };
        if !self.search_query.trim().is_empty() {
            self.logger.log(AppEvent::Search {
                query: self.search_query.trim().to_string(),
                results: self.search_matches.len(),
            });
        }
    }

    fn search_find(&mut self, forward: bool) {
        if self.search_matches.is_empty() {
            return;
        }
        let n = self.search_matches.len() as isize;
        let cur = self.search_current.unwrap_or(0) as isize;
        let next = if forward { cur + 1 } else { cur - 1 };
        let idx = ((next % n) + n) % n;
        self.search_current = Some(idx as usize);
    }

    fn search_clear(&mut self) {
        self.search_query.clear();
        self.search_matches = Vec::new();
        self.search_current = None;
    }

    // ---------- Outline ----------

    fn load_outline_if_needed(&mut self) {
        if self.outline_loaded {
            return;
        }
        if let Some(doc) = &self.document {
            self.outline = doc.outline();
            self.outline_loaded = true;
        }
    }

    // ---------- Persistence ----------

    /// Writes the current note's annotations to its note folder.
    fn autosave(&mut self) {
        let Some(id) = self.current_note else {
            return;
        };
        let ann_path = self.notes.annotations_path(id);
        let json = self.store.to_json();
        if let Err(e) = std::fs::write(&ann_path, json) {
            self.status = Some(format!("Autosave failed: {e}"));
            return;
        }
        let _ = self.notes.save();
    }

    fn sync_note_meta(&mut self) {
        if let Some(id) = self.current_note {
            if let Some(doc) = &self.document {
                let _ = self.notes.set_page_count(id, doc.page_count());
            }
        }
    }

    /// Persists page CRUD changes back into the note's PDF file.
    fn save_pdf_if_note(&mut self) {
        if self.current_note.is_none() {
            return;
        }
        let Some(doc) = &self.document else {
            return;
        };
        let Some(path) = self.file_path.clone() else {
            return;
        };
        if let Err(e) = doc.save(&path) {
            self.status = Some(format!("Save PDF failed: {e}"));
        }
    }

    // ---------- File dialogs (annotations / export) ----------

    fn save_annotations(&mut self) {
        if self.document.is_none() {
            self.status = Some("Open a PDF or note first.".to_string());
            return;
        }
        let default = self
            .file_path
            .as_deref()
            .map(annotation_path_for)
            .unwrap_or_else(|| PathBuf::from("annotations.freedf.json"));
        #[cfg(target_os = "windows")]
        {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("FreeDF annotations", &["json"])
                .set_file_name(
                    default
                        .file_name()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "annotations.freedf.json".into()),
                )
                .save_file()
            {
                self.do_save_annotations(&path);
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = default;
            self.modal = Some(ModalState::ask_text(
                "Save Annotations",
                "Enter the JSON file path to save to",
                TextAction::SaveAnnotations,
            ));
        }
    }

    fn do_save_annotations(&mut self, path: &Path) {
        match std::fs::write(path, self.store.to_json()) {
            Ok(()) => self.status = Some(format!("Annotations saved: {}", path.display())),
            Err(e) => self.status = Some(format!("Save failed: {e}")),
        }
    }

    fn load_annotations(&mut self) {
        #[cfg(target_os = "windows")]
        {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("FreeDF annotations", &["json"])
                .pick_file()
            {
                self.do_load_annotations(&path);
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            self.modal = Some(ModalState::ask_text(
                "Load Annotations",
                "Enter the JSON file path to load",
                TextAction::LoadAnnotations,
            ));
        }
    }

    fn do_load_annotations(&mut self, path: &Path) {
        match std::fs::read_to_string(path) {
            Ok(text) => match AnnotationStore::from_json(&text) {
                Ok(store) => {
                    self.store = store;
                    self.status = Some(format!("Annotations loaded: {}", path.display()));
                }
                Err(e) => self.status = Some(format!("Invalid annotation file: {e}")),
            },
            Err(e) => self.status = Some(format!("Load failed: {e}")),
        }
    }

    fn export_png(&mut self) {
        if self.document.is_none() {
            self.status = Some("Open a PDF or note first.".to_string());
            return;
        }
        #[cfg(target_os = "windows")]
        {
            let default_name = self
                .file_path
                .as_deref()
                .and_then(|p| p.file_stem())
                .map(|s| format!("{}-p{}.png", s.to_string_lossy(), self.current_page + 1))
                .unwrap_or_else(|| format!("page-{}.png", self.current_page + 1));
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("PNG image", &["png"])
                .set_file_name(&default_name)
                .save_file()
            {
                self.do_export_png(&path);
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            self.modal = Some(ModalState::ask_text(
                "Export PNG",
                "Enter the PNG file path to save to",
                TextAction::ExportPng,
            ));
        }
    }

    fn do_export_png(&mut self, path: &Path) {
        let Some(doc) = &self.document else {
            return;
        };
        let page_pts = doc.page_size_pts(self.current_page);
        let dpi = 150.0;
        let target_w = page_pts[0] * dpi / 72.0;
        match doc.render_page(self.current_page, target_w, 20_000.0) {
            Ok(rendered) => {
                let mut img = match image::RgbaImage::from_raw(
                    rendered.width as u32,
                    rendered.height as u32,
                    rendered.rgba,
                ) {
                    Some(img) => img,
                    None => {
                        self.status =
                            Some("Could not convert render result to image.".to_string());
                        return;
                    }
                };
                let scale = rendered.width as f32 / page_pts[0];
                // Paper tint + grid for notes (per-page settings)
                if self.current_note.is_some() {
                    let paper = self.current_page_paper();
                    crate::export::draw_paper(
                        &mut img,
                        page_pts[0],
                        page_pts[1],
                        scale,
                        paper.style,
                        paper.color,
                        paper.spacing,
                    );
                }
                let strokes: Vec<_> = self.store.strokes_on(self.current_page).to_vec();
                draw_strokes_on_image(&mut img, &strokes, scale);
                match img.save(path) {
                    Ok(()) => {
                        self.logger.log(AppEvent::ExportPng {
                            page: self.current_page,
                        });
                        self.status = Some(format!("Exported: {}", path.display()));
                    }
                    Err(e) => self.status = Some(format!("PNG save failed: {e}")),
                }
            }
            Err(e) => self.status = Some(format!("Export failed: {e}")),
        }
    }

    fn run_text_action(&mut self, action: TextAction, text: String) {
        match action {
            TextAction::NewNote => self.create_note_action(text.trim()),
            TextAction::RenameNote => {
                if let Some(id) = self.current_note {
                    self.rename_note_action(id, text.trim());
                }
            }
            TextAction::OpenPdf => self.open_pdf(&PathBuf::from(text.trim())),
            TextAction::SaveAnnotations => self.do_save_annotations(&PathBuf::from(text.trim())),
            TextAction::LoadAnnotations => self.do_load_annotations(&PathBuf::from(text.trim())),
            TextAction::ExportPng => self.do_export_png(&PathBuf::from(text.trim())),
        }
    }

    fn run_confirm_action(&mut self, action: ConfirmAction, text: String) {
        match action {
            ConfirmAction::DeleteNote => {
                if let Ok(id) = text.trim().parse::<u64>() {
                    self.delete_note_action(id);
                }
            }
        }
    }

    // ---------- UI: tabs bar ----------

    fn tabs_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("tabs_bar").show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
            ui.add_space(3.0);
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button(icon_text(ui, "New", icons::PLUS))
                    .on_hover_text("New note (Ctrl+N)")
                    .clicked()
                {
                    self.modal = Some(ModalState::ask_text(
                        "New Note",
                        "Note title:",
                        TextAction::NewNote,
                    ));
                }
                if ui
                    .button(icon_text(ui, "Open", icons::FOLDER_OPEN))
                    .on_hover_text("Open PDF (Ctrl+O)")
                    .clicked()
                {
                    self.open_file_dialog();
                }
                ui.separator();

                if self.tabs.is_empty() {
                    ui.label(egui::RichText::new("No documents open").weak());
                    return;
                }
                let mut to_switch: Option<usize> = None;
                let mut to_close: Option<usize> = None;
                let active_fill = crate::theme::nord::semantic::BG_SURFACE;
                let accent = crate::theme::nord::semantic::ACCENT_ACTIVE;
                // Scrollable tab strip: many tabs or long titles scroll
                // instead of wrapping to a new line ("folding").
                egui::ScrollArea::horizontal()
                    .id_salt("tabs_scroll")
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for (i, tab) in self.tabs.iter().enumerate() {
                                let selected = i == self.active;
                                // The active tab's title lives in `self.file_name`
                                // (its `tab.label` is emptied by restore_from);
                                // inactive tabs keep their own label.
                                let title: &str = if selected {
                                    &self.file_name
                                } else {
                                    &tab.label
                                };
                                egui::Frame::new()
                                    .fill(if selected {
                                        active_fill
                                    } else {
                                        egui::Color32::TRANSPARENT
                                    })
                                    .stroke(if selected {
                                        Stroke::new(1.0, accent)
                                    } else {
                                        Stroke::NONE
                                    })
                                    .corner_radius(6)
                                    .inner_margin(egui::Margin::symmetric(8, 3))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
                                            // Fixed-width truncated title; full
                                            // name is shown on hover.
                                            let title_resp = ui.add_sized(
                                                egui::vec2(180.0, 22.0),
                                                egui::Label::new(egui::RichText::new(title))
                                                    .truncate()
                                                    .sense(egui::Sense::click()),
                                            );
                                            if title_resp.on_hover_text(title).clicked() {
                                                to_switch = Some(i);
                                            }
                                            let close = ui.add(
                                                egui::Button::new(icon_text(ui, "", icons::X))
                                                    .frame(false)
                                                    .small(),
                                            );
                                            if close.on_hover_text("Close document").clicked() {
                                                to_close = Some(i);
                                            }
                                        });
                                    });
                            }
                        });
                    });
                if let Some(i) = to_close {
                    self.close_tab(i);
                }
                if let Some(i) = to_switch {
                    self.switch_tab(i);
                }
            });
        });
    }

    // ---------- UI: toolbar ----------

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("toolbar").show(ui, |ui| {
            // Compact spacing + padding; uniform control height for tidy rows
            ui.spacing_mut().button_padding = egui::vec2(9.0, 5.0);
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);
            ui.spacing_mut().interact_size = egui::vec2(0.0, 28.0);
            ui.add_space(4.0);
            // Row 1: panels / page tools / ink tools
            toolbar_row(ui, |ui| {
                ui.horizontal(|ui| {
                if ui
                    .toggle_value(&mut self.show_library, icon_text(ui, "Library", icons::NOTEBOOK))
                    .on_hover_text("Library (notes, PDFs, recents)")
                    .changed()
                {
                    // Zoom is preserved; the canvas re-centers on resize.
                    self.save_session();
                }
                if ui
                    .toggle_value(&mut self.show_outline, icon_text(ui, "Outline", icons::LIST_BULLETS))
                    .on_hover_text("Outline")
                    .changed()
                {
                    // Zoom is preserved; the canvas re-centers on resize.
                    self.save_session();
                }
                if ui
                    .toggle_value(&mut self.show_palette, icon_text(ui, "Palette", icons::PALETTE))
                    .on_hover_text("Writing-tool color palette (right side of canvas)")
                    .changed()
                {
                    self.save_default_session();
                }
                ui.separator();

                // Bookmark the current page + jump list.
                let bookmarked = self.store.is_bookmarked(self.current_page);
                if ui
                    .selectable_label(
                        bookmarked,
                        icon_text(ui, "Bookmark", icons::BOOKMARK_SIMPLE),
                    )
                    .on_hover_text(if bookmarked {
                        "Remove bookmark from this page"
                    } else {
                        "Bookmark this page"
                    })
                    .clicked()
                {
                    self.toggle_bookmark(self.current_page);
                }
                ui.menu_button(icon_text(ui, "Bookmarks", icons::BOOKMARKS_SIMPLE), |ui| {
                    let pages: Vec<PageIndex> = self.store.bookmarks().to_vec();
                    if pages.is_empty() {
                        ui.label("No bookmarks yet");
                        return;
                    }
                    for p in pages {
                        if ui.button(format!("Page {}", p + 1)).clicked() {
                            ui.close();
                            self.goto_page(p);
                        }
                    }
                    ui.separator();
                    if ui.button("Clear all bookmarks").clicked() {
                        self.clear_bookmarks();
                    }
                });
                ui.separator();

                if !self.show_library && !self.show_outline {
                    // With the side panels collapsed the canvas is wide, so let
                    // the page be aligned left / center / right.
                    ui.separator();
                    let aligns = [
                        (PageAlign::Left, icons::TEXT_ALIGN_LEFT, "Align left"),
                        (PageAlign::Center, icons::TEXT_ALIGN_CENTER, "Align center"),
                        (PageAlign::Right, icons::TEXT_ALIGN_RIGHT, "Align right"),
                    ];
                    for (a, ic, hint) in aligns {
                        if ui
                            .selectable_label(self.page_align == a, icon_text(ui, "", ic))
                            .on_hover_text(hint)
                            .clicked()
                        {
                            self.page_align = a;
                            self.realign();
                            self.save_session();
                        }
                    }
                }
                ui.separator();

                if ui
                    .add_enabled(
                        self.history.can_undo(),
                        egui::Button::new(icon_text(ui, "Undo", icons::ARROW_COUNTER_CLOCKWISE)),
                    )
                    .on_hover_text("Undo (Ctrl+Z)")
                    .clicked()
                {
                    self.undo();
                }
                if ui
                    .add_enabled(
                        self.history.can_redo(),
                        egui::Button::new(icon_text(ui, "Redo", icons::ARROW_CLOCKWISE)),
                    )
                    .on_hover_text("Redo (Ctrl+Y)")
                    .clicked()
                {
                    self.redo();
                }
                if ui
                    .button(icon_text(ui, "Clear", icons::X_CIRCLE))
                    .on_hover_text("Clear page")
                    .clicked()
                {
                    self.clear_page();
                }
                ui.separator();

                if ui
                    .button(icon_text(ui, "Save", icons::FLOPPY_DISK))
                    .on_hover_text("Save annotations (Ctrl+S)")
                    .clicked()
                {
                    self.save_annotations();
                }
                if ui
                    .button(icon_text(ui, "Load", icons::FOLDER_SIMPLE))
                    .on_hover_text("Load annotations")
                    .clicked()
                {
                    self.load_annotations();
                }
                if ui
                    .button(icon_text(ui, "Export", icons::IMAGE))
                    .on_hover_text("Export current page as PNG (Ctrl+E)")
                    .clicked()
                {
                    self.export_png();
                }
                });
            });

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(2.0);

            // Row 2: Page (structure + paper styling)
            toolbar_row(ui, |ui| {
                ui.horizontal(|ui| {
                ui.label(icon_text(ui, "Page", icons::FILES));
                let page_count = self.document.as_ref().map(|d| d.page_count()).unwrap_or(0);
                ui.menu_button(icon_text(ui, "Insert Page", icons::PLUS_SQUARE), |ui| {
                    let insert = [
                        (InsertTarget::FromCurrent, "From current page"),
                        (InsertTarget::FrontBegin, "Front begin"),
                        (InsertTarget::FrontEnd, "Front end"),
                        (InsertTarget::BeforeCurrent, "Before current page"),
                        (InsertTarget::AfterCurrent, "After current page"),
                    ];
                    for (target, label) in insert {
                        if ui
                            .add_enabled(page_count > 0, egui::Button::new(label))
                            .clicked()
                        {
                            ui.close();
                            self.insert_page_action(target);
                        }
                    }
                })
                .response
                .on_hover_text("Insert a blank page");
                ui.menu_button(icon_text(ui, "Rotate", icons::REPEAT), |ui| {
                    if ui
                        .add_enabled(page_count > 0, egui::Button::new("Rotate current page CW"))
                        .clicked()
                    {
                        ui.close();
                        self.rotate_page_action(true);
                    }
                    if ui
                        .add_enabled(
                            page_count > 0,
                            egui::Button::new("Rotate current page CCW"),
                        )
                        .clicked()
                    {
                        ui.close();
                        self.rotate_page_action(false);
                    }
                    if ui
                        .add_enabled(page_count > 0, egui::Button::new("Rotate all pages CW"))
                        .clicked()
                    {
                        ui.close();
                        self.rotate_all_pages_action(true);
                    }
                    if ui
                        .add_enabled(page_count > 0, egui::Button::new("Rotate all pages CCW"))
                        .clicked()
                    {
                        ui.close();
                        self.rotate_all_pages_action(false);
                    }
                })
                .response
                .on_hover_text("Rotate pages (CW = clockwise)");
                if ui
                    .add_enabled(
                        page_count > 1,
                        egui::Button::new(icon_text(ui, "Delete", icons::TRASH_SIMPLE)),
                    )
                    .on_hover_text("Delete this page")
                    .clicked()
                {
                    self.delete_page_action();
                }
                ui.separator();

                // Paper (grid / ruling / color) — applied per page;
                // paper size selects the size for new pages & notes.
                ui.label(icon_text(ui, "Paper", icons::NOTEBOOK));
                egui::ComboBox::from_id_salt("paper_style")
                    .selected_text(self.paper_style.label())
                    .show_ui(ui, |ui| {
                        for style in PaperStyle::all() {
                            let changed = ui
                                .selectable_value(&mut self.paper_style, style, style.label())
                                .changed();
                            if changed {
                                self.apply_paper_to_current_page();
                                self.save_default_session();
                                self.save_session();
                            }
                        }
                    })
                    .response
                    .on_hover_text("Style applied to the current page");
                for (i, paper) in PAPER_COLORS.iter().enumerate() {
                    let color =
                        Color32::from_rgba_unmultiplied(paper[0], paper[1], paper[2], paper[3]);
                    let selected = self.paper_color == *paper;
                    if color_circle_swatch(ui, ("paper_swatch", i), color, selected)
                        .on_hover_text("Paper color")
                        .clicked()
                    {
                        self.paper_color = *paper;
                        self.apply_paper_to_current_page();
                        self.save_default_session();
                        self.save_session();
                    }
                }
                egui::ComboBox::from_id_salt("paper_size")
                    .selected_text(self.paper_size.label())
                    .show_ui(ui, |ui| {
                        for size in PaperSize::all() {
                            let changed = ui
                                .selectable_value(&mut self.paper_size, size, size.label())
                                .changed();
                            if changed {
                                self.save_default_session();
                                self.save_session();
                            }
                        }
                    })
                    .response
                    .on_hover_text("Size of new pages & new notes");
                // 줄/격자 간격 (숫자 직접 입력).
                if ui
                    .add(
                        egui::DragValue::new(&mut self.paper_spacing)
                            .range(12.0..=120.0)
                            .speed(1.0)
                            .prefix("Spacing ")
                            .suffix("pt"),
                    )
                    .on_hover_text("Ruled/Grid/Dotted spacing applied to the current page")
                    .changed()
                {
                    self.paper_spacing = clamp_spacing(self.paper_spacing);
                    self.apply_paper_to_current_page();
                    self.save_default_session();
                    self.save_session();
                }
                });
            });

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(2.0);

            // Row 3: drawing tools (icon-only picker + settings)
            toolbar_row(ui, |ui| {
                ui.horizontal(|ui| {
                let tool_buttons = [
                    (ToolType::Pen, icons::PEN, "Pen"),
                    (ToolType::Highlighter, icons::MARKER_CIRCLE, "Highlight"),
                    (ToolType::Eraser, icons::ERASER, "Eraser"),
                    (ToolType::Pan, icons::HAND, "Pan"),
                ];
                for (tool, ic, label) in tool_buttons {
                    if ui
                        .selectable_label(self.tool == tool, icon_text(ui, "", ic))
                        .on_hover_text(format!("{label} (P / H / E / V)"))
                        .clicked()
                    {
                        self.tool = tool;
                        self.save_session();
                    }
                }
                ui.separator();

                match self.tool {
                    ToolType::Pen => {
                        egui::ComboBox::from_id_salt("family")
                            .selected_text(self.color_family.label())
                            .show_ui(ui, |ui| {
                                for family in ColorFamily::all() {
                                    if ui
                                        .selectable_value(
                                            &mut self.color_family,
                                            family,
                                            family.label(),
                                        )
                                        .changed()
                                    {
                                        self.save_session();
                                    }
                                }
                            });
                        let swatches = Palette::swatches(self.color_family);
                        // Round color swatches forming a neat color bar.
                        for (i, swatch) in swatches.iter().enumerate() {
                            let color = Color32::from_rgba_unmultiplied(
                                swatch[0],
                                swatch[1],
                                swatch[2],
                                swatch[3],
                            );
                            let selected = *swatch == self.pen_color;
                            if color_circle_swatch(ui, ("pen_swatch", i), color, selected)
                                .on_hover_text("Pen color")
                                .clicked()
                            {
                                self.pen_color = *swatch;
                                self.save_default_session();
                                self.save_session();
                            }
                        }
                        if ui
                            .add(egui::Slider::new(&mut self.pen_width, 0.5..=12.0).text("Width"))
                            .changed()
                        {
                            self.save_session();
                        }
                        if ui.checkbox(&mut self.pressure_enabled, "Pressure").changed() {
                            self.save_session();
                        }
                        if self.pressure_enabled {
                            if ui
                                .add(
                                    egui::Slider::new(
                                        &mut self.pressure_curve.min_ratio,
                                        0.1..=1.0,
                                    )
                                    .text("Min"),
                                )
                                .changed()
                            {
                                self.save_session();
                            }
                            if ui
                                .add(
                                    egui::Slider::new(
                                        &mut self.pressure_curve.max_ratio,
                                        1.0..=3.0,
                                    )
                                    .text("Max"),
                                )
                                .changed()
                            {
                                self.save_session();
                            }
                        }
                    }
                    ToolType::Highlighter => {
                        let mut color = Color32::from_rgba_unmultiplied(
                            self.hi_color[0],
                            self.hi_color[1],
                            self.hi_color[2],
                            self.hi_color[3],
                        );
                        if ui.color_edit_button_srgba(&mut color).changed() {
                            self.hi_color = color.to_array();
                            self.save_session();
                        }
                        if ui
                            .add(egui::Slider::new(&mut self.hi_width, 4.0..=40.0).text("Width"))
                            .changed()
                        {
                            self.save_session();
                        }
                        if ui
                            .checkbox(&mut self.text_highlight_snap, "Snap to text")
                            .on_hover_text(
                                "Highlight the recognized document text your stroke touches\n\
                                 (off = freehand translucent stroke)",
                            )
                            .changed()
                        {
                            self.save_default_session();
                            self.save_session();
                        }
                    }
                    ToolType::Eraser => {
                        if ui
                            .add(egui::Slider::new(&mut self.eraser_radius, 4.0..=60.0).text("Radius"))
                            .changed()
                        {
                            self.save_session();
                        }
                    }
                    ToolType::Pan => {}
                }
                });
            });

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(2.0);

            // Row 4: search (only while Ctrl+F is pressed)
            if self.show_search {
                toolbar_row(ui, |ui| {
                    ui.horizontal(|ui| {
                    ui.label(icon_text(ui, "Find", icons::MAGNIFYING_GLASS));
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.search_query)
                            .hint_text("Search in this page...")
                            .desired_width(200.0),
                    );
                    let submitted =
                        resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if self.focus_search {
                        resp.request_focus();
                        self.focus_search = false;
                    }
                    if ui.button("Find").clicked() || submitted {
                        self.search_update();
                    }
                    let can = !self.search_matches.is_empty();
                    if ui
                        .add_enabled(can, egui::Button::new(icon_text(ui, "", icons::CARET_UP)))
                        .on_hover_text("Previous match")
                        .clicked()
                    {
                        self.search_find(false);
                    }
                    if ui
                        .add_enabled(can, egui::Button::new(icon_text(ui, "", icons::CARET_DOWN)))
                        .on_hover_text("Next match")
                        .clicked()
                    {
                        self.search_find(true);
                    }
                    if !self.search_matches.is_empty() {
                        let cur = self.search_current.map(|c| c + 1).unwrap_or(0);
                        ui.label(format!("{cur}/{}", self.search_matches.len()));
                    }
                    if ui
                        .add(egui::Button::new(icon_text(ui, "", icons::X)).frame(false))
                        .on_hover_text("Close search (Ctrl+F)")
                        .clicked()
                    {
                        self.show_search = false;
                        self.search_clear();
                    }
                });
                });
                ui.add_space(4.0);
            }
        });
    }

    // ---------- UI: library panel (Notes / PDFs / Recents) ----------

    fn library_panel(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 5.0);
        ui.add_space(6.0);

        // ── 헤더 + 검색 필터 ──────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Library").strong().size(16.0));
            let total = self.notes.list().len() + self.recents.sorted().len();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!("{total} items"))
                        .weak()
                        .small(),
                );
            });
        });
        ui.add_space(3.0);
        ui.add(
            egui::TextEdit::singleline(&mut self.library_filter)
                .hint_text("Search notes & files…")
                .desired_width(f32::INFINITY),
        );
        ui.add_space(2.0);
        ui.separator();

        let filter = self.library_filter.trim().to_lowercase();
        let matches = |t: &str| filter.is_empty() || t.to_lowercase().contains(&filter);
        let has_note = self.current_note.is_some();
        let mut rename_note = false;
        let mut delete_note = false;

        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                // ── Notes ──────────────────────────────────────────────
                let all_notes: Vec<(u64, String, usize)> = self
                    .notes
                    .list()
                    .iter()
                    .map(|m| (m.id, m.title.clone(), m.page_count))
                    .collect();
                let notes: Vec<(u64, String, usize)> = all_notes
                    .iter()
                    .filter(|(_, t, _)| matches(t))
                    .cloned()
                    .collect();
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(6.0, 0.0);
                    ui.label(icon_text(ui, "Notes", icons::NOTE_PENCIL));
                    ui.label(
                        egui::RichText::new(all_notes.len().to_string())
                            .weak()
                            .small(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(2.0, 0.0);
                        if ui
                            .add_enabled(
                                has_note,
                                egui::Button::new(icon_text(ui, "", icons::PENCIL_SIMPLE))
                                    .frame(false)
                                    .small(),
                            )
                            .on_hover_text("Rename current note")
                            .clicked()
                        {
                            rename_note = true;
                        }
                        if ui
                            .add_enabled(
                                has_note,
                                egui::Button::new(icon_text(ui, "", icons::TRASH_SIMPLE))
                                    .frame(false)
                                    .small(),
                            )
                            .on_hover_text("Delete current note")
                            .clicked()
                        {
                            delete_note = true;
                        }
                    });
                });
                ui.add_space(2.0);
                if notes.is_empty() {
                    ui.label(
                        egui::RichText::new("No notes yet — use ＋ New to create one.")
                            .weak()
                            .small(),
                    );
                } else {
                    for (id, title, page_count) in &notes {
                        let meta = if *page_count > 0 {
                            format!("{page_count}p")
                        } else {
                            String::new()
                        };
                        let selected = self.current_note == Some(*id);
                        if library_row(ui, selected, title, &meta) {
                            self.open_note(*id);
                        }
                    }
                }
                ui.add_space(4.0);
                ui.separator();

                // ── PDFs (recently opened files) ──────────────────────
                let files: Vec<RecentItem> = self
                    .recents
                    .sorted()
                    .into_iter()
                    .filter(|r| r.kind == RecentKind::File)
                    .cloned()
                    .collect();
                let visible: Vec<RecentItem> = files
                    .iter()
                    .filter(|f| matches(&f.title))
                    .cloned()
                    .collect();
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.label(icon_text(ui, "PDFs", icons::FILE_PDF));
                    ui.label(
                        egui::RichText::new(files.len().to_string())
                            .weak()
                            .small(),
                    );
                });
                ui.add_space(2.0);
                if visible.is_empty() {
                    ui.label(egui::RichText::new("No PDFs opened yet.").weak().small());
                } else {
                    for f in &visible {
                        if library_row(ui, false, &f.title, "PDF") {
                            if let Some(p) = &f.path {
                                self.open_pdf(p);
                            }
                        }
                    }
                }
                ui.add_space(4.0);
                ui.separator();

                // ── Recents (notes + PDFs) ────────────────────────────
                let recents: Vec<RecentItem> = self
                    .recents
                    .sorted()
                    .into_iter()
                    .filter(|r| matches(&r.title))
                    .cloned()
                    .collect();
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.label(icon_text(ui, "Recents", icons::CLOCK_COUNTER_CLOCKWISE));
                    ui.label(
                        egui::RichText::new(self.recents.sorted().len().to_string())
                            .weak()
                            .small(),
                    );
                });
                ui.add_space(2.0);
                if recents.is_empty() {
                    ui.label(egui::RichText::new("No recent files yet.").weak().small());
                } else {
                    for item in &recents {
                        let meta = match item.kind {
                            RecentKind::Note => "note".to_string(),
                            RecentKind::File => "pdf".to_string(),
                        };
                        if library_row(ui, false, &item.title, &meta) {
                            match item.kind {
                                RecentKind::Note => {
                                    if let Some(id) = item.note_id {
                                        self.open_note(id);
                                    }
                                }
                                RecentKind::File => {
                                    if let Some(p) = &item.path {
                                        self.open_pdf(p);
                                    }
                                }
                            }
                        }
                    }
                }
                ui.add_space(4.0);
            });

        if rename_note {
            if let Some(id) = self.current_note {
                let current = self
                    .notes
                    .get(id)
                    .map(|m| m.title.clone())
                    .unwrap_or_default();
                let mut modal =
                    ModalState::ask_text("Rename Note", "New title:", TextAction::RenameNote);
                modal.text = current;
                self.modal = Some(modal);
            }
        }
        if delete_note {
            if let Some(id) = self.current_note {
                let mut modal = ModalState::confirm(
                    "Delete Note",
                    "Delete this note and all its annotations? This cannot be undone.",
                    ConfirmAction::DeleteNote,
                );
                modal.text = id.to_string();
                self.modal = Some(modal);
            }
        }
    }

    // ---------- UI: outline panel ----------

    fn outline_panel(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);
        ui.add_space(4.0);
        ui.heading("Outline");
        ui.add_space(2.0);
        if !self.outline_loaded {
            self.load_outline_if_needed();
        }
        if self.outline.is_empty() {
            ui.label("No outline in this PDF.");
            return;
        }
        let mut jump: Option<(String, usize)> = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                let mut index = 0usize;
                for entry in flatten(&self.outline) {
                    index += 1;
                    let text = format!(
                        "{}{}",
                        "    ".repeat(entry.depth),
                        entry.node.title
                    );
                    let resp = ui.push_id(index, |ui| ui.selectable_label(false, text)).inner;
                    if resp.clicked() {
                        if let Some(p) = entry.node.page_index {
                            jump = Some((entry.node.title.clone(), p));
                        }
                    }
                }
            });
        if let Some((title, page)) = jump {
            self.logger.log(AppEvent::OutlineJump { title, page });
            self.goto_page(page);
        }
    }

    // ---------- UI: status bar ----------

    /// Shows only transient status messages (errors, export results, ...) and
    /// auto-clears them after a few seconds. Document title / page / zoom are
    /// intentionally not shown here.
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

    fn canvas(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let canvas = response.rect;
        let origin = canvas.min;
        let canvas_size = [canvas.width(), canvas.height()];
        // Preserve the zoom when the canvas resizes (panel toggles / window
        // resize): re-center the page at the current zoom instead of re-fitting.
        let resized = (self.prev_canvas[0] - canvas_size[0]).abs() > 2.0
            || (self.prev_canvas[1] - canvas_size[1]).abs() > 2.0;
        if self.document.is_some() && self.pending_fit.is_none() && resized {
            self.view
                .align_page(self.page_size_pts, canvas_size, TOP_MARGIN, self.page_align);
            self.render_dirty = true;
        }
        self.prev_canvas = canvas_size;
        self.last_canvas = canvas_size;

        // Background behind the page (Nord canvas surround — dark mode)
        let bg = crate::theme::nord::semantic::PAGE_SURROUND;
        painter.rect_filled(canvas, egui::CornerRadius::ZERO, bg);

        if self.document.is_none() {
            ui.painter_at(canvas).text(
                canvas.center(),
                egui::Align2::CENTER_CENTER,
                "Open a PDF or create a note to start annotating (Ctrl+O)",
                egui::TextStyle::Heading.resolve(ui.style()),
                ui.visuals().weak_text_color(),
            );
            return;
        }

        // Apply pending fit + render cache
        self.apply_pending_fit(canvas_size);
        self.ensure_texture(&ctx);

        // ---------- Input ----------
        self.handle_canvas_input(&ctx, &response, origin, canvas_size);
        // Keep the page within the canvas (no infinite panning)
        self.view.clamp_pan(self.page_size_pts, canvas_size, CANVAS_MARGIN);

        // Advance the page transition animation
        let mut animating = false;
        if let Some(anim) = &mut self.page_anim {
            let dt = ctx.input(|i| i.stable_dt).max(1e-4);
            anim.progress += dt / PAGE_ANIM_SECS;
            animating = anim.progress < 1.0;
            if !animating {
                self.page_anim = None;
                self.prev_texture = None;
            }
        }
        if animating {
            ctx.request_repaint();
        }

        // ---------- Draw ----------
        let page_view = self.view.page_size_to_view(self.page_size_pts[0], self.page_size_pts[1]);
        let page_rect = Rect::from_min_size(
            origin + Vec2::new(self.view.pan_x, self.view.pan_y),
            Vec2::new(page_view[0], page_view[1]),
        );

        // Paper color tint applied to the page image (colored paper).
        let paper = self.current_page_paper();
        let paper_tint = Color32::from_rgba_unmultiplied(
            paper.color[0],
            paper.color[1],
            paper.color[2],
            255,
        );

        // During a transition, draw the outgoing + incoming pages sliding.
        let mut anim_dx = 0.0_f32;
        if let (Some(anim), Some(prev)) = (&self.page_anim, &self.prev_texture) {
            let w = page_rect.width();
            let dir = anim.direction;
            let p = anim.progress;
            let old_off = -p * dir * w;
            let new_off = (1.0 - p) * dir * w;
            anim_dx = new_off;

            let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
            // Outgoing page (old texture)
            painter.rect_filled(
                page_rect.translate(Vec2::new(old_off, 0.0)).expand(6.0),
                egui::CornerRadius::same(4),
                Color32::from_black_alpha(70),
            );
            painter.image(
                prev.id(),
                page_rect.translate(Vec2::new(old_off, 0.0)),
                uv,
                paper_tint,
            );
            // Incoming page (new texture)
            painter.rect_filled(
                page_rect.translate(Vec2::new(new_off, 0.0)).expand(6.0),
                egui::CornerRadius::same(4),
                Color32::from_black_alpha(70),
            );
            if let Some(tex) = &self.texture {
                painter.image(
                    tex.id(),
                    page_rect.translate(Vec2::new(new_off, 0.0)),
                    uv,
                    paper_tint,
                );
            }
        }

        // Current-page rect/origin (shifted during a transition so border & ink follow)
        let draw_rect = page_rect.translate(Vec2::new(anim_dx, 0.0));
        let draw_origin = origin + Vec2::new(anim_dx, 0.0);

        // Page shadow, image and border (single page when not mid-transition)
        if self.page_anim.is_none() {
            painter.rect_filled(
                draw_rect.expand(6.0),
                egui::CornerRadius::same(4),
                Color32::from_black_alpha(70),
            );
            if let Some(tex) = &self.texture {
                painter.image(
                    tex.id(),
                    draw_rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    paper_tint,
                );
            }
            painter.rect_stroke(
                draw_rect,
                egui::CornerRadius::same(2),
                Stroke::new(1.0, Color32::from_gray(120)),
                egui::StrokeKind::Inside,
            );
            // Paper grid / ruling (only for notes)
            if self.current_note.is_some() {
                self.paint_paper(&painter, draw_origin);
            }
        }

        // Search highlights (under ink so annotations stay readable)
        self.paint_search_highlights(&painter, draw_origin);

        // Annotation strokes
        let strokes: Vec<_> = self.store.strokes_on(self.current_page).to_vec();
        for stroke in &strokes {
            self.paint_stroke(&painter, stroke, draw_origin);
        }
        if let Some(active) = &self.active_stroke {
            self.paint_active(&painter, active, draw_origin);
        }

        // Tool cursor — custom sprite over the page, OS cursor restored
        // everywhere else (so it never disappears outside the canvas).
        if let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) {
            if draw_rect.contains(pos) {
                ctx.set_cursor_icon(egui::CursorIcon::None);
                let time = ctx.input(|i| i.time) as f32;
                self.paint_custom_cursor(&painter, pos, time);
            } else {
                ctx.set_cursor_icon(egui::CursorIcon::Default);
            }
        } else {
            ctx.set_cursor_icon(egui::CursorIcon::Default);
        }

        // Zoom hint
        if self.document.is_some() && self.view.zoom >= 4.0 {
            painter.text(
                canvas.left_top() + Vec2::new(10.0, 10.0),
                egui::Align2::LEFT_TOP,
                "Ctrl+wheel: zoom / wheel: scroll & page / middle button: pan",
                egui::TextStyle::Small.resolve(ui.style()),
                ui.visuals().text_color(),
            );
        }

        // Floating page navigation overlay (bottom-center, semi-transparent).
        self.canvas_nav_overlay(&ctx, canvas);
        // Floating writing-tool / color palette (right-center of the canvas).
        self.canvas_palette_overlay(&ctx, canvas);
    }

    /// 페이지 내비게이션 오버레이: Prev/Next, 줌, Fit Width/Height를
    /// 캔버스 중앙 하단에 반투명하게 고정 표시합니다.
    fn canvas_nav_overlay(&mut self, ctx: &egui::Context, canvas: Rect) {
        let page_count = self.document.as_ref().map(|d| d.page_count()).unwrap_or(0);
        let can_prev = self.current_page > 0;
        let can_next = self.current_page + 1 < page_count;
        let fill = crate::theme::nord::semantic::overlay_bg();
        let stroke = crate::theme::nord::semantic::OVERLAY_BORDER;

        // 캔버스 중앙(왼쪽 패널이 열려 있어도)에 정렬되도록 화면 중앙 대비 오프셋.
        let screen = ctx.input(|i| i.raw.screen_rect).unwrap_or(canvas);
        let dx = canvas.center().x - screen.center().x;

        egui::Area::new(egui::Id::new("canvas_nav_overlay"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(dx, -12.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(fill)
                    .stroke(Stroke::new(1.0, stroke))
                    .corner_radius(8.0)
                    .inner_margin(egui::Margin::same(5))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                            if ui
                                .add_enabled(
                                    can_prev,
                                    egui::Button::new(icon_text(ui, "Prev", icons::CARET_LEFT)),
                                )
                                .on_hover_text("Previous page")
                                .clicked()
                            {
                                self.prev_page();
                            }
                            let mut page_num = self.current_page + 1;
                            if ui
                                .add(
                                    egui::DragValue::new(&mut page_num)
                                        .range(1..=page_count.max(1)),
                                )
                                .on_hover_text("Page number")
                                .changed()
                            {
                                self.goto_page(page_num.saturating_sub(1));
                            }
                            ui.label(format!("/ {}", page_count.max(1)));
                            if ui
                                .add_enabled(
                                    can_next,
                                    egui::Button::new(icon_text(ui, "Next", icons::CARET_RIGHT)),
                                )
                                .on_hover_text("Next page")
                                .clicked()
                            {
                                self.next_page();
                            }
                            ui.separator();
                            if ui
                                .button(icon_text(ui, "", icons::MAGNIFYING_GLASS_MINUS))
                                .on_hover_text("Zoom out")
                                .clicked()
                            {
                                self.zoom_by(1.0 / 1.25);
                            }
                            ui.label(format!("{:.0}%", self.view.zoom / ZOOM_100_PERCENT * 100.0));
                            if ui
                                .button(icon_text(ui, "", icons::MAGNIFYING_GLASS_PLUS))
                                .on_hover_text("Zoom in")
                                .clicked()
                            {
                                self.zoom_by(1.25);
                            }
                            ui.separator();
                            if ui
                                .button(icon_text(ui, "Fit Width", icons::ARROWS_HORIZONTAL))
                                .on_hover_text("Fit width")
                                .clicked()
                            {
                                self.fit_width();
                            }
                            if ui
                                .button(icon_text(ui, "Fit Height", icons::ARROWS_VERTICAL))
                                .on_hover_text("Fit height")
                                .clicked()
                            {
                                self.fit_height();
                            }
                        });
                    });
            });
    }

    /// 굿노트식 필기구 전용 세로 팔레트: 캔버스 오른쪽 중앙에 도구 선택과
    /// 자주 쓰는 색상(즐겨찾기)을 반투명 오버레이로 띄웁니다.
    fn canvas_palette_overlay(&mut self, ctx: &egui::Context, canvas: Rect) {
        if !self.show_palette || self.document.is_none() {
            return;
        }
        let fill = crate::theme::nord::semantic::overlay_bg();
        let stroke = crate::theme::nord::semantic::OVERLAY_BORDER;
        // 캔버스 오른쪽 끝에 붙도록 화면 대비 오프셋.
        let screen = ctx.input(|i| i.raw.screen_rect).unwrap_or(canvas);
        let dx = canvas.right() - screen.right() - 14.0;

        let mut to_add = false;
        let mut to_remove: Option<usize> = None;

        egui::Area::new(egui::Id::new("canvas_palette_overlay"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::RIGHT_CENTER, egui::vec2(dx, 0.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(fill)
                    .stroke(Stroke::new(1.0, stroke))
                    .corner_radius(8.0)
                    .inner_margin(egui::Margin::same(5))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                        // 도구 선택 (세로).
                        let tools = [
                            (ToolType::Pen, icons::PEN, "Pen (P)"),
                            (ToolType::Highlighter, icons::MARKER_CIRCLE, "Highlighter (H)"),
                            (ToolType::Eraser, icons::ERASER, "Eraser (E)"),
                            (ToolType::Pan, icons::HAND, "Pan (V)"),
                        ];
                        for (tool, ic, label) in tools {
                            if ui
                                .selectable_label(self.tool == tool, icon_text(ui, "", ic))
                                .on_hover_text(label)
                                .clicked()
                            {
                                self.tool = tool;
                                self.save_session();
                            }
                        }
                        ui.separator();

                        // 현재 펜 색 + 즐겨찾기에 추가 버튼.
                        let cur = Color32::from_rgba_unmultiplied(
                            self.pen_color[0],
                            self.pen_color[1],
                            self.pen_color[2],
                            self.pen_color[3],
                        );
                        if color_circle_swatch(ui, "current_color", cur, false)
                            .on_hover_text("Current pen color")
                            .clicked()
                        {
                            self.tool = ToolType::Pen;
                            self.save_session();
                        }
                        if ui
                            .add(egui::Button::new(icon_text(ui, "", icons::PLUS)).frame(false))
                            .on_hover_text("Add current color to favorites")
                            .clicked()
                        {
                            to_add = true;
                        }
                        ui.separator();

                        // 자주 쓰는 색상 (클릭 = 적용, 우클릭 = 제거).
                        for i in 0..self.favorite_colors.len() {
                            let c = self.favorite_colors[i]; // Copy
                            let col = Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]);
                            let selected = self.pen_color == c;
                            let resp = color_circle_swatch(ui, ("fav_swatch", i), col, selected);
                            if resp
                                .clone()
                                .on_hover_text("Set pen color (right-click to remove)")
                                .clicked()
                            {
                                self.pen_color = c;
                                self.tool = ToolType::Pen;
                                self.save_default_session();
                                self.save_session();
                            }
                            if resp.secondary_clicked() {
                                to_remove = Some(i);
                            }
                        }
                    });
            });

        if to_add {
            let c = self.pen_color;
            if !self.favorite_colors.contains(&c) && self.favorite_colors.len() < 16 {
                self.favorite_colors.push(c);
                self.save_default_session();
            }
        }
        if let Some(i) = to_remove {
            if i < self.favorite_colors.len() {
                self.favorite_colors.remove(i);
                self.save_default_session();
            }
        }
    }

    fn paint_search_highlights(&self, painter: &egui::Painter, origin: Pos2) {
        let match_fill = Color32::from_rgba_unmultiplied(255, 235, 60, 80);
        let current_fill = Color32::from_rgba_unmultiplied(255, 200, 40, 120);
        let current_stroke = Color32::from_rgb(255, 140, 0);
        for (i, m) in self.search_matches.iter().enumerate() {
            let r = m.rect;
            let a = self.view.page_to_view([r[0], r[1]]);
            let b = self.view.page_to_view([r[2], r[3]]);
            let rect = Rect::from_min_max(
                origin + Vec2::new(a[0], a[1]),
                origin + Vec2::new(b[0], b[1]),
            );
            if Some(i) == self.search_current {
                painter.rect_filled(rect, 2.0, current_fill);
                painter.rect_stroke(
                    rect,
                    2.0,
                    Stroke::new(2.0, current_stroke),
                    egui::StrokeKind::Inside,
                );
            } else {
                painter.rect_filled(rect, 2.0, match_fill);
            }
        }
    }

    /// Draws the paper grid / ruling / dots onto the page (notes only).
    fn paint_paper(&self, painter: &egui::Painter, origin: Pos2) {
        let w = self.page_size_pts[0];
        let h = self.page_size_pts[1];
        let paper = self.current_page_paper();
        let style = paper.style;
        let spacing = paper.spacing;
        let line = Color32::from_rgba_unmultiplied(120, 120, 140, 100);
        for [x0, y0, x1, y1] in paper_lines(w, h, style, spacing) {
            let a = self.view.page_to_view([x0, y0]);
            let b = self.view.page_to_view([x1, y1]);
            painter.line_segment(
                [origin + Vec2::new(a[0], a[1]), origin + Vec2::new(b[0], b[1])],
                Stroke::new(2.0, line),
            );
        }
        for [x, y] in paper_dots(w, h, style, spacing) {
            let v = self.view.page_to_view([x, y]);
            painter.circle_filled(origin + Vec2::new(v[0], v[1]), 2.0, line);
        }
    }

    // ---------- Input handling ----------

    fn handle_canvas_input(
        &mut self,
        ctx: &egui::Context,
        response: &egui::Response,
        origin: Pos2,
        canvas_size: [f32; 2],
    ) {
        let pointer_abs = response.interact_pointer_pos();

        // Zoom (pinch / trackpad pinch / Ctrl+wheel / Ctrl+two-finger scroll)
        let (zoom_delta, scroll) = ctx.input(|i| (i.zoom_delta(), i.smooth_scroll_delta));
        let scroll_x = scroll.x;
        let scroll_y = scroll.y;
        let ctrl_down = ctx.input(|i| i.modifiers.ctrl);
        let dt = ctx.input(|i| i.stable_dt).max(1e-4);
        let pointer_any_down = ctx.input(|i| i.pointer.any_down());

        // If egui already folded a pinch / Ctrl+scroll into zoom_delta, use it.
        // Otherwise synthesize zoom from Ctrl + wheel: discrete +1% per notch
        // that slowly accelerates (up to +8%) while you keep scrolling.
        let mut zoom_factor = zoom_delta;
        let mut scroll_zoom = false;
        let mut ctrl_wheel_notches = 0.0f32;
        {
            // Count raw wheel notches this frame (egui's smooth_scroll_delta is
            // smoothed, so a single notch can look like a huge jump).
            let events: Vec<egui::Event> = ctx.input(|i| i.events.iter().cloned().collect());
            for ev in &events {
                if let egui::Event::MouseWheel {
                    unit,
                    delta,
                    modifiers,
                    ..
                } = ev
                {
                    if modifiers.ctrl {
                        let n = match unit {
                            egui::MouseWheelUnit::Line => delta.y,
                            egui::MouseWheelUnit::Point => delta.y / 50.0,
                            egui::MouseWheelUnit::Page => delta.y,
                        };
                        ctrl_wheel_notches += n;
                    }
                }
            }
        }
        if ctrl_down && ctrl_wheel_notches.abs() > 1e-4 && (zoom_delta - 1.0).abs() <= 1e-4 {
            // Restart the ramp if the user paused between notches, then
            // accelerate from +1% up to +8% per notch while scrolling fast.
            let now = ctx.input(|i| i.time);
            if now - self.zoom_accel_last > 0.3 {
                self.zoom_accel = 0.0;
            }
            self.zoom_accel = (self.zoom_accel + 0.01 * ctrl_wheel_notches.abs()).min(0.08);
            self.zoom_accel_last = now;
            let dir = ctrl_wheel_notches.signum();
            zoom_factor = (1.0 + self.zoom_accel * dir).clamp(0.5, 2.0);
            scroll_zoom = true;
        } else if ctrl_down && ctx.input(|i| i.time) - self.zoom_accel_last > 0.3 {
            // Reset the acceleration ramp once scrolling pauses.
            self.zoom_accel = 0.0;
        }

        let zooming = (zoom_factor - 1.0).abs() > 1e-4;
        if zooming && (response.hovered() || scroll_zoom) {
            // Anchor at the pointer when available, otherwise the canvas center.
            let anchor = pointer_abs
                .map(|abs| [abs.x - origin.x, abs.y - origin.y])
                .unwrap_or([canvas_size[0] * 0.5, canvas_size[1] * 0.5]);
            self.view.zoom_at(anchor, zoom_factor, MIN_ZOOM, MAX_ZOOM);
            self.render_dirty = true;
            ctx.request_repaint();
        } else if (scroll_x.abs() + scroll_y.abs()) > 0.0 && response.hovered() && !ctrl_down {
            // Trackpad/wheel scroll: remember velocity for momentum, then pan/flip.
            self.scroll_vel = Vec2::new(scroll_x, scroll_y) / dt.max(1e-3);
            let page_h_px = self.page_size_pts[1] * self.view.zoom;
            if page_h_px <= canvas_size[1] && scroll_x.abs() <= scroll_y.abs() {
                // Whole page height visible & mostly-vertical gesture -> page flip.
                // Content follows the fingers (natural scrolling): positive
                // scroll_y (fingers down) shows earlier content -> previous page.
                if scroll_y > 0.0 {
                    self.prev_page();
                } else {
                    self.next_page();
                }
            } else {
                // Otherwise pan both axes; content follows the gesture.
                self.view.pan_by(scroll_x, scroll_y);
            }
            ctx.request_repaint();
        } else if !pointer_any_down {
            // Momentum: keep gliding after the gesture stops, then decay.
            let damping = (1.0 - SCROLL_DECAY * dt).max(0.0);
            self.scroll_vel *= damping;
            let page_h_px = self.page_size_pts[1] * self.view.zoom;
            let page_w_px = self.page_size_pts[0] * self.view.zoom;
            if self.scroll_vel.length_sq() > 1e-6 {
                let dx = if page_w_px <= canvas_size[0] { 0.0 } else { self.scroll_vel.x * dt };
                let dy = if page_h_px <= canvas_size[1] { 0.0 } else { self.scroll_vel.y * dt };
                self.view.pan_by(dx, dy);
                ctx.request_repaint();
            } else {
                self.scroll_vel = Vec2::ZERO;
            }
        }

        // Middle-button pan
        let middle_down = ctx.input(|i| i.pointer.button_down(egui::PointerButton::Middle));
        if middle_down {
            if let Some(abs) = ctx.input(|i| i.pointer.interact_pos()) {
                if let Some(last) = self.middle_pan_last {
                    let d = abs - last;
                    self.view.pan_by(d.x, d.y);
                }
                self.middle_pan_last = Some(abs);
            }
        } else {
            self.middle_pan_last = None;
        }

        let primary_down = ctx.input(|i| i.pointer.primary_down());

        match self.tool {
            ToolType::Pen | ToolType::Highlighter => {
                let page_w = self.page_size_pts[0];
                let page_h = self.page_size_pts[1];
                if primary_down && (response.is_pointer_button_down_on() || response.dragged()) {
                    if let Some(abs) = pointer_abs {
                        let p = abs - origin;
                        let raw = self.view.view_to_page([p.x, p.y]);
                        // 페이지(캔버스) 바깥에서는 필기 금지: 페이지 내부에서만
                        // 스트로크를 시작하고, 벗어나면 점을 추가하지 않습니다.
                        let inside = raw[0] >= 0.0
                            && raw[0] <= page_w
                            && raw[1] >= 0.0
                            && raw[1] <= page_h;
                        let page = [raw[0].clamp(0.0, page_w), raw[1].clamp(0.0, page_h)];
                        let pressure = self.sample_pressure(ctx);
                        if self.active_stroke.is_none() {
                            if inside {
                                let (color, width) = self.current_drawing_style();
                                self.active_stroke = Some(ActiveStroke {
                                    tool: self.tool,
                                    color,
                                    width,
                                    points: Vec::new(),
                                });
                            }
                        }
                        if let Some(st) = self.active_stroke.as_mut() {
                            if inside {
                                st.push(page, pressure);
                            }
                        }
                    }
                }
                if !primary_down && self.active_stroke.is_some() {
                    self.finish_stroke();
                }
                if response.clicked() && self.active_stroke.is_none() {
                    if let Some(abs) = pointer_abs {
                        let p = abs - origin;
                        let raw = self.view.view_to_page([p.x, p.y]);
                        // 클릭(점)도 페이지 내부일 때만 기록합니다.
                        if raw[0] >= 0.0
                            && raw[0] <= page_w
                            && raw[1] >= 0.0
                            && raw[1] <= page_h
                        {
                            let page = [raw[0].clamp(0.0, page_w), raw[1].clamp(0.0, page_h)];
                            let pressure = self.sample_pressure(ctx);
                            self.commit_dot(page, pressure);
                        }
                    }
                }
            }
            ToolType::Eraser => {
                if primary_down && (response.is_pointer_button_down_on() || response.dragged()) {
                    if let Some(abs) = pointer_abs {
                        let p = abs - origin;
                        let page = self.view.view_to_page([p.x, p.y]);
                        let radius = self.eraser_radius / self.view.zoom;
                        let removed = self.store.erase_at(self.current_page, page, radius);
                        if !removed.is_empty() {
                            self.history.push(Edit::RemoveStrokes {
                                page: self.current_page,
                                strokes: removed.clone(),
                            });
                            self.logger.log(AppEvent::StrokeErased {
                                page: self.current_page,
                                strokes: removed.len(),
                            });
                            self.autosave();
                        }
                    }
                }
            }
            ToolType::Pan => {
                if response.dragged() || response.is_pointer_button_down_on() {
                    if let Some(abs) = pointer_abs {
                        if let Some(last) = self.pan_last {
                            let d = abs - last;
                            self.view.pan_by(d.x, d.y);
                        }
                        self.pan_last = Some(abs);
                    }
                }
                if !primary_down {
                    self.pan_last = None;
                }
            }
        }
    }

    // ---------- Stroke painting ----------

    fn paint_active(&self, painter: &egui::Painter, active: &ActiveStroke, origin: Pos2) {
        let stroke = freedf_core::model::Stroke {
            id: 0,
            tool: active.tool,
            color: active.color,
            width: active.width,
            points: active.points.clone(),
        };
        self.paint_stroke(painter, &stroke, origin);
    }

    fn paint_stroke(&self, painter: &egui::Painter, stroke: &freedf_core::model::Stroke, origin: Pos2) {
        let color = Color32::from_rgba_unmultiplied(
            stroke.color[0],
            stroke.color[1],
            stroke.color[2],
            stroke.color[3],
        );
        let zoom = self.view.zoom;
        let pts = &stroke.points;
        if pts.is_empty() {
            return;
        }
        if pts.len() == 1 {
            let v = self.view.page_to_view([pts[0].x, pts[0].y]);
            let center = origin + Vec2::new(v[0], v[1]);
            let r = (self.pressure_curve.apply(stroke.width * zoom, pts[0].pressure) * 0.5)
                .max(0.75);
            painter.circle_filled(center, r, color);
            return;
        }
        for w in pts.windows(2) {
            let a = self.view.page_to_view([w[0].x, w[0].y]);
            let b = self.view.page_to_view([w[1].x, w[1].y]);
            let pressure = (w[0].pressure + w[1].pressure) * 0.5;
            let wpx = self
                .pressure_curve
                .apply(stroke.width * zoom, pressure)
                .max(0.5);
            let pa = origin + Vec2::new(a[0], a[1]);
            let pb = origin + Vec2::new(b[0], b[1]);
            painter.line_segment([pa, pb], Stroke::new(wpx, color));
        }
    }

    /// Draws a custom cursor sprite confined to the canvas, previewing the
    /// current tool's shape and color (Pen = translucent gray circle,
    /// Highlighter = colored rectangle, Eraser = white translucent circle).
    fn paint_custom_cursor(&self, painter: &egui::Painter, pos: Pos2, _time: f32) {
        match self.tool {
            ToolType::Pen => {
                // Small 4×4 pen dot.
                let rect = Rect::from_center_size(pos, Vec2::splat(4.0));
                painter.rect_filled(
                    rect,
                    0.0,
                    Color32::from_rgba_unmultiplied(120, 120, 120, 230),
                );
                painter.rect_stroke(
                    rect,
                    0.0,
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(70, 70, 70, 220)),
                    egui::StrokeKind::Outside,
                );
            }
            ToolType::Highlighter => {
                // Translucent rectangle in the highlighter color.
                let color = Color32::from_rgba_unmultiplied(
                    self.hi_color[0],
                    self.hi_color[1],
                    self.hi_color[2],
                    (self.hi_color[3] as f32 * 0.9) as u8,
                );
                let rect = Rect::from_center_size(pos, Vec2::new(22.0, 30.0));
                painter.rect_filled(rect, 4.0, color);
                painter.rect_stroke(
                    rect,
                    4.0,
                    Stroke::new(1.0, Color32::from_white_alpha(170)),
                    egui::StrokeKind::Inside,
                );
            }
            ToolType::Eraser => {
                // White translucent circle with a soft dark drop shadow so it
                // reads clearly even on white paper.
                let r = self.eraser_radius.max(6.0);
                painter.circle_filled(
                    pos + Vec2::new(2.5, 2.5),
                    r,
                    Color32::from_black_alpha(40),
                );
                painter.circle_filled(pos, r, Color32::from_white_alpha(85));
                painter.circle_stroke(pos, r, Stroke::new(2.0, Color32::from_white_alpha(215)));
                painter.circle_filled(pos, 2.0, Color32::from_black_alpha(110));
            }
            ToolType::Pan => {
                // Small, compact "move" crosshair (much smaller than the OS grab hand).
                let c = Color32::from_gray(180);
                let s = 6.0;
                painter.line_segment(
                    [pos - Vec2::new(s, 0.0), pos + Vec2::new(s, 0.0)],
                    Stroke::new(1.5, c),
                );
                painter.line_segment(
                    [pos - Vec2::new(0.0, s), pos + Vec2::new(0.0, s)],
                    Stroke::new(1.5, c),
                );
                painter.circle_filled(pos, 2.0, c);
            }
        }
    }

    // ---------- Shortcuts ----------

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
        if ctx
            .input(|i| i.key_pressed(egui::Key::PageDown) || i.key_pressed(egui::Key::ArrowRight))
        {
            self.next_page();
        }
        if ctx
            .input(|i| i.key_pressed(egui::Key::PageUp) || i.key_pressed(egui::Key::ArrowLeft))
        {
            self.prev_page();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals)) {
            self.zoom_by(1.25);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Minus)) {
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
        self.handle_shortcuts(&ctx);
        self.tabs_bar(ui);
        self.toolbar(ui);
        self.status_bar(ui);

        if self.show_library {
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
        if self.show_outline {
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
