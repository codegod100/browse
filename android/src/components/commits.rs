//! Commit history listing.

use eframe::egui;
use vidya::{dim_label, lead_trail, title_2, Theme};

use crate::view_api::{vocab, ViewModel};

pub struct Commits;

impl Commits {
    pub fn show(ui: &mut egui::Ui, th: &Theme, model: &ViewModel, n: usize) {
        title_2(ui, th, vocab(8));
        ui.add_space(th.spacing.md);

        if model.commits.is_empty() {
            dim_label(ui, th, "(no commits)");
            return;
        }

        // Expand to natural height; page scroll owns overflow (no nested list scroll).
        let show = n.min(model.commits.len());
        ui.set_min_width(ui.available_width());
        for c in model.commits.iter().take(show) {
            let summary = c.summary.clone();
            let meta = format!("{} · {}", c.short_id, c.author);
            lead_trail(
                ui,
                |ui| {
                    title_2(ui, th, &summary);
                },
                |ui| {
                    dim_label(ui, th, &meta);
                },
            );
            ui.add_space(th.spacing.sm);
        }
    }
}
