//// Browse — Gleam owns screens / navigation / view opcodes.
////
//// Exports: browse__{init,update,view_len,view_at}
//// Int ↔ Wasm i64. No `main`. Avoid module `const`.
////
//// Model: screen
////   0 = enter RID · 1 = viewing · 2 = error
////
//// Msg (guest): 1 = Back
//// Msg (host-injected after Open): 2 = Loaded · 3 = Failed
//// Msg 0 = Open is handled on the host (fetch by RID) then injects 2/3.
////
//// Opcodes: payload * 16 + tag
////   1 title       payload = text_code
////   2 body        payload = text_code
////   4 button      payload = (primary<<16)|(msg<<8)|label_code
////   5 space       0=xs · 1=sm · 2=md
////   6 status      text_code
////   7 header      text_code
////   8/9 card open/close
////  10 slot        payload = (style<<8)|slot_id
////                 style 0=title · 1=body · 2=status · 3=dim
////  11 tree_list   payload = entry_count (slots TREE_BASE..)
////
//// Text codes: 1=Browse 2=enter body 3=Open 4=Back 5=Error 6=README 7=Files
//// Slots (host): 0=name 1=desc 2=rid 3=head 4=readme 5=error 6..=tree

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
    1 -> 16
    2 -> 8
    _ -> 7
  }
}

pub fn view_at(model: Int, i: Int) -> Int {
  case model {
    1 -> view_at_i(i)
    2 -> error_at(i)
    _ -> enter_at(i)
  }
}

fn enter_at(i: Int) -> Int {
  case i {
    0 -> header(1)
    1 -> space(2)
    2 -> card_open()
    3 -> body(2)
    4 -> space(2)
    5 -> button(1, 0, 3)
    6 -> card_close()
    _ -> 0
  }
}

fn view_at_i(i: Int) -> Int {
  case i {
    0 -> header(1)
    1 -> space(1)
    2 -> button(0, 1, 4)
    3 -> space(2)
    4 -> card_open()
    5 -> slot(0, 0)
    6 -> slot(1, 1)
    7 -> slot(2, 2)
    8 -> slot(3, 3)
    9 -> space(2)
    10 -> title(6)
    11 -> slot(1, 4)
    12 -> space(2)
    13 -> title(7)
    14 -> tree_list(24)
    15 -> card_close()
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

fn header(code: Int) -> Int {
  pack_op(7, code)
}

fn title(code: Int) -> Int {
  pack_op(1, code)
}

fn body(code: Int) -> Int {
  pack_op(2, code)
}

fn button(primary: Int, msg: Int, label: Int) -> Int {
  pack_op(4, primary * 65_536 + msg * 256 + label)
}

fn space(sz: Int) -> Int {
  pack_op(5, sz)
}

fn card_open() -> Int {
  pack_op(8, 0)
}

fn card_close() -> Int {
  pack_op(9, 0)
}

fn slot(style: Int, id: Int) -> Int {
  pack_op(10, style * 256 + id)
}

fn tree_list(n: Int) -> Int {
  pack_op(11, n)
}

fn pack_op(tag: Int, payload: Int) -> Int {
  payload * 16 + tag
}
