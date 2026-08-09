//! Desktop window: RID field + Gleam-driven layout, Radicle fetch on Open.

use eframe::egui;
use radicle::Profile;
use vidya::{apply_dark, body, central_page, dim_label, Theme};

use crate::gleam_bridge::{self, PaintResult, Slots, MSG_FAILED, MSG_LOADED, MSG_OPEN};
use crate::gleam_guest;
use crate::rad;

pub fn run(initial_rid: Option<String>) -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([780.0, 640.0])
            .with_min_inner_size([400.0, 480.0])
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
    err: Option<String>,
    auto_open: bool,
}

impl BrowseApp {
    fn new(cc: &eframe::CreationContext<'_>, initial_rid: Option<String>) -> Self {
        apply_dark(&cc.egui_ctx);
        let theme = Theme::dark();

        let (profile, profile_err) = match rad::load_profile() {
            Ok(p) => (Some(p), None),
            Err(e) => (None, Some(e.to_string())),
        };

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
            err,
            auto_open,
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
                self.slots = Slots::from_view(&view);
                self.err = None;
                if let Some(m) = self.model {
                    match gleam_guest::update(m, MSG_LOADED) {
                        Ok(n) => self.model = Some(n),
                        Err(e) => self.err = Some(e),
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

    fn handle_msg(&mut self, msg: i64) {
        if msg == MSG_OPEN {
            self.open_current();
            return;
        }
        if let Some(m) = self.model {
            match gleam_guest::update(m, msg) {
                Ok(n) => {
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

        if self.auto_open {
            self.auto_open = false;
            if !self.rid_input.trim().is_empty() {
                self.open_current();
            }
        }

        central_page(ctx, &th, "browse", |g| {
            g.section(|ui| {
                if self.profile.is_none() {
                    dim_label(ui, &th, "Could not load ~/.radicle profile.");
                    if let Some(err) = &self.err {
                        body(ui, &th, err);
                    }
                    return;
                }

                // Host-owned RID field (Gleam shell has no text-input opcode yet).
                ui.horizontal(|ui| {
                    ui.label("RID");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.rid_input)
                            .desired_width(480.0)
                            .hint_text("rad:z…"),
                    );
                });
                ui.add_space(th.spacing.sm);

                let Some(model) = self.model else {
                    if let Some(err) = &self.err {
                        dim_label(ui, &th, err);
                    }
                    return;
                };

                let PaintResult {
                    pending_msg,
                    error,
                } = gleam_bridge::paint(ui, &th, model, &self.slots);

                if let Some(err) = error {
                    self.err = Some(err);
                }
                if let Some(msg) = pending_msg {
                    self.handle_msg(msg);
                }
                if let Some(err) = &self.err {
                    // Only show host err banner on enter screen; viewing/error
                    // screens already surface slots.
                    if model == 0 {
                        ui.add_space(th.spacing.sm);
                        dim_label(ui, &th, err);
                    }
                }
            });
        });
    }
}
