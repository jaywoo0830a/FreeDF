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
    paper_dots, paper_lines, PagePaper, PaperSize, PaperStyle, PAPER_COLORS, PAPER_WHITE,
};
use freedf_core::pen::{ColorFamily, Palette, PressureCurve};
use freedf_core::search::{find_matches, TextMatch, TextRun};
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
    /// Fit whole page
    Page,
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

    // ---------- Input ----------
    active_stroke: Option<ActiveStroke>,
    pan_last: Option<Pos2>,
    middle_pan_last: Option<Pos2>,
    /// Trackpad/wheel momentum (points/sec) for inertial panning
    scroll_vel: Vec2,
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

    // ---------- Outline ----------
    outline: Vec<OutlineNode>,
    outline_loaded: bool,

    // ---------- Panels ----------
    show_notes: bool,
    show_outline: bool,

    // ---------- Logging / status ----------
    logger: Logger,
    file_name: String,
    status: Option<String>,

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
        let show_notes = if has { s.show_notes } else { true };
        let show_outline = if has { s.show_outline } else { false };
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
            active_stroke: None,
            pan_last: None,
            middle_pan_last: None,
            scroll_vel: Vec2::ZERO,
            page_anim: None,
            prev_texture: None,
            transition_last_page: 0,
            search_query: String::new(),
            search_runs: Vec::new(),
            search_matches: Vec::new(),
            search_current: None,
            outline: Vec::new(),
            outline_loaded: false,
            show_notes,
            show_outline,
            logger,
            file_name: String::new(),
            status: None,
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
            show_notes: self.show_notes,
            show_outline: self.show_outline,
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
            show_notes: self.show_notes,
            show_outline: self.show_outline,
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
        self.show_notes = s.show_notes;
        self.show_outline = s.show_outline;
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

    fn add_page_action(&mut self) {
        let Some(doc) = &mut self.document else {
            return;
        };
        let size = self.paper_size.size_pts();
        if let Err(e) = doc.add_page(size) {
            self.status = Some(e);
            return;
        }
        let total = doc.page_count();
        let new_index = total - 1;
        self.store.insert_page(new_index);
        // 새 페이지에는 현재 용지 설정(스타일/색)을 적용합니다.
        self.store.set_paper(
            new_index,
            PagePaper {
                style: self.paper_style,
                color: self.paper_color,
            },
        );
        self.current_page = new_index;
        self.logger.log(AppEvent::PageAdded {
            page: new_index,
            total,
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

    fn fit_page(&mut self) {
        self.pending_fit = Some(FitMode::Page);
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
            FitMode::Page => {
                self.view.zoom =
                    ViewTransform::fit_page_zoom(self.page_size_pts, canvas, CANVAS_MARGIN);
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
                for (i, tab) in self.tabs.iter().enumerate() {
                    let selected = i == self.active;
                    let resp = ui.selectable_label(selected, &tab.label);
                    if resp.clicked() {
                        to_switch = Some(i);
                    }
                    if ui
                        .button(egui::RichText::new("✕").small())
                        .on_hover_text("Close document")
                        .clicked()
                    {
                        to_close = Some(i);
                    }
                }
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
            // Row 1: file / page / zoom / tools
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button(icon_text(ui, "Open", icons::FOLDER_OPEN))
                    .on_hover_text("Open PDF (Ctrl+O)")
                    .clicked()
                {
                    self.open_file_dialog();
                }
                ui.menu_button(icon_text(ui, "Recent", icons::CLOCK_COUNTER_CLOCKWISE), |ui| {
                    let recents: Vec<RecentItem> =
                        self.recents.sorted().into_iter().cloned().collect();
                    if recents.is_empty() {
                        ui.label("No recent files yet");
                        return;
                    }
                    for item in &recents {
                        let label = match item.kind {
                            RecentKind::Note => format!("📄 {}", item.title),
                            RecentKind::File => format!("📎 {}", item.title),
                        };
                        if ui.button(label).clicked() {
                            let kind = item.kind;
                            let note_id = item.note_id;
                            let path = item.path.clone();
                            ui.close();
                            match kind {
                                RecentKind::Note => {
                                    if let Some(id) = note_id {
                                        self.open_note(id);
                                    }
                                }
                                RecentKind::File => {
                                    if let Some(p) = path {
                                        self.open_pdf(&p);
                                    }
                                }
                            }
                        }
                    }
                });
                ui.separator();

                if ui
                    .toggle_value(&mut self.show_notes, icon_text(ui, "Notes", icons::NOTE_PENCIL))
                    .on_hover_text("Notes")
                    .changed()
                {
                    self.pending_fit = Some(FitMode::Width);
                    self.save_session();
                }
                if ui
                    .toggle_value(&mut self.show_outline, icon_text(ui, "Outline", icons::LIST_BULLETS))
                    .on_hover_text("Outline")
                    .changed()
                {
                    self.pending_fit = Some(FitMode::Width);
                    self.save_session();
                }
                ui.separator();

                let page_count = self.document.as_ref().map(|d| d.page_count()).unwrap_or(0);
                let can_prev = self.current_page > 0;
                let can_next = self.current_page + 1 < page_count;
                if ui
                    .add_enabled(can_prev, egui::Button::new(icon_text(ui, "Prev", icons::CARET_LEFT)))
                    .on_hover_text("Previous page")
                    .clicked()
                {
                    self.prev_page();
                }
                let mut page_num = self.current_page + 1;
                if ui
                    .add(egui::DragValue::new(&mut page_num).range(1..=page_count.max(1)))
                    .on_hover_text("Page number")
                    .changed()
                {
                    self.goto_page(page_num.saturating_sub(1));
                }
                if ui
                    .add_enabled(can_next, egui::Button::new(icon_text(ui, "Next", icons::CARET_RIGHT)))
                    .on_hover_text("Next page")
                    .clicked()
                {
                    self.next_page();
                }
                ui.label(format!("/ {}", page_count.max(1)));
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

                if ui
                    .add_enabled(page_count > 0, egui::Button::new(icon_text(ui, "Add Page", icons::PLUS_SQUARE)))
                    .on_hover_text("Add blank page at the end")
                    .clicked()
                {
                    self.add_page_action();
                }
                if ui
                    .add_enabled(page_count > 1, egui::Button::new(icon_text(ui, "Delete", icons::TRASH_SIMPLE)))
                    .on_hover_text("Delete this page")
                    .clicked()
                {
                    self.delete_page_action();
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
                if ui
                    .button(icon_text(ui, "Fit Width", icons::ARROWS_HORIZONTAL))
                    .on_hover_text("Fit width")
                    .clicked()
                {
                    self.fit_width();
                }
                if ui
                    .button(icon_text(ui, "Fit Page", icons::ARROWS_IN_CARDINAL))
                    .on_hover_text("Fit page")
                    .clicked()
                {
                    self.fit_page();
                }
                if !self.show_notes && !self.show_outline {
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
                            .selectable_label(self.page_align == a, icon_text(ui, a.label(), ic))
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

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(2.0);

            // Row 2: drawing tools (tool picker + tool settings)
            ui.horizontal_wrapped(|ui| {
                let tool_buttons = [
                    (ToolType::Pen, icons::PEN, "Pen"),
                    (ToolType::Highlighter, icons::MARKER_CIRCLE, "Highlight"),
                    (ToolType::Eraser, icons::ERASER, "Eraser"),
                    (ToolType::Pan, icons::HAND, "Pan"),
                ];
                for (tool, ic, label) in tool_buttons {
                    if ui
                        .selectable_label(self.tool == tool, icon_text(ui, label, ic))
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
                        // Even, square color swatches forming a neat color bar.
                        // Each sits in a 24×28 cell so the square's center lines
                        // up with the other 28px-tall controls on the row.
                        for swatch in &swatches {
                            let color = Color32::from_rgba_unmultiplied(
                                swatch[0],
                                swatch[1],
                                swatch[2],
                                swatch[3],
                            );
                            let selected = *swatch == self.pen_color;
                            let cell = egui::Layout::centered_and_justified(
                                egui::Direction::LeftToRight,
                            );
                            ui.allocate_ui_with_layout(egui::vec2(24.0, 28.0), cell, |ui| {
                                let mut btn =
                                    egui::Button::new("").fill(color).corner_radius(2);
                                if selected {
                                    // Brand-colored selection ring (drawn inside,
                                    // so the swatch keeps its exact size)
                                    btn = btn.stroke(Stroke::new(
                                        2.0,
                                        ui.visuals().selection.stroke.color,
                                    ));
                                }
                                if ui.add_sized([20.0, 20.0], btn).clicked() {
                                    self.pen_color = *swatch;
                                    self.save_default_session();
                                    self.save_session();
                                }
                            });
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

                // Paper (grid / ruling / color) — applied per page;
                // paper size selects the size for new pages & notes.
                ui.separator();
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
                for paper in PAPER_COLORS {
                    let color =
                        Color32::from_rgba_unmultiplied(paper[0], paper[1], paper[2], paper[3]);
                    let selected = self.paper_color == *paper;
                    let cell = egui::Layout::centered_and_justified(egui::Direction::LeftToRight);
                    ui.allocate_ui_with_layout(egui::vec2(24.0, 28.0), cell, |ui| {
                        let mut btn = egui::Button::new("").fill(color).corner_radius(2);
                        if selected {
                            btn = btn
                                .stroke(Stroke::new(2.0, ui.visuals().selection.stroke.color));
                        }
                        if ui.add_sized([20.0, 20.0], btn).clicked() {
                            self.paper_color = *paper;
                            self.apply_paper_to_current_page();
                            self.save_default_session();
                            self.save_session();
                        }
                    });
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
            });

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(2.0);

            // Row 3: search
            ui.horizontal_wrapped(|ui| {
                ui.label(icon_text(ui, "Find", icons::MAGNIFYING_GLASS));
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.search_query)
                        .hint_text("Search text in this page...")
                        .desired_width(180.0),
                );
                let submitted =
                    resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if ui.button("Find").clicked() || submitted {
                    self.search_update();
                }
                let can = !self.search_matches.is_empty();
                if ui
                    .add_enabled(can, egui::Button::new(icon_text(ui, "Prev", icons::CARET_UP)))
                    .on_hover_text("Previous match")
                    .clicked()
                {
                    self.search_find(false);
                }
                if ui
                    .add_enabled(can, egui::Button::new(icon_text(ui, "Next", icons::CARET_DOWN)))
                    .on_hover_text("Next match")
                    .clicked()
                {
                    self.search_find(true);
                }
                if ui
                    .add_enabled(can, egui::Button::new(icon_text(ui, "Clear", icons::X)))
                    .on_hover_text("Clear search")
                    .clicked()
                {
                    self.search_clear();
                }
                if !self.search_matches.is_empty() {
                    let cur = self.search_current.map(|c| c + 1).unwrap_or(0);
                    ui.label(format!("{cur}/{}", self.search_matches.len()));
                }
            });
            ui.add_space(4.0);
        });
    }

    // ---------- UI: notes panel ----------

    fn notes_panel(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);
        ui.add_space(4.0);
        ui.heading("Notes");
        ui.add_space(2.0);
        ui.horizontal(|ui| {
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
            let has_note = self.current_note.is_some();
            if ui
                .add_enabled(has_note, egui::Button::new(icon_text(ui, "Rename", icons::PENCIL_SIMPLE)))
                .on_hover_text("Rename note")
                .clicked()
            {
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
            if ui
                .add_enabled(has_note, egui::Button::new(icon_text(ui, "Delete", icons::TRASH_SIMPLE)))
                .on_hover_text("Delete note")
                .clicked()
            {
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
        });
        ui.separator();
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                let notes: Vec<(u64, String, usize)> = self
                    .notes
                    .list()
                    .iter()
                    .map(|m| (m.id, m.title.clone(), m.page_count))
                    .collect();
                if notes.is_empty() {
                    ui.label("No notes yet. Click ＋ New to create one.");
                }
                for (id, title, page_count) in notes {
                    let selected = self.current_note == Some(id);
                    let text = if page_count > 0 {
                        format!("{title}  ({}p)", page_count)
                    } else {
                        title
                    };
                    if ui.selectable_label(selected, text).clicked() {
                        self.open_note(id);
                    }
                }
            });
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

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let page_count = self.document.as_ref().map(|d| d.page_count()).unwrap_or(0);
        let zoom_pct = self.view.zoom / ZOOM_100_PERCENT * 100.0;
        let stroke_count = self.store.total_stroke_count();
        let file_name = self.file_name.clone();
        let status = self.status.clone();
        let ink = if self.pressure_enabled { "Ink: On" } else { "Ink: Off" };

        egui::Panel::bottom("status").show(ui, |ui| {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                if file_name.is_empty() {
                    ui.label(egui::RichText::new("No document").weak());
                } else {
                    ui.label(egui::RichText::new(&file_name).strong());
                }
                ui.separator();
                ui.label(format!(
                    "{}/{} pages",
                    (self.current_page + 1).min(page_count.max(1)),
                    page_count.max(1)
                ));
                ui.separator();
                ui.label(format!("Zoom {zoom_pct:.0}%"));
                ui.separator();
                ui.label(format!("Strokes: {stroke_count}"));
                ui.separator();
                ui.label(ink);
                if let Some(s) = &status {
                    ui.separator();
                    ui.label(
                        egui::RichText::new(s).color(ui.visuals().error_fg_color),
                    );
                }
            });
            ui.add_space(2.0);
        });
    }

    // ---------- UI: canvas ----------

    fn canvas(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let canvas = response.rect;
        let origin = canvas.min;
        let canvas_size = [canvas.width(), canvas.height()];
        self.last_canvas = canvas_size;

        // Background (Catppuccin crust: dark = Mocha crust, light = Latte crust)
        let bg = match ui.ctx().theme() {
            egui::Theme::Dark => Color32::from_rgb(0x11, 0x11, 0x1B), // Mocha crust
            egui::Theme::Light => Color32::from_rgb(0xDC, 0xE0, 0xE8), // Latte crust
        };
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
    }

    /// 페이지 내비게이션 오버레이: Prev/Next, 줌, Fit Width/Height를
    /// 캔버스 중앙 하단에 반투명하게 고정 표시합니다.
    fn canvas_nav_overlay(&mut self, ctx: &egui::Context, canvas: Rect) {
        let page_count = self.document.as_ref().map(|d| d.page_count()).unwrap_or(0);
        let can_prev = self.current_page > 0;
        let can_next = self.current_page + 1 < page_count;
        let dark = matches!(ctx.theme(), egui::Theme::Dark);
        let fill = if dark {
            Color32::from_rgba_unmultiplied(0x1E, 0x1E, 0x2E, 205) // Mocha base
        } else {
            Color32::from_rgba_unmultiplied(0xEF, 0xF1, 0xF5, 215) // Latte base
        };
        let stroke = if dark {
            Color32::from_rgba_unmultiplied(0x7F, 0x84, 0x9C, 60) // Mocha overlay1
        } else {
            Color32::from_rgba_unmultiplied(0x6C, 0x6F, 0x85, 40) // Latte overlay0
        };

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
        let style = self.current_page_paper().style;
        let line = Color32::from_rgba_unmultiplied(120, 120, 140, 100);
        for [x0, y0, x1, y1] in paper_lines(w, h, style) {
            let a = self.view.page_to_view([x0, y0]);
            let b = self.view.page_to_view([x1, y1]);
            painter.line_segment(
                [origin + Vec2::new(a[0], a[1]), origin + Vec2::new(b[0], b[1])],
                Stroke::new(2.0, line),
            );
        }
        for [x, y] in paper_dots(w, h, style) {
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
        // Otherwise synthesize zoom from Ctrl + two-finger scroll for trackpads
        // whose pinch gesture is not reported to the windowing system.
        let mut zoom_factor = zoom_delta;
        let mut scroll_zoom = false;
        if ctrl_down
            && (scroll_x.abs() + scroll_y.abs()) > 1e-4
            && (zoom_delta - 1.0).abs() <= 1e-4
        {
            let step = (scroll_x + scroll_y) * 0.01;
            zoom_factor = (1.0 + step).clamp(0.5, 2.0);
            scroll_zoom = true;
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
                if primary_down && (response.is_pointer_button_down_on() || response.dragged()) {
                    if let Some(abs) = pointer_abs {
                        let p = abs - origin;
                        let page = self.view.view_to_page([p.x, p.y]);
                        let pressure = self.sample_pressure(ctx);
                        if self.active_stroke.is_none() {
                            let (color, width) = self.current_drawing_style();
                            self.active_stroke = Some(ActiveStroke {
                                tool: self.tool,
                                color,
                                width,
                                points: Vec::new(),
                            });
                        }
                        let st = self.active_stroke.as_mut().expect("just created");
                        st.push(page, pressure);
                    }
                }
                if !primary_down && self.active_stroke.is_some() {
                    self.finish_stroke();
                }
                if response.clicked() && self.active_stroke.is_none() {
                    if let Some(abs) = pointer_abs {
                        let p = abs - origin;
                        let page = self.view.view_to_page([p.x, p.y]);
                        let pressure = self.sample_pressure(ctx);
                        self.commit_dot(page, pressure);
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
    /// Highlighter = colored rectangle, Eraser = animated red circle).
    fn paint_custom_cursor(&self, painter: &egui::Painter, pos: Pos2, time: f32) {
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
                // Animated red circle: radius pulses with time.
                let base = self.eraser_radius;
                let pulse = 1.0 + 0.12 * (time * 6.0).sin();
                let r = (base * pulse).max(6.0);
                painter.circle_stroke(
                    pos,
                    r,
                    Stroke::new(2.0, Color32::from_rgba_unmultiplied(255, 70, 70, 230)),
                );
                painter.circle_filled(pos, 2.5, Color32::from_rgb(255, 60, 60));
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
            self.search_update();
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

        if self.show_notes {
            egui::Panel::left("notes_panel")
                .resizable(true)
                .default_size(230.0)
                .show(ui, |ui| self.notes_panel(ui));
        }
        if self.show_outline {
            egui::Panel::left("outline_panel")
                .resizable(true)
                .default_size(220.0)
                .show(ui, |ui| self.outline_panel(ui));
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
