# freedf → freedf-gui 점진적 이전 계획

> 상태: **제안 (2026-09-17)** · 결정 전까지 `freedf`는 계속 출하 바이너리.
> 배경: `crates/freedf-gui`는 elm-magic(`view!`)으로 UI를 다시 쓰기 위한 재작성
> 실험 크레이트로, 현재 앱 셸(툴바·사이드바·탭·모달·상태바)까지 동작한다
> (`CHANGELOG.md` [Unreleased] 참고). 본 문서는 이를 정식 라인으로 만드는
> 단계별 계획이다.

## 0. 원칙

1. **freedf는 폐기 시점까지 손대지 않는다** — 이전 기간 내내 출하 가능 상태 유지
   (버그 수정만, 신규 기능 금지). 스위치오버 전까지 두 바이너리가 공존한다.
2. **공유 계층을 먼저 뽑아낸다** — UI가 아니라 서비스/모델 계층을 공용 크레이트로
   올려 "freedf 폐기"의 비용을 UI 코드만 남도록 만든다.
3. **계약은 그대로 유지** — eguidev 계측 id(`docs/eguidev-automation.md`)는 공개
   계약이므로 id를 새 크레이트에서 **동일하게** 재등록한다. smoketest도 대상만
   바꿔 재사용한다.
4. **각 단계의 끝은 항상 초록** — 두 크레이트 모두 `cargo check/test` + 스모크
   통과를 단계 종료 조건으로 삼는다.

## 1. 자산 분류 (crates/freedf ≒ 26.7k lines 기준)

| 분류 | 자산 | 이전 전략 |
|---|---|---|
| **순수 모델/계산** | `freedf-core`(노트·페이지·잉크·히스토리·검색·펜), `freedf-canvas`(잉크 메시), `freedf-sync`(프로토콜) | 이미 분리됨 — 그대로 재사용 |
| **서비스(GUI-프리에 가까움)** | `storage.rs`(295) · `sync_storage.rs`(1055) · `server.rs`(400, 미디어 클라이언트) · `sync_client.rs` · `pdf.rs`(557, pdfium) · `settings.rs`(983) · `recent.rs` · `recording.rs`/`player.rs` · `dictionary.rs` | **Phase 1** — 공용 크레이트로 추출 |
| **플랫폼(Windows 중심)** | `winstyle.rs` · `gamepad.rs` · `key_hook.rs` · Windows Ink 입력 | **Phase 4** — elm-magic 무관, 그대로 이식 |
| **UI(egui 명령형)** | `app/mod.rs`(3860) · `ui/`(3823) · `app/toolbar` · `app/panels` · `app/tabs` · `app/canvas` · `app/actions` · `theme/` | **Phase 2~3** — elm-magic으로 재작성 (포팅 아님) |
| **자동화** | `app/dev.rs` + `dev-automation` feature + smoketest | **Phase 3** — 계약 id 동일하게 재등록 |

## 2. 단계

### Phase 1 — 서비스 계층 추출 (freedf-gui v0.1) — **✅ 완료 (2026-09-17)**

- **완료**: `crates/freedf-services` 신설. `storage`·`sync_storage`·`server`·
  `sync_client`·`pdf`·`settings`·`recent`·`recording`·`player`를 `git mv`로 이동
  (이력 보존). `freedf`는 모듈 셔임(`pub(crate) use freedf_services::X::*;`)으로
  기존 `crate::X::*` 경로를 유지 — 호출부 무변화.
- 이동 중 정리: `pub(crate)` 항목 → `pub`(교차 크레이트 가시성, 20건),
  `pdf`가 `Pdfium` 타입을 재노출(freedf의 pdfium-render 직접 의존 제거),
  `settings::default_canvas_color`의 theme 참조를 리터럴로(서비스 계층은 egui 테마
  미의존 — 값은 NORD0 #2E3440 동일, 양쪽 동시 변경 주석), `server::normalized_base`
  → `pub` (freedf-gui 소비).
- 예외: `theme`(egui 스타일)과 `app/dictionary.rs`(오버레이 UI 포함)는 freedf에
  잔존 — settings의 `MacroKey::from_egui`만 예외적으로 egui::Key를 씀(services가
  egui에 얇게 의존).
- **검증**: `cargo test --workspace` 전부 통과 — freedf 92 + freedf-services 28
  (구 freedf 120을 정확히 분할) + freedf-gui 7(셸 6 + 서비스 스모크 1) + core/canvas/sync.
  freedf-gui는 이제 `freedf-services`를 직접 의존(`services_smoke` 테스트로 연결 확인).

### Phase 2 — 캔버스 (freedf-gui v1) — 최고 리스크 구간

- `<Raw>` 플레이스홀더 자리에 실제 페이지 렌더 + 잉크 오버레이 페인팅 연결:
  `freedf-core`의 프로젝션/변환 + `freedf-canvas` 메시 + pdfium 텍스처.
- 포인터/펜 입력 → `freedf-core` 커맨드 파이프라인 (기존 `app/input` 세션 라우터 재사용).
- **판단**: 캔버스는 painter 영역이라 elm-magic 어휘 밖 — `<Raw>` 사용이 *예외가
  아니라 정식 설계*. `<Raw>` 경계를 한 곳(`canvas.rs` 모듈)으로 몰아 둔다.
- **종료 조건**: freedf-gui에서 PDF 열기 → 확대/이동 → 잉크 스트로크 저장까지
  (freedf-core 저장소로) 동작. smoketest `20_ink_tool_picker` 상응 검증.

### Phase 2 — 캔버스 (freedf-gui v1) — **✅ 완료 (2026-09-17)**

- **완료**: `crates/freedf-gui/src/canvas.rs` — `<Raw>` 경계 뒤의 명령형 캔버스.
  잉크 지오메트리는 freedf와 같은 생성기(`freedf-canvas` mesher), 변환은
  `ViewTransform`, 저장은 `AnnotationStore`. 입력: 좌드래그=잉크, 우드래그=팬,
  휠=포인터 고정 줌. PDF 열기(모달 경로 입력) + 페이지 텍스처.
- 셸 통합: 툴바 줌/Open PDF/Clear Ink 버튼(elm 핸들러 → `canvas::*` 커맨드),
  상태바 캔버스 상태. **레이아웃 실측 노트**: 수평 Row 안의 수직 Col은 컨텐츠
  높이만 가용 높이로 받음(실측 57px) — 캔버스 `<Raw>`는 루트 Col 직접 자식.
- **검증**: 헤드리스 테스트 15건(egui 포인터 이벤트 주입 end-to-end 포함) 전부
  통과 + Xvfb 실행 캡처. `cargo test --workspace` 전체 통과.
- **v1 한계**: 필압 명목 1.0(Phase 4), PDF 페이지 이동(v2), 프레임마다 메시
  재굽기(BakeService 이식은 획 수 증가 시).

### Phase 3 — 위젯 UI 전면 재작성 (freedf-gui v2) — **진행 중 (2026-09-17 착수)**

- **✅ 계측 인프라 (완료)**: `dev.rs` 헬퍼 + `dev-automation` feature + 루트 스코프
  `freedf-gui.root` + 어댑터 버튼 `gui.<라벨 슬러그>`(중복 `.<n>`) + `canvas.surface`
  publish. 런처 `.edev-gui.toml`, 스모크 `smoketest-gui/10_launch_gui.luau` 통과.
  Phase 2 견고화도 함께 완료: 모달 뒤 잉크 차단(`sync_and_status` 동기화),
  팬 경계, PDF 렌더 재시도 루프 제거, 빈 탭 이름 가드, PDF 페이지 이동.
- **✅ 설정 창 + 잉크 기본값 저장/복원 (완료)**: Settings 모달(현재 리본 상태
  표시 + 저장). 파일 백엔드 `app_data_dir/gui-ink-defaults.json` — 표시 이름을
  저장하고 복원은 select_* 커맨드 경로로 폴백 내장. 시작 시 `load_defaults()`
  1회 호출(Canvas::default는 순수 유지). 테스트 30건(roundtrip·손상/미지 이름
  폴백·모달) 통과.
- **✅ 잉크 리본 + 토스트 (완료)**: 툴바 아래 2단 리본 — 도구(Pen/Fountain/
  Highlighter/Eraser)·색상(Black/Red/Blue)·굵기(Thin/Medium/Thick) 선택, 활성
  항목은 Strong 렌더. 문자열 커맨드(`select_tool`/`select_color`/`select_width`,
  알 수 없는 값은 기본값 폴백). 토스트는 3초 자동 만료 — 엔진이 (메시지, 시작
  시각)을 소유하고 상태바 자리를 대신 사용(레이아웃 흔들림 없음). Bookmark/잉크
  지우기/PDF 열기에 연결. 테스트 28건(리본→엔진 반영 end-to-end 포함) 통과.
- **✅ 진짜 문서 탭 + 북마크/아웃라인 패널 (완료)**: 캔버스 엔진이 문서 목록(`Doc`)을
  소유하고 셸은 `tab_names()`/`active_tab_id()`만 읽어 렌더 — 탭 이름과 문서의
  이중 소스 오브 트루스 제거. 탭 id(u64)로 선택(중복 이름 허용), 마지막 탭 닫기는
  빈 문서 리셋, 탭 전환/닫기 전 진행 중 획 마무리. 북마크 패널(토글/목록/점프,
  `freedf-core` store 북마크 API), 아웃라인 패널(`pdf.outline()` 트리 평탄화 —
  들여쓰기는 canvas 쪽에서 미리 계산). Open PDF는 새 문서 탭으로 열림.
  테스트 24건(문서별 잉크 분리·북마크·아웃라인 평탄화·탭 라이프사이클) 전부 통과.
- **✅ 테마 추출 + 창 배경 회귀 수정 (완료)**: 두 가지를 함께 처리했다.
  - **배경 회귀(실측 `#080808`)**: freedf-gui 창 배경이 Nord가 아니라 **근사 검정**으로
    보였다. 원인은 두 가지 — (1) eframe `clear_color` 기본값이 반투명 근사 검정
    `(12,12,12,α180)`이고, (2) elm-magic 셸은 egui 패널을 쓰지 않으므로 배경을
    칠하는 주체가 아무도 없었다(불투명 검정 위에 합성되어 `#080808`). 수정:
    `Host::clear_color`가 테마의 `window_fill`(Nord0 `#2E3440`)을 **불투명**으로
    반환하고, 루트 프레임(`shell::render_root`)이 같은 `window_fill`(Nord0)을 칠한다
    — 클리어 색과 같은 값이라 리사이즈 중 이음새가 없고, 원본 freedf 크롬(툴바·상태바
    실측 `#2E3440`)과 일치한다. 캔버스 스테이지 바탕은 `faint_bg_color`(Nord3).
    **패리티 잔여**: 원본 freedf는 사이드바 `#434C5E`(Nord2) — freedf-gui는 아직
    사이드바 배경을 따로 칠하지 않는다(Phase 3 UI 재작성에서 정리).
  - **테마 공유**: `crates/freedf-theme` 신설 — `nord.rs`/`tokens.rs`를 `git mv`로
    옮기고 `freedf`는 `pub use freedf_theme::{nord, tokens}`로 재노출(호출부 무변화,
    `super::` → `crate::`만 수정). 팔레트가 한 곳에만 존재하므로 두 바이너리의 색이
    갈라질 수 없다.
  - **루트 안쪽 여백 8px**: Windows 최대화 시 창이 좌우로 화면 밖에 밀려(DPI 배율에
    따라 증가, 150%에서 실측) 행 첫 글자가 잘렸다 — `shell::ROOT_INNER_MARGIN`으로
    상수화.
  - **DPI 디버그**: `FREEDF_GUI_PPP=1.5`로 배율을 흉내 내 재현 가능(환경변수, 기본
    동작 무변화).
  - **검증**: 실제 X11 창 캡처(합성 없음) 실측 — 루트 배경 `#2E3440`(Nord0, freedf
    크롬과 동일) · 스테이지 주변 `#4C566A`(Nord3) · `#080808` **없음**. 회귀 테스트 2건 추가
    (`freedf-theme`: 팔레트/설치가 라이트·다크 모두에 적용, `freedf-gui`: 루트 프레임과
    캔버스가 Nord 배경을 칠하고 클리어 색을 노출하지 않음) — 셸과 테스트가 같은
    렌더 경로(`render_root`)를 지나므로 배경이 다시 비면 테스트가 실패한다.
    *(이후 0.6 CSS 전환에서 freedf-gui 쪽 배경 렌더 테스트는 제거 — 스타일 해석은
    elm-magic의 계약이라 그쪽 테스트가 담당한다.)*
  - **주의(도구)**: eguidev 인프로세스 캡처는 반투명 클리어 색이 배경과 합성되어
    회색(58~80)으로 **잘못** 보였다 — 배경/클리어 색 검증은 반드시 실제 창 캡처
    (`xwininfo` + `import -window`)로 교차 확인한다.
- **✅ freedf-gui 스타일을 elm-magic 0.6 CSS로 전환 (완료)**: freedf-gui의
  `freedf-theme` 의존을 **0**으로 만들고, 셸의 시각 결정을 전부 elm-magic 0.6의
  CSS 속성으로 옮겼다.
  - **elm-magic 0.6.0**: 0.5까지 `css!`가 등록만 하던 것과 달리 **실제로 렌더**된다 —
    셀렉터(태그·클래스·`*`·복합·후손·자식·목록) · 캐스케이드(명시도→선언순) ·
    상속 · 상태(`:hover` `:active` `:focus` `:disabled`) · **36개 속성** ·
    팔레트 14토큰(`Palette`/`Token`/`Color`) · 스타일 적용 태그 14종.
  - **구성**: `crates/freedf-gui/src/style.rs` 하나가 스타일 출처 —
    `css!` 규칙(`.app` `.toolbar` `.ribbon` `.panel` `.panel_title` `.panel_item`
    `.status` `.muted` `.tabs` `.modal_actions` + 태그 `Col` `Row` `Button` `Text`
    `Strong` `Modal`)과 `palette()`. 렌더는 어댑터의
    `render_with_palette`로 팔레트를 넘겨 색 토큰(`bg: surface`)을 해석시킨다.
  - **대체된 것**: ① 배경/여백 — egui Frame 래퍼와 하드코딩 상수
    (`ROOT_INNER_MARGIN`)를 제거하고 루트 `Col`의 `.app { bg: background; padding: 8 }`
    가 담당. ② 버튼 — egui `Visuals` 대신 `Button` `Button:hover` `Button:active`
    규칙(CSS가 상태를 안다). ③ 툴바/리본/패널/상태바/모달 — 배경·여백·라운드·
    그림자를 CSS가 지정. ④ 캔버스(`<Raw>` painter, CSS 밖) — `style.rs`가 토큰
    색을 제공(`stage_color`/`page_border_color`)하고 canvas는 옮기기만 한다.
  - **남은 egui 설정은 예외 하나**: `<Raw>` 캔버스 · `<Input>` · 창 크롬 ·
    스크롤바는 elm-magic CSS가 닿지 않으므로 `style::install_egui_visuals`가
    최소한만 설정한다(색은 같은 팔레트 토큰 — 출처는 여전히 하나).
  - **스타일 테스트는 두지 않는다**: 스타일 해석/렌더는 elm-magic의 계약이라
    그쪽 테스트(`tests/style.rs` 25건 등, 워크스페이스 135건)가 담당한다.
    freedf-gui는 셸 동작 테스트만 유지한다.
- **✅ elm-magic 0.5.0 마이그레이션 (완료)**: crates.io 발행 0.5.0으로 전환(리비전 핀 제거
  — 발행본이 dev 브랜치 `d063fb21…`과 **소스 동일**임을 확인). `docs/elm-magic-bug-report.md`의
  3건이 수정되어 **워크어라운드 전부 제거**: 셸 컴포넌트가 `pub(crate) fn Shell`(vis 보존),
  `{if}` 안 지역 컬렉션이 `.iter().map(..)`(`sections`·`bookmarks`·`outline_entries`·`tab_names`),
  `remove(x)` 뒤에 다른 문장 허용. 파급: 전개가 클론을 하므로 `OutlineEntry`에 `#[derive(Clone)]`.
  회귀 테스트 3건(`bug1_*`/`bug2_*`/`bug3_*`)이 같은 3건을 고정 — 다시 깨지면 컴파일이 실패한다.
  검증: `ELM_MAGIC_DUMP=1` 덤프로 `#[derive] pub(crate) struct ShellProps`와
  `(sections).clone().into_iter().map` 확인, 0.5.0 이전 리비전으로는 26개 에러(대조군),
  freedf-gui 테스트 34건 통과. freedf(PoC 모달)는 코드 변경 없이 그대로 빌드된다.
- 라이브러리/아웃라인/북마크/미디어 패널, 3단 툴바, 설정 창, 검색 바, 토스트를
  elm-magic으로 **재작성** (기존 코드 복사가 아니라 `view!` 설계로 다시 씀 —
  이게 이 크레이트의 존재 이유).
- eguidev 계측 재등록: id는 `docs/eguidev-automation.md` 표 그대로
  (`toolbar.*`, `tabs.*`, `canvas.surface`, `toast.*`, `menu.*`). elm-magic 경계를
  넘는 위젯(어댑터가 그린 버튼)은 `Pass.buttons`의 `Response`에 계측을 붙이는
  얇은 래퍼를 만든다.
- **종료 조건**: smoketest 스위트를 freedf-gui 대상으로 녹색화 (기존 스위트를
  `smoketest/gui/`로 복제해 이행).

### Phase 4 — 플랫폼/입력 이식 (v2.5)

- Windows Ink 압력, 게임패드, `winstyle`(네이티브 창), 전역 키 훅, 화상 키보드
  우회 등 — UI 프레임워크와 무관한 코드는 그대로 이식.
- **종료 조건**: Windows 실기에서 잉크 압력/게임패드 동작 확인.

### Phase 5 — 스위치오버 (v3)

- `freedf-gui`를 기본 바이너리로. `freedf` 빈은 한동안 유지하되 deprecated 표기
  (호환용 경로: `--doc <id>` CLI, 설정/DB 마이그레이션 없음 — DB는 이미
  PostgreSQL 단일 진실이므로 상태 이전 불필요).
- `crates/freedf`의 UI 코드 삭제, 서비스 크레이트만 잔존 → 이후 크레이트명 정리.
- docs 갱신(README, UI-COMPONENTS, eguidev-automation의 대상 크레이트 명기).

## 3. 리스크 & 완화

| 리스크 | 영향 | 완화 |
|---|---|---|
| elm-magic 미성숙 — **버그 3건은 0.5.0에서 수정 완료**(`docs/elm-magic-bug-report.md`). 남은 제약: 리스트 아이템 필드 대입(`t.done = !t.done`), 한 행에 아이템 캡처 핸들러 2개 이상 | 재작성 중 컴파일/동작 장애 | crates.io 0.5.0 **버전 의존**(리비전 핀 해제) + 회귀 테스트 3건으로 고정. 잔여 제약은 설계로 회피(목록은 읽기 전용 + 커맨드 호출) |
| **렌더 순서 기반 슬롯** — 조건부로 등장하는 컴포넌트 순서가 바뀌면 상태 슬롯 섞임. **0.5.0에 keyed 슬롯(`<Row key={id}>` / `Arena::keyed_slot`)이 추가됨** | 패널 on/off 등 동적 UI에서 상태 오염 | freedf-gui는 상태를 컴포넌트 최상위에 두고 순서를 고정해 회피 중. 리스트/동적 자식에 keyed 슬롯을 도입하면 리스크 자체가 사라진다(다음 단계 후보) |
| eguidev 계약 id 재등록 누락 | 자동화/시각 리뷰 회귀 | 계약 표를 체크리스트화, smoketest를 스위치오버 게이트로 |
| 캔버스 성능/부드러움 재현 (연속 줌 최적화, One Euro 필터 등 freedf의 실측 튜닝) | 사용성 퇴보 | `ZOON-OPT.md`·`docs/OPTIMIZATION.md`의 계수/전략을 그대로 이식, 기존 테스트(freedf-core)가 로직을 보존 |
| Windows 전용 기능 (DWM/Mica, 잉크 압력) | Windows 품질 | Phase 4를 별도 단계로 분리 — Linux에서 먼저 기능 패리티 |
| 두 바이너리 공존 기간의 유지보수 분산 | 리소스 | Phase 1 이후 freedf는 feature freeze + 버그 픽스만 |

## 4. 지금 바로 하는 것

- [ ] `freedf` 리포지토리에 feature freeze 선언 (버그 픽스만)
- [x] Phase 1 완료: `freedf-services` 추출 (2026-09-17 — 위 참고)
- [x] `docs/elm-magic-bug-report.md` 업스트림 전달 → **0.5.0에서 3건 수정 완료, freedf-gui 워크어라운드 전부 제거 (2026-09-17)**
- [ ] 패리티 원장: README Features 목록을 체크리스트로 `docs/freedf-gui-parity.md`에 옮기고 Phase마다 갱신
