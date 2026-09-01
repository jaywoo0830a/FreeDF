//! PDFium 동적 라이브러리 로딩과 페이지 렌더링.
//!
//! Windows에서는 `pdfium.dll`, Linux에서는 `libpdfium.so`를
//! 실행 파일 옆에 두면 자동으로 찾습니다.

use pdfium_render::prelude::*;
use std::path::Path;

use freedf_core::outline::OutlineNode;
use freedf_core::search::TextRun;

/// PDFium 라이브러리를 로드합니다.
pub fn load_pdfium() -> Result<Pdfium, String> {
    // 1) 시스템에 등록된 라이브러리 시도
    if let Ok(bindings) = Pdfium::bind_to_system_library() {
        return Ok(Pdfium::new(bindings));
    }
    // 2) 플랫폼 기본 이름을 현재 디렉터리/검색 경로에서 시도
    let names: &[&str] = if cfg!(target_os = "windows") {
        &["pdfium.dll", "pdfium"]
    } else if cfg!(target_os = "macos") {
        &["libpdfium.dylib", "libpdfium"]
    } else {
        &["libpdfium.so", "libpdfium"]
    };
    for name in names {
        if let Ok(bindings) = Pdfium::bind_to_library(name) {
            return Ok(Pdfium::new(bindings));
        }
    }
    Err(format!(
        "PDFium library not found.\n\
         Put `pdfium.dll` (Windows) or `libpdfium.so` (Linux) next to the program\n\
         executable and restart. On Windows, run: scripts\\install-pdfium.ps1"
    ))
}

/// 열려 있는 PDF 문서. 렌더링 API에 필요한 문서 핸들과 페이지 크기를 보관합니다.
pub struct DocumentView {
    // 필드 선언 순서 = 드롭 순서. document가 pdfium보다 먼저 해제됩니다.
    document: PdfDocument<'static>,
    // PDFium 라이브러리 수명을 유지하는 소유자. 직접 읽지 않지만
    // document가 유효한 동안 반드시 살아 있어야 합니다.
    _pdfium: Box<Pdfium>,
    pub file_name: String,
    /// 페이지별 크기(포인트)
    pub page_sizes_pts: Vec<[f32; 2]>,
}

/// 렌더링 결과 (RGBA, top-down).
pub struct RenderedPage {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

impl DocumentView {
    /// 파일에서 문서를 엽니다.
    pub fn open(path: &Path) -> Result<Self, String> {
        let pdfium = Box::new(load_pdfium()?);
        let document = pdfium
            .load_pdf_from_file(path, None)
            .map_err(|e| format!("Could not open PDF: {e}"))?;

        // 안전성: `document`는 `pdfium`을 가리키는 수명 표시만 가집니다.
        // 둘은 같은 구조체에 함께 살며, pdfium은 힙(Box)에 있어 이동에 안전하고,
        // 필드 선언 순서상 document가 먼저 드롭되므로 절대 사용 후 파괴되지 않습니다.
        let document: PdfDocument<'static> = unsafe { std::mem::transmute(document) };

        let pages = document.pages();
        let count = pages.len() as usize;
        let mut sizes = Vec::with_capacity(count);
        for i in 0..count {
            let index = i as i32; // PdfPageIndex
            let size = pages
                .page_size(index)
                .map(|r| [r.width().value, r.height().value])
                .unwrap_or([595.0, 842.0]); // 기본 A4
            sizes.push(size);
        }
        let file_name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());

        Ok(Self {
            document,
            _pdfium: pdfium,
            file_name,
            page_sizes_pts: sizes,
        })
    }

    pub fn page_count(&self) -> usize {
        self.page_sizes_pts.len()
    }

    pub fn page_size_pts(&self, index: usize) -> [f32; 2] {
        self.page_sizes_pts
            .get(index)
            .copied()
            .unwrap_or([595.0, 842.0])
    }

    /// 페이지를 `target_width`(물리 픽셀)로 렌더링합니다.
    /// 종횡비는 유지되고 `max_dimension`을 넘지 않습니다.
    pub fn render_page(
        &self,
        index: usize,
        target_width: f32,
        max_dimension: f32,
    ) -> Result<RenderedPage, String> {
        let page = self
            .document
            .pages()
            .get(index as i32)
            .map_err(|e| format!("Could not read page: {e}"))?;

        let w = (target_width.round().clamp(1.0, 65_000.0)) as Pixels;
        let m = (max_dimension.round().clamp(1.0, 65_000.0)) as Pixels;

        let config = PdfRenderConfig::new()
            .set_target_width(w)
            .set_maximum_width(m)
            .set_maximum_height(m)
            .render_annotations(true)
            .use_lcd_text_rendering(true);

        let bitmap = page
            .render_with_config(&config)
            .map_err(|e| format!("Page render failed: {e}"))?;

        let width = bitmap.width() as usize;
        let height = bitmap.height() as usize;
        let rgba = bitmap.as_rgba_bytes();

        if width == 0 || height == 0 || rgba.len() != width * height * 4 {
            return Err("Render result is invalid.".to_string());
        }

        Ok(RenderedPage {
            width,
            height,
            rgba,
        })
    }

    /// 문서의 책갈피(목차) 트리를 `OutlineNode` 목록으로 변환합니다.
    pub fn outline(&self) -> Vec<OutlineNode> {
        fn walk(bookmark: &PdfBookmark) -> Vec<OutlineNode> {
            let mut nodes = Vec::new();
            for child in bookmark.iter_direct_children() {
                let title = child.title().unwrap_or_default();
                let page_index = child
                    .destination()
                    .and_then(|d| d.page_index().ok())
                    .filter(|i| *i >= 0)
                    .map(|i| i as usize);
                nodes.push(OutlineNode::new(title, page_index, walk(&child)));
            }
            nodes
        }
        let Some(root) = self.document.bookmarks().root() else {
            return Vec::new();
        };
        walk(&root)
    }

    /// 페이지의 텍스트 세그먼트를 검색용 `TextRun` 목록으로 변환합니다.
    pub fn page_text_runs(&self, index: usize) -> Result<Vec<TextRun>, String> {
        let page = self
            .document
            .pages()
            .get(index as i32)
            .map_err(|e| format!("Could not read page: {e}"))?;
        let text = page.text().map_err(|e| format!("Text extraction failed: {e}"))?;
        let mut runs = Vec::new();
        for seg in text.segments().iter() {
            let txt = seg.text();
            let b = seg.bounds();
            let rect = [b.left().value, b.top().value, b.right().value, b.bottom().value];
            // pdfium-render 0.9.3은 문자 단위 좌표를 노출하지 않으므로 빈 벡터.
            // core의 find_matches가 run.rect 비율 폴백으로 처리합니다.
            runs.push(TextRun::new(txt, rect, Vec::new()));
        }
        Ok(runs)
    }

    /// 문서 끝에 빈 페이지를 추가합니다.
    pub fn add_page(&mut self, size_pts: [f32; 2]) -> Result<(), String> {
        let paper =
            PdfPagePaperSize::new_custom(PdfPoints::new(size_pts[0]), PdfPoints::new(size_pts[1]));
        self.document
            .pages_mut()
            .create_page_at_end(paper)
            .map_err(|e| format!("Could not add page: {e}"))?;
        self.refresh_sizes();
        Ok(())
    }

    /// 페이지를 삭제합니다. 마지막 한 장은 삭제할 수 없습니다.
    pub fn delete_page(&mut self, index: usize) -> Result<(), String> {
        if self.page_count() <= 1 {
            return Err("Cannot delete the last remaining page.".to_string());
        }
        if index >= self.page_count() {
            return Err("Page index out of range.".to_string());
        }
        self.document
            .pages_mut()
            .get(index as i32)
            .map_err(|e| format!("Could not read page: {e}"))?
            .delete()
            .map_err(|e| format!("Could not delete page: {e}"))?;
        self.refresh_sizes();
        Ok(())
    }

    /// 현재 문서(주석 포함)를 파일로 저장합니다.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.document
            .save_to_file(path)
            .map_err(|e| format!("Save failed: {e}"))
    }

    /// 빈 PDF 문서를 생성해 저장합니다 (기본 A4).
    pub fn create_blank_pdf(path: &Path, size_pts: [f32; 2]) -> Result<(), String> {
        let pdfium = Box::new(load_pdfium()?);
        let document = pdfium
            .create_new_pdf()
            .map_err(|e| format!("Could not create new PDF: {e}"))?;
        // open()과 동일한 수명 확장 패턴.
        let mut document: PdfDocument<'static> = unsafe { std::mem::transmute(document) };
        let paper =
            PdfPagePaperSize::new_custom(PdfPoints::new(size_pts[0]), PdfPoints::new(size_pts[1]));
        document
            .pages_mut()
            .create_page_at_end(paper)
            .map_err(|e| format!("Could not create page: {e}"))?;
        document
            .save_to_file(path)
            .map_err(|e| format!("Save failed: {e}"))
    }

    /// 페이지 크기 캐시를 문서 상태에 맞게 다시 계산합니다.
    fn refresh_sizes(&mut self) {
        let count = self.document.pages().len() as usize;
        let mut sizes = Vec::with_capacity(count);
        for i in 0..count {
            let size = self
                .document
                .pages()
                .page_size(i as i32)
                .map(|r| [r.width().value, r.height().value])
                .unwrap_or([595.0, 842.0]); // 기본 A4
            sizes.push(size);
        }
        self.page_sizes_pts = sizes;
    }
}
