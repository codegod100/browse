//! Paint Gleam view opcodes via the host [`crate::view_api`].

use eframe::egui;
use radicle::Profile;

use crate::components::RepoUi;
use crate::gleam_guest;
use crate::rad::RepoView;
use crate::view_api::{Op, ViewModel};

pub const MSG_OPEN: i64 = 0;
pub const MSG_BACK: i64 = 1;
pub const MSG_LOADED: i64 = 2;
pub const MSG_FAILED: i64 = 3;

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
        }
    }
}

pub struct PaintResult {
    pub pending_msg: Option<i64>,
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
                error: Some(e),
            };
        }
    };

    let mut ops = Vec::with_capacity(len);
    for i in 0..len {
        match gleam_guest::view_at(model, i as i64) {
            Ok(packed) => ops.push(Op::decode(packed)),
            Err(e) => {
                return PaintResult {
                    pending_msg: None,
                    error: Some(e),
                };
            }
        }
    }

    PaintResult {
        pending_msg: crate::view_api::paint(
            ui, th, &ops, 0, ops.len(), slots, repo_ui, profile,
        ),
        error: None,
    }
}
