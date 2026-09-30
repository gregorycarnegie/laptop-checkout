//! App-wide reactive state shared through Leptos context.

use std::time::Duration;

use leptos::prelude::*;

use crate::{
    models::Settings,
    persist::DbStatus,
    repo, time,
    view_model::{push_toast, Page, Toast, ToastAction, ToastKind},
};

/// A queue of loans to email, one at a time.
#[derive(Clone, PartialEq, Debug)]
pub struct Compose {
    pub loan_ids: Vec<i64>,
    pub index: usize,
    /// Which kind of template to start with: `overdue`, `reminder` or `receipt`.
    pub purpose: &'static str,
}

#[derive(Clone, Copy)]
pub struct AppState {
    /// Bumped after every committed write so queries re-run.
    pub rev: RwSignal<u64>,
    /// Current time, refreshed every minute so due states stay accurate.
    pub clock: RwSignal<i64>,
    pub page: RwSignal<Page>,
    pub toasts: RwSignal<Vec<Toast>>,
    pub compose: RwSignal<Option<Compose>>,
    pub db_status: RwSignal<DbStatus>,
    pub notify_permission: RwSignal<String>,
    pub settings: Memo<Settings>,
    next_toast: StoredValue<u64>,
}

impl AppState {
    pub fn new(notify_permission: String) -> Self {
        let rev = RwSignal::new(0);
        let settings = Memo::new(move |_| {
            rev.track();
            repo::settings()
        });
        AppState {
            rev,
            clock: RwSignal::new(time::now()),
            page: RwSignal::new(Page::Desk),
            toasts: RwSignal::new(Vec::new()),
            compose: RwSignal::new(None),
            db_status: RwSignal::new(DbStatus::default()),
            notify_permission: RwSignal::new(notify_permission),
            settings,
            next_toast: StoredValue::new(0),
        }
    }

    pub fn go(&self, page: Page) {
        self.page.set(page);
        if let Some(w) = web_sys::window() {
            let _ = w.location().set_hash(page.slug());
            w.scroll_to_with_x_and_y(0.0, 0.0);
        }
    }

    pub fn toast_with(&self, kind: ToastKind, text: impl Into<String>, action: Option<(String, ToastAction)>) {
        let id = self.next_toast.get_value() + 1;
        self.next_toast.set_value(id);
        self.toasts.update(|t| push_toast(t, Toast { id, kind, text: text.into(), action }));
        let toasts = self.toasts;
        set_timeout(move || toasts.update(|t| t.retain(|x| x.id != id)), Duration::from_secs(kind.seconds()));
    }

    pub fn ok(&self, text: impl Into<String>) {
        self.toast_with(ToastKind::Ok, text, None);
    }

    pub fn warn(&self, text: impl Into<String>) {
        self.toast_with(ToastKind::Warn, text, None);
    }

    pub fn error(&self, text: impl Into<String>) {
        self.toast_with(ToastKind::Error, text, None);
    }

    /// Shows the result of a write: `success` if it worked, the reason if not.
    pub fn report<T>(&self, r: Result<T, String>, success: impl Into<String>) -> Option<T> {
        match r {
            Ok(v) => {
                self.ok(success);
                Some(v)
            }
            Err(e) => {
                self.error(e);
                None
            }
        }
    }

    pub fn email_loans(&self, loan_ids: Vec<i64>, purpose: &'static str) {
        if !loan_ids.is_empty() {
            self.compose.set(Some(Compose { loan_ids, index: 0, purpose }));
        }
    }
}

pub fn use_app() -> AppState {
    expect_context::<AppState>()
}
