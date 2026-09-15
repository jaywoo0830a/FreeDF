//! 사이드 패널 — Library(노트/PDF/최근) + 공용 헬퍼.
//!
//! 하위 모듈: [`outline`](PDF 목차), [`media`](문서 녹음).

pub(crate) use super::*;

impl FreeDfApp {
    pub(crate) fn library_panel(&mut self, ui: &mut egui::Ui) {
        ui.spacing_mut().item_spacing = egui::vec2(crate::ui::tokens::space::SM, crate::ui::tokens::space::SM);
        // 제목/개수 헤더는 오버레이 컨테이너가 담당 — 여기서는 검색부터.
        ui.add_space(crate::ui::tokens::space::SM);
        crate::ui::form::text(&mut self.library_filter)
            .hint("Search notes & files…")
            .width(f32::INFINITY)
            .show(ui);
        ui.add_space(crate::ui::tokens::space::SM);
        ui.separator();

        let filter = self.library_filter.trim().to_lowercase();
        let matches = |t: &str| filter.is_empty() || t.to_lowercase().contains(&filter);
        let has_note = self.current_note.is_some();
        let mut rename_note = false;
        let mut delete_note = false;
        // 다중 삭제: (선택된 노트 id, 선택된 PDF id) — 확인 모달로 전달.
        let mut delete_selected: Option<(Vec<i64>, Vec<i64>)> = None;
        // PDF 다운로드 (서버 → 로컬 디스크) — 행의 아이콘 버튼에서 요청.
        let mut download_pdf: Option<(i64, String)> = None;

        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(crate::ui::tokens::space::SM, crate::ui::tokens::space::SM);
                // ── Notes (계층 2: 섹션 헤더 + 행) ──
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
                ui.add_space(crate::ui::tokens::space::SM);
                ui.horizontal(|ui| {
                    section_header(ui, icons::NOTE_PENCIL, "Notes", all_notes.len());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(crate::ui::tokens::space::SM, 0.0);
                        // 섹션 헤더의 액션도 표준 아이콘 버튼(S_36 · 계약 id)으로 —
                        // 오버레이 헤더의 닫기와 같은 어휘를 공유합니다.
                        if crate::ui::kit::IconButton::new(
                            icons::TRASH_SIMPLE,
                            "Delete current note",
                        )
                        .hint("Delete current note")
                        .test_id("notes.delete")
                        .enabled(has_note)
                        .show(ui)
                        .clicked()
                        {
                            delete_note = true;
                        }
                        if crate::ui::kit::IconButton::new(
                            icons::PENCIL_SIMPLE,
                            "Rename current note",
                        )
                        .hint("Rename current note")
                        .test_id("notes.rename")
                        .enabled(has_note)
                        .show(ui)
                        .clicked()
                        {
                            rename_note = true;
                        }
                    });
                });
                if notes.is_empty() {
                    empty_state(ui, icons::NOTE_PENCIL, "No notes yet — use ＋ New to create one.");
                } else {
                    for (id, title, page_count) in &notes {
                        let mut sel = self.sel_notes.contains(&(*id as i64));
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(crate::ui::tokens::space::SM, 0.0);
                            if crate::ui::check(ui, &mut sel, "", "Select for multi-delete")
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
                            if library_row(ui, Some(icons::NOTE_PENCIL), selected, title, &meta)
                                .clicked()
                            {
                                self.open_note(*id);
                            }
                        });
                    }
                    let n_sel = self.sel_notes.len();
                    if n_sel > 0 {
                        ui.horizontal(|ui| {
                            ui.add_space(crate::ui::scale::hrem(3));
                            if ui
                                .button(format!("Delete selected ({n_sel})"))
                                .on_hover_text(
                                    "Delete all checked notes (and their annotations).",
                                )
                                .clicked()
                            {
                                let ids: Vec<i64> = self.sel_notes.iter().copied().collect();
                                delete_selected = Some((ids, Vec::new()));
                            }
                        });
                    }
                }
                ui.add_space(crate::ui::tokens::space::SM);
                ui.separator();

                // ── PDFs (계층 2) ──
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
                ui.add_space(crate::ui::tokens::space::SM);
                ui.horizontal(|ui| {
                    section_header(ui, icons::FILE_PDF, "PDFs", files.len());
                });
                if visible.is_empty() {
                    empty_state(ui, icons::FILE_PDF, "No PDFs opened yet.");
                } else {
                    for f in &visible {
                        let mut sel = f.doc_id.is_some_and(|d| self.sel_pdfs.contains(&d));
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(crate::ui::tokens::space::SM, 0.0);
                            if crate::ui::check(ui, &mut sel, "", "Select for multi-delete")
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
                            if let Some(d) = f.doc_id {
                                if ui
                                    .add(
                                        egui::Button::new(icon_text(
                                            ui,
                                            "",
                                            icons::DOWNLOAD_SIMPLE,
                                        ))
                                        .frame(false)
                                        .small(),
                                    )
                                    .on_hover_text("Download PDF to disk")
                                    .clicked()
                                {
                                    download_pdf = Some((d, f.title.clone()));
                                }
                            }
                            if library_row(ui, Some(icons::FILE_PDF), false, &f.title, "PDF")
                                .clicked()
                            {
                                if let Some(d) = f.doc_id {
                                    self.open_document(d);
                                }
                            }
                        });
                    }
                    if let Some((doc_id, title)) = download_pdf {
                        self.start_pdf_download(doc_id, title);
                    }
                    let n_sel = self.sel_pdfs.len();
                    if n_sel > 0 {
                        ui.horizontal(|ui| {
                            ui.add_space(crate::ui::scale::hrem(3));
                            if ui
                                .button(format!("Delete selected ({n_sel})"))
                                .on_hover_text(
                                    "Delete the checked PDF documents from the library \
                                     (the original files on disk are left untouched).",
                                )
                                .clicked()
                            {
                                let paths: Vec<i64> = self.sel_pdfs.iter().copied().collect();
                                delete_selected = Some((Vec::new(), paths));
                            }
                        });
                    }
                }
                ui.add_space(crate::ui::tokens::space::SM);
                ui.separator();

                // ── Unregistered PDFs (서버 CAS 고아 PDF — 문서 행 없음) ──
                ui.add_space(crate::ui::tokens::space::SM);
                let orphans = self.orphan_pdfs.clone();
                ui.horizontal(|ui| {
                    section_header(
                        ui,
                        icons::CLOUD_ARROW_DOWN,
                        "Unregistered PDFs",
                        orphans.len(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add(
                                egui::Button::new(icon_text(ui, "", icons::ARROWS_CLOCKWISE))
                                    .frame(false)
                                    .small(),
                            )
                            .on_hover_text("Refresh the list of server PDFs with no document")
                            .clicked()
                        {
                            self.refresh_orphan_pdfs();
                        }
                    });
                });
                if orphans.is_empty() {
                    empty_state(ui, icons::CLOUD, "No unregistered PDFs on the server.");
                } else {
                    let mut register: Option<freedf_sync::Digest> = None;
                    for o in &orphans {
                        // 가로 오버플로 방지: 한 행을 **오른쪽→왼쪽**으로 배치해
                        // Register(우) → 크기 → 다이제스트(좌, 잘림) 순으로
                        // 놓습니다. (이전엔 library_row가 전체 폭을 차지해
                        // Register 버튼이 화면 밖으로 밀려났습니다.)
                        let meta = format_bytes(o.size);
                        // 다이제스트 접두어(hex 8자리)만 표시 — 전체는 툴팁으로.
                        let ds = o.digest.as_str();
                        let title = if ds.len() > 15 {
                            format!("{}…", &ds[7..15])
                        } else {
                            ds.to_string()
                        };
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.spacing_mut().item_spacing = egui::vec2(crate::ui::tokens::space::SM, 0.0);
                                if ui
                                    .add(egui::Button::new("Register").small())
                                    .on_hover_text("Create a document from this PDF")
                                    .clicked()
                                {
                                    register = Some(o.digest.clone());
                                }
                                ui.label(egui::RichText::new(meta).weak().small());
                                // 다이제스트 — 전체는 툴팁으로, 화면엔 잘려서.
                                ui.add(
                                    egui::Label::new(egui::RichText::new(&title))
                                        .truncate()
                                        .sense(egui::Sense::hover()),
                                )
                                .on_hover_text(o.digest.as_str());
                            },
                        );
                    }
                    if let Some(d) = register {
                        self.register_orphan_pdf(d);
                    }
                }

                ui.add_space(crate::ui::tokens::space::SM);
                ui.separator();

                // ── Recents (계층 2) ──
                let recents: Vec<RecentItem> = self
                    .recents
                    .sorted()
                    .into_iter()
                    .filter(|r| matches(&r.title))
                    .cloned()
                    .collect();
                ui.add_space(crate::ui::tokens::space::SM);
                ui.horizontal(|ui| {
                    section_header(
                        ui,
                        icons::CLOCK_COUNTER_CLOCKWISE,
                        "Recents",
                        self.recents.sorted().len(),
                    );
                });
                if recents.is_empty() {
                    empty_state(ui, icons::CLOCK_COUNTER_CLOCKWISE, "No recent files yet.");
                } else {
                    for item in &recents {
                        let meta = match item.kind {
                            RecentKind::Note => "note".to_string(),
                            RecentKind::File => "pdf".to_string(),
                        };
                        if library_row(ui, Some(icons::CLOCK_COUNTER_CLOCKWISE), false, &item.title, &meta)
                                .clicked()
                            {
                            if let Some(doc_id) = item.doc_id {
                                self.open_document(doc_id);
                            }
                        }
                    }
                }
                ui.add_space(crate::ui::tokens::space::SM);
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

fn section_header(ui: &mut egui::Ui, ic: egui_phosphor_icons::Icon, name: &str, count: usize) {
    // 계층 2: 섹션 제목 — 아이콘 + 이름 + 개수(weak small).
    ui.spacing_mut().item_spacing = egui::vec2(crate::ui::tokens::space::SM, 0.0);
    ui.label(icon_text(ui, name, ic));
    ui.label(egui::RichText::new(count.to_string()).weak().small());
}

/// 섹션의 **빈 상태** — 카드 프레임(margin::CARD · radius::MD) 안에 아이콘과
/// 약한 안내 문구를 중앙 정렬로 보여줍니다. 한 줄 텍스트가 흩어져 보이던
/// 예전 빈 상태를 통일된 컴포넌트로 만듭니다.
pub(crate) fn empty_state(ui: &mut egui::Ui, icon: egui_phosphor_icons::Icon, text: &str) {
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .corner_radius(crate::ui::tokens::radius::MD)
        .inner_margin(crate::ui::tokens::margin::CARD)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| {
                ui.add_space(crate::ui::tokens::space::XS);
                ui.label(
                    egui::RichText::new(icon.0)
                        .small()
                        .font(egui::FontId::new(
                            20.0,
                            egui::FontFamily::Name("phosphor-regular".into()),
                        ))
                        .weak(),
                );
                ui.label(egui::RichText::new(text).weak().small());
                ui.add_space(crate::ui::tokens::space::XS);
            });
        });
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
