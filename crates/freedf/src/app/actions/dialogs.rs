//! dialogs — 액션 실행 로직 (자세한 내용은 mod.rs 참조).

use super::*;

impl FreeDfApp {
    pub(crate) fn run_text_action(&mut self, action: TextAction, text: String, pages: usize) {
        match action {
            TextAction::NewNote => self.create_note_action(text.trim(), pages),
            TextAction::RenameNote => {
                if let Some(id) = self.current_note {
                    self.rename_note_action(id as u64, text.trim());
                }
            }
            TextAction::OpenPdf => self.open_pdf(&PathBuf::from(text.trim())),
            TextAction::UploadMedia => self.upload_media_path(&PathBuf::from(text.trim())),
            TextAction::DownloadPdf { doc_id, title } => {
                self.download_pdf_action(doc_id, title, PathBuf::from(text.trim()));
            }
            TextAction::DownloadMedia { url, name } => {
                self.download_media_action(url, name, PathBuf::from(text.trim()));
            }
        }
    }

    pub(crate) fn run_confirm_action(&mut self, action: ConfirmAction, text: String) {
        match action {
            ConfirmAction::DeleteNote => {
                if let Ok(id) = text.trim().parse::<i64>() {
                    self.delete_note_action(id as u64);
                }
            }
            ConfirmAction::DeleteLibrary { notes, pdfs } => {
                let n_notes = notes.len();
                let n_pdfs = pdfs.len();
                for id in &notes {
                    self.delete_note_action(*id as u64);
                }
                for p in pdfs {
                    self.delete_pdf_action(p);
                }
                self.sel_notes.clear();
                self.sel_pdfs.clear();
                self.status = Some(format!(
                    "Deleted {n_notes} note(s) and {n_pdfs} PDF(s)"
                ));
            }
        }
    }
}
