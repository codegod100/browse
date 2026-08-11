//! Repo identity header: name, description, RID, head.

use eframe::egui;
use vidya::{dim_label, title, Theme};

use crate::linkify;
use crate::view_api::ViewModel;

pub struct Meta;

impl Meta {
    pub fn show(ui: &mut egui::Ui, th: &Theme, model: &ViewModel) {
        let name = model.string(0);
        let desc = model.string(1);
        let rid = model.string(2);
        let head = model.string(3);

        title(ui, th, name);
        ui.add_space(th.spacing.sm);
        if !desc.is_empty() {
            linkify::body(ui, th, desc);
            ui.add_space(th.spacing.sm);
        }
        dim_label(ui, th, rid);
        ui.add_space(th.spacing.xs);
        dim_label(ui, th, head);
        // Extra bottom padding so the last line is not flush against the card edge.
        ui.add_space(th.spacing.md);
    }
}
