//! elm-magic 0.7.4 **버그 최소 재현** — freedf-gui UI 재설계 중 실측으로 찾은 3건.
//!
//! 이 파일은 freedf-gui 스타일과 **무관**하다: 재현에 필요한 CSS·컴포넌트가 전부
//! 여기 안에 있고, elm-magic 어댑터만 직접 호출한다. 창은 800×600으로 고정한다.
//!
//! ## 단언은 "현재(버그 있는) 동작"을 잠근다
//!
//! elm-magic이 아래 3건을 고치면 **이 테스트가 깨진다** — 그게 의도된 신호다.
//! 그때는 단언을 뒤집고 `docs/elm-magic-notes.md`의 실측 기록을 갱신한다.
//!
//! | # | 증상 | 실측 근거 |
//! |---|---|---|
//! | 1 | `wrap: true`가 무시된다(행이 줄바꿈하지 않음) | freedf-gui 1줄 툴바가 900px에서 6개 버튼 offscreen |
//! | 2 | 콘텐츠 크기 자식의 `justify: end`가 무효다 | before 캡처에서 About 버튼이 x≈760에 멈춤 |
//! | 3 | `width: fill`이 뒤 형제 자리를 비우지 않는다(밀린 형제가 창을 넘으면 조상 `max_rect`까지 팽창) | 캔버스 폭 1754 > 창 1100, 루트 rect 1241 |

use eframe::egui;
use elm_magic::style::Palette;

// ── 재현용 CSS (freedf-gui 스타일과 무관) ────────────────────────────────
elm_magic::css! {
    // 케이스 컨테이너 — 폭을 **300px로 고정**해 창 폭과 분리한다.
    .case { width: 300; gap: 4; }
    // `wrap: true` 재현용(같은 컨테이너에 wrap만 추가).
    .wrapcase { wrap: true; }
    // 케이스 3 — 창(800)보다 살짝 좁은 컨테이너. 밀려난 형제가 **창을 넘게** 만든다.
    .fillcase { width: 780; gap: 4; }
    // 폭 고정 버튼 — 텍스트/폰트에 따라 흔들리지 않게.
    .wide { width: 200; min-height: 20; }
    .mid { width: 100; min-height: 20; }
    // `justify: end`만 있는 **콘텐츠 크기** 자식 Row.
    .jrow { justify: end; gap: 4; }
    // 남은 폭을 전부 먹는 스페이서(자식 없음).
    .fill { width: fill; }
}

// ── 재현용 컴포넌트 ──────────────────────────────────────────────────────
elm_magic::view! {
    /// 케이스 1 — 폭 300 안에 200px 버튼 3개(합 608px) + `wrap: true`.
    /// wrap이 동작하면 2·3번이 다음 줄로 내려가야 한다.
    pub fn WrapCase() {
        <Row class="case wrapcase">
            <Button class="wide">"A"</Button>
            <Button class="wide">"B"</Button>
            <Button class="wide">"C"</Button>
        </Row>
    }

    /// 케이스 2 — 폭 300 안의 콘텐츠 크기 자식 Row에 `justify: end`.
    /// 동작하면 버튼이 오른쪽(≈x200)에 붙어야 한다.
    pub fn JustifyCase() {
        <Row class="case">
            <Row class="jrow">
                <Button class="mid">"J"</Button>
            </Row>
        </Row>
    }

    /// 케이스 3 — 창(800)보다 좁은 780 컨테이너 안에 `width: fill` 스페이서 + 100px 버튼.
    /// 스페이서가 뒤 형제 자리를 비우면 버튼이 780 안에 남아야 한다.
    pub fn FillCase() {
        <Row class="fillcase">
            <Col class="fill" />
            <Button class="mid">"F"</Button>
        </Row>
    }
}

/// 한 프레임 렌더 결과 — 버튼 사각형과 루트 ui의 `max_rect` 폭.
struct Rendered {
    buttons: Vec<egui::Rect>,
    root_max_width: f32,
}

/// 헤드리스로 컴포넌트 하나를 그리고 버튼 사각형을 모은다.
///
/// `run_ui`가 `&mut Ui`를 주므로 어댑터를 직접 호출한다 — 셸(`render_shell`)과
/// 같은 경로(`frame` → `render_with_palette`)를 쓴다.
fn render<C: elm_magic::Component>(
    elm: &mut elm_magic::Ctx,
    props: &C::Props,
    window: egui::Vec2,
) -> Rendered {
    let ctx = egui::Context::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, window)),
        ..Default::default()
    };
    let palette: Palette = freedf_gui::style::palette();
    let mut buttons = Vec::new();
    let mut root_max_width = 0.0;
    let mut out = ctx.run_ui(input, |ui| {
        let tree = elm_magic::frame::<C>(elm, props);
        let pass = elm_magic_egui::render_with_palette(ui, &tree, &mut elm.arena, &palette);
        buttons.extend(pass.buttons.iter().map(|(_, r)| r.rect));
        root_max_width = ui.max_rect().width();
    });
    out.textures_delta.clear();
    Rendered {
        buttons,
        root_max_width,
    }
}

const WINDOW: egui::Vec2 = egui::vec2(800.0, 600.0);

/// **버그 1** — `wrap: true`가 무시된다(행이 줄바꿈하지 않는다).
///
/// 기대(고쳐지면): 3번 버튼이 다음 줄로 내려가 `y`가 1번과 다르고, 모든 버튼의
/// 오른쪽 끝이 케이스 폭 300 안에 들어온다.
/// 현재: 세 버튼이 **같은 줄**에 있고 3번은 x≈408에서 끝난다(컨테이너 밖).
#[test]
fn wrap_true_is_ignored() {
    let mut elm = elm_magic::Ctx::default();
    let r = render::<WrapCase>(&mut elm, &WrapCaseProps::default(), WINDOW);
    eprintln!("[repro 1] wrap:true — buttons = {:?}", r.buttons);
    assert_eq!(r.buttons.len(), 3, "버튼 3개가 그려져야 한다");
    // 같은 줄 = y가 같다 (wrap이 동작하면 달라진다).
    let y0 = r.buttons[0].min.y;
    for b in &r.buttons {
        assert!(
            (b.min.y - y0).abs() < 1.0,
            "wrap: true가 무시되어 모두 한 줄에 있다: {:?}",
            r.buttons
        );
    }
    // 3번째 버튼이 컨테이너(300) 밖으로 나간다 — wrap이 동작하면 안 나간다.
    assert!(
        r.buttons[2].max.x > 300.0,
        "3번째 버튼이 300px 컨테이너를 넘어간다(측정: {})",
        r.buttons[2].max.x
    );
}

/// **버그 2** — 콘텐츠 크기 자식 Row의 `justify: end`가 무효다.
///
/// 기대(고쳐지면): 버튼이 부모 폭 300의 오른쪽 끝에 붙는다(x≈200).
/// 현재: 자식 Row가 콘텐츠 폭(100)이라 정렬할 여지가 없고 버튼이 x≈0에 남는다.
#[test]
fn justify_end_on_content_sized_child_does_nothing() {
    let mut elm = elm_magic::Ctx::default();
    let r = render::<JustifyCase>(&mut elm, &JustifyCaseProps::default(), WINDOW);
    eprintln!("[repro 2] justify:end — buttons = {:?}", r.buttons);
    assert_eq!(r.buttons.len(), 1);
    let b = r.buttons[0];
    // 자식 Row는 콘텐츠 폭 100뿐이라 `justify: end`가 밀어낼 공간이 없다.
    assert!(
        b.width() <= 101.0,
        "자식 Row가 콘텐츠 폭으로 줄어들지 않았다(측정 폭 {})",
        b.width()
    );
    assert!(
        b.min.x < 50.0,
        "justify: end가 무효라 버튼이 왼쪽에 남았다(측정 x={})",
        b.min.x
    );
}

/// **버그 3** — `width: fill`이 **뒤 형제 자리를 비우지 않아** 부모를 밀어낸다.
///
/// 기대(고쳐지면): 스페이서가 남는 폭(780 − 100 = 680)만 차지해 버튼이 780 안에 남는다.
/// 현재: 스페이서가 "그 시점의 남은 폭"(780)을 전부 차지해 버튼이 컨테이너 밖(x≈784)으로
/// 밀리고, 그 오버플로가 **창(800)을 넘어** 조상 `max_rect`를 팽창시킨다.
/// (freedf-gui 실측: 캔버스 폭 1754 > 창 1100, 루트 rect 1241 — 오버플로 + fill 조합.)
#[test]
fn width_fill_pushes_siblings_out_and_inflates_parent() {
    let mut elm = elm_magic::Ctx::default();
    let r = render::<FillCase>(&mut elm, &FillCaseProps::default(), WINDOW);
    eprintln!(
        "[repro 3] width:fill — buttons = {:?}, root max width = {}",
        r.buttons, r.root_max_width
    );
    assert_eq!(r.buttons.len(), 1);
    let b = r.buttons[0];
    // (a) 형제 자리를 비우지 않는다 — 컨테이너(780) 밖으로 밀린다.
    assert!(
        b.min.x >= 780.0,
        "스페이서가 남은 폭을 전부 먹어 버튼이 컨테이너 밖으로 밀려야 한다(측정 x={})",
        b.min.x
    );
    // (b) 밀린 형제가 창을 넘으면 조상 max_rect가 창 밖으로 팽창한다.
    assert!(
        r.root_max_width > WINDOW.x,
        "오버플로가 부모 max_rect를 창({}) 밖으로 팽창시켜야 한다(측정 {})",
        WINDOW.x,
        r.root_max_width
    );
}
