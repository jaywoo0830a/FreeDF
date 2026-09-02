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
  between them; every tab keeps its own page, zoom/pan, annotations, search, outline,
  panel open state & **panel widths**, and ink/paper settings. The tab strip scrolls
  horizontally and long titles are truncated (full name on hover)
- 🪟 **Split a tab into a new window** — right-click any **standalone PDF** tab and
  choose *Open in new window*: FreeDF relaunches itself as a separate OS window
  (each window is its own process — eframe runs one window per process and PDFium
  is single-binding per process) and **moves** the tab there — the current window
  closes the tab (ink is flushed to the sidecar first), so the same document never
  appears in two windows. FreeDF **notes** share one annotation file, so the option
  is disabled for them to avoid losing ink
- 🗂️ **Library panel** — a modern side panel with a **search filter** and count
  badges groups **Notes**, **PDFs** and **Recents** into clean rows (title +
  meta), so you can jump between notebooks and recently opened files
- 🔖 **Bookmarks** — mark pages and jump back to them from the Bookmarks menu;
  bookmarks are stored per document and survive restarts
- 🗂️ **Notes (CRUD)** — create, rename, delete and switch between notes; each note
  stores its own PDF, annotations and metadata under the app data folder
- ➕➖ **Page CRUD** — insert a blank page **from the current page** (copies its
  size & paper), **at the front** (begin/end), or **before / after the current
  page**; delete the current page; annotations stay aligned and page changes
  are persisted back into the note's PDF
- 🔄 **Rotate pages** — rotate the **current page** or **all pages** 90° CW / CCW;
  existing ink is rotated along with the page so nothing drifts, and the rendered
  aspect ratio is preserved (rotated pages no longer look squashed)
- 🧭 **Outline panel** — browse the PDF bookmark tree and jump to a page on click
- 🔍 **In-page word search** — case-insensitive; press **Ctrl+F** to toggle the
  search bar (with focus), yellow highlights for matches, an orange border for
  the current one, prev/next and a result counter
- ✏️ **Pen variants (Ballpoint / Fountain)** — three ink tools (Pen, Ballpoint,
  Fountain) that are **visibly different** even with a mouse: Ballpoint draws
  **thin & steady** (~0.6× the Width setting), Fountain draws **thicker** (~1.25×)
  and gets **thinner when you write fast** for a live calligraphy feel, and Pen is
  the medium, pressure-following reference (each tool shows its profile hint under
  the Width slider). Strokes are stored in page coordinates, so they never drift
  under zoom/pan
- ✏️ **Highlighter / Eraser / Pan** — text-aware Highlighter, round Eraser and
  hand Pan tool complete the toolkit
- 🔖 **Text-aware highlighter** — by default the Highlighter recognizes the
  document's actual text and snaps to it: swipe across a paragraph and you get
  clean highlights exactly over the words (turn off with “Snap to text”). Works on
  **rotated pages** too (text coordinates are mapped into the page's display
  space), and if the page has **no selectable text** (scanned/image PDF) a status
  message tells you instead of silently drawing a freehand stroke
- 🎨 **Color families** — Red / Blue / Black (plus Green / Orange / Purple / Custom)
  **round** swatch palettes for one-tap color picking
- 🎨 **Favorite-color palette** — a GoodNotes-style floating vertical sidebar on
  the right edge of the canvas holds the writing tools + your favorite colors;
  click a **round** swatch to ink with it, **+** to add the current color,
  right-click a swatch to remove it (saved in `session.json`)
- 🖊️ **Pen pressure** — pressure is read from touch events (Windows Ink) and mapped
  to stroke width via an adjustable `min/max` pressure curve (0.4×–1.4× by default)
- ↩️ **Undo / Redo** — diff-based, up to 256 steps; eraser and clear are undoable
- 🖼️ **PNG export** — renders the annotated page at 150 dpi
- 🖥️ **High refresh** — the app keeps repainting while a document is open so ink
  stays smooth on 120 Hz+ displays
- 📊 **Analysis logs** — structured JSON-lines event log (`freedf.log`)
- 🔤 **Typography** — the entire UI uses a bundled **Inter** font, with
  **NanumGothic** as an automatic fallback so **Hangul / Korean** (PDF outlines,
  note titles, search) render correctly
- 🎨 **Phosphor icons** — toolbar and panels use crisp vector icons **with text
  labels**, so every action is recognizable
- 🗂️ **Three-tier toolbar** — panels / bookmarks / undo-save on the first row;
  **Page** tools (insert / delete + paper grid & color) on the second row; ink
  ink tool picker & settings (icon-only Pen / Ballpoint / Fountain / Highlighter /
  Eraser / Pan) on the third row — **drag a tool icon to reorder the toolbar**
  (the order is saved to `session.json`). The search bar appears only on `Ctrl+F`
- 🔍 **Zoom stays put** — expanding/collapsing the Library/Outline panels or
  resizing the window re-centers the page at the **current zoom** instead of
  resetting it
- 🎨 **Design system** — 1rem (16 px) base font scale, 4 px spacing grid, rounded
  corners, and a **Nord** dark theme built from primitive + semantic design
  tokens (Polar Night / Snow Storm / Frost / Aurora palette)
- 🌙 **Always dark** — the app locks to dark mode for comfortable long reading
  sessions; the PDF page itself stays white for contrast
- ♿ **Accessible (WCAG)** — theme-aware high-contrast text, error colors, text
  alternatives for icons, and roomy click targets
- 🖱️ **Trackpad & gestures** — pinch zoom, Ctrl + wheel zoom in small steps
  (+1 % per notch, gently accelerating while you scroll), Ctrl + two-finger
  scroll zoom, two-finger pan/flip, horizontal scroll, and momentum (inertial)
  scrolling
- 🪶 **Smooth animated scroll & zoom** — mouse-wheel scrolling and Ctrl+wheel
  zoom **glide** to their target over a few frames (eased), so panning and
  zooming feel fluid instead of stepping/jumping
- 🎞️ **Page transitions** — smooth slide animation when flipping pages; the zoom
  level is preserved when moving between pages
- 🔁 **PgDn / PgUp = next / previous page** (not vertical scrolling). On a FreeDF
  note, pressing **PgDn on the last page automatically appends a fresh page**
  (same size & paper) so you can keep writing without breaking flow
- 🧭 **Floating canvas controls** — a semi-transparent bar at the bottom-center
  of the canvas shows `Prev [page]/[total] Next`, zoom in/out, and
  Fit Width / Fit Height (page navigation, zoom and fit live here, not in the
  toolbar)
- 🪟 **Split-view focus mode** — when the window is **half or narrower than the
  monitor** (Windows snap / split view), FreeDF auto-collapses to just the
  canvas + palette; a small top-right control (`☰ Show UI`) brings the tabs and
  toolbar back on demand for multitasking, and `✕ Hide UI` hides them again
- 🪟 **Split-view focus mode** — when the window gets narrow (e.g. Windows split
  view), FreeDF auto-collapses to **canvas + writing palette** only; a small
  floating control (top-right “☰ Show UI”) restores the tabs/toolbar on demand,
  and toggles the palette — great for multitasking side-by-side
- 📄 **Paper per page** — each page keeps its own style (blank, ruled, grid,
  dotted) and color (white, cream, ice blue, mint, light gray), applied
  independently and exported to PNG
- 🔢 **Numerical paper spacing** — the ruled/grid/dotted spacing is a number
  (in points, 12–120) you can type directly in the Paper options; it is saved
  per page
- 📐 **Paper sizes** — A3 / A4 / A5 / Letter / Legal for new notes and pages
- 💾 **Remembers your last settings** — the most recently used pen ink color and
  paper style/color/size are saved to the default `session.json` and restored on
  startup
- 🗂️ **Per-document session** — reopening a note/PDF restores exactly where you
  left off: the last page, tool, pen/highlighter/eraser settings, zoom & pan,
  page alignment, paper choices, open panels, and panel widths (stored per document)
- 🎨 **Gray canvas** — neutral gray background behind the page (dark/light aware)
- ↔️ **Page alignment** — with the side panels collapsed, align the page
  left / center / right
- 🖊️ **Custom cursors** — Pen = small dot **or a round ring cursor** with a
  breathing halo (switch style in the pen settings), Highlighter = colored
  rectangle, Eraser = white translucent circle with a drop shadow, Pan = small
  move crosshair; the OS cursor is restored outside the page
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

You can also open a standalone PDF directly by passing it on the command line
(this is what *Open in new window* uses to spawn a fresh process):

```bash
freedf /path/to/document.pdf
# or
freedf --open /path/to/document.pdf
```

> When distributing, ship `pdfium.dll` next to `target/release/freedf.exe`.

## Shortcuts

| Shortcut | Action |
|---|---|
| `Ctrl+O` | Open PDF |
| `Ctrl+N` | New note |
| `Ctrl+F` | Toggle search bar |
| `Ctrl+Z` / `Ctrl+Y` (or `Ctrl+Shift+Z`) | Undo / Redo |
| `Ctrl+S` | Save annotations |
| `Ctrl+E` | Export current page as PNG |
| `PgDn` / `PgUp` | Next / previous page (on a note, PgDn adds a new page at the end) |
| `←` / `→` | Previous / next page |
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
| `crates/freedf/src/app/mod.rs` | app state (`FreeDfApp`) + `eframe::App` glue, session persistence, shared helpers |
| `crates/freedf/src/app/tabs.rs` | tab strip UI + tab lifecycle (open/switch/close) + detach to a new window |
| `crates/freedf/src/app/toolbar.rs` | three-tier toolbar, tool picker (drag to reorder), per-tool settings |
| `crates/freedf/src/app/panels.rs` | Library (Notes/PDFs/Recents) + Outline side panels |
| `crates/freedf/src/app/canvas.rs` | page canvas: pan/zoom/draw, painting, text-aware highlight, palette & nav overlays, cursors |
| `crates/freedf/src/app/actions.rs` | open/save/export, page CRUD & rotate, search, bookmarks, undo/redo |
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
