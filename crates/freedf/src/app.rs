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
use freedf_core::pen::{ColorFamily, Palette, PressureCurve};
use freedf_core::search::{find_matches, TextMatch, TextRun};
use freedf_core::store::AnnotationStore;
use freedf_core::transform::{PageAlign, ViewTransform, MAX_ZOOM, MIN_ZOOM, ZOOM_100_PERCENT};

use crate::export::draw_strokes_on_image;
use crate::pdf::DocumentView;
use egui_phosphor_icons::icons;

/// Canvas margin around the page
const CANVAS_MARGIN: f32 = 16.0;
/// Page top margin
const TOP_MARGIN: f32 = 16.0;
/// Default blank page size (A4, points)
const A4_PTS: [f32; 2] = [595.0, 842.0];
/// Page transition animation duration (seconds)
const PAGE_ANIM_SECS: f32 = 0.28;
/// Scroll momentum decay (1/second)
const SCROLL_DECAY: f32 = 6.0;

/// Fit mode
#[derive(Debug, Clone, Copy, PartialEq)]
enum FitMode {
    /// Fit page width
    Width,
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

/// RichText for an icon glyph rendered in the Phosphor icon font.
/// Icons render icon-only (human-readable labels live in tooltips), so the
/// `label` parameter is only kept for call-site readability.
fn icon_label(_label: &str, ic: egui_phosphor_icons::Icon) -> egui::RichText {
    ic.regular().size(18.0)
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

pub struct FreeDfApp {
    // ---------- Notes ----------
    notes: NotesManager,
    current_note: Option<u64>,

    // ---------- Document ----------
    document: Option<DocumentView>,
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

    // ---------- Fallback dialog ----------
    modal: Option<ModalState>,
}

impl FreeDfApp {
    pub fn new(cc: &eframe::CreationContext<'_>, notes: NotesManager, logger: Logger) -> Self {
        let dark = matches!(cc.egui_ctx.theme(), egui::Theme::Dark);
        let pen_color = if dark {
            [255, 255, 255, 255]
        } else {
            Palette::default_pen()
        };
        let hi_color = if dark {
            [255, 220, 60, 110]
        } else {
            Palette::default_highlighter()
        };
        Self {
            notes,
            current_note: None,
            document: None,
            file_path: None,
            current_page: 0,
            page_size_pts: A4_PTS,
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
            tool: ToolType::Pen,
            color_family: ColorFamily::Black,
            pen_color,
            pen_width: 2.5,
            hi_color,
            hi_width: 16.0,
            eraser_radius: 16.0,
            pressure_enabled: true,
            pressure_curve: PressureCurve::default(),
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
            show_notes: true,
            show_outline: false,
            logger,
            file_name: String::new(),
            status: None,
            modal: None,
        }
    }

    // ---------- Notes ----------

    /// Shows an error both in the status bar and as a popup alert.
    fn show_error(&mut self, msg: String) {
        self.status = Some(msg.clone());
        self.modal = Some(ModalState::alert("Error", &msg));
    }

    fn create_note_action(&mut self, title: &str) {
        match self.notes.create_note(title) {
            Ok(meta) => {
                let pdf_path = self.notes.pdf_path(meta.id);
                if let Err(e) = DocumentView::create_blank_pdf(&pdf_path, A4_PTS) {
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
                if self.current_note == Some(id) {
                    self.close_document();
                }
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
        let pdf_path = self.notes.pdf_path(id);
        if !pdf_path.exists() {
            if let Err(e) = DocumentView::create_blank_pdf(&pdf_path, A4_PTS) {
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
        match DocumentView::open(&pdf_path) {
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
                self.logger.log(AppEvent::NoteOpened {
                    note_id: id,
                    title: meta.title.clone(),
                    page_count,
                });
                self.load_outline_if_needed();
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
        match DocumentView::open(path) {
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
                self.load_outline_if_needed();
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
        self.pending_fit = Some(FitMode::Width);
        self.search_update();
        if let Some(doc) = &self.document {
            self.logger.log(AppEvent::PageChanged {
                page: self.current_page,
                total: doc.page_count(),
            });
        }
        self.start_page_anim(from, self.current_page);
        self.transition_last_page = self.current_page;
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
        let size = doc.page_size_pts(self.current_page);
        if let Err(e) = doc.add_page(size) {
            self.status = Some(e);
            return;
        }
        let total = doc.page_count();
        let new_index = total - 1;
        self.store.insert_page(new_index);
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
    }

    fn fit_width(&mut self) {
        self.pending_fit = Some(FitMode::Width);
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
            FitMode::Page => {
                self.view.zoom =
                    ViewTransform::fit_page_zoom(self.page_size_pts, canvas, CANVAS_MARGIN);
            }
        }
        self.view
            .align_page(self.page_size_pts, canvas, TOP_MARGIN, self.page_align);
        self.render_dirty = true;
    }

    /// Re-applies the current horizontal alignment without changing the zoom.
    fn realign(&mut self) {
        if self.document.is_none() {
            return;
        }
        self.view
            .align_page(self.page_size_pts, self.last_canvas, TOP_MARGIN, self.page_align);
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

    // ---------- UI: toolbar ----------

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.add_space(4.0);
            // Row 1: file / page / zoom / tools
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button(icon_label("", icons::FOLDER_OPEN))
                    .on_hover_text("Open PDF (Ctrl+O)")
                    .clicked()
                {
                    self.open_file_dialog();
                }
                ui.separator();

                if ui
                    .toggle_value(&mut self.show_notes, icon_label("", icons::NOTE_PENCIL))
                    .on_hover_text("Notes")
                    .changed()
                {
                    self.pending_fit = Some(FitMode::Width);
                }
                if ui
                    .toggle_value(&mut self.show_outline, icon_label("", icons::LIST_BULLETS))
                    .on_hover_text("Outline")
                    .changed()
                {
                    self.pending_fit = Some(FitMode::Width);
                }
                ui.separator();

                let page_count = self.document.as_ref().map(|d| d.page_count()).unwrap_or(0);
                let can_prev = self.current_page > 0;
                let can_next = self.current_page + 1 < page_count;
                if ui
                    .add_enabled(can_prev, egui::Button::new(icon_label("", icons::CARET_LEFT)))
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
                    .add_enabled(can_next, egui::Button::new(icon_label("", icons::CARET_RIGHT)))
                    .on_hover_text("Next page")
                    .clicked()
                {
                    self.next_page();
                }
                ui.label(format!("/ {}", page_count.max(1)));
                ui.separator();

                if ui
                    .add_enabled(page_count > 0, egui::Button::new(icon_label("", icons::PLUS_SQUARE)))
                    .on_hover_text("Add blank page at the end")
                    .clicked()
                {
                    self.add_page_action();
                }
                if ui
                    .add_enabled(page_count > 1, egui::Button::new(icon_label("", icons::TRASH_SIMPLE)))
                    .on_hover_text("Delete this page")
                    .clicked()
                {
                    self.delete_page_action();
                }
                ui.separator();

                if ui
                    .button(icon_label("", icons::MAGNIFYING_GLASS_MINUS))
                    .on_hover_text("Zoom out")
                    .clicked()
                {
                    self.zoom_by(1.0 / 1.25);
                }
                ui.label(format!("{:.0}%", self.view.zoom / ZOOM_100_PERCENT * 100.0));
                if ui
                    .button(icon_label("", icons::MAGNIFYING_GLASS_PLUS))
                    .on_hover_text("Zoom in")
                    .clicked()
                {
                    self.zoom_by(1.25);
                }
                if ui
                    .button(icon_label("", icons::ARROWS_HORIZONTAL))
                    .on_hover_text("Fit width")
                    .clicked()
                {
                    self.fit_width();
                }
                if ui
                    .button(icon_label("", icons::ARROWS_IN_CARDINAL))
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
                            .selectable_label(self.page_align == a, icon_label("", ic))
                            .on_hover_text(hint)
                            .clicked()
                        {
                            self.page_align = a;
                            self.realign();
                        }
                    }
                }
                ui.separator();

                let tool_icons = [
                    (ToolType::Pen, icons::PEN, "Pen (P)"),
                    (ToolType::Highlighter, icons::MARKER_CIRCLE, "Highlighter (H)"),
                    (ToolType::Eraser, icons::ERASER, "Eraser (E)"),
                    (ToolType::Pan, icons::HAND, "Pan (V)"),
                ];
                for (tool, ic, hint) in tool_icons {
                    if ui
                        .selectable_label(self.tool == tool, icon_label("", ic))
                        .on_hover_text(hint)
                        .clicked()
                    {
                        self.tool = tool;
                    }
                }

                match self.tool {
                    ToolType::Pen => {
                        egui::ComboBox::from_id_salt("family")
                            .selected_text(self.color_family.label())
                            .show_ui(ui, |ui| {
                                for family in ColorFamily::all() {
                                    ui.selectable_value(
                                        &mut self.color_family,
                                        family,
                                        family.label(),
                                    );
                                }
                            });
                        let swatches = Palette::swatches(self.color_family);
                        for swatch in &swatches {
                            let color = Color32::from_rgba_unmultiplied(
                                swatch[0],
                                swatch[1],
                                swatch[2],
                                swatch[3],
                            );
                            let selected = *swatch == self.pen_color;
                            let mut btn = egui::Button::new(egui::RichText::new("").color(color))
                                .fill(color);
                            if selected {
                                btn = btn.stroke(Stroke::new(2.0, Color32::WHITE));
                            }
                            if ui.add_sized([18.0, 18.0], btn).clicked() {
                                self.pen_color = *swatch;
                            }
                        }
                        ui.add(egui::Slider::new(&mut self.pen_width, 0.5..=12.0).text("Width"));
                        ui.checkbox(&mut self.pressure_enabled, "Pressure");
                        if self.pressure_enabled {
                            ui.add(
                                egui::Slider::new(&mut self.pressure_curve.min_ratio, 0.1..=1.0)
                                    .text("Min"),
                            );
                            ui.add(
                                egui::Slider::new(&mut self.pressure_curve.max_ratio, 1.0..=3.0)
                                    .text("Max"),
                            );
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
                        }
                        ui.add(egui::Slider::new(&mut self.hi_width, 4.0..=40.0).text("Width"));
                    }
                    ToolType::Eraser => {
                        ui.add(egui::Slider::new(&mut self.eraser_radius, 4.0..=60.0).text("Radius"));
                    }
                    ToolType::Pan => {}
                }
                ui.separator();

                if ui
                    .add_enabled(
                        self.history.can_undo(),
                        egui::Button::new(icon_label("", icons::ARROW_COUNTER_CLOCKWISE)),
                    )
                    .on_hover_text("Undo (Ctrl+Z)")
                    .clicked()
                {
                    self.undo();
                }
                if ui
                    .add_enabled(
                        self.history.can_redo(),
                        egui::Button::new(icon_label("", icons::ARROW_CLOCKWISE)),
                    )
                    .on_hover_text("Redo (Ctrl+Y)")
                    .clicked()
                {
                    self.redo();
                }
                if ui
                    .button(icon_label("", icons::X_CIRCLE))
                    .on_hover_text("Clear page")
                    .clicked()
                {
                    self.clear_page();
                }
                ui.separator();

                if ui
                    .button(icon_label("", icons::FLOPPY_DISK))
                    .on_hover_text("Save annotations (Ctrl+S)")
                    .clicked()
                {
                    self.save_annotations();
                }
                if ui
                    .button(icon_label("", icons::FOLDER_SIMPLE))
                    .on_hover_text("Load annotations")
                    .clicked()
                {
                    self.load_annotations();
                }
                if ui
                    .button(icon_label("", icons::IMAGE))
                    .on_hover_text("Export current page as PNG (Ctrl+E)")
                    .clicked()
                {
                    self.export_png();
                }
            });

            ui.add_space(4.0);
            ui.separator();
            ui.add_space(2.0);

            // Row 2: search
            ui.horizontal_wrapped(|ui| {
                ui.label(icon_label("", icons::MAGNIFYING_GLASS));
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
                    .add_enabled(can, egui::Button::new(icon_label("", icons::CARET_UP)))
                    .on_hover_text("Previous match")
                    .clicked()
                {
                    self.search_find(false);
                }
                if ui
                    .add_enabled(can, egui::Button::new(icon_label("", icons::CARET_DOWN)))
                    .on_hover_text("Next match")
                    .clicked()
                {
                    self.search_find(true);
                }
                if ui
                    .add_enabled(can, egui::Button::new(icon_label("", icons::X)))
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
        ui.heading("Notes");
        ui.horizontal(|ui| {
            if ui
                .button(icon_label("", icons::PLUS))
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
                .add_enabled(has_note, egui::Button::new(icon_label("", icons::PENCIL_SIMPLE)))
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
                .add_enabled(has_note, egui::Button::new(icon_label("", icons::TRASH_SIMPLE)))
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
        ui.heading("Outline");
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
                    ui.label(egui::RichText::new(s).color(Color32::from_rgb(230, 120, 60)));
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

        // Background (neutral gray so the page reads clearly)
        let bg = match ui.ctx().theme() {
            egui::Theme::Dark => Color32::from_rgb(46, 46, 46),
            egui::Theme::Light => Color32::from_rgb(168, 168, 168),
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
                Color32::WHITE,
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
                    Color32::WHITE,
                );
            }
        }

        // Current-page rect/origin (shifted during a transition so border & ink follow)
        let draw_rect = page_rect.translate(Vec2::new(anim_dx, 0.0));
        let draw_origin = origin + Vec2::new(anim_dx, 0.0);

        // Custom tool cursor while hovering the page area
        if let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) {
            if draw_rect.contains(pos) {
                match self.tool {
                    ToolType::Pan => ctx.set_cursor_icon(egui::CursorIcon::Grab),
                    _ => {
                        // Hide the OS cursor and draw a custom one
                        ctx.set_cursor_icon(egui::CursorIcon::None);
                        self.paint_custom_cursor(&painter, pos);
                    }
                }
            }
        }

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
                    Color32::WHITE,
                );
            }
            painter.rect_stroke(
                draw_rect,
                egui::CornerRadius::same(2),
                Stroke::new(1.0, Color32::from_gray(120)),
                egui::StrokeKind::Inside,
            );
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

        // Zoom hint
        if self.document.is_some() && self.view.zoom >= 4.0 {
            painter.text(
                canvas.left_top() + Vec2::new(10.0, 10.0),
                egui::Align2::LEFT_TOP,
                "Ctrl+wheel: zoom / wheel: scroll & page / middle button: pan",
                egui::TextStyle::Small.resolve(ui.style()),
                ui.visuals().weak_text_color(),
            );
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

    // ---------- Input handling ----------

    fn handle_canvas_input(
        &mut self,
        ctx: &egui::Context,
        response: &egui::Response,
        origin: Pos2,
        canvas_size: [f32; 2],
    ) {
        let pointer_abs = response.interact_pointer_pos();

        // Zoom (pinch / Ctrl+wheel)
        let (zoom_delta, scroll) = ctx.input(|i| (i.zoom_delta(), i.smooth_scroll_delta));
        let scroll_x = scroll.x;
        let scroll_y = scroll.y;
        let ctrl_down = ctx.input(|i| i.modifiers.ctrl);
        let dt = ctx.input(|i| i.stable_dt).max(1e-4);
        let pointer_any_down = ctx.input(|i| i.pointer.any_down());

        if (zoom_delta - 1.0).abs() > 1e-4 && response.hovered() {
            if let Some(abs) = pointer_abs {
                let anchor = [abs.x - origin.x, abs.y - origin.y];
                self.view.zoom_at(anchor, zoom_delta, MIN_ZOOM, MAX_ZOOM);
                self.render_dirty = true;
            }
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

    /// Draws a custom cursor that previews the current tool (size + color).
    fn paint_custom_cursor(&self, painter: &egui::Painter, pos: Pos2) {
        let zoom = self.view.zoom;
        match self.tool {
            ToolType::Pen => {
                let color = Color32::from_rgba_unmultiplied(
                    self.pen_color[0],
                    self.pen_color[1],
                    self.pen_color[2],
                    255,
                );
                let r = (self.pen_width * zoom * 0.5).clamp(1.5, 14.0);
                // Crosshair guides
                let cross = Stroke::new(1.0, Color32::from_gray(150));
                painter.line_segment([pos - Vec2::new(13.0, 0.0), pos + Vec2::new(13.0, 0.0)], cross);
                painter.line_segment([pos - Vec2::new(0.0, 13.0), pos + Vec2::new(0.0, 13.0)], cross);
                // Nib sized to the pen width
                painter.circle_stroke(pos, r + 1.0, Stroke::new(1.0, Color32::from_black_alpha(90)));
                painter.circle_filled(pos, r, color);
            }
            ToolType::Highlighter => {
                let w = (self.hi_width * zoom).clamp(3.0, 60.0);
                let color = Color32::from_rgba_unmultiplied(
                    self.hi_color[0],
                    self.hi_color[1],
                    self.hi_color[2],
                    130,
                );
                let bar = Rect::from_center_size(pos, Vec2::new(w, 30.0));
                painter.rect_filled(bar, 4.0, color);
                painter.rect_stroke(
                    bar,
                    4.0,
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 160)),
                    egui::StrokeKind::Inside,
                );
            }
            ToolType::Eraser => {
                // Red ring showing the erase radius, plus a center dot
                painter.circle_stroke(
                    pos,
                    self.eraser_radius,
                    Stroke::new(1.5, Color32::from_rgba_unmultiplied(255, 90, 90, 220)),
                );
                painter.circle_stroke(
                    pos,
                    self.eraser_radius + 1.0,
                    Stroke::new(1.0, Color32::from_black_alpha(60)),
                );
                painter.circle_filled(pos, 2.0, Color32::from_rgb(255, 90, 90));
            }
            ToolType::Pan => {}
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
