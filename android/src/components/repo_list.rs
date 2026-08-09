//! Startup page: local Radicle storage inventory under the RID field.

use eframe::egui::{self, CursorIcon, RichText, Sense, Vec2};
use vidya::{body, dim_label, title_2, Theme};

use crate::rad::RepoSummary;

pub struct RepoList;

impl RepoList {
    /// Filter by name/RID substring (`query`), paint clickable rows.
    /// Returns a RID to open when a row is clicked.
    pub fn show(
        ui: &mut egui::Ui,
        th: &Theme,
        repos: &[RepoSummary],
        query: &str,
    ) -> Option<String> {
        title_2(ui, th, "Local repos");
        ui.add_space(th.spacing.sm);

        let q = query.trim().to_lowercase();
        let filtered: Vec<&RepoSummary> = repos
            .iter()
            .filter(|r| {
                if q.is_empty() {
                    return true;
                }
                r.name.to_lowercase().contains(&q) || r.rid.to_lowercase().contains(&q)
            })
            .collect();

        if repos.is_empty() {
            dim_label(ui, th, "No repositories in local storage yet.");
            return None;
        }
        if filtered.is_empty() {
            dim_label(ui, th, "No repos match this filter.");
            return None;
        }

        let mut open: Option<String> = None;
        // Fill remaining viewport height (page scroll + card chrome).
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
