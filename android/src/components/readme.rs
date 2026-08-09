//! README section with markdown rendering.

use eframe::egui;
use vidya::{dim_label, title_2, Theme};

use crate::markdown;
use crate::view_api::{vocab, ViewModel};

pub struct Readme;

impl Readme {
    pub fn show(ui: &mut egui::Ui, th: &Theme, model: &ViewModel, slot: usize) {
        title_2(ui, th, vocab(6));
        ui.add_space(th.spacing.md);
        let md = model.string(slot);
        if md.is_empty() || md == "(no README)" {
            dim_label(ui, th, "(no README)");
            return;
        }
        egui::ScrollArea::vertical()
            .id_salt(("readme", slot))
            .max_height(ui.available_height().max(120.0))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                markdown::render(ui, th, md);
            });
    }
}
