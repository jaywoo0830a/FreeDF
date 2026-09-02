//! 영단어 사전 오버레이 — 페이지의 단어를 탭하면 사전을 조회해 띄웁니다.
//!
//! - 단어 추출은 core의 `search::word_at`(pdfium 글자 좌표 기반)을 사용합니다.
//! - 조회는 백그라운드 스레드(ureq, dictionaryapi.dev)로 하고, 결과는
//!   DB `word_cache` 테이블에 캐시되어 오프라인 재조회가 가능합니다.
//! - UI는 캔버스 위 플로팅 Area로 표시됩니다.

use super::*;
use std::sync::mpsc::{channel, Receiver, TryRecvError};

const DICT_URL: &str = "https://api.dictionaryapi.dev/api/v2/entries/en/";

/// 사전 오버레이 상태.
#[derive(Default)]
pub(crate) struct Dictionary {
    pub enabled: bool,
    /// 현재 조회 중인 단어.
    pub query: Option<String>,
    pub loading: bool,
    /// (포맷된 결과 또는 오류 메시지).
    pub display: Option<Result<String, String>>,
    /// 백그라운드 조회 결과 수신 채널.
    pub rx: Option<Receiver<Result<String, String>>>,
    /// 탭한 화면 좌표 (오버레이 앵커).
    pub anchor: Pos2,
}

/// 백그라운드 스레드에서 단어를 조회합니다 (UI 블로킹 없음).
pub(crate) fn spawn_lookup(db: Db, word: String) -> Receiver<Result<String, String>> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(lookup(&db, &word));
    });
    rx
}

fn lookup(db: &Db, word: &str) -> Result<String, String> {
    // 1) DB 캐시.
    if let Some(v) = db.get_word_cache(word) {
        return Ok(format_entry(&v, word));
    }
    // 2) 온라인 사전 API.
    let url = format!("{DICT_URL}{}", word);
    let resp = ureq::get(&url)
        .call()
        .map_err(|e| format!("Dictionary unreachable — check your internet connection.\n({e})"))?;
    let value: serde_json::Value = resp
        .into_json()
        .map_err(|e| format!("Bad dictionary response: {e}"))?;
    // 3) 캐시 저장 + 표시.
    db.set_word_cache(word, &value);
    Ok(format_entry(&value, word))
}

/// dictionaryapi.dev 응답을 읽기 좋은 평문으로 변환합니다.
fn format_entry(v: &serde_json::Value, fallback_word: &str) -> String {
    let Some(entries) = v.as_array() else {
        return "No definition found.".to_string();
    };
    let mut out = String::new();
    let mut count = 0usize;
    for e in entries {
        let w = e
            .get("word")
            .and_then(|w| w.as_str())
            .unwrap_or(fallback_word);
        let phonetic = e.get("phonetic").and_then(|p| p.as_str()).unwrap_or("");
        if count == 0 {
            if phonetic.is_empty() {
                out.push_str(&format!("{w}\n\n"));
            } else {
                out.push_str(&format!("{w}  /{phonetic}/\n\n"));
            }
        }
        let empty = Vec::new();
        for m in e
            .get("meanings")
            .and_then(|m| m.as_array())
            .unwrap_or(&empty)
        {
            let pos = m.get("partOfSpeech").and_then(|p| p.as_str()).unwrap_or("");
            for d in m
                .get("definitions")
                .and_then(|d| d.as_array())
                .unwrap_or(&empty)
            {
                if count >= 5 {
                    return out;
                }
                let def = d
                    .get("definition")
                    .and_then(|d| d.as_str())
                    .unwrap_or("");
                if pos.is_empty() {
                    out.push_str(&format!("• {def}\n"));
                } else {
                    out.push_str(&format!("• [{pos}] {def}\n"));
                }
                count += 1;
            }
        }
    }
    if count == 0 {
        "No definition found.".to_string()
    } else {
        out
    }
}

impl FreeDfApp {
    /// 페이지 좌표의 단어를 조회해 오버레이를 엽니다.
    pub(crate) fn lookup_word_at(&mut self, page_pt: [f32; 2], anchor: Pos2) {
        let Some(doc) = &self.document else {
            return;
        };
        let chars = match doc.page_chars(self.current_page) {
            Ok(c) => c,
            Err(e) => {
                self.status = Some(format!("Could not read page text: {e}"));
                return;
            }
        };
        match freedf_core::search::word_at(&chars, page_pt) {
            Some((word, _)) if !word.trim().is_empty() => {
                self.dictionary.query = Some(word.clone());
                self.dictionary.loading = true;
                self.dictionary.display = None;
                self.dictionary.anchor = anchor;
                self.dictionary.rx = Some(spawn_lookup(self.db.clone(), word));
            }
            Some(_) => {
                self.status = Some("No word under the pointer.".to_string());
            }
            None => {
                self.status = Some("No word under the pointer.".to_string());
            }
        }
    }

    /// 사전 오버레이 UI (캔버스 위 플로팅 창).
    pub(crate) fn dict_overlay(&mut self, ctx: &egui::Context) {
        // 백그라운드 조회 결과 수신.
        if let Some(rx) = &self.dictionary.rx {
            match rx.try_recv() {
                Ok(res) => {
                    self.dictionary.display = Some(res);
                    self.dictionary.rx = None;
                    self.dictionary.loading = false;
                }
                Err(TryRecvError::Empty) => {
                    ctx.request_repaint();
                }
                Err(TryRecvError::Disconnected) => {
                    self.dictionary.rx = None;
                    self.dictionary.loading = false;
                }
            }
        }
        if self.dictionary.query.is_none() && self.dictionary.display.is_none() {
            return;
        }
        let fill = crate::theme::nord::semantic::overlay_bg();
        let stroke = crate::theme::nord::semantic::OVERLAY_BORDER;
        let mut close = false;
        egui::Area::new(egui::Id::new("dict_overlay"))
            .order(egui::Order::Foreground)
            .fixed_pos(self.dictionary.anchor + Vec2::new(16.0, -12.0))
            .show(ctx, |ui| {
                egui::Frame::window(&ui.style())
                    .fill(fill)
                    .stroke(Stroke::new(1.0, stroke))
                    .corner_radius(8)
                    .inner_margin(egui::Margin::same(10))
                    .show(ui, |ui| {
                        ui.set_max_width(340.0);
                        ui.horizontal(|ui| {
                            if let Some(q) = &self.dictionary.query {
                                ui.label(egui::RichText::new(q).strong().size(17.0));
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add(
                                            egui::Button::new(icon_text(ui, "", icons::X))
                                                .frame(false),
                                        )
                                        .on_hover_text("Close dictionary")
                                        .clicked()
                                    {
                                        close = true;
                                    }
                                },
                            );
                        });
                        ui.separator();
                        match (&self.dictionary.loading, &self.dictionary.display) {
                            (true, _) => {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label("Looking up…");
                                });
                            }
                            (_, Some(Ok(text))) => {
                                ui.label(text);
                            }
                            (_, Some(Err(e))) => {
                                ui.colored_label(ui.visuals().error_fg_color, e);
                            }
                            _ => {}
                        }
                    });
            });
        if close {
            self.dictionary.query = None;
            self.dictionary.display = None;
            self.dictionary.loading = false;
        }
    }
}
