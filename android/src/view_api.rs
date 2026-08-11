//! Host view API: Gleam emits packed opcodes; Rust paints Vidya widgets.
//!
//! Guest text may come from Wasm strings (`browse__view_text`) or legacy int
//! vocab codes in the opcode payload. Dynamic repo content still lives in
//! [`ViewModel`]. Section UI lives in [`crate::components`].
//!
//! Opcode packing: `payload * 16 + tag` (same as the Gleam guest).

use eframe::egui;
use vidya::{body, button, card, dim_label, hflow, primary_button, title, title_2, Theme};

use crate::components::{Files, Meta, Readme, RepoBrowser, RepoUi};
use crate::rad::RepoSummary;
use radicle::Profile;

/// Dynamic strings + typed rows the guest references by opcode payload.
#[derive(Debug, Clone, Default)]
pub struct ViewModel {
    pub strings: Vec<String>,
    pub files: Vec<FileRow>,
    pub commits: Vec<CommitRow>,
    pub patches: Vec<PatchRow>,
    pub issues: Vec<IssueRow>,
    pub jobs: Vec<JobRow>,
    /// Enter-screen inventory (host-filled; Gleam may emit `RepoList` op).
    pub local_repos: Vec<RepoSummary>,
    /// RID/filter text used when Gleam paints a repo list.
    pub rid_query: String,
}

#[derive(Debug, Clone)]
pub struct FileRow {
    pub name: String,
    pub is_tree: bool,
}

#[derive(Debug, Clone)]
pub struct CommitRow {
    pub id: String,
    pub summary: String,
    pub short_id: String,
    pub author: String,
}

#[derive(Debug, Clone)]
pub struct PatchRow {
    pub id: String,
    pub short_id: String,
    pub title: String,
    pub state: String,
    pub author: String,
    pub description: String,
    pub head: String,
    pub base: String,
    pub revisions: usize,
    /// Milliseconds since epoch (for sort / display).
    pub updated_ms: u64,
}

#[derive(Debug, Clone)]
pub struct IssueRow {
    pub id: String,
    pub short_id: String,
    pub title: String,
    pub state: String,
    pub author: String,
    pub description: String,
    pub replies: usize,
    /// Milliseconds since epoch (for sort / display).
    pub updated_ms: u64,
}

#[derive(Debug, Clone)]
pub struct JobRunRow {
    pub node: String,
    pub run_id: String,
    pub status: String,
    pub log: String,
    pub timestamp_secs: u64,
}

#[derive(Debug, Clone)]
pub struct JobRow {
    pub id: String,
    pub short_id: String,
    pub commit: String,
    pub short_commit: String,
    pub status: String,
    pub run_count: usize,
    pub node_count: usize,
    pub updated_secs: u64,
    pub runs: Vec<JobRunRow>,
}

/// Side effects collected while painting a Gleam view.
#[derive(Debug, Default)]
pub struct PaintEffects {
    pub pending_msg: Option<i64>,
    pub open_rid: Option<String>,
}

impl PaintEffects {
    fn merge(&mut self, other: Self) {
        if other.pending_msg.is_some() {
            self.pending_msg = other.pending_msg;
        }
        if other.open_rid.is_some() {
            self.open_rid = other.open_rid;
        }
    }
}

/// Decoded view op (Gleam → host).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    /// Repo name / desc / rid / head.
    Meta,
    Title(String),
    Body(String),
    /// Local storage inventory (enter screen).
    RepoList,
    Button {
        primary: bool,
        msg: i64,
        label: String,
    },
    Space(i64),
    Status(String),
    Header(String),
    CardOpen,
    CardClose,
    Slot {
        style: i64,
        id: usize,
    },
    TreeList(usize),
    MdBody(usize),
    FileList(usize),
    CommitList(usize),
    /// Files / Commits / Patches / Issues / Jobs tabs browser.
    RepoTabs,
    Unknown(i64),
}

impl Op {
    pub fn decode_packed(packed: i64) -> (Self, /* needs_text */ bool) {
        let tag = packed % 16;
        let payload = packed / 16;
        match tag {
            0 => (Self::Meta, false),
            1 => (Self::Title(String::new()), true),
            2 => (Self::Body(String::new()), true),
            3 => (Self::RepoList, false),
            4 => {
                let label_code = payload % 256;
                let msg = (payload / 256) % 256;
                let primary = (payload / 65_536) % 2 == 1;
                (
                    Self::Button {
                        primary,
                        msg,
                        label: vocab(label_code).to_string(),
                    },
                    true,
                )
            }
            5 => (Self::Space(payload), false),
            6 => (Self::Status(String::new()), true),
            7 => (Self::Header(String::new()), true),
            8 => (Self::CardOpen, false),
            9 => (Self::CardClose, false),
            10 => (
                Self::Slot {
                    style: (payload / 256) % 256,
                    id: (payload % 256) as usize,
                },
                false,
            ),
            11 => (Self::TreeList(payload.max(0) as usize), false),
            12 => (Self::MdBody(payload.max(0) as usize), false),
            13 => (Self::FileList(payload.max(0) as usize), false),
            14 => (Self::CommitList(payload.max(0) as usize), false),
            15 => (Self::RepoTabs, false),
            _ => (Self::Unknown(packed), false),
        }
    }

    /// Legacy path when the guest has no `browse__view_text`.
    pub fn decode(packed: i64) -> Self {
        let (mut op, needs_text) = Self::decode_packed(packed);
        if needs_text {
            let payload = packed / 16;
            match &mut op {
                Self::Title(s) => *s = vocab(payload).to_string(),
                Self::Body(s) => *s = vocab(payload).to_string(),
                Self::Status(s) => *s = vocab(payload).to_string(),
                Self::Header(s) => *s = vocab(payload).to_string(),
                Self::Button { label, .. } => {
                    let label_code = payload % 256;
                    *label = vocab(label_code).to_string();
                }
                _ => {}
            }
        }
        op
    }

    pub fn apply_guest_text(&mut self, text: String) {
        if text.is_empty() {
            return;
        }
        match self {
            Self::Title(s) | Self::Body(s) | Self::Status(s) | Self::Header(s) => *s = text,
            Self::Button { label, .. } => *label = text,
            _ => {}
        }
    }
}

pub fn vocab(code: i64) -> &'static str {
    match code {
        1 => "Browse",
        2 => "Paste a Radicle ID (rad:z…) for a repo already in local storage, then Open.",
        3 => "Open",
        4 => "Back",
        5 => "Could not open",
        6 => "README",
        7 => "Files",
        8 => "Commits",
        _ => "?",
    }
}

/// Host chrome strings — prefer Gleam `label(id)`, else Rust fallback.
pub fn label_fallback(id: i64) -> &'static str {
    match id {
        1 => "Browse",
        2 => "Open",
        3 => "RID",
        4 => "Back",
        5 => "Files",
        6 => "Commits",
        7 => "README",
        8 => "Local repos",
        9 => "No repositories in local storage yet.",
        10 => "No repos match this filter.",
        11 => "Up",
        12 => "(binary or too large)",
        13 => "(empty tree)",
        14 => "(no commits)",
        15 => "Select a file to view its contents.",
        16 => "Select a commit to inspect its diff.",
        17 => "Changed files",
        18 => "Diff",
        19 => "Seed a repo with radicle, then it will show up here.",
        20 => "Filter by typing in the RID field.",
        21 => "Help",
        22 => "(no file changes)",
        23 => "Select a changed file to view the patch.",
        24 => "Files",
        25 => "About",
        26 => "Patches",
        27 => "Issues",
        28 => "Jobs",
        29 => "Copy RID",
        30 => "Head",
        31 => "Description",
        32 => "Name",
        33 => "Storage",
        34 => "Local only — Browse does not fetch from the network.",
        35 => "Vidya shell · Gleam screens · Radicle storage",
        36 => "Repository",
        37 => "History",
        38 => "Tree",
        39 => "Blob",
        40 => "Markdown",
        _ => "?",
    }
}

/// Resolve a shared UI label from the Gleam guest when available.
pub fn ui_label(id: i64) -> String {
    match crate::gleam_guest::label(id) {
        Ok(Some(s)) if !s.is_empty() => s,
        _ => label_fallback(id).to_string(),
    }
}

impl ViewModel {
    pub fn string(&self, id: usize) -> &str {
        self.strings.get(id).map(|s| s.as_str()).unwrap_or("")
    }
}

fn space_px(th: &Theme, sz: i64) -> f32 {
    match sz {
        0 => th.spacing.xs,
        1 => th.spacing.sm,
        2 => th.spacing.md,
        3 => th.spacing.lg,
        4 => th.spacing.xl,
        _ => th.spacing.page,
    }
}

/// Paint a contiguous op slice; returns button msgs / repo opens.
pub fn paint(
    ui: &mut egui::Ui,
    th: &Theme,
    ops: &[Op],
    start: usize,
    end: usize,
    model: &ViewModel,
    repo_ui: &mut RepoUi,
    profile: Option<&Profile>,
) -> PaintEffects {
    let mut effects = PaintEffects::default();
    let mut button_row: Vec<(i64, bool, String)> = Vec::new();
    let mut i = start;

    let flush_buttons = |ui: &mut egui::Ui,
                         th: &Theme,
                         row: &mut Vec<(i64, bool, String)>,
                         effects: &mut PaintEffects| {
        if row.is_empty() {
            return;
        }
        hflow(ui, th, |ui| {
            for (msg, primary, label) in row.iter() {
                let clicked = if *primary {
                    primary_button(ui, th, label).clicked()
                } else {
                    button(ui, th, label).clicked()
                };
                if clicked {
                    effects.pending_msg = Some(*msg);
                }
            }
        });
        row.clear();
    };

    while i < end {
        match &ops[i] {
            Op::Meta => {
                flush_buttons(ui, th, &mut button_row, &mut effects);
                Meta::show(ui, th, model);
                i += 1;
            }
            Op::Title(text) => {
                flush_buttons(ui, th, &mut button_row, &mut effects);
                title_2(ui, th, text);
                i += 1;
            }
            Op::Body(text) => {
                flush_buttons(ui, th, &mut button_row, &mut effects);
                body(ui, th, text);
                i += 1;
            }
            Op::RepoList => {
                // Recently-viewed + searchable local inventory is host-owned
                // (`app` model==0 → RepoList::show). Gleam may still emit this
                // tag; treat as a no-op so enter chrome and inventory don't dual-paint.
                flush_buttons(ui, th, &mut button_row, &mut effects);
                i += 1;
            }
            Op::Button {
                primary,
                msg,
                label,
            } => {
                button_row.push((*msg, *primary, label.clone()));
                i += 1;
            }
            Op::Space(sz) => {
                flush_buttons(ui, th, &mut button_row, &mut effects);
                ui.add_space(space_px(th, *sz));
                i += 1;
            }
            Op::Status(text) => {
                flush_buttons(ui, th, &mut button_row, &mut effects);
                dim_label(ui, th, text);
                i += 1;
            }
            Op::Header(text) => {
                flush_buttons(ui, th, &mut button_row, &mut effects);
                title(ui, th, text);
                i += 1;
            }
            Op::CardOpen => {
                flush_buttons(ui, th, &mut button_row, &mut effects);
                let close = find_card_end(ops, i + 1, end);
                let inner = {
                    let mut inner_effects = PaintEffects::default();
                    card(ui, th, |ui| {
                        inner_effects = paint(
                            ui, th, ops, i + 1, close, model, repo_ui, profile,
                        );
                    });
                    inner_effects
                };
                effects.merge(inner);
                i = close.min(end.saturating_sub(1)) + 1;
                if close >= end {
                    break;
                }
            }
            Op::CardClose => {
                flush_buttons(ui, th, &mut button_row, &mut effects);
                break;
            }
            Op::Slot { style, id } => {
                flush_buttons(ui, th, &mut button_row, &mut effects);
                let text = model.string(*id);
                match style {
                    0 => title_2(ui, th, text),
                    1 => body(ui, th, text),
                    _ => dim_label(ui, th, text),
                }
                i += 1;
            }
            Op::TreeList(n) | Op::FileList(n) => {
                flush_buttons(ui, th, &mut button_row, &mut effects);
                Files::show(ui, th, model, *n);
                i += 1;
            }
            Op::MdBody(slot) => {
                flush_buttons(ui, th, &mut button_row, &mut effects);
                Readme::show(ui, th, model, *slot);
                i += 1;
            }
            Op::CommitList(n) => {
                flush_buttons(ui, th, &mut button_row, &mut effects);
                crate::components::Commits::show(ui, th, model, *n);
                i += 1;
            }
            Op::RepoTabs => {
                // Keep pending Gleam buttons (e.g. Back) in the same row as Files/Commits/…
                RepoBrowser::show(
                    ui,
                    th,
                    model,
                    repo_ui,
                    profile,
                    std::mem::take(&mut button_row),
                    &mut effects,
                );
                i += 1;
            }
            Op::Unknown(_) => {
                i += 1;
            }
        }
    }
    flush_buttons(ui, th, &mut button_row, &mut effects);
    effects
}

fn find_card_end(ops: &[Op], from: usize, end: usize) -> usize {
    let mut depth = 1i32;
    let mut i = from;
    while i < end {
        match ops[i] {
            Op::CardOpen => depth += 1,
            Op::CardClose => {
                depth -= 1;
                if depth == 0 {
                    return i;
                }
            }
            _ => {}
        }
        i += 1;
    }
    end
}
