//! wasmtime session for the `browse` Gleam TEA guest.

use std::sync::{Mutex, OnceLock};

use wasmtime::{Engine, Instance, Module, Store, TypedFunc};

const BROWSE_WASM: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/browse.wasm"));

struct Session {
    store: Store<()>,
    init: TypedFunc<(), i64>,
    update: TypedFunc<(i64, i64), i64>,
    view_len: TypedFunc<i64, i64>,
    view_at: TypedFunc<(i64, i64), i64>,
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

        Ok(Self {
            init: get0(&mut store, "browse__init")?,
            update: get2(&mut store, "browse__update")?,
            view_len: get1(&mut store, "browse__view_len")?,
            view_at: get2(&mut store, "browse__view_at")?,
            store,
        })
    }
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
