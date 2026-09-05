//! 프레임 조립과 그리기 대상(GPU/래스터) 추상화.
//!
//! UI 스레드가 하는 일은 **경량 커맨드 조립과 제출뿐**이어야 합니다.
//! [`FrameAssembler`]는 순수 함수 — 굽힌 페이지 + 뷰 → [`DrawCommand`] 목록.
//! 실제 GPU 제출은 [`Surface`] 구현체(앱 쪽 egui/wgpu)가 담당하고,
//! 테스트는 [`RecordingSurface`]로 커맨드를 캡처해 검증합니다.

use crate::bake::BakedPage;
use crate::geom::ViewTransform;
use crate::ink::Mesh;
use std::sync::Arc;

/// 메시에 적용할 변환 — 굽힌 메시는 페이지 좌표이므로
/// 그리기 단계에서 zoom(배율) + pan(이동)을 적용합니다.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Transform {
    pub translate: [f32; 2],
    pub scale: f32,
}

impl Transform {
    pub fn identity() -> Self {
        Self {
            translate: [0.0, 0.0],
            scale: 1.0,
        }
    }
}

/// UI 스레드가 서피스에 넘기는 경량 그리기 커맨드.
#[derive(Debug, Clone, PartialEq)]
pub enum DrawCommand {
    Clear { color: [f32; 4] },
    Mesh { mesh: Arc<Mesh>, transform: Transform },
}

/// 그리기 대상 — 앱이 GPU/래스터로 구현. 커맨드 제출만 있고 반환 없음.
pub trait Surface {
    fn submit(&mut self, commands: &[DrawCommand]);
}

/// 테스트/스냅샷용 캡처 서피스 — 제출된 커맨드를 그대로 기록합니다.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct RecordingSurface {
    pub commands: Vec<DrawCommand>,
}

impl Surface for RecordingSurface {
    fn submit(&mut self, commands: &[DrawCommand]) {
        self.commands.extend(commands.iter().cloned());
    }
}

/// 프레임 조립 — **순수 함수**. 굽힌 페이지를 뷰로 변환해 커맨드로 내립니다.
///
/// **계약**: 메시는 페이지 좌표로 구워져 있고, 여기서
/// `Transform { translate: pan, scale: zoom }`을 붙입니다. 팬만 바뀌면
/// 재굽기 없이 이 커맨드의 변환만 갱신하면 됩니다.
pub struct FrameAssembler;

impl FrameAssembler {
    pub fn assemble(page: &BakedPage, view: &ViewTransform) -> Vec<DrawCommand> {
        vec![DrawCommand::Mesh {
            mesh: Arc::new(page.mesh.clone()),
            transform: Transform {
                translate: [view.pan_x, view.pan_y],
                scale: view.zoom,
            },
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bake::BakeParams;
    use crate::scene::Revision;

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
}
