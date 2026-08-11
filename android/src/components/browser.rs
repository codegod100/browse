//! Interactive Files / Commits / Patches / Issues / Jobs browser (host-owned selection).

use eframe::egui::{
    self, Align, CornerRadius, CursorIcon, FontFamily, FontId, Layout, Margin, Pos2, Rect,
    RichText, Sense, Stroke, StrokeKind, UiBuilder, Vec2,
};
use radicle::Profile;
use vidya::{body, button, card, dim_label, primary_button, side_by_side, title_2, Theme};

use crate::linkify;
use crate::markdown;
use crate::rad;
use crate::view_api::{CommitRow, FileRow, IssueRow, JobRow, PatchRow, ViewModel, ui_label};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Files,
    Commits,
    Patches,
    Issues,
    Jobs,
}

/// Patch list status tabs (Radicle states + All).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PatchStatus {
    #[default]
    Open,
    Draft,
    Merged,
    Archived,
    All,
}

impl PatchStatus {
    const ALL: &[Self] = &[
        Self::Open,
        Self::Draft,
        Self::Merged,
        Self::Archived,
        Self::All,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Draft => "Draft",
            Self::Merged => "Merged",
            Self::Archived => "Archived",
            Self::All => "All",
        }
    }

    fn matches(self, state: &str) -> bool {
        match self {
            Self::All => true,
            Self::Open => state == "open",
            Self::Draft => state == "draft",
            Self::Merged => state == "merged",
            Self::Archived => state == "archived",
        }
    }
}

/// Issue list status tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IssueStatus {
    #[default]
    Open,
    Closed,
    All,
}

impl IssueStatus {
    const ALL: &[Self] = &[Self::Open, Self::Closed, Self::All];

    fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Closed => "Closed",
            Self::All => "All",
        }
    }

    fn matches(self, state: &str) -> bool {
        match self {
            Self::All => true,
            Self::Open => state == "open",
            Self::Closed => state == "closed",
        }
    }
}

/// Job list status tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JobStatus {
    #[default]
    All,
    Started,
    Succeeded,
    Failed,
}

impl JobStatus {
    const ALL: &[Self] = &[Self::All, Self::Started, Self::Succeeded, Self::Failed];

    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Started => "Started",
            Self::Succeeded => "Succeeded",
            Self::Failed => "Failed",
        }
    }

    fn matches(self, status: &str) -> bool {
        match self {
            Self::All => true,
            Self::Started => status == "started",
            Self::Succeeded => status == "succeeded",
            Self::Failed => status == "failed",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RepoUi {
    pub tab: Tab,
    pub rid: String,
    pub head_oid: String,

    /// Current directory relative to repo root (`""` = root).
    pub cwd: String,
    pub dir_entries: Vec<FileRow>,

    pub selected_file: Option<String>,
    pub file_content: String,
    pub file_is_md: bool,
    pub file_error: Option<String>,

    pub selected_commit: Option<String>,
    pub commit_paths: Vec<String>,
    pub selected_diff_path: Option<String>,
    pub diff_text: String,
    pub commit_error: Option<String>,

    pub selected_patch: Option<String>,
    pub patch_base: String,
    pub patch_head: String,
    pub patch_paths: Vec<String>,
    pub selected_patch_diff_path: Option<String>,
    pub patch_diff_text: String,
    pub patch_error: Option<String>,

    pub selected_issue: Option<String>,
    pub selected_job: Option<String>,

    /// Set when the active Patches / Issues / Jobs tab is pressed again.
    pub reload_requested: bool,

    /// Status tab for the Patches list (Open / Draft / …).
    pub patch_status: PatchStatus,
    /// Status tab for the Issues list (Open / Closed / All).
    pub issue_status: IssueStatus,
    /// Status tab for the Jobs list.
    pub job_status: JobStatus,

    /// Substring filter for the Patches tab list.
    pub patch_filter: String,
    /// Substring filter for the Issues tab list.
    pub issue_filter: String,

    /// Set when tab or status filters change so the host can persist prefs.
    pub prefs_dirty: bool,
}

impl RepoUi {
    pub fn reset_for(&mut self, rid: &str, head_oid: &str, files: &[FileRow]) {
        *self = Self {
            rid: rid.to_string(),
            head_oid: head_oid.to_string(),
            dir_entries: files.to_vec(),
            ..Self::default()
        };
    }

    /// Drop COB selections that no longer exist after a reload.
    pub fn prune_cob_selections(
        &mut self,
        patches: &[PatchRow],
        issues: &[IssueRow],
        jobs: &[JobRow],
    ) {
        if let Some(id) = self.selected_patch.as_deref() {
            if !patches.iter().any(|p| p.id == id) {
                self.clear_patch_selection();
            }
        }
        if let Some(id) = self.selected_issue.as_deref() {
            if !issues.iter().any(|i| i.id == id) {
                self.selected_issue = None;
            }
        }
        if let Some(id) = self.selected_job.as_deref() {
            if !jobs.iter().any(|j| j.id == id) {
                self.selected_job = None;
            }
        }
    }

    fn join_cwd(&self, name: &str) -> String {
        if self.cwd.is_empty() {
            name.to_string()
        } else {
            format!("{}/{}", self.cwd, name)
        }
    }

    fn load_dir(&mut self, profile: &Profile, dir: &str) {
        match rad::list_dir(profile, &self.rid, &self.head_oid, dir) {
            Ok(entries) => {
                self.cwd = dir.to_string();
                self.dir_entries = entries;
                self.file_error = None;
            }
            Err(e) => {
                self.file_error = Some(e.to_string());
            }
        }
    }

    fn enter_dir(&mut self, profile: &Profile, name: &str) {
        let next = self.join_cwd(name);
        self.load_dir(profile, &next);
    }

    fn go_up(&mut self, profile: &Profile) {
        if self.cwd.is_empty() {
            return;
        }
        let parent = match self.cwd.rsplit_once('/') {
            Some((p, _)) => p.to_string(),
            None => String::new(),
        };
        self.load_dir(profile, &parent);
    }

    fn select_file(&mut self, profile: &Profile, path: &str) {
        self.selected_file = Some(path.to_string());
        self.file_error = None;
        self.file_is_md = path
            .rsplit('.')
            .next()
            .is_some_and(|e| matches!(e, "md" | "markdown" | "mdown" | "mkd"));
        match rad::read_file(profile, &self.rid, &self.head_oid, path) {
            Ok(text) => self.file_content = text,
            Err(e) => {
                self.file_content.clear();
                self.file_error = Some(e.to_string());
            }
        }
    }

    /// Open the Files tab on a README blob when the tree has one.
    pub fn open_readme_if_present(&mut self, profile: &Profile, files: &[FileRow]) {
        self.tab = Tab::Files;
        const CANDIDATES: &[&str] = &[
            "README.md",
            "README",
            "README.markdown",
            "README.txt",
            "README.rst",
            "Readme.md",
            "readme.md",
        ];
        for cand in CANDIDATES {
            if let Some(file) = files
                .iter()
                .find(|f| !f.is_tree && f.name.eq_ignore_ascii_case(cand))
            {
                self.select_file(profile, &file.name);
                return;
            }
        }
    }

    fn select_commit(&mut self, profile: &Profile, oid: &str) {
        self.selected_commit = Some(oid.to_string());
        self.selected_diff_path = None;
        self.diff_text.clear();
        self.commit_error = None;
        match rad::commit_paths(profile, &self.rid, oid) {
            Ok(paths) => {
                self.commit_paths = paths;
                if let Some(first) = self.commit_paths.first().cloned() {
                    self.select_diff_path(profile, &first);
                }
            }
            Err(e) => {
                self.commit_paths.clear();
                self.commit_error = Some(e.to_string());
            }
        }
    }

    fn select_diff_path(&mut self, profile: &Profile, path: &str) {
        let Some(oid) = self.selected_commit.clone() else {
            return;
        };
        self.selected_diff_path = Some(path.to_string());
        match rad::file_patch(profile, &self.rid, &oid, path) {
            Ok(text) => self.diff_text = text,
            Err(e) => {
                self.diff_text.clear();
                self.commit_error = Some(e.to_string());
            }
        }
    }

    fn clear_patch_selection(&mut self) {
        self.selected_patch = None;
        self.patch_base.clear();
        self.patch_head.clear();
        self.patch_paths.clear();
        self.selected_patch_diff_path = None;
        self.patch_diff_text.clear();
        self.patch_error = None;
    }

    fn select_patch(&mut self, profile: &Profile, patch: &PatchRow) {
        self.selected_patch = Some(patch.id.clone());
        self.patch_base = patch.base.clone();
        self.patch_head = patch.head.clone();
        self.selected_patch_diff_path = None;
        self.patch_diff_text.clear();
        self.patch_error = None;
        match rad::range_paths(profile, &self.rid, &patch.base, &patch.head) {
            Ok(paths) => {
                self.patch_paths = paths;
                if let Some(first) = self.patch_paths.first().cloned() {
                    self.select_patch_diff_path(profile, &first);
                }
            }
            Err(e) => {
                self.patch_paths.clear();
                self.patch_error = Some(e.to_string());
            }
        }
    }

    fn select_patch_diff_path(&mut self, profile: &Profile, path: &str) {
        if self.selected_patch.is_none() || self.patch_base.is_empty() || self.patch_head.is_empty()
        {
            return;
        }
        self.selected_patch_diff_path = Some(path.to_string());
        match rad::range_file_patch(
            profile,
            &self.rid,
            &self.patch_base,
            &self.patch_head,
            path,
        ) {
            Ok(text) => {
                self.patch_diff_text = text;
                self.patch_error = None;
            }
            Err(e) => {
                self.patch_diff_text.clear();
                self.patch_error = Some(e.to_string());
            }
        }
    }
}

pub struct RepoBrowser;

impl RepoBrowser {
    pub fn show(
        ui: &mut egui::Ui,
        th: &Theme,
        model: &ViewModel,
        state: &mut RepoUi,
        profile: Option<&Profile>,
        leading: Vec<(i64, bool, String)>,
        effects: &mut crate::view_api::PaintEffects,
        cobs_loading: bool,
    ) {
        // Wrap on narrow viewports — Back + five tabs no longer fit one row
        // after SWARM-54 (overflow clipped Jobs into a vertical strip).
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = th.spacing.sm;
            for (msg, primary, label) in &leading {
                let clicked = if *primary {
                    primary_button(ui, th, label).clicked()
                } else {
                    button(ui, th, label).clicked()
                };
                if clicked {
                    effects.pending_msg = Some(*msg);
                }
            }
            let files = ui_label(5);
            let commits = ui_label(6);
            let patches = ui_label(26);
            let issues = ui_label(27);
            let jobs = ui_label(28);
            tab_btn(ui, th, state.tab == Tab::Files, &files, None, false, false, || {
                if state.tab != Tab::Files {
                    state.tab = Tab::Files;
                    state.prefs_dirty = true;
                }
            });
            tab_btn(ui, th, state.tab == Tab::Commits, &commits, None, false, false, || {
                if state.tab != Tab::Commits {
                    state.tab = Tab::Commits;
                    state.prefs_dirty = true;
                }
            });
            tab_btn(
                ui,
                th,
                state.tab == Tab::Patches,
                &patches,
                Some("Press again to reload"),
                cobs_loading,
                true,
                || {
                    if state.tab == Tab::Patches {
                        state.reload_requested = true;
                    } else {
                        state.tab = Tab::Patches;
                        state.prefs_dirty = true;
                    }
                },
            );
            tab_btn(
                ui,
                th,
                state.tab == Tab::Issues,
                &issues,
                Some("Press again to reload"),
                cobs_loading,
                true,
                || {
                    if state.tab == Tab::Issues {
                        state.reload_requested = true;
                    } else {
                        state.tab = Tab::Issues;
                        state.prefs_dirty = true;
                    }
                },
            );
            tab_btn(
                ui,
                th,
                state.tab == Tab::Jobs,
                &jobs,
                Some("Press again to reload"),
                cobs_loading,
                true,
                || {
                    if state.tab == Tab::Jobs {
                        state.reload_requested = true;
                    } else {
                        state.tab = Tab::Jobs;
                        state.prefs_dirty = true;
                    }
                },
            );
        });
        ui.add_space(th.spacing.md);

        match state.tab {
            Tab::Files => files_tab(ui, th, model, state, profile),
            Tab::Commits => commits_tab(ui, th, model, state, profile),
            Tab::Patches => patches_tab(ui, th, model, state, profile),
            Tab::Issues => issues_tab(ui, th, model, state),
            Tab::Jobs => jobs_tab(ui, th, model, state),
        }
    }
}

fn tab_btn(
    ui: &mut egui::Ui,
    th: &Theme,
    active: bool,
    label: &str,
    hover: Option<&str>,
    loading: bool,
    // Keep a spinner-sized trailing slot so load start/stop does not resize the button.
    reserve_spinner: bool,
    on: impl FnOnce(),
) {
    let response = if reserve_spinner {
        tab_btn_with_spinner_slot(ui, th, active, label, loading)
    } else if active {
        primary_button(ui, th, label)
    } else {
        button(ui, th, label)
    };
    let response = if loading {
        response.on_hover_text("Loading patches…")
    } else if let Some(tip) = hover.filter(|_| active) {
        response.on_hover_text(tip)
    } else {
        response
    };
    if response.clicked() && !loading {
        on();
    }
}

/// Patches tab: fixed chrome with optional spinner (no width FOUC when load toggles).
fn tab_btn_with_spinner_slot(
    ui: &mut egui::Ui,
    th: &Theme,
    active: bool,
    label: &str,
    loading: bool,
) -> egui::Response {
    let p = &th.palette;
    let (fg, bg, stroke) = if active {
        (p.accent_fg, p.accent, Stroke::NONE)
    } else {
        (p.button_fg, p.button_bg, Stroke::new(1.0_f32, p.border_soft))
    };

    let spinner_size = th.type_scale.body;
    let gap = th.spacing.xs;
    let h_pad = th.spacing.sm + 2.0;

    let galley = ui.fonts(|f| {
        f.layout_no_wrap(
            label.to_owned(),
            FontId::proportional(th.type_scale.body),
            fg,
        )
    });

    let height = th.spacing.control_height;
    let width = h_pad + galley.size().x + gap + spinner_size + h_pad;
    let (rect, mut response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());

    if ui.is_rect_visible(rect) {
        let fill = if !active && response.hovered() {
            p.button_hover
        } else if active && response.hovered() {
            p.accent_hover
        } else {
            bg
        };
        ui.painter().rect(
            rect,
            CornerRadius::same(th.spacing.radius_md as u8),
            fill,
            stroke,
            StrokeKind::Inside,
        );
        let text_pos = egui::pos2(
            rect.left() + h_pad,
            rect.center().y - galley.size().y * 0.5,
        );
        ui.painter().galley(text_pos, galley, fg);
        if loading {
            let spinner_rect = Rect::from_center_size(
                egui::pos2(
                    rect.right() - h_pad - spinner_size * 0.5,
                    rect.center().y,
                ),
                Vec2::splat(spinner_size),
            );
            egui::Spinner::new()
                .size(spinner_size)
                .color(fg)
                .paint_at(ui, spinner_rect);
        }
    }

    if let Some(cursor) = ui.visuals().interact_cursor {
        response = response.on_hover_cursor(cursor);
    }
    response
}

/// Left list pane tracks a fraction of available width (no fixed px cap).
fn list_pane_width(avail_w: f32, fraction: f32) -> f32 {
    (avail_w * fraction).max(1.0)
}

fn files_tab(
    ui: &mut egui::Ui,
    th: &Theme,
    _model: &ViewModel,
    state: &mut RepoUi,
    profile: Option<&Profile>,
) {
    let gap = th.spacing.lg;
    let avail_w = ui.available_width();
    let list_w = list_pane_width(avail_w, 0.32);

    if side_by_side(avail_w, 200.0, gap) {
        let rest = (avail_w - list_w - gap).max(1.0);
        ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(list_w, 1.0),
                Layout::top_down(Align::Min),
                |ui| {
                    ui.set_min_width(list_w);
                    ui.set_max_width(list_w);
                    card(ui, th, |ui| {
                        file_list(ui, th, state, profile);
                    });
                },
            );
            ui.add_space(gap);
            ui.allocate_ui_with_layout(
                Vec2::new(rest, 1.0),
                Layout::top_down(Align::Min),
                |ui| {
                    ui.set_min_width(rest);
                    ui.set_max_width(rest);
                    card(ui, th, |ui| {
                        file_content(ui, th, state);
                    });
                },
            );
        });
    } else {
        card(ui, th, |ui| {
            file_list(ui, th, state, profile);
        });
        ui.add_space(gap);
        card(ui, th, |ui| {
            file_content(ui, th, state);
        });
    }
}

fn commits_tab(
    ui: &mut egui::Ui,
    th: &Theme,
    model: &ViewModel,
    state: &mut RepoUi,
    profile: Option<&Profile>,
) {
    let gap = th.spacing.lg;
    let avail_w = ui.available_width();
    let min_col = 200.0;
    let selected = state.selected_commit.is_some();
    // 3-col: commit list | changed files | diff
    // 2-col (selected): changed files | diff — list collapses so files stay left
    // 2-col (idle): commit list | placeholder
    let two = side_by_side(avail_w, min_col, gap);
    let three = selected && avail_w >= min_col * 3.0 + gap * 2.0;

    let paths = state.commit_paths.clone();
    let selected_path = state.selected_diff_path.clone();

    if three {
        let list_w = list_pane_width(avail_w, 0.28);
        let files_w = list_pane_width(avail_w, 0.28);
        let rest = (avail_w - list_w - files_w - gap * 2.0).max(1.0);
        ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
            pane(ui, list_w, |ui| {
                card(ui, th, |ui| {
                    commit_list(ui, th, &model.commits, state, profile);
                });
            });
            ui.add_space(gap);
            pane(ui, files_w, |ui| {
                changed_files_card(ui, th, &paths, selected_path.as_deref(), |path| {
                    if let Some(profile) = profile {
                        state.select_diff_path(profile, path);
                    }
                });
            });
            ui.add_space(gap);
            pane(ui, rest, |ui| {
                commit_diff_pane(ui, th, state);
            });
        });
    } else if two && selected {
        // List collapses out of the column row so changed files stay left.
        if button(ui, th, "← Commits").clicked() {
            state.selected_commit = None;
            state.selected_diff_path = None;
            state.diff_text.clear();
            state.commit_paths.clear();
            state.commit_error = None;
            return;
        }
        ui.add_space(th.spacing.sm);
        let files_w = list_pane_width(avail_w, 0.32);
        let rest = (avail_w - files_w - gap).max(1.0);
        ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
            pane(ui, files_w, |ui| {
                changed_files_card(ui, th, &paths, selected_path.as_deref(), |path| {
                    if let Some(profile) = profile {
                        state.select_diff_path(profile, path);
                    }
                });
            });
            ui.add_space(gap);
            pane(ui, rest, |ui| {
                commit_diff_pane(ui, th, state);
            });
        });
    } else if two {
        let list_w = list_pane_width(avail_w, 0.34);
        let rest = (avail_w - list_w - gap).max(1.0);
        ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
            pane(ui, list_w, |ui| {
                card(ui, th, |ui| {
                    commit_list(ui, th, &model.commits, state, profile);
                });
            });
            ui.add_space(gap);
            pane(ui, rest, |ui| {
                commit_detail(ui, th, state, profile);
            });
        });
    } else {
        card(ui, th, |ui| {
            commit_list(ui, th, &model.commits, state, profile);
        });
        ui.add_space(gap);
        commit_detail(ui, th, state, profile);
    }
}

fn pane(ui: &mut egui::Ui, width: f32, content: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(
        Vec2::new(width, 1.0),
        Layout::top_down(Align::Min),
        |ui| {
            ui.set_min_width(width);
            ui.set_max_width(width);
            content(ui);
        },
    );
}

fn file_list(
    ui: &mut egui::Ui,
    th: &Theme,
    state: &mut RepoUi,
    profile: Option<&Profile>,
) {
    let title = if state.cwd.is_empty() {
        "Files".to_string()
    } else {
        format!("Files / {}", state.cwd)
    };
    title_2(ui, th, &title);
    ui.add_space(th.spacing.md);

    if state.dir_entries.is_empty() && state.cwd.is_empty() {
        dim_label(ui, th, "(empty tree)");
        return;
    }

    // Clone names so we can mutate state on click without borrow fights.
    let up = !state.cwd.is_empty();
    let entries: Vec<(String, bool)> = state
        .dir_entries
        .iter()
        .map(|f| (f.name.clone(), f.is_tree))
        .collect();

    if up {
        let response = selectable_row(ui, th, "../", false, true, true);
        if response.clicked() {
            if let Some(profile) = profile {
                state.go_up(profile);
            }
        }
    }
    for (name, is_tree) in entries {
        let full = state.join_cwd(&name);
        let selected = !is_tree && state.selected_file.as_deref() == Some(full.as_str());
        let label = if is_tree {
            format!("{name}/")
        } else {
            name.clone()
        };
        let response = selectable_row(ui, th, &label, selected, is_tree, true);
        if response.clicked() {
            if let Some(profile) = profile {
                if is_tree {
                    state.enter_dir(profile, &name);
                } else {
                    state.select_file(profile, &full);
                }
            }
        }
    }
}

fn file_content(ui: &mut egui::Ui, th: &Theme, state: &RepoUi) {
    match &state.selected_file {
        Some(path) => {
            title_2(ui, th, path);
            ui.add_space(th.spacing.md);
            if let Some(err) = &state.file_error {
                dim_label(ui, th, err);
                return;
            }
            ui.set_min_width(ui.available_width());
            if state.file_is_md {
                markdown::render(ui, th, &state.file_content);
            } else {
                code_block(ui, th, &state.file_content);
            }
        }
        None => dim_label(ui, th, "Select a file to view its contents."),
    }
}

fn commit_list(
    ui: &mut egui::Ui,
    th: &Theme,
    commits: &[CommitRow],
    state: &mut RepoUi,
    profile: Option<&Profile>,
) {
    title_2(ui, th, "Commits");
    ui.add_space(th.spacing.md);
    if commits.is_empty() {
        dim_label(ui, th, "(no commits)");
        return;
    }
    for c in commits {
        let selected = state.selected_commit.as_deref() == Some(c.id.as_str());
        let label = format!("{}  {}", c.short_id, c.summary);
        let response = selectable_row(ui, th, &label, selected, false, true);
        if response.clicked() {
            if let Some(profile) = profile {
                state.select_commit(profile, &c.id);
            }
        }
        dim_label(ui, th, &c.author);
        ui.add_space(th.spacing.sm);
    }
}

fn patches_tab(
    ui: &mut egui::Ui,
    th: &Theme,
    model: &ViewModel,
    state: &mut RepoUi,
    profile: Option<&Profile>,
) {
    let gap = th.spacing.lg;
    let avail_w = ui.available_width();
    let min_col = 200.0;
    let selected = state.selected_patch.is_some();
    // Same responsive rules as commits: when collapsed to two columns with a
    // selection, changed files occupy the left column (patch list collapses).
    let two = side_by_side(avail_w, min_col, gap);
    let three = selected && avail_w >= min_col * 3.0 + gap * 2.0;

    let paths = state.patch_paths.clone();
    let selected_path = state.selected_patch_diff_path.clone();

    if three {
        patch_meta(ui, th, &model.patches, state);
        ui.add_space(gap);
        let list_w = list_pane_width(avail_w, 0.28);
        let files_w = list_pane_width(avail_w, 0.28);
        let rest = (avail_w - list_w - files_w - gap * 2.0).max(1.0);
        ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
            pane(ui, list_w, |ui| {
                card(ui, th, |ui| {
                    patch_list(ui, th, &model.patches, state, profile);
                });
            });
            ui.add_space(gap);
            pane(ui, files_w, |ui| {
                changed_files_card(ui, th, &paths, selected_path.as_deref(), |path| {
                    if let Some(profile) = profile {
                        state.select_patch_diff_path(profile, path);
                    }
                });
            });
            ui.add_space(gap);
            pane(ui, rest, |ui| {
                patch_diff_pane(ui, th, state);
            });
        });
    } else if two && selected {
        if button(ui, th, "← Patches").clicked() {
            state.clear_patch_selection();
            return;
        }
        ui.add_space(th.spacing.sm);
        patch_meta(ui, th, &model.patches, state);
        ui.add_space(gap);
        // List collapses out of the column row so changed files stay left.
        let files_w = list_pane_width(avail_w, 0.32);
        let rest = (avail_w - files_w - gap).max(1.0);
        ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
            pane(ui, files_w, |ui| {
                changed_files_card(ui, th, &paths, selected_path.as_deref(), |path| {
                    if let Some(profile) = profile {
                        state.select_patch_diff_path(profile, path);
                    }
                });
            });
            ui.add_space(gap);
            pane(ui, rest, |ui| {
                patch_diff_pane(ui, th, state);
            });
        });
    } else if two {
        let list_w = list_pane_width(avail_w, 0.34);
        let rest = (avail_w - list_w - gap).max(1.0);
        ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
            pane(ui, list_w, |ui| {
                card(ui, th, |ui| {
                    patch_list(ui, th, &model.patches, state, profile);
                });
            });
            ui.add_space(gap);
            pane(ui, rest, |ui| {
                patch_detail(ui, th, &model.patches, state, profile);
            });
        });
    } else {
        card(ui, th, |ui| {
            patch_list(ui, th, &model.patches, state, profile);
        });
        ui.add_space(gap);
        patch_detail(ui, th, &model.patches, state, profile);
    }
}

fn issues_tab(ui: &mut egui::Ui, th: &Theme, model: &ViewModel, state: &mut RepoUi) {
    let gap = th.spacing.lg;
    let avail_w = ui.available_width();
    let list_w = list_pane_width(avail_w, 0.34);

    if side_by_side(avail_w, 220.0, gap) {
        let rest = (avail_w - list_w - gap).max(1.0);
        ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(list_w, 1.0),
                Layout::top_down(Align::Min),
                |ui| {
                    ui.set_min_width(list_w);
                    ui.set_max_width(list_w);
                    card(ui, th, |ui| {
                        issue_list(ui, th, &model.issues, state);
                    });
                },
            );
            ui.add_space(gap);
            ui.allocate_ui_with_layout(
                Vec2::new(rest, 1.0),
                Layout::top_down(Align::Min),
                |ui| {
                    ui.set_min_width(rest);
                    ui.set_max_width(rest);
                    card(ui, th, |ui| {
                        issue_detail(ui, th, &model.issues, state);
                    });
                },
            );
        });
    } else {
        card(ui, th, |ui| {
            issue_list(ui, th, &model.issues, state);
        });
        ui.add_space(gap);
        card(ui, th, |ui| {
            issue_detail(ui, th, &model.issues, state);
        });
    }
}

fn patch_list(
    ui: &mut egui::Ui,
    th: &Theme,
    patches: &[PatchRow],
    state: &mut RepoUi,
    profile: Option<&Profile>,
) {
    title_2(ui, th, "Patches");
    ui.add_space(th.spacing.sm);
    status_tabs(ui, th, PatchStatus::ALL, state.patch_status, |s| {
        if state.patch_status != s {
            state.patch_status = s;
            state.prefs_dirty = true;
            if let Some(id) = state.selected_patch.as_deref() {
                let keep = patches
                    .iter()
                    .any(|p| p.id == id && s.matches(&p.state));
                if !keep {
                    state.clear_patch_selection();
                }
            }
        }
    });
    ui.add_space(th.spacing.sm);
    search_field(ui, th, &mut state.patch_filter, "Search title, id, author…");
    ui.add_space(th.spacing.sm);

    if patches.is_empty() {
        dim_label(ui, th, "(no patches)");
        return;
    }

    let status = state.patch_status;
    let q = state.patch_filter.trim().to_lowercase();
    let in_status = patches.iter().filter(|p| status.matches(&p.state)).count();
    let filtered: Vec<&PatchRow> = patches
        .iter()
        .filter(|p| status.matches(&p.state) && patch_matches(p, &q))
        .collect();

    dim_label(
        ui,
        th,
        &format!("{} of {} {}", filtered.len(), in_status, status.label().to_lowercase()),
    );
    ui.add_space(th.spacing.xs);
    if filtered.is_empty() {
        dim_label(
            ui,
            th,
            if q.is_empty() {
                "(none)"
            } else {
                "No patches match this search."
            },
        );
        return;
    }

    // Clone rows so we can mutate selection without borrow fights.
    let show_state = status == PatchStatus::All;
    let rows: Vec<PatchRow> = filtered.into_iter().cloned().collect();

    for p in rows {
        let selected = state.selected_patch.as_deref() == Some(p.id.as_str());
        let label = if show_state {
            format!("[{}] {}", p.state, p.title)
        } else {
            p.title.clone()
        };
        let meta = format!("{} · {}", p.short_id, p.author);
        let response = cob_list_row(ui, th, &p.id, &label, &meta, selected);
        if response.clicked() {
            if let Some(profile) = profile {
                state.select_patch(profile, &p);
            } else {
                state.selected_patch = Some(p.id);
            }
        }
    }
}

fn patch_detail(
    ui: &mut egui::Ui,
    th: &Theme,
    patches: &[PatchRow],
    state: &mut RepoUi,
    profile: Option<&Profile>,
) {
    if state.selected_patch.is_none() {
        card(ui, th, |ui| {
            dim_label(ui, th, "Select a patch to view its details.");
        });
        return;
    }

    let gap = th.spacing.md;
    // Stacked / single-column: description, then changed files, then diff.
    patch_meta(ui, th, patches, state);
    ui.add_space(gap);

    if let Some(err) = &state.patch_error {
        card(ui, th, |ui| {
            dim_label(ui, th, err);
        });
        ui.add_space(gap);
    }

    let paths = state.patch_paths.clone();
    let selected = state.selected_patch_diff_path.clone();
    let diff_text = state.patch_diff_text.clone();
    changed_files_diff_panes(ui, th, &paths, selected.as_deref(), &diff_text, |path| {
        if let Some(profile) = profile {
            state.select_patch_diff_path(profile, path);
        }
    });
}

/// Patch title / meta / description card (full width).
fn patch_meta(ui: &mut egui::Ui, th: &Theme, patches: &[PatchRow], state: &RepoUi) {
    let Some(id) = state.selected_patch.as_deref() else {
        return;
    };
    let Some(p) = patches.iter().find(|p| p.id == id) else {
        card(ui, th, |ui| {
            dim_label(ui, th, "Patch not found in snapshot.");
        });
        return;
    };

    card(ui, th, |ui| {
        title_2(ui, th, &p.title);
        ui.add_space(th.spacing.sm);
        dim_label(
            ui,
            th,
            &format!(
                "{} · {} · {} · {} revision{}",
                p.state,
                p.short_id,
                p.author,
                p.revisions,
                if p.revisions == 1 { "" } else { "s" }
            ),
        );
        ui.add_space(th.spacing.sm);
        dim_label(
            ui,
            th,
            &format!(
                "base {} → head {}",
                short_oid_display(&p.base),
                short_oid_display(&p.head)
            ),
        );
        ui.add_space(th.spacing.md);
        ui.set_min_width(ui.available_width());
        if p.description.is_empty() {
            dim_label(ui, th, "(no description)");
        } else if looks_like_md(&p.description) {
            markdown::render(ui, th, &p.description);
        } else {
            linkify::body(ui, th, &p.description);
        }
    });
}

fn patch_diff_pane(ui: &mut egui::Ui, th: &Theme, state: &RepoUi) {
    if let Some(err) = &state.patch_error {
        card(ui, th, |ui| {
            dim_label(ui, th, err);
        });
        ui.add_space(th.spacing.md);
    }
    diff_card(
        ui,
        th,
        state.selected_patch_diff_path.as_deref(),
        &state.patch_diff_text,
    );
}

fn issue_list(ui: &mut egui::Ui, th: &Theme, issues: &[IssueRow], state: &mut RepoUi) {
    title_2(ui, th, "Issues");
    ui.add_space(th.spacing.sm);
    status_tabs(ui, th, IssueStatus::ALL, state.issue_status, |s| {
        if state.issue_status != s {
            state.issue_status = s;
            state.prefs_dirty = true;
            if let Some(id) = state.selected_issue.as_deref() {
                let keep = issues
                    .iter()
                    .any(|i| i.id == id && s.matches(&i.state));
                if !keep {
                    state.selected_issue = None;
                }
            }
        }
    });
    ui.add_space(th.spacing.sm);
    search_field(ui, th, &mut state.issue_filter, "Search title, id, author…");
    ui.add_space(th.spacing.sm);

    if issues.is_empty() {
        dim_label(ui, th, "(no issues)");
        return;
    }

    let status = state.issue_status;
    let q = state.issue_filter.trim().to_lowercase();
    let in_status = issues.iter().filter(|i| status.matches(&i.state)).count();
    let filtered: Vec<&IssueRow> = issues
        .iter()
        .filter(|i| status.matches(&i.state) && issue_matches(i, &q))
        .collect();

    dim_label(
        ui,
        th,
        &format!("{} of {} {}", filtered.len(), in_status, status.label().to_lowercase()),
    );
    ui.add_space(th.spacing.xs);
    if filtered.is_empty() {
        dim_label(
            ui,
            th,
            if q.is_empty() {
                "(none)"
            } else {
                "No issues match this search."
            },
        );
        return;
    }

    let show_state = status == IssueStatus::All;
    let rows: Vec<(String, String, String, String)> = filtered
        .iter()
        .map(|issue| {
            let replies = if issue.replies == 0 {
                "no replies".to_string()
            } else {
                format!(
                    "{} repl{}",
                    issue.replies,
                    if issue.replies == 1 { "y" } else { "ies" }
                )
            };
            (
                issue.id.clone(),
                issue.state.clone(),
                issue.title.clone(),
                format!("{} · {} · {}", issue.short_id, issue.author, replies),
            )
        })
        .collect();

    for (id, state_label, title, meta) in rows {
        let selected = state.selected_issue.as_deref() == Some(id.as_str());
        let label = if show_state {
            format!("[{state_label}] {title}")
        } else {
            title
        };
        let response = cob_list_row(ui, th, &id, &label, &meta, selected);
        if response.clicked() {
            state.selected_issue = Some(id);
        }
    }
}

fn issue_detail(ui: &mut egui::Ui, th: &Theme, issues: &[IssueRow], state: &RepoUi) {
    let Some(id) = state.selected_issue.as_deref() else {
        dim_label(ui, th, "Select an issue to view its details.");
        return;
    };
    let Some(issue) = issues.iter().find(|i| i.id == id) else {
        dim_label(ui, th, "Issue not found in snapshot.");
        return;
    };

    title_2(ui, th, &issue.title);
    ui.add_space(th.spacing.sm);
    dim_label(
        ui,
        th,
        &format!("{} · {} · {}", issue.state, issue.short_id, issue.author),
    );
    ui.add_space(th.spacing.md);
    ui.set_min_width(ui.available_width());
    if issue.description.is_empty() {
        dim_label(ui, th, "(no description)");
    } else if looks_like_md(&issue.description) {
        markdown::render(ui, th, &issue.description);
    } else {
        linkify::body(ui, th, &issue.description);
    }
    if issue.replies > 0 {
        ui.add_space(th.spacing.lg);
        dim_label(
            ui,
            th,
            &format!(
                "{} repl{} in thread",
                issue.replies,
                if issue.replies == 1 { "y" } else { "ies" }
            ),
        );
    }
}

fn jobs_tab(ui: &mut egui::Ui, th: &Theme, model: &ViewModel, state: &mut RepoUi) {
    let gap = th.spacing.lg;
    let avail_w = ui.available_width();
    let list_w = list_pane_width(avail_w, 0.34);

    if side_by_side(avail_w, 220.0, gap) {
        let rest = (avail_w - list_w - gap).max(1.0);
        ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(list_w, 1.0),
                Layout::top_down(Align::Min),
                |ui| {
                    ui.set_min_width(list_w);
                    ui.set_max_width(list_w);
                    card(ui, th, |ui| {
                        job_list(ui, th, &model.jobs, state);
                    });
                },
            );
            ui.add_space(gap);
            ui.allocate_ui_with_layout(
                Vec2::new(rest, 1.0),
                Layout::top_down(Align::Min),
                |ui| {
                    ui.set_min_width(rest);
                    ui.set_max_width(rest);
                    card(ui, th, |ui| {
                        job_detail(ui, th, &model.jobs, state);
                    });
                },
            );
        });
    } else {
        card(ui, th, |ui| {
            job_list(ui, th, &model.jobs, state);
        });
        ui.add_space(gap);
        card(ui, th, |ui| {
            job_detail(ui, th, &model.jobs, state);
        });
    }
}

fn job_list(ui: &mut egui::Ui, th: &Theme, jobs: &[JobRow], state: &mut RepoUi) {
    title_2(ui, th, "Jobs");
    ui.add_space(th.spacing.sm);
    status_tabs(ui, th, JobStatus::ALL, state.job_status, |s| {
        if state.job_status != s {
            state.job_status = s;
            state.prefs_dirty = true;
            if let Some(id) = state.selected_job.as_deref() {
                let keep = jobs.iter().any(|j| j.id == id && s.matches(&j.status));
                if !keep {
                    state.selected_job = None;
                }
            }
        }
    });
    ui.add_space(th.spacing.md);
    if jobs.is_empty() {
        dim_label(ui, th, "(no job COBs)");
        return;
    }

    let status = state.job_status;
    let filtered: Vec<&JobRow> = jobs
        .iter()
        .filter(|j| status.matches(&j.status))
        .collect();
    dim_label(
        ui,
        th,
        &format!("{} of {} {}", filtered.len(), jobs.len(), status.label().to_lowercase()),
    );
    ui.add_space(th.spacing.xs);
    if filtered.is_empty() {
        dim_label(ui, th, "(none)");
        return;
    }

    let show_status = status == JobStatus::All;
    let rows: Vec<(String, String, String, String)> = filtered
        .iter()
        .map(|job| {
            (
                job.id.clone(),
                job.status.clone(),
                job.short_id.clone(),
                format!(
                    "commit {} · {} run{} · {} node{}",
                    job.short_commit,
                    job.run_count,
                    if job.run_count == 1 { "" } else { "s" },
                    job.node_count,
                    if job.node_count == 1 { "" } else { "s" }
                ),
            )
        })
        .collect();

    for (id, status_label, short_id, meta) in rows {
        let selected = state.selected_job.as_deref() == Some(id.as_str());
        let label = if show_status {
            format!("[{status_label}] {short_id}")
        } else {
            short_id
        };
        let response = cob_list_row(ui, th, &id, &label, &meta, selected);
        if response.clicked() {
            state.selected_job = Some(id);
        }
    }
}

fn status_tabs<S: Copy + PartialEq>(
    ui: &mut egui::Ui,
    th: &Theme,
    statuses: &[S],
    active: S,
    mut on_select: impl FnMut(S),
) where
    S: StatusTab,
{
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = th.spacing.sm;
        for &s in statuses {
            tab_btn(ui, th, active == s, s.label(), None, false, false, || on_select(s));
        }
    });
}

trait StatusTab {
    fn label(self) -> &'static str;
}

impl StatusTab for PatchStatus {
    fn label(self) -> &'static str {
        PatchStatus::label(self)
    }
}

impl StatusTab for IssueStatus {
    fn label(self) -> &'static str {
        IssueStatus::label(self)
    }
}

impl StatusTab for JobStatus {
    fn label(self) -> &'static str {
        JobStatus::label(self)
    }
}

fn job_detail(ui: &mut egui::Ui, th: &Theme, jobs: &[JobRow], state: &RepoUi) {
    let Some(id) = state.selected_job.as_deref() else {
        dim_label(ui, th, "Select a job to view its runs.");
        return;
    };
    let Some(job) = jobs.iter().find(|j| j.id == id) else {
        dim_label(ui, th, "Job not found in snapshot.");
        return;
    };

    title_2(ui, th, &format!("Job {}", job.short_id));
    ui.add_space(th.spacing.sm);
    dim_label(
        ui,
        th,
        &format!(
            "{} · commit {} · {} run{} across {} node{}",
            job.status,
            job.short_commit,
            job.run_count,
            if job.run_count == 1 { "" } else { "s" },
            job.node_count,
            if job.node_count == 1 { "" } else { "s" }
        ),
    );
    ui.add_space(th.spacing.sm);
    dim_label(ui, th, &format!("id {}", job.id));
    ui.add_space(th.spacing.sm);
    dim_label(ui, th, &format!("commit {}", job.commit));
    ui.add_space(th.spacing.md);

    ui.set_min_width(ui.available_width());
    if job.runs.is_empty() {
        dim_label(ui, th, "(no runs yet)");
        return;
    }
    for run in &job.runs {
        body(
            ui,
            th,
            &format!(
                "[{}] {} · {}",
                run.status,
                run.node,
                short_uuid(&run.run_id)
            ),
        );
        // Log is a typed URL on the job COB — always render it as a real link,
        // on its own line so wrapping/hit-testing stay reliable in narrow panes.
        dim_label(ui, th, "log");
        let log = run.log.trim();
        if linkify::is_url(log) {
            linkify::link(ui, th, log);
        } else {
            linkify::dim(ui, th, log);
        }
        dim_label(ui, th, &format!("ts {}", run.timestamp_secs));
        ui.add_space(th.spacing.md);
    }
}

fn short_uuid(id: &str) -> &str {
    if id.len() > 8 {
        &id[..8]
    } else {
        id
    }
}

fn short_oid_display(oid: &str) -> &str {
    if oid.len() > 7 { &oid[..7] } else { oid }
}

fn looks_like_md(text: &str) -> bool {
    text.contains('\n')
        || text.contains("```")
        || text.contains("**")
        || text.starts_with('#')
        || text.contains("](")
}

fn patch_matches(p: &PatchRow, q: &str) -> bool {
    if q.is_empty() {
        return true;
    }
    p.title.to_lowercase().contains(q)
        || p.id.to_lowercase().contains(q)
        || p.short_id.to_lowercase().contains(q)
        || p.author.to_lowercase().contains(q)
        || p.state.to_lowercase().contains(q)
        || p.description.to_lowercase().contains(q)
        || p.head.to_lowercase().contains(q)
        || p.base.to_lowercase().contains(q)
}

fn issue_matches(i: &IssueRow, q: &str) -> bool {
    if q.is_empty() {
        return true;
    }
    i.title.to_lowercase().contains(q)
        || i.id.to_lowercase().contains(q)
        || i.short_id.to_lowercase().contains(q)
        || i.author.to_lowercase().contains(q)
        || i.state.to_lowercase().contains(q)
        || i.description.to_lowercase().contains(q)
}

/// Framed single-line search field matching the RID / repo-list inputs.
fn search_field(ui: &mut egui::Ui, th: &Theme, text: &mut String, hint: &str) {
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
                    .hint_text(hint),
            );
        },
    );
}

fn commit_detail(ui: &mut egui::Ui, th: &Theme, state: &mut RepoUi, profile: Option<&Profile>) {
    if state.selected_commit.is_none() {
        card(ui, th, |ui| {
            dim_label(ui, th, "Select a commit to inspect its diff.");
        });
        return;
    }

    // Stacked path: keep files | diff side-by-side when the detail pane is wide
    // enough; otherwise files above diff.
    let paths = state.commit_paths.clone();
    let selected = state.selected_diff_path.clone();
    let diff_text = state.diff_text.clone();
    if let Some(err) = &state.commit_error {
        card(ui, th, |ui| {
            dim_label(ui, th, err);
        });
        ui.add_space(th.spacing.md);
    }
    changed_files_diff_panes(ui, th, &paths, selected.as_deref(), &diff_text, |path| {
        if let Some(profile) = profile {
            state.select_diff_path(profile, path);
        }
    });
}

fn commit_diff_pane(ui: &mut egui::Ui, th: &Theme, state: &RepoUi) {
    if let Some(err) = &state.commit_error {
        card(ui, th, |ui| {
            dim_label(ui, th, err);
        });
        ui.add_space(th.spacing.md);
    }
    diff_card(ui, th, state.selected_diff_path.as_deref(), &state.diff_text);
}

fn changed_files_card(
    ui: &mut egui::Ui,
    th: &Theme,
    paths: &[String],
    selected_path: Option<&str>,
    mut on_select: impl FnMut(&str),
) {
    card(ui, th, |ui| {
        title_2(ui, th, "Changed files");
        ui.add_space(th.spacing.sm);
        if paths.is_empty() {
            dim_label(ui, th, "(no file changes)");
        } else {
            ui.set_min_width(ui.available_width());
            for path in paths {
                let selected = selected_path == Some(path.as_str());
                let response = selectable_row(ui, th, path, selected, false, true);
                if response.clicked() {
                    on_select(path);
                }
            }
        }
    });
}

fn diff_card(ui: &mut egui::Ui, th: &Theme, selected_path: Option<&str>, diff_text: &str) {
    card(ui, th, |ui| {
        let title = selected_path.unwrap_or("Diff");
        title_2(ui, th, title);
        ui.add_space(th.spacing.md);
        if diff_text.is_empty() {
            dim_label(ui, th, "Select a changed file to view the patch.");
        } else {
            ui.set_min_width(ui.available_width());
            diff_widget(ui, th, diff_text);
        }
    });
}

/// Left: changed-file list. Right: selected file diff. Stacks when narrow.
/// Used on the single-column / stacked detail path.
fn changed_files_diff_panes(
    ui: &mut egui::Ui,
    th: &Theme,
    paths: &[String],
    selected_path: Option<&str>,
    diff_text: &str,
    mut on_select: impl FnMut(&str),
) {
    let gap = th.spacing.lg;
    let avail_w = ui.available_width();
    let list_w = list_pane_width(avail_w, 0.32);

    if side_by_side(avail_w, 200.0, gap) {
        let rest = (avail_w - list_w - gap).max(1.0);
        ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
            pane(ui, list_w, |ui| {
                changed_files_card(ui, th, paths, selected_path, &mut on_select);
            });
            ui.add_space(gap);
            pane(ui, rest, |ui| {
                diff_card(ui, th, selected_path, diff_text);
            });
        });
    } else {
        changed_files_card(ui, th, paths, selected_path, &mut on_select);
        ui.add_space(gap);
        diff_card(ui, th, selected_path, diff_text);
    }
}

fn selectable_row(
    ui: &mut egui::Ui,
    th: &Theme,
    label: &str,
    selected: bool,
    dim: bool,
    clickable: bool,
) -> egui::Response {
    let color = if selected {
        th.palette.accent
    } else if dim {
        th.palette.text_secondary
    } else {
        th.palette.text
    };
    let text = RichText::new(label)
        .size(th.type_scale.body)
        .color(color);
    let sense = if clickable {
        Sense::click()
    } else {
        Sense::hover()
    };
    let mut response = ui.add(egui::Label::new(text).sense(sense).wrap());
    if clickable {
        response = response.on_hover_cursor(CursorIcon::PointingHand);
    }
    if selected {
        let rect = response.rect.expand2(Vec2::new(4.0, 2.0));
        let a = th.palette.accent;
        ui.painter().rect_filled(
            rect,
            th.spacing.radius_sm,
            egui::Color32::from_rgba_unmultiplied(a.r(), a.g(), a.b(), 36),
        );
    }
    response
}

/// Title + meta as one full-width click target (not just the title glyph bounds).
fn cob_list_row(
    ui: &mut egui::Ui,
    th: &Theme,
    id: &str,
    title: &str,
    meta: &str,
    selected: bool,
) -> egui::Response {
    let interact_id = ui.id().with("cob_row").with(id);
    let top = ui.cursor().top();

    let color = if selected {
        th.palette.accent
    } else {
        th.palette.text
    };
    ui.add(
        egui::Label::new(
            RichText::new(title)
                .size(th.type_scale.body)
                .color(color),
        )
        .wrap(),
    );
    dim_label(ui, th, meta);

    let bottom = ui.cursor().top();
    let hit = Rect::from_min_max(
        Pos2::new(ui.max_rect().left(), top),
        Pos2::new(ui.max_rect().right(), bottom),
    );
    let response = ui
        .interact(hit, interact_id, Sense::click())
        .on_hover_cursor(CursorIcon::PointingHand);

    if selected || response.hovered() {
        let a = th.palette.accent;
        let alpha = if selected { 36 } else { 28 };
        ui.painter().rect_filled(
            hit.expand2(Vec2::new(0.0, 2.0)),
            th.spacing.radius_sm,
            egui::Color32::from_rgba_unmultiplied(a.r(), a.g(), a.b(), alpha),
        );
    }

    ui.add_space(th.spacing.sm);
    response
}

fn code_block(ui: &mut egui::Ui, th: &Theme, text: &str) {
    ui.add(
        egui::Label::new(
            RichText::new(text)
                .font(egui::FontId::new(th.type_scale.body, FontFamily::Monospace))
                .color(th.palette.text),
        )
        .wrap(),
    );
}

fn diff_widget(ui: &mut egui::Ui, th: &Theme, patch: &str) {
    for line in patch.lines() {
        let (color, show) = if line.starts_with('+') && !line.starts_with("+++") {
            (th.palette.success, line)
        } else if line.starts_with('-') && !line.starts_with("---") {
            (th.palette.destructive, line)
        } else if line.starts_with("@@") {
            (th.palette.accent, line)
        } else {
            (th.palette.text_secondary, line)
        };
        ui.add(
            egui::Label::new(
                RichText::new(show)
                    .font(egui::FontId::new(
                        th.type_scale.caption.max(12.0),
                        FontFamily::Monospace,
                    ))
                    .color(color),
            )
            .wrap(),
        );
    }
}
