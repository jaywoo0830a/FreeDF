//! Media 패널 — 문서의 녹음 목록(업로드/재생/삭제).

use super::*;

impl FreeDfApp {
    /// 녹음(미디어) 패널 — 현재 문서의 녹음 업로드/목록/재생/삭제.
    pub(crate) fn media_panel(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.strong("Recordings");
            ui.add_space(6.0);
            if ui
                .small_button(icon_text(ui, "", icons::ARROWS_CLOCKWISE))
                .on_hover_text("Refresh list from server")
                .clicked()
            {
                self.media_refresh();
            }
            if ui
                .small_button(icon_text(ui, "Upload", icons::UPLOAD_SIMPLE))
                .on_hover_text("Upload an audio file to this document")
                .clicked()
            {
                self.upload_media_dialog();
            }
        });
        ui.separator();

        if !self.media_config.enabled {
            ui.label("Media server is not configured yet.");
            if ui.button("Server settings").clicked() {
                self.server_settings_open = true;
            }
            return;
        }
        let Some(doc_id) = self.doc_id else {
            ui.label("Open a document to manage its recordings.");
            return;
        };
        // 문서가 바뀌었으면 목록 자동 갱신 (1회 — media_refresh가 id를 기록).
        if self.media_loaded_for != Some(doc_id) {
            self.media_items.clear();
            self.media_refresh();
        }
        if let Some(status) = &self.media_status {
            ui.colored_label(ui.visuals().text_color(), status);
            ui.add_space(2.0);
        }
        if self.media_items.is_empty() {
            return;
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            // 액션은 목록 순회 후 처리 (self 빌림 충돌 방지).
            let items = self.media_items.clone();
            for item in &items {
                ui.horizontal(|ui| {
                    if ui
                        .small_button(icon_text(ui, "", icons::PLAY))
                        .on_hover_text("Play in your default media player")
                        .clicked()
                    {
                        self.play_media_item(item.url.clone());
                    }
                    let _ = ui
                        .add_sized(
                            egui::vec2(150.0, 18.0),
                            egui::Label::new(egui::RichText::new(&item.name))
                                .truncate()
                                .sense(egui::Sense::hover()),
                        )
                        .on_hover_text(&item.name);
                    ui.label(
                        egui::RichText::new(format_bytes(item.size))
                            .weak()
                            .small(),
                    );
                    if ui
                        .small_button(icon_text(ui, "", icons::TRASH))
                        .on_hover_text("Delete this recording")
                        .clicked()
                    {
                        self.delete_media_item(item.id);
                    }
                });
            }
        });
    }
}
