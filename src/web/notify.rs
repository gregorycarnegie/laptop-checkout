//! Desktop notifications for overdue laptops, falling back to in-page alerts.

use std::time::Duration;

use leptos::{prelude::*, task::spawn_local};
use wasm_bindgen::prelude::*;
use web_sys::{Notification, NotificationOptions, NotificationPermission};

use crate::{
    alerts::{self, AlertAction, AlertSink, Note},
    time,
    ui::state::AppState,
    view_model::{Page, ToastAction, ToastKind},
};

/// How often to look for newly overdue loans while the app is open.
pub const CHECK_EVERY: Duration = Duration::from_secs(60);

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
                show(st, &Note {
                    title: "Notifications are on".into(),
                    body: "You'll be told here when a laptop is overdue.".into(),
                    tag: "test".into(),
                });
            }
            "denied" => st.warn(
                "Notifications are blocked. Allow them in the browser's site settings (the icon left of the address bar).",
            ),
            _ => {}
        }
    });
}

pub fn show(st: AppState, note: &Note) {
    let opts = NotificationOptions::new();
    opts.set_body(&note.body);
    opts.set_tag(&note.tag);
    if let Ok(n) = Notification::new_with_options(&note.title, &opts) {
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

/// Shows alerts as desktop notifications and toasts.
struct Browser(AppState);

impl AlertSink for Browser {
    fn can_notify(&self) -> bool {
        let p = permission();
        self.0.notify_permission.set(p.clone());
        p == "granted"
    }

    fn notify(&self, note: &Note) {
        show(self.0, note);
    }

    fn toast(&self, text: &str, action_label: &'static str, action: AlertAction) {
        let action = match action {
            AlertAction::EmailLoan(loan_id) => ToastAction::EmailLoan { loan_id, purpose: "overdue" },
            AlertAction::EmailAllLate => ToastAction::EmailAllLate,
        };
        self.0.toast_with(ToastKind::Warn, text, Some((action_label.into(), action)));
    }
}

fn tick(st: AppState) {
    let now = time::now();
    st.clock.set(now);
    alerts::check(now, &st.settings.get_untracked(), &Browser(st));
}

/// Starts the once-a-minute overdue check.
pub fn start(st: AppState) {
    set_timeout(move || tick(st), Duration::from_secs(3));
    let _ = set_interval_with_handle(move || tick(st), CHECK_EVERY);
}
