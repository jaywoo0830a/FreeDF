# AI 디자인 개선 가이드 — 에이전트가 "직접 보고" 판단하게 만들기

> **대상**: 이 저장소를 처음 클론한 개발자. **VS Code + Cline 확장은 이미 설치**되어 있다고 가정합니다.
> **목표**: 에이전트가 스크린샷(렌더된 실제 프레임)과 측정값을 근거로 디자인의 장단점을 말하고,
> 개선 → 재측정 → 회귀 확인까지 스스로 돌게 만듭니다.
> **소요**: STEP 1~6 약 20분. 이후 STEP 7 루프를 반복합니다.

핵심 아이디어는 하나입니다. 화면을 긁는(screen-scrape) 게 아니라
**[eguidev](https://github.com/cortesi/eguidev)가 앱 안에서 프레임을 캡처해 이미지로 돌려주고**,
동시에 **위젯 기하·레이아웃 문제·색 대비를 수치로** 줍니다. 그래서 에이전트가
"예쁜 것 같습니다"가 아니라 "그룹 간격 24px/8px는 일관되지만 토스트가 Settings 버튼을 덮습니다"
처럼 말할 수 있습니다.

---

## 0. 해야 할 순서 (30초 요약)

**순서를 바꾸지 마세요.** 각 단계가 다음 단계의 실패 원인을 하나씩 제거합니다.

| # | 단계 | 명령(요약) | 성공 신호 |
|---|---|---|---|
| 1 | 기본 빌드 (자동화 없이) | `cargo check -p freedf` | 에러 0 |
| 2 | (선택) 서버 연결 | `server/db` + `server/backend` → 앱 Settings | "first run setup" 모달이 안 뜸 |
| 3 | 자동화 계측 확인 | `scripts/edev-run.sh dump` | 위젯 트리 텍스트 출력 |
| 4 | 캡처해서 눈으로 보기 | `scripts/edev-run.sh eval scripts/design-shot.luau --out-dir tmp/eguidev-screenshots` | `*.jpg` 생성 |
| 5 | Cline에 MCP 연결 | `cline_mcp_settings.json` + Reload Window | 도구 목록에 `eguidev-freedf` |
| 6 | 디자인 감사 | `scripts/edev-run.sh eval scripts/design-audit.luau …` | `success: true` JSON + 이미지 |
| 7 | 개선 루프 | 감사 → 수정 → **같은 각도로 재감사** → 스모크 | 지표가 의도대로 변함 |

> 왜 이 순서인가: ①이 깨지면 자동화 문제와 코드 문제를 구분할 수 없고, ③이 깨지면
> ④의 실패 원인이 "렌더"인지 "계측"인지 모릅니다. ④를 눈으로 확인하지 않고
> ⑤(MCP)로 가면 에이전트 설정 문제인지 계측 문제인지 알 수 없습니다.

---

## 1. 사전 조건

| 도구 | 요구 사항 | 확인 |
|---|---|---|
| Rust | **≥ 1.85** (포크가 edition 2024), 검증 환경 1.98.1 | `rustc --version` |
| VS Code + Cline | 설치되어 있다고 가정(이 가이드는 **설치**를 다루지 않음) | Cline 패널 열기 |
| `edev` CLI | 아래 3단계에서 설치(포크 리비전 고정) | `edev --help` |
| Linux **헤드리스**만 | Xvfb + xcb/GL 라이브러리 | 아래 참조 |
| (선택) 서버 | PostgreSQL 18.6 + 미디어 API (Docker) | `server/README.md` |

```bash
# Linux 헤드리스(VS Code Server/SSH/CI)에서만 필요합니다.
sudo apt-get install -y xvfb libxcb1 libxkbcommon-x11-0 libxcursor1 \
                        libxrandr2 libxi6 libgl1-mesa-dri
```

> **Windows / macOS 개발자**: DISPLAY가 필요 없고 Xvfb도 필요 없습니다.
> `scripts/edev-run.sh`·`scripts/edev-mcp.sh`는 **Linux에서 DISPLAY와 WAYLAND_DISPLAY가
> 모두 없을 때만** Xvfb를 띄우므로 그대로 써도 됩니다.

---

## 2. STEP 1 — 클론 & 기본 빌드

```bash
git clone https://github.com/jaywoo0830a/FreeDF.git
cd FreeDF

# 자동화 기능 없이 먼저 됩니다. (dev-automation은 기본 빌드에 포함되지 않습니다)
cargo check -p freedf

# 디스플레이가 있는 환경이면 실제로 띄워 봅니다.
cargo run -p freedf
```

**이 단계에서 확인할 것**
- `dev-automation`을 켜지 않아도 앱이 정상 동작해야 합니다. 계측 헬퍼는 기능이 꺼져
  있으면 전부 no-op이므로, 평소 실행의 동작/성능에 영향이 없습니다.
- 처음 실행하면 **"FreeDF — first run setup"** 모달이 뜹니다(서버 미설정 상태의 정상 동작입니다).
  이 모달을 닫고 넘어가도 자동화 실습에는 문제가 없습니다.

---

## 3. STEP 2 — (선택) 서버를 붙여 첫 실행 모달 없애기

노트/PDF를 여는 시나리오를 자동화하려면 서버가 필요합니다(모든 데이터가 PostgreSQL에 있습니다).

```bash
cd server/db && ./init.sh && ./up.sh          # PostgreSQL 18.6 + 스키마
cd ../backend && ./init.sh                    # .env 생성(API 키 자동 생성)
```

앱에서 **Settings → 서버 주소 + API 키**를 입력하면 다음 위치에 저장됩니다.

| OS | `server.json` 경로 |
|---|---|
| Windows | `%LOCALAPPDATA%\FreeDF\server.json` |
| Linux/macOS | `~/.local/share/freedf/server.json` |

자세한 내용은 `server/README.md`. 서버 없이도 디자인 리뷰(스크린샷/측정)는 전부 가능합니다.

---

## 4. STEP 3 — EDEV 설치 & 자동화 계측 확인

```bash
# 포크에서 설치 — 브랜치는 지워질 수 있으므로 반드시 rev(SHA)로 고정합니다.
cargo install --git https://github.com/jaywoo0830a/eguidev \
              --rev 84ab2da60b36fa5f4235c0792b78e5535b95700a edev
```

```bash
scripts/edev-run.sh dump        # 위젯 트리
scripts/edev-run.sh smoke       # smoketest/ 스위트
scripts/edev-run.sh fixtures    # 등록된 픽스처 목록(현재 없음)
```

**성공 신호**

```
viewport root "" 1280x820 frame=7
  freedf.root unknown [0,0 1280x820]
    tabs.new_note button "New Note" [16,6 110.4x28]
    toolbar.panel.library toggle "Library" value=true [127.1,43 89.3x32] selected
    toolbar.tool.pen button "Pen" [8,171 32x32] selected
    canvas.surface unknown [17,257 1246x546]
...
[PASS] 10_launch.luau (217ms)
[PASS] 20_ink_tool_picker.luau (254ms)
```

`dump`가 위젯을 못 보여주면 **자동화 문제**입니다. 이때는 `docs/eguidev-automation.md`의
"왜 포크를 쓰는가 / 계측 지점" 절과 트러블슈팅(§12)을 보세요. **여기서 멈추고 해결**하는 게
중요합니다 — 스크린샷도 이 계측 위에서 돌아갑니다.


---

## 5. STEP 4 — 캡처해서 "내 눈으로" 먼저 보기

MCP를 붙이기 **전에** 사람이 한 번 봅니다. 그래야 이후 에이전트가 이상한 말을 할 때
"이미지가 잘못 나온 것"과 "해석이 틀린 것"을 구분할 수 있습니다.

```bash
# 뷰포트 전체
scripts/edev-run.sh eval scripts/design-shot.luau --out-dir tmp/eguidev-screenshots

# 특정 위젯만 크롭 (가장 저렴한 "집중 시각 자료")
scripts/edev-run.sh eval scripts/design-shot.luau --out-dir tmp/eguidev-screenshots \
    --arg widget=canvas.surface
```

출력 JSON의 `images[].file`이 저장 경로입니다(절대 경로).

```json
{"images":[{"file":"…/tmp/eguidev-screenshots/design-shot-img_0.jpg",
            "kind":"widget","rect":{"min":{"x":17,"y":257},"max":{"x":1263,"y":803}},
            "target":{"id":"canvas.surface","viewport_id":"root"}}],
 "success":true}
```

VS Code 탐색기에서 `tmp/eguidev-screenshots/*.jpg`를 열거나, 작업 표시줄의
**Tasks: Run Task → "edev: 화면 캡처 (스크린샷 파일)"** 을 쓰세요.

> 이 리비전의 `screenshot()`은 **JPEG(장변 ≤1600px)** 로 고정입니다(포맷 옵션은 더 최신
> 리비전에만 있습니다). 고해상도가 필요하면 위젯 크롭을 쓰세요.

---

## 6. STEP 5 — Cline에 MCP 서버 연결 (에이전트가 이미지를 받는 지점)

MCP로 연결하면 `script_eval` 응답에서 **도달 가능한 모든 `ImageRef`가 이미지 콘텐츠
블록**으로 에이전트에게 전달됩니다. 즉 에이전트가 스크린샷을 "읽습니다".

### 6-1. 설정 파일 위치

| 환경 | 경로 |
|---|---|
| VS Code Server(SSH) | `~/.vscode-server/data/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json` |
| 데스크톱 VS Code (Linux) | `~/.config/Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json` |
| 데스크톱 VS Code (macOS) | `~/Library/Application Support/Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json` |
| 데스크톱 VS Code (Windows) | `%APPDATA%\Code\User\globalStorage\saoudrizwan.claude-dev\settings\cline_mcp_settings.json` |

### 6-2. 넣을 내용

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

- **`command`는 `edev`가 아니라 래퍼**입니다. 이유는 세 가지입니다.
  1. VS Code Server의 확장 호스트에는 `DISPLAY`가 없어 Xvfb가 필요합니다 — 래퍼가 대신 띄웁니다.
  2. MCP 클라이언트의 cwd가 워크스페이스가 아닐 수 있어 `.edev.toml`을 못 찾습니다 — 래퍼가 `--cwd`를 고정합니다.
  3. MCP stdio는 **stdout이 곧 JSON-RPC**입니다 — 래퍼는 진단을 stderr로만 보냅니다.
- `autoApprove`는 편의 설정입니다. `script_eval`을 빼면 캡처마다 승인 버튼을 눌러야 합니다.
  (스크립트 샌드박스는 파일/네트워크 접근이 없습니다. 조직 정책에 맞게 조정하세요.)
- **모델이 vision-capable**이어야 합니다(Claude Sonnet/Opus, GPT-4o 계열 등).
  텍스트 전용 모델이면 이미지 블록이 무시됩니다.

### 6-3. 적용 & 확인

1. VS Code에서 **Reload Window** (Cline이 시작 시 설정을 읽습니다).
2. Cline → MCP Servers 목록에 `eguidev-freedf`가 보이는지 확인.
3. 첫 호출: `start` → 앱이 뜨고, `script_api`/`script_eval`로 위젯 트리가 반환되는지 확인.

이 저장소에는 이미 들어 있습니다.
- `.clinerules/eguidev-visual-review.md` — "추측 금지, 캡처로 근거를 만들라"는 규칙 + API 요약
- `.vscode/mcp.json` — VS Code 네이티브 MCP 클라이언트(Copilot 등)용 등록(`${workspaceFolder}` 사용)
- `.vscode/tasks.json` — 캡처/감사/덤프/스모크 작업
- `.vscode/settings.json` — `*.luau` 연결, `target/`·`.edev-instances` 제외, clippy, 터미널 GL 환경

---

## 7. STEP 6 — 디자인 감사 실행 & 지표 읽기

```bash
scripts/edev-run.sh eval scripts/design-audit.luau --out-dir tmp/eguidev-screenshots
```

반환값(에이전트는 MCP `script_eval`로 같은 걸 받습니다):

| 키 | 내용 | 판단에 쓰는 법 |
|---|---|---|
| `image` | 실제 프레임 | 최종 판단은 항상 이미지로 |
| `widgets[]` | id/role/label/x/y/w/h/enabled/selected | 정렬·간격·크기 일관성 |
| `layout_issues[]` | overlap/clipping/overflow/zero_size/text_truncation/offscreen | **형제 스코프**만 판정(§11 주의) |
| `small_targets[]` | 인터랙티브인데 24pt 미만 | 접근성/터치 오류 |
| `disabled[]` | 비활성 어포던스 | 상태 표현이 적절한지 |
| `palette[]` | 화면 그리드 샘플링 색 빈도(RGB, share) | 테마 일관성, **강조색 비중** |
| `contrast[]` | 위젯별 `bg`(최빈색)·`fg`(최대 편차)·`ratio`·`aa_body`·`aa_ui`·`distinct` | WCAG AA 판정 |

**판독 기준**
- 본문 텍스트: `ratio ≥ 4.5` (`aa_body`), 큰 글자·UI 구성요소: `≥ 3.0` (`aa_ui`).
- `distinct: false` → 그 위젯 안에서 글자/아이콘 픽셀이 안 잡힘 = **수치 무효**, 이미지로 판단.
- 그룹 간격은 `widgets[]`의 x/y로 직접 검산합니다(예: Row1 그룹 내 8px, 그룹 간 24px).
- 팔레트는 "비율"이지 "좋다/나쁘다"가 아닙니다. 강조색 share가 1% 미만이면 위계가 약하다는 **신호**로 읽습니다.

인자로 조정할 수 있습니다: `--arg grid=64x40`(팔레트 해상도), `--arg contrast_max=30`(대비 측정 위젯 수).

---

## 8. STEP 7 — 개선 루프 (이 순서를 지키세요)

```
① 감사 실행            scripts/edev-run.sh eval scripts/design-audit.luau …
② 근거 정리            "무엇이 몇 px / 무슨 색 / 어떤 비율인가"를 먼저 적는다
③ 문제 정의            관찰 → 원인 후보 → 검증 방법 (추측과 사실을 구분)
④ 계획 → 승인          어떤 코드를 바꾸고, 어떤 지표가 어떻게 변해야 하는지 예측
⑤ 수정                계측 id는 유지한 채 값/배치만 바꾼다
⑥ 같은 각도로 재감사    동일 커맨드·동일 창 크기로 다시 측정 (예측과 비교)
⑦ 회귀 확인            scripts/edev-run.sh smoke  → [PASS] 전부
⑧ 커밋                §14 체크리스트
```

**루프에서 지킬 규칙**
- **④에서 "예측"을 먼저 적으세요.** "토스트를 우측 하단으로 옮기면 Row1 우측 버튼의
  interact_rect가 토스트에 가려지지 않고, `layout_issues`는 그대로 비어 있을 것이며
  이미지에서 겹침이 사라진다"처럼 쓰면, ⑥에서 맞는지 틀린지 판정할 수 있습니다.
- **한 번에 한 축만 바꾸세요.** 색과 간격을 동시에 바꾸면 어떤 변화가 어떤 지표를
  움직였는지 알 수 없습니다.
- **⑧ 이전에는 항상 ⑦을 돌리세요.** 툴바/캔버스는 클릭 좌표가 계약이라 배치를 바꾸면
  스모크가 깨질 수 있습니다(그게 스모크의 목적입니다).
- **`edev dump`를 커밋 메시지 근거로 남기세요.** before/after `dump` 텍스트를 붙이면
  리뷰어가 px 단위 변화를 바로 확인합니다.

**현재 상태 예시(이 저장소를 이 가이드 순서로 돌렸을 때 실제로 나온 값)**

| 관찰 | 근거 | 유형 |
|---|---|---|
| 그룹 간격이 일관됨(그룹 내 8px, 그룹 간 24px) | `widgets[]` x좌표 검산 | 장점 |
| 활성/비활성 대비가 명확(7.45 / 2.98) | `contrast[]` | 장점 |
| 모든 인터랙티브 컨트롤 ≥24pt | `small_targets: []` | 장점 |
| **토스트가 Row1 우측 버튼을 덮음** | 이미지(레이어 겹침은 형제 스코프 밖) | 단점 |
| **강조색 비중 0.6%**(`#81a1c1` 1440샘플 중 9개) | `palette[]` | 단점 |
| 행 간격 34px vs 30px 불일치 | Row1(43–75)→Row2(109) vs Row2(141)→Row3(171) | 단점 |
| 캔버스에 빈 상태 안내 없음 | 이미지 + `widgets[]`(canvas.surface만 존재) | 단점 |

---

## 9. STEP 8 — 새 계측이 필요할 때 (`dev.rs`)

기존 id로 부족하면 계측을 추가합니다. **`crates/freedf/src/app/dev.rs`의 헬퍼만** 쓰세요
(호출부에 `#[cfg]`가 필요 없고, 기능이 꺼지면 no-op입니다).

| 헬퍼 | 용도 |
|---|---|
| `tag_button_with(ui, id, label, add)` | 커스텀 버튼을 **그리면서** 등록 |
| `tag_button(ui, id, label, &resp)` | 이미 얻은 `Response`를 버튼으로 등록 |
| `tag_selected_button(ui, id, label, &resp, selected)` | 선택 상태가 있는 버튼 |
| `tag_toggle(ui, id, label, &resp, value)` | 토글 |
| `tag(ui, id, add)` | 역할을 모르는 커스텀 위젯 |
| `publish_rect(ui, id, rect)` | painter로 그린 영역(캔버스 등) |

**규칙 4가지**
1. **한 프레임에 같은 id를 두 번 등록하지 마세요.** eguidev가 중복 id를 계측 결함으로
   잡고 **자동화를 멈춥니다**. 그래서 "그리기 + 등록"은 `tag_button_with`처럼 한 번에 합니다.
2. **id는 공개 계약입니다.** `toolbar.undo`, `canvas.surface`처럼 `영역.대상` 형식으로
   짓고, 라벨·문구를 바꿔도 id는 유지하세요(`docs/eguidev-automation.md`의 표가 기준).
3. **`name_viewport("root")` 같은 호출은 하지 마세요.** 루트 뷰포트는 eguidev가 암묵적으로
   `root`로 두는 **예약어**입니다.
4. **선택/토글 상태를 꼭 실어 주세요.** `selected`와 `Toggle + Bool value`가 없으면
   스크립트가 `{ selected = true }`로 대기/단언할 수 없습니다.

추가 후에는:
```bash
cargo check -p freedf --features dev-automation   # 계측 코드가 실제로 컴파일되는지
scripts/edev-run.sh dump                          # 새 id가 트리에 보이는지
scripts/edev-run.sh smoke                         # 기존 시나리오가 안 깨졌는지
```

---

## 10. 무엇을 "좋은 디자인"으로 볼 것인가 (이 리포의 권위 문서)

에이전트에게 판단 기준을 주지 않으면 취향대로 말합니다. 이 저장소의 기준은 문서입니다.

| 문서 | 역할 |
|---|---|
| `docs/DESIGN-TOOLBAR.md` | **설계 권위**. "하나의 액션 = 한 곳(Single Home)", 2중 배치 예외, 컴포넌트 분리 원칙 |
| `docs/UI-COMPONENTS.md` | 컴포넌트 계층(`ui::layout` / `components` / `ds` / `form` ↔ React/Bootstrap 대응) |
| `docs/UI-UX-REPORT.md` | 진단 보고서(툴바 과밀, 정보 그룹핑 혼선 등) — **개선 우선순위의 출발점** |
| `docs/eguidev-automation.md` | 자동화 계약(계측 id, 헬퍼, MCP, 트러블슈팅) |

개선 제안은 이 문서들의 원칙에 근거해야 하고, 원칙과 충돌하면 **문서를 먼저 갱신**한 뒤
코드를 바꾸세요(문서가 "단일 진실 공급원"입니다).

---

## 11. 지표 오탐 주의 (에이전트가 잘못 판단하는 지점)

측정값을 맹신하면 오히려 잘못된 개선을 합니다. 아래는 **실제로 겪은 함정**입니다.

| 함정 | 실제 사례 | 올바른 사용법 |
|---|---|---|
| `layout_issues`는 **형제 스코프**만 본다 | 토스트가 우측 상단 버튼을 덮는데 `layout_issues: []` | 레이어를 넘는 겹침은 **이미지로** 판단. `widgets[]` 사각형끼리 교차 검산도 가능 |
| 한 점 샘플링은 빈 공간을 집는다 | 중심 1점 vs 모서리 1점 방식에서 `Bookmarks` 대비가 1.0(거짓) | `design-audit.luau`는 위젯 내부를 그리드로 훑어 배경=최빈색/전경=최대편차로 계산 → 7.45(정상) |
| `distinct: false`면 수치 무효 | 텍스트/아이콘이 그리드에 안 걸린 위젯 | 그 위젯은 이미지로 판단 |
| 계측 커버리지 갭 | Row1 우측 끝 여백 45px(좌측 8px) → **id 없는 버튼이 존재** | id를 추가하거나, 이미지에서 확인. 없는 위젯은 어떤 지표에도 안 나온다 |
| 캡처 타이밍 | 첫 프레임/전환 직후 캡처 | `viewport:wait_capture()`, `eguidev.wait_frames(n)`, 상태 변경 후 `:wait({...})` |
| JPEG + 1600px 제한 | 큰 화면에서 작은 글자 판독 한계 | 위젯 크롭(`widget:screenshot()`)으로 필요한 부분만 |
| 팔레트는 "비율" | "강조색이 0.6%"가 곧 나쁨은 아님 | 위계 의도와 비교해 해석(의도적으로 절제한 accent일 수도) |
| 판정 기준은 문서 | "예쁘다"는 판단 | `docs/DESIGN-TOOLBAR.md`의 원칙 위반 여부로 판단 |

---

## 12. 트러블슈팅

| 증상 | 원인 | 해결 |
|---|---|---|
| `error: duplicate key: ruau` (포크 빌드) | 포크의 `0.1.0`/`main`이 병합 사고 상태 | **SHA 핀을 쓰세요**(§4 커맨드). 그 브랜치로 갈아타지 마세요 |
| `error inheriting 'ruau-script-api' … was not found` | 해당 브랜치가 비공개 크레이트 의존 | `84ab2da…`로 고정 (최신 기능이 필요하면 문서 §"0.1.0 경고" 참조) |
| `edev: command not found` | CLI 미설치 | §4 설치 명령(rev 고정) |
| `DISPLAY` 없음 / 창이 안 뜸 | 헤드리스 Linux | `scripts/edev-run.sh`(또는 `edev-mcp.sh`)를 통해 실행 + §1 패키지 설치 |
| 앱은 뜨는데 프레임이 안 움직임 | wgpu 백엔드 유휴 프레임 정지 | `FREEDF_RENDERER=glow`(래퍼가 기본 설정), `LIBGL_ALWAYS_SOFTWARE=1` |
| `instrumentation_fault` (중복 id) | 한 프레임에 같은 계측 id 2회 등록 | `tag_button_with`/`tag`로 그리기+등록을 한 번에. §9 규칙 1 |
| `not_found: widget <id>` | 그 프레임에 없거나 `visible=false` | `scripts/edev-run.sh dump`로 id 확인(오타/커버리지), `:wait()` 사용 |
| `timeout` | 첫 프레임/서버 연결이 느림 | `.edev.toml`의 `script_timeout_secs`(현재 120) 상향 |
| "first run setup" 모달이 계속 뜸 | `server.json` 미설정 | §3. 서버 없이 캡처만 할 거면 모달을 닫고 진행 |
| PDF가 렌더되지 않음 | PDFium 미설치 | GUI는 뜹니다(지연 로드). 실행 파일 옆 `libpdfium.so`/`pdfium.dll` 필요(`scripts/install-pdfium.ps1`) |
| 종료가 유예 시간 초과로 강제 종료 | 종료 확인 창 | 자동화 실행에서는 `dev::automation_active()`로 창을 건너뜁니다. 그 로직을 건드렸다면 복구 |
| `jq`로 `edev eval` 파싱 실패 | stdout에 진단 메시지가 섞임 | 최신 `scripts/edev-run.sh`는 진단을 stderr로 보냅니다(`2>/dev/null` 사용 가능) |
| Cline에 MCP 도구가 안 보임 | 설정 미반영/JSON 오류/서버 실패 | JSON 유효성 검사 → **Reload Window** → `scripts/edev-mcp.sh mcp`를 터미널에서 직접 실행해 stderr 확인 |
| MCP 도구 목록에 `script_eval`이 없음 | 앱 수준 도구는 `start` 이후 노출 | 먼저 `start` 호출(초기 도구는 `start/stop/restart/status`) |
| 에이전트가 이미지를 못 봄 | 텍스트 전용 모델 / `ImageRef`가 반환값에 없음 | 비전 모델 사용. `return { image = vp:screenshot() }`처럼 **반환 테이블에 담기** |
| `tmp/eguidev-screenshots`가 비어 있음 | `--out-dir` 미지정/경로 오타 | 출력 JSON의 `images[].file` 절대경로 확인 |
| 캡처가 검은 화면 | Xvfb 크기/GL | `-screen 0 1600x1000x24`, `LIBGL_ALWAYS_SOFTWARE=1` |
| `.edev-instances/` 관련 이상 | EDEV 소유 상태 | **읽기·편집·삭제 금지** |
| `Cargo.lock`이 스모크 실행으로 바뀜 | 의존성 드리프트 | `.edev.toml`의 `--locked` 유지, 커밋 금지 |

---

## 13. 파일 지도 (이 워크플로에 관련된 것만)

| 경로 | 역할 |
|---|---|
| `crates/freedf/src/app/dev.rs` | **계측 헬퍼**(no-op 분기 포함) — 새 계측은 여기에 |
| `crates/freedf/src/app/toolbar/ribbon.rs` | 툴바 버튼/토글/도구 선택기 계측 |
| `crates/freedf/src/app/tabs.rs` | 탭바(New Note / Open PDF / 탭 제목) 계측 |
| `crates/freedf/src/app/canvas/mod.rs` | `canvas.surface` 공개 |
| `crates/freedf/src/app/mod.rs` | `DevMcp` 소유, `frame_scope`, 자동화 시 종료 창 우회 |
| `.edev.toml` | 런처 설정(실행 커맨드, `--locked`, `dev-automation`, 타임아웃) |
| `scripts/edev-run.sh` | 헤드리스 실행 래퍼(Xvfb, 소프트웨어 GL) |
| `scripts/edev-mcp.sh` | MCP stdio 래퍼(Xvfb + `--cwd`, stdout 프로토콜 전용) |
| `scripts/design-shot.luau` | 뷰포트/위젯 캡처 |
| `scripts/design-audit.luau` | 이미지 + 기하/레이아웃/팔레트/대비 감사 |
| `smoketest/*.luau` | 회귀 시나리오(`edev smoke`) |
| `.clinerules/eguidev-visual-review.md` | Cline 규칙(에이전트 행동 지침) |
| `.vscode/{mcp,tasks,settings}.json` | MCP 등록 / 작업 / 편집기 설정 |
| `docs/eguidev-automation.md` | 자동화 계약 문서(계측 id 표) |
| `docs/DESIGN-TOOLBAR.md` 외 | 디자인 판단 기준 |
| `tmp/eguidev-screenshots/` | 캡처 산출물(gitignore) |

---

## 14. 커밋 전 체크리스트

- [ ] `cargo check -p freedf` (기본 빌드 — 자동화 없이도 컴파일)
- [ ] `cargo check -p freedf --features dev-automation` (계측 코드 컴파일)
- [ ] `scripts/edev-run.sh smoke` → **모든 `[PASS]`**
- [ ] 같은 창 크기·같은 스크립트로 **before/after 캡처** 비교(이미지)
- [ ] `design-audit` 지표의 **예측 vs 실제**를 기록(어떤 값이 왜 변했는지)
- [ ] 계측 id를 바꿨다면 `docs/eguidev-automation.md` 표도 함께 갱신
- [ ] 디자인 원칙과 충돌하는 변경이면 `docs/DESIGN-TOOLBAR.md`를 먼저/함께 수정
- [ ] `Cargo.lock` 변경 없음(`--locked` 실행 유지)
- [ ] 커밋 메시지에 **근거** 포함: 핵심 수치(간격/대비/색 비중)와 스크린샷 경로, `dump` before/after 조각

---

## 15. 다음 단계(선택)

1. **픽스처 추가** — 지금은 모든 캡처가 앱 시작 상태에서 출발합니다. `DevMcp::fixtures(...)` +
   `on_fixture_ui(...)`를 등록하면 `eguidev.fixture("design.baseline")`로 **고정된 기준 화면**을
   만들 수 있어 before/after 비교가 훨씬 정확해집니다.
2. **실패 번들 활용** — `scripts/edev-run.sh smoke --bundle`로 실패 시 스크린샷/위젯 트리가
   묶인 번들을 남겨 리뷰 근거로 첨부하세요.
3. **사람용 녹화** — `edev record out.mov`(이 리비전에 있음)로 스모크 실행을 창 녹화할 수
   있습니다(움직임/애니메이션 리뷰용).
4. **포크 태그 만들기** — SHA 핀은 안전하지만 브랜치가 사라지면 불안합니다. 포크에
   `freedf-egui036` 태그를 만들어 두면 재현성이 더 좋아집니다(`docs/eguidev-automation.md` 참조).


