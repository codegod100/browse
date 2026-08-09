//! Desktop window: RID field + Gleam-driven layout, Radicle fetch on Open.

use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align, Frame, Layout, Margin, RichText, Sense, Shadow, Stroke, StrokeKind, UiBuilder,
    Vec2,
};
use radicle::Profile;
use vidya::{
    apply_dark, body, card, central_page, dim_label, paint_icon_in, primary_button, Icon, Theme,
};

use crate::components::{RepoList, RepoUi};
use crate::gleam_bridge::{self, PaintResult, Slots, MSG_BACK, MSG_FAILED, MSG_LOADED, MSG_OPEN};
use crate::gleam_guest;
use crate::rad::{self, CloneOutcome, RepoSummary};

const TOAST_SECS: u64 = 2;

enum PendingClone {
    Idle,
    Working {
        rx: Receiver<Result<CloneOutcome, String>>,
    },
}

pub fn run(initial_rid: Option<String>) -> eframe::Result {
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

struct BrowseApp {
    theme: Theme,
    profile: Option<Profile>,
    rid_input: String,
    model: Option<i64>,
    slots: Slots,
    repo_ui: RepoUi,
    local_repos: Vec<RepoSummary>,
    err: Option<String>,
    auto_open: bool,
    toast: Option<(String, Instant)>,
    pending_clone: PendingClone,
    status: Option<String>,
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

        Self {
            theme,
            profile,
            rid_input: initial_rid.unwrap_or_default(),
            model,
            slots: Slots::default(),
            repo_ui: RepoUi::default(),
            local_repos,
            err,
            auto_open,
            toast: None,
            pending_clone: PendingClone::Idle,
            status: None,
        }
    }

    fn show_toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    fn refresh_local_repos(&mut self) {
        if let Some(profile) = &self.profile {
            match rad::list_local_repos(profile) {
                Ok(repos) => self.local_repos = repos,
                Err(e) => self.err = Some(e.to_string()),
            }
        }
    }

    fn fail_open(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        self.slots = Slots::from_error(&msg);
        self.err = Some(msg);
        self.status = None;
        if let Some(m) = self.model {
            match gleam_guest::update(m, MSG_FAILED) {
                Ok(n) => self.model = Some(n),
                Err(e) => self.err = Some(e),
            }
        }
    }

    fn apply_view(&mut self, view: rad::RepoView) {
        let Some(profile) = &self.profile else {
            return;
        };
        self.repo_ui.reset_for(&view.rid, &view.head_oid, &view.files);
        self.repo_ui.open_readme_if_present(profile, &view.files);
        self.slots = Slots::from_view(&view);
        self.err = None;
        self.status = None;
        if let Some(m) = self.model {
            match gleam_guest::update(m, MSG_LOADED) {
                Ok(n) => self.model = Some(n),
                Err(e) => self.err = Some(e),
            }
        }
    }

    fn apply_clone_outcome(&mut self, outcome: CloneOutcome) {
        self.refresh_local_repos();
        if outcome.fetched && outcome.checked_out {
            self.show_toast(format!(
                "Cloned {} → {}",
                outcome.name,
                outcome.path.display()
            ));
        } else if outcome.fetched {
            self.show_toast(format!("Cloned {} to local storage", outcome.name));
        } else if outcome.checked_out {
            self.show_toast(format!(
                "Moved {} → {}",
                outcome.name,
                outcome.path.display()
            ));
        }
        self.open_after_clone(&outcome.rid);
    }

    fn open_after_clone(&mut self, rid: &str) {
        let Some(profile) = &self.profile else {
            self.fail_open("No Radicle profile loaded.");
            return;
        };
        match rad::open_repo(profile, rid) {
            Ok(view) => self.apply_view(view),
            Err(e) => self.fail_open(e.to_string()),
        }
    }

    fn open_current(&mut self) {
        if !matches!(self.pending_clone, PendingClone::Idle) {
            return;
        }

        let Some(profile) = &self.profile else {
            self.fail_open("No Radicle profile loaded.");
            return;
        };

        let rid = self.rid_input.trim().to_string();
        if rid.is_empty() {
            self.fail_open("Enter a Radicle ID (rad:z…).");
            return;
        }

        // Already local: ensure ~/code checkout, then open. Missing: clone on a worker thread.
        let local = match rad::has_local(profile, &rid) {
            Ok(v) => v,
            Err(e) => {
                self.fail_open(e.to_string());
                return;
            }
        };

        if local {
            let checkout = rad::clone_to_local(profile, &rid);
            let view = rad::open_repo(profile, &rid);
            match (checkout, view) {
                (Ok(outcome), Ok(view)) => {
                    if outcome.checked_out {
                        self.show_toast(format!(
                            "Moved {} to {}",
                            outcome.name,
                            outcome.path.display()
                        ));
                    }
                    self.apply_view(view);
                }
                (Err(e), Ok(view)) => {
                    // Checkout may fail (path busy); still open from storage.
                    self.show_toast(format!("Opened from storage ({e})"));
                    self.apply_view(view);
                }
                (_, Err(e)) => self.fail_open(e.to_string()),
            }
            return;
        }

        self.status = Some(format!("Cloning {rid} to local storage…"));
        self.err = None;
        // Profile isn't Sync; re-load inside the worker.
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let result = (|| {
                let profile = rad::load_profile().map_err(|e| e.to_string())?;
                rad::clone_to_local(&profile, &rid).map_err(|e| e.to_string())
            })();
            let _ = tx.send(result);
        });
        self.pending_clone = PendingClone::Working { rx };
    }

    fn poll_clone(&mut self, ctx: &egui::Context) {
        let rx = match &self.pending_clone {
            PendingClone::Working { rx, .. } => rx,
            PendingClone::Idle => return,
        };
        let msg = match rx.try_recv() {
            Ok(v) => Some(v),
            Err(mpsc::TryRecvError::Empty) => {
                ctx.request_repaint_after(Duration::from_millis(100));
                None
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("Clone worker disconnected.".into()))
            }
        };
        if let Some(result) = msg {
            self.pending_clone = PendingClone::Idle;
            match result {
                Ok(outcome) => self.apply_clone_outcome(outcome),
                Err(e) => self.fail_open(e),
            }
        }
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
                        self.refresh_local_repos();
                        self.rid_input.clear();
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

        if let Some((_, at)) = &self.toast {
            if at.elapsed() > Duration::from_secs(TOAST_SECS) {
                self.toast = None;
            } else {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
        }

        self.poll_clone(ctx);

        if self.auto_open {
            self.auto_open = false;
            if !self.rid_input.trim().is_empty() {
                self.open_current();
            }
        }

        let cloning = !matches!(self.pending_clone, PendingClone::Idle);

        central_page(ctx, &th, "browse", |g| {
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
                let mut copy_rid = false;

                let h = th.spacing.control_height;
                let row_w = ui.available_width().max(1.0);
                ui.allocate_ui_with_layout(
                    Vec2::new(row_w, h),
                    Layout::right_to_left(Align::Center),
                    |ui| {
                        ui.set_min_height(h);
                        ui.set_max_height(h);
                        let open_label = if cloning { "Cloning…" } else { "Open" };
                        let open = primary_button(ui, &th, open_label);
                        if open.clicked() && !cloning {
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
                                let field = rid_input_field(ui, &th, &mut self.rid_input, h);
                                if field.copy_clicked {
                                    copy_rid = true;
                                }
                                if field.enter {
                                    enter_open = true;
                                }
                            },
                        );
                    },
                );

                if copy_rid {
                    let rid = self.rid_input.trim();
                    if !rid.is_empty() {
                        ui.ctx().copy_text(rid.to_string());
                        self.show_toast("RID copied");
                    }
                }
                if (enter_open || btn_open) && !cloning {
                    self.open_current();
                    return;
                }
                ui.add_space(th.spacing.sm);

                if let Some(status) = &self.status {
                    dim_label(ui, &th, status);
                    ui.add_space(th.spacing.sm);
                }

                let Some(model) = self.model else {
                    if let Some(err) = &self.err {
                        dim_label(ui, &th, err);
                    }
                    return;
                };

                // Startup: local inventory under the RID row.
                if model == 0 {
                    let mut clicked = None;
                    card(ui, &th, |ui| {
                        clicked = RepoList::show(ui, &th, &self.local_repos, &self.rid_input);
                    });
                    if let Some(rid) = clicked {
                        if !cloning {
                            self.rid_input = rid;
                            self.open_current();
                            return;
                        }
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
                if let Some(msg) = pending_msg {
                    self.handle_msg(msg);
                }
            });
        });

        paint_toast(ctx, &th, self.toast.as_ref().map(|(m, _)| m.as_str()));
    }
}

fn paint_toast(ctx: &egui::Context, th: &Theme, msg: Option<&str>) {
    let Some(msg) = msg else {
        return;
    };
    egui::Area::new(egui::Id::new("toast"))
        .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -24.0])
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
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
    copy_clicked: bool,
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
        copy_clicked: false,
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
        out.copy_clicked = true;
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
                    .hint_text("rad:z… — open local or clone + move to ~/code"),
            )
        },
    );
    if inner.inner.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        out.enter = true;
    }

    out
}
