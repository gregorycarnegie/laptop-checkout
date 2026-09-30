//! SQLite, running inside the WebAssembly bundle via rusqlite.
//!
//! The database lives in memory while the app is open. After every change it
//! is serialized and handed to `js/storage.js`, which writes it to IndexedDB on
//! this PC and, when the user has linked one, to a real `.sqlite` file.
//!
//! Queries take their parameters as a JSON array and decode rows into any
//! `serde::Deserialize` type by column name, which keeps `repo.rs` compact.

use std::cell::{Cell, RefCell};
use std::time::Duration;

use leptos::prelude::*;
use leptos::task::spawn_local;
use rusqlite::types::{Value as SqlValue, ValueRef};
use rusqlite::{params_from_iter, Connection, MAIN_DB};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{Map, Number, Value};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/js/storage.js")]
extern "C" {
    #[wasm_bindgen(js_name = fileInfoJson)]
    fn js_file_info_json() -> String;
    #[wasm_bindgen(catch, js_name = loadInitial)]
    async fn js_load_initial() -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = persist)]
    async fn js_persist(bytes: js_sys::Uint8Array) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = createFile)]
    async fn js_create_file(bytes: js_sys::Uint8Array) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = openFile)]
    async fn js_open_file() -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = reconnectFile)]
    async fn js_reconnect_file() -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = disconnectFile)]
    async fn js_disconnect_file() -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = readUpload)]
    async fn js_read_upload(file: web_sys::File) -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = downloadBytes)]
    fn js_download_bytes(bytes: js_sys::Uint8Array, name: &str) -> Result<(), JsValue>;
    #[wasm_bindgen(js_name = installUnloadGuard)]
    fn js_install_unload_guard(flush: &Closure<dyn Fn()>, has_unsaved: &Closure<dyn Fn() -> bool>);
    #[wasm_bindgen(js_name = copyText)]
    async fn js_copy_text(text: &str) -> JsValue;
}

/// Where and whether the database has been saved.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DbStatus {
    pub fs_supported: bool,
    pub file_name: Option<String>,
    pub file_connected: bool,
    pub file_needs_permission: bool,
    pub dirty: bool,
    pub saving: bool,
    pub last_saved: Option<i64>,
    pub error: Option<String>,
}

#[derive(Deserialize, Default)]
struct FileInfo {
    fs_supported: bool,
    file_name: Option<String>,
    file_connected: bool,
    file_needs_permission: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ExecResult {
    #[allow(dead_code)]
    pub changes: usize,
    pub last_id: i64,
}

#[derive(Default)]
struct SaveState {
    generation: u64,
    dirty: bool,
    saving: bool,
    last_saved: Option<i64>,
    error: Option<String>,
}

thread_local! {
    static CONN: RefCell<Option<Connection>> = const { RefCell::new(None) };
    static SAVE: RefCell<SaveState> = RefCell::new(SaveState::default());
    /// Bumped after every write so reactive queries re-run.
    static REVISION: Cell<Option<RwSignal<u64>>> = const { Cell::new(None) };
    static STATUS: Cell<Option<RwSignal<DbStatus>>> = const { Cell::new(None) };
}

pub fn set_signals(revision: RwSignal<u64>, status: RwSignal<DbStatus>) {
    REVISION.with(|r| r.set(Some(revision)));
    STATUS.with(|s| s.set(Some(status)));
    refresh_status();
}

pub fn bump() {
    if let Some(sig) = REVISION.with(Cell::get) {
        sig.update(|v| *v += 1);
    }
}

fn refresh_status() {
    let Some(sig) = STATUS.with(Cell::get) else { return };
    let info: FileInfo = serde_json::from_str(&js_file_info_json()).unwrap_or_default();
    let status = SAVE.with_borrow(|s| DbStatus {
        fs_supported: info.fs_supported,
        file_name: info.file_name,
        file_connected: info.file_connected,
        file_needs_permission: info.file_needs_permission,
        dirty: s.dirty,
        saving: s.saving,
        last_saved: s.last_saved,
        error: s.error.clone(),
    });
    sig.set(status);
}

pub fn js_error(e: JsValue) -> String {
    if let Some(s) = e.as_string() {
        return s;
    }
    js_sys::Reflect::get(&e, &JsValue::from_str("message"))
        .ok()
        .and_then(|m| m.as_string())
        .unwrap_or_else(|| format!("{e:?}"))
}

fn log(msg: &str) {
    web_sys::console::error_1(&JsValue::from_str(msg));
}

// ---------------------------------------------------------------- connection

fn open(bytes: &[u8]) -> Result<Connection, String> {
    let mut conn = Connection::open_in_memory().map_err(|e| e.to_string())?;
    if !bytes.is_empty() {
        conn.deserialize_read_exact(MAIN_DB, bytes, bytes.len(), false)
            .map_err(|e| e.to_string())?;
    }
    // Touch the schema so a file that isn't SQLite fails here, not later.
    conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0))
        .map_err(|_| "That file isn't a Laptop Checkout (SQLite) database.".to_string())?;
    Ok(conn)
}

/// Swaps in a different database, e.g. one opened from a file.
pub fn replace(bytes: &[u8]) -> Result<(), String> {
    let conn = open(bytes)?;
    CONN.with_borrow_mut(|c| *c = Some(conn));
    Ok(())
}

fn with_conn<T>(f: impl FnOnce(&Connection) -> Result<T, String>) -> Result<T, String> {
    CONN.with_borrow(|c| match c {
        Some(conn) => f(conn),
        None => Err("The database isn't open yet.".into()),
    })
}

fn serialize() -> Result<Vec<u8>, String> {
    with_conn(|c| {
        c.serialize(MAIN_DB)
            .map(|data| data.to_vec())
            .map_err(|e| e.to_string())
    })
}

/// Loads the saved database. Returns `true` for a brand-new one.
pub async fn init() -> Result<bool, String> {
    let saved = js_load_initial().await.map_err(js_error)?;
    let bytes = (!saved.is_null() && !saved.is_undefined())
        .then(|| js_sys::Uint8Array::new(&saved).to_vec())
        .unwrap_or_default();
    let fresh = bytes.is_empty();
    replace(&bytes)?;

    let flush = Closure::<dyn Fn()>::new(|| {
        if SAVE.with_borrow(|s| s.dirty) {
            spawn_local(save());
        }
    });
    let has_unsaved = Closure::<dyn Fn() -> bool>::new(|| SAVE.with_borrow(|s| s.dirty || s.saving));
    js_install_unload_guard(&flush, &has_unsaved);
    flush.forget();
    has_unsaved.forget();

    refresh_status();
    Ok(fresh)
}

// ---------------------------------------------------------------- queries

fn to_sql(v: &Value) -> SqlValue {
    match v {
        Value::Null => SqlValue::Null,
        Value::Bool(b) => SqlValue::Integer(*b as i64),
        Value::Number(n) => match n.as_i64() {
            Some(i) => SqlValue::Integer(i),
            None => SqlValue::Real(n.as_f64().unwrap_or(0.0)),
        },
        Value::String(s) => SqlValue::Text(s.clone()),
        other => SqlValue::Text(other.to_string()),
    }
}

fn params(v: &Value) -> Vec<SqlValue> {
    match v {
        Value::Array(items) => items.iter().map(to_sql).collect(),
        Value::Null => Vec::new(),
        other => vec![to_sql(other)],
    }
}

fn to_json(v: ValueRef<'_>) -> Value {
    match v {
        ValueRef::Null | ValueRef::Blob(_) => Value::Null,
        ValueRef::Integer(i) => Value::from(i),
        ValueRef::Real(f) => Number::from_f64(f).map(Value::Number).unwrap_or(Value::Null),
        ValueRef::Text(t) => Value::String(String::from_utf8_lossy(t).into_owned()),
    }
}

fn rows(sql: &str, args: &Value) -> Result<Vec<Value>, String> {
    with_conn(|c| {
        let mut stmt = c.prepare_cached(sql).map_err(|e| e.to_string())?;
        let names: Vec<String> = stmt.column_names().into_iter().map(String::from).collect();
        let mut out = Vec::new();
        let mut rows = stmt
            .query(params_from_iter(params(args)))
            .map_err(|e| e.to_string())?;
        while let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let mut obj = Map::with_capacity(names.len());
            for (i, name) in names.iter().enumerate() {
                let v = row.get_ref(i).map_err(|e| e.to_string())?;
                obj.insert(name.clone(), to_json(v));
            }
            out.push(Value::Object(obj));
        }
        Ok(out)
    })
}

pub fn query<T: DeserializeOwned>(sql: &str, args: Value) -> Vec<T> {
    match rows(sql, &args) {
        Ok(rows) => rows
            .into_iter()
            .filter_map(|r| {
                serde_json::from_value(r)
                    .map_err(|e| log(&format!("Couldn't read a row ({e}) for: {sql}")))
                    .ok()
            })
            .collect(),
        Err(e) => {
            log(&format!("{e} in: {sql}"));
            Vec::new()
        }
    }
}

pub fn query_one<T: DeserializeOwned>(sql: &str, args: Value) -> Option<T> {
    query(sql, args).into_iter().next()
}

pub fn scalar(sql: &str, args: Value) -> i64 {
    #[derive(Deserialize)]
    struct N {
        n: Option<i64>,
    }
    query_one::<N>(sql, args).and_then(|r| r.n).unwrap_or(0)
}

/// Runs a write without refreshing the UI. Use inside `transaction`.
pub fn exec_quiet(sql: &str, args: Value) -> Result<ExecResult, String> {
    let r = with_conn(|c| {
        let mut stmt = c.prepare_cached(sql).map_err(|e| e.to_string())?;
        let changes = stmt
            .execute(params_from_iter(params(&args)))
            .map_err(|e| e.to_string())?;
        Ok(ExecResult { changes, last_id: c.last_insert_rowid() })
    })?;
    mark_dirty();
    Ok(r)
}

pub fn exec(sql: &str, args: Value) -> Result<ExecResult, String> {
    let r = exec_quiet(sql, args)?;
    bump();
    Ok(r)
}

pub fn run_script(sql: &str) -> Result<(), String> {
    with_conn(|c| c.execute_batch(sql).map_err(|e| e.to_string()))?;
    mark_dirty();
    Ok(())
}

/// Runs `f` inside BEGIN/COMMIT, rolling back if it returns an error.
pub fn transaction<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    run_script("BEGIN")?;
    match f() {
        Ok(v) => {
            run_script("COMMIT")?;
            bump();
            Ok(v)
        }
        Err(e) => {
            let _ = run_script("ROLLBACK");
            Err(e)
        }
    }
}

// ---------------------------------------------------------------- saving

fn mark_dirty() {
    let generation = SAVE.with_borrow_mut(|s| {
        s.dirty = true;
        s.generation += 1;
        s.generation
    });
    refresh_status();
    set_timeout(
        move || {
            if SAVE.with_borrow(|s| s.generation) == generation {
                spawn_local(save());
            }
        },
        Duration::from_millis(400),
    );
}

async fn save() {
    let busy = SAVE.with_borrow_mut(|s| {
        let busy = s.saving;
        if !busy {
            s.saving = true;
            s.dirty = false;
        }
        busy
    });
    if busy {
        mark_dirty();
        return;
    }
    refresh_status();
    let result = match serialize() {
        Ok(bytes) => js_persist(js_sys::Uint8Array::from(bytes.as_slice()))
            .await
            .map_err(js_error),
        Err(e) => Err(e),
    };
    SAVE.with_borrow_mut(|s| {
        s.saving = false;
        match result {
            Ok(_) => {
                s.last_saved = Some(crate::time::now());
                s.error = None;
            }
            Err(e) => {
                s.dirty = true;
                s.error = Some(format!("Last save failed: {e}"));
            }
        }
    });
    refresh_status();
}

pub async fn save_now() {
    save().await;
}

// ---------------------------------------------------------------- files on this PC

/// Creates a new `.sqlite` file on the PC and keeps writing every change into it.
pub async fn create_file() -> Result<(), String> {
    let bytes = serialize()?;
    let r = js_create_file(js_sys::Uint8Array::from(bytes.as_slice()))
        .await
        .map(|_| ())
        .map_err(js_error);
    if r.is_ok() {
        SAVE.with_borrow_mut(|s| s.last_saved = Some(crate::time::now()));
    }
    refresh_status();
    r
}

/// Opens an existing `.sqlite` file and uses it from now on.
pub async fn open_file() -> Result<(), String> {
    let bytes = js_open_file().await.map_err(js_error)?;
    let r = replace(&js_sys::Uint8Array::new(&bytes).to_vec());
    refresh_status();
    r
}

/// Re-grants access to the linked file after a browser restart.
pub async fn reconnect_file() -> Result<bool, String> {
    let bytes = js_reconnect_file().await.map_err(js_error)?;
    let ok = !bytes.is_null();
    if ok {
        replace(&js_sys::Uint8Array::new(&bytes).to_vec())?;
    }
    refresh_status();
    Ok(ok)
}

pub async fn disconnect_file() -> Result<(), String> {
    let r = js_disconnect_file().await.map(|_| ()).map_err(js_error);
    refresh_status();
    r
}

/// Replaces the current database with a backup the user uploaded.
pub async fn restore_upload(file: web_sys::File) -> Result<(), String> {
    let bytes = js_read_upload(file).await.map_err(js_error)?;
    replace(&js_sys::Uint8Array::new(&bytes).to_vec())
}

pub fn download(name: &str) -> Result<(), String> {
    let bytes = serialize()?;
    js_download_bytes(js_sys::Uint8Array::from(bytes.as_slice()), name).map_err(js_error)
}

pub async fn copy_text(text: &str) -> bool {
    js_copy_text(text).await.as_bool().unwrap_or(false)
}

/// True when the user dismissed a file picker, which isn't worth an error message.
pub fn is_cancel(err: &str) -> bool {
    err.contains("aborted") || err.contains("AbortError") || err.contains("cancel")
}
