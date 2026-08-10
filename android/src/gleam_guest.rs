//! wasmtime session for the `browse` Gleam TEA guest.
//!
//! Scalar TEA stays `i64`. When the guest is built with Gleam Wasm strings it
//! also exports `browse__view_text` (managed `String` pointer) plus linear
//! `memory` and UTF-8 inspect helpers — see [`read_guest_string`].

use std::sync::{Mutex, OnceLock};

use wasmtime::{Engine, Instance, Memory, Module, Store, TypedFunc};

const BROWSE_WASM: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/browse.wasm"));

/// Names tried for (byte_len, utf8_data_ptr) helpers on a managed String.
const STRING_LEN_EXPORTS: &[&str] = &[
    "gleam_string_utf8_len",
    "__gleam_string_len",
    "__gleam_string_byte_len",
    "string_len",
    "string_byte_size",
];
const STRING_DATA_EXPORTS: &[&str] = &[
    "gleam_string_utf8_ptr",
    "__gleam_string_data",
    "__gleam_string_as_bytes",
    "string_data",
    "string_utf8_ptr",
];

struct Session {
    store: Store<()>,
    /// Kept so the module stays instantiated with its exports.
    #[allow(dead_code)]
    instance: Instance,
    memory: Option<Memory>,
    init: TypedFunc<(), i64>,
    update: TypedFunc<(i64, i64), i64>,
    view_len: TypedFunc<i64, i64>,
    view_at: TypedFunc<(i64, i64), i64>,
    /// `browse__view_text(model, i) -> managed String ptr` (optional).
    view_text: Option<TypedFunc<(i64, i64), i32>>,
    /// `browse__label(id) -> managed String ptr` (optional).
    label: Option<TypedFunc<i32, i32>>,
    string_len: Option<TypedFunc<i32, i32>>,
    string_data: Option<TypedFunc<i32, i32>>,
}

static SESSION: OnceLock<Mutex<Session>> = OnceLock::new();

fn session() -> Result<&'static Mutex<Session>, String> {
    if let Some(s) = SESSION.get() {
        return Ok(s);
    }
    let loaded = Session::load()?;
    let _ = SESSION.set(Mutex::new(loaded));
    SESSION
        .get()
        .ok_or_else(|| "browse session missing after init".into())
}

impl Session {
    fn load() -> Result<Self, String> {
        let engine = Engine::default();
        let module =
            Module::new(&engine, BROWSE_WASM).map_err(|e| format!("parse browse.wasm: {e}"))?;
        let mut store = Store::new(&engine, ());
        let instance = Instance::new(&mut store, &module, &[])
            .map_err(|e| format!("instantiate browse.wasm: {e}"))?;

        let get0 = |store: &mut Store<()>, name: &str| -> Result<TypedFunc<(), i64>, String> {
            instance
                .get_typed_func(store, name)
                .map_err(|e| format!("export {name}: {e}"))
        };
        let get1 = |store: &mut Store<()>, name: &str| -> Result<TypedFunc<i64, i64>, String> {
            instance
                .get_typed_func(store, name)
                .map_err(|e| format!("export {name}: {e}"))
        };
        let get2 =
            |store: &mut Store<()>, name: &str| -> Result<TypedFunc<(i64, i64), i64>, String> {
                instance
                    .get_typed_func(store, name)
                    .map_err(|e| format!("export {name}: {e}"))
            };

        let memory = instance.get_memory(&mut store, "memory");
        let view_text = instance
            .get_typed_func::<(i64, i64), i32>(&mut store, "browse__view_text")
            .ok();
        let label = instance
            .get_typed_func::<i32, i32>(&mut store, "browse__label")
            .ok();
        let string_len = first_typed::<i32, i32>(&instance, &mut store, STRING_LEN_EXPORTS);
        let string_data = first_typed::<i32, i32>(&instance, &mut store, STRING_DATA_EXPORTS);

        if view_text.is_some() || label.is_some() {
            eprintln!(
                "browse gleam_guest: view_text={} label={} memory={} str_helpers={}",
                view_text.is_some(),
                label.is_some(),
                memory.is_some(),
                string_len.is_some() && string_data.is_some()
            );
        }

        Ok(Self {
            init: get0(&mut store, "browse__init")?,
            update: get2(&mut store, "browse__update")?,
            view_len: get1(&mut store, "browse__view_len")?,
            view_at: get2(&mut store, "browse__view_at")?,
            view_text,
            label,
            string_len,
            string_data,
            memory,
            instance,
            store,
        })
    }
}

fn first_typed<Params, Results>(
    instance: &Instance,
    store: &mut Store<()>,
    names: &[&str],
) -> Option<TypedFunc<Params, Results>>
where
    Params: wasmtime::WasmParams,
    Results: wasmtime::WasmResults,
{
    for name in names {
        if let Ok(f) = instance.get_typed_func::<Params, Results>(&mut *store, name) {
            return Some(f);
        }
    }
    None
}

pub fn has_view_text() -> bool {
    session()
        .ok()
        .and_then(|s| s.lock().ok())
        .map(|g| g.view_text.is_some())
        .unwrap_or(false)
}

pub fn init() -> Result<i64, String> {
    let s = session()?;
    let mut g = s.lock().map_err(|_| "browse session poisoned".to_string())?;
    let f = g.init.clone();
    f.call(&mut g.store, ())
        .map_err(|e| format!("browse__init: {e}"))
}

pub fn update(model: i64, msg: i64) -> Result<i64, String> {
    let s = session()?;
    let mut g = s.lock().map_err(|_| "browse session poisoned".to_string())?;
    let f = g.update.clone();
    f.call(&mut g.store, (model, msg))
        .map_err(|e| format!("browse__update: {e}"))
}

pub fn view_len(model: i64) -> Result<i64, String> {
    let s = session()?;
    let mut g = s.lock().map_err(|_| "browse session poisoned".to_string())?;
    let f = g.view_len.clone();
    f.call(&mut g.store, model)
        .map_err(|e| format!("browse__view_len: {e}"))
}

pub fn view_at(model: i64, i: i64) -> Result<i64, String> {
    let s = session()?;
    let mut g = s.lock().map_err(|_| "browse session poisoned".to_string())?;
    let f = g.view_at.clone();
    f.call(&mut g.store, (model, i))
        .map_err(|e| format!("browse__view_at: {e}"))
}

/// Guest-owned copy for opcode `i`, when `browse__view_text` is exported.
///
/// Empty string means the op has no guest text (host vocab / slots still apply).
pub fn view_text(model: i64, i: i64) -> Result<Option<String>, String> {
    let s = session()?;
    let mut g = s.lock().map_err(|_| "browse session poisoned".to_string())?;
    let Some(view_text) = g.view_text.clone() else {
        return Ok(None);
    };
    let ptr = view_text
        .call(&mut g.store, (model, i))
        .map_err(|e| format!("browse__view_text: {e}"))?;
    if ptr == 0 {
        return Ok(Some(String::new()));
    }
    let text = read_guest_string(&mut g, ptr)?;
    Ok(Some(text))
}

/// Shared chrome label from the guest (`browse__label`), when exported.
pub fn label(id: i64) -> Result<Option<String>, String> {
    let s = session()?;
    let mut g = s.lock().map_err(|_| "browse session poisoned".to_string())?;
    let Some(label_f) = g.label.clone() else {
        return Ok(None);
    };
    let ptr = label_f
        .call(&mut g.store, id as i32)
        .map_err(|e| format!("browse__label: {e}"))?;
    if ptr == 0 {
        return Ok(Some(String::new()));
    }
    let text = read_guest_string(&mut g, ptr)?;
    Ok(Some(text))
}

pub fn has_label() -> bool {
    session()
        .ok()
        .and_then(|s| s.lock().ok())
        .map(|g| g.label.is_some())
        .unwrap_or(false)
}

fn read_guest_string(g: &mut Session, ptr: i32) -> Result<String, String> {
    if ptr <= 0 {
        return Ok(String::new());
    }

    // Preferred: runtime helpers + linear memory.
    if let (Some(len_f), Some(data_f), Some(memory)) =
        (g.string_len.clone(), g.string_data.clone(), g.memory)
    {
        let len = len_f
            .call(&mut g.store, ptr)
            .map_err(|e| format!("string_len: {e}"))?;
        let data_ptr = data_f
            .call(&mut g.store, ptr)
            .map_err(|e| format!("string_data: {e}"))?;
        return copy_utf8(&memory, &g.store, data_ptr, len);
    }

    let memory = g
        .memory
        .ok_or_else(|| {
            "browse__view_text returned a String ptr but guest exports no `memory` \
             (rebuild with Gleam Wasm strings)"
                .to_string()
        })?;

    // Fallback layouts in guest linear memory (little-endian).
    // 1) length-prefixed: [u32 len][utf8…]
    if let Ok(s) = read_len_prefixed(&memory, &g.store, ptr) {
        return Ok(s);
    }
    // 2) tagged object: [u32 tag=1][u32 len][utf8…] (Regulus-style)
    if let Ok(s) = read_tagged_string(&memory, &g.store, ptr) {
        return Ok(s);
    }

    Err(format!(
        "cannot decode Gleam String at ptr {ptr}: need gleam_string_utf8_len/ptr \
         (or __gleam_string_len/data) exports, or a length-prefixed UTF-8 layout"
    ))
}

fn copy_utf8(
    memory: &Memory,
    store: &Store<()>,
    data_ptr: i32,
    len: i32,
) -> Result<String, String> {
    if len < 0 {
        return Err(format!("negative string length {len}"));
    }
    if data_ptr < 0 {
        return Err(format!("negative string data ptr {data_ptr}"));
    }
    let start = data_ptr as usize;
    let end = start
        .checked_add(len as usize)
        .ok_or_else(|| "string span overflow".to_string())?;
    let data = memory.data(store);
    let bytes = data
        .get(start..end)
        .ok_or_else(|| format!("string bytes [{start}..{end}) out of memory"))?;
    String::from_utf8(bytes.to_vec()).map_err(|e| format!("guest string utf8: {e}"))
}

fn read_len_prefixed(memory: &Memory, store: &Store<()>, ptr: i32) -> Result<String, String> {
    let data = memory.data(store);
    let p = ptr as usize;
    let len_bytes = data
        .get(p..p + 4)
        .ok_or_else(|| "len prefix OOB".to_string())?;
    let len = u32::from_le_bytes(len_bytes.try_into().unwrap()) as usize;
    // Reject absurd lengths (guards random memory).
    if len > 1_000_000 {
        return Err("len prefix too large".into());
    }
    let start = p + 4;
    let end = start
        .checked_add(len)
        .ok_or_else(|| "len prefix span overflow".to_string())?;
    let bytes = data
        .get(start..end)
        .ok_or_else(|| "len prefix bytes OOB".to_string())?;
    String::from_utf8(bytes.to_vec()).map_err(|e| format!("len-prefixed utf8: {e}"))
}

fn read_tagged_string(memory: &Memory, store: &Store<()>, ptr: i32) -> Result<String, String> {
    let data = memory.data(store);
    let p = ptr as usize;
    let header = data
        .get(p..p + 8)
        .ok_or_else(|| "tagged header OOB".to_string())?;
    let tag = u32::from_le_bytes(header[0..4].try_into().unwrap());
    let len = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
    if tag != 1 {
        return Err(format!("not a string tag ({tag})"));
    }
    if len > 1_000_000 {
        return Err("tagged string too large".into());
    }
    let start = p + 8;
    let end = start
        .checked_add(len)
        .ok_or_else(|| "tagged span overflow".to_string())?;
    let bytes = data
        .get(start..end)
        .ok_or_else(|| "tagged bytes OOB".to_string())?;
    String::from_utf8(bytes.to_vec()).map_err(|e| format!("tagged utf8: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_text_returns_guest_strings() {
        let model = init().expect("init");
        assert_eq!(model, 0);
        assert_eq!(view_len(model).expect("enter len"), 8);
        let hint = view_text(model, 2).expect("text").expect("some");
        assert!(hint.contains("Radicle ID"), "{hint}");
        assert_eq!(view_text(model, 4).unwrap().unwrap(), "Filter by typing in the RID field.");
        assert_eq!(view_text(model, 6).unwrap().unwrap(), "Local only — Browse does not fetch from the network.");

        assert!(has_label());
        assert_eq!(label(2).unwrap().unwrap(), "Open");
        assert_eq!(label(5).unwrap().unwrap(), "Files");
        assert_eq!(label(35).unwrap().unwrap(), "Vidya shell · Gleam screens · Radicle storage");

        let model = update(model, 2).expect("loaded");
        assert_eq!(model, 1);
        assert_eq!(view_len(model).expect("len"), 16);
        assert_eq!(view_text(model, 0).unwrap().unwrap(), "Browse");
        assert_eq!(view_text(model, 9).unwrap().unwrap(), "Repository");

        assert_eq!(update(0, 3).unwrap(), 2);
        assert_eq!(view_len(2).unwrap(), 14);

        assert_eq!(update(0, 4).unwrap(), 3);
        assert_eq!(view_len(3).unwrap(), 11);

        // Help/About msgs are gone — unknown msgs leave the model unchanged.
        assert_eq!(update(0, 5).unwrap(), 0);
        assert_eq!(update(0, 6).unwrap(), 0);
    }
}
