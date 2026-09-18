//! Outline 패널 — PDF 목차 트리.

use super::*;

/// 깊이 1레벨당 들여쓰기(pt).
const OUTLINE_INDENT: f32 = 16.0;
/// 들여쓰기가 멈추는 최대 깊이 (이보다 깊으면 같은 깊이로 표시).
const OUTLINE_MAX_DEPTH: usize = 6;
/// 제목 표시 글자 수 상한 (넘으면 …으로 잘라 창 폭이 늘어나지 않게 함).
const OUTLINE_MAX_CHARS: usize = 56;

impl FreeDfApp {
    pub(crate) fn outline_panel(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing =
            egui::vec2(crate::ui::tokens::space::SM, crate::ui::tokens::space::XS);
        // 제목/개수 헤더는 오버레이 컨테이너가 담당 — 여기서는 목차 트리부터.
        ui.add_space(crate::ui::tokens::space::XS);
        if !self.outline_loaded {
            self.load_outline_if_needed();
        }
        if self.outline.is_empty() {
            panels::empty_state(ui, icons::LIST_BULLETS, "No outline in this PDF.");
            return;
        }
        let mut jump: Option<(String, usize)> = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                for entry in flatten(&self.outline) {
                    // 계층 2~N: 깊이 1당 16pt 들여쓰기, 깊이 6에서 상한.
                    // 깊은 계층 때문에 창 폭이 늘어나 닫기 버튼이 밀리는
                    // 문제를 막기 위해 제목도 적당한 길이로 자릅니다.
                    // 행은 Library와 같은 `library_row` 리듬(높이 S_36 · 호버 강조) —
                    // 세 오버레이가 하나의 행 어휘를 공유합니다. 오른쪽 메타로
                    // 이동할 페이지를 미리 보여줍니다(있을 때).
                    ui.horizontal(|ui| {
                        let depth = (entry.depth as f32).min(OUTLINE_MAX_DEPTH as f32);
                        ui.add_space(crate::ui::tokens::space::MD + depth * OUTLINE_INDENT);
                        let title = truncate_outline_title(&entry.node.title);
                        let meta = entry
                            .node
                            .page_index
                            .map(|p| format!("p {}", p + 1))
                            .unwrap_or_default();
                        // 전체 제목은 툴팁으로 — `library_row`가 Response를 돌려줍니다.
                        if library_row(ui, Some(icons::LIST_BULLETS), false, &title, &meta)
                            .on_hover_text(&entry.node.title)
                            .clicked()
                        {
                            if let Some(p) = entry.node.page_index {
                                jump = Some((entry.node.title.clone(), p));
                            }
                        }
                    });
                }
            });
        if let Some((title, page)) = jump {
            self.logger.log(AppEvent::OutlineJump { title, page });
            self.goto_page(page);
        }
    }
}

/// 목차 제목을 `OUTLINE_MAX_CHARS`자로 자르고 …을 붙입니다.
fn truncate_outline_title(title: &str) -> String {
    if title.chars().count() <= OUTLINE_MAX_CHARS {
        return title.to_string();
    }
    let mut out: String = title.chars().take(OUTLINE_MAX_CHARS).collect();
    out.push('…');
    out
}
