//! The save scheduler in a real browser: writes are saved shortly after they
//! happen, the screen is told to refresh, and file actions swap databases.
//! `cargo test --target wasm32-unknown-unknown --test browser_persistence`.
#![cfg(target_arch = "wasm32")]

mod browser_support;

use std::time::Duration;

use browser_support as fake;
use browser_support::{eventually, sleep};
use laptop_checkout::{
    db,
    models::BorrowerInput,
    persist::DbStatus,
    repo,
    web::{persistence, storage},
};
use leptos::prelude::*;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

struct Signals {
    rev: RwSignal<u64>,
    status: RwSignal<DbStatus>,
}

/// A fresh browser, with the database connected to storage.
async fn connected() -> Signals {
    fake::init_executor();
    let s = Signals { rev: RwSignal::new(0), status: RwSignal::new(DbStatus::default()) };
    // Drop the last test's pending saves and let a running one finish before
    // clearing storage, or it lands in the next test's "fresh" browser.
    persistence::connect(s.rev, s.status);
    while s.status.get_untracked().saving {
        fake::settle().await;
    }
    storage::disconnect_file().await.unwrap();
    fake::delete_browser_database().await;
    assert!(persistence::init().await.unwrap(), "nothing saved yet");
    repo::migrate().unwrap();
    persistence::save_now().await;
    s.rev.set(0);
    s
}

fn add(name: &str) {
    repo::add_borrower(&BorrowerInput { name: name.into(), ..Default::default() }).unwrap();
}

fn names() -> Vec<String> {
    repo::borrowers().into_iter().map(|b| b.name).collect()
}

async fn saved_names() -> Vec<String> {
    let bytes = storage::load_initial().await.unwrap().expect("saved");
    let current = db::serialize().unwrap();
    db::replace(&bytes).unwrap();
    let saved = names();
    db::replace(&current).unwrap();
    saved
}

#[wasm_bindgen_test]
async fn a_write_is_saved_shortly_afterwards() {
    let s = connected().await;
    add("Amara");
    assert!(s.status.get_untracked().dirty, "marked unsaved straight away");
    eventually("the save", || {
        let st = s.status.get_untracked();
        !st.dirty && !st.saving && st.last_saved.is_some()
    })
    .await;
    assert_eq!(saved_names().await, ["Amara"]);
}

#[wasm_bindgen_test]
async fn a_burst_of_writes_is_saved_together() {
    let _s = connected().await;
    add("A");
    add("B");
    add("C");
    sleep(Duration::from_millis(900)).await;
    assert_eq!(saved_names().await, ["A", "B", "C"]);
}

#[wasm_bindgen_test]
async fn save_now_does_not_wait() {
    let _s = connected().await;
    add("Amara");
    persistence::save_now().await;
    assert_eq!(saved_names().await, ["Amara"]);
}

#[wasm_bindgen_test]
async fn only_complete_changes_refresh_the_screen() {
    let s = connected().await;
    db::exec_quiet("INSERT INTO settings (key, value) VALUES ('x', '1')", []).unwrap();
    assert_eq!(s.rev.get_untracked(), 0);
    add("Amara");
    assert_eq!(s.rev.get_untracked(), 1);
    add("Liam");
    assert_eq!(s.rev.get_untracked(), 2);
}

#[wasm_bindgen_test]
async fn a_returning_visitor_gets_their_saved_database() {
    let _s = connected().await;
    add("Amara");
    persistence::save_now().await;
    db::open_empty().unwrap();
    assert!(!persistence::init().await.unwrap(), "not a first visit");
    assert_eq!(names(), ["Amara"]);
}

#[wasm_bindgen_test]
async fn saving_to_a_file_links_it_and_marks_it_saved() {
    let s = connected().await;
    fake::install_pickers().await;
    add("Amara");
    persistence::create_file().await.unwrap();
    let st = s.status.get_untracked();
    assert_eq!((st.file_name.as_deref(), st.file_connected), (Some(fake::FILE), true));
    assert!(st.last_saved.is_some());
    db::replace(&fake::read_file().await).unwrap();
    assert_eq!(names(), ["Amara"]);
}

#[wasm_bindgen_test]
async fn a_cancelled_save_as_changes_nothing() {
    let s = connected().await;
    fake::install_pickers().await;
    fake::fail_picker(Some("AbortError".into()));
    assert!(persistence::create_file().await.is_err());
    fake::fail_picker(None);
    assert_eq!(s.status.get_untracked().file_name, None);
}

#[wasm_bindgen_test]
async fn opening_a_file_replaces_the_database_and_refreshes() {
    let s = connected().await;
    fake::install_pickers().await;
    add("From the file");
    fake::write_file(&db::serialize().unwrap()).await;
    db::open_empty().unwrap();
    repo::migrate().unwrap();
    s.rev.set(0);

    persistence::open_file().await.unwrap();
    assert_eq!(names(), ["From the file"]);
    assert!(s.rev.get_untracked() > 0, "the screen refreshes");
    assert!(s.status.get_untracked().file_connected);
}

#[wasm_bindgen_test]
async fn reconnecting_needs_permission() {
    let s = connected().await;
    fake::install_pickers().await;
    persistence::create_file().await.unwrap();
    add("Only in the file");
    persistence::save_now().await;
    db::open_empty().unwrap();

    fake::set_permission("denied");
    assert!(!persistence::reconnect_file().await.unwrap());
    fake::set_permission("granted");
    assert!(persistence::reconnect_file().await.unwrap());
    assert_eq!(names(), ["Only in the file"]);
    assert!(s.status.get_untracked().file_connected);
}

#[wasm_bindgen_test]
async fn disconnecting_updates_the_status() {
    let s = connected().await;
    fake::install_pickers().await;
    persistence::create_file().await.unwrap();
    persistence::disconnect_file().await.unwrap();
    assert_eq!(s.status.get_untracked().file_name, None);
}

#[wasm_bindgen_test]
async fn restoring_a_backup_replaces_everything() {
    let _s = connected().await;
    add("In the backup");
    let bytes = db::serialize().unwrap();
    add("Added later");
    let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes.as_slice()));
    let file = web_sys::File::new_with_u8_array_sequence(&parts, "backup.sqlite").unwrap();
    persistence::restore_upload(file).await.unwrap();
    assert_eq!(names(), ["In the backup"]);
}

#[wasm_bindgen_test]
async fn restoring_something_that_is_not_a_backup_is_refused() {
    let _s = connected().await;
    add("Kept");
    let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(&b"name,email"[..]));
    let file = web_sys::File::new_with_u8_array_sequence(&parts, "people.csv").unwrap();
    assert_eq!(persistence::restore_upload(file).await, Err(db::NOT_A_DATABASE.into()));
    assert_eq!(names(), ["Kept"]);
}

#[wasm_bindgen_test]
async fn downloading_offers_the_current_database() {
    let _s = connected().await;
    fake::capture_downloads();
    add("Amara");
    persistence::download("backup.sqlite").unwrap();
    let got: Vec<(String, String)> = serde_json::from_str(&fake::downloads()).unwrap();
    assert_eq!(got.len(), 1);
    assert_eq!(fake::fetch_bytes(&got[0].1).await, db::serialize().unwrap());
}
