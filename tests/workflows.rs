//! End-to-end workflows across modules: what a person at the desk actually
//! does, checked step by step (unlike unit tests, each step is asserted).
#![cfg(not(target_arch = "wasm32"))]

mod common;

use std::cell::RefCell;

use common::{at, fresh, TestResult};
use laptop_checkout::{
    alerts::{self, AlertAction, AlertSink, Note},
    email,
    import::{ImportKind, Parsed, Tally},
    models::ServiceStatus,
    repo,
    time::{self, clock, DAY},
    view_model::{self as vm, PickItem},
};
use pretty_assertions::assert_eq;
use rstest::rstest;

/// Records alerts instead of showing them (a hand-written test double).
#[derive(Default)]
struct Recorder {
    notes: RefCell<Vec<Note>>,
    toasts: RefCell<Vec<String>>,
}

impl AlertSink for Recorder {
    fn can_notify(&self) -> bool {
        true
    }
    fn notify(&self, note: &Note) {
        self.notes.borrow_mut().push(note.clone());
    }
    fn toast(&self, text: &str, _: &'static str, _: AlertAction) {
        self.toasts.borrow_mut().push(text.into());
    }
}

fn pick(items: &[PickItem], typed: &str) -> i64 {
    vm::pick_on_enter(items, typed, 0).unwrap_or_else(|| panic!("nothing matches {typed:?}"))
}

#[rstest]
fn a_term_of_lending_from_import_to_return(_fresh: ()) -> TestResult {
    // 1. Import staff and laptops from spreadsheets.
    let people = Parsed::build(
        ImportKind::Borrowers,
        "First Name,Surname,Email Address,Department,Staff ID\n\
         Grace,Whitfield,g.whitfield@school.org,English,T0388\n\
         Priya,Natarajan,p.natarajan@school.org,Science,T0412\n",
    )?;
    assert_eq!(Tally::of(&people.preview()), Tally { new: 2, existing: 0, bad: 0 });
    people.import(false)?;
    let kit = Parsed::build(
        ImportKind::Laptops,
        "Asset Tag,Model,Serial Number\nLT-0201,Dell Latitude 3440,7HQ9ZK3\nLT-0202,Dell Latitude 3440,7HQ9ZK4\n",
    )?;
    kit.import(false)?;

    // 2. Scan a laptop and a staff card at the desk, due in a week.
    let laptop = pick(&vm::available_laptop_items(&repo::laptops()), "LT-0201");
    let grace = pick(&vm::borrower_items(&repo::borrowers()), "t0388");
    let due = time::due_in_days(repo::settings().loan_days);
    let loan = repo::check_out(laptop, grace, due, "with charger")?;
    let out = repo::loan(loan).ok_or("loan missing")?;
    assert_eq!(vm::checked_out_message(&out), "LT-0201 checked out to Grace Whitfield. Due Tue 13 Oct.");
    assert_eq!(vm::available_laptop_items(&repo::laptops()).len(), 1, "LT-0201 is off the shelf");

    // 3. Nine days later it is overdue: the desk is alerted once.
    clock::set_now(at(2026, 10, 15, 9, 0));
    let alerts = Recorder::default();
    assert_eq!(alerts::check(time::now(), &repo::settings(), &alerts), 1);
    assert_eq!(*alerts.toasts.borrow(), ["LT-0201 is overdue (Grace Whitfield, 2 days late)."]);
    assert_eq!(alerts::check(time::now() + 60_000, &repo::settings(), &alerts), 0);

    // 4. Email her with the overdue template the app picks.
    let late = vm::late(&repo::open_loans(), time::now());
    let ids = vm::emailable(&late);
    assert_eq!(ids, [loan]);
    let templates = repo::templates();
    let chosen = vm::pick_template(&templates, "overdue", repo::settings().late_template).ok_or("no template")?;
    let t = templates.iter().find(|t| t.id == chosen).ok_or("template missing")?;
    assert_eq!(t.name, "Overdue notice");
    let l = &late[0];
    let s = repo::settings();
    let (subject, body) = (email::render(&t.subject, l, &s), email::render(&t.body, l, &s));
    assert_eq!(subject, "Overdue laptop LT-0201: please return it");
    assert!(body.starts_with("Hi Grace,"));
    let link = email::compose_url("gmail", &l.borrower_email, &subject, &body);
    assert!(
        link.starts_with("https://mail.google.com/mail/?view=cm&fs=1&to=g.whitfield%40school.org&su=Overdue%20laptop")
    );
    repo::log_email(l, &l.borrower_email, &subject, &t.name)?;

    // 5. She renews it instead of returning it.
    let new_due = repo::renew(loan, 7)?;
    assert_eq!(time::short(new_due), "Thu 22 Oct");
    assert!(vm::late(&repo::open_loans(), time::now()).is_empty());

    // 6. She brings it back a day late.
    clock::set_now(new_due + DAY);
    let returned = repo::check_in(loan, "small scratch on lid")?;
    assert_eq!(vm::returned_message(&returned), "LT-0201 returned by Grace Whitfield, 1 day late.");
    assert_eq!(returned.note, "with charger\nReturned: small scratch on lid");
    assert_eq!((returned.emails_sent, returned.renewals), (1, 1));
    assert_eq!(vm::available_laptop_items(&repo::laptops()).len(), 2, "back on the shelf");

    // 7. The history keeps everything.
    let history = repo::all_loans();
    assert_eq!(history.len(), 1);
    assert_eq!(repo::email_log(10)[0].subject, subject);
    Ok(())
}

#[rstest]
fn a_laptop_goes_for_repair_and_comes_back(_fresh: ()) -> TestResult {
    let id = repo::add_laptop(&common::machine("LT-7"))?;
    repo::set_laptop_status(id, ServiceStatus::Repair)?;
    assert!(vm::available_laptop_items(&repo::laptops()).is_empty());
    let who = repo::add_borrower(&common::person("Liam Chen", "liam@example.org"))?;
    assert_eq!(repo::check_out(id, who, common::due_in(3), ""), Err("LT-7 is marked as in repair.".into()));
    repo::set_laptop_status(id, ServiceStatus::Available)?;
    assert_eq!(vm::available_laptop_items(&repo::laptops()).len(), 1);
    repo::check_out(id, who, common::due_in(3), "")?;
    Ok(())
}

#[rstest]
fn a_leaver_is_deactivated_but_keeps_their_history(_fresh: ()) -> TestResult {
    let loan = common::lend("LT-1", "Amara Okafor", 2)?;
    let amara = repo::loan(loan).ok_or("missing")?.borrower_id;
    repo::check_in(loan, "")?;
    assert!(repo::delete_borrower(amara).is_err());
    repo::set_borrower_active(amara, false)?;
    assert!(vm::borrower_items(&repo::borrowers()).is_empty(), "no longer offered at check-out");
    assert_eq!(repo::all_loans()[0].borrower_name, "Amara Okafor");
    Ok(())
}
