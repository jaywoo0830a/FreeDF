# eguidev — AI/에이전트가 FreeDF GUI를 직접 보고 조작하기

[eguidev](https://github.com/cortesi/eguidev)는 egui 앱용 인프로세스 자동화
라이브러리입니다(Playwright의 GUI판). 화면 픽셀이 아니라 **실제 위젯 상태**를
읽고, 좌표 기반 입력을 앱의 이벤트 루프에 주입합니다. 이 문서는 FreeDF에서
그걸 어떻게 켜고 쓰는지 설명합니다.

## 왜 포크를 쓰는가

| 항목 | 상황 |
|---|---|
| FreeDF | `egui 0.36.1` |
| crates.io `eguidev 0.1.0` | `egui ^0.35` — **호환 불가** |
| upstream `main` | `egui 0.36` ✓ 이지만 의존성이 저자 로컬 경로(`/Users/cortesi/...`)이고 비공개 `ruau-script-api` 크레이트를 요구 → **외부에서 빌드 불가** |

그래서 리비전을 고정한 포크를 씁니다:
**https://github.com/jaywoo0830a/eguidev** 브랜치 `freedf`.

포크가 바꾼 것은 세 가지뿐이고, 앱이 쓰는 API는 upstream과 같습니다.

1. 베이스 = upstream `a439da6` (2026-08-26) — **비공개 `ruau-script-api` 도입 직전**
   커밋이면서 egui 0.36을 쓰는 마지막 지점.
2. `ruau` / `tmcp`의 로컬 경로 의존성을 **같은 시점의 공개 저장소 리비전**으로 교체.
3. 비공개 ruau에만 있던 `ModuleBinding::LibraryOverride` 사용 2곳 제거.

브랜치는 **`freedf-egui036`** 이고, 커밋은
`84ab2da60b36fa5f4235c0792b78e5535b95700a`로 **SHA 고정**합니다. 브랜치는
누구든 옮길 수 있으므로 워크스페이스 핀은 항상 SHA로 두세요.

## 켜기

```bash
# 1) EDEV CLI (포크에서 설치 — 브랜치는 옮겨질 수 있으므로 rev로 고정)
cargo install --git https://github.com/jaywoo0830a/eguidev \
              --rev 84ab2da60b36fa5f4235c0792b78e5535b95700a edev

# 2) 계측이 켜진 FreeDF 실행
cargo run -p freedf --features dev-automation
```

`dev-automation`은 **기본 빌드에 포함되지 않습니다**. 그리고 기능을 켜도
EDEV가 `EGUIDEV_MCP_ADDR`을 주입하지 않은 실행에서는 `DevMcp`가 inert라
서버가 뜨지 않습니다 — 평소 `cargo run`은 동작/성능이 그대로입니다.

## 헤드리스(Linux 서버/CI)

eframe/glow는 디스플레이가 필요하므로 Xvfb 위에서 돌립니다:

```bash
sudo apt-get install -y xvfb libxcb1 libxkbcommon-x11-0 libxcursor1 \
                        libxrandr2 libxi6 libgl1-mesa-dri

scripts/edev-run.sh dump        # 위젯 트리 텍스트 덤프
scripts/edev-run.sh eval tmp/probe.luau --out-dir tmp/out
scripts/edev-run.sh smoke       # smoketest/ 스위트
```

`scripts/edev-run.sh`는 Xvfb를 띄우고 `LIBGL_ALWAYS_SOFTWARE=1`로 소프트웨어
GL을 쓰게 한 뒤 `edev`를 실행합니다. 이미 `DISPLAY`가 있으면 그대로 씁니다.

## 계측 지점(자동화 계약 id)

id는 **스크립트가 의존하는 공개 계약**입니다. 라벨을 바꿔도 id는 유지하세요.

| id | 대상 |
|---|---|
| `freedf.root` | 루트 뷰포트 프레임 스코프 (뷰포트 이름은 eguidev가 암묵적으로 `root`로 둡니다 — `name_viewport`로 다시 지정하면 예약어라 계측 결함이 됩니다) |
| `toolbar.hide_ui` / `.settings` | Row 1 워크스페이스 |
| `toolbar.undo` / `.redo` / `.clear_page` | Row 1 편집 이력 |
| `toolbar.save_edits` / `.load_edits` | Row 1 파일 |
| `toolbar.panel.library` / `.outline` / `.bookmarks` / `.palette` | Row 1 패널 토글 |
| `toolbar.insert_page` / `.delete_page` | Row 2 페이지 |
| `toolbar.tool.pen` / `.fountain` / `.highlighter` / `.eraser` / `.pan` | Row 3 도구 선택기 |
| `tabs.new_note` / `.open_pdf` | 탭바 버튼 |
| `tabs.tab.<n>` | 탭 n개 (0-based) |
| `canvas.surface` | 페이지를 그리는 캔버스 영역 (painter 영역) |

계측은 `crates/freedf/src/app/dev.rs`의 얇은 헬퍼로만 합니다. 헬퍼마다
**기능이 꺼졌을 때의 no-op 분기**를 함께 제공하므로 호출부에 `#[cfg]`가 필요
없습니다.

| 헬퍼 | 쓰는 곳 |
|---|---|
| `tag_button_with(ui, id, label, add)` | 커스텀 버튼을 그리면서 등록 (탭바 New Note / Open PDF) |
| `tag_button(ui, id, label, &resp)` | 이미 얻은 `Response`를 버튼으로 등록 (툴바 `icon_button`) |
| `tag_selected_button(ui, id, label, &resp, selected)` | 선택 상태가 있는 버튼 (도구 선택기) |
| `tag_toggle(ui, id, label, &resp, value)` | 토글 (패널 표시/숨김) |
| `tag(ui, id, add)` | 역할을 모르는 커스텀 위젯 (탭 제목 라벨) |
| `publish_rect(ui, id, rect)` | painter로 그린 영역 (페이지 캔버스) |

주의: **한 프레임에 같은 id를 두 번 등록하면 안 됩니다** — eguidev가 중복 id를
계측 결함으로 잡고 자동화를 멈춥니다. 그래서 그리기와 등록은
`tag_button_with`/`tag`처럼 한 번에 하는 헬퍼를 쓰고, `tag_button`은 이미
`Response`를 가진 경우에만 씁니다.

```rust
// 커스텀 버튼: 그리기 + 등록을 한 번에
let resp = crate::app::dev::tag_button_with(ui, "tabs.new_note", "New Note", |ui| {
    ui.button("New Note")
});
// 이미 얻은 Response 등록
let resp = icon_button(ui, /* ... */);
crate::app::dev::tag_button(ui, "toolbar.undo", "Undo", &resp);
// 선택 상태가 있는 버튼 / 토글
crate::app::dev::tag_selected_button(ui, "toolbar.tool.pen", label, &resp, selected);
crate::app::dev::tag_toggle(ui, "toolbar.panel.library", "Library", &resp, self.show_library);
// painter로 그린 영역
crate::app::dev::publish_rect(ui, "canvas.surface", rect);
```

## 스크립트

스크립트는 샌드박스에서 도는 strict Luau이며, 전역은 `eguidev` 하나뿐입니다.
전체 API는 `edev docs`가 반환합니다.

```lua
eguidev.wait_viewport({ name = "root" })
eguidev.widget("toolbar.tool.eraser"):click()
eguidev.widget("toolbar.tool.eraser"):wait({ selected = true })
return { screenshot = eguidev.root:screenshot() }   -- 이미지 블록으로 반환
```

`edev smoke`는 `smoketest/`의 각 `.luau`를 독립 실행합니다. 현재:

- `10_launch.luau` — 창이 뜨고 탭바·3단 툴바·캔버스가 그려지는지 + 스크린샷
- `20_ink_tool_picker.luau` — 실제 클릭으로 도구 선택이 바뀌는지

## MCP로 에이전트에 연결

```toml
[mcp_servers.freedf-edev]
command = "edev"
args = ["mcp"]
```

에이전트는 `start`/`stop`/`restart`/`status` 뒤에 앱이 제공하는 `script_api`,
`script_eval`로 **한 번의 호출에 준비·조작·검증·보고**를 끝냅니다.

## 알아 둘 점

- **종료 확인 창**: FreeDF는 종료 시 "Save before quitting?" 창을 띄우지만,
  자동화 실행에서는 그 창을 건너뜁니다(`dev::automation_active()`). 그러지
  않으면 EDEV의 정상 종료가 유예 시간을 넘겨 강제 종료됩니다.
- **픽스처 없음**: 아직 `eguidev.fixture(...)`로 등록한 베이스라인이 없어
  스크립트는 앱의 시작 상태에서 출발합니다. 상태 리셋이 필요한 시나리오가
  생기면 `DevMcp::fixtures(...)` + `on_fixture_ui(...)`를 추가하세요.
- **DB 흐름**: Sync v3 서버가 없으면 앱은 "first run setup" 창과 함께
  disconnected 폴백으로 시작합니다. 노트/PDF를 여는 시나리오를 자동화하려면
  `server.json`에 도달 가능한 서버를 설정해야 합니다.
- **PDFium**: 없어도 GUI는 뜹니다(지연 로드). PDF 렌더링 검증에는 실행 파일
  옆에 `libpdfium.so` / `pdfium.dll`이 필요합니다.
- **렌더러**: glow를 쓰세요. wgpu 백엔드는 특정 조합에서 자동화 중 유휴 프레임이
  멈춥니다.
