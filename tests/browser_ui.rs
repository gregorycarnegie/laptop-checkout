//! The real app in headless Chrome: the Desk, driven with DOM events the way a
//! person (or a barcode scanner) would.
//! `cargo test --target wasm32-unknown-unknown --test browser_ui`.
#![cfg(target_arch = "wasm32")]

mod browser_support;

use std::time::Duration;

use browser_support::*;
use laptop_checkout::models::{BorrowerInput, LaptopInput};
use laptop_checkout::{repo, time};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
use web_sys::{HtmlButtonElement, HtmlElement};

wasm_bindgen_test_configure!(run_in_browser);

fn count(selector: &str) -> usize {
    text(selector).trim().parse().unwrap_or(usize::MAX)
}

#[wasm_bindgen_test]
async fn the_desk_lends_a_laptop_and_takes_one_back() {
    start_app().await;
    go("Desk").await;
    assert!(text(".banner").contains("example data"));
    let on_loan = count(".tally-item .tally-n");
    let late = count(".tally-item.late .tally-n");

    // Check out: scan a laptop, scan a staff card, submit.
    scan("#out-laptop", "LT-0102").await;
    assert_eq!(text(".picked .picked-label"), "LT-0102");
    scan("#out-borrower", "T0388").await;
    click("form.slip button[type=submit]").await;
    assert_toast(
        &("LT-0102 checked out to Grace Whitfield. Due ".to_string() + &time::short(time::due_in_days(7)) + "."),
    );
    assert!(button("Email receipt").is_some(), "Grace has an email address");
    assert_eq!(count(".tally-item .tally-n"), on_loan + 1);
    sleep(Duration::from_millis(50)).await;
    let focused = document().active_element().map(|e| e.id()).unwrap_or_default();
    assert_eq!(focused, "out-laptop", "ready for the next scan");

    // Check in a late laptop by scanning it.
    scan("#in-laptop", "lt-0103").await;
    assert!(text(".return-card").contains("Amara Okafor"));
    let forms = all("form.slip");
    forms[1].query_selector("button[type=submit]").unwrap().unwrap().unchecked_into::<HtmlElement>().click();
    settle().await;
    assert_toast("LT-0103 returned by Amara Okafor, 5 days late.");
    assert_eq!(count(".tally-item.late .tally-n"), late - 1);
}

#[wasm_bindgen_test]
async fn checking_in_without_a_laptop_asks_for_one() {
    start_app().await;
    go("Desk").await;
    let forms = all("form.slip");
    forms[1].query_selector("button[type=submit]").unwrap().unwrap().unchecked_into::<HtmlElement>().click();
    settle().await;
    assert_toast("Scan or choose the laptop being returned.");
    assert!(query(".toast.warn").is_some());
}

#[wasm_bindgen_test]
async fn checking_out_without_choosing_explains_what_is_missing() {
    start_app().await;
    go("Desk").await;
    click("form.slip button[type=submit]").await;
    assert_eq!(text(".form-error"), "Choose a laptop and a borrower.");
}

#[wasm_bindgen_test]
async fn the_picker_works_from_the_keyboard() {
    start_app().await;
    go("Desk").await;
    // Available laptops matching "lt-011": LT-0111, LT-0114, LT-0116.
    type_into("#out-laptop", "lt-011").await;
    let options: Vec<String> =
        all(".picker-list .picker-option .picked-label").into_iter().map(|o| o.text_content().unwrap()).collect();
    assert_eq!(options, ["LT-0111", "LT-0114", "LT-0116"]);
    key("#out-laptop", "ArrowDown").await;
    key("#out-laptop", "ArrowDown").await;
    key("#out-laptop", "ArrowUp").await;
    assert_eq!(text(".picker-option.active .picked-label"), "LT-0114");
    key("#out-laptop", "Enter").await;
    assert_eq!(text(".picked .picked-label"), "LT-0114");

    click_button("Change").await;
    type_into("#out-laptop", "lt").await;
    assert!(query(".picker-list").is_some());
    key("#out-laptop", "Escape").await;
    assert!(query(".picker-list").is_none(), "Escape closes the suggestions");
    key("#out-laptop", "Tab").await;
    assert!(query(".picked").is_none(), "other keys do nothing");
}

#[wasm_bindgen_test]
async fn emailing_late_borrowers_steps_through_a_queue() {
    start_app().await;
    go("Desk").await;
    let late = count(".tally-item.late .tally-n");
    click_button(&format!("Email all {late} late borrowers")).await;
    find(".dialog").await;
    assert!(value("#compose-subject").starts_with("Overdue laptop"), "{}", value("#compose-subject"));
    assert_eq!(text(".dialog .eyebrow"), format!("Email 1 of {late}"));
    let first_to = value("#compose-to");
    click_button("Next borrower").await;
    assert_eq!(text(".dialog .eyebrow"), format!("Email 2 of {late}"));
    assert_ne!(value("#compose-to"), first_to);

    // Opening the mail app logs the email against the loan.
    let before = repo::email_log(100).len();
    let open: HtmlElement = find(".dialog-actions a.btn").await.unchecked_into();
    open.set_attribute("href", "#").unwrap(); // don't leave the test page
    open.click();
    settle().await;
    assert_eq!(repo::email_log(100).len(), before + 1);
    assert!(text(".dialog .hint").starts_with("Logged."));

    click(".dialog .toast-close").await;
    assert!(query(".dialog").is_none());

    // A receipt starts on the receipt template, not the one used last time.
    clear_toasts().await;
    let loan = repo::open_loans().into_iter().find(|l| l.due_at > time::now()).unwrap();
    scan("#out-laptop", "LT-0105").await;
    scan("#out-borrower", &loan.borrower_email).await;
    click("form.slip button[type=submit]").await;
    click_button("Email receipt").await;
    assert!(value("#compose-subject").starts_with("You've borrowed laptop LT-0105"), "{}", value("#compose-subject"));
    assert_eq!(text(".dialog .eyebrow"), "Email");
    click(".dialog .toast-close").await;
}

#[wasm_bindgen_test]
async fn a_borrower_without_email_cannot_be_emailed() {
    start_app().await;
    let who = repo::add_borrower(&BorrowerInput { name: "Cher".into(), ..Default::default() }).unwrap();
    let lap = repo::add_laptop(&LaptopInput { asset_tag: "LT-0990".into(), ..Default::default() }).unwrap();
    repo::check_out(lap, who, time::due_in_days(3), "").unwrap();
    go("Loans").await;
    let row = all(".ledger-row").into_iter().find(|r| r.text_content().unwrap().contains("Cher")).unwrap();
    assert!(row.text_content().unwrap().contains("No email on file"));
    let email: HtmlButtonElement = row.query_selector("button").unwrap().unwrap().unchecked_into();
    assert_eq!(email.text_content().unwrap(), "Email");
    assert!(email.disabled());
    let other = all(".ledger-row").into_iter().find(|r| !r.text_content().unwrap().contains("Cher")).unwrap();
    assert!(!other.query_selector("button").unwrap().unwrap().unchecked_into::<HtmlButtonElement>().disabled());
}

#[wasm_bindgen_test]
async fn toasts_can_be_dismissed_and_expire_oldest_first() {
    const ASK: &str = "Scan or choose the laptop being returned.";
    let asks = || toast_texts().into_iter().filter(|t| t == ASK).count();
    start_app().await;
    go("Desk").await;
    clear_toasts().await;
    let forms = all("form.slip");
    let check_in: HtmlElement = forms[1].query_selector("button[type=submit]").unwrap().unwrap().unchecked_into();

    // Dismissing one leaves the other.
    check_in.click();
    check_in.click();
    settle().await;
    assert_eq!(asks(), 2);
    let close = all(".toast").into_iter().rev().find(|t| t.text_content().unwrap().contains(ASK)).unwrap();
    close.query_selector(".toast-close").unwrap().unwrap().unchecked_into::<HtmlElement>().click();
    settle().await;
    assert_eq!(asks(), 1);

    // Each toast expires six seconds after it appeared, oldest first.
    clear_toasts().await;
    check_in.click();
    sleep(Duration::from_millis(3_000)).await;
    check_in.click();
    settle().await;
    assert_eq!(asks(), 2);
    sleep(Duration::from_millis(3_300)).await;
    assert_eq!(asks(), 1, "the first has expired, the second hasn't");
    sleep(Duration::from_millis(3_000)).await;
    assert_eq!(asks(), 0);
}

#[wasm_bindgen_test]
async fn pages_are_reachable_from_the_navigation() {
    start_app().await;
    for (label, heading, hash) in [
        ("Loans", "Loans", "#loans"),
        ("Laptops", "Laptops", "#laptops"),
        ("Borrowers", "Borrowers", "#borrowers"),
        ("Email templates", "Email templates", "#emails"),
        ("Settings", "Settings", "#settings"),
        ("Desk", "Circulation desk", "#desk"),
    ] {
        go(label).await;
        assert_eq!(text("h1"), heading);
        assert_eq!(web_sys::window().unwrap().location().hash().unwrap(), hash);
    }
    assert_eq!(text(".nav-badge"), count(".tally-item.late .tally-n").to_string(), "the Desk shows how many are late");
}
