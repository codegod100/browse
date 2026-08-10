//// Browse — Gleam owns screens, navigation, view opcodes, and UI copy.
////
//// Exports:
////   browse__{init,update,view_len,view_at,view_text,label,screen_name,tag_name}
////
//// Screens (model):
////   0 enter   — hint, help/about (inventory is host-owned: recent + search)
////   1 viewing — chrome + meta + files|commits|patches|issues|jobs
////   2 error   — open failure
////   3 noprof  — missing ~/.radicle profile
////   4 help    — how to use Browse
////   5 about   — what Browse is
////
//// Packing: payload * 16 + tag
////   0 meta · 1 title · 2 body · 3 repo_list · 4 button · 5 space · 6 status
////   7 header · 8/9 card · 10 slot · 11 tree_list · 12 md_body
////  13 file_list · 14 commit_list · 15 repo_tabs (Files|Commits|Patches|Issues|Jobs)
////
//// Enter chrome is Gleam; recently-viewed + local inventory stay host-owned
//// so search/recent UX stays rich (android/src/components/repo_list.rs).
////
//// `view_text(model, i)` — per-opcode String copy.
//// `label(id)` — shared chrome strings the host paints (tabs, RID row, empties).
//// Space: 0=xs · 1=sm · 2=md · 3=lg · 4=xl · 5=page
////
//// Note: keep helpers in this module — Wasm Gleam cannot call across modules yet.

// --- Messages --------------------------------------------------------------

pub fn msg_open() -> Int {
  0
}

pub fn msg_back() -> Int {
  1
}

pub fn msg_loaded() -> Int {
  2
}

pub fn msg_failed() -> Int {
  3
}

pub fn msg_noprofile() -> Int {
  4
}

pub fn msg_help() -> Int {
  5
}

pub fn msg_about() -> Int {
  6
}

// --- Screen / opcode names (host diagnostics + docs) -----------------------

pub fn screen_name(model: Int) -> String {
  case model {
    0 -> "enter"
    1 -> "viewing"
    2 -> "error"
    3 -> "noprof"
    4 -> "help"
    5 -> "about"
    _ -> "unknown"
  }
}

pub fn tag_name(tag: Int) -> String {
  case tag {
    0 -> "meta"
    1 -> "title"
    2 -> "body"
    3 -> "repo_list"
    4 -> "button"
    5 -> "space"
    6 -> "status"
    7 -> "header"
    8 -> "card_open"
    9 -> "card_close"
    10 -> "slot"
    11 -> "tree_list"
    12 -> "md_body"
    13 -> "file_list"
    14 -> "commit_list"
    15 -> "repo_tabs"
    _ -> "unknown"
  }
}

pub fn is_enter(model: Int) -> Int {
  case model {
    0 -> 1
    _ -> 0
  }
}

pub fn is_viewing(model: Int) -> Int {
  case model {
    1 -> 1
    _ -> 0
  }
}

pub fn shows_repo_chrome(model: Int) -> Int {
  case model {
    1 -> 1
    _ -> 0
  }
}

// --- Shared labels (host chrome) -------------------------------------------
//
// Stable ids — keep in sync with Rust fallbacks in view_api::label_fallback.

pub fn label(id: Int) -> String {
  case id {
    1 -> "Browse"
    2 -> "Open"
    3 -> "RID"
    4 -> "Back"
    5 -> "Files"
    6 -> "Commits"
    7 -> "README"
    8 -> "Local repos"
    9 -> "No repositories in local storage yet."
    10 -> "No repos match this filter."
    11 -> "Up"
    12 -> "(binary or too large)"
    13 -> "(empty tree)"
    14 -> "(no commits)"
    15 -> "Select a file to view its contents."
    16 -> "Select a commit to inspect its diff."
    17 -> "Changed files"
    18 -> "Diff"
    19 -> "Seed a repo with radicle, then it will show up here."
    20 -> "Filter by typing in the RID field."
    21 -> "Help"
    22 -> "(no file changes)"
    23 -> "Select a changed file to view the patch."
    24 -> "Files"
    25 -> "About"
    26 -> "Patches"
    27 -> "Issues"
    28 -> "Jobs"
    29 -> "Copy RID"
    30 -> "Head"
    31 -> "Description"
    32 -> "Name"
    33 -> "Storage"
    34 -> "Local only — Browse does not fetch from the network."
    35 -> "Vidya shell · Gleam screens · Radicle storage"
    36 -> "Repository"
    37 -> "History"
    38 -> "Tree"
    39 -> "Blob"
    40 -> "Markdown"
    _ -> "?"
  }
}

// --- TEA -------------------------------------------------------------------

pub fn init() -> Int {
  0
}

pub fn update(model: Int, msg: Int) -> Int {
  case msg {
    // back — always return to enter
    1 -> 0
    // loaded → viewing
    2 -> 1
    // failed → error
    3 -> 2
    // noprofile
    4 -> 3
    // help
    5 -> 4
    // about
    6 -> 5
    _ -> model
  }
}

pub fn view_len(model: Int) -> Int {
  case model {
    0 -> enter_len()
    1 -> viewing_len()
    2 -> error_len()
    3 -> noprof_len()
    4 -> help_len()
    5 -> about_len()
    _ -> 0
  }
}

pub fn view_at(model: Int, i: Int) -> Int {
  case model {
    0 -> enter_at(i)
    1 -> viewing_at(i)
    2 -> error_at(i)
    3 -> noprof_at(i)
    4 -> help_at(i)
    5 -> about_at(i)
    _ -> 0
  }
}

pub fn view_text(model: Int, i: Int) -> String {
  case model {
    0 -> enter_text(i)
    1 -> viewing_text(i)
    2 -> error_text(i)
    3 -> noprof_text(i)
    4 -> help_text(i)
    5 -> about_text(i)
    _ -> ""
  }
}

// --- Enter -----------------------------------------------------------------

fn enter_len() -> Int {
  12
}

fn enter_at(i: Int) -> Int {
  case i {
    0 -> header_op()
    1 -> space(1)
    2 -> body_op()
    3 -> space(1)
    4 -> status_op()
    5 -> space(1)
    6 -> status_op()
    7 -> space(2)
    8 -> button(0, msg_help(), 0)
    9 -> space(1)
    10 -> button(0, msg_about(), 0)
    11 -> space(2)
    _ -> 0
  }
}

fn enter_text(i: Int) -> String {
  case i {
    0 -> label(1)
    2 -> "Paste a Radicle ID (rad:z…) for a repo already in local storage, then Open."
    4 -> label(20)
    6 -> label(34)
    8 -> label(21)
    10 -> label(25)
    _ -> ""
  }
}

// --- Viewing ---------------------------------------------------------------

fn viewing_len() -> Int {
  16
}

fn viewing_at(i: Int) -> Int {
  case i {
    0 -> header_op()
    1 -> space(1)
    2 -> button(0, msg_back(), 0)
    3 -> space(1)
    4 -> status_op()
    5 -> space(1)
    6 -> status_op()
    7 -> space(2)
    8 -> card_open()
    9 -> title_op()
    10 -> space(1)
    11 -> meta()
    12 -> card_close()
    13 -> space(2)
    14 -> status_op()
    15 -> repo_tabs()
    _ -> 0
  }
}

fn viewing_text(i: Int) -> String {
  case i {
    0 -> label(1)
    2 -> label(4)
    4 -> "Repository opened from local Radicle storage."
    6 -> label(35)
    9 -> label(36)
    14 -> "Browse files at HEAD or recent commits and diffs."
    _ -> ""
  }
}

// --- Error -----------------------------------------------------------------

fn error_len() -> Int {
  14
}

fn error_at(i: Int) -> Int {
  case i {
    0 -> header_op()
    1 -> space(2)
    2 -> card_open()
    3 -> title_op()
    4 -> status_op()
    5 -> space(1)
    6 -> body_op()
    7 -> space(1)
    8 -> body_op()
    9 -> space(1)
    10 -> slot(1, 5)
    11 -> space(2)
    12 -> button(1, msg_back(), 0)
    13 -> card_close()
    _ -> 0
  }
}

fn error_text(i: Int) -> String {
  case i {
    0 -> label(1)
    3 -> "Could not open"
    4 -> "Check the RID and that the repo is seeded locally."
    6 -> "The host could not load this repository from your Radicle profile."
    8 -> "Confirm the ID starts with rad:z and that `rad` can see the repo."
    12 -> label(4)
    _ -> ""
  }
}

// --- No profile ------------------------------------------------------------

fn noprof_len() -> Int {
  11
}

fn noprof_at(i: Int) -> Int {
  case i {
    0 -> header_op()
    1 -> space(2)
    2 -> card_open()
    3 -> title_op()
    4 -> body_op()
    5 -> space(1)
    6 -> status_op()
    7 -> space(1)
    8 -> body_op()
    9 -> slot(2, 5)
    10 -> card_close()
    _ -> 0
  }
}

fn noprof_text(i: Int) -> String {
  case i {
    0 -> label(1)
    3 -> "No Radicle profile"
    4 -> "Could not load ~/.radicle. Create a profile with radicle, then reopen Browse."
    6 -> "Browse only shows repositories already present in local storage."
    8 -> "After `rad auth` (or equivalent), restart Browse and seed a project."
    _ -> ""
  }
}

// --- Help ------------------------------------------------------------------

fn help_len() -> Int {
  26
}

fn help_at(i: Int) -> Int {
  case i {
    0 -> header_op()
    1 -> space(1)
    2 -> button(0, msg_back(), 0)
    3 -> space(1)
    4 -> button(0, msg_about(), 0)
    5 -> space(2)
    6 -> card_open()
    7 -> title_op()
    8 -> space(1)
    9 -> body_op()
    10 -> space(1)
    11 -> body_op()
    12 -> space(1)
    13 -> body_op()
    14 -> space(1)
    15 -> body_op()
    16 -> space(1)
    17 -> status_op()
    18 -> space(2)
    19 -> title_op()
    20 -> space(1)
    21 -> body_op()
    22 -> space(1)
    23 -> body_op()
    24 -> body_op()
    25 -> card_close()
    _ -> 0
  }
}

fn help_text(i: Int) -> String {
  case i {
    0 -> label(1)
    2 -> label(4)
    4 -> label(25)
    7 -> "How to use Browse"
    9 -> "1. Seed or clone a repository into your local Radicle storage."
    11 -> "2. Paste its rad:z… ID in the RID field, or pick it from Local repos."
    13 -> "3. Press Open to load name, description, files, and commits."
    15 -> "4. Use Files for the tree at HEAD; use Commits for history and diffs."
    17 -> "Files opens blobs at HEAD. Commits shows history and per-path diffs."
    19 -> "Tips"
    21 -> "The RID field also filters the local inventory as you type."
    23 -> "Click a local repo row to fill the RID and open it immediately."
    24 -> "Copy chip on the RID field puts the current ID on the clipboard."
    _ -> ""
  }
}

// --- About -----------------------------------------------------------------

fn about_len() -> Int {
  20
}

fn about_at(i: Int) -> Int {
  case i {
    0 -> header_op()
    1 -> space(1)
    2 -> button(0, msg_back(), 0)
    3 -> space(1)
    4 -> button(0, msg_help(), 0)
    5 -> space(2)
    6 -> card_open()
    7 -> title_op()
    8 -> space(1)
    9 -> body_op()
    10 -> space(1)
    11 -> body_op()
    12 -> space(1)
    13 -> status_op()
    14 -> space(2)
    15 -> title_op()
    16 -> space(1)
    17 -> body_op()
    18 -> body_op()
    19 -> card_close()
    _ -> 0
  }
}

fn about_text(i: Int) -> String {
  case i {
    0 -> label(1)
    2 -> label(4)
    4 -> label(21)
    7 -> "About Browse"
    9 -> "Browse is a minimal Radicle repository viewer for your desktop."
    11 -> "It reads projects already seeded in local storage — it does not dial the network."
    13 -> label(35)
    15 -> "Stack"
    17 -> "Gleam owns screens, navigation, and UI copy (this Wasm guest)."
    18 -> "Rust hosts Vidya/egui painting and the radicle crates for Profile + git."
    _ -> ""
  }
}

// --- View DSL --------------------------------------------------------------

fn meta() -> Int {
  pack(tag_meta(), 0)
}

fn title_op() -> Int {
  pack(tag_title(), 0)
}

fn body_op() -> Int {
  pack(tag_body(), 0)
}

fn repo_list() -> Int {
  pack(tag_repo_list(), 0)
}

fn button(primary: Int, msg: Int, label_code: Int) -> Int {
  pack(tag_button(), primary * 65_536 + msg * 256 + label_code)
}

fn space(sz: Int) -> Int {
  pack(tag_space(), sz)
}

fn status_op() -> Int {
  pack(tag_status(), 0)
}

fn header_op() -> Int {
  pack(tag_header(), 0)
}

fn card_open() -> Int {
  pack(tag_card_open(), 0)
}

fn card_close() -> Int {
  pack(tag_card_close(), 0)
}

fn slot(style: Int, id: Int) -> Int {
  pack(tag_slot(), style * 256 + id)
}

fn tree_list(n: Int) -> Int {
  pack(tag_tree_list(), n)
}

fn md_body(slot_id: Int) -> Int {
  pack(tag_md_body(), slot_id)
}

fn file_list(n: Int) -> Int {
  pack(tag_file_list(), n)
}

fn commit_list(n: Int) -> Int {
  pack(tag_commit_list(), n)
}

fn repo_tabs() -> Int {
  pack(tag_repo_tabs(), 0)
}

fn pack(tag: Int, payload: Int) -> Int {
  payload * 16 + tag
}

fn unpack_tag(packed: Int) -> Int {
  packed - packed / 16 * 16
}

fn unpack_payload(packed: Int) -> Int {
  packed / 16
}

// Opcode tag constants (single source for the DSL).

fn tag_meta() -> Int {
  0
}

fn tag_title() -> Int {
  1
}

fn tag_body() -> Int {
  2
}

fn tag_repo_list() -> Int {
  3
}

fn tag_button() -> Int {
  4
}

fn tag_space() -> Int {
  5
}

fn tag_status() -> Int {
  6
}

fn tag_header() -> Int {
  7
}

fn tag_card_open() -> Int {
  8
}

fn tag_card_close() -> Int {
  9
}

fn tag_slot() -> Int {
  10
}

fn tag_tree_list() -> Int {
  11
}

fn tag_md_body() -> Int {
  12
}

fn tag_file_list() -> Int {
  13
}

fn tag_commit_list() -> Int {
  14
}

fn tag_repo_tabs() -> Int {
  15
}

// Keep unpack helpers referenced so the Wasm build retains them for hosts.
pub fn debug_tag(packed: Int) -> Int {
  unpack_tag(packed)
}

pub fn debug_payload(packed: Int) -> Int {
  unpack_payload(packed)
}
