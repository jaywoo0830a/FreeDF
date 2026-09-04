# FreeDF — Lightweight PDF Viewer & Ink (PostgreSQL-backed)

An ultra-lightweight PDF viewer with a drawing-pad annotation layer, built on
**egui 0.36.1** + **pdfium-render 0.9.3**. It is designed for taking handwritten
notes on a tablet / drawing pad and is Windows 11-friendly (system theme, HiDPI,
native file dialogs, Windows Ink pressure).

**FreeDF v2 stores everything in PostgreSQL 18.6** — notes, PDF binaries, strokes,
paper settings, sessions, recents and the event log all live in the database
(JSON files are legacy and gone). The DB (and its schema) is managed server-side:

```bash
cd server/db && ./init.sh && ./up.sh
```

```
├─ crates/
│  ├─ freedf-core/   # Pure-Rust, GUI-free core (model, store, transform, history,
│  │                 #   notes, pages, outline, search, pen, logging) + unit/integration tests
│  └─ freedf/        # egui + pdfium-render + PostgreSQL desktop app
└─ server/           # 서버 측 (외부 서비스 0, VPS 자체 호스팅)
   ├─ db/            # PostgreSQL 18.6 + 마이그레이션(스키마 단일 진실 공급원) + 스크립트
   ├─ backend/       # 미디어 API (axum): 업로드/목록/삭제
   ├─ nginx/         # 미디어 정적 서빙(Range 스트리밍) + API 프록시
   └─ docker-compose.yml
```

## Features

- 📄 **PDF open / render** — high-resolution page rendering via PDFium (HiDPI aware)
- 🗂️ **Tabs — multiple documents** — open several notes and PDFs at once and switch
  between them; every tab keeps its own page, zoom/pan, annotations, search, outline,
  panel open state & **panel widths**, and ink/paper settings. The tab strip scrolls
  horizontally and long titles are truncated (full name on hover)
- 🪟 **Split a tab into a new window** — right-click **any** tab (note or PDF)
  and choose *Open in new window*: FreeDF relaunches itself as a separate OS
  window (each window is its own process — eframe runs one window per process
  and PDFium is single-binding per process) that reopens the same **database**
  document (`freedf --doc <id>`). Because all data is in PostgreSQL, even the
  same note can now be open in two windows without losing ink
- 🗂️ **Library panel** — a modern side panel with a **search filter** and count
  badges groups **Notes**, **PDFs** and **Recents** into clean rows (title +
  meta), so you can jump between notebooks and recently opened files. Notes and
  PDFs support **multi-select deletion**: tick the checkboxes and press
  *Delete selected* — notes are removed with all their data, PDF documents are
  removed from the library (the original files on disk are left untouched)
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
  the medium, pressure-following reference. Each tool shows a **live mini-stroke
  preview** next to the Width slider that plots the actual ink function of that
  tool (Pen: width = f(pressure) · Ballpoint: ≈ constant · Fountain: width =
  f(pressure, speed)) — hover it for the profile description. Strokes are stored
  in page coordinates, so they never drift under zoom/pan
- 🖊️ **Real-pen ink feel** — pen & fountain strokes **taper** at both ends like
  real ink (thin start, thin flick-off), fountain pens leave a small **ink blob**
  where the nib first touches the paper, and every stroke is drawn as one smooth
  **variable-width ribbon** (per-point widths, no more circle-stacking “beading”).
  A **stabilizer slider** (Smoothing, 0–1) filters hand tremor with a
  speed-adaptive **One Euro filter** — silky lines, zero lag on fast strokes
  (unit-tested in `freedf-core`)
- 🎨 **Favorite-color palette** — a GoodNotes-style floating vertical sidebar on
  the right edge of the canvas holds the writing tools + your favorite colors
  (**exactly 3 slots**: black / red / blue by default); click a **round** swatch
  to ink with it, **+** to add the current color (disabled when full),
  right-click a swatch to replace it (saved in `session.json`)
- 🖊️ **Pen pressure** — pressure is read from touch events (Windows Ink) and mapped
  to stroke width via an adjustable `min/max` pressure curve (0.4×–1.4× by default);
  **Min** = thickness multiplier at the lightest touch, **Max** = multiplier at
  full pressure (both explained on hover)
- ✏️ **Highlighter / Eraser / Pan** — text-aware Highlighter, round Eraser and
  hand Pan tool complete the toolkit
- 🔖 **Highlighter (default: plain marker)** — the Highlighter draws a clean,
  even, semi-transparent marker line along your stroke by default: it is filled
  as one **precise rectangle ribbon** with flat (square) ends that sit exactly
  on the path — no round caps poking out, no pressure wobble. The on-canvas
  **cursor is a small precise rectangle** whose height previews the real marker
  width and whose left edge is anchored to the pen tip. Optionally enable
  **“Snap to text”** in the tool settings to make it follow the document's real
  text instead — snapping uses PDFium's **per-character** boxes
  (`tight_bounds`), merges each touched line into one clean band, ignores
  pressure, and works on **rotated pages**; if the page has no selectable text
  it falls back to the plain marker stroke
- 🎨 **Color families** — Red / Blue / Black (plus Green / Orange / Purple / Custom)
  **round** swatch palettes for one-tap color picking
- ↩️ **Undo / Redo** — diff-based, up to 256 steps; eraser and clear are undoable
- ️ **High refresh** — the app keeps repainting while a document is open so ink
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
- 🖊️ **Device-aware input** — a pen writes, everything else pans: mouse/trackpad
  automatically pan the page in every ink tool (like real note apps), and only
  a stylus draws ink. Enable *Mouse ink* in the pen settings to let the mouse
  draw too. The on-page cursor switches to a pan crosshair for mouse users
- 📖 **Dictionary overlay** — toggle *Dictionary* and tap any word on the page:
  a floating overlay shows the pronunciation and definitions
  (dictionaryapi.dev). Lookups run in the background and are **cached in the
  database** (`word_cache`), so words you've seen work offline. Word extraction
  uses PDFium's per-character boxes (`search::word_at`)
- 🎞️ **Smooth page transitions** — the next/previous page texture is
  **pre-rendered in advance** (prefetch), so page flips start instantly
  without waiting for the CPU raster pass
- 🖥️ **DirectX 12 renderer (opt-in)** — run with `FREEDF_RENDERER=wgpu` to use
  the wgpu/DirectX 12 backend for GPU-accelerated compositing, zoom and page
  transitions (recommended with a discrete GPU). Default stays glow/OpenGL for
  maximum compatibility
- 🔁 **PgDn / PgUp = browser-style paging** — if the page is taller than the
  canvas it scrolls **one viewport at a time** inside the page (just like a web
  browser); only when there is no more room does it move to the next / previous
  page (flip is vertical). On a FreeDF note, reaching the **last page** with
  PgDn automatically appends a fresh page (same size & paper) so you can keep
  writing. The step logic is defined by unit tests
  (`transform::browser_page_step`)
- 🧭 **Floating canvas controls** — a semi-transparent bar at the bottom-center
  of the canvas shows `Prev [page]/[total] Next`, zoom in/out, **zoom lock**, and
  Fit Width / Fit Height (page navigation, zoom and fit live here, not in the
  toolbar). The **lock** (or `Ctrl+L`) freezes the zoom — wheel, pinch,
  shortcuts and the zoom/fit buttons are all ignored until you unlock, so
  accidental zooming is impossible
- 🪟 **Split-view focus mode** — when the window gets narrow (e.g. Windows split
  view), FreeDF auto-collapses to **canvas + writing palette** only; a small
  floating control (top-right “☰ Show UI”) restores the tabs/toolbar on demand,
  and toggles the palette — great for multitasking side-by-side. `Ctrl+Shift+M`
  toggles the same focus mode at **any** window size
- 📄 **Paper per page** — each page keeps its own style (blank, ruled, grid,
  dotted) and color, applied independently. The Paper
  section edits the **current page** and becomes the **default for new pages**;
  press **Apply to all** to copy the current paper onto every page at once
- 🎨 **Fully custom paper** — paper **color** accepts any color from a full color
  picker (beyond the 5 presets), the ruling **line color / thickness / spacing**
  are all adjustable, and the page size supports a **Custom** size with
  width × height entered in millimetres (new pages & notes use it)
- 🎨 **Custom ruling** — the **line color** (any RGBA) and **line thickness** of
  the ruled / grid / dotted paper are adjustable per page; the thickness is
  stored in page points (scales with zoom). Settings live in the Paper
  settings window (Paper ▸ Settings)
- 🔢 **Numerical paper spacing** — the ruled/grid/dotted spacing is a number
  (in points, 12–120) you can type directly in the Paper options; it is saved
  per page
- 📐 **Paper sizes** — A3 / A4 / A5 / Letter / Legal / **Custom (mm)** for new
  notes and pages
- 💾 **Remembers your last settings** — the most recently used pen ink color and
  paper style/color/size are saved to the default `session.json` and restored on
  startup
- 🗂️ **Per-document session** — reopening a note/PDF restores exactly where you
  left off: the last page, tool, pen/highlighter/eraser settings, zoom & pan,
  page alignment, paper choices, open panels, and panel widths (stored per document)
- 🎨 **Gray canvas** — neutral gray background behind the page (dark/light aware)
- ↔️ **Page alignment** — with the side panels collapsed, align the page
  left / center / right
- 🎨 **Custom pen color** — in addition to the family swatches, the pen tools
  accept **any color** via a full color picker (RGBA, right next to the swatches)
- 🖊️ **Custom cursors** — Pen = small dot **or a round ring cursor** with a
  breathing halo (switch style in the pen settings), Highlighter = colored
  rectangle, Eraser = white translucent circle with a drop shadow, Pan = small
  move crosshair; the OS cursor is always restored outside the page and never
  disappears (even under floating overlays / side panels)
- 💾 **Auto-save & close prompt** — strokes and pages are saved continuously, and
  quitting asks whether to save first
- 🎙️ **Microphone recording** — the Recordings panel records audio from your
  microphone (WAV) and uploads it to the server automatically (Windows/macOS)
- 🧪 **Full test coverage** — unit tests for every core feature plus end-to-end
  integration tests, all runnable without a GUI or PDFium

## Requirements

- Rust 1.75+ (MSRV follows egui 0.36)
- **Sync v3 API 서버** — `server/backend` (axum) + PostgreSQL (Docker 권장:
  `cd server/db && ./up.sh`, API는 `cd server/backend && ./up.sh`)
  - 앱의 저장소는 전부 이 서버 — 첫 실행 대화상자에서 서버 주소/API 키를
    입력하면 `server.json`에 저장됩니다 (DB 직접 연결 워크플로우는 제거됨).
  - 문서는 스냅샷 ZIP으로 왕복, 프로토콜 명세는
    `docs/sync-protocol-v3.md` + `docs/openapi/sync-v3.openapi.yaml`
    (타입 단일 소스: `crates/freedf-sync`)
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

# DB smoke test (Docker postgres 필요)
FREEDF_TEST_DB=1 cargo test -p freedf smoke_against_live_postgres

# Build & run the app
cargo run -p freedf

# Small, fast release binary
cargo build --release -p freedf
```

### 데이터베이스 시작 (운영)

```bash
cd server/db
./init.sh                  # .env 생성 (비밀번호 자동 생성, 편집 가능)
./up.sh                    # PostgreSQL 18.6 시작 + 마이그레이션 자동 적용
./down.sh                  # 중지 (데이터 유지)
./down.sh --wipe           # 중지 + 데이터 완전 삭제
```

- **Win 11 (WSL2)**: Docker Desktop의 WSL Integration을 켜고 WSL 터미널에서 실행
  (`docker.exe` 자동 감지).
- **Mac / Linux**: Docker Desktop 또는 Docker Engine 그대로.
- PostgreSQL 튜닝은 `server/db/postgresql.conf` (SSD 전용 — PG18 내장 비동기
  I/O, WAL zstd 압축 등). 호스트 RAM에 맞춰 `shared_buffers` 조정.
- 백업: `docker compose -f server/db/docker-compose.yml exec db pg_dump -U freedf freedf > backup.sql`

### 데이터 저장 구조 (전부 DB)

| 데이터 | 테이블 | 비고 |
|---|---|---|
| 노트/PDF 문서 | `documents` | PDF 본문은 `BYTEA` (단일 진실 공급원) |
| 주석(획) | `strokes` | **획 단위 행** — 그릴 때마다 증분 INSERT, 지우개는 DELETE |
| 용지/북마크 | `pages` | 페이지별 그리드/색/간격/선 두께 + 북마크 |
| **영속 히스토리** | `doc_edits` | 그리기/지우기 한 번 = 행 하나(Edit JSONB) — **재시작 후에도 undo/redo 복원**, 문서당 500건 유지 |
| **미디어/첨부** | `media` | 이미지/오디오/파일용 스키마 준비 (클라우드·미디어 기능 대비) |
| GUI 세션 | `sessions` (문서별) / `app_state` (전역) | JSONB |
| 최근 목록 | `recents` | `ON DELETE CASCADE` |
| 이벤트 로그 | `event_log` | 구조화 JSONB |
| 제목 전문 검색 | GIN 표현식 인덱스 (`documents_title_fts_idx`) | 라이브러리 검색 대비 |

외부 PDF를 열면 **DB로 import**되어(원본 파일은 그대로 두고) 어느 기기에서나
같은 데이터를 봅니다. 스트로크 id는 전역 시퀀스(`stroke_id_seq`)로 할당되어
undo/redo가 정확히 같은 행을 복원합니다. 같은 노트를 **두 창에서** 여는 것도
이제 안전합니다 (탭 우클릭 → Open in new window).

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
| `Ctrl+Shift+M` | Toggle **focus mode**: hide all toolbars (any window size);
  canvas + palette remain, `☰ Show UI` (top-right) or the shortcut restores them |
| `PgDn` / `PgUp` | Browser-style paging: scroll one viewport inside the page;
  at the page edge move to next / previous page (vertical flip; on a note,
  PgDn past the last page appends a fresh page) |
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
`undo_redo`, `search`, `outline_jump`, `error`.

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
| `crates/freedf/src/app/actions.rs` | open/save, page CRUD & rotate, search, bookmarks, undo/redo |

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
