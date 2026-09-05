//! Macro 설정 — 페이지/탭/가상 데스크탑 단축키 매핑 창 + 키 캡처 UI.
//!
//! 툴바 Row1의 "Macro" 버튼이 창을 열고, 매핑은 `MacroState`(세션 영속)에
//! 저장됩니다. 각 그룹(페이지/탭/데스크탑)은 독립 토글로 켜고 끌 수 있습니다.
//! 키 입력은 **egui 이벤트**로 받고(화상 키보드·물리 키보드 모두 도달),
//! 데스크탑 조합만 enigo(key_hook)로 OS에 주입합니다.
//! 기본 배치: q/w = 데스크탑, a/s = 탭, z/x = 페이지 (왼손 홈 로우).

use super::*;
use crate::settings::MacroKey;

/// 캡처 중인 매핑 슬롯 (어떤 단축키를 다시 지정하는 중인지).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MacroSlot {
    PagePrev,
    PageNext,
    TabPrev,
    TabNext,
    DesktopPrev,
    DesktopNext,
}

/// 이번 프레임에 `mk` 키가 눌렸는지 (에지 — events 기반).
pub(crate) fn macro_key_pressed(ctx: &egui::Context, mk: MacroKey) -> bool {
    ctx.input(|i| {
        i.events.iter().any(|e| match e {
            egui::Event::Key {
                key,
                pressed: true,
                ..
            } => MacroKey::from_egui(*key) == Some(mk),
            _ => false,
        })
    })
}

impl FreeDfApp {
    /// Macro 설정 창 내용.
    pub(crate) fn macro_settings_ui(&mut self, ui: &mut egui::Ui) {
        ui.label(
            egui::RichText::new("Shortcuts & macros")
                .strong()
                .size(16.0),
        );
        ui.label(
            egui::RichText::new(
                "Click a key button, then press the key to assign it (Esc cancels).",
            )
            .weak()
            .small(),
        );
        {
            let (dp, dn) = (self.macro_cfg.desktop_prev, self.macro_cfg.desktop_next);
            let (tp, tn) = (self.macro_cfg.tab_prev, self.macro_cfg.tab_next);
            let (pp, pn) = (self.macro_cfg.page_prev, self.macro_cfg.page_next);
            ui.label(
                egui::RichText::new(format!(
                    "{}/{} → desktop · {}/{} → tab · {}/{} → page",
                    dp.label(),
                    dn.label(),
                    tp.label(),
                    tn.label(),
                    pp.label(),
                    pn.label()
                ))
                .weak()
                .small(),
            );
        }
        ui.separator();

        // ── 페이지 이동 (왼손 키보드 + 오른손 펜) ──
        if ui
            .checkbox(&mut self.macro_cfg.page_enabled, "Enable page keys")
            .changed()
        {
            self.macro_changed();
        }
        ui.add_enabled_ui(self.macro_cfg.page_enabled, |ui| {
            ui.horizontal(|ui| {
                let (pp, pn) = (self.macro_cfg.page_prev, self.macro_cfg.page_next);
                ui.label("Previous page");
                self.key_button(ui, MacroSlot::PagePrev, &pp);
                ui.label("Next page");
                self.key_button(ui, MacroSlot::PageNext, &pn);
            });
        });
        ui.label(
            egui::RichText::new(
                "Works while the pen is in your right hand — these keys work with \
                 both physical and on-screen keyboards.",
            )
            .weak()
            .small(),
        );
        ui.separator();

        // ── 탭 전환 (이 창 안에서) ──
        if ui
            .checkbox(&mut self.macro_cfg.tab_enabled, "Enable tab keys")
            .changed()
        {
            self.macro_changed();
        }
        ui.add_enabled_ui(self.macro_cfg.tab_enabled, |ui| {
            ui.horizontal(|ui| {
                let (tp, tn) = (self.macro_cfg.tab_prev, self.macro_cfg.tab_next);
                ui.label("Previous tab");
                self.key_button(ui, MacroSlot::TabPrev, &tp);
                ui.label("Next tab");
                self.key_button(ui, MacroSlot::TabNext, &tn);
            });
        });
        ui.separator();

        // ── Windows 가상 데스크탑 전환 ──
        ui.label(egui::RichText::new("Virtual desktops (Windows)").strong());
        if ui
            .checkbox(
                &mut self.macro_cfg.desktop_enabled,
                "Send Ctrl+Win+←/→ to switch virtual desktops",
            )
            .changed()
        {
            self.macro_changed();
        }
        ui.add_enabled_ui(self.macro_cfg.desktop_enabled, |ui| {
            ui.horizontal(|ui| {
                let (dp, dn) = (self.macro_cfg.desktop_prev, self.macro_cfg.desktop_next);
                ui.label("Previous desktop");
                self.key_button(ui, MacroSlot::DesktopPrev, &dp);
                ui.label("Next desktop");
                self.key_button(ui, MacroSlot::DesktopNext, &dn);
            });
        });
        ui.label(
            egui::RichText::new(
                "While FreeDF is focused, these keys switch Windows virtual desktops \
                 (enigo injection). Requires 2+ virtual desktops (Win+Tab).",
            )
            .weak()
            .small(),
        );
        ui.separator();

        // ── Hook 디버그 로그 ──
        egui::CollapsingHeader::new(
            egui::RichText::new("Hook debug log")
                .strong()
                .size(13.0),
        )
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let status = if crate::app::key_hook::pipeline_enabled() {
                    "desktop keys: enigo (Windows)"
                } else {
                    "desktop keys: disabled (Windows only)"
                };
                ui.label(egui::RichText::new(status).strong().small());
                if ui.button("Clear").clicked() {
                    crate::app::key_hook::hook_log_clear();
                }
                if ui.button("Copy").clicked() {
                    let text = crate::app::key_hook::hook_log_snapshot().join("\n");
                    ui.ctx().copy_text(text);
                }
            });
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(300));
            let lines = crate::app::key_hook::hook_log_snapshot();
            egui::ScrollArea::vertical()
                .id_salt("hook_log_scroll")
                .max_height(220.0)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    if lines.is_empty() {
                        ui.label(
                            egui::RichText::new(
                                "(no events yet — press a mapped key while FreeDF is focused)",
                            )
                            .weak(),
                        );
                    }
                    for l in &lines {
                        ui.label(egui::RichText::new(l).monospace().size(10.0));
                    }
                });
        });
    }

    /// 캡처를 마무리합니다 — 창을 그린 직후 호출해 다음 키 입력을 슬롯에 기록.
    pub(crate) fn macro_capture_finish(&mut self, ctx: &egui::Context) {
        let Some(slot) = self.macro_capture else {
            return;
        };
        let pressed = ctx.input(|i| {
            i.events.iter().rev().find_map(|e| match e {
                egui::Event::Key {
                    key,
                    pressed: true,
                    ..
                } => Some(*key),
                _ => None,
            })
        });
        let Some(k) = pressed else {
            return;
        };
        if k == egui::Key::Escape {
            self.macro_capture = None;
            return;
        }
        let Some(mk) = MacroKey::from_egui(k) else {
            // 지원하지 않는 키 — 캡처 취소.
            self.macro_capture = None;
            return;
        };
        match slot {
            MacroSlot::PagePrev => self.macro_cfg.page_prev = mk,
            MacroSlot::PageNext => self.macro_cfg.page_next = mk,
            MacroSlot::TabPrev => self.macro_cfg.tab_prev = mk,
            MacroSlot::TabNext => self.macro_cfg.tab_next = mk,
            MacroSlot::DesktopPrev => self.macro_cfg.desktop_prev = mk,
            MacroSlot::DesktopNext => self.macro_cfg.desktop_next = mk,
        }
        self.macro_capture = None;
        self.macro_changed();
    }

    /// 매핑 변경 공통 처리 — 훅 반영 + 세션 저장.
    fn macro_changed(&mut self) {
        self.push_macro_config();
        self.save_default_session();
        self.save_session();
    }

    /// 단일 키 캡처 버튼.
    fn key_button(&mut self, ui: &mut egui::Ui, slot: MacroSlot, current: &MacroKey) {
        let capturing = self.macro_capture == Some(slot);
        let label = if capturing {
            "Press a key…".to_string()
        } else {
            current.label().to_string()
        };
        let btn = ui.add_sized([88.0, 24.0], egui::Button::new(label));
        if btn.clicked() {
            self.macro_capture = if capturing { None } else { Some(slot) };
        }
    }
}
