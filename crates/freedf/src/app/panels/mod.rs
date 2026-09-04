//! 사이드 패널 — Library(노트/PDF/최근) + 공용 헬퍼.
//!
//! 하위 모듈: [`outline`](PDF 목차), [`media`](문서 녹음).

pub(crate) use super::*;

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
        // 다중 삭제: (선택된 노트 id, 선택된 PDF id) — 확인 모달로 전달.
        let mut delete_selected: Option<(Vec<i64>, Vec<i64>)> = None;

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
                        let mut sel = self.sel_notes.contains(&(*id as i64));
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
                            if ui
                                .checkbox(&mut sel, "")
                                .on_hover_text("Select for multi-delete")
                                .changed()
                            {
                                if sel {
                                    self.sel_notes.insert(*id as i64);
                                } else {
                                    self.sel_notes.remove(&(*id as i64));
                                }
                            }
                            let meta = if *page_count > 0 {
                                format!("{page_count}p")
                            } else {
                                String::new()
                            };
                            let selected = self.current_note == Some(*id as i64);
                            if library_row(ui, selected, title, &meta) {
                                self.open_note(*id);
                            }
                        });
                    }
                    let n_sel = self.sel_notes.len();
                    if ui
                        .add_enabled(n_sel > 0, egui::Button::new(format!("Delete selected ({n_sel})")))
                        .on_hover_text(
                            "Delete all checked notes (and their annotations).",
                        )
                        .clicked()
                    {
                        let ids: Vec<i64> = self.sel_notes.iter().copied().collect();
                        delete_selected = Some((ids, Vec::new()));
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
                        let mut sel = f.doc_id.is_some_and(|d| self.sel_pdfs.contains(&d));
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
                            if ui
                                .checkbox(&mut sel, "")
                                .on_hover_text("Select for multi-delete")
                                .changed()
                            {
                                if let Some(d) = f.doc_id {
                                    if sel {
                                        self.sel_pdfs.insert(d);
                                    } else {
                                        self.sel_pdfs.remove(&d);
                                    }
                                }
                            }
                            if library_row(ui, false, &f.title, "PDF") {
                                if let Some(d) = f.doc_id {
                                    self.open_document(d);
                                }
                            }
                        });
                    }
                    let n_sel = self.sel_pdfs.len();
                    if ui
                        .add_enabled(n_sel > 0, egui::Button::new(format!("Delete selected ({n_sel})")))
                        .on_hover_text(
                            "Delete the checked PDF documents from the library \
                             (the original files on disk are left untouched).",
                        )
                        .clicked()
                    {
                        let paths: Vec<i64> = self.sel_pdfs.iter().copied().collect();
                        delete_selected = Some((Vec::new(), paths));
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
                            if let Some(doc_id) = item.doc_id {
                                self.open_document(doc_id);
                            }
                        }
                    }
                }
                ui.add_space(4.0);
            });

        if let Some((nids, ppaths)) = delete_selected {
            let mut parts: Vec<String> = Vec::new();
            if !nids.is_empty() {
                parts.push(format!("{} note(s) and their annotations", nids.len()));
            }
            if !ppaths.is_empty() {
                parts.push(format!("{} PDF document(s) from the library", ppaths.len()));
            }
            let msg = format!(
                "Delete {}?\nThis cannot be undone.",
                parts.join(" and ")
            );
            self.modal = Some(ModalState {
                kind: ModalKind::Confirm {
                    title: "Delete from Library".to_string(),
                    message: msg,
                    action: ConfirmAction::DeleteLibrary {
                        notes: nids,
                        pdfs: ppaths,
                    },
                },
                text: String::new(),
                pages: 1,
            });
        }

        if rename_note {
            if let Some(id) = self.current_note {
                let current = self
                    .notes
                    .get(id as u64)
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
}

fn format_bytes(n: i64) -> String {
    if n >= 1024 * 1024 {
        format!("{:.1} MB", n as f64 / 1_048_576.0)
    } else if n >= 1024 {
        format!("{:.0} KB", n as f64 / 1024.0)
    } else {
        format!("{n} B")
    }
}

mod media;
mod outline;
