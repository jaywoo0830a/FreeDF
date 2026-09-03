//! 영단어 사전 오버레이 — 페이지의 단어를 탭하면 사전을 조회해 띄웁니다.
//!
//! - 단어 추출은 core의 `text::word_at`(pdfium 글자 좌표 + 줄 클러스터링)를
//!   사용합니다.
//! - 조회는 백그라운드 스레드로 하고, 결과는 DB `word_cache`에 캐시되어
//!   오프라인 재조회가 가능합니다.
//! - **OCP(개방-폐쇄) 프로바이더 구조**: `DictionaryProvider` 트레이트를 구현한
//!   프로바이더를 `DictionaryService`에 등록하면 순서대로 시도합니다.
//!   새 API를 추가하려면 앱의 프로바이더 구현 + core의 `parse_*` 함수만
//!   추가하면 됩니다 (UI/캐시/조회 코드는 변경 없음).
//! - UI는 캔버스 위 플로팅 Area로 표시됩니다.

use super::*;
use freedf_core::dictionary::DictionaryEntry;
use freedf_core::text::word_at;
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::time::Duration;

/// 사전 조회 API 하나를 나타냅니다. 응답은 **공통 형식**(
/// [`DictionaryEntry`])으로 정규화해 반환합니다.
pub(crate) trait DictionaryProvider: Send + Sync {
    /// 사용자에게 보여줄 이름 (오류 메시지 등).
    fn name(&self) -> &'static str;
    /// 단어 조회. 네트워크/파싱 실패는 `Err(사유)`.
    fn lookup(&self, agent: &ureq::Agent, word: &str) -> Result<DictionaryEntry, String>;
}

/// Wiktionary REST API (키 불필요, HTML 정의 포함).
struct WiktionaryProvider;

impl DictionaryProvider for WiktionaryProvider {
    fn name(&self) -> &'static str {
        "Wiktionary"
    }

    fn lookup(&self, agent: &ureq::Agent, word: &str) -> Result<DictionaryEntry, String> {
        let url = format!("https://en.wiktionary.org/api/rest_v1/page/definition/{word}");
        let resp = agent.get(&url).call().map_err(|e| e.to_string())?;
        let value: serde_json::Value = resp
            .into_json()
            .map_err(|e| format!("bad response: {e}"))?;
        let mut entry = freedf_core::dictionary::parse_wiktionary(&value);
        if entry.word.is_empty() {
            entry.word = word.to_string();
        }
        if entry.definitions.is_empty() {
            return Err("no definitions".to_string());
        }
        Ok(entry)
    }
}

/// 등록된 프로바이더들을 순서대로 시도하는 사전 조회 서비스.
#[derive(Clone)]
pub(crate) struct DictionaryService {
    agent: ureq::Agent,
    providers: std::sync::Arc<Vec<Box<dyn DictionaryProvider>>>,
}

impl DictionaryService {
    /// 기본 구성: 타임아웃 10초 + 프로바이더 3개.
    /// 새 프로바이더는 여기 `providers`에 추가하면 됩니다.
    pub fn new() -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(10))
            .user_agent("FreeDF/3.0")
            .build();
        Self {
            agent,
            providers: std::sync::Arc::new(vec![Box::new(WiktionaryProvider)]),
        }
    }

    /// 프로바이더를 순서대로 시도합니다 (모두 실패 시 이유를 모아서 보고).
    fn query(&self, word: &str) -> Result<DictionaryEntry, String> {
        let mut failures = Vec::new();
        for p in self.providers.iter() {
            match p.lookup(&self.agent, word) {
                Ok(entry) => return Ok(entry),
                Err(e) => failures.push(format!("{}: {e}", p.name())),
            }
        }
        Err(format!(
            "Dictionary lookup failed — check your internet connection.\n{}",
            failures.join("\n")
        ))
    }
}

impl Default for DictionaryService {
    fn default() -> Self {
        Self::new()
    }
}

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
    /// 프로바이더 서비스 (OCP: 교체/확장 가능).
    pub service: DictionaryService,
}

/// 백그라운드 스레드에서 단어를 조회합니다 (UI 블로킹 없음).
pub(crate) fn spawn_lookup(
    service: DictionaryService,
    db: std::sync::Arc<dyn StorageBackend>,
    word: String,
) -> Receiver<Result<String, String>> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(lookup(&service, db.as_ref(), &word));
    });
    rx
}

fn lookup(service: &DictionaryService, db: &dyn StorageBackend, word: &str) -> Result<String, String> {
    // 1) DB 캐시 (공통 형식 JSONB; 이전 형식이면 무시하고 재조회).
    if let Some(v) = db.get_word_cache(word) {
        if let Some(e) = DictionaryEntry::from_value(&v) {
            return Ok(e.format(word));
        }
    }
    // 2) 프로바이더 순차 조회 (OCP 목록).
    let entry = service.query(word)?;
    // 3) 캐시 저장 + 표시.
    db.set_word_cache(word, &entry.to_value());
    Ok(entry.format(word))
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
        match word_at(&chars, page_pt, 4.0) {
            Some((word, _)) if !word.trim().is_empty() => {
                self.dictionary.query = Some(word.clone());
                self.dictionary.loading = true;
                self.dictionary.display = None;
                self.dictionary.anchor = anchor;
                self.dictionary.rx = Some(spawn_lookup(
                    self.dictionary.service.clone(),
                    self.db.clone(),
                    word,
                ));
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
