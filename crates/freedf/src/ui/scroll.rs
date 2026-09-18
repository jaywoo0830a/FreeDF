//! ScrollArea wrapper — consistent, optional-only styling for scrollbars.
//!
//! egui exposes rich scrollbar styling; this builder centralizes it into a
//! small, reusable API instead of repeating `ScrollArea` config everywhere.

#![allow(dead_code)] // pre-built kit; wired per call-site as features land.

use eframe::egui;

/// Scroll direction.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ScrollDir {
    Vertical,
    Horizontal,
    Both,
}

pub struct Scroll<'a> {
    id: egui::Id,
    dir: ScrollDir,
    max_height: Option<f32>,
    stick_to_bottom: bool,
    auto_shrink: [bool; 2],
    _marker: std::marker::PhantomData<&'a ()>,
}

impl<'a> Scroll<'a> {
    pub fn new(id_salt: &str) -> Self {
        Self {
            id: egui::Id::new(("ui_scroll", id_salt)),
            dir: ScrollDir::Vertical,
            max_height: None,
            stick_to_bottom: false,
            auto_shrink: [false, false],
            _marker: std::marker::PhantomData,
        }
    }

    pub fn dir(mut self, d: ScrollDir) -> Self {
        self.dir = d;
        self
    }
    pub fn max_height(mut self, h: f32) -> Self {
        self.max_height = Some(h);
        self
    }
    pub fn stick_to_bottom(mut self, on: bool) -> Self {
        self.stick_to_bottom = on;
        self
    }

    pub fn show<R>(self, ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui) -> R) -> R {
        let mut area = match self.dir {
            ScrollDir::Vertical => egui::ScrollArea::vertical().id_salt(self.id),
            ScrollDir::Horizontal => egui::ScrollArea::horizontal().id_salt(self.id),
            ScrollDir::Both => egui::ScrollArea::both().id_salt(self.id),
        };
        area = area.auto_shrink(self.auto_shrink);
        if let Some(h) = self.max_height {
            area = area.max_height(h);
        }
        if self.stick_to_bottom {
            area = area.stick_to_bottom(true);
        }
        area.show(ui, content).inner
    }
}

pub fn scroll(id: &str) -> Scroll<'_> {
    Scroll::new(id)
}
