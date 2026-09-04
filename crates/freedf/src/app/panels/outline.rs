//! Outline 패널 — PDF 목차 트리.

use super::*;

impl FreeDfApp {
    pub(crate) fn outline_panel(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 2.0);
        ui.add_space(6.0);
        // 계층 1: 패널 제목 + 항목 수.
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Outline").strong().size(16.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!("{} entries", self.outline.len()))
                        .weak()
                        .small(),
                );
            });
        });
        ui.add_space(4.0);
        ui.separator();
        if !self.outline_loaded {
            self.load_outline_if_needed();
        }
        if self.outline.is_empty() {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.add_space(6.0);
                ui.label(egui::RichText::new("No outline in this PDF.").weak().small());
            });
            return;
        }
        let mut jump: Option<(String, usize)> = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                for entry in flatten(&self.outline) {
                    // 계층 2~N: 깊이 1당 16pt 들여쓰기 — 데이터 계층이
                    // 정렬로 바로 보입니다 (공백 문자열 대신 실제 인덴트).
                    ui.horizontal(|ui| {
                        ui.add_space(6.0 + entry.depth as f32 * 16.0);
                        let title = &entry.node.title;
                        if ui
                            .selectable_label(false, title)
                            .on_hover_text(title)
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
