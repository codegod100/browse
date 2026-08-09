//! Startup page: recently viewed + local Radicle storage inventory under the RID field.

use eframe::egui::{
    self, Align, CursorIcon, Layout, Margin, RichText, Sense, Stroke, StrokeKind, UiBuilder,
    Vec2,
};
use vidya::{body, dim_label, title_2, Theme};

use crate::rad::RepoSummary;
use crate::recent::{self, RecentRepo};

pub struct RepoList;

impl RepoList {
    /// Paint recently viewed (when any) then local inventory.
    /// Filter by name/RID/description substring (`query`). Returns a RID to open when a row is clicked.
    pub fn show(
        ui: &mut egui::Ui,
        th: &Theme,
        recent: &[RecentRepo],
        repos: &[RepoSummary],
        query: &mut String,
    ) -> Option<String> {
        let mut open: Option<String> = None;

        search_field(ui, th, query);
        ui.add_space(th.spacing.sm);

        let recent_filtered = recent::filter(recent, query);
        if !recent.is_empty() {
            title_2(ui, th, "Recently viewed");
            ui.add_space(th.spacing.sm);
            if recent_filtered.is_empty() {
                dim_label(ui, th, "No recent repos match this search.");
            } else {
                for repo in &recent_filtered {
                    let summary = RepoSummary::from(*repo);
                    let response = repo_row(ui, th, &summary);
                    if response.clicked() {
                        open = Some(repo.rid.clone());
                    }
                }
            }
            ui.add_space(th.spacing.md);
        }

        title_2(ui, th, "Local repos");
        ui.add_space(th.spacing.sm);

        let q = query.trim().to_lowercase();
        let filtered: Vec<&RepoSummary> = repos
            .iter()
            .filter(|r| {
                if q.is_empty() {
                    return true;
                }
                r.name.to_lowercase().contains(&q)
                    || r.rid.to_lowercase().contains(&q)
                    || r.description.to_lowercase().contains(&q)
            })
            .collect();

        if repos.is_empty() {
            dim_label(ui, th, "No repositories in local storage yet.");
            return open;
        }
        if !q.is_empty() {
            dim_label(
                ui,
                th,
                &format!("{} of {} repos", filtered.len(), repos.len()),
            );
            ui.add_space(th.spacing.xs);
        }
        if filtered.is_empty() {
            dim_label(ui, th, "No repos match this search.");
            return open;
        }

        // Fill remaining viewport height under the RID row / card chrome.
        let h = (ui.clip_rect().bottom() - ui.cursor().top() - 12.0).max(200.0);
        egui::ScrollArea::vertical()
            .id_salt("local_repos")
            .max_height(h)
            .min_scrolled_height(h)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                for repo in filtered {
                    let response = repo_row(ui, th, repo);
                    if response.clicked() {
                        open = Some(repo.rid.clone());
                    }
                }
            });
        open
    }
}

fn repo_row(ui: &mut egui::Ui, th: &Theme, repo: &RepoSummary) -> egui::Response {
    let name = RichText::new(&repo.name)
        .size(th.type_scale.body)
        .color(th.palette.text);
    let mut response = ui
        .add(egui::Label::new(name).sense(Sense::click()).wrap())
        .on_hover_cursor(CursorIcon::PointingHand);

    let rid = RichText::new(&repo.rid)
        .size(th.type_scale.caption)
        .color(th.palette.text_secondary);
    let rid_r = ui
        .add(egui::Label::new(rid).sense(Sense::click()).wrap())
        .on_hover_cursor(CursorIcon::PointingHand);
    response |= rid_r;

    if !repo.description.is_empty() {
        body(ui, th, &repo.description);
    }

    if response.hovered() || response.clicked() {
        let rect = response.rect.expand2(Vec2::new(4.0, 2.0));
        let a = th.palette.accent;
        ui.painter().rect_filled(
            rect,
            th.spacing.radius_sm,
            egui::Color32::from_rgba_unmultiplied(a.r(), a.g(), a.b(), 28),
        );
    }

    ui.add_space(th.spacing.md);
    response
}

fn search_field(ui: &mut egui::Ui, th: &Theme, text: &mut String) {
    let h = th.spacing.control_height;
    let w = ui.available_width().max(1.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());

    ui.painter().rect(
        rect,
        th.spacing.radius_md,
        th.palette.view_bg,
        Stroke::new(1.0_f32, th.palette.border_soft),
        StrokeKind::Inside,
    );

    let pad_x = th.spacing.field_pad_x;
    let edit_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + pad_x, rect.top()),
        egui::pos2(rect.right() - pad_x, rect.bottom()),
    );
    ui.allocate_new_ui(
        UiBuilder::new()
            .max_rect(edit_rect)
            .layout(Layout::left_to_right(Align::Center)),
        |ui| {
            ui.add(
                egui::TextEdit::singleline(text)
                    .frame(false)
                    .desired_width(edit_rect.width())
                    .margin(Margin::ZERO)
                    .hint_text("Search by name, RID, or description…"),
            );
        },
    );
}
