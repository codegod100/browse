//! Interactive Files / Commits / Patches / Issues / Jobs browser (host-owned selection).

use eframe::egui::{
    self, Align, CursorIcon, FontFamily, Layout, Margin, RichText, Sense, Stroke, StrokeKind,
    UiBuilder, Vec2,
};
use radicle::Profile;
use vidya::{body, button, card, dim_label, primary_button, side_by_side, title_2, Theme};

use crate::markdown;
use crate::rad;
use crate::view_api::{CommitRow, FileRow, IssueRow, JobRow, PatchRow, ViewModel};

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
                self.selected_patch = None;
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
}

pub struct RepoBrowser;

impl RepoBrowser {
    pub fn show(
        ui: &mut egui::Ui,
        th: &Theme,
        model: &ViewModel,
        state: &mut RepoUi,
        profile: Option<&Profile>,
    ) {
        ui.horizontal(|ui| {
            tab_btn(ui, th, state.tab == Tab::Files, "Files", None, || {
                state.tab = Tab::Files;
            });
            ui.add_space(th.spacing.sm);
            tab_btn(ui, th, state.tab == Tab::Commits, "Commits", None, || {
                state.tab = Tab::Commits;
            });
            ui.add_space(th.spacing.sm);
            tab_btn(
                ui,
                th,
                state.tab == Tab::Patches,
                "Patches",
                Some("Press again to reload"),
                || {
                    if state.tab == Tab::Patches {
                        state.reload_requested = true;
                    } else {
                        state.tab = Tab::Patches;
                    }
                },
            );
            ui.add_space(th.spacing.sm);
            tab_btn(
                ui,
                th,
                state.tab == Tab::Issues,
                "Issues",
                Some("Press again to reload"),
                || {
                    if state.tab == Tab::Issues {
                        state.reload_requested = true;
                    } else {
                        state.tab = Tab::Issues;
                    }
                },
            );
            ui.add_space(th.spacing.sm);
            tab_btn(
                ui,
                th,
                state.tab == Tab::Jobs,
                "Jobs",
                Some("Press again to reload"),
                || {
                    if state.tab == Tab::Jobs {
                        state.reload_requested = true;
                    } else {
                        state.tab = Tab::Jobs;
                    }
                },
            );
        });
        ui.add_space(th.spacing.md);

        match state.tab {
            Tab::Files => files_tab(ui, th, model, state, profile),
            Tab::Commits => commits_tab(ui, th, model, state, profile),
            Tab::Patches => patches_tab(ui, th, model, state),
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
    on: impl FnOnce(),
) {
    let response = if active {
        primary_button(ui, th, label)
    } else {
        button(ui, th, label)
    };
    let response = if let Some(tip) = hover.filter(|_| active) {
        response.on_hover_text(tip)
    } else {
        response
    };
    if response.clicked() {
        on();
    }
}

fn remaining_height(ui: &egui::Ui) -> f32 {
    // Prefer the visible clip region so panes fill the central panel residual.
    (ui.clip_rect().bottom() - ui.cursor().top() - 12.0).max(160.0)
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
    let avail_h = remaining_height(ui);
    let list_w = (avail_w * 0.32).clamp(180.0, 280.0);

    if side_by_side(avail_w, 200.0, gap) {
        ui.allocate_ui_with_layout(
            Vec2::new(avail_w, avail_h),
            Layout::left_to_right(Align::Min),
            |ui| {
                ui.set_min_size(Vec2::new(avail_w, avail_h));
                ui.set_max_size(Vec2::new(avail_w, avail_h));
                pane(ui, list_w, avail_h, |ui| {
                    card(ui, th, |ui| {
                        file_list(ui, th, state, profile);
                    });
                });
                ui.add_space(gap);
                let rest = (avail_w - list_w - gap).max(1.0);
                pane(ui, rest, avail_h, |ui| {
                    card(ui, th, |ui| {
                        file_content(ui, th, state);
                    });
                });
            },
        );
    } else {
        let half = ((avail_h - gap) * 0.4).max(120.0);
        ui.set_min_height(avail_h);
        card(ui, th, |ui| {
            ui.set_min_height(half);
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
    let avail_h = remaining_height(ui);
    let list_w = (avail_w * 0.34).clamp(220.0, 320.0);

    if side_by_side(avail_w, 220.0, gap) {
        ui.allocate_ui_with_layout(
            Vec2::new(avail_w, avail_h),
            Layout::left_to_right(Align::Min),
            |ui| {
                ui.set_min_size(Vec2::new(avail_w, avail_h));
                ui.set_max_size(Vec2::new(avail_w, avail_h));
                pane(ui, list_w, avail_h, |ui| {
                    card(ui, th, |ui| {
                        commit_list(ui, th, &model.commits, state, profile);
                    });
                });
                ui.add_space(gap);
                let rest = (avail_w - list_w - gap).max(1.0);
                pane(ui, rest, avail_h, |ui| {
                    commit_detail(ui, th, state, profile);
                });
            },
        );
    } else {
        let half = ((avail_h - gap) * 0.4).max(120.0);
        ui.set_min_height(avail_h);
        card(ui, th, |ui| {
            ui.set_min_height(half);
            commit_list(ui, th, &model.commits, state, profile);
        });
        ui.add_space(gap);
        commit_detail(ui, th, state, profile);
    }
}

fn pane(ui: &mut egui::Ui, width: f32, height: f32, add: impl FnOnce(&mut egui::Ui)) {
    let w = width.max(1.0);
    let h = height.max(1.0);
    ui.allocate_ui_with_layout(Vec2::new(w, h), Layout::top_down(Align::Min), |ui| {
        ui.set_min_size(Vec2::new(w, h));
        ui.set_max_size(Vec2::new(w, h));
        add(ui);
    });
}

fn fill_scroll(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash,
    both: bool,
    add: impl FnOnce(&mut egui::Ui),
) {
    let h = ui.available_height().max(80.0);
    let mut area = if both {
        egui::ScrollArea::both()
    } else {
        egui::ScrollArea::vertical()
    };
    area = area.id_salt(id).max_height(h).auto_shrink([false, false]);
    area.show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        add(ui);
    });
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

    fill_scroll(ui, "tab_files", false, |ui| {
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
    });
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
            fill_scroll(ui, "file_content", true, |ui| {
                if state.file_is_md {
                    markdown::render(ui, th, &state.file_content);
                } else {
                    code_block(ui, th, &state.file_content);
                }
            });
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
    fill_scroll(ui, "tab_commits", false, |ui| {
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
    });
}

fn patches_tab(ui: &mut egui::Ui, th: &Theme, model: &ViewModel, state: &mut RepoUi) {
    let gap = th.spacing.lg;
    let avail_w = ui.available_width();
    let avail_h = remaining_height(ui);
    let list_w = (avail_w * 0.34).clamp(220.0, 320.0);

    if side_by_side(avail_w, 220.0, gap) {
        ui.allocate_ui_with_layout(
            Vec2::new(avail_w, avail_h),
            Layout::left_to_right(Align::Min),
            |ui| {
                ui.set_min_size(Vec2::new(avail_w, avail_h));
                ui.set_max_size(Vec2::new(avail_w, avail_h));
                pane(ui, list_w, avail_h, |ui| {
                    card(ui, th, |ui| {
                        patch_list(ui, th, &model.patches, state);
                    });
                });
                ui.add_space(gap);
                let rest = (avail_w - list_w - gap).max(1.0);
                pane(ui, rest, avail_h, |ui| {
                    card(ui, th, |ui| {
                        patch_detail(ui, th, &model.patches, state);
                    });
                });
            },
        );
    } else {
        let half = ((avail_h - gap) * 0.4).max(120.0);
        ui.set_min_height(avail_h);
        card(ui, th, |ui| {
            ui.set_min_height(half);
            patch_list(ui, th, &model.patches, state);
        });
        ui.add_space(gap);
        card(ui, th, |ui| {
            patch_detail(ui, th, &model.patches, state);
        });
    }
}

fn issues_tab(ui: &mut egui::Ui, th: &Theme, model: &ViewModel, state: &mut RepoUi) {
    let gap = th.spacing.lg;
    let avail_w = ui.available_width();
    let avail_h = remaining_height(ui);
    let list_w = (avail_w * 0.34).clamp(220.0, 320.0);

    if side_by_side(avail_w, 220.0, gap) {
        ui.allocate_ui_with_layout(
            Vec2::new(avail_w, avail_h),
            Layout::left_to_right(Align::Min),
            |ui| {
                ui.set_min_size(Vec2::new(avail_w, avail_h));
                ui.set_max_size(Vec2::new(avail_w, avail_h));
                pane(ui, list_w, avail_h, |ui| {
                    card(ui, th, |ui| {
                        issue_list(ui, th, &model.issues, state);
                    });
                });
                ui.add_space(gap);
                let rest = (avail_w - list_w - gap).max(1.0);
                pane(ui, rest, avail_h, |ui| {
                    card(ui, th, |ui| {
                        issue_detail(ui, th, &model.issues, state);
                    });
                });
            },
        );
    } else {
        let half = ((avail_h - gap) * 0.4).max(120.0);
        ui.set_min_height(avail_h);
        card(ui, th, |ui| {
            ui.set_min_height(half);
            issue_list(ui, th, &model.issues, state);
        });
        ui.add_space(gap);
        card(ui, th, |ui| {
            issue_detail(ui, th, &model.issues, state);
        });
    }
}

fn patch_list(ui: &mut egui::Ui, th: &Theme, patches: &[PatchRow], state: &mut RepoUi) {
    title_2(ui, th, "Patches");
    ui.add_space(th.spacing.sm);
    status_tabs(ui, th, PatchStatus::ALL, state.patch_status, |s| {
        if state.patch_status != s {
            state.patch_status = s;
            if let Some(id) = state.selected_patch.as_deref() {
                let keep = patches
                    .iter()
                    .any(|p| p.id == id && s.matches(&p.state));
                if !keep {
                    state.selected_patch = None;
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

    // Clone ids so we can mutate selection without borrow fights.
    let show_state = status == PatchStatus::All;
    let rows: Vec<(String, String, String, String)> = filtered
        .iter()
        .map(|p| {
            (
                p.id.clone(),
                p.state.clone(),
                p.title.clone(),
                format!("{} · {}", p.short_id, p.author),
            )
        })
        .collect();

    fill_scroll(ui, "tab_patches", false, |ui| {
        for (id, state_label, title, meta) in rows {
            let selected = state.selected_patch.as_deref() == Some(id.as_str());
            let label = if show_state {
                format!("[{state_label}] {title}")
            } else {
                title
            };
            let response = selectable_row(ui, th, &label, selected, false, true);
            if response.clicked() {
                state.selected_patch = Some(id);
            }
            dim_label(ui, th, &meta);
            ui.add_space(th.spacing.sm);
        }
    });
}

fn patch_detail(ui: &mut egui::Ui, th: &Theme, patches: &[PatchRow], state: &RepoUi) {
    let Some(id) = state.selected_patch.as_deref() else {
        dim_label(ui, th, "Select a patch to view its details.");
        return;
    };
    let Some(p) = patches.iter().find(|p| p.id == id) else {
        dim_label(ui, th, "Patch not found in snapshot.");
        return;
    };

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
    fill_scroll(ui, "patch_detail", true, |ui| {
        if p.description.is_empty() {
            dim_label(ui, th, "(no description)");
        } else if looks_like_md(&p.description) {
            markdown::render(ui, th, &p.description);
        } else {
            body(ui, th, &p.description);
        }
    });
}

fn issue_list(ui: &mut egui::Ui, th: &Theme, issues: &[IssueRow], state: &mut RepoUi) {
    title_2(ui, th, "Issues");
    ui.add_space(th.spacing.sm);
    status_tabs(ui, th, IssueStatus::ALL, state.issue_status, |s| {
        if state.issue_status != s {
            state.issue_status = s;
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

    fill_scroll(ui, "tab_issues", false, |ui| {
        for (id, state_label, title, meta) in rows {
            let selected = state.selected_issue.as_deref() == Some(id.as_str());
            let label = if show_state {
                format!("[{state_label}] {title}")
            } else {
                title
            };
            let response = selectable_row(ui, th, &label, selected, false, true);
            if response.clicked() {
                state.selected_issue = Some(id);
            }
            dim_label(ui, th, &meta);
            ui.add_space(th.spacing.sm);
        }
    });
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
    fill_scroll(ui, "issue_detail", true, |ui| {
        if issue.description.is_empty() {
            dim_label(ui, th, "(no description)");
        } else if looks_like_md(&issue.description) {
            markdown::render(ui, th, &issue.description);
        } else {
            body(ui, th, &issue.description);
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
    });
}

fn jobs_tab(ui: &mut egui::Ui, th: &Theme, model: &ViewModel, state: &mut RepoUi) {
    let gap = th.spacing.lg;
    let avail_w = ui.available_width();
    let avail_h = remaining_height(ui);
    let list_w = (avail_w * 0.34).clamp(220.0, 320.0);

    if side_by_side(avail_w, 220.0, gap) {
        ui.allocate_ui_with_layout(
            Vec2::new(avail_w, avail_h),
            Layout::left_to_right(Align::Min),
            |ui| {
                ui.set_min_size(Vec2::new(avail_w, avail_h));
                ui.set_max_size(Vec2::new(avail_w, avail_h));
                pane(ui, list_w, avail_h, |ui| {
                    card(ui, th, |ui| {
                        job_list(ui, th, &model.jobs, state);
                    });
                });
                ui.add_space(gap);
                let rest = (avail_w - list_w - gap).max(1.0);
                pane(ui, rest, avail_h, |ui| {
                    card(ui, th, |ui| {
                        job_detail(ui, th, &model.jobs, state);
                    });
                });
            },
        );
    } else {
        let half = ((avail_h - gap) * 0.4).max(120.0);
        ui.set_min_height(avail_h);
        card(ui, th, |ui| {
            ui.set_min_height(half);
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

    fill_scroll(ui, "tab_jobs", false, |ui| {
        for (id, status_label, short_id, meta) in rows {
            let selected = state.selected_job.as_deref() == Some(id.as_str());
            let label = if show_status {
                format!("[{status_label}] {short_id}")
            } else {
                short_id
            };
            let response = selectable_row(ui, th, &label, selected, false, true);
            if response.clicked() {
                state.selected_job = Some(id);
            }
            dim_label(ui, th, &meta);
            ui.add_space(th.spacing.sm);
        }
    });
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
            tab_btn(ui, th, active == s, s.label(), None, || on_select(s));
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

    fill_scroll(ui, "job_detail", true, |ui| {
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
            dim_label(ui, th, &format!("log {}", run.log));
            dim_label(ui, th, &format!("ts {}", run.timestamp_secs));
            ui.add_space(th.spacing.md);
        }
    });
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

    if let Some(err) = &state.commit_error {
        card(ui, th, |ui| {
            dim_label(ui, th, err);
        });
    }

    let gap = th.spacing.md;
    // Already inside a height-capped pane — use residual height, not page clip.
    let avail_h = ui.available_height().max(160.0);
    let paths_h = (avail_h * 0.28).clamp(100.0, 220.0);

    card(ui, th, |ui| {
        title_2(ui, th, "Changed files");
        ui.add_space(th.spacing.sm);
        if state.commit_paths.is_empty() {
            dim_label(ui, th, "(no file changes)");
        } else {
            egui::ScrollArea::vertical()
                .id_salt("commit_paths")
                .max_height(paths_h)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    for path in state.commit_paths.clone() {
                        let selected = state.selected_diff_path.as_deref() == Some(path.as_str());
                        let response = selectable_row(ui, th, &path, selected, false, true);
                        if response.clicked() {
                            if let Some(profile) = profile {
                                state.select_diff_path(profile, &path);
                            }
                        }
                    }
                });
        }
    });

    ui.add_space(gap);

    card(ui, th, |ui| {
        let title = state.selected_diff_path.as_deref().unwrap_or("Diff");
        title_2(ui, th, title);
        ui.add_space(th.spacing.md);
        if state.diff_text.is_empty() {
            dim_label(ui, th, "Select a changed file to view the patch.");
        } else {
            fill_scroll(ui, "diff_widget", true, |ui| {
                diff_widget(ui, th, &state.diff_text);
            });
        }
    });
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
