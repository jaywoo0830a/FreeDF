//! PDFium 동적 라이브러리 로딩과 페이지 렌더링.
//!
//! Windows에서는 `pdfium.dll`, Linux에서는 `libpdfium.so`를
//! 실행 파일 옆에 두면 자동으로 찾습니다.

use pdfium_render::prelude::*;
use std::path::Path;

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
        "PDFium 라이브러리를 찾지 못했습니다.\n\
         (Windows: `pdfium.dll` / Linux: `libpdfium.so`) 파일을 프로그램 실행 파일 옆에 둔 뒤 다시 실행해 주세요."
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
            .map_err(|e| format!("PDF를 열 수 없습니다: {e}"))?;

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
            .map_err(|e| format!("페이지를 읽을 수 없습니다: {e}"))?;

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
            .map_err(|e| format!("페이지 렌더링 실패: {e}"))?;

        let width = bitmap.width() as usize;
        let height = bitmap.height() as usize;
        let rgba = bitmap.as_rgba_bytes();

        if width == 0 || height == 0 || rgba.len() != width * height * 4 {
            return Err("렌더링 결과가 비정상입니다.".to_string());
        }

        Ok(RenderedPage {
            width,
            height,
            rgba,
        })
    }
}
