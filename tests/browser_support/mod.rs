//! Fakes for browser APIs that need a person or the OS: the file pickers, the
//! clipboard, downloads and desktop notifications. The fake pickers hand back
//! real files from Chrome's Origin Private File System, so the app's Rust code
//! reads and writes genuine `FileSystemFileHandle`s.
#![allow(dead_code)]

use std::time::Duration;

use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

#[wasm_bindgen(inline_js = r#"
export async function installPickers(name) {
  const root = await navigator.storage.getDirectory();
  const handle = await root.getFileHandle(name, { create: true });
  window.__picks = [];
  window.__permissionAsks = [];
  window.__permission = "granted";
  window.__pickerError = null;
  const fail = () => { if (window.__pickerError) throw window.__pickerError; };
  window.showSaveFilePicker = async (opts) => { window.__picks.push(opts); fail(); return handle; };
  window.showOpenFilePicker = async (opts) => { window.__picks.push(opts); fail(); return [handle]; };
  const proto = Object.getPrototypeOf(FileSystemFileHandle.prototype);
  proto.queryPermission = async (d) => { window.__permissionAsks.push(["query", d.mode]); return window.__permission; };
  proto.requestPermission = async (d) => { window.__permissionAsks.push(["request", d.mode]); return window.__permission; };
}
export function removePickers() { delete window.showSaveFilePicker; delete window.showOpenFilePicker; }
export function setPermission(p) { window.__permission = p; }
export function failPicker(name) {
  const message = name === "AbortError" ? "The user aborted a request." : "The request is not allowed.";
  window.__pickerError = name ? new DOMException(message, name) : null;
}
export function lastPick() { return JSON.stringify(window.__picks[window.__picks.length - 1] ?? null); }
export function permissionAsks() { return JSON.stringify(window.__permissionAsks); }
export async function readFile(name) {
  const root = await navigator.storage.getDirectory();
  const file = await (await root.getFileHandle(name)).getFile();
  return new Uint8Array(await file.arrayBuffer());
}
export async function writeFile(name, bytes) {
  const root = await navigator.storage.getDirectory();
  const out = await (await root.getFileHandle(name, { create: true })).createWritable();
  await out.write(bytes);
  await out.close();
}
export async function deleteFile(name) {
  const root = await navigator.storage.getDirectory();
  await root.removeEntry(name);
}
export function deleteBrowserDatabase() {
  return new Promise((resolve) => {
    const req = indexedDB.deleteDatabase("laptop-checkout");
    req.onsuccess = req.onerror = req.onblocked = () => resolve();
  });
}

export function fakeClipboard(apiWorks, execWorks) {
  window.__copied = null;
  window.__exec = [];
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { writeText: async (t) => { if (!apiWorks) throw new Error("denied"); window.__copied = t; } },
  });
  document.execCommand = (cmd) => {
    const ta = document.querySelector("textarea[readonly]");
    window.__exec.push([cmd, ta ? ta.value : null]);
    return execWorks;
  };
}
export function copied() { return window.__copied; }
export function execCalls() { return JSON.stringify(window.__exec); }

export function captureDownloads() {
  window.__downloads = [];
  HTMLAnchorElement.prototype.click = function () { window.__downloads.push([this.download, this.href]); };
}
export function downloads() { return JSON.stringify(window.__downloads); }
export async function fetchBytes(url) { return new Uint8Array(await (await fetch(url)).arrayBuffer()); }

export function fakeNotifications(permission, afterAsking) {
  window.__notes = [];
  window.__perm = permission;
  window.__permAfter = afterAsking;
  if (permission == null) { delete window.Notification; return; }
  window.Notification = class {
    constructor(title, opts) { this.title = title; this.body = opts.body; this.tag = opts.tag; window.__notes.push(this); }
    static get permission() { return window.__perm; }
    static requestPermission() { window.__perm = window.__permAfter; return Promise.resolve(window.__perm); }
  };
}
export function notes() { return JSON.stringify(window.__notes.map((n) => [n.title, n.body, n.tag])); }
export function clickLastNote() { const n = window.__notes[window.__notes.length - 1]; n.onclick(); }
"#)]
extern "C" {
    #[wasm_bindgen(js_name = installPickers)]
    async fn install_pickers_js(name: &str);
    #[wasm_bindgen(js_name = removePickers)]
    pub fn remove_pickers();
    #[wasm_bindgen(js_name = setPermission)]
    pub fn set_permission(p: &str);
    #[wasm_bindgen(js_name = failPicker)]
    pub fn fail_picker(name: Option<String>);
    #[wasm_bindgen(js_name = lastPick)]
    pub fn last_pick() -> String;
    #[wasm_bindgen(js_name = permissionAsks)]
    pub fn permission_asks() -> String;
    #[wasm_bindgen(js_name = readFile)]
    async fn read_file_js(name: &str) -> JsValue;
    #[wasm_bindgen(js_name = writeFile)]
    async fn write_file_js(name: &str, bytes: js_sys::Uint8Array);
    #[wasm_bindgen(js_name = deleteFile)]
    pub async fn delete_file(name: &str);
    #[wasm_bindgen(js_name = deleteBrowserDatabase)]
    pub async fn delete_browser_database();
    #[wasm_bindgen(js_name = fakeClipboard)]
    pub fn fake_clipboard(api_works: bool, exec_works: bool);
    pub fn copied() -> Option<String>;
    #[wasm_bindgen(js_name = execCalls)]
    pub fn exec_calls() -> String;
    #[wasm_bindgen(js_name = captureDownloads)]
    pub fn capture_downloads();
    pub fn downloads() -> String;
    #[wasm_bindgen(js_name = fetchBytes)]
    async fn fetch_bytes_js(url: &str) -> JsValue;
    #[wasm_bindgen(js_name = fakeNotifications)]
    pub fn fake_notifications(permission: Option<String>, after_asking: &str);
    pub fn notes() -> String;
    #[wasm_bindgen(js_name = clickLastNote)]
    pub fn click_last_note();
}

pub const FILE: &str = "picked.sqlite";

/// Starts Leptos's task executor (the app does this when it mounts).
pub fn init_executor() {
    let _ = any_spawner::Executor::init_wasm_bindgen();
}

pub async fn install_pickers() {
    install_pickers_js(FILE).await;
}

pub async fn read_file() -> Vec<u8> {
    js_sys::Uint8Array::new(&read_file_js(FILE).await).to_vec()
}

pub async fn write_file(bytes: &[u8]) {
    write_file_js(FILE, js_sys::Uint8Array::from(bytes)).await;
}

pub async fn fetch_bytes(url: &str) -> Vec<u8> {
    js_sys::Uint8Array::new(&fetch_bytes_js(url).await).to_vec()
}

pub async fn sleep(d: Duration) {
    let p = js_sys::Promise::new(&mut |resolve, _| {
        web_sys::window()
            .unwrap()
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, d.as_millis() as i32)
            .unwrap();
    });
    JsFuture::from(p).await.unwrap();
}

/// Waits (up to 5 s) for `check` to pass.
pub async fn eventually(what: &str, check: impl Fn() -> bool) {
    for _ in 0..100 {
        if check() {
            return;
        }
        sleep(Duration::from_millis(50)).await;
    }
    panic!("timed out waiting for {what}");
}

// ---------------------------------------------------------------- driving the UI

use wasm_bindgen::JsCast;
use web_sys::{Element, Event, EventInit, HtmlElement, HtmlInputElement, KeyboardEvent, KeyboardEventInit};

pub fn document() -> web_sys::Document {
    web_sys::window().unwrap().document().unwrap()
}

pub fn query(selector: &str) -> Option<Element> {
    document().query_selector(selector).unwrap()
}

pub fn all(selector: &str) -> Vec<Element> {
    let list = document().query_selector_all(selector).unwrap();
    (0..list.length()).map(|i| list.item(i).unwrap().unchecked_into()).collect()
}

/// Waits (up to 5 s) for an element to appear.
pub async fn find(selector: &str) -> Element {
    for _ in 0..100 {
        if let Some(el) = query(selector) {
            return el;
        }
        sleep(Duration::from_millis(50)).await;
    }
    panic!("{selector} never appeared");
}

pub fn text(selector: &str) -> String {
    query(selector).map(|e| e.text_content().unwrap_or_default()).unwrap_or_default()
}

fn bubbling() -> EventInit {
    let init = EventInit::new();
    init.set_bubbles(true);
    init
}

pub async fn settle() {
    sleep(Duration::from_millis(80)).await;
}

/// Types into an input or textarea as a keyboard would.
pub async fn type_into(selector: &str, value: &str) {
    let el = find(selector).await;
    el.unchecked_ref::<HtmlElement>().focus().unwrap();
    js_sys::Reflect::set(&el, &"value".into(), &value.into()).unwrap();
    el.dispatch_event(&Event::new_with_event_init_dict("input", &bubbling()).unwrap()).unwrap();
    settle().await;
}

/// Changes an input or select and fires `change`, as leaving the field does.
pub async fn change(selector: &str, value: &str) {
    let el = find(selector).await;
    js_sys::Reflect::set(&el, &"value".into(), &value.into()).unwrap();
    el.dispatch_event(&Event::new_with_event_init_dict("change", &bubbling()).unwrap()).unwrap();
    settle().await;
}

pub async fn key(selector: &str, key: &str) {
    let el = find(selector).await;
    let init = KeyboardEventInit::new();
    init.set_key(key);
    init.set_bubbles(true);
    init.set_cancelable(true);
    el.dispatch_event(&KeyboardEvent::new_with_keyboard_event_init_dict("keydown", &init).unwrap()).unwrap();
    settle().await;
}

/// Types a value and presses Enter, as a barcode scanner does.
pub async fn scan(selector: &str, value: &str) {
    type_into(selector, value).await;
    key(selector, "Enter").await;
}

pub async fn click(selector: &str) {
    find(selector).await.unchecked_into::<HtmlElement>().click();
    settle().await;
}

pub fn button(label: &str) -> Option<HtmlElement> {
    all("button, a.btn")
        .into_iter()
        .find(|b| b.text_content().unwrap_or_default().trim() == label)
        .map(|b| b.unchecked_into())
}

pub async fn click_button(label: &str) {
    let b = button(label).unwrap_or_else(|| panic!("no button labelled {label:?}"));
    b.click();
    settle().await;
}

pub fn value(selector: &str) -> String {
    query(selector).map(|e| e.unchecked_into::<HtmlInputElement>().value()).unwrap_or_default()
}

pub fn toast_texts() -> Vec<String> {
    all(".toast p").into_iter().map(|p| p.text_content().unwrap_or_default()).collect()
}

pub fn last_toast() -> String {
    toast_texts().pop().unwrap_or_default()
}

pub async fn go(label: &str) {
    let link = all(".nav-item")
        .into_iter()
        .find(|a| a.text_content().unwrap_or_default().starts_with(label))
        .unwrap_or_else(|| panic!("no page {label}"));
    link.unchecked_into::<HtmlElement>().click();
    settle().await;
}

/// Mounts the app once per test file, on a fresh browser database (so the
/// example data loads). Returns once the shell is showing.
pub async fn start_app() {
    if query(".shell").is_some() {
        return;
    }
    delete_browser_database().await;
    let boot = document().create_element("div").unwrap();
    boot.set_id("boot");
    document().body().unwrap().append_child(&boot).unwrap();
    laptop_checkout::ui::start();
    find(".shell").await;
    assert!(query("#boot").is_none(), "the loading placeholder is removed");
}

/// Dismisses every toast on screen.
pub async fn clear_toasts() {
    for close in all(".toast-close") {
        close.unchecked_into::<HtmlElement>().click();
    }
    settle().await;
}
