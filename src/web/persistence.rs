//! Connects the database to storage on the PC: every write marks the data
//! dirty, and shortly after the last one the bytes are saved.

use std::cell::{Cell, RefCell};
use std::time::Duration;

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::db::{self, Change};
use crate::persist::{DbStatus, SaveState};
use crate::time;
use crate::web::storage;

/// Wait after the last write before saving.
const SAVE_DELAY: Duration = Duration::from_millis(400);

thread_local! {
    static SAVE: RefCell<SaveState> = RefCell::new(SaveState::default());
    static STATUS: Cell<Option<RwSignal<DbStatus>>> = const { Cell::new(None) };
    static REVISION: Cell<Option<RwSignal<u64>>> = const { Cell::new(None) };
}

/// Routes database changes to saving (and `revision` bumps, which refresh queries).
pub fn connect(revision: RwSignal<u64>, status: RwSignal<DbStatus>) {
    REVISION.with(|r| r.set(Some(revision)));
    STATUS.with(|s| s.set(Some(status)));
    db::set_change_hook(|change| {
        mark_dirty();
        if change == Change::Committed {
            if let Some(rev) = REVISION.with(Cell::get) {
                rev.update(|v| *v += 1);
            }
        }
    });
    refresh_status();
}

fn refresh_status() {
    if let Some(sig) = STATUS.with(Cell::get) {
        let info = storage::file_info();
        sig.set(SAVE.with_borrow(|s| s.status(info)));
    }
}

fn mark_dirty() {
    let ticket = SAVE.with_borrow_mut(SaveState::mark_dirty);
    refresh_status();
    set_timeout(
        move || {
            if SAVE.with_borrow(|s| s.is_latest(ticket)) {
                spawn_local(save());
            }
        },
        SAVE_DELAY,
    );
}

async fn save() {
    if !SAVE.with_borrow_mut(SaveState::begin) {
        mark_dirty();
        return;
    }
    refresh_status();
    let result = match db::serialize() {
        Ok(bytes) => storage::persist(&bytes).await,
        Err(e) => Err(e),
    };
    SAVE.with_borrow_mut(|s| s.finish(result, time::now()));
    refresh_status();
}

pub async fn save_now() {
    save().await;
}

/// Loads the saved database. Returns `true` for a brand-new one.
pub async fn init() -> Result<bool, String> {
    let saved = storage::load_initial().await?;
    db::replace(saved.as_deref().unwrap_or_default())?;
    storage::install_unload_guard(
        || {
            if SAVE.with_borrow(|s| s.has_unsaved()) {
                spawn_local(save());
            }
        },
        || SAVE.with_borrow(SaveState::has_unsaved),
    );
    refresh_status();
    Ok(saved.is_none())
}

/// Creates a new `.sqlite` file on the PC and keeps writing every change into it.
pub async fn create_file() -> Result<(), String> {
    let r = storage::create_file(&db::serialize()?).await;
    if r.is_ok() {
        SAVE.with_borrow_mut(|s| s.saved_at(time::now()));
    }
    refresh_status();
    r
}

fn swap_in(bytes: &[u8]) -> Result<(), String> {
    db::replace(bytes)?;
    db::announce_change();
    Ok(())
}

/// Opens an existing `.sqlite` file and uses it from now on.
pub async fn open_file() -> Result<(), String> {
    let r = storage::open_file().await.and_then(|b| swap_in(&b));
    refresh_status();
    r
}

/// Re-grants access to the linked file after a browser restart.
pub async fn reconnect_file() -> Result<bool, String> {
    let r = match storage::reconnect_file().await? {
        Some(bytes) => swap_in(&bytes).map(|_| true),
        None => Ok(false),
    };
    refresh_status();
    r
}

pub async fn disconnect_file() -> Result<(), String> {
    let r = storage::disconnect_file().await;
    refresh_status();
    r
}

/// Replaces the current database with a backup the user uploaded.
pub async fn restore_upload(file: web_sys::File) -> Result<(), String> {
    swap_in(&storage::read_upload(file).await?)
}

pub fn download(name: &str) -> Result<(), String> {
    storage::download(&db::serialize()?, name)
}
