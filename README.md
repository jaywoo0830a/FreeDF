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
- 🔤 **Typography** — the entire UI uses a single bundled **PT Serif** font
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

> **Tip:** if pressing `Ctrl+O` and picking a PDF seems to "do nothing", it is almost
> always because `pdfium.dll` is missing next to `freedf.exe` — run the installer
> script above. The app shows the error in the bottom status bar.

On Linux, place `libpdfium.so` next to the executable.

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
