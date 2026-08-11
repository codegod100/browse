//! Per-repo browser tab + status-filter prefs under XDG config.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::components::{IssueStatus, JobStatus, PatchStatus, RepoUi, Tab};

const MAX_REPOS: usize = 50;
const FILE_NAME: &str = "tabs.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepoTabPrefs {
    pub rid: String,
    /// `files` | `commits` | `patches` | `issues` | `jobs`
    pub tab: String,
    /// Patch status filter (`open` / `draft` / `merged` / `archived` / `all`).
    pub patch_status: String,
    /// Issue status filter (`open` / `closed` / `all`).
    pub issue_status: String,
    /// Job status filter (`all` / `started` / `succeeded` / `failed`).
    pub job_status: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TabPrefsStore {
    pub repos: Vec<RepoTabPrefs>,
}

/// `~/.config/browse/tabs.json` (or `$XDG_CONFIG_HOME/browse/tabs.json`).
pub fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| {
                let mut p = PathBuf::from(h);
                p.push(".config");
                p
            })
        })?;
    Some(base.join("browse").join(FILE_NAME))
}

pub fn load() -> TabPrefsStore {
    let Some(path) = config_path() else {
        return TabPrefsStore::default();
    };
    let Ok(bytes) = fs::read(&path) else {
        return TabPrefsStore::default();
    };
    serde_json::from_slice::<TabPrefsStore>(&bytes).unwrap_or_default()
}

pub fn save(store: &TabPrefsStore) {
    let Some(path) = config_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(store) {
        let _ = fs::write(path, bytes);
    }
}

pub fn get<'a>(store: &'a TabPrefsStore, rid: &str) -> Option<&'a RepoTabPrefs> {
    store.repos.iter().find(|r| r.rid == rid)
}

/// Insert or replace prefs for `rid` (MRU front). Caps at [`MAX_REPOS`].
pub fn record(store: &mut TabPrefsStore, prefs: RepoTabPrefs) {
    store.repos.retain(|r| r.rid != prefs.rid);
    store.repos.insert(0, prefs);
    store.repos.truncate(MAX_REPOS);
}

pub fn from_ui(ui: &RepoUi) -> RepoTabPrefs {
    RepoTabPrefs {
        rid: ui.rid.clone(),
        tab: tab_to_str(ui.tab).to_string(),
        patch_status: patch_status_to_str(ui.patch_status).to_string(),
        issue_status: issue_status_to_str(ui.issue_status).to_string(),
        job_status: job_status_to_str(ui.job_status).to_string(),
    }
}

pub fn apply_to_ui(prefs: &RepoTabPrefs, ui: &mut RepoUi) {
    ui.tab = parse_tab(&prefs.tab).unwrap_or_default();
    ui.patch_status = parse_patch_status(&prefs.patch_status).unwrap_or_default();
    ui.issue_status = parse_issue_status(&prefs.issue_status).unwrap_or_default();
    ui.job_status = parse_job_status(&prefs.job_status).unwrap_or_default();
}

fn tab_to_str(tab: Tab) -> &'static str {
    match tab {
        Tab::Files => "files",
        Tab::Commits => "commits",
        Tab::Patches => "patches",
        Tab::Issues => "issues",
        Tab::Jobs => "jobs",
    }
}

fn parse_tab(s: &str) -> Option<Tab> {
    Some(match s {
        "files" => Tab::Files,
        "commits" => Tab::Commits,
        "patches" => Tab::Patches,
        "issues" => Tab::Issues,
        "jobs" => Tab::Jobs,
        _ => return None,
    })
}

fn patch_status_to_str(s: PatchStatus) -> &'static str {
    match s {
        PatchStatus::Open => "open",
        PatchStatus::Draft => "draft",
        PatchStatus::Merged => "merged",
        PatchStatus::Archived => "archived",
        PatchStatus::All => "all",
    }
}

fn parse_patch_status(s: &str) -> Option<PatchStatus> {
    Some(match s {
        "open" => PatchStatus::Open,
        "draft" => PatchStatus::Draft,
        "merged" => PatchStatus::Merged,
        "archived" => PatchStatus::Archived,
        "all" => PatchStatus::All,
        _ => return None,
    })
}

fn issue_status_to_str(s: IssueStatus) -> &'static str {
    match s {
        IssueStatus::Open => "open",
        IssueStatus::Closed => "closed",
        IssueStatus::All => "all",
    }
}

fn parse_issue_status(s: &str) -> Option<IssueStatus> {
    Some(match s {
        "open" => IssueStatus::Open,
        "closed" => IssueStatus::Closed,
        "all" => IssueStatus::All,
        _ => return None,
    })
}

fn job_status_to_str(s: JobStatus) -> &'static str {
    match s {
        JobStatus::All => "all",
        JobStatus::Started => "started",
        JobStatus::Succeeded => "succeeded",
        JobStatus::Failed => "failed",
    }
}

fn parse_job_status(s: &str) -> Option<JobStatus> {
    Some(match s {
        "all" => JobStatus::All,
        "started" => JobStatus::Started,
        "succeeded" => JobStatus::Succeeded,
        "failed" => JobStatus::Failed,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_dedupes_and_mru() {
        let mut store = TabPrefsStore::default();
        record(
            &mut store,
            RepoTabPrefs {
                rid: "rad:z1".into(),
                tab: "patches".into(),
                patch_status: "open".into(),
                issue_status: "open".into(),
                job_status: "all".into(),
            },
        );
        record(
            &mut store,
            RepoTabPrefs {
                rid: "rad:z2".into(),
                tab: "issues".into(),
                patch_status: "open".into(),
                issue_status: "closed".into(),
                job_status: "all".into(),
            },
        );
        record(
            &mut store,
            RepoTabPrefs {
                rid: "rad:z1".into(),
                tab: "jobs".into(),
                patch_status: "draft".into(),
                issue_status: "open".into(),
                job_status: "failed".into(),
            },
        );
        assert_eq!(store.repos.len(), 2);
        assert_eq!(store.repos[0].rid, "rad:z1");
        assert_eq!(store.repos[0].tab, "jobs");
        assert_eq!(store.repos[0].patch_status, "draft");
        assert_eq!(store.repos[1].rid, "rad:z2");
    }

    #[test]
    fn roundtrip_ui_prefs() {
        let mut ui = RepoUi {
            rid: "rad:zabc".into(),
            tab: Tab::Patches,
            patch_status: PatchStatus::Merged,
            issue_status: IssueStatus::Closed,
            job_status: JobStatus::Succeeded,
            ..RepoUi::default()
        };
        let prefs = from_ui(&ui);
        assert_eq!(prefs.tab, "patches");
        assert_eq!(prefs.patch_status, "merged");
        assert_eq!(prefs.issue_status, "closed");
        assert_eq!(prefs.job_status, "succeeded");

        ui = RepoUi::default();
        apply_to_ui(&prefs, &mut ui);
        assert_eq!(ui.tab, Tab::Patches);
        assert_eq!(ui.patch_status, PatchStatus::Merged);
        assert_eq!(ui.issue_status, IssueStatus::Closed);
        assert_eq!(ui.job_status, JobStatus::Succeeded);
    }

    #[test]
    fn parse_unknown_falls_back() {
        let prefs = RepoTabPrefs {
            rid: "rad:z".into(),
            tab: "nope".into(),
            patch_status: "weird".into(),
            issue_status: "nah".into(),
            job_status: "zzz".into(),
        };
        let mut ui = RepoUi {
            tab: Tab::Jobs,
            ..RepoUi::default()
        };
        apply_to_ui(&prefs, &mut ui);
        assert_eq!(ui.tab, Tab::Files);
        assert_eq!(ui.patch_status, PatchStatus::Open);
        assert_eq!(ui.issue_status, IssueStatus::Open);
        assert_eq!(ui.job_status, JobStatus::All);
    }

    #[test]
    fn record_caps_at_max() {
        let mut store = TabPrefsStore::default();
        for i in 0..(MAX_REPOS + 5) {
            record(
                &mut store,
                RepoTabPrefs {
                    rid: format!("rad:z{i}"),
                    tab: "files".into(),
                    patch_status: "open".into(),
                    issue_status: "open".into(),
                    job_status: "all".into(),
                },
            );
        }
        assert_eq!(store.repos.len(), MAX_REPOS);
        assert_eq!(store.repos[0].rid, format!("rad:z{}", MAX_REPOS + 4));
    }
}
