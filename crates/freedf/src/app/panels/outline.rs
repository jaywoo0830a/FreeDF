//! Outline 패널 — PDF 목차 트리.

use super::*;

impl FreeDfApp {
    pub(crate) fn outline_panel(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 6.0);
        ui.add_space(4.0);
        ui.heading("Outline");
        ui.add_space(2.0);
        if !self.outline_loaded {
            self.load_outline_if_needed();
        }
        if self.outline.is_empty() {
            ui.label("No outline in this PDF.");
            return;
        }
        let mut jump: Option<(String, usize)> = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                let mut index = 0usize;
                for entry in flatten(&self.outline) {
                    index += 1;
                    let text = format!(
                        "{}{}",
                        "    ".repeat(entry.depth),
                        entry.node.title
                    );
                    let resp = ui.push_id(index, |ui| ui.selectable_label(false, text)).inner;
                    if resp.clicked() {
                        if let Some(p) = entry.node.page_index {
                            jump = Some((entry.node.title.clone(), p));
                        }
                    }
                }
            });
        if let Some((title, page)) = jump {
            self.logger.log(AppEvent::OutlineJump { title, page });
            self.goto_page(page);
        }
    }
}
