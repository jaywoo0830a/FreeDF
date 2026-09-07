Here is a detailed technical explanation of the performance issues and optimization strategies for rendering PDFs with `pdfium-render` and `egui` in Rust.

## The Core Problem

When you zoom in or out, the slow performance stems from triggering a **full-quality re-render of the entire visible page** for every single zoom step.

A single press of a zoom control can fire a dozen or more zoom steps per second. If each step initiates a complete re-render, these rendering tasks pile up faster than they can complete. While a single page might render in isolation within, say, 180ms, running a dozen such renders against work the user has already moved past locks the viewer, pins a CPU core at 100%, and causes the interface to freeze.

The solution is **not** a faster rasterizer. The solution is a **cache** that returns finished pages instantly and a **render loop willing to abandon work the moment it goes stale**.

---

## Strategic Optimizations

### 1. Bitmap Reuse (The Most Direct Performance Gain)

**The Problem:** Older versions of `pdfium-render` created a new `PdfBitmap` (and allocated new memory for it) on every page render, which is a major source of overhead.

**The Solution:** The newer versions (v0.7.12+) introduced `PdfPage::render_into_bitmap()` and `PdfPage::render_into_bitmap_with_config()`.

These functions offer higher performance by allowing you to **reuse an existing `PdfBitmap` object** rather than creating a new one each time. This significantly reduces the overhead from memory allocation and deallocation.

### 2. Implement a Smart Cache (The Most Critical Strategy)

This is the most important optimization to prevent repetitive re-rendering. You should cache the rendered bitmaps and serve them from the cache when the user zooms.

**Cache Key:** The cache key must be precise and include all factors that affect the final rendered image:
*   Page index
*   Zoom level or effective output pixel dimensions (width and height)
*   Rotation angle
*   Monitor DPI (PixelsPerInch)
*   Render options (e.g., annotations on/off, grayscale)

**Eviction Policy:** An unbounded cache will quickly exhaust memory, especially on large or high-DPI documents. You should implement a policy like **LRU (Least Recently Used)** and set a firm **memory limit**.

**Implementation Example (Rust Pseudocode)**:
```rust
use lru::LruCache;
use std::num::NonZeroUsize;

struct PdfCache {
    cache: LruCache<CacheKey, PdfBitmap>,
    max_memory_bytes: usize,
}

#[derive(Hash, PartialEq, Eq)]
struct CacheKey {
    page_index: usize,
    width: u32,
    height: u32,
    rotation: i32,
    // ... other configurations like DPI, render options
}

impl PdfCache {
    fn get_or_render(&mut self, key: CacheKey, page: &PdfPage) -> Option<&PdfBitmap> {
        if let Some(bitmap) = self.cache.get(&key) {
            return Some(bitmap);
        }
        // Estimate the new bitmap size and ensure space
        let new_size_bytes = key.width as usize * key.height as usize * 4;
        self.ensure_space(new_size_bytes);

        let mut bitmap = PdfBitmap::empty(key.width, key.height)?;
        // Use the high-performance render_into_bitmap_with_config
        page.render_into_bitmap_with_config(&mut bitmap, &config)?;
        self.cache.put(key, bitmap);
        self.cache.get(&key)
    }

    fn ensure_space(&mut self, needed: usize) {
        // Implement LRU eviction until there is enough space.
    }
}
```

### 3. Optimize Render Configuration (`PdfRenderConfig`)

You can balance quality and performance through the configuration.

*   **Use `thumbnail()` for Previews**: For thumbnails or quick previews, use `PdfRenderConfig::thumbnail()`, which reduces image quality settings to improve performance.
*   **Control Image Cache**: You can limit Pdfium's internal image cache size. A smaller cache may reduce memory usage at the cost of slower rendering.
*   **Be Aware of Transform Limits**: Pdfium's rendering pipeline supports either rendering with form data or with a custom transformation matrix, but **not both at the same time**. Applying a transformation (like zoom) automatically disables form data rendering. If you must render both, consider using `PdfPage::flatten()` to flatten the form elements into the page first.

### 4. Asynchronous Rendering & Cancellation

Move rendering to a background thread and support cancellation of stale tasks.

*   **Use `spawn_blocking`**: `pdfium-render` rendering is CPU-intensive. Use `tokio::task::spawn_blocking` or similar to run it on a dedicated thread pool, preventing it from blocking the UI thread.
*   **Implement Cancellation**: When a user zooms or scrolls quickly, many rendering requests are fired off. The work from previous zoom levels becomes wasted effort that should be abandoned.
    *   Use a **cancellation token** (e.g., `CancellationToken` from `tokio-util`).
    *   Pass this token to the render task.
    *   The render task should periodically check the token's cancellation status.
    *   If cancelled, the task should abort the rendering process and clean up.
    *   PDFium's progressive rendering API (`FPDF_RenderPageBitmap_Start` / `FPDF_RenderPage_Continue`) can be leveraged for this. By polling a cancellation token between rendering bands, you can abort a long render mid-page instead of blocking until completion.

---

## EGUI-Specific Optimization Tips

*   **Cache `egui::TextureHandle`**: Once a `PdfBitmap` is rendered, upload it to the GPU as a texture. Cache the resulting `egui::TextureHandle`. When zooming, if you hit the bitmap cache, you can directly draw the existing texture, avoiding both re-rendering and re-uploading.
*   **Use `egui::Image`**: Display the cached texture using `egui::Image::from_texture()`.
*   **Consider `egui::paint_callback`**: For more fine-grained control over GPU drawing, you can use this to bypass parts of egui's painting pipeline.

---

## Special Considerations for WASM

If you are compiling to WebAssembly (WASM), there are additional performance hurdles:

*   **Memory Copy Overhead**: The Pdfium WASM module and your Rust application live in separate memory spaces. Rendering results must be copied across this boundary, which is a major source of overhead.
*   **Use `as_array()`**: `pdfium-render` v0.7.11+ provides a WASM-specific `PdfBitmap::as_array()` function, which is a higher-performance alternative to the cross-platform `as_bytes()` function.
*   **Disable Debug Flags**: Ensure debug flags are set to `false` when initializing the Pdfium WASM module. Debug builds can be significantly slower.
*   **Memory Limits**: Browsers have limits on the size of `ImageData` objects (e.g., ~64MB in Safari). Rendering extremely large bitmaps might fail. You should limit the maximum render size or implement tiled rendering.

---

## Summary & Action Plan

1.  **Implement a Smart Cache**: This is the most critical step. Design a precise cache key and a firm eviction policy to avoid most re-rendering.
2.  **Use Bitmap Reuse**: Employ `render_into_bitmap_with_config()` to reduce memory allocation overhead.
3.  **Make it Asynchronous and Cancellable**: Move rendering to a background thread and implement cancellation logic to keep the UI responsive during rapid zoom operations.
4.  **Tweak Render Configuration**: Use `thumbnail()` for previews and be aware of the trade-offs in `PdfRenderConfig`.
5.  **WASM-Specific Checks**: If in a WASM environment, use `as_array()`, disable debug flags, and be mindful of memory limits.

[PDFIUM OFFICIAL DOC](https://pdfium.googlesource.com/pdfium/+/HEAD/docs/getting-started.md)