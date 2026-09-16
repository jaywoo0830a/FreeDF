    use super::*;

    /// 실제 렌더 경로(리본 → freedf-canvas 메시 → egui)를 돌려, 모든
    /// 정점이 유한하고 스트로크 근처에 머무는지 확인합니다.
    fn mesh_bounds(pts: &[[f32; 2]], halves: &[f32]) -> egui::Rect {
        let sp: Vec<freedf_canvas::StrokePoint> = pts
            .iter()
            .map(|p| freedf_canvas::StrokePoint {
                position: freedf_canvas::PagePoint::new(p[0], p[1]),
                pressure: 1.0,
                t_ms: 0,
                width: 0.0,
            })
            .collect();
        let mut cm = freedf_canvas::Mesh::default();
        freedf_canvas::append_stroke_ribbon(&mut cm, &sp, halves, 0.5, true, [255, 0, 0, 255], None);
        let mesh = canvas_mesh_to_egui(&cm, egui::pos2(0.0, 0.0), 1.0, 0.0, 0.0);
        assert!(!mesh.indices.is_empty(), "빈 메시");
        let mut bounds = egui::Rect::NOTHING;
        for v in &mesh.vertices {
            assert!(v.pos.x.is_finite() && v.pos.y.is_finite(), "NaN: {:?}", v.pos);
            bounds.extend_with(v.pos);
        }
        bounds
    }

    #[test]
    fn straight_stroke_mesh_stays_bounded() {
        let pts: Vec<[f32; 2]> = (0..24).map(|i| [20.0 + i as f32 * 8.0, 80.0]).collect();
        let halves: Vec<f32> = vec![1.5; 24];
        // 직선 렌즈도 본체가 빠지지 않아야 함 (면적 기준 완전 커버).
        match freedf_core::pen::stroke_geometry(&pts, &halves, true) {
            freedf_core::pen::StrokeFill::Tris(t) => {
                let area: f32 = t.tris.iter().fold(0.0, |acc, tr| {
                    let (a, b, c) = (
                        t.poly[tr[0] as usize],
                        t.poly[tr[1] as usize],
                        t.poly[tr[2] as usize],
                    );
                    acc + ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs()
                        * 0.5
                });
                let expected = 184.0 * 3.0 + std::f32::consts::PI * 1.5 * 1.5;
                assert!((area - expected).abs() < 1.0, "직선 렌즈 면적: {area} vs {expected}");
            }
            freedf_core::pen::StrokeFill::Fallback(_) => {
                panic!("직선 렌즈는 완전 분할이어야 함")
            }
        }
        let b = mesh_bounds(&pts, &halves);
        assert!(b.min.x > 10.0 && b.max.x < 210.0, "x 경계: {b:?}");
        assert!(b.min.y > 70.0 && b.max.y < 90.0, "y 경계: {b:?}");
    }

    #[test]
    fn scribble_cluster_mesh_stays_bounded() {
        // 스크린샷과 같은 밀집 클러스터 + 급격한 루프 입력 — 리본이 유한한
        // 경계 안에서 커버해야 합니다.
        let mut pts: Vec<[f32; 2]> = Vec::new();
        let mut t = 0.0f32;
        while t < std::f32::consts::TAU * 3.0 {
            let r = 20.0 + 8.0 * (t * 3.0).sin();
            pts.push([100.0 + r * t.cos(), 100.0 + r * t.sin() * 0.7]);
            t += 0.08;
        }
        let halves: Vec<f32> = pts
            .iter()
            .enumerate()
            .map(|(i, _)| 0.4 + 2.0 * (i as f32 / pts.len() as f32))
            .collect();
        let b = mesh_bounds(&pts, &halves);
        assert!(b.min.x > 50.0 && b.max.x < 150.0, "x 경계: {b:?}");
        assert!(b.min.y > 50.0 && b.max.y < 150.0, "y 경계: {b:?}");
    }

    #[test]
    fn duplicate_points_mesh_stays_bounded() {
        // 펜을 누른 채 정지(중복 점) → 필압 램프.
        let mut pts: Vec<[f32; 2]> = Vec::new();
        for _ in 0..6 {
            pts.push([50.0, 50.0]);
        }
        for i in 0..16 {
            pts.push([50.0 + i as f32 * 6.0, 50.0]);
        }
        let halves: Vec<f32> = (0..pts.len())
            .map(|i| 0.4 + 2.0 * (i as f32 / pts.len() as f32))
            .collect();
        let b = mesh_bounds(&pts, &halves);
        assert!(b.min.x > 30.0 && b.max.x < 160.0, "x 경계: {b:?}");
        assert!(b.min.y > 30.0 && b.max.y < 70.0, "y 경계: {b:?}");
    }

    #[test]
    fn ribbon_active_mesh_stays_bounded() {
        // 진행 중 획이 매 프레임 쓰는 리본 경로 — 유한하고 스트로크 근처에 머뭄.
        let mut pts: Vec<[f32; 2]> = Vec::new();
        let mut t = 0.0f32;
        while t < std::f32::consts::TAU * 2.0 {
            let r = 20.0 + 6.0 * (t * 2.0).sin();
            pts.push([100.0 + r * t.cos(), 100.0 + r * t.sin() * 0.6]);
            t += 0.05;
        }
        let halves: Vec<f32> = vec![2.0; pts.len()];
        for feather in [0.0f32, 0.5] {
            let rb = freedf_core::pen::stroke_ribbon(&pts, &halves, feather, true, None);
            assert_eq!(rb.verts.len(), rb.alphas.len());
            assert!(!rb.tris.is_empty());
            let sp: Vec<freedf_canvas::StrokePoint> = pts
                .iter()
                .map(|p| freedf_canvas::StrokePoint {
                    position: freedf_canvas::PagePoint::new(p[0], p[1]),
                    pressure: 1.0,
                    t_ms: 0,
                    width: 0.0,
                })
                .collect();
            let mut cm = freedf_canvas::Mesh::default();
            freedf_canvas::append_stroke_ribbon(
                &mut cm,
                &sp,
                &halves,
                feather,
                true,
                [255, 0, 0, 255],
                None,
            );
            let mesh = canvas_mesh_to_egui(&cm, egui::pos2(0.0, 0.0), 1.0, 0.0, 0.0);
            let mut bounds = egui::Rect::NOTHING;
            for v in &mesh.vertices {
                assert!(v.pos.x.is_finite() && v.pos.y.is_finite(), "NaN: {:?}", v.pos);
                bounds.extend_with(v.pos);
            }
            assert!(bounds.min.x > 50.0 && bounds.max.x < 150.0, "x: {bounds:?}");
            assert!(bounds.min.y > 50.0 && bounds.max.y < 150.0, "y: {bounds:?}");
        }
    }

    #[test]
    fn ink_grain_alphas_stay_bounded_for_ribbon() {
        // 통합 경로: 질감 밀도 × 포화 램프를 좌우 알파로 합성해 리본에 넣어도
        // 모든 정점 알파가 0..1 안에 머물고 정점/알파 수가 일치합니다.
        let pts: Vec<StrokePoint> = (0..64)
            .map(|i| StrokePoint::with_time(i as f32 * 3.0, 50.0, 0.5, i as u64 * 5))
            .collect();
        let g = freedf_core::ink::InkGrain::default();
        let dens = freedf_core::ink::stroke_ink_lr(ToolType::Fountain, &pts, g);
        let alphas: Vec<[f32; 2]> = pts
            .iter()
            .enumerate()
            .map(|(i, _)| {
                let sat = 0.35 + 0.65 * (i as f32 / pts.len() as f32);
                [
                    freedf_core::ink::combine_saturation(sat, dens[i][0]),
                    freedf_core::ink::combine_saturation(sat, dens[i][1]),
                ]
            })
            .collect();
        assert!(alphas
            .iter()
            .flatten()
            .all(|a| a.is_finite() && (0.0..=1.0).contains(a)));
        let p: Vec<[f32; 2]> = pts.iter().map(|q| [q.x, q.y]).collect();
        let halves = vec![1.0f32; pts.len()];
        let rb = freedf_core::pen::stroke_ribbon_lr(&p, &halves, 0.5, true, Some(&alphas));
        assert_eq!(rb.verts.len(), rb.alphas.len());
        assert!(rb.alphas.iter().all(|a| *a >= 0.0 && *a <= 1.0));
    }

    /// 증분 append와 좌표 변환은 freedf-canvas로 이전 — 여기서는 경계
    /// 어댑터(canvas Mesh → egui Mesh)가 팬/줌을 올바르게 적용하는지 검증.
    #[test]
    fn canvas_mesh_to_egui_applies_pan_and_zoom() {
        let mut mesh = freedf_canvas::Mesh {
            vertices: vec![[10.0, 20.0]],
            colors: vec![[0.0, 0.0, 0.0, 1.0]],
            indices: Vec::new(),
        };
        let out = canvas_mesh_to_egui(
            &mesh,
            egui::pos2(5.0, 6.0),
            2.0,
            100.0,
            200.0,
        );
        assert_eq!(out.vertices.len(), 1);
        let v = out.vertices[0];
        assert!((v.pos.x - (5.0 + 10.0 * 2.0 + 100.0)).abs() < 1e-4, "x: {}", v.pos.x);
        assert!((v.pos.y - (6.0 + 20.0 * 2.0 + 200.0)).abs() < 1e-4, "y: {}", v.pos.y);
        mesh.indices = vec![0];
        let out2 = canvas_mesh_to_egui(&mesh, egui::pos2(0.0, 0.0), 1.0, 0.0, 0.0);
        assert_eq!(out2.indices, vec![0]);
    }

    #[test]
    fn tilt_azimuth_maps_direction() {
        let (az, cos) = tilt_azimuth(&[20.0, 0.0]);
        assert!(az.abs() < 1e-3, "오른쪽 기울기 → 방위각 0");
        assert!((cos - 20.0f32.to_radians().cos()).abs() < 1e-4);
        let (az2, _) = tilt_azimuth(&[0.0, 25.0]);
        assert!(
            (az2 - std::f32::consts::FRAC_PI_2).abs() < 1e-3,
            "사용자 쪽 기울기 → +90°"
        );
    }

    #[test]
    fn clamp_azimuth_hand_keeps_half_plane() {
        // 오른손잡이: 오른쪽 반평면(|az| ≤ 90°)만.
        assert!(clamp_azimuth_hand(-0.6, false).abs() < 1.0);
        assert!(clamp_azimuth_hand(2.2, false).cos() >= 0.0, "왼쪽 아래 → 오른쪽");
        assert!(clamp_azimuth_hand(-2.2, false).cos() >= 0.0, "왼쪽 위 → 오른쪽");
        // 왼손잡이: 왼쪽 반평면만.
        assert!(clamp_azimuth_hand(0.6, true).cos() <= 0.0, "오른쪽 → 왼쪽");
        assert!(clamp_azimuth_hand(-2.2, true).cos() <= 0.0);
        // 경계는 유지.
        let f = std::f32::consts::FRAC_PI_2;
        assert!((clamp_azimuth_hand(f, false) - f).abs() < 1e-4);
        assert!((clamp_azimuth_hand(-f, true) - (-f)).abs() < 1e-4);
    }

    #[test]
    fn smooth_tilt_rejects_violent_jumps() {
        // 패드 진입 시 ±90° 스파이크가 연달아 와도 한 걸음이 24°×0.3 = 7.2°를
        // 넘지 않고, 같은 값이 계속되면 서서히 수렴합니다.
        let mut t = [0.0f32, 0.0];
        for _ in 0..8 {
            let prev = t;
            t = smooth_tilt(t, [90.0, -90.0]);
            assert!((t[0] - prev[0]).abs() <= 7.2 + 1e-3, "급격 점프 제한");
            assert!((t[1] - prev[1]).abs() <= 7.2 + 1e-3);
        }
        assert!(t[0] > 40.0 && t[1] < -40.0, "결국 목표로 수렴");
        // 상수 입력에는 정확히 수렴.
        let mut t2 = [10.0f32, -10.0];
        for _ in 0..50 {
            t2 = smooth_tilt(t2, [20.0, 5.0]);
        }
        assert!((t2[0] - 20.0).abs() < 0.5 && (t2[1] - 5.0).abs() < 0.5);
    }

    // ═══════════════════════════════════════════════════════════════════
    // ColorWheel (원형 색상 휠) 단위 테스트
    // ═══════════════════════════════════════════════════════════════════
    // 화면 없이 계산만으로 동작합니다.
    // 각 테스트는 "입력 → 기대 결과" 한 가지만 짧게 확인합니다.

    /// 테스트용 휠: 중심 (cx, cy)에 색 n개를 둡니다.
    fn make_wheel(cx: f32, cy: f32, n: usize) -> ColorWheel {
        let ring: Vec<[u8; 4]> = (0..n).map(|i| [i as u8, 0, 0, 255]).collect();
        ColorWheel {
            center: egui::pos2(cx, cy),
            ring,
        }
    }

    #[test]
    fn wheel_center_tap_returns_center() {
        // 가운데(현재 색)를 탭하면 "변경 없이 닫기"다.
        let wheel = make_wheel(100.0, 100.0, 4);
        assert_eq!(wheel.hit(egui::pos2(100.0, 100.0)), WheelHit::Center);
    }

    #[test]
    fn wheel_swatch_tap_selects_that_swatch() {
        // 각 색의 바로 위를 탭하면 그 색이 선택된다.
        let wheel = make_wheel(200.0, 300.0, 4);
        for i in 0..4 {
            assert_eq!(wheel.hit(wheel.swatch_pos(i)), WheelHit::Swatch(i));
        }
    }

    #[test]
    fn wheel_gap_tap_returns_backplate() {
        // 색과 색 사이의 빈 곳을 탭하면 "그냥 닫기"다.
        // (40, 0)은 두 스와치(위/아래)에서 멀고, 휠 반지름(56) 안쪽이다.
        let wheel = make_wheel(0.0, 0.0, 2);
        assert_eq!(wheel.hit(egui::pos2(40.0, 0.0)), WheelHit::Backplate);
    }

    #[test]
    fn wheel_outside_tap_is_ignored() {
        // 휠 밖을 탭하면 아무 일도 없다 (닫기는 다른 곳에서 처리).
        let wheel = make_wheel(0.0, 0.0, 3);
        assert_eq!(wheel.hit(egui::pos2(0.0, 200.0)), WheelHit::Outside);
    }

    #[test]
    fn wheel_first_swatch_is_at_twelve_oclock() {
        // 첫 색은 항상 12시 방향(중심 바로 위)에 있다.
        let wheel = make_wheel(0.0, 0.0, 4);
        let first = wheel.swatch_pos(0);
        assert!((first.x - 0.0).abs() < 0.01, "x는 중심과 같아야 함");
        assert!(first.y < 0.0, "y는 중심보다 위여야 함");
    }

    #[test]
    fn wheel_swatches_are_evenly_spaced() {
        // 모든 색은 중심에서 같은 거리에 있고, 서로 같은 각도 간격이다.
        let wheel = make_wheel(50.0, 60.0, 6);
        for i in 0..6 {
            let d = wheel.swatch_pos(i).distance(wheel.center);
            assert!((d - WHEEL_RING_R).abs() < 0.01, "중심에서 거리: {d}");
        }
        // 0번과 1번 색 사이의 각도 = 360°/6 = 60°.
        let a = wheel.swatch_pos(1) - wheel.center;
        let b = wheel.swatch_pos(0) - wheel.center;
        let angle = (a.dot(b) / (WHEEL_RING_R * WHEEL_RING_R)).acos();
        assert!((angle - std::f32::consts::TAU / 6.0).abs() < 0.01);
    }

    #[test]
    fn wheel_center_is_clamped_into_canvas() {
        // 펜이 캔버스 밖에 있어도 휠은 캔버스 안에 머문다.
        let canvas = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(400.0, 300.0));
        let center = ColorWheel::clamp_center(egui::pos2(-1000.0, 50.0), canvas);
        assert!(center.x >= canvas.min.x + WHEEL_BACK_R, "왼쪽 클램프");
        assert!(center.y >= canvas.min.y + WHEEL_BACK_R, "위쪽 클램프");
        let center = ColorWheel::clamp_center(egui::pos2(5000.0, 5000.0), canvas);
        assert!(center.x <= canvas.max.x - WHEEL_BACK_R, "오른쪽 클램프");
        assert!(center.y <= canvas.max.y - WHEEL_BACK_R, "아래쪽 클램프");
    }

    #[test]
    fn wheel_center_in_middle_stays_put() {
        // 펜이 캔버스 안쪽이면 그 자리에 휠이 열린다.
        let canvas = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(400.0, 300.0));
        let center = ColorWheel::clamp_center(egui::pos2(150.0, 120.0), canvas);
        assert_eq!(center, egui::pos2(150.0, 120.0));
    }

    #[test]
    fn tiny_canvas_uses_its_own_center() {
        // 캔버스가 휠보다 작으면 캔버스 중앙을 쓴다.
        let canvas = egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(80.0, 80.0));
        let center = ColorWheel::clamp_center(egui::pos2(9999.0, 9999.0), canvas);
        assert_eq!(center, canvas.center());
    }
