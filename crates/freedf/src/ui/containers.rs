//! Generic containers — spacing, grid and flex helpers.
//!
//! - [`Container`]  : a framed box with margin / padding / fill / border / radius
//! - [`Grid`]       : configurable grid (`columns`, `gap`, `striped`, widths)
//! - [`row`]/[`centered`] : flex-ish horizontal alignment helpers
//!
//! All are props-only (they never touch app state) so they compose freely.

#![allow(dead_code)] // pre-built kit; wired per call-site as features land.

use eframe::egui;

/// A framed box (margin + padding + fill + border + rounding).
pub struct Container {
    inner_margin: egui::Margin,
    outer_margin: egui::Margin,
    fill: Option<egui::Color32>,
    stroke: Option<egui::Stroke>,
    corner: f32,
}

impl Container {
    pub fn new() -> Self {
        Self {
            inner_margin: egui::Margin::symmetric(8, 8),
            outer_margin: egui::Margin::ZERO,
            fill: None,
            stroke: None,
            corner: 6.0,
        }
    }

    pub fn pad(mut self, m: egui::Margin) -> Self {
        self.inner_margin = m;
        self
    }
    pub fn pad_all(mut self, v: i8) -> Self {
        self.inner_margin = egui::Margin::same(v);
        self
    }
    pub fn pad_symmetric(mut self, x: i8, y: i8) -> Self {
        self.inner_margin = egui::Margin::symmetric(x, y);
        self
    }
    pub fn margin(mut self, m: egui::Margin) -> Self {
        self.outer_margin = m;
        self
    }
    pub fn fill(mut self, c: egui::Color32) -> Self {
        self.fill = Some(c);
        self
    }
    pub fn bordered(mut self, color: egui::Color32, width: f32) -> Self {
        self.stroke = Some(egui::Stroke::new(width, color));
        self
    }
    pub fn rounded(mut self, r: f32) -> Self {
        self.corner = r;
        self
    }

    pub fn show<R>(self, ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui) -> R) -> R {
        let f = egui::Frame::new()
            .inner_margin(self.inner_margin)
            .outer_margin(self.outer_margin)
            .corner_radius(self.corner);
        let f = if let Some(fillv) = self.fill {
            f.fill(fillv)
        } else {
            f
        };
        let f = if let Some(stroke) = self.stroke {
            f.stroke(stroke)
        } else {
            f
        };
        f.show(ui, content).inner
    }
}

impl Default for Container {
    fn default() -> Self {
        Self::new()
    }
}

pub fn container() -> Container {
    Container::new()
}

/// A configurable grid (`egui::Grid` wrapper).
pub struct Grid {
    columns: usize,
    spacing: egui::Vec2,
    striped: bool,
    min_col_width: f32,
}

impl Grid {
    pub fn new(columns: usize) -> Self {
        Self {
            columns,
            spacing: egui::vec2(8.0, 8.0),
            striped: false,
            min_col_width: 0.0,
        }
    }
    pub fn gap(mut self, x: f32, y: f32) -> Self {
        self.spacing = egui::vec2(x, y);
        self
    }
    pub fn striped(mut self, on: bool) -> Self {
        self.striped = on;
        self
    }
    pub fn min_col_width(mut self, w: f32) -> Self {
        self.min_col_width = w;
        self
    }

    pub fn show<R>(
        self,
        ui: &mut egui::Ui,
        id_salt: impl std::hash::Hash + std::fmt::Debug,
        content: impl FnOnce(&mut egui::Ui) -> R,
    ) -> egui::InnerResponse<R> {
        let mut g = egui::Grid::new(egui::Id::new(id_salt))
            .num_columns(self.columns)
            .spacing(self.spacing)
            .striped(self.striped);
        if self.min_col_width > 0.0 {
            g = g.min_col_width(self.min_col_width);
        }
        g.show(ui, content)
    }
}

pub fn grid(columns: usize) -> Grid {
    Grid::new(columns)
}

/// Horizontal row with an explicit vertical `Align`.
pub fn row<R>(
    ui: &mut egui::Ui,
    align: egui::Align,
    gap: f32,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let prev = ui.spacing_mut().item_spacing.x;
    ui.spacing_mut().item_spacing.x = gap;
    let r = ui.with_layout(egui::Layout::left_to_right(align), content).inner;
    ui.spacing_mut().item_spacing.x = prev;
    r
}

/// Horizontally centered content.
pub fn centered<R>(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.with_layout(
        egui::Layout::centered_and_justified(egui::Direction::TopDown),
        content,
    )
    .inner
}