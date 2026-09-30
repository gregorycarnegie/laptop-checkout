//! Browser notifications for overdue laptops.
//!
//! While the app is open it checks every minute. Each overdue loan triggers a
//! notification, then stays quiet for the "remind again" period in Settings.
//! If notifications are blocked, the same alert appears inside the page.

use std::time::Duration;

use leptos::prelude::*;
use leptos::task::spawn_local;
use wasm_bindgen::prelude::*;
use web_sys::{Notification, NotificationOptions, NotificationPermission};

use crate::repo;
use crate::state::{AppState, Page, ToastAction, ToastKind};
use crate::time;

fn supported() -> bool {
    web_sys::window()
        .map(|w| js_sys::Reflect::has(&w, &JsValue::from_str("Notification")).unwrap_or(false))
        .unwrap_or(false)
}

/// `granted`, `denied`, `default` or `unsupported`.
pub fn permission() -> String {
    if !supported() {
        return "unsupported".into();
    }
    match Notification::permission() {
        NotificationPermission::Granted => "granted",
        NotificationPermission::Denied => "denied",
        _ => "default",
    }
    .into()
}

/// Asks the browser for permission. Must be called from a click.
pub fn request(st: AppState) {
    if !supported() {
        st.warn("This browser can't show notifications. Overdue alerts will appear in the page instead.");
        return;
    }
    let Ok(promise) = Notification::request_permission() else {
        st.error("The browser refused to ask for notification permission.");
        return;
    };
    spawn_local(async move {
        let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
        let p = permission();
        st.notify_permission.set(p.clone());
        match p.as_str() {
            "granted" => {
                st.ok("Notifications are on.");
                show(st, "Notifications are on", "You'll be told here when a laptop is overdue.", "test");
            }
            "denied" => st.warn(
                "Notifications are blocked. Allow them in the browser's site settings (the icon left of the address bar).",
            ),
            _ => {}
        }
    });
}

pub fn show(st: AppState, title: &str, body: &str, tag: &str) {
    let opts = NotificationOptions::new();
    opts.set_body(body);
    opts.set_tag(tag);
    if let Ok(n) = Notification::new_with_options(title, &opts) {
        let onclick = Closure::<dyn Fn()>::new(move || {
            if let Some(w) = web_sys::window() {
                let _ = w.focus();
            }
            st.go(Page::Desk);
        });
        n.set_onclick(Some(onclick.as_ref().unchecked_ref()));
        onclick.forget();
    }
}

fn check(st: AppState) {
    let now = time::now();
    st.clock.set(now);
    let s = repo::settings();
    if !s.notify_enabled {
        return;
    }
    let loans = repo::loans_needing_alert(now, s.renotify_hours);
    if loans.is_empty() {
        return;
    }
    let granted = permission() == "granted";
    st.notify_permission.set(permission());
    if granted {
        if loans.len() <= 3 {
            for l in &loans {
                show(
                    st,
                    &format!("{} is overdue", l.asset_tag),
                    &format!(
                        "{} · {} · due {}",
                        l.borrower_name,
                        time::describe_due(l.due_at, now),
                        time::short(l.due_at)
                    ),
                    &format!("loan-{}", l.id),
                );
            }
        } else {
            let names: Vec<String> = loans.iter().take(4).map(|l| l.borrower_name.clone()).collect();
            show(
                st,
                &format!("{} laptops are overdue", loans.len()),
                &format!("{}{}", names.join(", "), if loans.len() > 4 { "…" } else { "" }),
                "overdue-summary",
            );
        }
    }
    let text = if loans.len() == 1 {
        format!(
            "{} is overdue ({}, {}).",
            loans[0].asset_tag,
            loans[0].borrower_name,
            time::describe_due(loans[0].due_at, now)
        )
    } else {
        format!("{} laptops are overdue.", loans.len())
    };
    let ids: Vec<i64> = loans.iter().map(|l| l.id).collect();
    st.toast_with(
        ToastKind::Warn,
        text,
        Some(if ids.len() == 1 {
            ("Email borrower".into(), ToastAction::EmailLoan { loan_id: ids[0], purpose: "overdue" })
        } else {
            ("Email all late".into(), ToastAction::EmailAllLate)
        }),
    );
    repo::mark_notified(&ids, now);
}

/// Starts the once-a-minute overdue check.
pub fn start(st: AppState) {
    set_timeout(move || check(st), Duration::from_secs(3));
    let _ = set_interval_with_handle(move || check(st), Duration::from_secs(60));
}
