//! Interactive Files / Commits / Patches / Issues / Jobs browser (host-owned selection).

use eframe::egui::{self, Align, CursorIcon, FontFamily, Layout, RichText, Sense, Vec2};
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
            tab_btn(ui, th, state.tab == Tab::Files, "Files", || {
                state.tab = Tab::Files;
            });
            ui.add_space(th.spacing.sm);
            tab_btn(ui, th, state.tab == Tab::Commits, "Commits", || {
                state.tab = Tab::Commits;
            });
            ui.add_space(th.spacing.sm);
            tab_btn(ui, th, state.tab == Tab::Patches, "Patches", || {
                state.tab = Tab::Patches;
            });
            ui.add_space(th.spacing.sm);
            tab_btn(ui, th, state.tab == Tab::Issues, "Issues", || {
                state.tab = Tab::Issues;
            });
            ui.add_space(th.spacing.sm);
            tab_btn(ui, th, state.tab == Tab::Jobs, "Jobs", || {
                state.tab = Tab::Jobs;
            });
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

fn tab_btn(ui: &mut egui::Ui, th: &Theme, active: bool, label: &str, on: impl FnOnce()) {
    let response = if active {
        primary_button(ui, th, label)
    } else {
        button(ui, th, label)
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
    ui.add_space(th.spacing.md);
    if patches.is_empty() {
        dim_label(ui, th, "(no patches)");
        return;
    }
    fill_scroll(ui, "tab_patches", false, |ui| {
        for p in patches {
            let selected = state.selected_patch.as_deref() == Some(p.id.as_str());
            let label = format!("[{}] {}", p.state, p.title);
            let response = selectable_row(ui, th, &label, selected, false, true);
            if response.clicked() {
                state.selected_patch = Some(p.id.clone());
            }
            dim_label(ui, th, &format!("{} · {}", p.short_id, p.author));
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
    ui.add_space(th.spacing.md);
    if issues.is_empty() {
        dim_label(ui, th, "(no issues)");
        return;
    }
    fill_scroll(ui, "tab_issues", false, |ui| {
        for issue in issues {
            let selected = state.selected_issue.as_deref() == Some(issue.id.as_str());
            let label = format!("[{}] {}", issue.state, issue.title);
            let response = selectable_row(ui, th, &label, selected, false, true);
            if response.clicked() {
                state.selected_issue = Some(issue.id.clone());
            }
            let replies = if issue.replies == 0 {
                "no replies".to_string()
            } else {
                format!(
                    "{} repl{}",
                    issue.replies,
                    if issue.replies == 1 { "y" } else { "ies" }
                )
            };
            dim_label(
                ui,
                th,
                &format!("{} · {} · {}", issue.short_id, issue.author, replies),
            );
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
    ui.add_space(th.spacing.md);
    if jobs.is_empty() {
        dim_label(ui, th, "(no job COBs)");
        return;
    }
    fill_scroll(ui, "tab_jobs", false, |ui| {
        for job in jobs {
            let selected = state.selected_job.as_deref() == Some(job.id.as_str());
            let label = format!("[{}] {}", job.status, job.short_id);
            let response = selectable_row(ui, th, &label, selected, false, true);
            if response.clicked() {
                state.selected_job = Some(job.id.clone());
            }
            dim_label(
                ui,
                th,
                &format!(
                    "commit {} · {} run{} · {} node{}",
                    job.short_commit,
                    job.run_count,
                    if job.run_count == 1 { "" } else { "s" },
                    job.node_count,
                    if job.node_count == 1 { "" } else { "s" }
                ),
            );
            ui.add_space(th.spacing.sm);
        }
    });
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
