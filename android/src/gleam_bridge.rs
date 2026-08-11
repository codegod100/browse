//! Paint Gleam view opcodes via the host [`crate::view_api`].

use eframe::egui;
use radicle::Profile;

use crate::components::RepoUi;
use crate::gleam_guest;
use crate::rad::{RepoSummary, RepoView};
use crate::view_api::{Op, PaintEffects, ViewModel};

pub const MSG_OPEN: i64 = 0;
pub const MSG_BACK: i64 = 1;
pub const MSG_LOADED: i64 = 2;
pub const MSG_FAILED: i64 = 3;
pub const MSG_NOPROFILE: i64 = 4;

pub type Slots = ViewModel;

impl ViewModel {
    pub fn from_view(v: &RepoView) -> Self {
        Self {
            strings: vec![
                v.name.clone(),
                if v.description.is_empty() {
                    "(no description)".into()
                } else {
                    v.description.clone()
                },
                v.rid.clone(),
                format!("head {}", v.head),
                v.readme.clone(),
                String::new(),
            ],
            files: v.files.clone(),
            commits: v.commits.clone(),
            patches: v.patches.clone(),
            issues: v.issues.clone(),
            jobs: v.jobs.clone(),
            local_repos: Vec::new(),
            rid_query: String::new(),
        }
    }

    pub fn from_error(err: &str) -> Self {
        Self {
            strings: vec![
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                String::new(),
                err.to_string(),
            ],
            files: Vec::new(),
            commits: Vec::new(),
            patches: Vec::new(),
            issues: Vec::new(),
            jobs: Vec::new(),
            local_repos: Vec::new(),
            rid_query: String::new(),
        }
    }

    pub fn for_enter(repos: &[RepoSummary], rid_query: &str) -> Self {
        Self {
            strings: Vec::new(),
            files: Vec::new(),
            commits: Vec::new(),
            patches: Vec::new(),
            issues: Vec::new(),
            jobs: Vec::new(),
            local_repos: repos.to_vec(),
            rid_query: rid_query.to_string(),
        }
    }
}

pub struct PaintResult {
    pub pending_msg: Option<i64>,
    pub open_rid: Option<String>,
    pub error: Option<String>,
}

pub fn paint(
    ui: &mut egui::Ui,
    th: &vidya::Theme,
    model: i64,
    slots: &Slots,
    repo_ui: &mut RepoUi,
    profile: Option<&Profile>,
) -> PaintResult {
    let len = match gleam_guest::view_len(model) {
        Ok(n) => n.max(0) as usize,
        Err(e) => {
            return PaintResult {
                pending_msg: None,
                open_rid: None,
                error: Some(e),
            };
        }
    };

    let use_guest_text = gleam_guest::has_view_text();
    let mut ops = Vec::with_capacity(len);
    for i in 0..len {
        let packed = match gleam_guest::view_at(model, i as i64) {
            Ok(packed) => packed,
            Err(e) => {
                return PaintResult {
                    pending_msg: None,
                    open_rid: None,
                    error: Some(e),
                };
            }
        };

        let mut op = if use_guest_text {
            Op::decode_packed(packed).0
        } else {
            Op::decode(packed)
        };

        if use_guest_text {
            match gleam_guest::view_text(model, i as i64) {
                Ok(Some(text)) => op.apply_guest_text(text),
                Ok(None) => {}
                Err(e) => {
                    return PaintResult {
                        pending_msg: None,
                        open_rid: None,
                        error: Some(e),
                    };
                }
            }
        }

        ops.push(op);
    }

    let PaintEffects {
        pending_msg,
        open_rid,
    } = crate::view_api::paint(ui, th, &ops, 0, ops.len(), slots, repo_ui, profile);

    PaintResult {
        pending_msg,
        open_rid,
        error: None,
    }
}
