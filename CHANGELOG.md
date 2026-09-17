# Changelog

형식: **[버전/일자]** 변경 요약. 항목은 되도록 **행동/계약 변경**과 **리팩터**를 구분합니다.
검증은 `cargo build`(0 경고) + `cargo test -p freedf-core` / `-p freedf-canvas` 기준입니다.

## [Unreleased] — 2026-09-13

### Phase 3 — elm-magic 0.5.0 마이그레이션 (docs/elm-magic-bug-report.md)
- **의존성**: 리비전 핀(`14f11eb9…`, git) → **crates.io `elm-magic = "0.5.0"` /
  `elm-magic-egui = "0.5.0"`**. 발행본이 dev 브랜치 `d063fb21…`와 소스 동일임을
  `.crate` 다운로드 후 diff로 확인(코어/매크로/어댑터 3크레이트 전부).
- **버그 리포트 3건 수정 반영 — freedf-gui의 워크어라운드 전부 제거**:
  1. `pub fn`/`pub(crate) fn` 컴포넌트 → 셸을 `pub(crate) fn Shell`로 선언(vis 보존),
     진입 래퍼 `render_shell`은 eguidev 계측 태깅 때문에만 잔존.
  2. `remove(x)` 뒤 문장 구분자 → 리포트 재현 형태(`remove`가 **첫 문장** + 뒤 대입)를
     회귀 테스트로 편입.
  3. `{if}` 안 지역 컬렉션 `.iter().map(..)` → `sections`·`bookmarks`·`outline_entries`·
     `tab_names` 전부 `.iter()`로 복원(이전엔 `.into_iter()` 우회).
- **파급 수정**: 전개가 `(x).clone().into_iter()`라 **컬렉션이 `Clone`이어야** 한다 —
  `canvas::OutlineEntry`에 `#[derive(Clone)]`.
- **회귀 테스트 3건 추가**(`shell.rs`): `bug1_pub_fn_component_is_usable_outside_its_module`,
  `bug2_remove_before_assignment_and_bug3_local_iter_in_conditional`,
  `bug3_conditional_branch_hides_local_collection` — 3건이 다시 깨지면 컴파일/테스트 실패.
- 검증: `ELM_MAGIC_DUMP=1`로 `#[derive(..)] pub(crate) struct ShellProps`와
  `(sections).clone().into_iter().map` 전개 확인 · 0.5.0 이전 리비전으로 되돌리면
  `visibility pub is not followed by an item` 등 **26개 에러**(대조군) ·
  freedf-gui 테스트 **34건 통과** · `cargo build --workspace` 통과(freedf PoC 모달은
  코드 변경 없이 그대로 빌드).

### Phase 3 — 테마 공유(`freedf-theme`) + 창 배경 회귀 수정 (docs/freedf-gui-migration.md)
- **버그(실측)**: freedf-gui 창 배경이 Nord가 아니라 `#080808`(근사 검정)이었다.
  eframe `clear_color` 기본값 `(12,12,12,α180)` + 배경을 칠하는 주체 없음(elm-magic
  셸은 egui 패널 미사용) → 불투명 검정 위에 합성. **플랫폼 무관** 버그(Windows 실측 +
  Linux 실제 X11 창 캡처 모두 동일).
- **수정**: `Host::clear_color` = 테마 `window_fill`(Nord0 `#2E3440`) **불투명**,
  루트 프레임 = 같은 `window_fill`(Nord0 — 리사이즈 이음새 없음, 원본 freedf 크롬과
  동일: 툴바·상태바 실측 `#2E3440`), 캔버스 스테이지 = `faint_bg_color`(Nord3).
  렌더 진입점을 `shell::render_root`로 뽑아 앱과 테스트가 같은 경로를 지난다.
- **테마 추출**: `crates/freedf-theme` 신설 — `nord.rs`/`tokens.rs` 이동(이력 보존),
  `freedf`는 `pub use freedf_theme::{nord, tokens}` 재노출로 호출부 무변화. egui 외
  의존 없음(서비스 계층과 같은 원칙 — Phase 1 참고).
- **Windows 최대화 잘림 방어**: 루트 안쪽 여백 8px(`shell::ROOT_INNER_MARGIN`) —
  150% 배율에서 창이 좌우로 화면 밖에 밀려 첫 글자가 잘리는 문제.
- **DPI 재현**: `FREEDF_GUI_PPP=1.5`(환경변수)로 배율 흉내 — 기본 동작 무변화.
- 검증: 실제 X11 창 캡처 실측 — 배경 `#2E3440`·스테이지 `#4C566A`·`#080808` 없음.
  회귀 테스트 2건 추가(freedf-theme 팔레트/설치, freedf-gui 루트 프레임+캔버스 배경)
  — freedf-gui 31건 · freedf-theme 2건 통과, 워크스페이스 전체 테스트 통과.
  **주의**: eguidev 인프로세스 캡처는 클리어 색 합성 때문에 배경을 회색으로 오판하게
  한다 — 배경 검증은 실제 창 캡처로 교차 확인.

### Phase 3 — 설정 창 + 잉크 기본값 저장/복원 (docs/freedf-gui-migration.md)
- **설정 창**: Settings 버튼 → 모달 — 현재 잉크 기본값(도구·색상·굵기 표시 이름)을
  보여주고 "Save as default"로 저장. 성공/실패는 토스트로 보고.
- **파일 백엔드**: `<app_data_dir>/gui-ink-defaults.json` (Windows:
  `%LOCALAPPDATA%\FreeDF`, 그 외 `~/.local/share/freedf`) — `save_defaults_to`/
  `load_defaults_from`으로 경로 주입 가능(테스트는 임시 파일 사용).
- **복원은 select_* 커맨드 경로 재사용** — 파일에 알 수 없는 이름이 있어도
  기본값 폴백이 자동 적용된다 (저장 JSON은 원본 보존). 앱 시작 시 `load_defaults()`
  한 번 호출 — `Canvas::default()`는 순수하게 유지해 테스트 오염 방지.
- 검증: freedf-gui 테스트 **30건 전부 통과** (저장→리셋→복원 roundtrip, 손상
  파일 폴백, 알 수 없는 이름 폴백, 설정 모달 열기/닫기 추가). `edev smoke` 통과
  (`gui.settings` 계약 추가), Xvfb 캡처로 설정 모달 렌더 확인, 워크스페이스
  전체 테스트 통과.

### Phase 3 — 잉크 리본 + 토스트 (docs/freedf-gui-migration.md)
- **잉크 리본**: 툴바 아래 2단 Row — 도구(Pen/Fountain/Highlighter/Eraser)·
  색상(Black/Red/Blue — settings 서비스 기본 즐겨찾기 팔레트)·굵기(Thin/Medium/
  Thick) 선택. 활성 항목은 Strong, 비활성은 Button으로 렌더 (`view!`의 조건부
  분기). 문자열 기반 커맨드(`select_tool`/`select_color`/`select_width`)로
  이벤트 경로를 단순화 — 알 수 없는 값은 프리셋 기본값 폴백.
- **토스트**: 시간 기반 자동 만료 알림(3초) — 캔버스 엔진이 (메시지, 시작 시각)을
  소유하고 `toast()`로 만료를 판정. 상태바 자리를 대신 사용해 레이아웃 흔들림
  없음. Bookmark 토글/잉크 지우기/PDF 열기 성공·실패에 연결.
- 검증: freedf-gui 테스트 **28건 전부 통과** (리본 선택·폴백·토스트 만료·리본→
  엔진 반영 end-to-end 추가, 북마크 플로우 테스트를 토스트 문구 기준으로 갱신).
  `edev smoke` 통과(리본 계약 id 5종 추가), Xvfb 캡처로 리본 렌더 + 토스트 표시
  확인, 워크스페이스 전체 테스트 통과.

### Phase 3 — 진짜 문서 탭 + 북마크/아웃라인 패널 (docs/freedf-gui-migration.md)
- **탭 = 문서**: 캔버스 엔진이 문서 목록(`Doc`: AnnotationStore·페이지·뷰·PDF·텍스처)을
  소유하고, 셸은 `canvas::tab_names()`/`active_tab_id()`를 매 프레임 읽어 렌더만
  한다 — **두 소스 오브 트루스 문제를 제거** (이전: elm 슬롯의 탭 이름과 엔진 상태
  분리 → 이름만 있고 문서가 없는 유령 탭). 탭 id는 u64 고유값 — 이름이 같은 탭도
  구분해 선택 가능.
- 커맨드 추가: `add_tab`/`close_tab`(마지막 탭은 빈 문서로 리셋)/`select_tab(id)`/
  `toggle_bookmark`/`go_to_page`/`bookmark_list`. 탭 전환·닫기 전 진행 중 획을
  마쳐 잘못된 문서 기록을 방지.
- **북마크 패널**: 툴바 Bookmark(현재 페이지 토글)·Bookmarks(패널 토글) 버튼 —
  북마크 목록 행 클릭 시 해당 페이지로 점프. 상태바에 `북마크 n` 표시.
- **아웃라인 패널**: Outline 버튼 → PDF 북마크 트리를 깊이 들여쓰기로 평탄화해
  표시(`flatten_outline` 순수 함수), 행 클릭 시 해당 페이지로 점프. 들여쓰기는
  문자열 보간 밖(canvas 쪽)에서 계산 — 보간 안의 중첩 문자열 식은 format string을
  깨뜨린다는 실측 노트.
- Open PDF가 이제 **새 문서 탭**으로 열린다 (이름 = 파일 스템, 페이지 크기 = PDF 1페이지).
- 검증: freedf-gui 테스트 **24건 전부 통과** (탭 라이프사이클·문서별 잉크 분리·
  북마크 토글/점프·아웃라인 평탄화·리셋 경로 추가). `edev smoke` 통과(툴바 계약
  12개 id 포함), Xvfb 실행 캡처 확인, 워크스페이스 전체 테스트 통과.

### Phase 3 착수 — freedf-gui eguidev 계측 인프라 + 견고화 (docs/freedf-gui-migration.md)
- **계측 인프라**: `crates/freedf-gui/src/dev.rs` (freedf `app/dev.rs`와 동일 헬퍼
  패턴 — `dev-automation` 기능 게이트 no-op). 루트 프레임 스코프 `freedf-gui.root`,
  어댑터가 그린 버튼/탭은 `Pass.buttons`에서 **`gui.<라벨 슬러그>`** id로 등록
  (같은 라벨 중복 시 `.<n>` 접미사 — eguidev 중복 id 결함 방지), 캔버스는
  `canvas.surface` publish. 런처 설정 `.edev-gui.toml` + 스모크 스위트
  `smoketest-gui/10_launch_gui.luau` — **edev smoke 통과**로 계약 등록 검증.
- **버그 검토·수정 (canvas.rs)**:
  1. 모달 창이 캔버스 위에 떠 있을 때 뒤에서 잉크가 그려지는 버그 — 셸이 매
     프레임 `sync_and_status(modal_open)`으로 캔버스 입력 활성화를 동기화
     (egui `hovered()`는 헤드리스/신규 인터랙션 모델에서 불안정해 플래그 방식 채택).
  2. 캔버스 밖에서 누른 오른쪽 드래그가 팬으로 새는 버그 — `pan_active` 플래그로
     캔버스 안에서 눌렀을 때만 팬 시작.
  3. PDF 페이지 렌더 실패 시 **매 프레임 블로킹 렌더 재시도**(프리즈) 버그 —
     `tex_attempted`로 페이지당 1회만 시도.
  4. 빈 이름 탭 생성 — "Untitled"로 대체.
  5. **PDF 페이지 이동 추가** (Phase 2 잔여): Prev/Next Page 버튼 + 상태바
     `PDF n/m` 표시 (PDF 없을 때 no-op).
- 검증: freedf-gui 테스트 17건(+입력 차단·페이지 이동·모달 흐름) 전부 통과,
  `dev-automation` 빌드 경고 0, `edev smoke` 통과, 워크스페이스 전체 테스트 통과.

### Phase 2 완료 — freedf-gui 캔버스 v1 (docs/freedf-gui-migration.md)
- **`crates/freedf-gui/src/canvas.rs`** — `<Raw>` 경계 뒤의 명령형 캔버스 엔진:
  - 잉크 지오메트리/스밈은 freedf와 **같은 생성기**(`freedf-canvas`의
    `halves_for_stroke` + `append_stroke_ribbon` + `alphas_for_stroke`),
    페이지↔화면 변환은 `ViewTransform`, 저장은 `freedf-core::store::AnnotationStore`.
  - 입력: 좌클릭 드래그 = 잉크(획 종료 시 저장소 기록), 우클릭 드래그 = 팬,
    휠 = 포인터 고정 줌(`zoom_at_view` 순수 함수 — freedf-core MIN/MAX_ZOOM 클램프).
  - PDF: `freedf-services::pdf`로 열기(셸 모달 경로 입력) + 페이지 텍스처 렌더.
    pdfium 부재 시 오류를 상태바에 표시하고 빈 페이지로 잉크는 계속 동작.
- 셸 통합: 툴바에 Zoom In/Out · Fit · Open PDF · Clear Ink(확인 모달) 추가,
  상태바에 캔버스 상태(페이지/줌/획 수) 표시. 커맨드는 elm 핸들러에서
  `canvas::zoom_in()` 등으로 호출.
- 레이아웃 실측 노트: egui에서 **수평 Row 안의 수직 Col은 컨텐츠 높이만** 가용
  높이로 받는다(실측 57px) — 캔버스 `<Raw>`는 루트 Col 직접 자식으로 배치
  (남은 높이 전체). 한글 렌더를 위해 freedf의 임베드 폰트(Asta Sans/NanumGothic)를
  freedf-gui `fonts.rs`에서 공유.
- 검증: 헤드리스 테스트 15건 전부 통과 — 줌 수학(포인터 고정/클램프),
  **egui 원시 포인터 이벤트 주입으로 획이 저장소에 기록되는 end-to-end**,
  clear/open_pdf 오류 경로, 셸 버튼↔캔버스 상태 연동. Xvfb 실행 캡처로
  페이지/툴바/한글 상태바 확인. 워크스페이스 전체 테스트 통과, 경고 0.
- v1 한계(문서화): 필압 명목 1.0(Phase 4 어댑터 과제), PDF 페이지 이동 v2,
  프레임마다 메시 재굽기(획 수가 커지면 freedf의 BakeService 이식).

### Phase 1 완료 — freedf-services 서비스 계층 추출 (docs/freedf-gui-migration.md)
- **`crates/freedf-services` 신설** — freedf에서 `storage`·`sync_storage`·`server`·
  `sync_client`·`pdf`·`settings`·`recent`·`recording`·`player`를 `git mv`로 이동
  (이력 보존). `freedf`는 모듈 셔임(`pub(crate) use freedf_services::X::*;`)으로
  기존 `crate::X::*` 호출부를 **무변화** 유지 — 이후 freedf-gui가 같은 계층을 공유.
- 부수 정리: 교차 크레이트 가시성을 위해 `pub(crate)`→`pub` 20건, `pdf`가 `Pdfium`
  재노출(freedf의 pdfium-render 직접 의존 제거), hound/cpal/rodio 의존성도 services로
  이동. `theme`(egui)과 `app/dictionary.rs`(오버레이 UI)는 freedf 잔존.
- 검증: `cargo test --workspace` 전체 통과 — freedf 92 + services 28(구 freedf 120의
  정확한 분할) + freedf-gui 7(셸 6 + 서비스 연결 스모크 1) + core/canvas/sync.

### freedf-gui 신규 크레이트 — elm-magic으로 앱 셸을 처음부터 재작성 (v0)
- **`crates/freedf-gui`** — 마이그레이션이 아니라 **재작성** 실험. eframe 호스트가
  `elm_magic::Ctx`를 프레임 간 유지하고, 매 프레임 `elm_magic::frame` →
  `elm_magic_egui::render`로 `Shell` 컴포넌트를 그린다. 상태는 전부 컴포넌트
  매개변수 슬롯(tabs/active/sidebar_open/status/modal/input).
- **셸 구성(전부 순수 elm-magic)**: 툴바(Sidebar 토글 · New Tab · Close Tab · About),
  사이드바(Library · Notes/PDFs/Recents 행, 클릭 시 상태바 갱신), 탭 스트립
  (`tabs.map` + `<Tab active>` — 클릭 선택, 모달 확인 후 `remove` 삭제), 캔버스는
  `<Raw>` 플레이스홀더(페인터 테두리 + 안내문), 상태바, `match modal` 다이얼로그 3종
  (New Tab 입력 · Close 확인 · About).
- **실행**: `cargo run -p freedf-gui` · 검증: 헤드리스 테스트 6건 전부 통과 +
  Xvfb 실행 캡처로 실제 렌더 확인. 워크스페이스 `cargo check --workspace` 경고 0.
- **발견한 elm-magic 버그(우회법 포함, 업스트림 보고 대상)**:
  1. `view!`에 `pub fn`을 쓰면 `pub #[derive(...)]`를 출력해 컴파일 실패 →
     컴포넌트는 모듈 프라이빗으로 두고 `render_shell()` 진입 함수로 노출.
  2. `remove(x)` 특수 폼 이후의 문장이 `,`로 이어져 생성됨 → `remove`를
     핸들러의 **마지막** 문장으로 배치해 우회.
  3. `{if ...}` 식 내부의 `.iter().map(...)`은 소유 반복으로 재작성되지 않아
     지역 배열을 빌린 핸들러 캡처가 E0716 → 지역 컬렉션은 `into_iter()`로 명시.
- 다음 단계(v1): 캔버스 `<Raw>`에 freedf-core/freedf-canvas 페인팅 연결, storage 백엔드 연동.
- **이후 방침**: freedf → freedf-gui 점진적 이전 계획은 `docs/freedf-gui-migration.md`,
  발견한 elm-magic 버그의 업스트림 리포트는 `docs/elm-magic-bug-report.md`.

### elm-magic PoC — fallback_dialog 본문을 `view!`로 렌더링 (ui/elm_modal.rs)
- **파일럿**: 모달(AskText/Confirm/Alert)의 **본문만** `elm_magic::view!` 컴포넌트로
  그린다. 윈도우 타이틀/폭/여백은 기존 `ui::dialog::modal`을 그대로 재사용하고,
  내용물은 `elm_magic_egui::render`(egui 0.36 어댑터)가 그린다. 상태(타이핑·OK/Cancel)
  는 아레나 슬롯 대입으로 기록되고, 슬롯을 읽어 기존 `run_text_action`/`run_confirm_action`
  흐름에 그대로 넘긴다. 의존성은 git rev 고정(`14f11eb`) — crates.io 미발행 실험 단계.
- 검증: 헤드리스 단위 테스트 4건(클릭→슬롯 계약) + eguidev 캡처로 실제 렌더 확인.
- **알려진 PoC 한계**: 액션 행 우측 정렬 불가(어댑터 `Row`가 좌→우 흐름 한정),
  NewNote 페이지 콤보박스는 egui 어휘 부재로 호출부가 직접 렌더(버튼 아래 배치),
  입력 필드는 `ui::form::text` 스타일이 아닌 어댑터 순수 `TextEdit`.
- 전체 마이그레이션은 **비추천** — 캔버스/잉크는 어차피 `<Raw>` 탈출구가 필요하고
  eguidev 계측 id 계약을 다시 붙여야 한다. 이 파일럿으로 라이브러리 성숙도를 계속 관찰.

### 툴바 전면 재설계 — "도메인 3행 + 단일 설정 홈" (ribbon.rs)
- **`app/toolbar/rows.rs` → `app/toolbar/ribbon.rs` 대체** — 툴바를 **도메인별 3행**으로 재편:
  Row1 `Workspace`(Hide UI ／ Library·Outline·Bookmarks·Palette ／ Undo·Redo·Clear ／
  Save·Load ／ 전역 `Settings` · `More`), Row2 `Page`(Insert·Delete·Rotate ／ Paper ／ Canvas),
  Row3 `Ink`(도구 피커 ／ 도구별 옵션 ／ Refresh Hz). 각 행 = 하나의 주제.
- **중복/단일 홈 정리** — `Window Focus` 상주 제거(`More ▸ Input`로 이동), 설정 진입을
  전역 `Settings` + 도구별 `Draw` **2곳**으로 수렴(`Pen Settings`·`Fountain Settings`·
  `Cursor Size` 3중 버튼 제거), `Color wheel`을 Row2에서 분리해 설정 탭으로 단일화.
- **`More` 메뉴 주제별 섹션화** — Page · View · Lookup · Input · Server · Maintenance ·
  Diagnostics로 묶어 평면 나열을 제거.
- **오버레이 입력 불일치 수정** — 펜 설정 "Input & cursor"에 기생하던 `Debug HUD`
  체크박스 삭제(진단 토글의 단일 홈 = `More`). `Paper`=RULER, `Canvas`=SQUARES_FOUR,
  `Palette`=PALETTE 로 아이콘 중복을 해소.
- **`ui::components` 중복 제거** — `placeholder()` 함수 안에 실수로 중첩돼 접근 불가였던
  `Tone`/`badge`/`alert` 등 `ui::ds` 복제 요소를 삭제하고, 경량 원자(pill/caption/help/
  placeholder)만 유지. 문서 `docs/UI-COMPONENTS.md`·`docs/DESIGN-TOOLBAR.md` 갱신.
- 검증: `cargo build`(경고 0) · `cargo test -p freedf-core`/`-p freedf-canvas`/`-p freedf` 전체 통과.

### UI/UX 개편 (툴바 계층 · 그룹핑 · 진단 확충) — `docs/UI-UX-REPORT.md` 반영
- **기본 한글/UI 폰트를 Asta Sans로 교체** — Google Fonts Asta Sans(위 300–800) Regular를
  내장해 'Proportional'의 1순위로 등록(Inter·NanumGothic은 폴백 유지). `fonts.rs`.
- **툴바 Row1 계층화(P0)** — 자주 안 쓰는 도구/설정(Dictionary·Media Server·Media·Macro·
  Gamepad·Cache·정렬)을 1층에서 제거하고 끝의 **"More" 오버플로 메뉴**로 이동. 첫 줄 요소를
  약 22 → 16개(그룹: 창/패널 · 명령 · 저장 · More)로 정돈. `rows.rs::row_top`.
- **Fast ink noise 위치 이동(P1)** — Debug HUD 체크박스 제거, 펜/만년필 설정 창의
  **"Ink grain" 필드셋**(도구별 질감 옵션)으로 이동. `paint.rs`·`toolbar/mod.rs`.
- **Debug HUD 확충(P3 부분)** — 디버그 창에 **"System / About"** 접이식 섹션 추가: 소프트웨어
  버전(`CARGO_PKG_VERSION`)·OS·아키텍처·빌드 프로필·렌더러(glow/wgpu)·PID·CPU·DB 연결 상태 +
  **"Copy diagnostics"**(클립보드로 1-클릭 진단 복사). `gamepad.rs`.
- **설정 다이얼로그 통합(P2)** — 11개 분산 모달(도구/커서/종이/캔버스/휠/페이지/엣지/포커스/
  서버/매크로/게임패드)을 **단일 "Settings" 창(좌측 탭 레일 + 우측 내용)** 으로 통합.
  툴바의 각 `*_open` 요청이 해당 탭으로 라우팅되고 창을 닫으면 해제. `settings.rs`·`mod.rs`.
- 그 외: 미사용이 된 `fast_ink_noise` App 필드/`form` import 제거, `ui::dialog::dialog` 헬퍼 제거(경고 0 유지).
- **툴바 전체에 컴포넌트 키트 적용** — Row1~Row3의 모든 그룹 경계를 `layout::group`+`vdivider`,
  행 조립은 `layout::vstack`+`hseparator`, `toolbar()` 패널은 `containers::container()` 래핑.
  More 메뉴의 순수 텍스트 버튼을 3계층 `buttons::Button::secondary`로 교체. `rows.rs`/`toolbar/mod.rs`.
- **툴바 키트 반복 적용(R2)** — Row2를 G1 Page/G2 Canvas/G3 ColorWheel/G4 Paper로 `layout::group`
  세분화, Rotate 메뉴 4개 버튼을 `buttons::Button::secondary().enabled()`로, Paper 색 스와치 행을
  `layout::hstack`(flex)로 래핑.
- **디자인 시스템 키트 `ui::ds` + 자율 적용(R3)** — `Tone`·`badge`·`status_dot`·`alert`·`kbd`·`card`
  를 `crate::ui::ds`로 신설(재사용, dead_code 유지). 상태바에 연결 배지(online/offline), Debug HUD
  System에 `badge`(DB), 게임패드 로그에 `scroll` 키트 적용. Row3 펜/하이라이터 스와치를
  `layout::hstack`(flex)로 래핑. (정리: `ui::components` 확장 블록은 `ui::ds`로 이관.)
- **버튼 3계층 확산(R4)** — Debug HUD "Copy diagnostics"·미디어 패널 Upload/Refresh를
  `buttons::Button::{primary,ghost}`(아이콘 포함)로 전환. `Button::hint`를 `&str`→소유 `String`
  (`impl Into<String>`)로 유연화해 `format!` 툴팁도 허용.
- **서버 설정·연결 다이얼로그 키트화(R5)** — Connect/Reconnect·Save를 `buttons::Button::{primary,
  secondary}`로, 연결 결과를 `ds::alert`(Success/Danger)로 전환. `ds::alert` 메시지도
  `Into<String>`로 유연화.
- **툴바 재설계 밀스톤 1(R6)** — "하나의 액션 = 한 곳" 원칙 청사진
  `docs/DESIGN-TOOLBAR.md` 수립. 선언형 액션 바 `ui::actionbar::ActionBar` 신설하고
  Row1의 Undo/Redo/Clear를 스펙 기반(React식)으로 전환.
- **툴바 재설계 밀스톤 2(R7)** — `ActionBar`에 `Toggle`(상태 바인딩)·`Select`(라디오)
  지원 추가. Row1 패널 토글(Library/Outline/Bookmarks/Palette)과 More의 정렬 라디오를
  스펙으로 전환 — egui 상태 갱신은 팩토리가, 호출부는 부수효과만.
- **툴바 재설계 밀스톤 3(R8)** — Row1 Save/Load와 Row2 페이지 그룹(Insert+Delete)을
  `ActionBar`로 전환, 중복 Delete 제거. Row1 전체가 스펙 기반이 됨.
- **Row1 스펙 완성 + 검색 단추(K9)** — Hide UI·Window Focus를 액션 바로 통합해 Row1
  전체를 `ActionBar` 스펙으로 일원화, 중복 원시 `icon_button` 제거. 검색 "Find" 텍스트
  단추를 3계층 `buttons::Button::primary`로 전환.
- **설정 대화상자 UI 블록화+WCAG(K10)** — 설정 창을 안정적 UI 블록으로 재구성:
  헤더(제목+조작 힌트)·`containers::container`로 좌/우 레일 경계 블록화·선택 라벨
  포커스 요청·탭 툴팁 라벨링. 접근성: 키보드 포커스, 목적 라벨링, 테마 기반 대비.
- **커넥션 다이얼로그 통일(K11)** — Connect/Reconnect를 3계층
  `buttons::Button::primary`로, 연결 상태를 `ds::alert`(Success/Danger)로 전환해
  server_settings와 동일한 블록/스펙 사용.
- **설정 창 Y축 확장 수정(K12)** — egui `Frame`은 커져야 늘어나지 않는 특성이라,
  창 콘텐츠 높이(`max_rect().height()`)를 좌/우 레일의 `Ui::set_height`로 명시해
  컨테이너가 세로 전체를 채우도록 수정(스크롤 영역이 뷰를 채움).
- **UI 영어화(K13)** — 설정 창에 임시로 넣었던 한글 문자열(힌트/섹션 제목/툴팁)을
  영어로 교정. 사용자 노출 문자열은 전부 영어 유지.
- **대비 강화(K14)** — 버튼 `on_color`를 WCAG 상대 휘도·대비비 공식으로 재작성(흑/백 중
  최고 대비 확실 선택), Danger 채움색도 `on_color` 적용. (참고: 같은 턴의 툴바 행 높이
  고정 시도는 `auto_shrink(false)`가 행을 창 전체 크기로 펼쳐 캔버스를 가리는 회귀를
  유발하여 즉시 되돌림.)
- **액션바 롤백·UI 단순화(K15)** — `ui::actionbar::ActionBar` 제거. 툴바 버튼/토글/라디오를
  전부 원시 프리미티브(`icon_button`/`icon_toggle`/`icon_select`)로 되돌림. `id` 토큰+`match`
  관례와 단일 액션을 위한 추상 계층을 걷어내 단순화. **React식 분리는 유지** — 그룹별
  컴포넌트 함수(`toolbar_panel_group`, `toolbar_overflow_menu`)와 상태↔props 연결은 그대로.
- **앱 레이아웃 프레임을 원시 컨테이너로 감쌈** — 탭바(`tabs_bar`), 툴바, 상태바, 중앙
  캔버스 스테이지를 `containers::container()`(패딩/필/보더) + `layout::vstack`로 래핑.
  여백/스타일이 컴포넌트 키트를 통해 일원화됨.
- **웹 엘리먼트 라이브러리 확장(R1)** — `ui::components`에 `Tone`(세마틱 색)과 함께
  `badge`/`tag`/`alert`/`status_dot`/`progress`/`spinner`/`kbd`/`code`/`breadcrumb`/
  `divider_label`/`avatar`/`count`/`tabs`/`card` 추가(Bootstrap `.badge/.alert/.progress/…` 매핑).
- **컴포넌트 라이브러리 확장(React·Bootstrap 스타일)** — 3계층 버튼 `ui::buttons`
  (`Button::{primary,secondary,ghost}`·size/danger/icon·숏컷), 범용 컨테이너 `ui::containers`
  (margin/padding/fill/border 컨테이너 + grid + flex row/centered), 스크롤바 `ui::scroll`,
  토스트 `ui::toast`(우상단 스택·자동 소멸·✕; 앱 `toasts` 필드에 배선, 시작 환영 토스트).
  폼 확장 `textarea`/`segmented`. 문서 `docs/UI-COMPONENTS.md` 갱신.
- **레이아웃/범용 컴포넌트 키트 신설(React·Bootstrap 스타일)** — `crate::ui::layout`(8px 그리드
  `SP_*`, `hstack`/`vstack`, `vdivider`/`hseparator`, `toolbar_row`/`group`)과
  `crate::ui::components`(`pill`/`caption`/`help`/`placeholder`) 추가. `toolbar_row`를 app→ui로 이전해
  재사용. 툴바 조립부(`toolbar()`·`row_top`)를 이 키트(vstack/hseparator/vdivider/group)로 재구성.
  설계서 `docs/UI-COMPONENTS.md`(툴바 계층 트리 + 카탈로그 + 작성 가이드).
- **툴바 Row1 컴포넌트 분해(React 스타일)** — `row_top`을 컨테이너 컴포넌트 합성으로 정리:
  `toolbar_panel_group`(패널 토글 묶음)과 `toolbar_overflow_menu`(More 메뉴)를 메서드 컴포넌트로
  추출, 프레젠테이션은 `crate::ui` 컴포넌트(props)가 담당·상태는 컨테이너가 연결하는
  기존 아키텍처 주석과 일치. `rows.rs`.

### 필기 파이프라인 리팩터 (InkPipeline / WritingMaterial / Materials)
- **`LiveStroke` + `InkPipeline` 신설** (`crates/freedf-core/src/pipeline.rs`)
  - 진행 획을 한 객체로 묶어 호출 시퀀스를 `down()`+`drag()`+`up()`으로 압축.
  - `LiveStroke`는 append-only 점 버퍼 + frontier(`mark_meshed`) + 증분 bbox,
    `freeze()`로 **불변 `Stroke`** 동결.
- **`WritingMaterial` trait + `Materials` 퍼사드** (`pen.rs`)
  - `LockerProfile` enum 분기 제거 → `BallPenProfile`/`FountainProfile`/`ConstantMaterial`
    이 `WritingMaterial` 구현.
  - `widths()`(배치)를 trait로 이동 → `halves_for_stroke`가 `Materials::for_tool(...).widths(...)`로 위임.
  - `material_for` 자유 함수 → `Materials` 퍼사드로 교체.
- **`InkPipeline::down`에 `tilt_mag` 파라미터 추가** — 만년필(italic/틸트) 폭 대비 보존.
  - 계약 테스트 `pipeline_down_threads_tilt_into_width_locker`.
- **`smoothing <= 0.001`이면 1€ 필터 생략 → raw 좌표 통과** — 앱의 "스무딩 꺼짐" 경로와
  행동 일치. 계약 테스트 `pipeline_smoothing_zero_passes_raw_coords`.

### 앱 배선 (input / ink / finish_stroke / commit_dot)
- `FreeDfApp`에 `ink: Option<InkPipeline>` 필드 도입, `width_locker`·`smooth_x/y/p`·`smooth_active` 제거.
- `input.rs` 펜 down/drag가 파이프라인 경유; `active_stroke`는 `pipeline.live().points`와
  매 드래그 동기화해 **렌더 == 커밋 (WYSIWYG) 보존**.
- `ink.rs::finish_stroke`가 `self.ink.take().up(0)`의 **동결·불변 `Stroke`**로 커밋.
- `ink.rs::commit_dot`(탭 점)도 동일 파이프라인 down→up 경유.
- `ActiveStroke::push` 제거(미사용).

### UI — 통합 디버그 HUD
- 흩어져 있던 **펜/캔버스 오버레이**와 **분리된 "Gamepad debug" 창**을 **한 개의
  `Debug HUD` 윈도우**로 통합 (`crates/freedf/src/app/gamepad.rs::debug_hud_ui`).
  - `Pen / Canvas`(기본 열림) + `Gamepad`(기본 닫힘) 접이식 섹션을
    `form::fieldset`으로 구성. 토글(이미지 근사 등)은 `form::check` 사용.
  - 단일 토글 `debug_hud`로 열고 닫으며, 창 ✕로 닫아도 상태가 동기화.
  - 캔버스 오버레이 `Area`(paint_debug_hud)와 게임패드 전용 창/`gamepad_debug_open`
    필드 제거, 로그는 전부 영어로 통일 (한국어 "수신됨/없음/off(…)" → 영어).
### 서비스 계층 — 인터페이스화 + 컴포지션 루
- **`BakeService` 비제네릭화** (`freedf-canvas/bake.rs`): `W: BakeWorker` 제네릭 →
  `Arc<Box<dyn BakeWorker + Send + Sync + 'static>>` 보유. `start(Box<dyn BakeWorker>)`로 생성.
  → DI/테스트로 서비스 교체 가능.
- **`AppDeps` 컴포지션 루 신설** (`crates/freedf/src/app/deps.rs`): `{ clock, logger }`를
  `AppDeps::compose(...)`로 조립, `FreeDfApp::new(cc, db, ..., deps, ...)`에 **생성자 주입**.
  - `logger: Logger` 파라미터 → `deps: AppDeps`로 교체.
  - `FreeDfApp::now_ms() = self.clock.now_ms()` — 앱 전역이 **주입된 Clock** 사용.

### Clock 배선 (전역)
- 앱의 자유 `now_ms()`(벽시계 직접) 호출을 `FreeDfApp::now_ms()`(주입 Clock)으로 전환
  (canvas/actions/panels/gamepad 포함).
- 자유 `now_ms()`는 **sync_storage 백그라운드 워커 전용**으로 잔존.

### 문서
- `docs/ink-pipeline-design.md`: 실제 시그니처(tilt_mag, smoothing-raw), 앱 배선 표,
  `AppDeps` 컴포지션 루 섹션 반영.

### 성능 — OPTIMIZATION.md + P0 계측 (추정 벤치)
- **`docs/OPTIMIZATION.md` 신설**: live 잉크 렌더의 점당 비용 병목(live 5~7 O(n) 패스 +
  할당 7+, `value_noise` 점당 ~16 해시)을 정리하고, **증분 tail O(k) + 근사/비트(노이즈 타일·LUT·
  단일 패스·비트 플래그)** 전략의 **예상 절감 수치**(전체 재구성 −50~65%, 그리기 성장 경로 −90~99%,
  할당 −80~90%)와 벤치 표를 문서화.
- **P0 벤치 훅** `#[ignore]` `bench_live_render_cost_per_point` (`freedf-canvas/core_mesh.rs`):
  `CoreRibbonMesher::append_stroke` 실측 — **n=1k: 560 µs(560 ns/pt), n=10k: 5.93 ms(593 ns/pt)**(debug).
  OPTIMIZATION.md §4의 baseline을 실측치로 교체. CI에선 실행되지 않음(`ignored`).
- **동작보존 최적화 구현 (성능 1차)** — 출력·결정성 그대로:
  - `InkGrain::density_lr` + `value_noise_pair`/`ink_field_pair` (`freedf-core/ink.rs`): 단면(좌/우)
    밀도가 같은 `u`를 쓰는 점에서 각 옥타브의 **x 공통 부분을 1번만** 계산 (해시 수는 유지).
    parity 테스트 `density_lr_matches_two_density_calls`로 `density` 2회 호출과 동일함을 고정.
  - `append_stroke_ribbon` 메시 용량 예약 (`freedf-canvas/core_mesh.rs`): realloc 방지.
  - 실측: **n=1k 560→548 µs(−2.1%), n=10k 5933→5593 µs(−5.7%)** (debug). OPTIMIZATION.md §4에 실측 갱신.
  - 큰 폭(50~65%)은 **시각을 바꾸는 근사**(노이즈 타일·LUT·절대 호 길이)로만 가능 → GUI 검증
    필요로 **미구현 유지**하고 문서에 "P3 근사(추정)"로 구분.
- **시각 근사 캔버스 (P3 부분)**: `InkGrain.fast_noise`(기본 false) — 켜면 **고주파 위킹
  옥타브를 생략**해 질감 계산이 절반으로 줄어듭니다. `density`/`density_lr` 분기.
  - **앱 Debug HUD에 체크박스** 추가 (`paint_debug_hud` → `&mut self`, `fast_ink_noise` 필드) —
    켜는 즉시 펜/만년필 그레인에 전파되어 live·굽기 양쪽에 반영 (view_key에 그레인이 있어 캐시 재구성).
  - 테스트 `fast_noise_is_deterministic_bounded_and_non_popping`로 결정성·범위·no-popping 보호.
- **fast ink noise 기본 ON** — `InkGrain::default().fast_noise = true` + 앱 `fast_ink_noise` 초기값 true.
  parity 테스트(`density_lr_matches_two_density_calls`)를 두 모드(정확/fast) 모두 검증하도록 확장.
  P0 벤치 실측: **n=1k 504 µs(−10%), n=10k 5.13 ms(−13.6%)** (초기 560 µs/5.93 ms 대비). OPTIMIZATION.md §4 갱신.
- **Debug HUD 접근성**: `row_top`(상단 툴바)에 **"Debug HUD" 토글 버튼** 추가 — 설정 창을
  열지 않아도 오버레이를 바로 켜고 끌 수 있습니다 (기존 Pen Settings→Input & cursor(접힘) 경로 불필요).
- **매크로 기본 비활성화**: `MacroState::default()`의 `page_enabled`/`tab_enabled`/`desktop_enabled`/
  `desktop_focus_only`를 **모두 false**로 — 새 세션에서 매크로가 기본으로 꺼져 있고(UI 섹션 비활성),
  사용자가 Macro 창에서 개별 활성화합니다.

### 버그 수정
- **연속 줌 시 프리즈→크래시 (OOM)** — PDF 페이지 래스터의 최대 차원을
  `MAX_RENDER_DIM = 4096`px으로 제한 (`freedf/src/pdf.rs::MAX_RENDER_DIM`,
  `render_page`의 너비/높이/최대 차원 클램프 상한을 65,000→4096으로 인하,
  `ensure_texture`/`prefetch` 호출부 갱신).
  - 원인: 줌이 커지면 pdfium이 거대한 비트맵(고해상도·고줌에서 수십~수백 MB)을 할당하고,
    `as_rgba_bytes()`·`ColorImage` 복제로 순간 최대 ~3배 메모리 → 릴리즈에서 메모리 폭주(OOM)
    → 프리즈 후 크래시. 4096px 상한은 재생산 약 67MB/버퍼(복제 포함 ~200MB)로 안전.
  - 100%·fit-width·4K 고해상도는 상한 아래라 화질 영향 없음. 극단 줌만 소프트 (GPU 스케일).

### 검증
- `cargo test -p freedf-core`: **211 passed / 0 failed** (단위 200 + 통합 11), 0 경고.
- `cargo test -p freedf-canvas`: **29 passed / 0 failed / 1 ignored** (P0 벤치), 0 경고.
- 워크스페이스 `cargo build`: **0 에러 / 0 경고**.
