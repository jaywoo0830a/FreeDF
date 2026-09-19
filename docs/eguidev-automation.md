# eguidev — freedf-gui 자동화 계약 (최소 문서)

[eguidev](https://github.com/cortesi/eguidev)는 egui 앱용 인프로세스 자동화입니다.
화면 픽셀을 긁지 않고 **실제 위젯 상태**를 읽으며, 앱이 직접 프레임을 캡처해
이미지를 돌려줍니다. 이 문서는 freedf-gui(`crates/freedf-gui`)에 필요한 것만
남긴 최소 계약입니다. freedf(구 앱)는 컴파일만 유지하는 레거시라 자동화 계약을
더 이상 문서화하지 않습니다.

## 켜기

```bash
# EDEV CLI — 포크를 SHA로 고정 (브랜치는 옮겨질 수 있음)
cargo install --git https://github.com/jaywoo0830a/eguidev \
              --rev 84ab2da60b36fa5f4235c0792b78e5535b95700a edev

# 계측이 켜진 freedf-gui (기본 빌드에는 포함되지 않음)
cargo run -p freedf-gui --features dev-automation
```

`EGUIDEV_MCP_ADDR`가 없는 실행에서는 계측이 inert라 서버가 뜨지 않습니다 — 평소
실행은 동작/성능이 그대로입니다. 헤드리스 Linux는 `scripts/edev-run.sh`가 Xvfb와
소프트웨어 GL(`LIBGL_ALWAYS_SOFTWARE=1`)을 대신 설정합니다.

```bash
scripts/edev-run.sh --config .edev-gui.toml dump            # 위젯 트리 텍스트
scripts/edev-run.sh --config .edev-gui.toml smoke           # smoketest-gui/ 스위트
scripts/edev-run.sh --config .edev-gui.toml eval tmp/probe.luau --out-dir tmp/out
```

`edev-run.sh`는 `flock`으로 실행을 **직렬화**합니다. edev는 실제 창을 쓰므로 두
인스턴스를 동시에 돌리면 서로의 프레임/스크린샷을 깨뜨립니다.

## 계약 id (`crates/freedf-gui`, 런처 `.edev-gui.toml`)

id는 **스크립트가 의존하는 공개 계약**입니다. 라벨은 바꿔도 id는 유지하고,
한 프레임에 같은 id를 두 번 등록하지 마세요(자동화가 멈춥니다).

| id | 대상 |
|---|---|
| `freedf-gui.root` | 루트 프레임 스코프 |
| `gui.<라벨 슬러그>` | 어댑터(elm-magic)가 그린 버튼/탭 — 라벨 소문자 + 공백→`_`(`save_edits`). 같은 라벨이 한 프레임에 두 번 이상이면 `.<n>` 접미사 (`gui.untitled`, `gui.untitled.1`) |
| `canvas.surface` | 잉크 캔버스 영역 |

`smoketest-gui/10_launch_gui.luau`가 검증하는 계약 id(바 이름: TopBar → InkBar 1·2줄
→ ViewBar 1·2줄 → Statusbar):
`gui.new_tab` · `gui.close_tab` · `gui.open_pdf` · `gui.settings` · `gui.about` ·
`gui.undo` · `gui.redo` · `gui.save_edits` · `gui.load_edits` · `gui.zoom_in` ·
`gui.zoom_out` · `gui.fit` · `gui.prev_page` · `gui.next_page` · `gui.pen` ·
`gui.fountain` · `gui.highlighter` · `gui.eraser` · `gui.swatch_1` … `gui.swatch_3` ·
`gui.thin` · `gui.medium` · `gui.thick` · `gui.pressure` · `gui.sidebar` ·
`gui.bookmarks` · `gui.outline` · `gui.bookmark` · `gui.clear_ink` ·
`canvas.surface`(+ 탭 `gui.untitled`).

배치(디자인 사양 `docs/DESIGN-SYSTEM.md` §6.1):

| 바 | 항목 |
|---|---|
| TopBar (1줄) | 브랜드 · 탭 · New/Close/Open PDF · Settings/About |
| InkBar 1줄 | Pen/Fountain/Highlighter/Eraser · Swatch 1-3 |
| InkBar 2줄 | Undo/Redo · Thin/Medium/Thick/Pressure |
| ViewBar 1줄 | Zoom In/Out/Fit/Prev/Next Page |
| ViewBar 2줄 | Save/Load Edits · Bookmark · Clear Ink · Sidebar/Bookmarks/Outline |
| Statusbar | 상태 텍스트(자동화 `assert_text` 대상 — painter가 아니라 트리 노드) |

**팔레트 스와치**: 라벨 `Swatch N`(1-기반)이 곧 id입니다(`gui.swatch_N`). 목록은
`freedf-services::settings`의 즐겨찾기 색(`MAX_FAVORITE_COLORS` = 8 상한, 기본
3색 Black/Red/Blue)에서 옵니다. 활성 스와치도 **Button**이라 id가 있습니다
(`.swatch--on` 수정자 — `<Strong>`이 아닙니다).

**활성 상태 항목도 id가 있습니다**: `BtnOn`은 `<Strong>`이 아니라 `Button` +
`.btn--on` 수정자로 그려집니다 — `gui.medium`(굵기 Medium), `gui.sidebar`,
`gui.pressure`가 그대로 등록됩니다(감사 실측: `gui.medium` ratio 5.17로 측정됨).

`gui.pressure`는 필압 반영 토글입니다(장치 스트림이 없으면 값은 명목 1.0).
Settings 모달의 스무딩 프리셋(`gui.off`/`light`/`normal`/`strong`)은 모달이 열려
있을 때만 존재합니다 — 리본 굵기 라벨(`gui.medium`)과 겹치지 않게 고른 이름입니다.

| id | 대상 |
|---|---|
| `gui.undo` · `gui.redo` | 편집 행 — 되돌리기/다시 실행 |
| `gui.save_edits` · `gui.load_edits` | 편집 행 — 문서 주석 저장/불러오기 |
| `gui.settings` | 설정 모달 열기 |
| `gui.off` · `gui.light` · `gui.normal` · `gui.strong` | 스무딩 프리셋(1€ 필터 강도 0 / 0.25 / 0.4 / 0.7) — 모달이 열려 있을 때만 존재 |

> 프리셋 라벨은 리본 굵기(`gui.medium`)와 겹치지 않게 고릅니다(0.4 = `Normal`).

## 스크립트 (`script_eval`)

샌드박스에서 도는 strict Luau이고 전역은 `eguidev` 하나뿐입니다(파일/네트워크
접근 없음). 준비 → 캡처 → 측정을 한 번의 호출로 끝냅니다.

```lua
eguidev.root:wait_capture()
return {
  image = eguidev.root:screenshot(),          -- 응답에 이미지 블록으로 포함
  tree  = eguidev.dump_text({ fields = "core" }),
}
```

`ImageRef`가 **반환 테이블에서 도달 가능**해야 이미지 블록이 됩니다.

| 스크립트 | 목적 |
|---|---|
| `scripts/design-shot.luau` | 뷰포트 전체 또는 `widget` 인자 위젯 크롭 캡처 |
| `scripts/design-audit.luau` | 이미지 + 위젯 기하 + 레이아웃 문제 + 팔레트 + WCAG 대비 |

```bash
scripts/edev-run.sh --config .edev-gui.toml eval scripts/design-audit.luau \
  --out-dir tmp/eguidev-screenshots
```

`design-audit.luau` 반환 키: `image` · `widgets`(id/역할/라벨/사각형/활성) ·
`layout_issues`(overlap/clipping/overflow/zero_size/text_truncation/offscreen) ·
`small_targets`(24pt 미만 클릭 대상) · `disabled` · `palette`(화면 색 빈도) ·
`contrast`(WCAG 대비비 + `aa_body`/`aa_ui`). `contrast.distinct == false`면 글자
픽셀이 안 잡힌 것이므로 수치 대신 이미지를 보고 판단합니다.

읽을 때 주의: `contrast`는 **렌더된 픽셀**에서 나오므로 안티에일리어싱 때문에 이론값보다
낮게 나옵니다 — 수치가 경계선(4.5 근처)이면 이미지와 함께 판단합니다. `small_targets`는
elm-magic 0.7.4부터 `Button`/`Tab`도 CSS `padding`/`min-height`를 반영하므로 실제
클릭 크기입니다 (`docs/elm-magic-notes.md`).

## 알아 둘 점

- **렌더러**: glow를 쓰세요(`FREEDF_RENDERER=glow`). wgpu 백엔드는 특정 조합에서
  자동화 중 유휴 프레임이 멈춥니다.
- **MCP**: `scripts/edev-mcp.sh`가 stdio 래퍼입니다(Xvfb + `--cwd` 고정, stdout은
  프로토콜 전용). VS Code/Cline 설정은 `.vscode/mcp.json`에 있고, 에이전트 규칙은
  `.clinerules/eguidev-visual-review.md`입니다.
- **`.edev-instances/`는 EDEV 소유 상태**입니다 — 읽지도, 편집하지도, 지우지도
  마세요.
- **테스트는 코어/GUI가 나눠 갖습니다**: 코어 규약(필터·지우기 히트 기하·이력
  역연산·JSON 왕복)은 `freedf-core`가, 그 API를 어느 문서에 몇 단계로 부르는지는
  `crates/freedf-gui/tests/`가 검증합니다. 픽셀 판단이 필요한 경우에만 캡처를
  씁니다.
