//! Desktop window: RID field + Gleam-driven layout, Radicle fetch on Open.

use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align, Frame, Layout, Margin, RichText, Sense, Shadow, Stroke, StrokeKind, UiBuilder,
    Vec2,
};
use radicle::Profile;
use vidya::{
    apply_dark, body, dim_label, grid_cols_with, paint_icon_in, primary_button, ColSpec, GridOpts,
    Icon, Theme,
};

use crate::components::{RepoList, RepoUi, Tab};
use crate::gleam_bridge::{self, PaintResult, Slots, MSG_BACK, MSG_FAILED, MSG_LOADED, MSG_OPEN};
use crate::gleam_guest;
use crate::rad::{self, RepoSummary};
use crate::recent::{self, RecentRepo};
use crate::tab_prefs::{self, TabPrefsStore};

const TOAST_SECS: u64 = 2;

pub fn run_desktop(initial_rid: Option<String>) -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([960.0, 720.0])
            .with_min_inner_size([420.0, 480.0])
            .with_title("Browse"),
        ..Default::default()
    };
    eframe::run_native(
        "Browse",
        options,
        Box::new(move |cc| Ok(Box::new(BrowseApp::new(cc, initial_rid)))),
    )
}

/// Android NativeActivity entry.
#[cfg(target_os = "android")]
pub fn run_android(android_app: winit::platform::android::activity::AndroidApp) -> eframe::Result {
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("Browse"),
        ..Default::default()
    };
    options.android_app = Some(android_app);
    eframe::run_native(
        "Browse",
        options,
        Box::new(|cc| Ok(Box::new(BrowseApp::new(cc, None)))),
    )
}

struct BrowseApp {
    theme: Theme,
    profile: Option<Profile>,
    rid_input: String,
    repo_filter: String,
    model: Option<i64>,
    slots: Slots,
    repo_ui: RepoUi,
    local_repos: Vec<RepoSummary>,
    recent_repos: Vec<RecentRepo>,
    tab_prefs: TabPrefsStore,
    err: Option<String>,
    auto_open: bool,
    /// Toast message, show time, and optional anchor (screen pos of the source chip).
    toast: Option<(String, Instant, Option<egui::Pos2>)>,
}

impl BrowseApp {
    fn new(cc: &eframe::CreationContext<'_>, initial_rid: Option<String>) -> Self {
        apply_dark(&cc.egui_ctx);
        let theme = Theme::dark();

        let (profile, profile_err) = match rad::load_profile() {
            Ok(p) => (Some(p), None),
            Err(e) => (None, Some(e.to_string())),
        };

        let local_repos = profile
            .as_ref()
            .and_then(|p| rad::list_local_repos(p).ok())
            .unwrap_or_default();

        let (model, model_err) = match gleam_guest::init() {
            Ok(n) => (Some(n), None),
            Err(e) => (None, Some(e)),
        };

        let auto_open = initial_rid.is_some();
        let err = profile_err.or(model_err);
        let recent_repos = recent::load();
        let tab_prefs = tab_prefs::load();

        Self {
            theme,
            profile,
            rid_input: initial_rid.unwrap_or_default(),
            repo_filter: String::new(),
            model,
            slots: Slots::default(),
            repo_ui: RepoUi::default(),
            local_repos,
            recent_repos,
            tab_prefs,
            err,
            auto_open,
            toast: None,
        }
    }

    #[allow(dead_code)] // bottom-center toasts for non-chip messages
    fn show_toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now(), None));
    }

    fn show_toast_at(&mut self, msg: impl Into<String>, at: egui::Pos2) {
        self.toast = Some((msg.into(), Instant::now(), Some(at)));
    }

    fn refresh_local_repos(&mut self) {
        if let Some(profile) = &self.profile {
            match rad::list_local_repos(profile) {
                Ok(repos) => self.local_repos = repos,
                Err(e) => self.err = Some(e.to_string()),
            }
        }
    }

    fn open_current(&mut self) {
        let Some(profile) = &self.profile else {
            self.err = Some("No Radicle profile loaded.".into());
            if let Some(m) = self.model {
                if let Ok(n) = gleam_guest::update(m, MSG_FAILED) {
                    self.model = Some(n);
                    self.slots = Slots::from_error("No Radicle profile loaded.");
                }
            }
            return;
        };

        match rad::open_repo(profile, &self.rid_input) {
            Ok(view) => {
                recent::record(
                    &mut self.recent_repos,
                    &view.rid,
                    &view.name,
                    &view.description,
                );
                recent::save(&self.recent_repos);
                let already_viewing = self.model == Some(1) && self.repo_ui.rid == view.rid;
                if already_viewing {
                    self.apply_view_reload(&view);
                    self.show_toast("Reloaded");
                } else {
                    self.repo_ui.reset_for(&view.rid, &view.head_oid, &view.files);
                    restore_tab_prefs(&mut self.repo_ui, &self.tab_prefs, profile, &view.files);
                    self.slots = Slots::from_view(&view);
                    self.err = None;
                    if let Some(m) = self.model {
                        match gleam_guest::update(m, MSG_LOADED) {
                            Ok(n) => self.model = Some(n),
                            Err(e) => self.err = Some(e),
                        }
                    }
                }
            }
            Err(e) => {
                let msg = e.to_string();
                self.slots = Slots::from_error(&msg);
                self.err = Some(msg.clone());
                if let Some(m) = self.model {
                    match gleam_guest::update(m, MSG_FAILED) {
                        Ok(n) => self.model = Some(n),
                        Err(e) => self.err = Some(e),
                    }
                }
            }
        }
    }

    /// Refresh snapshot in place (keep tab + selection when still valid).
    fn apply_view_reload(&mut self, view: &rad::RepoView) {
        let keep = (
            self.repo_ui.tab,
            self.repo_ui.selected_patch.clone(),
            self.repo_ui.selected_issue.clone(),
            self.repo_ui.selected_job.clone(),
            self.repo_ui.cwd.clone(),
            self.repo_ui.selected_file.clone(),
            self.repo_ui.selected_commit.clone(),
            self.repo_ui.patch_status,
            self.repo_ui.issue_status,
            self.repo_ui.job_status,
            self.repo_ui.patch_filter.clone(),
            self.repo_ui.issue_filter.clone(),
        );

        self.repo_ui.head_oid = view.head_oid.clone();
        self.repo_ui.tab = keep.0;
        self.repo_ui.selected_patch = keep.1;
        self.repo_ui.selected_issue = keep.2;
        self.repo_ui.selected_job = keep.3;
        self.repo_ui.cwd = keep.4;
        self.repo_ui.selected_file = keep.5;
        self.repo_ui.selected_commit = keep.6;
        self.repo_ui.patch_status = keep.7;
        self.repo_ui.issue_status = keep.8;
        self.repo_ui.job_status = keep.9;
        self.repo_ui.patch_filter = keep.10;
        self.repo_ui.issue_filter = keep.11;
        self.repo_ui.prune_cob_selections(&view.patches, &view.issues, &view.jobs);
        self.repo_ui.reload_requested = false;
        self.slots = Slots::from_view(view);
        self.err = None;
    }

    fn reload_current(&mut self) {
        let Some(profile) = &self.profile else {
            return;
        };
        let rid = if self.rid_input.trim().is_empty() {
            self.repo_ui.rid.clone()
        } else {
            self.rid_input.clone()
        };
        if rid.trim().is_empty() {
            return;
        }
        match rad::open_repo(profile, &rid) {
            Ok(view) => {
                self.apply_view_reload(&view);
                self.show_toast("Reloaded");
            }
            Err(e) => {
                self.err = Some(e.to_string());
            }
        }
    }

    /// Restore last Files/Commits/Patches/Issues/Jobs tab (+ status filters) for this RID.
    fn persist_tab_prefs_if_dirty(&mut self) {
        if !self.repo_ui.prefs_dirty || self.repo_ui.rid.is_empty() {
            return;
        }
        tab_prefs::record(&mut self.tab_prefs, tab_prefs::from_ui(&self.repo_ui));
        tab_prefs::save(&self.tab_prefs);
        self.repo_ui.prefs_dirty = false;
    }

    fn handle_msg(&mut self, msg: i64) {
        if msg == MSG_OPEN {
            self.open_current();
            return;
        }
        if let Some(m) = self.model {
            match gleam_guest::update(m, msg) {
                Ok(n) => {
                    // Back to enter — refresh inventory and clear RID.
                    if msg == MSG_BACK || n == 0 {
                        if !self.repo_ui.rid.is_empty() {
                            tab_prefs::record(
                                &mut self.tab_prefs,
                                tab_prefs::from_ui(&self.repo_ui),
                            );
                            tab_prefs::save(&self.tab_prefs);
                            self.repo_ui.prefs_dirty = false;
                        }
                        self.refresh_local_repos();
                        self.rid_input.clear();
                        self.repo_filter.clear();
                    }
                    self.model = Some(n);
                    self.err = None;
                }
                Err(e) => self.err = Some(e),
            }
        }
    }
}

impl eframe::App for BrowseApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        apply_dark(ctx);
        let th = self.theme.clone();

        if let Some((_, at, _)) = &self.toast {
            if at.elapsed() > Duration::from_secs(TOAST_SECS) {
                self.toast = None;
            } else {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
        }

        if self.auto_open {
            self.auto_open = false;
            if !self.rid_input.trim().is_empty() {
                self.open_current();
            }
        }

        // Fixed central shell (no page ScrollArea): fill-height lists/panes own
        // scrolling so we do not nest solid gutters (double scrollbar).
        egui::CentralPanel::default()
            .frame(th.page_frame())
            .show(ctx, |ui| {
                grid_cols_with(
                    ui,
                    &th,
                    "browse",
                    &[ColSpec::Flex],
                    GridOpts::page(&th),
                    |g| {
                        g.section(|ui| {
                            if self.profile.is_none() {
                                dim_label(ui, &th, "Could not load ~/.radicle profile.");
                                if let Some(err) = &self.err {
                                    body(ui, &th, err);
                                }
                                return;
                            }

                            let mut enter_open = false;
                            let mut btn_open = false;
                            let mut copy_rid_at = None;

                            let h = th.spacing.control_height;
                            let row_w = ui.available_width().max(1.0);
                            ui.allocate_ui_with_layout(
                                Vec2::new(row_w, h),
                                Layout::right_to_left(Align::Center),
                                |ui| {
                                    ui.set_min_height(h);
                                    ui.set_max_height(h);
                            if primary_button(ui, &th, "Open")
                                .on_hover_text("Open repo; press again to reload")
                                .clicked()
                            {
                                btn_open = true;
                            }
                                    ui.add_space(th.spacing.sm);

                                    let rest = ui.available_width().max(1.0);
                                    ui.allocate_ui_with_layout(
                                        Vec2::new(rest, h),
                                        Layout::left_to_right(Align::Center),
                                        |ui| {
                                            ui.set_min_height(h);
                                            ui.set_max_height(h);
                                            ui.spacing_mut().item_spacing.x = th.spacing.sm;
                                            ui.label("RID");
                                            let field =
                                                rid_input_field(ui, &th, &mut self.rid_input, h);
                                            if let Some(at) = field.copy_clicked_at {
                                                copy_rid_at = Some(at);
                                            }
                                            if field.enter {
                                                enter_open = true;
                                            }
                                        },
                                    );
                                },
                            );

                            if let Some(at) = copy_rid_at {
                                let rid = self.rid_input.trim();
                                if !rid.is_empty() {
                                    ui.ctx().copy_text(rid.to_string());
                                    self.show_toast_at("RID copied", at);
                                }
                            }
                            if enter_open || btn_open {
                                self.open_current();
                                return;
                            }
                            ui.add_space(th.spacing.sm);

                            let Some(model) = self.model else {
                                if let Some(err) = &self.err {
                                    dim_label(ui, &th, err);
                                }
                                return;
                            };

                            // Startup: recently viewed + local inventory (full-bleed hover rows).
                            if model == 0 {
                                let clicked = RepoList::show(
                                    ui,
                                    &th,
                                    &self.recent_repos,
                                    &self.local_repos,
                                    &mut self.repo_filter,
                                );
                                if let Some(rid) = clicked {
                                    self.rid_input = rid;
                                    self.open_current();
                                    return;
                                }
                                if let Some(err) = &self.err {
                                    ui.add_space(th.spacing.sm);
                                    dim_label(ui, &th, err);
                                }
                                return;
                            }

                            let PaintResult {
                                pending_msg,
                                error,
                            } = gleam_bridge::paint(
                                ui,
                                &th,
                                model,
                                &self.slots,
                                &mut self.repo_ui,
                                self.profile.as_ref(),
                            );

                            if let Some(err) = error {
                                self.err = Some(err);
                            }
                            if self.repo_ui.reload_requested {
                                self.reload_current();
                                return;
                            }
                            self.persist_tab_prefs_if_dirty();
                            if let Some(msg) = pending_msg {
                                self.handle_msg(msg);
                            }
                        });
                    },
                );
            });

        paint_toast(
            ctx,
            &th,
            self.toast
                .as_ref()
                .map(|(m, _, at)| (m.as_str(), *at)),
        );
    }
}

fn restore_tab_prefs(
    ui: &mut RepoUi,
    store: &TabPrefsStore,
    profile: &Profile,
    files: &[crate::view_api::FileRow],
) {
    if let Some(prefs) = tab_prefs::get(store, &ui.rid).cloned() {
        tab_prefs::apply_to_ui(&prefs, ui);
        ui.prefs_dirty = false;
        if ui.tab == Tab::Files {
            ui.open_readme_if_present(profile, files);
        }
    } else {
        ui.open_readme_if_present(profile, files);
    }
}

fn paint_toast(ctx: &egui::Context, th: &Theme, toast: Option<(&str, Option<egui::Pos2>)>) {
    let Some((msg, anchor)) = toast else {
        return;
    };
    // Prefer anchoring just under the source chip; fall back to bottom-center.
    let mut area = egui::Area::new(egui::Id::new("toast"))
        .order(egui::Order::Foreground)
        .interactable(false);
    area = if let Some(at) = anchor {
        let gap = th.spacing.xs;
        area.pivot(egui::Align2::CENTER_TOP)
            .fixed_pos(egui::pos2(at.x, at.y + gap))
    } else {
        area.anchor(egui::Align2::CENTER_BOTTOM, [0.0, -24.0])
    };
    area.show(ctx, |ui| {
        Frame::new()
            .fill(th.palette.popover_bg)
            .stroke(Stroke::new(1.0_f32, th.palette.border))
            .corner_radius(th.spacing.radius_md)
            .inner_margin(Margin::symmetric(
                th.spacing.lg as i8,
                th.spacing.md as i8,
            ))
            .shadow(Shadow {
                offset: [0, 4],
                blur: 12,
                spread: 0,
                color: th.palette.shade,
            })
            .show(ui, |ui| {
                ui.label(
                    RichText::new(msg)
                        .size(th.type_scale.body)
                        .color(th.palette.text),
                );
            });
    });
}

struct RidFieldResult {
    /// Screen position of the copy chip when clicked (bottom-center of icon).
    copy_clicked_at: Option<egui::Pos2>,
    enter: bool,
}

/// Framed RID field of exact height `h`, with a vertically centered in-field copy icon.
fn rid_input_field(
    ui: &mut egui::Ui,
    th: &Theme,
    text: &mut String,
    h: f32,
) -> RidFieldResult {
    let mut out = RidFieldResult {
        copy_clicked_at: None,
        enter: false,
    };

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
    let icon_hit = 22.0_f32;
    let icon_center = egui::pos2(rect.right() - pad_x - icon_hit * 0.5, rect.center().y);
    let icon_rect = egui::Rect::from_center_size(icon_center, Vec2::splat(icon_hit));

    let icon_r = ui
        .interact(icon_rect, ui.id().with("rid_copy"), Sense::click())
        .on_hover_text("Copy RID")
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    let icon_color = if icon_r.hovered() {
        th.palette.text
    } else {
        th.palette.text_secondary
    };
    paint_icon_in(ui, icon_rect.shrink(icon_hit * 0.22), Icon::Copy, icon_color);
    if icon_r.clicked() {
        // Anchor under the chip (bottom center), not the icon midpoint.
        out.copy_clicked_at = Some(egui::pos2(icon_rect.center().x, icon_rect.bottom()));
    }

    let edit_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + pad_x, rect.top()),
        egui::pos2(icon_rect.left() - th.spacing.xs, rect.bottom()),
    );
    let inner = ui.allocate_new_ui(
        UiBuilder::new()
            .max_rect(edit_rect)
            .layout(Layout::left_to_right(Align::Center)),
        |ui| {
            ui.add(
                egui::TextEdit::singleline(text)
                    .frame(false)
                    .desired_width(edit_rect.width())
                    .margin(Margin::ZERO)
                    .hint_text("rad:z…"),
            )
        },
    );
    if inner.inner.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        out.enter = true;
    }

    out
}
