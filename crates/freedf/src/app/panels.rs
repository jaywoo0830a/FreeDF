//! Library (Notes/PDFs/Recents) and Outline side panels.
//!
//! Extracted from `app.rs` during the layered refactor — these are
//! `FreeDfApp` methods living in a submodule (`use super::*` re-exports the
//! app state types and shared helpers).

use super::*;

impl FreeDfApp {
    pub(crate) fn library_panel(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 5.0);
        ui.add_space(6.0);

        // ── 헤더 + 검색 필터 ──────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Library").strong().size(16.0));
            let total = self.notes.list().len() + self.recents.sorted().len();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format!("{total} items"))
                        .weak()
                        .small(),
                );
            });
        });
        ui.add_space(3.0);
        ui.add(
            egui::TextEdit::singleline(&mut self.library_filter)
                .hint_text("Search notes & files…")
                .desired_width(f32::INFINITY),
        );
        ui.add_space(2.0);
        ui.separator();

        let filter = self.library_filter.trim().to_lowercase();
        let matches = |t: &str| filter.is_empty() || t.to_lowercase().contains(&filter);
        let has_note = self.current_note.is_some();
        let mut rename_note = false;
        let mut delete_note = false;

        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                // ── Notes ──────────────────────────────────────────────
                let all_notes: Vec<(u64, String, usize)> = self
                    .notes
                    .list()
                    .iter()
                    .map(|m| (m.id, m.title.clone(), m.page_count))
                    .collect();
                let notes: Vec<(u64, String, usize)> = all_notes
                    .iter()
                    .filter(|(_, t, _)| matches(t))
                    .cloned()
                    .collect();
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(6.0, 0.0);
                    ui.label(icon_text(ui, "Notes", icons::NOTE_PENCIL));
                    ui.label(
                        egui::RichText::new(all_notes.len().to_string())
                            .weak()
                            .small(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(2.0, 0.0);
                        if ui
                            .add_enabled(
                                has_note,
                                egui::Button::new(icon_text(ui, "", icons::PENCIL_SIMPLE))
                                    .frame(false)
                                    .small(),
                            )
                            .on_hover_text("Rename current note")
                            .clicked()
                        {
                            rename_note = true;
                        }
                        if ui
                            .add_enabled(
                                has_note,
                                egui::Button::new(icon_text(ui, "", icons::TRASH_SIMPLE))
                                    .frame(false)
                                    .small(),
                            )
                            .on_hover_text("Delete current note")
                            .clicked()
                        {
                            delete_note = true;
                        }
                    });
                });
                ui.add_space(2.0);
                if notes.is_empty() {
                    ui.label(
                        egui::RichText::new("No notes yet — use ＋ New to create one.")
                            .weak()
                            .small(),
                    );
                } else {
                    for (id, title, page_count) in &notes {
                        let meta = if *page_count > 0 {
                            format!("{page_count}p")
                        } else {
                            String::new()
                        };
                        let selected = self.current_note == Some(*id);
                        if library_row(ui, selected, title, &meta) {
                            self.open_note(*id);
                        }
                    }
                }
                ui.add_space(4.0);
                ui.separator();

                // ── PDFs (recently opened files) ──────────────────────
                let files: Vec<RecentItem> = self
                    .recents
                    .sorted()
                    .into_iter()
                    .filter(|r| r.kind == RecentKind::File)
                    .cloned()
                    .collect();
                let visible: Vec<RecentItem> = files
                    .iter()
                    .filter(|f| matches(&f.title))
                    .cloned()
                    .collect();
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.label(icon_text(ui, "PDFs", icons::FILE_PDF));
                    ui.label(
                        egui::RichText::new(files.len().to_string())
                            .weak()
                            .small(),
                    );
                });
                ui.add_space(2.0);
                if visible.is_empty() {
                    ui.label(egui::RichText::new("No PDFs opened yet.").weak().small());
                } else {
                    for f in &visible {
                        if library_row(ui, false, &f.title, "PDF") {
                            if let Some(p) = &f.path {
                                self.open_pdf(p);
                            }
                        }
                    }
                }
                ui.add_space(4.0);
                ui.separator();

                // ── Recents (notes + PDFs) ────────────────────────────
                let recents: Vec<RecentItem> = self
                    .recents
                    .sorted()
                    .into_iter()
                    .filter(|r| matches(&r.title))
                    .cloned()
                    .collect();
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    ui.label(icon_text(ui, "Recents", icons::CLOCK_COUNTER_CLOCKWISE));
                    ui.label(
                        egui::RichText::new(self.recents.sorted().len().to_string())
                            .weak()
                            .small(),
                    );
                });
                ui.add_space(2.0);
                if recents.is_empty() {
                    ui.label(egui::RichText::new("No recent files yet.").weak().small());
                } else {
                    for item in &recents {
                        let meta = match item.kind {
                            RecentKind::Note => "note".to_string(),
                            RecentKind::File => "pdf".to_string(),
                        };
                        if library_row(ui, false, &item.title, &meta) {
                            match item.kind {
                                RecentKind::Note => {
                                    if let Some(id) = item.note_id {
                                        self.open_note(id);
                                    }
                                }
                                RecentKind::File => {
                                    if let Some(p) = &item.path {
                                        self.open_pdf(p);
                                    }
                                }
                            }
                        }
                    }
                }
                ui.add_space(4.0);
            });

        if rename_note {
            if let Some(id) = self.current_note {
                let current = self
                    .notes
                    .get(id)
                    .map(|m| m.title.clone())
                    .unwrap_or_default();
                let mut modal =
                    ModalState::ask_text("Rename Note", "New title:", TextAction::RenameNote);
                modal.text = current;
                self.modal = Some(modal);
            }
        }
        if delete_note {
            if let Some(id) = self.current_note {
                let mut modal = ModalState::confirm(
                    "Delete Note",
                    "Delete this note and all its annotations? This cannot be undone.",
                    ConfirmAction::DeleteNote,
                );
                modal.text = id.to_string();
                self.modal = Some(modal);
            }
        }
    }

    // ---------- UI: outline panel ----------

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
