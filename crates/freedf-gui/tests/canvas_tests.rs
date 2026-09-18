//! `canvas` 모듈 테스트 — `src/canvas.rs`의 `#[cfg(test)]` 모듈에서 이동했습니다.

use eframe::egui;
use freedf_canvas::{PagePoint, ViewTransform};
use freedf_core::model::StrokePoint;
use freedf_core::model::ToolType;
use freedf_core::transform::{MAX_ZOOM, MIN_ZOOM};
use freedf_gui::canvas::*;

/// 헤드리스 egui 프레임 하나 — `paint`를 `CentralPanel`에 직접 그린다
/// (셸 경유 테스트는 각자 셸을 그린다).
fn paint_frame(ctx: &egui::Context, events: Vec<egui::Event>) {
    let mut input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(800.0, 600.0),
        )),
        ..Default::default()
    };
    input.events = events;
    let mut out = ctx.run_ui(input, |ctx| {
        egui::CentralPanel::default().show(ctx, paint);
    });
    out.textures_delta.clear();
}

/// 좌클릭 한 번의 press/release 이벤트.
fn press(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    }
}

#[test]
fn zoom_at_keeps_pointer_page_point_fixed() {
    let view = ViewTransform::new(1.0, 0.0, 0.0);
    let pointer = [300.0_f32, 400.0];
    let page_pt = view.view_to_page(PagePoint::new(pointer[0], pointer[1]));
    let zoomed = zoom_at_view(view, pointer, 1.5);
    let still = zoomed.view_to_page(PagePoint::new(pointer[0], pointer[1]));
    assert!((page_pt.x - still.x).abs() < 1e-4);
    assert!((page_pt.y - still.y).abs() < 1e-4);
    assert!((zoomed.zoom - 1.5).abs() < 1e-6);
}

#[test]
fn zoom_clamped_to_core_bounds() {
    let zoomed = zoom_at_view(ViewTransform::new(MAX_ZOOM, 0.0, 0.0), [0.0, 0.0], 2.0);
    assert!((zoomed.zoom - MAX_ZOOM).abs() < 1e-6);
    let zoomed = zoom_at_view(ViewTransform::new(MIN_ZOOM, 0.0, 0.0), [0.0, 0.0], 0.1);
    assert!((zoomed.zoom - MIN_ZOOM).abs() < 1e-6);
}

/// 헤드리스 egui에 실제 포인터 이벤트를 주입해 잉크가 저장소에 기록되는지
/// 검증 (freedf-canvas 어댑터 테스트와 동일한 headless 방식).
#[test]
fn ink_stroke_lands_in_store() {
    with(|c| *c = Canvas::default());

    let ctx = egui::Context::default();
    let base = || egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(800.0, 600.0),
        )),
        ..Default::default()
    };
    let press = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    let frame = |events: Vec<egui::Event>| {
        let mut input = base();
        input.events = events;
        let mut out = ctx.run_ui(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| paint(ui));
        });
        // headless: 폰트 아틀라스 델타 소비 (어댑터 테스트와 동일).
        out.textures_delta.clear();
    };

    frame(vec![
        egui::Event::PointerMoved(egui::pos2(300.0, 300.0)),
        press(egui::pos2(300.0, 300.0), true),
    ]);
    // 프레임당 최신 포인터 위치 1개가 샘플링된다 (60fps 실사용과 동일).
    frame(vec![egui::Event::PointerMoved(egui::pos2(350.0, 340.0))]);
    frame(vec![egui::Event::PointerMoved(egui::pos2(400.0, 380.0))]);
    frame(vec![egui::Event::PointerMoved(egui::pos2(450.0, 420.0))]);
    frame(vec![press(egui::pos2(450.0, 420.0), false)]);

    with(|c| {
        assert_eq!(
            c.docs[0].store.total_stroke_count(),
            1,
            "획이 저장소에 기록되어야 한다"
        );
        let pts = &c.docs[0].store.strokes_on(0)[0].points;
        assert!(pts.len() >= 4, "4점 이상: {}", pts.len());
        assert!(pts[0].x < pts.last().unwrap().x);
        assert!(pts[0].y < pts.last().unwrap().y);
        assert!(pts[0].x > 0.0 && pts[0].x < BLANK_PAGE[0]);
        assert!(pts[0].y > 0.0 && pts[0].y < BLANK_PAGE[1]);
    });
}

/// 펜을 짧게 찍은 탭(한 프레임 안 press+release)도 **점 하나짜리 획**으로
/// 남는다 — 구 구현은 2점 미만을 버려 탭이 통째로 사라졌다.
#[test]
fn single_point_tap_commits_a_dot() {
    with(|c| *c = Canvas::default());
    let ctx = egui::Context::default();
    let at = egui::pos2(300.0, 300.0);
    paint_frame(
        &ctx,
        vec![
            egui::Event::PointerMoved(at),
            press(at, true),
            press(at, false),
        ],
    );
    with(|c| {
        assert_eq!(
            c.docs[0].store.total_stroke_count(),
            1,
            "탭도 점으로 남아야 한다"
        );
        assert_eq!(c.docs[0].store.strokes_on(0)[0].points.len(), 1);
    });
}

/// 팬(오른쪽 드래그) 뒤에도 **누른 자리**에 잉크가 남는다 — 종이/히트테스트/
/// 메시가 같은 뷰 변환(팬 포함)을 쓴다.
///
/// 불변식으로 검증한다: 팬 (dx, dy)는 페이지를 화면에서 (dx, dy)만큼 옮기므로,
/// 같은 페이지 점은 팬 전에는 P, 팬 후에는 P+(dx, dy)에서 눌린다.
#[test]
fn pan_keeps_press_position_in_sync() {
    let ctx = egui::Context::default();
    let tap = |at: egui::Pos2| {
        paint_frame(
            &ctx,
            vec![
                egui::Event::PointerMoved(at),
                press(at, true),
                press(at, false),
            ],
        );
    };
    let only_point = |page: usize| with(|c| c.docs[0].store.strokes_on(page)[0].points[0].clone());

    // ① 팬 없이 찍은 페이지 점.
    with(|c| *c = Canvas::default());
    let base = egui::pos2(350.0, 330.0);
    tap(base);
    let before = only_point(0);

    // ② 오른쪽 드래그로 팬 (+50, +30) — 같은 페이지 점을 노려 찍는다.
    with(|c| *c = Canvas::default());
    let right = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Secondary,
        pressed,
        modifiers: Default::default(),
    };
    let from = egui::pos2(300.0, 300.0);
    paint_frame(
        &ctx,
        vec![egui::Event::PointerMoved(from), right(from, true)],
    );
    paint_frame(
        &ctx,
        vec![egui::Event::PointerMoved(egui::pos2(350.0, 330.0))],
    );
    paint_frame(&ctx, vec![right(egui::pos2(350.0, 330.0), false)]);
    with(|c| {
        assert_eq!(c.docs[0].view.pan_x, 50.0);
        assert_eq!(c.docs[0].view.pan_y, 30.0);
    });
    tap(base + egui::vec2(50.0, 30.0));
    let after = only_point(0);

    assert!(
        (before.x - after.x).abs() < 0.01,
        "x가 어긋난다: {} vs {}",
        before.x,
        after.x
    );
    assert!(
        (before.y - after.y).abs() < 0.01,
        "y가 어긋난다: {} vs {}",
        before.y,
        after.y
    );
}

/// 지우개는 아직 지우기 세션이 없다 — 눌러도 패닉하지 않고 잉크를 남기지
/// 않는다 (코어 `Materials::for_tool(Eraser)`가 재료를 주지 않는다).
#[test]
fn eraser_press_leaves_no_ink() {
    with(|c| *c = Canvas::default());
    select_tool("Eraser");
    let ctx = egui::Context::default();
    let start = egui::pos2(300.0, 300.0);
    paint_frame(
        &ctx,
        vec![egui::Event::PointerMoved(start), press(start, true)],
    );
    paint_frame(
        &ctx,
        vec![egui::Event::PointerMoved(egui::pos2(340.0, 330.0))],
    );
    paint_frame(&ctx, vec![press(egui::pos2(340.0, 330.0), false)]);
    with(|c| {
        assert_eq!(
            c.docs[0].store.total_stroke_count(),
            0,
            "지우개는 잉크를 남기지 않는다"
        );
    });
}

/// 실제 앱 경로(elm-magic 셸 안의 `<Raw>` 캔버스)로도 획이 기록되는지.
///
/// `paint`를 직접 부르는 테스트는 셸의 배치/입력 상호작용을 우회한다 —
/// 여기서는 셸 전체를 그려 캔버스가 실제로 놓이는 자리에서 입력이 닿는지 본다.
#[test]
fn ink_stroke_lands_through_shell_raw() {
    with(|c| *c = Canvas::default());

    let mut elm = elm_magic::Ctx::default();
    let ctx = egui::Context::default();
    let base = || egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1100.0, 720.0),
        )),
        ..Default::default()
    };
    let press = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    let frame = |elm: &mut elm_magic::Ctx, events: Vec<egui::Event>| {
        let mut input = base();
        input.events = events;
        let mut out = ctx.run_ui(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                freedf_gui::shell::render_shell(ui, &mut *elm);
            });
        });
        out.textures_delta.clear();
    };

    // 셸을 두 프레임 그려 캔버스 자리를 확정한 뒤, 그 안쪽에 펜 드래그를 준다.
    frame(&mut elm, vec![]);
    frame(&mut elm, vec![]);
    let start = egui::pos2(500.0, 400.0);
    frame(
        &mut elm,
        vec![egui::Event::PointerMoved(start), press(start, true)],
    );
    for i in 1..=4 {
        let p = egui::pos2(500.0 + 20.0 * i as f32, 400.0 + 10.0 * i as f32);
        frame(&mut elm, vec![egui::Event::PointerMoved(p)]);
    }
    let end = egui::pos2(600.0, 460.0);
    frame(&mut elm, vec![press(end, false)]);

    with(|c| {
        assert_eq!(
            c.docs[0].store.total_stroke_count(),
            1,
            "셸 안의 캔버스에도 획이 기록되어야 한다"
        );
    });
}

/// 커밋된 획이 실제로 **화면 도형**으로 나오는지 (잉크 렌더 회귀 방지).
/// 입력이 아니라 렌더 경로만 본다 — 저장소에 획을 직접 넣고 한 프레임 그린 뒤
/// 프레임 출력에 정점을 가진 메시가 있는지 확인한다.
#[test]
fn committed_stroke_renders_mesh() {
    with(|c| {
        *c = Canvas::default();
        let pts = vec![
            StrokePoint {
                x: 100.0,
                y: 100.0,
                pressure: 1.0,
                t_ms: now_ms(),
                width: 2.0,
            },
            StrokePoint {
                x: 200.0,
                y: 160.0,
                pressure: 1.0,
                t_ms: now_ms(),
                width: 2.0,
            },
        ];
        c.doc()
            .store
            .add_stroke(0, ToolType::Pen, [26, 26, 28, 255], 2.5, pts);
    });

    let ctx = egui::Context::default();
    let input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(800.0, 600.0),
        )),
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| paint(ui));
    });
    out.textures_delta.clear();

    let mut mesh_vertices = 0usize;
    for clipped in &out.shapes {
        if let egui::Shape::Mesh(m) = &clipped.shape {
            mesh_vertices += m.vertices.len();
        }
    }
    assert!(mesh_vertices > 0, "커밋된 획이 메시로 그려져야 한다");
}

#[test]
fn clear_ink_empties_page() {
    with(|c| {
        *c = Canvas::default();
        let pts = vec![
            StrokePoint {
                x: 10.0,
                y: 10.0,
                pressure: 1.0,
                t_ms: now_ms(),
                width: 2.0,
            },
            StrokePoint {
                x: 40.0,
                y: 40.0,
                pressure: 1.0,
                t_ms: now_ms(),
                width: 2.0,
            },
        ];
        c.doc()
            .store
            .add_stroke(0, ToolType::Pen, [0, 0, 0, 255], 2.0, pts);
        assert_eq!(c.doc().store.total_stroke_count(), 1);
    });
    clear_ink();
    with(|c| assert_eq!(c.doc().store.total_stroke_count(), 0));
}

#[test]
fn page_nav_without_pdf_is_noop() {
    with(|c| *c = Canvas::default());
    page_next();
    page_prev();
    with(|c| {
        assert_eq!(c.doc().page, 0);
        assert!(c.doc().pdf.is_none());
        assert!(c.pdf_error.is_none());
    });
}

#[test]
fn open_pdf_without_engine_records_error_and_adds_no_doc() {
    with(|c| *c = Canvas::default());
    open_pdf("/nonexistent/no-such-file.pdf".to_string());
    with(|c| {
        assert!(c
            .pdf_error
            .as_deref()
            .map(|e| e.contains("PDF"))
            .unwrap_or(false));
        assert_eq!(c.docs.len(), 1);
    });
}

#[test]
fn tab_lifecycle_add_select_close() {
    with(|c| *c = Canvas::default());
    add_tab("Sketch 2".to_string());
    add_tab("Sketch 3".to_string());
    with(|c| {
        assert_eq!(c.docs.len(), 3);
        assert_eq!(c.docs[c.active].name, "Sketch 3", "새 탭이 활성이어야 한다");
    });
    // 이름이 같은 탭도 id로 구분해 선택한다 — 첫 "Untitled"의 id를 찾아 선택.
    let untitled_id = tab_names()
        .into_iter()
        .find(|(_, name)| name == "Untitled")
        .map(|(id, _)| id)
        .expect("Untitled 탭");
    select_tab(untitled_id);
    with(|c| assert_eq!(c.docs[c.active].name, "Untitled"));
    // 닫기: 활성 문서 제거, 남은 문서로 활성 이동.
    close_tab();
    with(|c| {
        assert_eq!(c.docs.len(), 2);
        assert_eq!(c.docs[c.active].name, "Sketch 2");
    });
    close_tab();
    // 마지막 남은 탭(Sketch 3)에 획을 추가한다.
    with(|c| {
        assert_eq!(c.docs.len(), 1);
        assert_eq!(c.docs[c.active].name, "Sketch 3");
        let pts = vec![
            StrokePoint {
                x: 1.0,
                y: 1.0,
                pressure: 1.0,
                t_ms: 0,
                width: 2.0,
            },
            StrokePoint {
                x: 9.0,
                y: 9.0,
                pressure: 1.0,
                t_ms: 0,
                width: 2.0,
            },
        ];
        c.doc()
            .store
            .add_stroke(0, ToolType::Pen, [0, 0, 0, 255], 2.0, pts);
    });
    // 마지막 탭을 닫으면 닫지 않고 **빈 문서로 리셋** (획도 사라진다).
    close_tab();
    with(|c| {
        assert_eq!(c.docs.len(), 1);
        assert_eq!(c.docs[0].name, "Untitled");
        assert_eq!(c.docs[0].store.total_stroke_count(), 0, "리셋된 빈 문서");
    });
}

#[test]
fn ink_is_isolated_per_tab() {
    with(|c| {
        *c = Canvas::default();
        let pts = vec![
            StrokePoint {
                x: 1.0,
                y: 1.0,
                pressure: 1.0,
                t_ms: 0,
                width: 2.0,
            },
            StrokePoint {
                x: 9.0,
                y: 9.0,
                pressure: 1.0,
                t_ms: 0,
                width: 2.0,
            },
        ];
        c.doc()
            .store
            .add_stroke(0, ToolType::Pen, [0, 0, 0, 255], 2.0, pts);
    });
    add_tab("Second".to_string()); // 새 문서로 전환됨
    with(|c| {
        // 문서별 저장소 분리 — 새 문서에는 획이 없고 첫 문서에만 있다.
        assert_eq!(c.docs[0].store.total_stroke_count(), 1);
        assert_eq!(c.docs[1].store.total_stroke_count(), 0);
    });
}

#[test]
fn bookmarks_toggle_and_list() {
    with(|c| *c = Canvas::default());
    assert!(bookmark_list().is_empty());
    toggle_bookmark();
    assert_eq!(bookmark_list(), vec![0]);
    go_to_page(0);
    toggle_bookmark();
    assert!(bookmark_list().is_empty());
}

#[test]
fn outline_flattens_tree_with_indent() {
    // PDF 없이도 평탄화 순수 함수를 검증한다 (깊이 들여쓰기 + 순서).
    use freedf_core::outline::OutlineNode;
    let tree = vec![OutlineNode::new(
        "Ch1",
        Some(0),
        vec![
            OutlineNode::new("1.1", Some(1), vec![]),
            OutlineNode::new(
                "1.2",
                Some(2),
                vec![OutlineNode::new("1.2.1", Some(3), vec![])],
            ),
        ],
    )];
    let mut out = Vec::new();
    flatten_outline(&tree, 0, &mut out);
    assert_eq!(out.len(), 4);
    assert_eq!(out[0].title, "Ch1");
    assert_eq!(out[0].page, 0);
    assert_eq!(out[1].title, "  1.1");
    assert_eq!(out[1].page, 1);
    assert_eq!(out[3].title, "    1.2.1");
    assert_eq!(out[3].page, 3);
}

#[test]
fn outline_list_without_pdf_is_empty() {
    with(|c| *c = Canvas::default());
    assert!(outline_list().is_empty());
}

#[test]
fn ribbon_selects_tool_color_width() {
    with(|c| *c = Canvas::default());
    assert_eq!(tool_name(), "Pen");
    assert_eq!(color_name(), "Black");
    assert_eq!(width_name(), "Medium");
    select_tool("Highlighter");
    assert_eq!(tool_name(), "Highlighter");
    select_tool("Fountain");
    assert_eq!(tool_name(), "Fountain");
    select_tool("Eraser");
    assert_eq!(tool_name(), "Eraser");
    select_tool("Pen");
    assert_eq!(tool_name(), "Pen");
    select_color("Red");
    assert_eq!(color_name(), "Red");
    select_color("Blue");
    assert_eq!(color_name(), "Blue");
    select_color("Black");
    assert_eq!(color_name(), "Black");
    select_width("Thin");
    assert_eq!(width_name(), "Thin");
    select_width("Thick");
    assert_eq!(width_name(), "Thick");
    // 알 수 없는 값 — 프리셋 기본값으로 폴백.
    select_tool("Nonsense");
    assert_eq!(tool_name(), "Pen");
    select_color("Nonsense");
    assert_eq!(color_name(), "Black");
    select_width("Nonsense");
    assert_eq!(width_name(), "Medium");
}

#[test]
fn toast_expires_after_delay() {
    with(|c| *c = Canvas::default());
    assert!(toast().is_none());
    show_toast("hello");
    assert_eq!(toast().as_deref(), Some("hello"));
    // 표시 시작 시각을 만료 시각 이전으로 되돌려 시간 경과를 시뮬레이션.
    with(|c| {
        if let Some((_, at)) = c.toast.as_mut() {
            *at -= std::time::Duration::from_secs(TOAST_SECS + 1);
        }
    });
    assert!(toast().is_none());
}

#[test]
fn actions_raise_toasts() {
    with(|c| *c = Canvas::default());
    toggle_bookmark();
    assert!(toast().unwrap().contains("북마크 추가"));
    toggle_bookmark();
    assert!(toast().unwrap().contains("북마크 제거"));
    clear_ink();
    assert!(toast().unwrap().contains("잉크"));
}

#[test]
fn ink_defaults_roundtrip_and_fallback() {
    let path = std::env::temp_dir().join(format!("freedf-gui-test-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&path);
    // 1) 현재 상태 저장 → 엔진을 다르게 바꾸고 → 복원이 되돌린다.
    with(|c| *c = Canvas::default());
    select_tool("Highlighter");
    select_color("Red");
    select_width("Thick");
    save_defaults_to(&path).expect("save");
    with(|c| *c = Canvas::default());
    assert_eq!(tool_name(), "Pen");
    let restored = load_defaults_from(&path).expect("load");
    assert_eq!(restored.tool, "Highlighter");
    assert_eq!(restored.color, "Red");
    assert_eq!(restored.width, "Thick");
    assert_eq!(tool_name(), "Highlighter");
    assert_eq!(color_name(), "Red");
    assert_eq!(width_name(), "Thick");
    // 2) 손상된 파일 — 오류를 돌려주고 엔진은 그대로 (조용한 폴백은 load_defaults 몫).
    std::fs::write(&path, "not json").unwrap();
    assert!(load_defaults_from(&path).is_err());
    assert_eq!(tool_name(), "Highlighter");
    // 3) 파일에 알 수 없는 이름 — select_* 폴백으로 기본값 적용.
    std::fs::write(&path, r#"{"tool":"Warp","color":"Neon","width":"Huge"}"#).unwrap();
    let fallback = load_defaults_from(&path).expect("parse");
    assert_eq!(fallback.tool, "Warp"); // 저장 값은 그대로 (기록 보존)
    assert_eq!(tool_name(), "Pen"); // 적용은 폴백
    assert_eq!(color_name(), "Black");
    assert_eq!(width_name(), "Medium");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn modal_blocks_canvas_input() {
    with(|c| *c = Canvas::default());
    sync_and_status(true); // 모달 열림 — 캔버스 입력 차단

    let ctx = egui::Context::default();
    let base = || egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(800.0, 600.0),
        )),
        ..Default::default()
    };
    let press = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Default::default(),
    };
    let frame = |events: Vec<egui::Event>| {
        let mut input = base();
        input.events = events;
        let mut out = ctx.run_ui(input, |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| paint(ui));
        });
        out.textures_delta.clear();
    };

    frame(vec![
        egui::Event::PointerMoved(egui::pos2(300.0, 300.0)),
        press(egui::pos2(300.0, 300.0), true),
    ]);
    frame(vec![egui::Event::PointerMoved(egui::pos2(350.0, 340.0))]);
    frame(vec![press(egui::pos2(350.0, 340.0), false)]);
    with(|c| {
        assert_eq!(
            c.docs[0].store.total_stroke_count(),
            0,
            "모달 중에는 획이 없어야 한다"
        )
    });

    sync_and_status(false); // 모달 닫힘 — 이제 그려진다
    frame(vec![
        egui::Event::PointerMoved(egui::pos2(300.0, 300.0)),
        press(egui::pos2(300.0, 300.0), true),
    ]);
    frame(vec![egui::Event::PointerMoved(egui::pos2(350.0, 340.0))]);
    frame(vec![press(egui::pos2(350.0, 340.0), false)]);
    with(|c| {
        assert_eq!(
            c.docs[0].store.total_stroke_count(),
            1,
            "모달이 닫히면 그려진다"
        )
    });
}

// ── 지우개·이력·저장/불러오기·스무딩 — **코어 API 배선**만 본다 ──────────
//
// 코어가 이미 보장하는 것(지우기 히트 기하, 1€ 필터의 평활, 이력의 역연산,
// JSON 왕복)은 freedf-core의 테스트가 덮는다. 여기서 검증하는 것은
// "freedf-gui가 그 API를 **어느 문서에, 몇 단계로** 부르는가"다.

/// 계약: 지우개 드래그는 획을 지우고, **세션 하나(누름~뗌)가 undo 한 단계**다.
#[test]
fn eraser_session_is_one_undo_step() {
    with(|c| *c = Canvas::default());
    let ctx = egui::Context::default();
    let a = egui::pos2(300.0, 300.0);
    let b = egui::pos2(400.0, 340.0);
    let tap = |at: egui::Pos2| {
        paint_frame(
            &ctx,
            vec![
                egui::Event::PointerMoved(at),
                press(at, true),
                press(at, false),
            ],
        );
    };

    // 펜으로 두 점을 찍는다 (획 2개 + 이력 2단계).
    tap(a);
    tap(b);
    with(|c| {
        assert_eq!(
            c.docs[0].store.total_stroke_count(),
            2,
            "펜 탭이 획으로 남는다"
        );
        assert_eq!(c.docs[0].history.undo_len(), 2);
    });

    // 지우개 한 세션으로 둘 다 지운다.
    select_tool("Eraser");
    paint_frame(&ctx, vec![egui::Event::PointerMoved(a), press(a, true)]);
    paint_frame(&ctx, vec![egui::Event::PointerMoved(b)]);
    paint_frame(&ctx, vec![press(b, false)]);
    with(|c| {
        assert_eq!(
            c.docs[0].store.total_stroke_count(),
            0,
            "지우개가 반경 안의 획을 지운다"
        );
        assert_eq!(
            c.docs[0].history.undo_len(),
            3,
            "지우기 세션은 이력 한 단계(획2 + 지우기1)"
        );
    });

    // 한 단계이므로 undo 한 번에 둘 다 돌아온다.
    undo();
    with(|c| {
        assert_eq!(
            c.docs[0].store.total_stroke_count(),
            2,
            "undo 한 번에 둘 다 복원"
        )
    });
    redo();
    with(|c| {
        assert_eq!(
            c.docs[0].store.total_stroke_count(),
            0,
            "redo 한 번에 둘 다 제거"
        )
    });
}

/// 계약: 이력은 **문서별**이다 — 다른 탭에서 undo해도 이 탭은 그대로다.
#[test]
fn history_is_per_document() {
    with(|c| *c = Canvas::default());
    let ctx = egui::Context::default();
    let at = egui::pos2(300.0, 300.0);
    paint_frame(
        &ctx,
        vec![
            egui::Event::PointerMoved(at),
            press(at, true),
            press(at, false),
        ],
    );
    with(|c| assert_eq!(c.docs[0].store.total_stroke_count(), 1));

    add_tab("Second".to_string());
    with(|c| {
        assert!(!c.doc().history.can_undo(), "새 문서는 이력이 비어 있다");
    });
    undo();
    with(|c| {
        assert_eq!(
            c.docs[0].store.total_stroke_count(),
            1,
            "다른 문서는 손대지 않는다"
        );
        assert_eq!(c.docs[1].store.total_stroke_count(), 0);
    });
}

/// 계약: 저장/불러오기가 **활성 문서**를 파일로 왕복하고, 불러온 문서의 이력은
/// 초기화된다 (방금 읽은 상태를 되돌릴 수는 없다).
#[test]
fn save_and_load_document_edits() {
    let path =
        std::env::temp_dir().join(format!("freedf-gui-doc-test-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&path);

    with(|c| *c = Canvas::default());
    let ctx = egui::Context::default();
    let at = egui::pos2(300.0, 300.0);
    paint_frame(
        &ctx,
        vec![
            egui::Event::PointerMoved(at),
            press(at, true),
            press(at, false),
        ],
    );
    save_doc_to(&path).expect("save");

    // 엔진을 통째로 비우고 다시 불러온다.
    with(|c| *c = Canvas::default());
    load_doc_from(&path).expect("load");
    with(|c| {
        assert_eq!(
            c.docs[0].store.total_stroke_count(),
            1,
            "저장했던 획이 돌아온다"
        );
        assert!(
            !c.docs[0].history.can_undo(),
            "불러오기는 이력을 초기화한다"
        );
    });

    assert!(load_doc_from(std::path::Path::new("/nonexistent/no.json")).is_err());
    let _ = std::fs::remove_file(&path);
}

/// 계약: 스무딩 프리셋 매핑 — 알 수 없는 이름은 기본값(Off)으로 폴백한다.
#[test]
fn smoothing_presets_map_and_fall_back() {
    assert_eq!(smoothing_name_for(0.0), "Off");
    assert_eq!(smoothing_name_for(0.4), "Normal");
    assert_eq!(smoothing_name_for(0.9), "Custom");

    with(|c| *c = Canvas::default());
    assert_eq!(smoothing_name(), "Off", "기본값은 Off (freedf 설정 기본)");
    select_smoothing("Strong");
    assert_eq!(smoothing_name(), "Strong");
    with(|c| assert!((c.smoothing - 0.7).abs() < 1e-6));
    select_smoothing("Nonsense");
    assert_eq!(smoothing_name(), "Off", "알 수 없는 이름은 기본값");
    with(|c| assert!(c.smoothing.abs() < 1e-6));
}
