//! Media 패널 — 문서의 미디어 목록(재생/미리보기/외부 열기/다운로드/삭제).

use super::*;

fn fmt_secs(d: std::time::Duration) -> String {
    let s = d.as_secs();
    format!("{:02}:{:02}", s / 60, s % 60)
}

impl FreeDfApp {
    /// 미디어 패널 — 현재 문서의 미디어 업로드/목록/재생/미리보기/삭제.
    pub(crate) fn media_panel(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.strong("Media");
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
                .on_hover_text("Upload an audio / image / video file to this document")
                .clicked()
            {
                self.upload_media_dialog();
            }
            ui.add_space(6.0);
            let elapsed = self
                .recording
                .as_ref()
                .map(|r| now_ms().saturating_sub(r.started_ms()) / 1000)
                .unwrap_or(0);
            if self.recording.is_some() {
                ui.colored_label(
                    egui::Color32::RED,
                    format!("● {:02}:{:02}", elapsed / 60, elapsed % 60),
                );
                if ui
                    .small_button(icon_text(ui, "Stop", icons::STOP))
                    .on_hover_text("Stop and upload this recording")
                    .clicked()
                {
                    self.stop_recording_action();
                }
            } else if ui
                .small_button(icon_text(ui, "Record", icons::MICROPHONE))
                .on_hover_text("Record audio from your microphone")
                .clicked()
            {
                self.start_recording_action();
            }
        });
        ui.separator();

        // ── 인앱 스트리밍 재생기 (버퍼링/재생 상태) ──
        if self.streaming_dl.is_some() {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new());
                ui.label("Buffering…");
            });
            ui.separator();
        }
        if let Some(p) = &self.player {
            let mut stop = false;
            let mut seek = None;
            ui.horizontal(|ui| {
                if ui
                    .small_button(icon_text(
                        ui,
                        "",
                        if p.is_paused() { icons::PLAY } else { icons::PAUSE },
                    ))
                    .on_hover_text("Play / Pause")
                    .clicked()
                {
                    p.toggle();
                }
                let total = p.total().map(|d| d.as_secs_f32()).unwrap_or(0.0).max(0.001);
                let mut pos = p.elapsed().as_secs_f32().min(total);
                let resp = ui.add(
                    egui::Slider::new(&mut pos, 0.0..=total)
                        .show_value(false)
                        .trailing_fill(true),
                );
                if resp.changed() {
                    seek = Some(std::time::Duration::from_secs_f32(pos));
                }
                ui.label(format!(
                    "{} / {}",
                    fmt_secs(p.elapsed()),
                    fmt_secs(p.total().unwrap_or_default())
                ));
                if ui
                    .small_button(icon_text(ui, "", icons::X))
                    .on_hover_text("Stop playback")
                    .clicked()
                {
                    stop = true;
                }
            });
            if let Some(d) = seek {
                p.seek(d);
            }
            if stop {
                self.stop_player();
            }
            ui.separator();
        }

        if !self.media_config.enabled {
            ui.label("Media server is not configured yet.");
            if ui.button("Server settings").clicked() {
                self.server_settings_open = true;
            }
            return;
        }
        let Some(doc_id) = self.doc_id else {
            ui.label("Open a document to manage its media.");
            return;
        };
        // 문서가 바뀌었으면 목록 자동 갱신 (1회 — media_refresh가 id를 기록).
        if self.media_loaded_for != Some(doc_id) {
            self.media_items.clear();
            self.media_preview = None;
            self.media_refresh();
        }
        if let Some(status) = &self.media_status {
            ui.colored_label(ui.visuals().text_color(), status);
            ui.add_space(2.0);
        }

        // ── 인앱 이미지 미리보기 ──
        let mut close_preview = false;
        let preview = match &mut self.media_preview {
            Some(pv) => {
                if pv.texture.is_none() {
                    let image = pv.image.clone();
                    pv.texture = Some(ui.ctx().load_texture(
                        format!("media-preview-{}", pv.id),
                        image,
                        egui::TextureOptions::LINEAR,
                    ));
                }
                Some((pv.texture.as_ref().expect("texture").clone(), pv.name.clone()))
            }
            None => None,
        };
        if let Some((tex, name)) = preview {
            ui.horizontal(|ui| {
                ui.strong("Preview");
                ui.label(egui::RichText::new(name).weak());
                if ui
                    .small_button(icon_text(ui, "", icons::X))
                    .on_hover_text("Close preview")
                    .clicked()
                {
                    close_preview = true;
                }
            });
            ui.add(
                egui::Image::new(&tex)
                    .max_width(ui.available_width().min(480.0))
                    .max_height(280.0),
            );
            ui.separator();
        }
        if close_preview {
            self.media_preview = None;
        }

        if self.media_items.is_empty() {
            return;
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            // 액션은 목록 순회 후 처리 (self 빌림 충돌 방지).
            let items = self.media_items.clone();
            for item in &items {
                ui.horizontal(|ui| {
                    if item.kind == "audio"
                        && ui
                            .small_button(icon_text(ui, "", icons::PLAY))
                            .on_hover_text("Stream and play inside the app")
                            .clicked()
                    {
                        self.stream_media_item(item.clone());
                    }
                    if item.kind == "photo"
                        && ui
                            .small_button(icon_text(ui, "", icons::IMAGE_SQUARE))
                            .on_hover_text("Preview this image inside the app")
                            .clicked()
                    {
                        self.preview_media_item(item.clone());
                    }
                    if (item.kind == "video" || item.kind == "photo")
                        && ui
                            .small_button(icon_text(ui, "", icons::ARROW_SQUARE_OUT))
                            .on_hover_text("Download and open with your default app")
                            .clicked()
                    {
                        self.open_media_externally(item.clone());
                    }
                    if ui
                        .small_button(icon_text(ui, "", icons::DOWNLOAD_SIMPLE))
                        .on_hover_text("Download to disk")
                        .clicked()
                    {
                        self.download_media_dialog(item.clone());
                    }
                    let _ = ui
                        .add_sized(
                            egui::vec2(150.0, 18.0),
                            egui::Label::new(egui::RichText::new(&item.name))
                                .truncate()
                                .sense(egui::Sense::hover()),
                        )
                        .on_hover_text(&item.name);
                    ui.label(egui::RichText::new(&item.kind).weak().small());
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
