//! `surface` 모듈 단위 테스트 — `src/surface.rs`의 `#[cfg(test)]` 모듈에서 이동했습니다.

use freedf_canvas::surface::*;
use freedf_canvas::bake::BakedPage;
use freedf_canvas::geom::ViewTransform;
use freedf_canvas::ink::Mesh;
use freedf_canvas::bake::BakeParams;
use freedf_canvas::scene::Revision;

fn empty_page(zoom: f32) -> BakedPage {
    BakedPage {
        revision: Revision(1),
        params: BakeParams { zoom },
        mesh: Mesh::default(),
    }
}

/// 계약: RecordingSurface는 제출된 커맨드를 그대로 캡처합니다.
#[test]
fn recording_surface_captures_commands() {
    let mut surface = RecordingSurface::default();
    surface.submit(&[DrawCommand::Clear {
        color: [1.0, 1.0, 1.0, 1.0],
    }]);
    assert_eq!(surface.commands.len(), 1);
    assert!(matches!(surface.commands[0], DrawCommand::Clear { .. }));
}

/// 계약: 프레임 조립은 팬/줌을 Transform으로 옮기고 메시는 페이지 좌표 유지.
#[test]
fn frame_assembler_puts_view_into_transform() {
    let view = ViewTransform::new(1.5, 12.0, -7.0);
    let commands = FrameAssembler::assemble(&empty_page(1.5), &view);
    assert_eq!(commands.len(), 1);
    match &commands[0] {
        DrawCommand::Mesh { transform, mesh } => {
            assert_eq!(transform.translate, [12.0, -7.0]);
            assert!((transform.scale - 1.5).abs() < 1e-6);
            assert!(mesh.vertices.is_empty(), "스켈레톤 빈 메시");
        }
        _ => panic!("Mesh 커맨드여야 함"),
    }
}

/// 계약: 같은 페이지를 두 번 조립해도 동일 커맨드 (순수성).
#[test]
fn frame_assembler_is_pure() {
    let view = ViewTransform::new(1.0, 0.0, 0.0);
    let page = empty_page(1.0);
    assert_eq!(
        FrameAssembler::assemble(&page, &view),
        FrameAssembler::assemble(&page, &view)
    );
}
