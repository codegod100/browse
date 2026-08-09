//! Host view API: Gleam emits packed opcodes; Rust paints Vidya widgets.
//!
//! Gleam stays int-only across Wasm. Dynamic content lives in [`ViewModel`].
//! Section UI lives in [`crate::components`].
//!
//! Opcode packing: `payload * 16 + tag` (same as the Gleam guest).

use eframe::egui;
use vidya::{body, button, card, dim_label, hflow, primary_button, title, title_2, Theme};

use crate::components::{Files, Meta, Readme, RepoBrowser, RepoUi};
use radicle::Profile;

/// Dynamic strings + typed rows the guest references by opcode payload.
#[derive(Debug, Clone, Default)]
pub struct ViewModel {
    pub strings: Vec<String>,
    pub files: Vec<FileRow>,
    pub commits: Vec<CommitRow>,
    pub patches: Vec<PatchRow>,
    pub issues: Vec<IssueRow>,
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
}

/// Decoded view op (Gleam → host).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// Repo name / desc / rid / head.
    Meta,
    Title(i64),
    Body(i64),
    Button {
        primary: bool,
        msg: i64,
        label: i64,
    },
    Space(i64),
    Status(i64),
    Header(i64),
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
    /// Files / Commits / Patches / Issues tabs browser.
    RepoTabs,
    Unknown(i64),
}

impl Op {
    pub fn decode(packed: i64) -> Self {
        let tag = packed % 16;
        let payload = packed / 16;
        match tag {
            0 => Self::Meta,
            1 => Self::Title(payload),
            2 => Self::Body(payload),
            4 => {
                let label = payload % 256;
                let msg = (payload / 256) % 256;
                let primary = (payload / 65_536) % 2 == 1;
                Self::Button {
                    primary,
                    msg,
                    label,
                }
            }
            5 => Self::Space(payload),
            6 => Self::Status(payload),
            7 => Self::Header(payload),
            8 => Self::CardOpen,
            9 => Self::CardClose,
            10 => Self::Slot {
                style: (payload / 256) % 256,
                id: (payload % 256) as usize,
            },
            11 => Self::TreeList(payload.max(0) as usize),
            12 => Self::MdBody(payload.max(0) as usize),
            13 => Self::FileList(payload.max(0) as usize),
            14 => Self::CommitList(payload.max(0) as usize),
            15 => Self::RepoTabs,
            _ => Self::Unknown(packed),
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
        9 => "Patches",
        10 => "Issues",
        _ => "?",
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

/// Paint a contiguous op slice; returns a pending button msg if any.
pub fn paint(
    ui: &mut egui::Ui,
    th: &Theme,
    ops: &[Op],
    start: usize,
    end: usize,
    model: &ViewModel,
    repo_ui: &mut RepoUi,
    profile: Option<&Profile>,
) -> Option<i64> {
    let mut pending: Option<i64> = None;
    let mut button_row: Vec<(i64, bool, &'static str)> = Vec::new();
    let mut i = start;

    let flush_buttons = |ui: &mut egui::Ui,
                         th: &Theme,
                         row: &mut Vec<(i64, bool, &'static str)>,
                         pending: &mut Option<i64>| {
        if row.is_empty() {
            return;
        }
        hflow(ui, th, |ui| {
            for &(msg, primary, label) in row.iter() {
                let clicked = if primary {
                    primary_button(ui, th, label).clicked()
                } else {
                    button(ui, th, label).clicked()
                };
                if clicked {
                    *pending = Some(msg);
                }
            }
        });
        row.clear();
    };

    while i < end {
        match ops[i] {
            Op::Meta => {
                flush_buttons(ui, th, &mut button_row, &mut pending);
                Meta::show(ui, th, model);
                i += 1;
            }
            Op::Title(code) => {
                flush_buttons(ui, th, &mut button_row, &mut pending);
                title_2(ui, th, vocab(code));
                i += 1;
            }
            Op::Body(code) => {
                flush_buttons(ui, th, &mut button_row, &mut pending);
                body(ui, th, vocab(code));
                i += 1;
            }
            Op::Button {
                primary,
                msg,
                label,
            } => {
                button_row.push((msg, primary, vocab(label)));
                i += 1;
            }
            Op::Space(sz) => {
                flush_buttons(ui, th, &mut button_row, &mut pending);
                ui.add_space(space_px(th, sz));
                i += 1;
            }
            Op::Status(code) => {
                flush_buttons(ui, th, &mut button_row, &mut pending);
                dim_label(ui, th, vocab(code));
                i += 1;
            }
            Op::Header(code) => {
                flush_buttons(ui, th, &mut button_row, &mut pending);
                title(ui, th, vocab(code));
                i += 1;
            }
            Op::CardOpen => {
                flush_buttons(ui, th, &mut button_row, &mut pending);
                let close = find_card_end(ops, i + 1, end);
                let inner = {
                    let mut p = None;
                    card(ui, th, |ui| {
                        p = paint(ui, th, ops, i + 1, close, model, repo_ui, profile);
                    });
                    p
                };
                if inner.is_some() {
                    pending = inner;
                }
                i = close.min(end.saturating_sub(1)) + 1;
                if close >= end {
                    break;
                }
            }
            Op::CardClose => {
                flush_buttons(ui, th, &mut button_row, &mut pending);
                break;
            }
            Op::Slot { style, id } => {
                flush_buttons(ui, th, &mut button_row, &mut pending);
                let text = model.string(id);
                match style {
                    0 => title_2(ui, th, text),
                    1 => body(ui, th, text),
                    _ => dim_label(ui, th, text),
                }
                i += 1;
            }
            Op::TreeList(n) | Op::FileList(n) => {
                flush_buttons(ui, th, &mut button_row, &mut pending);
                Files::show(ui, th, model, n);
                i += 1;
            }
            Op::MdBody(slot) => {
                flush_buttons(ui, th, &mut button_row, &mut pending);
                Readme::show(ui, th, model, slot);
                i += 1;
            }
            Op::CommitList(n) => {
                flush_buttons(ui, th, &mut button_row, &mut pending);
                crate::components::Commits::show(ui, th, model, n);
                i += 1;
            }
            Op::RepoTabs => {
                flush_buttons(ui, th, &mut button_row, &mut pending);
                RepoBrowser::show(ui, th, model, repo_ui, profile);
                i += 1;
            }
            Op::Unknown(_) => {
                i += 1;
            }
        }
    }
    flush_buttons(ui, th, &mut button_row, &mut pending);
    pending
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
