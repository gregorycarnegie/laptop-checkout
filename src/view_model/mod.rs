//! Everything the screens decide, as plain functions: filters, search,
//! picker matching, messages and labels. The Leptos views only wire these up,
//! so the behaviour is tested here without a browser.

use crate::{
    models::{Borrower, Laptop, Loan, ServiceStatus, Template},
    time::{self, DueState, DAY},
};

mod borrowers;
mod emails;
mod laptops;
mod loans;
mod picker;
mod settings;

pub use borrowers::*;
pub use emails::*;
pub use laptops::*;
pub use loans::*;
pub use picker::*;
pub use settings::*;

// ---------------------------------------------------------------- navigation

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
    pub const ALL: [Page; 6] = [Page::Desk, Page::Loans, Page::Laptops, Page::Borrowers, Page::Emails, Page::Settings];

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

    /// The page for a URL hash such as `#loans`; anything else is the Desk.
    pub fn from_hash(hash: &str) -> Page {
        let h = hash.trim_start_matches('#');
        Page::ALL.into_iter().find(|p| p.slug() == h).unwrap_or(Page::Desk)
    }
}

// ---------------------------------------------------------------- toasts

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ToastKind {
    Ok,
    Warn,
    Error,
}

impl ToastKind {
    pub fn class(self) -> &'static str {
        match self {
            ToastKind::Ok => "toast ok",
            ToastKind::Warn => "toast warn",
            ToastKind::Error => "toast error",
        }
    }

    /// How long the toast stays up, in seconds. Errors stay longer.
    pub fn seconds(self) -> u64 {
        match self {
            ToastKind::Error => 9,
            _ => 6,
        }
    }
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

/// At most this many toasts are on screen; the oldest goes first.
pub const MAX_TOASTS: usize = 4;

pub fn push_toast(list: &mut Vec<Toast>, toast: Toast) {
    list.push(toast);
    if list.len() > MAX_TOASTS {
        list.remove(0);
    }
}

// ---------------------------------------------------------------- search

/// Case-insensitive match of every word in `query` against `haystack`.
pub fn matches(haystack: &str, query: &str) -> bool {
    let hay = haystack.to_lowercase();
    query.to_lowercase().split_whitespace().all(|w| hay.contains(w))
}

/// How many items each filter would show.
pub fn counts<T, F: Copy>(items: &[T], options: &[(F, &str)], keep: impl Fn(F, &T) -> bool) -> Vec<usize> {
    options.iter().map(|(f, _)| items.iter().filter(|i| keep(*f, i)).count()).collect()
}

// Native only: these use test crates that don't build for WebAssembly.
#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
mod tests;
