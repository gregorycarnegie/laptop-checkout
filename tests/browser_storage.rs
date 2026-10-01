//! Browser tests for saving on the PC, run in headless Chrome:
//! `cargo test --target wasm32-unknown-unknown --test browser_storage`.
#![cfg(target_arch = "wasm32")]

mod browser_support;

use std::{cell::Cell, rc::Rc};

use browser_support as fake;
use laptop_checkout::{db, models::BorrowerInput, repo, time, web::storage};
use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

fn amara() -> BorrowerInput {
    BorrowerInput { name: "Amara Okafor".into(), email: "amara@example.org".into(), ..Default::default() }
}

/// An empty browser (no saved database, no linked file) and a fresh database.
async fn clean() {
    storage::disconnect_file().await.unwrap();
    fake::delete_browser_database().await;
    db::open_empty().unwrap();
    repo::migrate().unwrap();
}

fn names() -> Vec<String> {
    repo::borrowers().into_iter().map(|b| b.name).collect()
}

async fn reload() {
    db::open_empty().unwrap();
    let saved = storage::load_initial().await.unwrap().expect("a saved database");
    db::replace(&saved).unwrap();
}

// ---------------------------------------------------------------- IndexedDB

#[wasm_bindgen_test]
async fn a_new_browser_has_nothing_saved() {
    clean().await;
    assert_eq!(storage::load_initial().await.unwrap(), None);
}

#[wasm_bindgen_test]
async fn the_database_is_saved_to_indexeddb_and_loaded_back() {
    clean().await;
    repo::add_borrower(&amara()).unwrap();
    storage::persist(&db::serialize().unwrap()).await.unwrap();
    reload().await;
    assert_eq!(names(), ["Amara Okafor"]);
}

#[wasm_bindgen_test]
async fn a_later_save_replaces_the_earlier_one() {
    clean().await;
    storage::persist(&db::serialize().unwrap()).await.unwrap();
    repo::add_borrower(&amara()).unwrap();
    storage::persist(&db::serialize().unwrap()).await.unwrap();
    reload().await;
    assert_eq!(names(), ["Amara Okafor"]);
}

// ---------------------------------------------------------------- a linked .sqlite file

#[wasm_bindgen_test]
async fn saving_to_a_new_file_writes_the_database_into_it() {
    clean().await;
    fake::install_pickers().await;
    repo::add_borrower(&amara()).unwrap();
    storage::create_file(&db::serialize().unwrap()).await.unwrap();

    let pick = fake::last_pick();
    assert!(pick.contains(r#""suggestedName":"laptop-library.sqlite""#), "{pick}");
    assert!(pick.contains(r#"".sqlite",".sqlite3",".db""#), "{pick}");
    let info = storage::file_info();
    assert_eq!((info.file_name.as_deref(), info.writable), (Some(fake::FILE), true));
    assert_eq!(fake::read_file().await, db::serialize().unwrap());
}

#[wasm_bindgen_test]
async fn every_save_also_goes_into_the_linked_file() {
    clean().await;
    fake::install_pickers().await;
    storage::create_file(&db::serialize().unwrap()).await.unwrap();
    repo::add_borrower(&amara()).unwrap();
    storage::persist(&db::serialize().unwrap()).await.unwrap();
    db::open_empty().unwrap();
    db::replace(&fake::read_file().await).unwrap();
    assert_eq!(names(), ["Amara Okafor"]);
}

#[wasm_bindgen_test]
async fn on_the_next_visit_the_linked_file_wins_when_allowed() {
    clean().await;
    fake::install_pickers().await;
    storage::create_file(&db::serialize().unwrap()).await.unwrap();
    // Someone edits the file elsewhere; the browser copy is now out of date.
    repo::add_borrower(&amara()).unwrap();
    fake::write_file(&db::serialize().unwrap()).await;
    reload().await;
    assert_eq!(names(), ["Amara Okafor"]);
    assert!(fake::permission_asks().contains(r#"["query","readwrite"]"#));
}

#[wasm_bindgen_test]
async fn without_permission_the_browser_copy_is_used_until_reconnected() {
    clean().await;
    fake::install_pickers().await;
    storage::create_file(&db::serialize().unwrap()).await.unwrap();
    repo::add_borrower(&amara()).unwrap();
    fake::write_file(&db::serialize().unwrap()).await;

    fake::set_permission("prompt");
    reload().await;
    assert!(names().is_empty(), "the browser copy has no borrowers");
    let info = storage::file_info();
    assert_eq!((info.file_name.as_deref(), info.writable), (Some(fake::FILE), false));

    // Still refused: nothing to reconnect to.
    assert_eq!(storage::reconnect_file().await.unwrap(), None);

    fake::set_permission("granted");
    let bytes = storage::reconnect_file().await.unwrap().expect("the file's bytes");
    db::replace(&bytes).unwrap();
    assert_eq!(names(), ["Amara Okafor"]);
    assert!(storage::file_info().writable);
    assert!(fake::permission_asks().contains(r#"["request","readwrite"]"#));
}

#[wasm_bindgen_test]
async fn a_linked_file_that_was_deleted_falls_back_to_the_browser_copy() {
    clean().await;
    fake::install_pickers().await;
    repo::add_borrower(&amara()).unwrap();
    storage::create_file(&db::serialize().unwrap()).await.unwrap();
    fake::delete_file(fake::FILE).await;
    reload().await;
    assert_eq!(names(), ["Amara Okafor"]);
    assert!(!storage::file_info().writable, "the missing file isn't written to");
}

#[wasm_bindgen_test]
async fn opening_an_existing_file_returns_its_contents_and_links_it() {
    clean().await;
    fake::install_pickers().await;
    repo::add_borrower(&amara()).unwrap();
    let theirs = db::serialize().unwrap();
    fake::write_file(&theirs).await;
    storage::disconnect_file().await.unwrap();

    assert_eq!(storage::open_file().await.unwrap(), theirs);
    assert!(fake::last_pick().contains(r#""multiple":false"#));
    assert_eq!(storage::file_info().file_name.as_deref(), Some(fake::FILE));
}

#[wasm_bindgen_test]
async fn a_cancelled_picker_is_reported() {
    clean().await;
    fake::install_pickers().await;
    fake::fail_picker(Some("AbortError".into()));
    let err = storage::create_file(&db::serialize().unwrap()).await.unwrap_err();
    assert!(err.contains("aborted"), "{err}");
    assert!(storage::open_file().await.is_err());
    fake::fail_picker(None);
    assert_eq!(storage::file_info().file_name, None);
}

#[wasm_bindgen_test]
async fn disconnecting_forgets_the_file() {
    clean().await;
    fake::install_pickers().await;
    storage::create_file(&db::serialize().unwrap()).await.unwrap();
    storage::disconnect_file().await.unwrap();
    assert_eq!(storage::file_info().file_name, None);
    assert_eq!(storage::reconnect_file().await.unwrap(), None);
    reload().await; // Uses the browser copy; the handle is gone from storage too.
    assert_eq!(storage::file_info().file_name, None);
}

#[wasm_bindgen_test]
async fn saving_to_a_file_needs_a_supporting_browser() {
    fake::install_pickers().await;
    assert!(storage::file_info().fs_supported);
    fake::remove_pickers();
    assert!(!storage::file_info().fs_supported);
}

// ---------------------------------------------------------------- uploads and downloads

#[wasm_bindgen_test]
async fn an_uploaded_backup_is_read_byte_for_byte() {
    clean().await;
    repo::add_borrower(&amara()).unwrap();
    let bytes = db::serialize().unwrap();
    let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes.as_slice()));
    let file = web_sys::File::new_with_u8_array_sequence(&parts, "backup.sqlite").unwrap();
    assert_eq!(storage::read_upload(file).await.unwrap(), bytes);
}

#[wasm_bindgen_test]
async fn a_backup_download_carries_the_database() {
    clean().await;
    fake::capture_downloads();
    let bytes = db::serialize().unwrap();
    storage::download(&bytes, "backup.sqlite").unwrap();
    let got = fake::downloads();
    let (name, url): (String, String) = {
        let v: Vec<(String, String)> = serde_json::from_str(&got).unwrap();
        v.into_iter().next().expect("one download")
    };
    assert_eq!(name, "backup.sqlite");
    assert_eq!(fake::fetch_bytes(&url).await, bytes);
    let doc = web_sys::window().unwrap().document().unwrap();
    assert!(doc.query_selector("a[download]").unwrap().is_none(), "the temporary link is removed");
}

// ---------------------------------------------------------------- clipboard

#[wasm_bindgen_test]
async fn copying_uses_the_clipboard_when_allowed() {
    fake::fake_clipboard(true, false);
    assert!(storage::copy_text("amara@example.org").await);
    assert_eq!(fake::copied().as_deref(), Some("amara@example.org"));
    assert_eq!(fake::exec_calls(), "[]");
}

#[wasm_bindgen_test]
async fn copying_falls_back_to_selecting_hidden_text() {
    fake::fake_clipboard(false, true);
    assert!(storage::copy_text("subject line").await);
    assert_eq!(fake::exec_calls(), r#"[["copy","subject line"]]"#);
    let doc = web_sys::window().unwrap().document().unwrap();
    assert!(doc.query_selector("textarea[readonly]").unwrap().is_none(), "the hidden textarea is removed");
}

#[wasm_bindgen_test]
async fn copying_reports_failure_when_both_ways_are_refused() {
    fake::fake_clipboard(false, false);
    assert!(!storage::copy_text("x").await);
}

// ---------------------------------------------------------------- leaving the page

#[wasm_bindgen_test]
fn closing_the_tab_with_unsaved_changes_saves_and_warns() {
    let flushed = Rc::new(Cell::new(0));
    let unsaved = Rc::new(Cell::new(false));
    let (f, u) = (flushed.clone(), unsaved.clone());
    storage::install_unload_guard(move || f.set(f.get() + 1), move || u.get());
    let window = web_sys::window().unwrap();
    let init = web_sys::EventInit::new();
    init.set_cancelable(true);

    let calm = web_sys::Event::new_with_event_init_dict("beforeunload", &init).unwrap();
    window.dispatch_event(&calm).unwrap();
    assert_eq!((flushed.get(), calm.default_prevented()), (0, false));

    unsaved.set(true);
    let busy = web_sys::Event::new_with_event_init_dict("beforeunload", &init).unwrap();
    window.dispatch_event(&busy).unwrap();
    assert_eq!((flushed.get(), busy.default_prevented()), (1, true));

    // Switching tabs saves only when the page is actually hidden (it isn't here).
    let doc = window.document().unwrap();
    doc.dispatch_event(&web_sys::Event::new("visibilitychange").unwrap()).unwrap();
    assert_eq!(flushed.get(), 1);
}

// ---------------------------------------------------------------- odds and ends

#[wasm_bindgen_test]
fn the_browser_clock_and_timezone_are_used() {
    let js_now = js_sys::Date::now() as i64;
    assert!((time::now() - js_now).abs() < 1_000);
    let d = js_sys::Date::new(&JsValue::from_f64(js_now as f64));
    let l = time::local(js_now);
    assert_eq!((l.hour, l.minute, l.day), (d.get_hours(), d.get_minutes(), d.get_date()));
}

#[wasm_bindgen_test]
fn errors_from_javascript_become_readable_messages() {
    assert_eq!(storage::js_error(JsValue::from_str("plain")), "plain");
    assert_eq!(storage::js_error(js_sys::Error::new("with message").into()), "with message");
    assert_eq!(storage::js_error(JsValue::from_f64(3.0)), "JsValue(3)");
}
