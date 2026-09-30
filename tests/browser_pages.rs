//! The management pages in headless Chrome: borrowers, laptops, templates and
//! settings. `cargo test --target wasm32-unknown-unknown --test browser_pages`.
#![cfg(target_arch = "wasm32")]

mod browser_support;

use browser_support::*;
use laptop_checkout::repo;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
use web_sys::{HtmlButtonElement, HtmlElement};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn a_borrower_can_be_added_one_after_another() {
    start_app().await;
    go("Borrowers").await;
    click_button("Add borrower").await;
    type_into("#borrower-name", "Tariq Aziz").await;
    type_into("#borrower-email", "t.aziz@example.org").await;
    find("#borrower-another").await.unchecked_into::<HtmlElement>().click();
    click("form.form-panel button[type=submit]").await;
    assert_toast("Tariq Aziz saved.");
    assert_eq!(value("#borrower-name"), "", "cleared for the next person");

    type_into("#borrower-name", "Bea Lund").await;
    find("#borrower-another").await.unchecked_into::<HtmlElement>().click();
    click("form.form-panel button[type=submit]").await;
    assert!(query("form.form-panel").is_none(), "closes after the last one");
    assert!(repo::find_borrower_by_email("t.aziz@example.org").is_some());
}

#[wasm_bindgen_test]
async fn a_duplicate_email_is_explained_in_the_form() {
    start_app().await;
    go("Borrowers").await;
    click_button("Add borrower").await;
    type_into("#borrower-name", "Amara Again").await;
    type_into("#borrower-email", "amara.okafor@example.org").await;
    click("form.form-panel button[type=submit]").await;
    assert_eq!(text(".form-error"), "A borrower with amara.okafor@example.org already exists.");
    click_button("Cancel").await;
}

#[wasm_bindgen_test]
async fn editing_a_borrower_starts_from_their_details() {
    start_app().await;
    go("Borrowers").await;
    type_into("#borrower-search", "liam").await;
    click_button("Edit").await;
    assert_eq!(value("#borrower-email"), "liam.chen@example.org");
    type_into("#borrower-dept", "Year 11").await;
    click("form.form-panel button[type=submit]").await;
    assert_toast("Liam Chen saved.");
    assert!(query("form.form-panel").is_none());
}

#[wasm_bindgen_test]
async fn borrowers_can_be_deactivated() {
    start_app().await;
    go("Borrowers").await;
    type_into("#borrower-search", "ethan").await;
    click_button("Deactivate").await;
    assert_toast("Borrower deactivated. They won't appear when checking out.");
}

#[wasm_bindgen_test]
async fn late_borrowers_can_be_emailed_from_their_row() {
    start_app().await;
    go("Borrowers").await;
    type_into("#borrower-search", "hannah").await;
    click_button("Email").await;
    assert!(value("#compose-to").starts_with("hannah"));
    click(".dialog .toast-close").await;
}

#[wasm_bindgen_test]
async fn borrowers_are_imported_from_pasted_csv() {
    start_app().await;
    go("Borrowers").await;
    click_button("Import CSV").await;
    type_into("textarea[id^=csv-text]", "First Name,Surname,Email\nNia,Brooks,nia@example.org\nBad,Row,nope").await;
    assert_eq!(text(".import-summary .pill.ok"), "1 new");
    assert_eq!(text(".import-summary .pill.late"), "1 with problems");
    type_into("textarea[id^=csv-text]", "   ").await;
    assert!(query(".import-summary").is_none(), "an empty box shows no preview");
    type_into("textarea[id^=csv-text]", "First Name,Surname,Email\nNia,Brooks,nia@example.org").await;
    click_button("Import 1").await;
    assert_toast("Added 1 borrower.");
    assert!(repo::find_borrower_by_email("nia@example.org").is_some());
}

#[wasm_bindgen_test]
async fn a_laptop_status_change_is_confirmed() {
    start_app().await;
    go("Laptops").await;
    type_into("#laptop-search", "LT-0108").await;
    let select = find("select[id^=laptop-status]").await;
    change(&format!("#{}", select.id()), "repair").await;
    assert_toast("LT-0108 is in repair.");
    assert_eq!(text("td .pill"), "In repair");
}

#[wasm_bindgen_test]
async fn a_new_laptop_can_be_deleted_after_confirming() {
    start_app().await;
    go("Laptops").await;
    click_button("Add laptop").await;
    type_into("#laptop-tag", "LT-0777").await;
    click("form.form-panel button[type=submit]").await;
    assert_toast("LT-0777 saved.");
    type_into("#laptop-search", "LT-0777").await;
    click_button("Delete").await;
    assert!(repo::find_laptop_by_tag("LT-0777").is_some(), "one click only arms the button");
    click_button("Delete?").await;
    assert_toast("Laptop deleted.");
    assert!(repo::find_laptop_by_tag("LT-0777").is_none());
}

#[wasm_bindgen_test]
async fn laptops_with_history_offer_no_delete() {
    start_app().await;
    go("Laptops").await;
    type_into("#laptop-search", "LT-0101").await;
    assert!(button("Delete").is_none());
    click_button("Edit").await;
    assert_eq!(value("#laptop-model"), "Dell Latitude 3440");
    click_button("Cancel").await;
}

#[wasm_bindgen_test]
async fn a_template_is_edited_and_saved() {
    start_app().await;
    go("Email templates").await;
    let save = || button("Save template").unwrap().unchecked_into::<HtmlButtonElement>();
    find("#tpl-name").await;
    assert!(save().disabled(), "nothing to save yet");
    type_into("#tpl-subject", "Please return {{asset_tag}} today").await;
    assert!(!save().disabled());
    click_button("{{first_name}}").await;
    assert!(value("#tpl-body").contains("{{first_name}}"));
    click_button("Save template").await;
    assert_toast("Template saved.");
    assert!(save().disabled(), "saved, so nothing left to save");

    type_into("#tpl-name", "").await;
    click_button("Save template").await;
    assert_toast("Give the template a name.");
    assert!(!save().disabled(), "the failed save is still unsaved");
}

#[wasm_bindgen_test]
async fn settings_check_the_loan_period() {
    start_app().await;
    go("Settings").await;
    change("#loan-days", "0").await;
    assert_toast("Enter a number of days between 1 and 365.");
    assert!(query(".toast.error").is_some());
    change("#loan-days", "14").await;
    assert_toast("Loan period saved.");
    assert_eq!(repo::settings().loan_days, 14);
}

#[wasm_bindgen_test]
async fn the_database_can_be_saved_to_a_file_from_settings() {
    start_app().await;
    install_pickers().await;
    go("Settings").await;

    // Closing the picker isn't an error...
    fail_picker(Some("AbortError".into()));
    let before = toast_texts().len();
    click_button("Save to a new file…").await;
    sleep(std::time::Duration::from_millis(200)).await;
    assert_eq!(toast_texts().len(), before, "no message for a cancelled picker");

    // ...but a refusal is.
    fail_picker(Some("NotAllowedError".into()));
    click_button("Save to a new file…").await;
    sleep(std::time::Duration::from_millis(200)).await;
    assert_toast("The request is not allowed.");

    fail_picker(None);
    click_button("Save to a new file…").await;
    eventually("the linked file", || text(".callout.ok").contains(FILE)).await;
    assert_toast("Saving to the file on this PC.");
    assert!(text(".storage-line").contains(FILE));

    click_button("Stop saving to this file").await;
    eventually("the confirmation", || toast_texts().iter().any(|t| t.starts_with("Stopped saving"))).await;
}
