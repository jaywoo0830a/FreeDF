//! `geom` 모듈 단위 테스트 — `src/geom.rs`의 `#[cfg(test)]` 모듈에서 이동했습니다.

use freedf_canvas::geom::*;

/// 계약: 페이지→뷰→페이지 왕복은 항등이어야 합니다 (팬·줌 무관).
#[test]
fn view_roundtrip_is_identity() {
    let view = ViewTransform::new(1.7, -33.0, 81.0);
    let p = PagePoint::new(123.4, 56.7);
    let back = view.view_to_page(view.page_to_view(p));
    assert!((back.x - p.x).abs() < 1e-3, "x: {} vs {}", back.x, p.x);
    assert!((back.y - p.y).abs() < 1e-3, "y: {} vs {}", back.y, p.y);
}

/// 계약: page_to_view는 zoom 배율 + pan 평행 이동.
#[test]
fn page_to_view_scales_then_translates() {
    let view = ViewTransform::new(2.0, 10.0, 20.0);
    let v = view.page_to_view(PagePoint::new(5.0, 6.0));
    assert!((v.x - 20.0).abs() < 1e-6);
    assert!((v.y - 32.0).abs() < 1e-6);
}

/// 계약: clamped_zoom은 팬을 건드리지 않고 줌만 제한합니다.
#[test]
fn clamped_zoom_keeps_pan() {
    let view = ViewTransform::new(99.0, 7.0, 8.0);
    let c = view.clamped_zoom(0.5, 2.0);
    assert_eq!(c.zoom, 2.0);
    assert_eq!((c.pan_x, c.pan_y), (7.0, 8.0));
}
