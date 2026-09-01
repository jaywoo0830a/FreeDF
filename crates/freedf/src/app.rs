//! FreeDF 메인 앱: PDF 뷰어 캔버스 + 필기/메모 드로잉 + 툴바 + 파일 IO.
//!
//! egui의 화면 좌표 공간을 그대로 사용합니다.
//! 캔버스(뷰포트) 좌상단 = `response.rect.min`, 페이지 좌표 ↔ 뷰 좌표는
//! `freedf_core::transform::ViewTransform`이 담당합니다.

use std::path::{Path, PathBuf};

use eframe::egui;
use egui::{Color32, Pos2, Rect, Sense, Stroke, Vec2};

use freedf_core::history::{Edit, History};
use freedf_core::model::{PageIndex, StrokePoint, ToolType};
use freedf_core::store::AnnotationStore;
use freedf_core::transform::{ViewTransform, MAX_ZOOM, MIN_ZOOM, ZOOM_100_PERCENT};

use crate::export::draw_strokes_on_image;
use crate::pdf::DocumentView;

/// 캔버스 기본 여백
const CANVAS_MARGIN: f32 = 16.0;
/// 페이지 위쪽 여백
const TOP_MARGIN: f32 = 16.0;

/// 페이지 맞춤 모드
#[derive(Debug, Clone, Copy, PartialEq)]
enum FitMode {
    /// 가로 폭 맞춤
    Width,
    /// 페이지 전체 맞춤
    Page,
}

/// 그리는 중인 스트로크
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

/// 폴백 파일 대화상자 (Windows 이외/네이티브 대화상자 없을 때)
#[derive(Debug, Clone, Copy, PartialEq)]
enum ModalAction {
    OpenPdf,
    LoadAnnotations,
    SaveAnnotations,
    ExportPng,
}

#[derive(Debug, Clone)]
struct ModalState {
    title: String,
    hint: String,
    path: String,
    action: ModalAction,
}

impl ModalState {
    fn new(title: &str, hint: &str, action: ModalAction) -> Self {
        Self {
            title: title.to_string(),
            hint: hint.to_string(),
            path: String::new(),
            action,
        }
    }
}

pub struct FreeDfApp {
    // 문서
    document: Option<DocumentView>,
    file_path: Option<PathBuf>,
    current_page: usize,
    page_size_pts: [f32; 2],

    // 뷰
    view: ViewTransform,
    last_canvas: [f32; 2],
    pending_fit: Option<FitMode>,

    // 렌더링 캐시
    texture: Option<egui::TextureHandle>,
    render_dirty: bool,
    last_render_zoom: f32,
    last_render_ppp: f32,

    // 주석
    store: AnnotationStore,
    history: History,

    // 도구
    tool: ToolType,
    pen_color: Color32,
    pen_width: f32,
    hi_color: Color32,
    hi_width: f32,
    eraser_radius: f32,

    // 입력 상태
    active_stroke: Option<ActiveStroke>,
    pan_last: Option<Pos2>,
    middle_pan_last: Option<Pos2>,

    // 상태/메시지
    file_name: String,
    status: Option<String>,

    // 폴백 다이얼로그
    modal: Option<ModalState>,
}

impl FreeDfApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let dark = matches!(cc.egui_ctx.theme(), egui::Theme::Dark);
        let pen_color = if dark {
            Color32::WHITE
        } else {
            Color32::from_rgb(20, 20, 20)
        };
        let hi_color = if dark {
            Color32::from_rgba_unmultiplied(255, 220, 60, 110)
        } else {
            Color32::from_rgba_unmultiplied(255, 235, 59, 90)
        };
        Self {
            document: None,
            file_path: None,
            current_page: 0,
            page_size_pts: [595.0, 842.0],
            view: ViewTransform::default(),
            last_canvas: [1280.0, 600.0],
            pending_fit: None,
            texture: None,
            render_dirty: true,
            last_render_zoom: 0.0,
            last_render_ppp: 0.0,
            store: AnnotationStore::new(),
            history: History::new(256),
            tool: ToolType::Pen,
            pen_color,
            pen_width: 2.5,
            hi_color,
            hi_width: 16.0,
            eraser_radius: 16.0,
            active_stroke: None,
            pan_last: None,
            middle_pan_last: None,
            file_name: String::new(),
            status: None,
            modal: None,
        }
    }

    // ---------- 문서 열기 / 페이지 ----------

    fn open_file_dialog(&mut self) {
        #[cfg(target_os = "windows")]
        {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("PDF 문서", &["pdf"])
                .pick_file()
            {
                self.open_pdf(&path);
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            self.modal = Some(ModalState::new(
                "PDF 열기",
                "PDF 파일 경로를 입력하세요 (예: C:/Users/me/doc.pdf)",
                ModalAction::OpenPdf,
            ));
        }
    }

    fn open_pdf(&mut self, path: &Path) {
        match DocumentView::open(path) {
            Ok(doc) => {
                self.current_page = 0;
                self.page_size_pts = doc.page_size_pts(0);
                self.file_name = doc.file_name.clone();
                self.document = Some(doc);
                self.store = AnnotationStore::new();
                self.history = History::new(256);
                self.active_stroke = None;
                self.render_dirty = true;
                self.pending_fit = Some(FitMode::Width);
                self.status = None;
                self.file_path = Some(path.to_path_buf());

                // 옆에 저장된 메모 파일이 있으면 자동으로 불러오기
                let ann_path = annotation_path_for(path);
                if ann_path.exists() {
                    if let Ok(text) = std::fs::read_to_string(&ann_path) {
                        if let Ok(store) = AnnotationStore::from_json(&text) {
                            self.store = store;
                            self.status = Some(format!(
                                "메모를 자동으로 불러왔습니다: {}",
                                ann_path.file_name().unwrap_or_default().to_string_lossy()
                            ));
                        }
                    }
                }
            }
            Err(e) => self.status = Some(e),
        }
    }

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
        self.active_stroke = None;
        self.pan_last = None;
        self.middle_pan_last = None;
        if let Some(doc) = &self.document {
            self.page_size_pts = doc.page_size_pts(self.current_page);
        }
        self.render_dirty = true;
        self.pending_fit = Some(FitMode::Width);
    }

    // ---------- 줌 / 핏 ----------

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

    /// 캔버스 크기를 알고 있을 때 pending fit을 적용합니다.
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
        self.view.center_page(self.page_size_pts, canvas, TOP_MARGIN);
        self.render_dirty = true;
    }

    // ---------- 실행취소 / 다시실행 / 지우기 ----------

    fn undo(&mut self) {
        if let Some(edit) = self.history.undo() {
            self.store.apply_edit(&edit);
        }
    }

    fn redo(&mut self) {
        if let Some(edit) = self.history.redo() {
            self.store.apply_edit(&edit);
        }
    }

    fn clear_page(&mut self) {
        let removed = self.store.clear_page(self.current_page);
        if !removed.is_empty() {
            self.history.push(Edit::RemoveStrokes {
                page: self.current_page,
                strokes: removed,
            });
        }
    }

    // ---------- 드로잉 ----------

    fn current_drawing_style(&self) -> ([u8; 4], f32) {
        match self.tool {
            ToolType::Pen => (self.pen_color.to_array(), self.pen_width),
            ToolType::Highlighter => (self.hi_color.to_array(), self.hi_width),
            _ => ([0, 0, 0, 255], 2.0),
        }
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
                    strokes: vec![stroke],
                });
            }
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

    // ---------- 텍스처 렌더링 ----------

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
            Err(e) => self.status = Some(format!("렌더 오류: {e}")),
        }
    }

    // ---------- 파일 저장 / 불러오기 / 내보내기 ----------

    fn save_annotations(&mut self) {
        if self.document.is_none() {
            self.status = Some("먼저 PDF를 열어 주세요.".to_string());
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
                .add_filter("FreeDF 메모", &["json"])
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
            self.modal = Some(ModalState::new(
                "메모 저장",
                "저장할 JSON 파일 경로를 입력하세요",
                ModalAction::SaveAnnotations,
            ));
        }
    }

    fn do_save_annotations(&mut self, path: &Path) {
        match std::fs::write(path, self.store.to_json()) {
            Ok(()) => self.status = Some(format!("메모 저장 완료: {}", path.display())),
            Err(e) => self.status = Some(format!("메모 저장 실패: {e}")),
        }
    }

    fn load_annotations(&mut self) {
        #[cfg(target_os = "windows")]
        {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("FreeDF 메모", &["json"])
                .pick_file()
            {
                self.do_load_annotations(&path);
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            self.modal = Some(ModalState::new(
                "메모 불러오기",
                "불러올 JSON 파일 경로를 입력하세요",
                ModalAction::LoadAnnotations,
            ));
        }
    }

    fn do_load_annotations(&mut self, path: &Path) {
        match std::fs::read_to_string(path) {
            Ok(text) => match AnnotationStore::from_json(&text) {
                Ok(store) => {
                    self.store = store;
                    self.status = Some(format!("메모 불러오기 완료: {}", path.display()));
                }
                Err(e) => self.status = Some(format!("메모 파일이 올바르지 않습니다: {e}")),
            },
            Err(e) => self.status = Some(format!("메모 불러오기 실패: {e}")),
        }
    }

    fn export_png(&mut self) {
        if self.document.is_none() {
            self.status = Some("먼저 PDF를 열어 주세요.".to_string());
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
                .add_filter("PNG 이미지", &["png"])
                .set_file_name(&default_name)
                .save_file()
            {
                self.do_export_png(&path);
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            self.modal = Some(ModalState::new(
                "PNG 내보내기",
                "저장할 PNG 파일 경로를 입력하세요",
                ModalAction::ExportPng,
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
                        self.status = Some("렌더링 결과를 이미지로 변환하지 못했습니다.".to_string());
                        return;
                    }
                };
                let scale = rendered.width as f32 / page_pts[0];
                let strokes: Vec<_> = self.store.strokes_on(self.current_page).to_vec();
                draw_strokes_on_image(&mut img, &strokes, scale);
                match img.save(path) {
                    Ok(()) => self.status = Some(format!("내보내기 완료: {}", path.display())),
                    Err(e) => self.status = Some(format!("PNG 저장 실패: {e}")),
                }
            }
            Err(e) => self.status = Some(format!("내보내기 실패: {e}")),
        }
    }

    fn run_modal_action(&mut self, action: ModalAction, path: String) {
        let path = PathBuf::from(path.trim());
        match action {
            ModalAction::OpenPdf => self.open_pdf(&path),
            ModalAction::LoadAnnotations => self.do_load_annotations(&path),
            ModalAction::SaveAnnotations => self.do_save_annotations(&path),
            ModalAction::ExportPng => self.do_export_png(&path),
        }
    }

    // ---------- UI: 툴바 ----------

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                if ui.button("열기").on_hover_text("Ctrl+O").clicked() {
                    self.open_file_dialog();
                }
                ui.separator();

                let page_count = self.document.as_ref().map(|d| d.page_count()).unwrap_or(0);
                let can_prev = self.current_page > 0;
                let can_next = self.current_page + 1 < page_count;
                if ui
                    .add_enabled(can_prev, egui::Button::new("◀"))
                    .on_hover_text("이전 페이지")
                    .clicked()
                {
                    self.prev_page();
                }
                let mut page_num = self.current_page + 1;
                if ui
                    .add(egui::DragValue::new(&mut page_num).range(1..=page_count.max(1)))
                    .on_hover_text("페이지 번호")
                    .changed()
                {
                    self.goto_page(page_num - 1);
                }
                if ui
                    .add_enabled(can_next, egui::Button::new("▶"))
                    .on_hover_text("다음 페이지")
                    .clicked()
                {
                    self.next_page();
                }
                ui.label(format!("/ {}", page_count.max(1)));
                ui.separator();

                if ui.button("−").on_hover_text("축소").clicked() {
                    self.zoom_by(1.0 / 1.25);
                }
                ui.label(format!(
                    "{:.0}%",
                    self.view.zoom / ZOOM_100_PERCENT * 100.0
                ));
                if ui.button("+").on_hover_text("확대").clicked() {
                    self.zoom_by(1.25);
                }
                if ui.button("폭 맞춤").clicked() {
                    self.fit_width();
                }
                if ui.button("페이지 맞춤").clicked() {
                    self.fit_page();
                }
                ui.separator();

                for tool in [
                    ToolType::Pen,
                    ToolType::Highlighter,
                    ToolType::Eraser,
                    ToolType::Pan,
                ] {
                    if ui.selectable_label(self.tool == tool, tool.label()).clicked() {
                        self.tool = tool;
                    }
                }

                match self.tool {
                    ToolType::Pen => {
                        ui.color_edit_button_srgba(&mut self.pen_color);
                        ui.add(
                            egui::Slider::new(&mut self.pen_width, 0.5..=12.0).text("두께"),
                        );
                    }
                    ToolType::Highlighter => {
                        ui.color_edit_button_srgba(&mut self.hi_color);
                        ui.add(
                            egui::Slider::new(&mut self.hi_width, 4.0..=40.0).text("두께"),
                        );
                    }
                    ToolType::Eraser => {
                        ui.add(
                            egui::Slider::new(&mut self.eraser_radius, 4.0..=60.0).text("반경"),
                        );
                    }
                    ToolType::Pan => {}
                }
                ui.separator();

                if ui
                    .add_enabled(self.history.can_undo(), egui::Button::new("실행취소"))
                    .on_hover_text("Ctrl+Z")
                    .clicked()
                {
                    self.undo();
                }
                if ui
                    .add_enabled(self.history.can_redo(), egui::Button::new("다시실행"))
                    .on_hover_text("Ctrl+Y")
                    .clicked()
                {
                    self.redo();
                }
                if ui.button("페이지 지우기").clicked() {
                    self.clear_page();
                }
                ui.separator();

                if ui.button("메모 저장").on_hover_text("Ctrl+S").clicked() {
                    self.save_annotations();
                }
                if ui.button("메모 불러오기").clicked() {
                    self.load_annotations();
                }
                if ui.button("PNG 내보내기").on_hover_text("Ctrl+E").clicked() {
                    self.export_png();
                }
            });
            ui.add_space(6.0);
        });
    }

    // ---------- UI: 하단 상태 바 ----------

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let page_count = self.document.as_ref().map(|d| d.page_count()).unwrap_or(0);
        let zoom_pct = self.view.zoom / ZOOM_100_PERCENT * 100.0;
        let stroke_count = self.store.total_stroke_count();
        let file_name = self.file_name.clone();
        let status = self.status.clone();

        egui::Panel::bottom("status").show(ui, |ui| {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                if file_name.is_empty() {
                    ui.label(egui::RichText::new("문서 없음").weak());
                } else {
                    ui.label(egui::RichText::new(&file_name).strong());
                }
                ui.separator();
                ui.label(format!(
                    "{}/{} 페이지",
                    (self.current_page + 1).min(page_count.max(1)),
                    page_count.max(1)
                ));
                ui.separator();
                ui.label(format!("줌 {zoom_pct:.0}%"));
                ui.separator();
                ui.label(format!("스트로크 {stroke_count}개"));
                if let Some(s) = &status {
                    ui.separator();
                    ui.label(egui::RichText::new(s).color(Color32::from_rgb(230, 120, 60)));
                }
            });
            ui.add_space(2.0);
        });
    }

    // ---------- UI: 캔버스 ----------

    fn canvas(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let canvas = response.rect;
        let origin = canvas.min;
        let canvas_size = [canvas.width(), canvas.height()];
        self.last_canvas = canvas_size;

        // 배경
        painter.rect_filled(canvas, egui::CornerRadius::ZERO, ui.visuals().extreme_bg_color);

        if self.document.is_none() {
            ui.painter_at(canvas).text(
                canvas.center(),
                egui::Align2::CENTER_CENTER,
                "PDF를 열어 메모와 필기를 시작하세요 (Ctrl+O)",
                egui::TextStyle::Heading.resolve(ui.style()),
                ui.visuals().weak_text_color(),
            );
            return;
        }

        // pending fit 적용
        self.apply_pending_fit(canvas_size);
        // 렌더 캐시 갱신 (입력 처리 전에 줌 반영)
        self.ensure_texture(&ctx);

        // ---------- 입력 처리 ----------
        self.handle_canvas_input(&ctx, &response, origin, canvas_size);

        // ---------- 그리기 ----------
        let page_view = self.view.page_size_to_view(self.page_size_pts[0], self.page_size_pts[1]);
        let page_rect = Rect::from_min_size(
            origin + Vec2::new(self.view.pan_x, self.view.pan_y),
            Vec2::new(page_view[0], page_view[1]),
        );

        // 페이지 그림자
        painter.rect_filled(
            page_rect.expand(6.0),
            egui::CornerRadius::same(4),
            Color32::from_black_alpha(70),
        );
        // 페이지 이미지
        if let Some(tex) = &self.texture {
            painter.image(
                tex.id(),
                page_rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        // 페이지 테두리
        painter.rect_stroke(
            page_rect,
            egui::CornerRadius::same(2),
            Stroke::new(1.0, Color32::from_gray(120)),
            egui::StrokeKind::Inside,
        );

        // 주석 스트로크
        let strokes: Vec<_> = self.store.strokes_on(self.current_page).to_vec();
        for stroke in &strokes {
            self.paint_stroke(&painter, stroke, origin);
        }
        if let Some(active) = &self.active_stroke {
            self.paint_active(&painter, active, origin);
        }

        // 지우개 커서
        if self.tool == ToolType::Eraser {
            if let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) {
                if canvas.contains(pos) {
                    painter.circle_stroke(
                        pos,
                        self.eraser_radius,
                        Stroke::new(1.5, Color32::from_rgba_unmultiplied(255, 90, 90, 200)),
                    );
                }
            }
        }

        // 줌 힌트
        if self.document.is_some() && self.view.zoom >= 4.0 {
            painter.text(
                canvas.left_top() + Vec2::new(10.0, 10.0),
                egui::Align2::LEFT_TOP,
                "Ctrl+휠: 줌 / 휠: 스크롤·페이지 / 중간버튼: 이동",
                egui::TextStyle::Small.resolve(ui.style()),
                ui.visuals().weak_text_color(),
            );
        }
    }

    // ---------- 입력 처리 ----------

    fn handle_canvas_input(
        &mut self,
        ctx: &egui::Context,
        response: &egui::Response,
        origin: Pos2,
        canvas_size: [f32; 2],
    ) {
        let pointer_abs = response.interact_pointer_pos();

        // 줌(핀치 / Ctrl+휠)
        let (zoom_delta, scroll_y) = ctx.input(|i| (i.zoom_delta(), i.smooth_scroll_delta.y));
        let ctrl_down = ctx.input(|i| i.modifiers.ctrl);
        if (zoom_delta - 1.0).abs() > 1e-4 && response.hovered() {
            if let Some(abs) = pointer_abs {
                let anchor = [abs.x - origin.x, abs.y - origin.y];
                self.view.zoom_at(anchor, zoom_delta, MIN_ZOOM, MAX_ZOOM);
                self.render_dirty = true;
            }
        } else if scroll_y.abs() > 0.0 && response.hovered() && !ctrl_down {
            let page_h_px = self.page_size_pts[1] * self.view.zoom;
            if page_h_px <= canvas_size[1] {
                // 페이지가 통째로 보이면 페이지 넘기기
                if scroll_y < 0.0 {
                    self.next_page();
                } else {
                    self.prev_page();
                }
            } else {
                self.view.pan_by(0.0, -scroll_y);
            }
        }

        // 중간 버튼으로 항상 팬
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
                        let pressure = 0.5; // egui 0.36에는 펜 압력 API가 없음
                        if self.active_stroke.is_none() {
                            let (color, width) = self.current_drawing_style();
                            self.active_stroke = Some(ActiveStroke {
                                tool: self.tool,
                                color,
                                width,
                                points: Vec::new(),
                            });
                        }
                        let st = self.active_stroke.as_mut().expect("방금 생성");
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
                        let pressure = 0.5;
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
                                strokes: removed,
                            });
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

    // ---------- 스트로크 그리기 ----------

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
            let r = (stroke.width * zoom * 0.5).max(0.75);
            painter.circle_filled(center, r, color);
            return;
        }
        for w in pts.windows(2) {
            let a = self.view.page_to_view([w[0].x, w[0].y]);
            let b = self.view.page_to_view([w[1].x, w[1].y]);
            let pressure = (w[0].pressure + w[1].pressure) * 0.5;
            let wpx = stroke.width * zoom * (0.45 + 0.55 * pressure);
            let pa = origin + Vec2::new(a[0], a[1]);
            let pb = origin + Vec2::new(b[0], b[1]);
            painter.line_segment([pa, pb], Stroke::new(wpx.max(0.5), color));
        }
    }

    // ---------- 단축키 ----------

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let ctrl = ctx.input(|i| i.modifiers.command);
        let shift = ctx.input(|i| i.modifiers.shift);

        if ctrl && ctx.input(|i| i.key_pressed(egui::Key::O)) {
            self.open_file_dialog();
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
        if ctx.input(|i| i.key_pressed(egui::Key::PageDown) || i.key_pressed(egui::Key::ArrowRight)) {
            self.next_page();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::PageUp) || i.key_pressed(egui::Key::ArrowLeft)) {
            self.prev_page();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals)) {
            self.zoom_by(1.25);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Minus)) {
            self.zoom_by(1.0 / 1.25);
        }
        // 도구 단축키
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

    // ---------- 폴백 다이얼로그 ----------

    fn fallback_dialog(&mut self, ctx: &egui::Context) {
        let Some(modal) = self.modal.clone() else {
            return;
        };
        let mut path = modal.path.clone();
        let mut ok = false;
        let mut cancel = false;

        egui::Window::new(modal.title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(&modal.hint);
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut path)
                        .hint_text("경로 입력")
                        .desired_width(360.0),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ok = ui.button("확인").clicked();
                    cancel = ui.button("취소").clicked();
                });
                if resp.lost_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                    ok = true;
                }
            });

        // 입력 중 상태 유지
        if let Some(m) = &mut self.modal {
            m.path = path.clone();
        }

        if ok && !path.trim().is_empty() {
            let action = self.modal.as_ref().map(|m| m.action);
            self.modal = None;
            if let Some(action) = action {
                self.run_modal_action(action, path);
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

        egui::CentralPanel::default().show(ui, |ui| {
            self.canvas(ui);
        });

        self.fallback_dialog(&ctx);
    }
}

/// PDF 파일 옆에 놓이는 메모 파일 경로 (`doc.freedf.json`).
fn annotation_path_for(pdf_path: &Path) -> PathBuf {
    let mut os = pdf_path.as_os_str().to_os_string();
    os.push(".freedf.json");
    PathBuf::from(os)
}
