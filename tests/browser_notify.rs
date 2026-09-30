//! Desktop notifications, with the browser's Notification API replaced by a
//! fake that records what would have been shown.
//! `cargo test --target wasm32-unknown-unknown --test browser_notify`.
#![cfg(target_arch = "wasm32")]

mod browser_support;

use std::time::Duration;

use browser_support as fake;
use browser_support::{eventually, sleep};
use laptop_checkout::alerts::Note;
use laptop_checkout::ui::state::AppState;
use laptop_checkout::view_model::{Page, ToastAction, ToastKind};
use laptop_checkout::web::notify;
use laptop_checkout::{db, repo};
use leptos::prelude::*;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

fn state() -> AppState {
    fake::init_executor();
    db::open_empty().unwrap();
    repo::migrate().unwrap();
    AppState::new(notify::permission())
}

fn toasts(st: AppState) -> Vec<(ToastKind, String)> {
    st.toasts.get_untracked().into_iter().map(|t| (t.kind, t.text)).collect()
}

fn notes() -> Vec<(String, String, String)> {
    serde_json::from_str(&fake::notes()).unwrap()
}

#[wasm_bindgen_test]
fn permission_reports_what_the_browser_says() {
    for p in ["granted", "denied", "default"] {
        fake::fake_notifications(Some(p.into()), p);
        assert_eq!(notify::permission(), p);
    }
    fake::fake_notifications(None, "");
    assert_eq!(notify::permission(), "unsupported");
}

#[wasm_bindgen_test]
async fn turning_notifications_on_shows_a_confirmation() {
    fake::fake_notifications(Some("default".into()), "granted");
    let st = state();
    notify::request(st);
    eventually("the answer", || st.notify_permission.get_untracked() == "granted").await;
    assert_eq!(toasts(st), [(ToastKind::Ok, "Notifications are on.".to_string())]);
    assert_eq!(notes()[0].0, "Notifications are on");
}

#[wasm_bindgen_test]
async fn blocking_notifications_explains_how_to_allow_them() {
    fake::fake_notifications(Some("default".into()), "denied");
    let st = state();
    notify::request(st);
    eventually("the answer", || st.notify_permission.get_untracked() == "denied").await;
    let t = toasts(st);
    assert_eq!(t[0].0, ToastKind::Warn);
    assert!(t[0].1.starts_with("Notifications are blocked."));
    assert!(notes().is_empty());
}

#[wasm_bindgen_test]
async fn dismissing_the_question_changes_nothing() {
    fake::fake_notifications(Some("default".into()), "default");
    let st = state();
    notify::request(st);
    sleep(Duration::from_millis(100)).await;
    assert!(toasts(st).is_empty());
}

#[wasm_bindgen_test]
fn browsers_without_notifications_get_in_page_alerts() {
    fake::fake_notifications(None, "");
    let st = state();
    notify::request(st);
    let t = toasts(st);
    assert_eq!(t[0].0, ToastKind::Warn);
    assert!(t[0].1.contains("can't show notifications"));
}

#[wasm_bindgen_test]
fn clicking_a_notification_opens_the_desk() {
    fake::fake_notifications(Some("granted".into()), "granted");
    let st = state();
    st.page.set(Page::Settings);
    notify::show(st, &Note { title: "LT-1 is overdue".into(), body: "Amara".into(), tag: "loan-1".into() });
    assert_eq!(notes(), [("LT-1 is overdue".to_string(), "Amara".to_string(), "loan-1".to_string())]);
    fake::click_last_note();
    assert_eq!(st.page.get_untracked(), Page::Desk);
}

#[wasm_bindgen_test]
async fn overdue_laptops_raise_notifications_and_an_email_prompt() {
    fake::fake_notifications(Some("granted".into()), "granted");
    let st = state();
    repo::seed_sample().unwrap();
    notify::start(st);
    eventually("the first check", || !toasts(st).is_empty()).await;
    let tags: Vec<String> = notes().into_iter().map(|n| n.2).collect();
    assert_eq!(tags.len(), 3, "{tags:?}");
    assert!(tags.iter().all(|t| t.starts_with("loan-")));
    let toast = st.toasts.get_untracked().pop().unwrap();
    assert_eq!((toast.kind, toast.text.as_str()), (ToastKind::Warn, "3 laptops are overdue."));
    assert_eq!(toast.action, Some(("Email all late".to_string(), ToastAction::EmailAllLate)));
    assert_eq!(st.notify_permission.get_untracked(), "granted");
}

#[wasm_bindgen_test]
async fn with_notifications_blocked_only_the_page_alert_shows() {
    fake::fake_notifications(Some("denied".into()), "denied");
    let st = state();
    repo::seed_sample().unwrap();
    // One late loan, so the alert offers to email that borrower.
    for l in repo::open_loans().into_iter().skip(1) {
        repo::check_in(l.id, "").unwrap();
    }
    notify::start(st);
    eventually("the first check", || !toasts(st).is_empty()).await;
    assert!(notes().is_empty());
    let toast = st.toasts.get_untracked().pop().unwrap();
    assert!(toast.text.starts_with("LT-0103 is overdue"), "{}", toast.text);
    let Some((label, ToastAction::EmailLoan { purpose, .. })) = toast.action else { panic!("no email button") };
    assert_eq!((label.as_str(), purpose), ("Email borrower", "overdue"));
}
