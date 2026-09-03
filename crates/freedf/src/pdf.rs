//! PDFium 동적 라이브러리 로딩과 페이지 렌더링.
//!
//! Windows에서는 `pdfium.dll`, Linux에서는 `libpdfium.so`를
//! 실행 파일 옆에 두면 자동으로 찾습니다.

use pdfium_render::prelude::*;
use std::path::{Path, PathBuf};

use freedf_core::outline::OutlineNode;
use freedf_core::search::TextRun;
use freedf_core::text::{content_rect_to_display as core_content_rect_to_display, PageRotation, TextChar};

/// 콘텐츠 공간 `PdfRect` → 표시 공간 `[x0,y0,x1,y1]` (core 변환의 pdfium 래퍼).
fn content_rect_to_display(r: PdfRect, w: f32, h: f32, rot: PageRotation) -> [f32; 4] {
    core_content_rect_to_display(
        [r.left().value, r.bottom().value, r.right().value, r.top().value],
        w,
        h,
        rot,
    )
}

/// 플랫폼별 PDFium 라이브러리 파일 이름 후보.
fn pdfium_names() -> &'static [&'static str] {
    if cfg!(target_os = "windows") {
        &["pdfium.dll", "pdfium"]
    } else if cfg!(target_os = "macos") {
        &["libpdfium.dylib", "libpdfium"]
    } else {
        &["libpdfium.so", "libpdfium"]
    }
}

/// PDFium 라이브러리를 찾을 후보 디렉터리 (실행 파일 폴더, 현재 폴더, 앱 데이터 폴더).
fn library_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            dirs.push(dir.to_path_buf());
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(cwd);
    }
    // 앱 데이터 폴더 (%LOCALAPPDATA%/FreeDF 또는 ~/.local/share/freedf)
    let data = app_data_dir();
    if !data.as_os_str().is_empty() {
        dirs.push(data);
    }
    dirs
}

/// Per-user app data directory (mirrors main.rs).
fn app_data_dir() -> PathBuf {
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(local).join("FreeDF");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".local").join("share").join("freedf");
    }
    PathBuf::new()
}

/// PDFium 라이브러리를 로드합니다.
///
/// 검색 순서: ① 시스템 등록 라이브러리 → ② 실행 파일/현재 폴더/앱 데이터 폴더의
/// 명시적 경로 → ③ OS 기본 검색 경로. 실패 시 실제 검색 경로와 로드 오류를 표시합니다.
pub fn load_pdfium() -> Result<Pdfium, String> {
    // 1) 시스템에 등록된 라이브러리 시도
    if let Ok(bindings) = Pdfium::bind_to_system_library() {
        return Ok(Pdfium::new(bindings));
    }

    // 검색한 경로와, 파일은 있으나 로드에 실패한 오류를 수집합니다.
    let mut tried: Vec<String> = Vec::new();
    let mut load_errors: Vec<String> = Vec::new();

    // 2) 실행 파일/현재 폴더/앱 데이터 폴더에서 명시적 경로로 시도
    for dir in library_search_dirs() {
        for name in pdfium_names() {
            let path = dir.join(name);
            tried.push(path.display().to_string());
            if path.exists() {
                match Pdfium::bind_to_library(&path) {
                    Ok(bindings) => return Ok(Pdfium::new(bindings)),
                    Err(e) => load_errors.push(format!("{}: {e}", path.display())),
                }
            }
        }
    }

    // 3) 플랫폼 기본 이름을 OS 검색 경로에서 시도
    for name in pdfium_names() {
        if let Ok(bindings) = Pdfium::bind_to_library(name) {
            return Ok(Pdfium::new(bindings));
        }
    }

    let mut msg = String::from("PDFium library not found or could not be loaded.\n\n");
    msg.push_str("Searched:\n");
    for p in &tried {
        msg.push_str(&format!("  • {p}\n"));
    }
    if !load_errors.is_empty() {
        msg.push_str("\nFound file(s) but loading failed:\n");
        for e in &load_errors {
            msg.push_str(&format!("  • {e}\n"));
        }
        msg.push_str("\nThis usually means a missing Microsoft Visual C++ Redistributable,\n");
        msg.push_str("or an architecture (x86 vs x64) mismatch with the executable.\n");
    }
    msg.push_str(
        "\nPut `pdfium.dll` (Windows) or `libpdfium.so` (Linux) next to the program\n\
         executable (or in the app data folder) and restart.\n\
         On Windows, run: scripts\\install-pdfium.ps1",
    );
    Err(msg)
}

/// 열려 있는 PDF 문서. 렌더링 API에 필요한 문서 핸들과 페이지 크기를 보관합니다.
///
/// pdfium-render는 프로세스당 라이브러리를 **한 번만** 초기화할 수 있으므로,
/// 이 구조체는 PDFium 인스턴스를 소유하지 않고 호출부가 넘겨준 인스턴스를
/// 사용합니다. 호출부(앱)는 이 문서가 살아 있는 동안 그 PDFium을 계속 유지해야
/// 합니다 (문서의 `'static` 수명 변환의 안전 불변식).
pub struct DocumentView {
    document: PdfDocument<'static>,
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
    /// 이미 초기화된 `pdfium` 인스턴스로 파일에서 문서를 엽니다.
    /// (pdfium은 프로세스당 한 번만 초기화되므로 재로딩하지 않습니다.)
    pub fn open(pdfium: &Pdfium, path: &Path) -> Result<Self, String> {
        let document = pdfium
            .load_pdf_from_file(path, None)
            .map_err(|e| format!("Could not open PDF: {e}"))?;

        // 안전성: `document`는 `pdfium`을 가리키는 수명 표시만 가집니다.
        // 호출부가 pdfium을 문서보다 오래 유지하므로, 'static 변환 후에도
        // document가 해제된 pdfium을 참조하지 않습니다.
        let document: PdfDocument<'static> = unsafe { std::mem::transmute(document) };

        let count = document.pages().len() as usize;
        let mut sizes = Vec::with_capacity(count);
        for i in 0..count {
            sizes.push(display_size_of(&document, i).unwrap_or([595.0, 842.0]));
        }
        let file_name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());

        Ok(Self {
            document,
            file_name,
            page_sizes_pts: sizes,
        })
    }

    /// PostgreSQL(BYTEA)에서 내려온 바이트로 문서를 엽니다.
    ///
    /// pdfium-render의 `load_pdf_from_byte_slice`는 바이트 슬라이스를 문서보다
    /// 오래 유지해야 하므로(수명 결합), 임시 파일에 쓴 뒤 `load_pdf_from_file`로
    /// 로드하고 즉시 정리합니다 (pdfium은 로드 시 파일 전체를 메모리로 읽음).
    pub fn open_bytes(pdfium: &Pdfium, bytes: &[u8], name: &str) -> Result<Self, String> {
        let temp = std::env::temp_dir().join(format!(
            "freedf-{}-{:x}.pdf",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::write(&temp, bytes).map_err(|e| format!("Could not stage PDF bytes: {e}"))?;
        let result = Self::open(pdfium, &temp);
        let _ = std::fs::remove_file(&temp);
        result.map(|mut doc| {
            doc.file_name = name.to_string();
            doc
        })
    }

    /// 이미 로드된 PDFium 인스턴스로 빈 문서(페이지 1장)를 **메모리에** 생성합니다.
    pub fn create_blank_view(
        pdfium: &Pdfium,
        size_pts: [f32; 2],
        name: &str,
    ) -> Result<Self, String> {
        let document = pdfium
            .create_new_pdf()
            .map_err(|e| format!("Could not create new PDF: {e}"))?;
        // open()과 동일한 수명 확장 패턴 (pdfium은 호출부에서 수명이 보장됨).
        let mut document: PdfDocument<'static> = unsafe { std::mem::transmute(document) };
        let paper =
            PdfPagePaperSize::new_custom(PdfPoints::new(size_pts[0]), PdfPoints::new(size_pts[1]));
        document
            .pages_mut()
            .create_page_at_end(paper)
            .map_err(|e| format!("Could not create page: {e}"))?;
        Ok(Self {
            document,
            file_name: name.to_string(),
            page_sizes_pts: vec![size_pts],
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

        let [w_pts, h_pts] = self.page_size_pts(index);
        let w = (target_width.round().clamp(1.0, 65_000.0)) as Pixels;
        // 표시 종횡비(너비/높이)에 맞는 높이를 명시해야 합니다. target_width만
        // 주면 pdfium-render가 어긋난 비트맵을 만들어 회전 페이지가 찌그러집니다.
        let h = ((w as f32 * h_pts / w_pts).round().clamp(1.0, 65_000.0)) as Pixels;
        let m = (max_dimension.round().clamp(1.0, 65_000.0)) as Pixels;
        // ── 회전 렌더링 (중요) ── pdfium은 페이지의 내장 /Rotate를 **렌더 시
        // 자동 적용**합니다 (CPDF_Page::UpdateDimensions가 page_matrix_에 회전을
        // 굽고 GetDisplayMatrix가 항상 곱함). 따라서 config에 rotate 플래그를
        // **다시 넘기면 이중 회전(90+90=180°)이 됩니다** — 넘기지 말 것.
        // FPDF_RenderPageBitmap의 rotate 인자는 0으로 유지해야 합니다.

        let config = PdfRenderConfig::new()
            .set_target_width(w)
            .set_target_height(h)
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
    ///
    /// pdfium-render의 `PdfBookmarks::root()`는 **합성 루트가 아니라 첫 번째
    /// 최상위 북마크**를 반환합니다. 따라서 최상위 항목들을 `next_sibling()`으로
    /// 이어가며, 각 항목의 하위 항목들은 `iter_direct_children()`으로 순회합니다.
    pub fn outline(&self) -> Vec<OutlineNode> {
        fn page_index_of(bookmark: &PdfBookmark) -> Option<usize> {
            bookmark
                .destination()
                .and_then(|d| d.page_index().ok())
                .filter(|i| *i >= 0)
                .map(|i| i as usize)
        }

        fn walk(bookmark: &PdfBookmark) -> Vec<OutlineNode> {
            let mut nodes = Vec::new();
            for child in bookmark.iter_direct_children() {
                let title = child.title().unwrap_or_default();
                nodes.push(OutlineNode::new(title, page_index_of(&child), walk(&child)));
            }
            nodes
        }

        let mut nodes = Vec::new();
        // 최상위 북마크 체인: root() = 첫 최상위, 나머지는 next_sibling().
        let mut current = self.document.bookmarks().root();
        while let Some(bookmark) = current {
            let title = bookmark.title().unwrap_or_default();
            nodes.push(OutlineNode::new(
                title,
                page_index_of(&bookmark),
                walk(&bookmark),
            ));
            current = bookmark.next_sibling();
        }
        nodes
    }

    /// 페이지의 텍스트 세그먼트를 검색용 `TextRun` 목록으로 변환합니다.
    ///
    /// pdfium의 텍스트 좌표는 **미디어박스 콘텐츠 공간** 입니다:
    /// 좌하단 원점(y 위)이고 페이지의 `/Rotate` 는 **적용되지 않습니다**
    /// (실측 검증: /Rotate 90/180 페이지에서 char 좌표가 변하지 않음).
    /// 앱의 페이지 좌표는 회전이 반영된 **표시 공간**(좌상단 원점, y 아래)
    /// 이므로, 여기서 세로 뒤집기 + 회전 변환을 해야 텍스트 하이라이트
    /// 스냅/검색 하이라이트가 렌더된 글자 위에 정확히 옵니다.
    pub fn page_text_runs(&self, index: usize) -> Result<Vec<TextRun>, String> {
        let page = self
            .document
            .pages()
            .get(index as i32)
            .map_err(|e| format!("Could not read page: {e}"))?;
        let w = page.width().value;
        let h = page.height().value;
        let rot = core_rotation(page.rotation().unwrap_or(PdfPageRenderRotation::None));
        let text = page.text().map_err(|e| format!("Text extraction failed: {e}"))?;
        let mut runs = Vec::new();
        for seg in text.segments().iter() {
            let txt = seg.text();
            let rect = content_rect_to_display(seg.bounds(), w, h, rot);
            runs.push(TextRun::new(txt, rect, Vec::new()));
        }
        Ok(runs)
    }

    /// 페이지의 **글자별** 경계 사각형(표시 공간) 목록을 반환합니다.
    ///
    /// pdfium-render 0.9.3의 정밀 텍스트 좌표는 `PdfPageTextChar::tight_bounds()`
    /// (FPDFText_GetCharBox)입니다. 하이라이트는 세그먼트 단위가 아니라 이
    /// 글자 단위로 판정해야 글이 딱 맞는 밴드로 칠해집니다.
    pub fn page_char_rects(&self, index: usize) -> Result<Vec<[f32; 4]>, String> {
        let page = self
            .document
            .pages()
            .get(index as i32)
            .map_err(|e| format!("Could not read page: {e}"))?;
        let w = page.width().value;
        let h = page.height().value;
        let rot = core_rotation(page.rotation().unwrap_or(PdfPageRenderRotation::None));
        let text = page.text().map_err(|e| format!("Text extraction failed: {e}"))?;
        let mut out = Vec::with_capacity(text.len().max(0) as usize);
        for ch in text.chars().iter() {
            // 오류가 나는 글자는 건너뜁니다.
            if let Ok(b) = ch.tight_bounds() {
                out.push(content_rect_to_display(b, w, h, rot));
            }
        }
        Ok(out)
    }

    /// 페이지의 **글자별 (텍스트, 표시 공간 사각형)** 목록을 반환합니다.
    /// 사전 오버레이의 단어 추출(`text::word_at`) 입력으로 사용합니다.
    pub fn page_chars(&self, index: usize) -> Result<Vec<TextChar>, String> {
        let page = self
            .document
            .pages()
            .get(index as i32)
            .map_err(|e| format!("Could not read page: {e}"))?;
        let w = page.width().value;
        let h = page.height().value;
        let rot = core_rotation(page.rotation().unwrap_or(PdfPageRenderRotation::None));
        let text = page.text().map_err(|e| format!("Text extraction failed: {e}"))?;
        let mut out = Vec::new();
        for ch in text.chars().iter() {
            if let Ok(b) = ch.tight_bounds() {
                let s = ch
                    .unicode_string()
                    .or_else(|| ch.unicode_char().map(|c| c.to_string()))
                    .unwrap_or_default();
                if s.is_empty() {
                    continue;
                }
                out.push(TextChar::new(s, content_rect_to_display(b, w, h, rot)));
            }
        }
        Ok(out)
    }

    /// 지정한 인덱스에 빈 페이지를 삽입합니다.
    pub fn insert_page_at(&mut self, index: usize, size_pts: [f32; 2]) -> Result<(), String> {
        let paper =
            PdfPagePaperSize::new_custom(PdfPoints::new(size_pts[0]), PdfPoints::new(size_pts[1]));
        self.document
            .pages_mut()
            .create_page_at_index(paper, index as i32)
            .map_err(|e| format!("Could not insert page: {e}"))?;
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

    /// 현재 문서(주석 포함)를 바이트로 직렬화합니다 (DB BYTEA 저장용).
    pub fn save_to_bytes(&self) -> Result<Vec<u8>, String> {
        self.document
            .save_to_bytes()
            .map_err(|e| format!("Save failed: {e}"))
    }

    /// 페이지를 시계(clockwise=true) 또는 반시계 방향으로 90° 회전합니다.
    /// 회전은 PDF에 저장되고, 표시 크기(너비/높이)도 함께 갱신됩니다.
    pub fn rotate_page(&mut self, index: usize, clockwise: bool) -> Result<(), String> {
        if index >= self.page_count() {
            return Err("Page index out of range.".to_string());
        }
        {
            let mut page = self
                .document
                .pages_mut()
                .get(index as i32)
                .map_err(|e| format!("Could not read page: {e}"))?;
            let current = page.rotation().unwrap_or(PdfPageRenderRotation::None);
            page.set_rotation(rotate_rotation(current, clockwise));
        }
        self.refresh_sizes();
        Ok(())
    }

    /// 문서의 모든 페이지를 시계/반시계 방향으로 90° 회전합니다.
    pub fn rotate_all_pages(&mut self, clockwise: bool) -> Result<(), String> {
        let count = self.page_count();
        for i in 0..count {
            self.rotate_page(i, clockwise)?;
        }
        Ok(())
    }

    /// 페이지 크기 캐시를 문서 상태에 맞게 다시 계산합니다.
    fn refresh_sizes(&mut self) {
        let count = self.document.pages().len() as usize;
        let mut sizes = Vec::with_capacity(count);
        for i in 0..count {
            sizes.push(display_size_of(&self.document, i).unwrap_or([595.0, 842.0]));
        }
        self.page_sizes_pts = sizes;
    }
}

/// 페이지의 표시 크기(포인트). **pdfium이 이미 /Rotate를 반영**합니다 —
/// FPDF_GetPageWidthF/HeightF는 회전 90/270일 때 이미 가로/세로가 뒤바뀐 값을
/// 돌려주므로(CPDF_Page::UpdateDimensions) 여기서 다시 바꾸면 **이중 교체**가
/// 되어 회전해도 캔버스 크기가 원래대로 남는 버그가 됩니다.
fn display_size_of(document: &PdfDocument<'static>, index: usize) -> Option<[f32; 2]> {
    let page = document.pages().get(index as i32).ok()?;
    Some([page.width().value, page.height().value])
}

/// 현재 회전 상태에서 시계/반시계 90° 회전한 다음 상태.
fn rotate_rotation(current: PdfPageRenderRotation, clockwise: bool) -> PdfPageRenderRotation {
    use PdfPageRenderRotation::*;
    match (current, clockwise) {
        (None, true) => Degrees90,
        (Degrees90, true) => Degrees180,
        (Degrees180, true) => Degrees270,
        (Degrees270, true) => None,
        (None, false) => Degrees270,
        (Degrees90, false) => None,
        (Degrees180, false) => Degrees90,
        (Degrees270, false) => Degrees180,
    }
}

/// pdfium의 회전 enum → core의 `PageRotation`.
fn core_rotation(r: PdfPageRenderRotation) -> PageRotation {
    match r {
        PdfPageRenderRotation::None => PageRotation::None,
        PdfPageRenderRotation::Degrees90 => PageRotation::Degrees90,
        PdfPageRenderRotation::Degrees180 => PageRotation::Degrees180,
        PdfPageRenderRotation::Degrees270 => PageRotation::Degrees270,
    }
}
