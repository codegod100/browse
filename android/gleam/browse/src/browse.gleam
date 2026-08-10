//// Browse — Gleam owns screens / navigation / view opcodes.
////
//// Exports: browse__{init,update,view_len,view_at}
////
//// View DSL packs to int opcodes; Rust `view_api` + `components` paint Vidya.
//// Packing: payload * 16 + tag
////   0 meta · 1 title · 2 body · 4 button · 5 space · 6 status · 7 header
////   8/9 card · 10 slot · 12 md_body · 13 file_list · 14 commit_list
////  15 repo_tabs     interactive Files | Commits | Patches | Issues | Jobs
////
//// Space: 0=xs · 1=sm · 2=md · 3=lg · 4=xl · 5=page
////
//// Viewing layout:
////   chrome → meta card → repo tabs (files|commits|patches|issues|jobs)
////
//// Note: keep DSL helpers in this module — Wasm Gleam cannot call across
//// modules yet.

pub fn init() -> Int {
  0
}

pub fn update(model: Int, msg: Int) -> Int {
  case msg {
    1 -> 0
    2 -> 1
    3 -> 2
    _ -> model
  }
}

pub fn view_len(model: Int) -> Int {
  case model {
    1 -> 9
    2 -> 8
    _ -> 0
  }
}

pub fn view_at(model: Int, i: Int) -> Int {
  case model {
    1 -> viewing_at(i)
    2 -> error_at(i)
    _ -> 0
  }
}

fn viewing_at(i: Int) -> Int {
  case i {
    // Chrome
    0 -> header(1)
    1 -> space(1)
    2 -> button(0, 1, 4)
    3 -> space(3)
    // Meta
    4 -> card_open()
    5 -> meta()
    6 -> card_close()
    7 -> space(3)
    // Files | Commits (README selected in Files when present)
    8 -> repo_tabs()
    _ -> 0
  }
}

fn error_at(i: Int) -> Int {
  case i {
    0 -> header(1)
    1 -> space(2)
    2 -> card_open()
    3 -> title(5)
    4 -> slot(1, 5)
    5 -> space(2)
    6 -> button(1, 1, 4)
    7 -> card_close()
    _ -> 0
  }
}

// --- View DSL -------------------------------------------------------------

fn meta() -> Int {
  pack(0, 0)
}

fn header(code: Int) -> Int {
  pack(7, code)
}

fn title(code: Int) -> Int {
  pack(1, code)
}

fn button(primary: Int, msg: Int, label: Int) -> Int {
  pack(4, primary * 65_536 + msg * 256 + label)
}

fn space(sz: Int) -> Int {
  pack(5, sz)
}

fn card_open() -> Int {
  pack(8, 0)
}

fn card_close() -> Int {
  pack(9, 0)
}

fn slot(style: Int, id: Int) -> Int {
  pack(10, style * 256 + id)
}

fn repo_tabs() -> Int {
  pack(15, 0)
}

fn pack(tag: Int, payload: Int) -> Int {
  payload * 16 + tag
}
