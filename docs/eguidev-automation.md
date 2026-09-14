# eguidev — AI/에이전트가 FreeDF GUI를 직접 보고 조작하기

[eguidev](https://github.com/cortesi/eguidev)는 egui 앱용 인프로세스 자동화
라이브러리입니다(Playwright의 GUI판). 화면 픽셀이 아니라 **실제 위젯 상태**를
읽고, 좌표 기반 입력을 앱의 이벤트 루프에 주입합니다. 이 문서는 FreeDF에서
그걸 어떻게 켜고 쓰는지 설명합니다.

## 왜 포크를 쓰는가

| 항목 | 상황 |
|---|---|
| FreeDF | `egui 0.36.1` |
| crates.io `eguidev 0.1.0` (릴리스) | `egui ^0.35` — **호환 불가** |
| 포크의 `0.1.0` / `main` 브랜치 | egui 0.36 ✓ 이지만 지금 **빌드 불가**: ① `Cargo.toml`에 `ruau`/`tmcp` 키가 중복(머지 사고) ② 비공개 `ruau-script-api`를 `automation/mod.rs`·`mcp.rs`·`script_docs.rs`에서 import |
| 포크의 `freedf-egui036` (커밋 `84ab2da`) | ✅ FreeDF와 함께 검증된 리비전 |

> **`0.1.0` 브랜치로 갈아타지 마세요.** 그 브랜치는 최신 upstream 상태라 기능은
> 더 많지만(recording, `script_docs`, 뷰포트 스크린샷 포맷 옵션 등), 비공개
> 크레이트 `ruau-script-api`에 하드 의존합니다. 공개 `ruau`(0.4.0)에는
> `ScriptApiQuery` / `ScriptApiResponse` / `ScriptApiMode` / `ScriptApiErrorKind`
> 가 없어서 단순한 `use` 경로 수정으로는 해결되지 않습니다(심(shim) 크레이트나
> 해당 4곳의 공개 API 백포트가 필요). 워크스페이스 핀은 아래 SHA를 유지하세요.

그래서 리비전을 고정한 포크를 씁니다:
**https://github.com/jaywoo0830a/eguidev**, 브랜치 `freedf-egui036`.

포크가 바꾼 것은 세 가지뿐이고, 앱이 쓰는 API는 upstream과 같습니다.

1. 베이스 = upstream `a439da6` (2026-08-26) — **비공개 `ruau-script-api` 도입 직전**
   커밋이면서 egui 0.36을 쓰는 마지막 지점.
2. `ruau` / `tmcp`의 로컬 경로 의존성을 **같은 시점의 공개 저장소 리비전**으로 교체.
3. 비공개 ruau에만 있던 `ModuleBinding::LibraryOverride` 사용 2곳 제거.

커밋 `84ab2da60b36fa5f4235c0792b78e5535b95700a`를 **SHA 고정**합니다. 브랜치는
누구든 옮기거나 지울 수 있으므로 워크스페이스 핀은 항상 SHA로 두세요. (실제로
포크의 브랜치 목록에서 `freedf-egui036`이 사라져 커밋 객체로만 도달 가능한 적이
있습니다. 그때도 SHA fetch는 성공했습니다 — 확인 방법:
`git fetch <repo> 84ab2da…`.)


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

## 화면 캡처와 디자인 리뷰 (에이전트가 "직접 보게" 하기)

화면을 긁는 대신 **앱이 프레임을 캡처해 이미지로 돌려줍니다**. 같은 스크립트가
두 경로로 동작합니다.

| 경로 | 이미지가 가는 곳 | 용도 |
|---|---|---|
| CLI `edev eval … --out-dir DIR` | `DIR/*.jpg` 파일 + JSON의 `images[].file` | 사람이 파일을 열어보거나, 에이전트가 파일을 직접 읽을 때 |
| MCP `script_eval` | 응답의 **이미지 콘텐츠 블록** | 에이전트가 도구 결과에서 바로 봄 |

준비된 스크립트 두 개:

```bash
# 뷰포트 전체(또는 --arg widget=canvas.surface 로 위젯 크롭) 캡처
scripts/edev-run.sh eval scripts/design-shot.luau --out-dir tmp/eguidev-screenshots

# 이미지 + 측정값(위젯 기하 / 레이아웃 문제 / 팔레트 / WCAG 대비) 한 번에
scripts/edev-run.sh eval scripts/design-audit.luau --out-dir tmp/eguidev-screenshots
```

`design-audit.luau`가 돌려주는 것:

| 키 | 내용 |
|---|---|
| `image` | 프레임 캡처(에이전트가 직접 봄) |
| `widgets` | 보이는 위젯의 id/역할/라벨/사각형/활성 상태 |
| `layout_issues` | eguidev 판정: overlap / clipping / overflow / zero_size / text_truncation / offscreen |
| `small_targets` | 클릭 대상인데 24pt 미만인 컨트롤 |
| `disabled` | 비활성 어포던스 목록 |
| `palette` | 화면 그리드 샘플링 색 빈도(테마 일관성·강조색 비중) |
| `contrast` | 위젯별 배경(최빈색) vs 전경(최대 편차) WCAG 대비비 + `aa_body`/`aa_ui` |

주의: `contrast.distinct == false`면 그 위젯 안에서 글자/아이콘 픽셀이 잡히지
않았다는 뜻입니다(수치 대신 이미지를 보고 판단). `layout_issues`는 **같은 부모를
공유하는 형제** 사이의 판정만 하므로, 레이어를 넘는 겹침(예: 토스트가 툴바를
덮는 경우)은 이미지로 판단해야 합니다.

MCP 반환값은 `ImageRef`가 **반환 테이블에서 도달 가능**해야 이미지 블록이 됩니다:

```lua
eguidev.root:wait_capture()
return {
  image = eguidev.root:screenshot(),
  tree = eguidev.dump_text({ fields = "core" }),
}
```

## VS Code / Cline 설정

이 저장소에 필요한 설정은 이미 들어 있습니다.

| 파일 | 역할 |
|---|---|
| `.vscode/mcp.json` | VS Code 네이티브 MCP 클라이언트용 서버 등록 |
| `.vscode/tasks.json` | 캡처/감사/덤프/스모크 작업 (Tasks: Run Task) |
| `.vscode/settings.json` | `*.luau` 연결, `target/`·`.edev-instances` 제외, clippy 검사, 터미널 GL 환경 |
| `.clinerules/eguidev-visual-review.md` | Cline에게 "추측 대신 캡처하라"는 규칙과 API 요약 |
| `scripts/edev-mcp.sh` | MCP stdio 래퍼(Xvfb + `--cwd` 고정, stdout은 프로토콜 전용) |

### Cline

Cline은 전역 설정 파일을 씁니다(VS Code Server 기준):

```
~/.vscode-server/data/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json
```

데스크톱 VS Code라면 `~/.config/Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json`.

```json
{
  "mcpServers": {
    "eguidev-freedf": {
      "type": "stdio",
      "command": "/absolute/path/to/FreeDF/scripts/edev-mcp.sh",
      "args": ["mcp"],
      "env": { "LIBGL_ALWAYS_SOFTWARE": "1", "FREEDF_RENDERER": "glow" },
      "disabled": false,
      "autoApprove": ["start", "stop", "restart", "status", "script_api", "script_eval", "app_close"],
      "timeout": 300
    }
  }
}
```

`autoApprove`는 선택입니다. `script_eval`을 빼면 캡처마다 승인 버튼을 눌러야
합니다(샌드박스 Luau는 파일/네트워크 접근이 없습니다).

### 왜 `command`가 `edev`가 아니라 래퍼인가

1. VS Code Server(SSH)의 확장 호스트에는 **`DISPLAY`가 없습니다**. 래퍼가 Xvfb를
   대신 띄웁니다.
2. MCP 클라이언트의 cwd가 워크스페이스가 아닐 수 있어 `.edev.toml`을 못 찾습니다.
   래퍼가 `--cwd`로 저장소 루트를 고정합니다.
3. MCP stdio는 **stdout이 곧 JSON-RPC**입니다. 래퍼는 진단을 stderr로만 보냅니다.

### 에이전트 사용 흐름

```
start          → 앱 준비(헤드리스 Xvfb)
script_eval    → 캡처/측정 (이미지 블록이 응답에 포함)
script_api     → 등록된 진단 provider 읽기
stop           → 정리
```

터미널에서 바로 확인:

```bash
scripts/edev-run.sh smoke                              # 회귀
scripts/edev-run.sh eval scripts/design-shot.luau --out-dir tmp/eguidev-screenshots
scripts/edev-mcp.sh mcp                                # MCP 서버(stdio) 직접 띄우기
```
