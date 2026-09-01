# FreeDF — Lightweight PDF Viewer & Ink

An ultra-lightweight PDF viewer with a drawing-pad annotation layer, built on
**egui 0.36.1** + **pdfium-render 0.9.3**. It is designed for taking handwritten
notes on a tablet / drawing pad and is Windows 11-friendly (system theme, HiDPI,
native file dialogs, Windows Ink pressure).

```
├─ crates/
│  ├─ freedf-core/   # Pure-Rust, GUI-free core (model, store, transform, history,
│  │                 #   notes, pages, outline, search, pen, logging) + unit/integration tests
│  └─ freedf/        # egui + pdfium-render desktop app
```

## Features

- 📄 **PDF open / render** — high-resolution page rendering via PDFium (HiDPI aware)
- 🗂️ **Tabs — multiple documents** — open several notes and PDFs at once and switch
  between them; every tab keeps its own page, zoom/pan, annotations, search and outline
- 🕘 **Recent files** — a combined list of the notes and PDFs you opened recently,
  one click reopens (or switches to the already-open tab)
- 🔖 **Bookmarks** — mark pages and jump back to them from the Bookmarks menu;
  bookmarks are stored per document and survive restarts
- 🗂️ **Notes (CRUD)** — create, rename, delete and switch between notes; each note
  stores its own PDF, annotations and metadata under the app data folder
- ➕➖ **Page CRUD** — add a blank page or delete the current page; annotations are
  kept aligned and page changes are persisted back into the note's PDF
- 🧭 **Outline panel** — browse the PDF bookmark tree and jump to a page on click
- 🔍 **In-page word search** — case-insensitive; yellow highlights for matches, an
  orange border for the current one, with prev/next/clear and a result counter
- ✏️ **Pen / Highlighter / Eraser / Pan** — strokes are stored in page coordinates,
  so they never drift under zoom/pan
- 🎨 **Color families** — Red / Blue / Black (plus Green / Orange / Purple / Custom)
  swatch palettes for one-tap color picking
- 🖊️ **Pen pressure** — pressure is read from touch events (Windows Ink) and mapped
  to stroke width via an adjustable `min/max` pressure curve (0.4×–1.4× by default)
- ↩️ **Undo / Redo** — diff-based, up to 256 steps; eraser and clear are undoable
- 🖼️ **PNG export** — renders the annotated page at 150 dpi
- 🖥️ **High refresh** — the app keeps repainting while a document is open so ink
  stays smooth on 120 Hz+ displays
- 📊 **Analysis logs** — structured JSON-lines event log (`freedf.log`)
- 🔤 **Typography** — the entire UI uses a single bundled **Inter** font
- 🎨 **Phosphor icons** — toolbar and panels use crisp vector icons **with text
  labels**, so every action is recognizable
- 🗂️ **Two-tier toolbar** — file/notes/bookmark/page-tools on the first row,
  drawing tools & pen settings on the second row, search on the third
- 🎨 **Design system** — 1rem (16 px) base font scale, 4 px spacing grid, rounded
  corners, and a **Nord** dark theme built from primitive + semantic design
  tokens (Polar Night / Snow Storm / Frost / Aurora palette)
- 🌙 **Always dark** — the app locks to dark mode for comfortable long reading
  sessions; the PDF page itself stays white for contrast
- ♿ **Accessible (WCAG)** — theme-aware high-contrast text, error colors, text
  alternatives for icons, and roomy click targets
- 🖱️ **Trackpad & gestures** — pinch zoom, Ctrl + two-finger scroll zoom,
  two-finger pan/flip, horizontal scroll, and momentum (inertial) scrolling
- 🎞️ **Page transitions** — smooth slide animation when flipping pages; the zoom
  level is preserved when moving between pages
- 🧭 **Floating canvas controls** — a semi-transparent bar at the bottom-center
  of the canvas shows `Prev [page]/[total] Next`, zoom in/out, and
  Fit Width / Fit Height (page navigation, zoom and fit live here, not in the
  toolbar)
- 📄 **Paper per page** — each page keeps its own style (blank, ruled, grid,
  dotted) and color (white, cream, ice blue, mint, light gray), applied
  independently and exported to PNG
- 📐 **Paper sizes** — A3 / A4 / A5 / Letter / Legal for new notes and pages
- 💾 **Remembers your last settings** — the most recently used pen ink color and
  paper style/color/size are saved to the default `session.json` and restored on
  startup
- 🗂️ **Per-document session** — reopening a note/PDF restores exactly where you
  left off: the last page, tool, pen/highlighter/eraser settings, zoom & pan,
  page alignment, paper choices, and open panels (stored per document)
- 🎨 **Gray canvas** — neutral gray background behind the page (dark/light aware)
- ↔️ **Page alignment** — with the side panels collapsed, align the page
  left / center / right
- 🖊️ **Custom cursors** — Pen = small 4×4 dot, Highlighter = colored rectangle,
  Eraser = animated red circle, Pan = small move crosshair; the OS cursor is
  restored outside the page
- 💾 **Auto-save & close prompt** — strokes and pages are saved continuously, and
  quitting asks whether to save first
- 🧪 **Full test coverage** — unit tests for every core feature plus end-to-end
  integration tests, all runnable without a GUI or PDFium

## Requirements

- Rust 1.75+ (MSRV follows egui 0.36)
- **PDFium library** placed next to the executable

### Windows 11: getting pdfium.dll

Run the provided installer script from the project root (PowerShell 5.1+ / Windows 10+):

```powershell
# Downloads the latest pdfium.dll and copies it next to the executable(s)
.\scripts\install-pdfium.ps1
```

Or do it manually (note: current releases name the asset `pdfium-win-x64.tgz`):

```powershell
Invoke-WebRequest https://github.com/bblanchon/pdfium-binaries/releases/latest/download/pdfium-win-x64.tgz -OutFile pdfium.tgz
tar -xzf pdfium.tgz
copy bin\pdfium.dll .
```

> **Tip:** if pressing `Ctrl+O` and picking a PDF (or creating a note) shows a
> "PDFium library not found" popup, the DLL isn't being found. Run the installer
> script above — it now also copies `pdfium.dll` into the app data folder
> (`%LOCALAPPDATA%\FreeDF`), which the app always searches. If the error says a
> file *was found but failed to load*, install the **Microsoft Visual C++
> Redistributable** (x64).

On Linux, place `libpdfium.so` next to the executable (or in `~/.local/share/freedf`).

## Build & Run

```bash
# Core tests (run without a GUI or PDFium)
cargo test -p freedf-core

# Build & run the app
cargo run -p freedf

# Small, fast release binary
cargo build --release -p freedf
```

> When distributing, ship `pdfium.dll` next to `target/release/freedf.exe`.

## Shortcuts

| Shortcut | Action |
|---|---|
| `Ctrl+O` | Open PDF |
| `Ctrl+N` | New note |
| `Ctrl+F` | Search in current page |
| `Ctrl+Z` / `Ctrl+Y` (or `Ctrl+Shift+Z`) | Undo / Redo |
| `Ctrl+S` | Save annotations |
| `Ctrl+E` | Export current page as PNG |
| `PgUp` / `PgDn`, `←` / `→` | Page navigation |
| `+` / `-` | Zoom in / out |
| `P` / `H` / `E` / `V` | Pen / Highlighter / Eraser / Pan |
| Ctrl+wheel | Zoom |
| Wheel | Page flip (if fully visible) or vertical scroll |
| Middle-button drag | Pan |

## App data & logging

Data lives in `%LOCALAPPDATA%/FreeDF` on Windows and `~/.local/share/freedf` elsewhere:

```
FreeDF/
├─ notes/
│  ├─ notes.json          # library index (titles, timestamps, page counts)
│  └─ <note-id>/
│     ├─ document.pdf     # the note's PDF
│     └─ annotations.json # that note's strokes
└─ logs/
   └─ freedf.log          # structured JSON-lines event log
```

The log records one JSON object per line for analysis, e.g.:

```json
{"epoch_ms":1719999999999,"seq":1,"event":"app_start","version":"0.1.0"}
{"epoch_ms":1720000000001,"seq":2,"event":"note_opened","note_id":0,"title":"Lecture","page_count":12}
{"epoch_ms":1720000000123,"seq":3,"event":"stroke_added","page":1,"points":42,"tool":"Pen","width":2.5}
```

Events: `app_start`, `note_opened`, `note_created`, `note_renamed`, `note_deleted`,
`page_changed`, `page_added`, `page_deleted`, `stroke_added`, `stroke_erased`,
`undo_redo`, `search`, `outline_jump`, `export_png`, `error`.

## Project structure

| Path | Contents |
|---|---|
| `crates/freedf-core/src/model.rs` | `ToolType`, `StrokePoint`, `Stroke` |
| `crates/freedf-core/src/store.rs` | per-page annotation store, eraser hit-test, JSON |
| `crates/freedf-core/src/transform.rs` | page ↔ view coordinates (zoom/pan/fit) |
| `crates/freedf-core/src/history.rs` | diff-based undo/redo |
| `crates/freedf-core/src/notes.rs` | note library CRUD + file layout |
| `crates/freedf-core/src/pages.rs` | page insert/delete with annotation re-alignment |
| `crates/freedf-core/src/outline.rs` | outline tree model + flatten/search |
| `crates/freedf-core/src/search.rs` | case-insensitive word search with highlight rects |
| `crates/freedf-core/src/pen.rs` | color families/palettes + pressure curve |
| `crates/freedf-core/src/logging.rs` | JSON-lines structured logger |
| `crates/freedf/src/pdf.rs` | PDFium loading, rendering, outline/text extraction, page CRUD, save |
| `crates/freedf/src/app.rs` | canvas, tools, toolbar, notes/outline/search UI, shortcuts, file IO |
| `crates/freedf/src/export.rs` | rasterize annotations onto an image → PNG |

## Troubleshooting

### Crash on startup with 0xc0000005 (access violation)
- **Cause:** eframe 0.36's default renderer is **wgpu (DX12)**, which can crash at
  startup on some Windows GPUs / VMs.
- **Fix:** this project is configured to use the **OpenGL (glow) renderer**. Rebuild
  from the latest source:
  ```powershell
  cargo build --release -p freedf
  ```
- If it still crashes, update your GPU driver, or use an OpenGL software renderer in
  remote-desktop / VM environments.

## License

MIT (FreeDF)
