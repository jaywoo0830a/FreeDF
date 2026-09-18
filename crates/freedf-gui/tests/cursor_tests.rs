//! `cursor` 모듈 테스트 — gui가 소유한 **커서 기하/각도/표시 정책**만 검증한다.
//!
//! 스프라이트 자체는 그림이라 값으로 검증할 수 없다 — 대신 각도(손잡이 반평면),
//! 배럴 기하, 표시 히스테리시스(순수 함수)와 "모든 도구가 패닉 없이 그려진다"를 본다.

use eframe::egui;
use freedf_core::model::ToolType;
use freedf_gui::cursor::{
    self, clamp_azimuth_hand, hysteresis, pen_azimuth, pen_barrel, CursorSpec,
    CURSOR_STABLE_FRAMES, DEFAULT_PEN_AZ,
};

#[test]
fn clamp_azimuth_keeps_handed_half_plane() {
    // 오른손잡이: 오른쪽 반평면(|az| ≤ 90°).
    assert!(clamp_azimuth_hand(-0.6, false).abs() < 1.0);
    assert!(clamp_azimuth_hand(2.2, false).cos() >= 0.0);
    assert!(clamp_azimuth_hand(-2.2, false).cos() >= 0.0);
    // 왼손잡이: 왼쪽 반평면만.
    assert!(clamp_azimuth_hand(0.6, true).cos() <= 0.0);
    assert!(clamp_azimuth_hand(-2.2, true).cos() <= 0.0);
    // 경계(수직)는 그대로 유지된다.
    let f = std::f32::consts::FRAC_PI_2;
    assert!((clamp_azimuth_hand(f, false) - f).abs() < 1e-4);
    assert!((clamp_azimuth_hand(-f, true) - (-f)).abs() < 1e-4);
}

#[test]
fn azimuth_falls_back_to_handedness_default_without_tilt() {
    // 틸트 미지원 장치(None) → 손잡이 기본값, 유효 반평면 안.
    let (right, cos_r) = pen_azimuth(None, false);
    assert!((right - DEFAULT_PEN_AZ).abs() < 1e-6);
    assert_eq!(cos_r, 1.0);
    let (left, _) = pen_azimuth(None, true);
    assert!(left.cos() <= 0.0, "왼손잡이는 왼쪽 반평면: {left}");
    // 장치 틸트는 그대로 통과하되 반평면 제한을 받는다 (cos_pitch는 보존).
    let (az, cos_pitch) = pen_azimuth(Some((2.2, 0.5)), false);
    assert!(az.cos() >= 0.0);
    assert_eq!(cos_pitch, 0.5);
}

#[test]
fn barrel_grows_and_narrows_as_pen_tilts() {
    // 수직(cos=1) = 기본 길이/폭, 눕힘(cos=0) = 길고 좁게.
    let (len_upright, w_upright) = pen_barrel(1.0, 1.0);
    let (len_flat, w_flat) = pen_barrel(0.0, 1.0);
    assert!((len_upright - 24.0).abs() < 1e-4);
    assert!((w_upright - 5.5).abs() < 1e-4);
    assert!(len_flat > len_upright, "눕히면 배럴이 길어진다");
    assert!(w_flat < w_upright, "눕히면 폭이 좁아진다");
    // 배율은 길이/폭에 그대로 곱해진다 (설정 cursor_scale).
    let (len2, w2) = pen_barrel(1.0, 2.0);
    assert!((len2 - 48.0).abs() < 1e-4 && (w2 - 11.0).abs() < 1e-4);
    // 폭에는 최소값이 있다 (눕혀도 0으로 사라지지 않음).
    assert!(pen_barrel(0.0, 1.0).1 >= 1.8);
}

#[test]
fn cursor_needs_three_stable_frames() {
    // want=true가 3프레임 연속이면 나타난다.
    let (c1, s1) = hysteresis(false, true, 0, false, CURSOR_STABLE_FRAMES);
    assert_eq!((c1, s1), (1, false));
    let (c2, s2) = hysteresis(true, true, c1, s1, CURSOR_STABLE_FRAMES);
    assert_eq!((c2, s2), (2, false));
    let (c3, s3) = hysteresis(true, true, c2, s2, CURSOR_STABLE_FRAMES);
    assert_eq!((c3, s3), (3, true));
    // 뒤집히면 새 실행의 1번째 프레임 — 표시 상태는 유지(깜빡임 없음).
    let (c4, s4) = hysteresis(true, false, c3, s3, CURSOR_STABLE_FRAMES);
    assert_eq!((c4, s4), (1, true));
    // want=false가 3프레임 연속이면 사라진다.
    let (c5, s5) = hysteresis(false, false, c4, s4, CURSOR_STABLE_FRAMES);
    let (c6, s6) = hysteresis(false, false, c5, s5, CURSOR_STABLE_FRAMES);
    assert_eq!((c6, s6), (3, false));
}

/// 모든 도구의 스프라이트가 헤드리스 egui 프레임에서 패닉 없이 그려진다.
#[test]
fn every_tool_sprite_paints() {
    let ctx = egui::Context::default();
    let tools = [
        ToolType::Pen,
        ToolType::Fountain,
        ToolType::Highlighter,
        ToolType::Eraser,
        ToolType::Pan,
    ];
    let mut input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(400.0, 300.0),
        )),
        ..Default::default()
    };
    input.time = Some(1.0);
    let mut out = ctx.run_ui(input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let painter = ui.painter().clone();
            for tool in tools {
                // 틸트 있는 장치/왼손잡이/근접 감쇠까지 한 번에 지나간다.
                let spec = CursorSpec {
                    tool,
                    color: [255, 71, 66, 200],
                    width_pt: 8.0,
                    zoom: 1.25,
                    eraser_radius_px: 12.0,
                    tilt: Some((0.4, 0.6)),
                    left_handed: true,
                    scale: 1.0,
                    proximity: 0.8,
                };
                cursor::paint(&painter, egui::pos2(50.0, 50.0), 1.0, &spec);
            }
        });
    });
    // headless: 폰트 아틀라스 델타 소비 (다른 캔버스 테스트와 동일).
    out.textures_delta.clear();
}
