//! App-wide reactive state shared through Leptos context.

use std::time::Duration;

use leptos::prelude::*;

use crate::db::DbStatus;
use crate::models::Settings;
use crate::repo;
use crate::time;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Desk,
    Loans,
    Laptops,
    Borrowers,
    Emails,
    Settings,
}

impl Page {
    pub const ALL: [Page; 6] = [
        Page::Desk,
        Page::Loans,
        Page::Laptops,
        Page::Borrowers,
        Page::Emails,
        Page::Settings,
    ];

    pub fn slug(self) -> &'static str {
        match self {
            Page::Desk => "desk",
            Page::Loans => "loans",
            Page::Laptops => "laptops",
            Page::Borrowers => "borrowers",
            Page::Emails => "emails",
            Page::Settings => "settings",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Page::Desk => "Desk",
            Page::Loans => "Loans",
            Page::Laptops => "Laptops",
            Page::Borrowers => "Borrowers",
            Page::Emails => "Email templates",
            Page::Settings => "Settings",
        }
    }

    pub fn from_hash(hash: &str) -> Page {
        let h = hash.trim_start_matches('#');
        Page::ALL
            .into_iter()
            .find(|p| p.slug() == h)
            .unwrap_or(Page::Desk)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToastKind {
    Ok,
    Warn,
    Error,
}

#[derive(Clone, PartialEq, Debug)]
pub enum ToastAction {
    EmailLoan { loan_id: i64, purpose: &'static str },
    EmailAllLate,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Toast {
    pub id: u64,
    pub kind: ToastKind,
    pub text: String,
    pub action: Option<(String, ToastAction)>,
}

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
    pub fn new() -> Self {
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
            notify_permission: RwSignal::new(crate::notify::permission()),
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
        self.toasts.update(|t| {
            t.push(Toast { id, kind, text: text.into(), action });
            if t.len() > 4 {
                t.remove(0);
            }
        });
        let toasts = self.toasts;
        let secs = if kind == ToastKind::Error { 9 } else { 6 };
        set_timeout(
            move || toasts.update(|t| t.retain(|x| x.id != id)),
            Duration::from_secs(secs),
        );
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

    /// Shows the result of a write: nothing extra on success, the reason on failure.
    pub fn report<T>(&self, r: Result<T, String>, success: impl Into<String>) -> Option<T> {
        match r {
            Ok(v) => {
                let text = success.into();
                if !text.is_empty() {
                    self.ok(text);
                }
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
