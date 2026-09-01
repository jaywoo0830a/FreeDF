# FreeDF — 초경량 PDF 뷰어 + 드로잉 패드

egui 0.36.1 + pdfium-render 0.9.3 기반의 **초경량 PDF 뷰어**입니다.
태블릿/드로잉 패드로 **메모와 필기**를 하기 좋게 설계했습니다.
Windows 11 친화적(시스템 테마 따라가기, HiDPI, 네이티브 파일 대화상자)이며
단일 페이지 중심의 빠른 렌더링을 제공합니다.

```
├─ crates/
│  ├─ freedf-core/   # GUI 없는 순수 Rust 코어 (모델·저장소·변환·이력) + 단위/통합 테스트
│  └─ freedf/        # egui + pdfium-render 기반 데스크톱 앱
```

## 주요 기능

- 📄 **PDF 열기/렌더링** — pdfium으로 페이지를 고해상도 렌더링 (HiDPI 대응)
- ✏️ **펜 / 형광펜 / 지우개 / 이동** — 페이지 좌표계로 저장되어 줌/팬에 흔들리지 않음
- 🖌️ 색상·두께 조절, 압력 값 저장(데이터 모델 포함, egui 0.36은 공용 압력 API 부재)
- ↩️ **실행취소/다시실행** (diff 기반, 최대 256단계)
- 🗂️ **메모 저장/불러오기** — PDF 옆에 `*.freedf.json` 자동 저장·자동 로드
- 🖼️ **PNG 내보내기** — 주석을 페이지 위에 그려 150dpi 이미지로 저장
- ⌨️ 단축키, 줌(핀치/Ctrl+휠), 페이지 넘기기, 중간버튼 팬

## 요구 사항

- Rust 1.75+ (MSRV는 egui 0.36 기준)
- **PDFium 라이브러리** (실행 파일 옆에 배치)

### Windows 11에서 pdfium.dll 준비

```powershell
# 프로젝트 루트에서 실행 (또는 target/release에 복사)
Invoke-WebRequest https://github.com/bblanchon/pdfium-binaries/releases/latest/download/pdfium-windows-x64.tgz -OutFile pdfium.tgz
tar -xzf pdfium.tgz
copy bin\pdfium.dll .
```

Linux에서는 `libpdfium.so`를 실행 파일 옆에 두면 됩니다.

## 빌드 & 실행

```bash
# 코어 테스트 (GUI/PDFium 없이 실행 가능)
cargo test -p freedf-core

# 앱 빌드/실행
cargo run -p freedf

# 릴리스 빌드 (작고 빠른 실행 파일)
cargo build --release -p freedf
```

> 릴리스 빌드 후 `target/release/freedf.exe` 옆에 `pdfium.dll`을 함께 배포하세요.

## 단축키

| 단축키 | 동작 |
|---|---|
| `Ctrl+O` | PDF 열기 |
| `Ctrl+Z` / `Ctrl+Y`(또는 `Ctrl+Shift+Z`) | 실행취소 / 다시실행 |
| `Ctrl+S` | 메모 저장 |
| `Ctrl+E` | 현재 페이지 PNG 내보내기 |
| `PgUp`/`PgDn`, `←`/`→` | 페이지 이동 |
| `+` / `-` | 확대 / 축소 |
| `P` / `H` / `E` / `V` | 펜 / 형광펜 / 지우개 / 이동 |
| Ctrl+휠 | 줌 |
| 휠 | 페이지 넘기기(꽉 차면) 또는 세로 스크롤 |
| 중간 버튼 드래그 | 화면 이동 |

## 메모 파일 형식

주석은 JSON(`serde`)으로 직렬화됩니다. 페이지별 스트로크(도구·RGBA·두께·좌표·압력)를 보관합니다.

```json
{"pages":{"0":{"page_index":0,"strokes":[{"id":0,"tool":"Pen","color":[20,20,20,255],"width":2.5,"points":[{"x":10.0,"y":20.0,"pressure":0.5}]}]}},"next_stroke_id":1}
```

## 프로젝트 구조

| 경로 | 내용 |
|---|---|
| `crates/freedf-core/src/model.rs` | `ToolType`, `StrokePoint`, `Stroke` |
| `crates/freedf-core/src/store.rs` | 페이지별 주석 저장소, 지우개 히트 테스트, JSON |
| `crates/freedf-core/src/transform.rs` | 페이지 좌표 ↔ 뷰 좌표 (줌/팬/핏) |
| `crates/freedf-core/src/history.rs` | diff 기반 undo/redo |
| `crates/freedf/src/pdf.rs` | PDFium 로딩 + 페이지 렌더링 |
| `crates/freedf/src/app.rs` | 뷰어 캔버스, 도구, 툴바, 단축키, 파일 IO |
| `crates/freedf/src/export.rs` | 주석을 이미지 위에 래스터라이즈 → PNG |

## 라이선스

MIT (FreeDF)

