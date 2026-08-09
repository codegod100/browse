//! Paint Gleam view opcodes with Vidya widgets. Dynamic text comes from host slots.

use eframe::egui;
use vidya::{body, button, card, dim_label, hflow, primary_button, title, title_2, Theme};

use crate::gleam_guest;
use crate::rad::RepoView;

pub const MSG_OPEN: i64 = 0;
#[allow(dead_code)]
pub const MSG_BACK: i64 = 1;
pub const MSG_LOADED: i64 = 2;
pub const MSG_FAILED: i64 = 3;

const TREE_BASE: usize = 6;

#[derive(Default)]
pub struct Slots {
    pub strings: Vec<String>,
}

impl Slots {
    pub fn from_view(v: &RepoView) -> Self {
        let mut strings = vec![
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
        ];
        for entry in &v.tree {
            strings.push(entry.clone());
        }
        Self { strings }
    }

    pub fn from_error(err: &str) -> Self {
        let mut strings = vec![String::new(); TREE_BASE];
        strings[5] = err.to_string();
        Self { strings }
    }

    fn get(&self, id: usize) -> &str {
        self.strings.get(id).map(|s| s.as_str()).unwrap_or("")
    }
}

fn vocab(code: i64) -> &'static str {
    match code {
        1 => "Browse",
        2 => "Paste a Radicle ID (rad:z…) for a repo already in local storage, then Open.",
        3 => "Open",
        4 => "Back",
        5 => "Could not open",
        6 => "README",
        7 => "Files",
        _ => "?",
    }
}

pub struct PaintResult {
    pub pending_msg: Option<i64>,
    pub error: Option<String>,
}

pub fn paint(ui: &mut egui::Ui, th: &Theme, model: i64, slots: &Slots) -> PaintResult {
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
            Ok(op) => ops.push(op),
            Err(e) => {
                return PaintResult {
                    pending_msg: None,
                    error: Some(e),
                };
            }
        }
    }

    PaintResult {
        pending_msg: paint_ops(ui, th, &ops, 0, ops.len(), slots),
        error: None,
    }
}

fn paint_ops(
    ui: &mut egui::Ui,
    th: &Theme,
    ops: &[i64],
    start: usize,
    end: usize,
    slots: &Slots,
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
        let op = ops[i];
        let tag = op % 16;
        let payload = op / 16;
        match tag {
            1 => {
                flush_buttons(ui, th, &mut button_row, &mut pending);
                title_2(ui, th, vocab(payload));
                i += 1;
            }
            2 => {
                flush_buttons(ui, th, &mut button_row, &mut pending);
                body(ui, th, vocab(payload));
                i += 1;
            }
            4 => {
                let label_code = payload % 256;
                let msg = (payload / 256) % 256;
                let primary = (payload / 65_536) % 2 == 1;
                button_row.push((msg, primary, vocab(label_code)));
                i += 1;
            }
            5 => {
                flush_buttons(ui, th, &mut button_row, &mut pending);
                let space = match payload {
                    0 => th.spacing.xs,
                    1 => th.spacing.sm,
                    _ => th.spacing.md,
                };
                ui.add_space(space);
                i += 1;
            }
            6 => {
                flush_buttons(ui, th, &mut button_row, &mut pending);
                dim_label(ui, th, vocab(payload));
                i += 1;
            }
            7 => {
                flush_buttons(ui, th, &mut button_row, &mut pending);
                title(ui, th, vocab(payload));
                i += 1;
            }
            8 => {
                flush_buttons(ui, th, &mut button_row, &mut pending);
                let close = find_card_end(ops, i + 1, end);
                let inner = {
                    let mut p = None;
                    card(ui, th, |ui| {
                        p = paint_ops(ui, th, ops, i + 1, close, slots);
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
            9 => {
                flush_buttons(ui, th, &mut button_row, &mut pending);
                break;
            }
            10 => {
                flush_buttons(ui, th, &mut button_row, &mut pending);
                let style = (payload / 256) % 256;
                let slot_id = (payload % 256) as usize;
                let text = slots.get(slot_id);
                match style {
                    0 => title_2(ui, th, text),
                    1 => body(ui, th, text),
                    2 => dim_label(ui, th, text),
                    _ => dim_label(ui, th, text),
                }
                i += 1;
            }
            11 => {
                flush_buttons(ui, th, &mut button_row, &mut pending);
                let n = payload.max(0) as usize;
                let available = slots.strings.len().saturating_sub(TREE_BASE);
                let show = n.min(available);
                if show == 0 {
                    dim_label(ui, th, "(empty tree)");
                } else {
                    for j in 0..show {
                        body(ui, th, slots.get(TREE_BASE + j));
                    }
                }
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }
    flush_buttons(ui, th, &mut button_row, &mut pending);
    pending
}

fn find_card_end(ops: &[i64], from: usize, end: usize) -> usize {
    let mut depth = 1i32;
    let mut i = from;
    while i < end {
        let tag = ops[i] % 16;
        if tag == 8 {
            depth += 1;
        } else if tag == 9 {
            depth -= 1;
            if depth == 0 {
                return i;
            }
        }
        i += 1;
    }
    end
}
