//! Moves the database's bytes to and from the user's PC, all in Rust:
//!
//! * IndexedDB on this computer (always, as an autosave), and
//! * a real `.sqlite` file the user picks, through the File System Access API
//!   (Chrome and Edge).
//!
//! Also downloads, uploads, the clipboard and the "unsaved changes" guard.
//! Everything here needs a browser, so it's exercised by the end-to-end tests
//! rather than unit tests.
#![cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]

use std::cell::{Cell, RefCell};

use js_sys::{Array, Function, Promise, Uint8Array};
use wasm_bindgen::{prelude::*, JsCast};
use wasm_bindgen_futures::JsFuture;
use web_sys::{IdbDatabase, IdbRequest, IdbTransaction, IdbTransactionMode};

use crate::persist::FileInfo;

// The File System Access API and async clipboard are still marked unstable in
// web-sys, so we declare just the parts we use.
#[wasm_bindgen]
extern "C" {
    /// `FileSystemFileHandle`
    pub type FileHandle;
    #[wasm_bindgen(method, getter)]
    fn name(this: &FileHandle) -> String;
    #[wasm_bindgen(method, js_name = getFile)]
    fn get_file(this: &FileHandle) -> Promise;
    #[wasm_bindgen(method, catch, js_name = createWritable)]
    fn create_writable(this: &FileHandle) -> Result<Promise, JsValue>;
    #[wasm_bindgen(method, catch, js_name = queryPermission)]
    fn query_permission(this: &FileHandle, descriptor: &JsValue) -> Result<Promise, JsValue>;
    #[wasm_bindgen(method, catch, js_name = requestPermission)]
    fn request_permission(this: &FileHandle, descriptor: &JsValue) -> Result<Promise, JsValue>;

    /// `FileSystemWritableFileStream`
    type WritableStream;
    #[wasm_bindgen(method, catch)]
    fn write(this: &WritableStream, data: &JsValue) -> Result<Promise, JsValue>;
    #[wasm_bindgen(method, catch)]
    fn close(this: &WritableStream) -> Result<Promise, JsValue>;

    #[wasm_bindgen(catch, js_namespace = window, js_name = showSaveFilePicker)]
    fn show_save_file_picker(options: &JsValue) -> Result<Promise, JsValue>;
    #[wasm_bindgen(catch, js_namespace = window, js_name = showOpenFilePicker)]
    fn show_open_file_picker(options: &JsValue) -> Result<Promise, JsValue>;

    #[wasm_bindgen(catch, js_namespace = ["navigator", "clipboard"], js_name = writeText)]
    fn clipboard_write_text(text: &str) -> Result<Promise, JsValue>;
}

const IDB_NAME: &str = "laptop-checkout";
const IDB_STORE: &str = "kv";
const KEY_DB: &str = "db";
const KEY_HANDLE: &str = "handle";
const PICKER_TYPES: &str =
    r#"[{"description":"SQLite database","accept":{"application/vnd.sqlite3":[".sqlite",".sqlite3",".db"]}}]"#;

thread_local! {
    static FILE: RefCell<Option<FileHandle>> = const { RefCell::new(None) };
    static WRITABLE: Cell<bool> = const { Cell::new(false) };
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

async fn wait(p: Promise) -> Result<JsValue, String> {
    JsFuture::from(p).await.map_err(js_error)
}

fn json(text: &str) -> JsValue {
    js_sys::JSON::parse(text).unwrap_or(JsValue::UNDEFINED)
}

fn readwrite() -> JsValue {
    json(r#"{"mode":"readwrite"}"#)
}

// ---------------------------------------------------------------- IndexedDB

/// A promise that settles when `req` succeeds or fails.
fn request_done(req: &IdbRequest) -> Promise {
    Promise::new(&mut |resolve: Function, reject: Function| {
        let r = req.clone();
        let ok = Closure::once_into_js(move || {
            let _ = resolve.call1(&JsValue::NULL, &r.result().unwrap_or(JsValue::UNDEFINED));
        });
        let fail = Closure::once_into_js(move || {
            let _ = reject.call1(&JsValue::NULL, &JsValue::from_str("IndexedDB request failed"));
        });
        req.set_onsuccess(Some(ok.unchecked_ref()));
        req.set_onerror(Some(fail.unchecked_ref()));
    })
}

/// A promise that settles when `tx` commits or fails.
fn transaction_done(tx: &IdbTransaction) -> Promise {
    Promise::new(&mut |resolve: Function, reject: Function| {
        let reject2 = reject.clone();
        let ok = Closure::once_into_js(move || {
            let _ = resolve.call0(&JsValue::NULL);
        });
        let fail = Closure::once_into_js(move || {
            let _ = reject.call1(&JsValue::NULL, &JsValue::from_str("IndexedDB write failed"));
        });
        let abort = Closure::once_into_js(move || {
            let _ = reject2.call1(&JsValue::NULL, &JsValue::from_str("IndexedDB write was aborted"));
        });
        tx.set_oncomplete(Some(ok.unchecked_ref()));
        tx.set_onerror(Some(fail.unchecked_ref()));
        tx.set_onabort(Some(abort.unchecked_ref()));
    })
}

async fn open_idb() -> Result<IdbDatabase, String> {
    let factory = web_sys::window()
        .ok_or("No browser window")?
        .indexed_db()
        .map_err(js_error)?
        .ok_or("This browser has no IndexedDB storage.")?;
    let req = factory.open_with_u32(IDB_NAME, 1).map_err(js_error)?;
    let upgrade_req = req.clone();
    let upgrade = Closure::once_into_js(move || {
        if let Ok(db) = upgrade_req.result() {
            let _ = db.unchecked_into::<IdbDatabase>().create_object_store(IDB_STORE);
        }
    });
    req.set_onupgradeneeded(Some(upgrade.unchecked_ref()));
    Ok(wait(request_done(&req)).await?.unchecked_into())
}

async fn idb_get(key: &str) -> Option<JsValue> {
    let db = open_idb().await.ok()?;
    let v = async {
        let tx = db.transaction_with_str(IDB_STORE).ok()?;
        let req = tx.object_store(IDB_STORE).ok()?.get(&JsValue::from_str(key)).ok()?;
        wait(request_done(&req)).await.ok()
    }
    .await;
    // Open connections block upgrades and deletes, so never keep one.
    db.close();
    v.filter(|v| !v.is_undefined() && !v.is_null())
}

async fn idb_write(key: &str, value: Option<&JsValue>) -> Result<(), String> {
    let db = open_idb().await?;
    let r = async {
        let tx = db.transaction_with_str_and_mode(IDB_STORE, IdbTransactionMode::Readwrite).map_err(js_error)?;
        let store = tx.object_store(IDB_STORE).map_err(js_error)?;
        let key = JsValue::from_str(key);
        match value {
            Some(v) => store.put_with_key(v, &key).map(|_| ()),
            None => store.delete(&key).map(|_| ()),
        }
        .map_err(js_error)?;
        wait(transaction_done(&tx)).await.map(|_| ())
    }
    .await;
    db.close();
    r
}

// ---------------------------------------------------------------- the linked file

fn fs_supported() -> bool {
    web_sys::window()
        .map(|w| js_sys::Reflect::has(&w, &JsValue::from_str("showSaveFilePicker")).unwrap_or(false))
        .unwrap_or(false)
}

pub fn file_info() -> FileInfo {
    FileInfo {
        fs_supported: fs_supported(),
        file_name: FILE.with_borrow(|f| f.as_ref().map(FileHandle::name)),
        writable: WRITABLE.with(Cell::get),
    }
}

/// Another reference to the same JavaScript handle.
fn share(h: &FileHandle) -> FileHandle {
    let v: &JsValue = h.as_ref();
    v.clone().unchecked_into()
}

fn current_file() -> Option<FileHandle> {
    FILE.with_borrow(|f| f.as_ref().map(share))
}

fn set_file(handle: Option<FileHandle>, writable: bool) {
    FILE.with_borrow_mut(|f| *f = handle);
    WRITABLE.with(|w| w.set(writable));
}

async fn read_blob(blob: JsValue) -> Result<Vec<u8>, String> {
    let blob: web_sys::Blob = blob.unchecked_into();
    let buf = wait(blob.array_buffer()).await?;
    Ok(Uint8Array::new(&buf).to_vec())
}

async fn read_handle(handle: &FileHandle) -> Result<Vec<u8>, String> {
    read_blob(wait(handle.get_file()).await?).await
}

async fn permission(handle: &FileHandle, ask: bool) -> bool {
    let p = if ask { handle.request_permission(&readwrite()) } else { handle.query_permission(&readwrite()) };
    match p {
        Ok(p) => wait(p).await.ok().and_then(|v| v.as_string()).as_deref() == Some("granted"),
        Err(_) => false,
    }
}

/// The saved database: the linked file if it's still allowed, else the
/// IndexedDB copy, else `None` for a first run.
pub async fn load_initial() -> Result<Option<Vec<u8>>, String> {
    if let Some(h) = idb_get(KEY_HANDLE).await {
        let handle: FileHandle = h.unchecked_into();
        let writable = permission(&handle, false).await;
        let bytes = if writable { read_handle(&handle).await.ok() } else { None };
        set_file(Some(handle), writable && bytes.is_some());
        if bytes.is_some() {
            return Ok(bytes);
        }
    }
    match idb_get(KEY_DB).await {
        Some(v) => Ok(Some(Uint8Array::new(&v).to_vec())),
        None => Ok(None),
    }
}

async fn write_file(handle: &FileHandle, bytes: &Uint8Array) -> Result<(), String> {
    let stream: WritableStream = wait(handle.create_writable().map_err(js_error)?).await?.unchecked_into();
    wait(stream.write(bytes).map_err(js_error)?).await?;
    wait(stream.close().map_err(js_error)?).await?;
    Ok(())
}

/// Saves the bytes to IndexedDB, and to the linked file when there is one.
pub async fn persist(bytes: &[u8]) -> Result<(), String> {
    let data = Uint8Array::from(bytes);
    idb_write(KEY_DB, Some(&data)).await?;
    if WRITABLE.with(Cell::get) {
        if let Some(handle) = current_file() {
            write_file(&handle, &data).await?;
        }
    }
    Ok(())
}

fn picker_options(save: bool) -> JsValue {
    let extra = if save { r#","suggestedName":"laptop-checkout.sqlite""# } else { r#","multiple":false"# };
    json(&format!(r#"{{"types":{PICKER_TYPES}{extra}}}"#))
}

/// Asks where to create a new `.sqlite` file, then writes `bytes` into it.
pub async fn create_file(bytes: &[u8]) -> Result<(), String> {
    let handle: FileHandle =
        wait(show_save_file_picker(&picker_options(true)).map_err(js_error)?).await?.unchecked_into();
    set_file(Some(share(&handle)), true);
    idb_write(KEY_HANDLE, Some(&handle)).await?;
    persist(bytes).await
}

/// Lets the user pick an existing `.sqlite` file and returns its bytes.
pub async fn open_file() -> Result<Vec<u8>, String> {
    let picked = wait(show_open_file_picker(&picker_options(false)).map_err(js_error)?).await?;
    let handle: FileHandle = Array::from(&picked).get(0).unchecked_into();
    let writable = permission(&handle, true).await;
    let bytes = read_handle(&handle).await?;
    idb_write(KEY_HANDLE, Some(&handle)).await?;
    set_file(Some(handle), writable);
    Ok(bytes)
}

/// Browsers ask again for file access after a restart; call this from a click.
pub async fn reconnect_file() -> Result<Option<Vec<u8>>, String> {
    let Some(handle) = current_file() else { return Ok(None) };
    if !permission(&handle, true).await {
        return Ok(None);
    }
    let bytes = read_handle(&handle).await?;
    set_file(Some(handle), true);
    Ok(Some(bytes))
}

pub async fn disconnect_file() -> Result<(), String> {
    set_file(None, false);
    idb_write(KEY_HANDLE, None).await
}

pub async fn read_upload(file: web_sys::File) -> Result<Vec<u8>, String> {
    read_blob(file.into()).await
}

// ---------------------------------------------------------------- downloads, clipboard, unload

pub fn download(bytes: &[u8], name: &str) -> Result<(), String> {
    let document = web_sys::window().and_then(|w| w.document()).ok_or("No document")?;
    let parts = Array::of1(&Uint8Array::from(bytes));
    let props = web_sys::BlobPropertyBag::new();
    props.set_type("application/vnd.sqlite3");
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &props).map_err(js_error)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(js_error)?;
    let a: web_sys::HtmlAnchorElement = document.create_element("a").map_err(js_error)?.unchecked_into();
    a.set_href(&url);
    a.set_download(name);
    if let Some(body) = document.body() {
        let _ = body.append_child(&a);
        a.click();
        a.remove();
    }
    let revoke = Closure::once_into_js(move || {
        let _ = web_sys::Url::revoke_object_url(&url);
    });
    if let Some(w) = web_sys::window() {
        let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(revoke.unchecked_ref(), 2000);
    }
    Ok(())
}

/// Copies text, falling back to a hidden textarea where the Clipboard API is refused.
pub async fn copy_text(text: &str) -> bool {
    if let Ok(p) = clipboard_write_text(text) {
        if wait(p).await.is_ok() {
            return true;
        }
    }
    let Some(document) = web_sys::window().and_then(|w| w.document()) else { return false };
    let Ok(el) = document.create_element("textarea") else { return false };
    let ta: web_sys::HtmlTextAreaElement = el.unchecked_into();
    ta.set_value(text);
    let _ = ta.set_attribute("readonly", "");
    let _ = ta.set_attribute("style", "position:fixed;opacity:0");
    let Some(body) = document.body() else { return false };
    let _ = body.append_child(&ta);
    ta.select();
    let ok = document.unchecked_ref::<web_sys::HtmlDocument>().exec_command("copy").unwrap_or(false);
    ta.remove();
    ok
}

/// Calls `flush` when the tab is hidden, and warns before closing with unsaved changes.
pub fn install_unload_guard(flush: impl Fn() + 'static, has_unsaved: impl Fn() -> bool + 'static) {
    let Some(window) = web_sys::window() else { return };
    let flush = std::rc::Rc::new(flush);
    if let Some(document) = window.document() {
        let doc = document.clone();
        let f = flush.clone();
        let on_hide = Closure::<dyn Fn()>::new(move || {
            if doc.visibility_state() == web_sys::VisibilityState::Hidden {
                f();
            }
        });
        let _ = document.add_event_listener_with_callback("visibilitychange", on_hide.as_ref().unchecked_ref());
        on_hide.forget();
    }
    let on_unload = Closure::<dyn Fn(web_sys::BeforeUnloadEvent)>::new(move |ev: web_sys::BeforeUnloadEvent| {
        if has_unsaved() {
            flush();
            ev.prevent_default();
        }
    });
    let _ = window.add_event_listener_with_callback("beforeunload", on_unload.as_ref().unchecked_ref());
    on_unload.forget();
}
