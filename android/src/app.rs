//! Desktop window: RID field + Gleam-driven layout, Radicle fetch on Open.

use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align, Frame, Layout, Margin, RichText, Sense, Shadow, Stroke, StrokeKind, UiBuilder,
    Vec2,
};
use radicle::Profile;
use vidya::{
    apply_dark, body, button, clipboard_text, dim_label, grid_cols_with, normalize_paste,
    paint_icon_in, primary_button, reserve_system_chrome, set_clipboard_text, sync_soft_keyboard,
    title, ColSpec, GridOpts, Icon, Theme,
};

use crate::components::{RepoList, RepoUi, Tab};
use crate::gleam_bridge::{self, PaintResult, Slots, MSG_BACK, MSG_FAILED, MSG_LOADED, MSG_OPEN};
use crate::gleam_guest;
use crate::rad::{self, RepoSummary, RepoView};
use crate::recent::{self, RecentRepo};
use crate::tab_prefs::{self, TabPrefsStore};
use crate::view_api::ui_label;

const TOAST_SECS: u64 = 2;

/// Background open / inventory results (worker thread → UI thread).
enum LoadMsg {
    /// Identity + files + commits — enough to leave the enter page.
    Core(Result<RepoView, String>),
    /// Patches / issues / jobs for a RID already shown from [`LoadMsg::Core`].
    Cobs {
        rid: String,
        result: Result<
            (
                Vec<crate::view_api::PatchRow>,
                Vec<crate::view_api::IssueRow>,
                Vec<crate::view_api::JobRow>,
            ),
            String,
        >,
    },
    /// Local storage inventory for the enter (node) page.
    Inventory(Result<Vec<RepoSummary>, String>),
}

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
    /// Alias for creating a profile when none is loaded.
    alias_input: String,
    /// Optional passphrase for profile creation (empty = unencrypted key).
    passphrase_input: String,
    model: Option<i64>,
    slots: Slots,
    repo_ui: RepoUi,
    local_repos: Vec<RepoSummary>,
    recent_repos: Vec<RecentRepo>,
    tab_prefs: TabPrefsStore,
    err: Option<String>,
    auto_open: bool,
    /// In-flight background work (open / COBs / inventory). Keeps the UI thread free.
    load_rx: Option<Receiver<LoadMsg>>,
    /// True while waiting for [`LoadMsg::Core`] (or a failed open).
    opening: bool,
    /// True while patches/issues/jobs are still loading after core.
    cobs_loading: bool,
    /// True while the enter-page local inventory is loading.
    inventory_loading: bool,
    /// True when the in-flight open is a reload of the repo already on screen.
    reload_in_flight: bool,
    /// Waiting for Java UI-thread clipboard read (Android NativeActivity).
    paste_pending: bool,
    /// When [`Self::paste_pending`] was set — times out hung UI-thread reads.
    paste_started: Option<Instant>,
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

        // Inventory is loaded off-thread so the enter (node) page paints immediately
        // with recently viewed repos; local storage fills in when ready.
        let local_repos = Vec::new();

        let (model, model_err) = match gleam_guest::init() {
            Ok(n) => (Some(n), None),
            Err(e) => (None, Some(e)),
        };

        let auto_open = initial_rid.is_some();
        // Not-found is expected on fresh APK installs — the create form handles it.
        let err = match profile_err {
            Some(e) if e.contains("no Radicle profile found") => model_err,
            other => other.or(model_err),
        };
        let recent_repos = recent::load();
        let tab_prefs = tab_prefs::load();
        let inventory_loading = profile.is_some();

        Self {
            theme,
            profile,
            rid_input: initial_rid.unwrap_or_default(),
            repo_filter: String::new(),
            alias_input: rad::default_alias(),
            passphrase_input: String::new(),
            model,
            slots: Slots::default(),
            repo_ui: RepoUi::default(),
            local_repos,
            recent_repos,
            tab_prefs,
            err,
            auto_open,
            load_rx: None,
            opening: false,
            cobs_loading: false,
            inventory_loading,
            reload_in_flight: false,
            paste_pending: false,
            paste_started: None,
            toast: None,
        }
    }

    fn is_opening(&self) -> bool {
        self.opening
    }

    fn is_busy(&self) -> bool {
        self.opening || self.cobs_loading || self.inventory_loading
    }

    #[allow(dead_code)] // bottom-center toasts for non-chip messages
    fn show_toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now(), None));
    }

    fn show_toast_at(&mut self, msg: impl Into<String>, at: egui::Pos2) {
        self.toast = Some((msg.into(), Instant::now(), Some(at)));
    }

    fn refresh_local_repos(&mut self, ctx: &egui::Context) {
        if self.profile.is_none() {
            self.inventory_loading = false;
            return;
        }
        // Mark wanted even if a repo open owns the channel; update() retries later.
        self.inventory_loading = true;
        self.start_inventory_load(ctx);
    }

    fn create_profile(&mut self, ctx: &egui::Context) {
        match rad::create_profile(&self.alias_input, Some(self.passphrase_input.as_str())) {
            Ok(profile) => {
                let did = profile.did().to_string();
                self.profile = Some(profile);
                self.passphrase_input.clear();
                self.err = None;
                self.refresh_local_repos(ctx);
                self.show_toast(format!("Profile created · {did}"));
            }
            Err(e) => self.err = Some(e.to_string()),
        }
    }

    /// Start a background `open_repo` for the RID field (Open / Enter / list click).
    fn open_current(&mut self, ctx: &egui::Context) {
        if self.profile.is_none() {
            self.err = Some("No Radicle profile loaded.".into());
            if let Some(m) = self.model {
                if let Ok(n) = gleam_guest::update(m, MSG_FAILED) {
                    self.model = Some(n);
                    self.slots = Slots::from_error("No Radicle profile loaded.");
                }
            }
            return;
        }
        let rid = self.rid_input.clone();
        if rid.trim().is_empty() {
            return;
        }
        self.start_load(ctx, rid);
    }

    /// Start a background reload for the repo currently on screen (tab re-press).
    fn reload_current(&mut self, ctx: &egui::Context) {
        if self.profile.is_none() {
            self.repo_ui.reload_requested = false;
            return;
        }
        let rid = if self.rid_input.trim().is_empty() {
            self.repo_ui.rid.clone()
        } else {
            self.rid_input.clone()
        };
        self.repo_ui.reload_requested = false;
        if rid.trim().is_empty() {
            return;
        }
        self.start_load(ctx, rid);
    }

    /// Load local storage inventory off the UI thread (enter / node page).
    fn start_inventory_load(&mut self, ctx: &egui::Context) {
        if self.opening || self.cobs_loading {
            // Repo open owns the channel; inventory retries when it finishes.
            return;
        }
        if self.profile.is_none() {
            self.inventory_loading = false;
            return;
        }
        if self.load_rx.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.load_rx = Some(rx);
        self.inventory_loading = true;
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let profile = rad::load_profile().map_err(|e| e.to_string())?;
                rad::list_local_repos(&profile).map_err(|e| e.to_string())
            })();
            let _ = tx.send(LoadMsg::Inventory(result));
            ctx.request_repaint();
        });
    }

    /// Spawn progressive `open_repo_core` then `open_repo_cobs` off the UI thread.
    fn start_load(&mut self, ctx: &egui::Context, rid: String) {
        if self.opening || self.cobs_loading {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.load_rx = Some(rx);
        self.opening = true;
        self.cobs_loading = false;
        self.inventory_loading = false;
        self.reload_in_flight =
            self.model == Some(1) && !self.repo_ui.rid.is_empty() && self.repo_ui.rid == rid;
        self.err = None;
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let profile = match rad::load_profile() {
                Ok(p) => p,
                Err(e) => {
                    let _ = tx.send(LoadMsg::Core(Err(e.to_string())));
                    ctx.request_repaint();
                    return;
                }
            };
            let core = rad::open_repo_core(&profile, &rid).map_err(|e| e.to_string());
            let core_ok = core.is_ok();
            let _ = tx.send(LoadMsg::Core(core));
            ctx.request_repaint();
            if !core_ok {
                return;
            }
            let cobs = rad::open_repo_cobs(&profile, &rid).map_err(|e| e.to_string());
            let _ = tx.send(LoadMsg::Cobs {
                rid,
                result: cobs,
            });
            ctx.request_repaint();
        });
    }

    /// Apply finished background messages on the UI thread (may be several per open).
    fn poll_load(&mut self) {
        loop {
            let msg = {
                let Some(rx) = self.load_rx.as_ref() else {
                    return;
                };
                match rx.try_recv() {
                    Ok(msg) => msg,
                    Err(mpsc::TryRecvError::Empty) => return,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.load_rx = None;
                        self.opening = false;
                        self.cobs_loading = false;
                        self.inventory_loading = false;
                        return;
                    }
                }
            };
            match msg {
                LoadMsg::Core(result) => {
                    self.opening = false;
                    match result {
                        Ok(view) => {
                            self.cobs_loading = true;
                            self.apply_open_result(view);
                        }
                        Err(msg) => {
                            self.cobs_loading = false;
                            self.reload_in_flight = false;
                            self.load_rx = None;
                            let already_viewing =
                                self.model == Some(1) && !self.repo_ui.rid.is_empty();
                            if already_viewing {
                                self.err = Some(msg);
                            } else {
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
                }
                LoadMsg::Cobs { rid, result } => {
                    self.cobs_loading = false;
                    self.load_rx = None;
                    match result {
                        Ok((patches, issues, jobs)) => {
                            self.apply_cobs(&rid, patches, issues, jobs);
                        }
                        Err(msg) => {
                            self.reload_in_flight = false;
                            // Core view stays; surface COB failure without tearing down.
                            self.err = Some(msg);
                        }
                    }
                }
                LoadMsg::Inventory(result) => {
                    self.inventory_loading = false;
                    if !self.opening && !self.cobs_loading {
                        self.load_rx = None;
                    }
                    match result {
                        Ok(repos) => self.local_repos = repos,
                        Err(e) => self.err = Some(e),
                    }
                }
            }
        }
    }

    fn apply_cobs(
        &mut self,
        rid: &str,
        patches: Vec<crate::view_api::PatchRow>,
        issues: Vec<crate::view_api::IssueRow>,
        jobs: Vec<crate::view_api::JobRow>,
    ) {
        if self.repo_ui.rid != rid {
            self.reload_in_flight = false;
            return;
        }
        let toast_reload = self.reload_in_flight;
        self.reload_in_flight = false;
        self.slots.patches = patches;
        self.slots.issues = issues;
        self.slots.jobs = jobs;
        self.repo_ui
            .prune_cob_selections(&self.slots.patches, &self.slots.issues, &self.slots.jobs);
        self.err = None;
        if toast_reload {
            self.show_toast("Reloaded");
        }
    }

    fn apply_open_result(&mut self, view: RepoView) {
        recent::record(
            &mut self.recent_repos,
            &view.rid,
            &view.name,
            &view.description,
        );
        recent::save(&self.recent_repos);
        let already_viewing = self.model == Some(1) && self.repo_ui.rid == view.rid;
        if already_viewing {
            // Keep existing COB lists until `LoadMsg::Cobs` arrives so the tabs
            // do not flash empty during progressive reload.
            self.apply_core_reload(&view);
        } else if let Some(profile) = &self.profile {
            self.repo_ui
                .reset_for(&view.rid, &view.head_oid, &view.files);
            restore_tab_prefs(&mut self.repo_ui, &self.tab_prefs, profile, &view.files);
            self.slots = Slots::from_view(&view);
            self.err = None;
            if let Some(m) = self.model {
                match gleam_guest::update(m, MSG_LOADED) {
                    Ok(n) => self.model = Some(n),
                    Err(e) => self.err = Some(e),
                }
            }
        } else {
            self.err = Some("No Radicle profile loaded.".into());
        }
    }

    /// Refresh files/commits/meta in place; leave patch/issue/job rows untouched.
    fn apply_core_reload(&mut self, view: &RepoView) {
        self.repo_ui.head_oid = view.head_oid.clone();
        self.slots.strings = vec![
            view.name.clone(),
            if view.description.is_empty() {
                "(no description)".into()
            } else {
                view.description.clone()
            },
            view.rid.clone(),
            format!("head {}", view.head),
            view.readme.clone(),
            String::new(),
        ];
        self.slots.files = view.files.clone();
        self.slots.commits = view.commits.clone();
        self.repo_ui.reload_requested = false;
        self.err = None;
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

    fn handle_msg(&mut self, ctx: &egui::Context, msg: i64) {
        if msg == MSG_OPEN {
            self.open_current(ctx);
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
                        self.refresh_local_repos(ctx);
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

        // First frames: kick off local inventory without blocking paint.
        if self.inventory_loading && self.load_rx.is_none() && !self.opening && !self.cobs_loading {
            self.start_inventory_load(ctx);
        }

        self.poll_load();
        if self.is_busy() {
            // Keep the frame alive while the worker runs so toasts / UI stay live.
            ctx.request_repaint_after(Duration::from_millis(50));
        }

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
                self.open_current(ctx);
            }
        }

        // Android edge-to-edge: clear status / gesture bars before CentralPanel.
        // No-op on desktop. See vidya::reserve_system_chrome / top_header.
        reserve_system_chrome(ctx, &th);

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
                                let mut create = false;
                                title(ui, &th, "Create Radicle profile");
                                body(
                                    ui,
                                    &th,
                                    "No profile found. Create one to browse local storage.",
                                );
                                ui.add_space(th.spacing.sm);
                                dim_label(ui, &th, &format!("Home: {}", rad::home_display()));
                                ui.add_space(th.spacing.md);

                                let h = th.spacing.control_height;
                                ui.horizontal(|ui| {
                                    ui.set_min_height(h);
                                    ui.label("Alias");
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.alias_input)
                                            .desired_width(ui.available_width().max(1.0))
                                            .hint_text("node alias"),
                                    );
                                });
                                ui.add_space(th.spacing.sm);
                                ui.horizontal(|ui| {
                                    ui.set_min_height(h);
                                    ui.label("Passphrase");
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.passphrase_input)
                                            .password(true)
                                            .desired_width(ui.available_width().max(1.0))
                                            .hint_text("optional"),
                                    );
                                });
                                ui.add_space(th.spacing.md);
                                if primary_button(ui, &th, "Create profile").clicked() {
                                    create = true;
                                }
                                if let Some(err) = &self.err {
                                    ui.add_space(th.spacing.sm);
                                    dim_label(ui, &th, err);
                                }
                                if create {
                                    self.create_profile(ui.ctx());
                                }
                                return;
                            }

                            let mut enter_open = false;
                            let mut btn_open = false;
                            let mut paste_rid = false;
                            let mut copy_rid_at = None;

                            let h = th.spacing.control_height;
                            let row_w = ui.available_width().max(1.0);
                            ui.allocate_ui_with_layout(
                                Vec2::new(row_w, h),
                                Layout::right_to_left(Align::Center),
                                |ui| {
                                    ui.set_min_height(h);
                                    ui.set_max_height(h);
                            let open_label = if self.is_opening() {
                                "Opening…".to_string()
                            } else {
                                ui_label(2)
                            };
                            if primary_button(ui, &th, &open_label)
                                .on_hover_text("Open repo; press again to reload")
                                .clicked()
                                && !self.is_opening()
                                && !self.cobs_loading
                            {
                                btn_open = true;
                            }
                                    ui.add_space(th.spacing.sm);
                                    // Full-height Paste control — NativeActivity has no
                                    // soft-keyboard / long-press paste into TextEdit.
                                    if button(ui, &th, "Paste")
                                        .on_hover_text("Paste RID from the system clipboard")
                                        .clicked()
                                    {
                                        paste_rid = true;
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
                                            ui.label(ui_label(3));
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

                            if paste_rid {
                                // Android: clipboard JNI must run on the Java UI
                                // thread with a focused EditText (API 29+).
                                #[cfg(target_os = "android")]
                                {
                                    crate::android_clipboard::request();
                                    self.paste_pending = true;
                                    self.paste_started = Some(Instant::now());
                                    ui.ctx().request_repaint();
                                }
                                #[cfg(not(target_os = "android"))]
                                {
                                    match clipboard_text() {
                                        Some(raw) => {
                                            let pasted = normalize_paste(&raw);
                                            if pasted.is_empty() {
                                                self.show_toast("Clipboard empty");
                                            } else {
                                                self.rid_input = pasted;
                                                self.show_toast("RID pasted");
                                            }
                                        }
                                        None => self.show_toast(
                                            "Clipboard unavailable — copy a rad:z… ID, then Paste",
                                        ),
                                    }
                                }
                            }
                            if self.paste_pending {
                                #[cfg(target_os = "android")]
                                {
                                    match crate::android_clipboard::poll() {
                                        None => {
                                            let timed_out = self
                                                .paste_started
                                                .is_some_and(|t| t.elapsed() > Duration::from_secs(2));
                                            if timed_out {
                                                self.paste_pending = false;
                                                self.paste_started = None;
                                                self.show_toast(
                                                    "Clipboard unavailable — copy a rad:z… ID, then Paste",
                                                );
                                            } else {
                                                ui.ctx().request_repaint();
                                            }
                                        }
                                        Some(None) => {
                                            self.paste_pending = false;
                                            self.paste_started = None;
                                            self.show_toast(
                                                "Clipboard unavailable — copy a rad:z… ID, then Paste",
                                            );
                                        }
                                        Some(Some(raw)) => {
                                            self.paste_pending = false;
                                            self.paste_started = None;
                                            let pasted = normalize_paste(&raw);
                                            if pasted.is_empty() {
                                                self.show_toast("Clipboard empty");
                                            } else {
                                                self.rid_input = pasted;
                                                self.show_toast("RID pasted");
                                            }
                                        }
                                    }
                                }
                                #[cfg(not(target_os = "android"))]
                                {
                                    self.paste_pending = false;
                                    self.paste_started = None;
                                }
                            }
                            if let Some(at) = copy_rid_at {
                                let rid = self.rid_input.trim();
                                if !rid.is_empty() {
                                    // egui-winit has no Android OS clipboard; also write via Vidya + browse.
                                    ui.ctx().copy_text(rid.to_string());
                                    set_clipboard_text(rid);
                                    #[cfg(target_os = "android")]
                                    crate::android_clipboard::set_text(rid);
                                    self.show_toast_at("RID copied", at);
                                }
                            }
                            if (enter_open || btn_open) && !self.is_opening() && !self.cobs_loading
                            {
                                self.open_current(ui.ctx());
                                return;
                            }
                            ui.add_space(th.spacing.sm);
                            if self.is_opening() {
                                dim_label(ui, &th, "Opening repo…");
                                ui.add_space(th.spacing.sm);
                            } else if self.cobs_loading {
                                dim_label(ui, &th, "Loading patches…");
                                ui.add_space(th.spacing.sm);
                            } else if self.inventory_loading && self.model == Some(0) {
                                dim_label(ui, &th, "Loading local repos…");
                                ui.add_space(th.spacing.sm);
                            }

                            let Some(model) = self.model else {
                                if let Some(err) = &self.err {
                                    dim_label(ui, &th, err);
                                }
                                return;
                            };

                            // Startup: Gleam enter chrome then host inventory.
                            if model == 0 {
                                self.slots =
                                    Slots::for_enter(&self.local_repos, &self.repo_filter);
                                let PaintResult {
                                    pending_msg,
                                    open_rid,
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
                                if let Some(rid) = open_rid {
                                    if !self.is_opening() && !self.cobs_loading {
                                        self.rid_input = rid;
                                        self.open_current(ui.ctx());
                                        return;
                                    }
                                }
                                if let Some(msg) = pending_msg {
                                    if !self.is_opening() {
                                        self.handle_msg(ui.ctx(), msg);
                                    }
                                    return;
                                }

                                // Keep the node inventory painted while a repo opens so the
                                // enter page does not blank out during the load.
                                let clicked = RepoList::show(
                                    ui,
                                    &th,
                                    &self.recent_repos,
                                    &self.local_repos,
                                    &mut self.repo_filter,
                                );
                                if let Some(rid) = clicked {
                                    if !self.is_opening() && !self.cobs_loading {
                                        self.rid_input = rid;
                                        self.open_current(ui.ctx());
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
                                open_rid,
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
                            if let Some(rid) = open_rid {
                                if !self.is_opening() && !self.cobs_loading {
                                    self.rid_input = rid;
                                    self.open_current(ui.ctx());
                                    return;
                                }
                            }
                            if self.repo_ui.reload_requested {
                                // Clear immediately and load off-thread so the click
                                // cannot freeze the UI on large patch/issue stores.
                                self.reload_current(ui.ctx());
                            }
                            self.persist_tab_prefs_if_dirty();
                            if let Some(msg) = pending_msg {
                                if !self.is_opening() {
                                    self.handle_msg(ui.ctx(), msg);
                                }
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

        // After widgets: show/hide the soft keyboard when a TextEdit has focus.
        sync_soft_keyboard(ctx);
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
        .interactable(false)
        // First-frame width under Vidya's global Wrap; avoids mid-phrase breaks
        // when the Area is constrained (e.g. chip near the right edge).
        .default_width(ctx.style().spacing.tooltip_width);
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
                ui.add(
                    egui::Label::new(
                        RichText::new(msg)
                            .size(th.type_scale.body)
                            .color(th.palette.text),
                    )
                    .extend(),
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
///
/// Paste lives in a separate full-size button next to Open — soft-keyboard /
/// long-press paste does not reach TextEdit on NativeActivity.
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
