# FreeDF 시각 리뷰 — 에이전트가 "눈으로 보고" 판단하는 방법

이 저장소에는 [eguidev](https://github.com/cortesi/eguidev) 인프로세스 자동화가
붙어 있습니다. **화면을 긁는(screen-scrape) 방식이 아니라** 앱이 직접 프레임을
캡처해 돌려주므로, 에이전트는 실제 렌더 결과를 이미지로 받습니다.

## 언제 쓰나

- 디자인/레이아웃 변경의 장단점을 판단할 때
- 색 대비, 간격, 정렬, 잘림, 겹침 같은 시각적 회귀를 확인할 때
- "화면에 어떻게 보이는지"를 근거로 코드 리뷰를 할 때

## 반드시 지킬 규칙

1. **추측하지 말고 캡처하세요.** 색/간격/크기에 대한 주장은 `scripts/design-audit.luau`
   의 측정값이나 캡처 이미지로 뒷받침하세요.
2. **기본 빌드는 건드리지 마세요.** 자동화는 `dev-automation` 기능 뒤에 있고,
   계측 호출은 기능이 꺼져 있으면 no-op입니다.
3. **`.edev-instances/`는 EDEV 소유 상태입니다.** 읽지도, 편집하지도, 지우지도 마세요.
4. **계측 id는 공개 계약입니다**(`docs/eguidev-automation.md`의 표). 라벨은 바꿔도
   id는 유지하세요. 한 프레임에 같은 id를 두 번 등록하면 자동화가 멈춥니다.

## 워크플로 (MCP 도구 `script_eval`)

한 번의 `script_eval` 호출로 준비 → 캡처 → 측정을 끝냅니다.

```lua
eguidev.root:wait_capture()
return {
  image = eguidev.root:screenshot(),          -- 응답에 이미지 블록으로 포함됨
  tree  = eguidev.dump_text({ fields = "core" }),
}
```

이미지를 **읽으려면** `screenshot()`의 반환값(`ImageRef`)이 `script_eval` 반환값에서
도달 가능해야 합니다. 테이블에 담아 반환하세요.

## 준비된 스크립트 (그대로 호출)

| 스크립트 | 목적 |
|---|---|
| `scripts/design-shot.luau` | 뷰포트 전체 또는 `widget` 인자로 지정한 위젯 크롭 캡처 |
| `scripts/design-audit.luau` | 이미지 + 위젯 기하 + 레이아웃 문제 + 팔레트 + WCAG 대비 |

`script_eval`로 실행할 때는 파일 내용을 전달하거나, 파일 경로 실행이 지원되면
경로를 씁니다. 터미널에서는:

```bash
scripts/edev-run.sh --config .edev-gui.toml eval scripts/design-audit.luau \
  --out-dir tmp/eguidev-screenshots
```

`--out-dir`에 JPEG가 저장되고 JSON의 `images[].file`에 절대 경로가 실립니다.
필요하면 그 파일을 직접 읽어도 됩니다.

## 무엇을 근거로 삼을 수 있나 (객관 지표)

| 데이터 | API | 디자인 판단에서의 의미 |
|---|---|---|
| 프레임 이미지 | `viewport:screenshot()`, `widget:screenshot()` | 실제 렌더 결과(색·여백·정렬·오버플로) |
| 위젯 사각형 | `widget:state().rect` / `interact_rect` | 정렬/간격/크기, 클릭 영역 |
| 색 | `viewport:sample_pixels()`, `sample_grid(nx, ny)` | 테마 일관성, 대비 측정 |
| 레이아웃 문제 | `viewport:layout_issues()`, `widget:layout_issues()` | overlap / clipping / overflow / zero_size / text_truncation / offscreen |
| 텍스트 계측 | `widget:text_measure()` | 줄바꿈/잘림/줄높이 |
| egui 진단 | `viewport:egui_diagnostics()` | id 충돌, rect 변경 등 계측 결함 |
| 위젯 트리 | `eguidev.dump_text()`, `eguidev.widgets({...})` | 계층/역할/활성 상태 |

## 자주 쓰는 읽기

- 뷰포트: `eguidev.root`, `eguidev.viewports()`, `eguidev.wait_viewport({...})`
- 위젯: `eguidev.widget(id)`, `eguidev.widgets({ visible = true, role = "button" })`
- 대기: `viewport:wait_capture()`, `widget:wait(cond)`, `eguidev.wait_frames(n)`

## 하지 말 것

- `screenshot()` 결과를 버리고 픽셀 좌표만 추측하기
- 스크립트 안에서 파일/네트워크 접근 시도(샌드박스에 없음)
- 종료 확인 창을 띄우는 경로로 앱을 종료시키기(자동화에서는 자동으로 건너뜀)
