//! Root file tree listing.

use eframe::egui;
use vidya::{body, dim_label, lead_trail, title_2, Theme};

use crate::view_api::{vocab, ViewModel};

pub struct Files;

impl Files {
    pub fn show(ui: &mut egui::Ui, th: &Theme, model: &ViewModel, n: usize) {
        title_2(ui, th, vocab(7));
        ui.add_space(th.spacing.md);

        if model.files.is_empty() {
            dim_label(ui, th, "(empty tree)");
            return;
        }

        let show = n.min(model.files.len());
        egui::ScrollArea::vertical()
            .id_salt("files_list")
            .max_height(360.0)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                for file in model.files.iter().take(show) {
                    let name = file.name.clone();
                    let kind = if file.is_tree { "dir" } else { "file" };
                    lead_trail(
                        ui,
                        |ui| {
                            body(ui, th, &name);
                        },
                        |ui| {
                            dim_label(ui, th, kind);
                        },
                    );
                    ui.add_space(th.spacing.xs);
                }
            });
    }
}
