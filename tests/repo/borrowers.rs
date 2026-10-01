use laptop_checkout::{
    models::{Borrower, BorrowerInput},
    repo::{self, ImportResult},
};
use pretty_assertions::assert_eq;
use rstest::rstest;

use crate::common::{due_in, fresh, lend, machine, now, person, seeded, TestResult};

fn named(name: &str) -> Option<Borrower> {
    repo::borrowers().into_iter().find(|b| b.name == name)
}

#[rstest]
fn adding_a_borrower_trims_every_field(_fresh: ()) -> TestResult {
    let id = repo::add_borrower(&BorrowerInput {
        name: "  Amara Okafor ".into(),
        email: " amara@example.org ".into(),
        department: " Year 11 ".into(),
        external_id: " S1 ".into(),
        phone: " 0123 ".into(),
        notes: " left-handed ".into(),
    })?;
    let b = named("Amara Okafor").unwrap();
    assert_eq!(
        (b.id, b.email.as_str(), b.department.as_str(), b.external_id.as_str(), b.phone.as_str(), b.notes.as_str()),
        (id, "amara@example.org", "Year 11", "S1", "0123", "left-handed")
    );
    assert!(b.is_active());
    Ok(())
}

#[rstest]
fn a_borrower_needs_a_name(_fresh: ()) {
    assert_eq!(repo::add_borrower(&person("  ", "a@b.org")), Err("Enter the borrower's name.".into()));
}

#[rstest]
fn a_borrower_can_have_no_email(_fresh: ()) {
    assert!(repo::add_borrower(&person("Cher", "")).is_ok());
}

#[rstest]
fn a_bad_email_is_refused(_fresh: ()) {
    assert_eq!(
        repo::add_borrower(&person("Amara", "amara")),
        Err("\"amara\" doesn't look like an email address.".into())
    );
}

#[rstest]
fn emails_are_unique_ignoring_case(_fresh: ()) -> TestResult {
    repo::add_borrower(&person("Amara", "amara@example.org"))?;
    assert_eq!(
        repo::add_borrower(&person("Amara Again", "AMARA@example.org")),
        Err("A borrower with AMARA@example.org already exists.".into())
    );
    Ok(())
}

#[rstest]
fn several_borrowers_may_have_no_email(_fresh: ()) -> TestResult {
    repo::add_borrower(&person("A", ""))?;
    repo::add_borrower(&person("B", ""))?;
    assert_eq!(repo::borrowers().len(), 2);
    Ok(())
}

#[rstest]
#[case("amara@example.org", true)]
#[case("  Amara@Example.org ", true)]
#[case("liam@example.org", false)]
#[case("", false)]
#[case("   ", false)]
fn borrowers_are_found_by_email(_fresh: (), #[case] email: &str, #[case] found: bool) -> TestResult {
    repo::add_borrower(&person("Amara", "amara@example.org"))?;
    repo::add_borrower(&person("No Email", ""))?;
    assert_eq!(repo::find_borrower_by_email(email).is_some(), found);
    Ok(())
}

#[rstest]
fn updating_a_borrower_changes_their_details(_fresh: ()) -> TestResult {
    let id = repo::add_borrower(&person("Amara", "amara@example.org"))?;
    repo::update_borrower(
        id,
        &BorrowerInput { department: "Year 12".into(), ..person("Amara Okafor", "amara@example.org") },
    )?;
    let b = named("Amara Okafor").unwrap();
    assert_eq!((b.id, b.department.as_str()), (id, "Year 12"));
    Ok(())
}

#[rstest]
fn an_update_cannot_take_someone_elses_email(_fresh: ()) -> TestResult {
    repo::add_borrower(&person("Amara", "amara@example.org"))?;
    let liam = repo::add_borrower(&person("Liam", "liam@example.org"))?;
    assert_eq!(
        repo::update_borrower(liam, &person("Liam", "amara@example.org")),
        Err("Another borrower already uses amara@example.org.".into())
    );
    Ok(())
}

#[rstest]
fn an_update_is_validated(_fresh: ()) -> TestResult {
    let id = repo::add_borrower(&person("Amara", "amara@example.org"))?;
    assert!(repo::update_borrower(id, &person("", "amara@example.org")).is_err());
    assert_eq!(named("Amara").map(|b| b.id), Some(id));
    Ok(())
}

#[rstest]
fn borrowers_can_be_deactivated_and_reactivated(_fresh: ()) -> TestResult {
    let id = repo::add_borrower(&person("Amara", "amara@example.org"))?;
    repo::set_borrower_active(id, false)?;
    assert!(!named("Amara").unwrap().is_active());
    repo::set_borrower_active(id, true)?;
    assert!(named("Amara").unwrap().is_active());
    Ok(())
}

#[rstest]
fn inactive_borrowers_are_listed_last(_fresh: ()) -> TestResult {
    let a = repo::add_borrower(&person("Aaron", ""))?;
    repo::add_borrower(&person("zed", ""))?;
    repo::set_borrower_active(a, false)?;
    let names: Vec<String> = repo::borrowers().into_iter().map(|b| b.name).collect();
    assert_eq!(names, ["zed", "Aaron"]);
    Ok(())
}

#[rstest]
fn a_borrower_with_no_loans_can_be_deleted(_fresh: ()) -> TestResult {
    let id = repo::add_borrower(&person("Amara", ""))?;
    repo::delete_borrower(id)?;
    assert!(repo::borrowers().is_empty());
    Ok(())
}

#[rstest]
fn a_borrower_with_loan_history_cannot_be_deleted(_fresh: ()) -> TestResult {
    lend("LT-1", "Amara Okafor", 7)?;
    let id = named("Amara Okafor").unwrap().id;
    assert_eq!(
        repo::delete_borrower(id),
        Err("This borrower has loan history, so they can only be deactivated.".into())
    );
    Ok(())
}

#[rstest]
fn borrower_counts_cover_open_late_and_past_loans(_fresh: ()) -> TestResult {
    let loan = lend("LT-1", "Amara Okafor", -2)?;
    let id = named("Amara Okafor").unwrap().id;
    let second = repo::add_laptop(&machine("LT-2"))?;
    repo::check_out(second, id, due_in(3), "")?;
    let third = repo::add_laptop(&machine("LT-3"))?;
    let old = repo::check_out(third, id, due_in(1), "")?;
    repo::check_in(old, "")?;
    let b = named("Amara Okafor").unwrap();
    assert_eq!((b.open_loans, b.late_loans, b.total_loans), (2, 1, 3));
    assert!(loan > 0);
    Ok(())
}

#[rstest]
fn importing_adds_new_people_and_skips_known_ones(_seeded: ()) -> TestResult {
    let before = repo::borrowers().len();
    let r = repo::import_borrowers(
        &[
            person("New Person", "new@example.org"),
            person("Amara Renamed", "AMARA.OKAFOR@example.org"),
            person("", "blank@example.org"),
        ],
        false,
    )?;
    assert_eq!(r, ImportResult { added: 1, updated: 0, skipped: 2 });
    assert_eq!(repo::borrowers().len(), before + 1);
    assert!(named("Amara Okafor").is_some());
    Ok(())
}

#[rstest]
fn importing_can_update_people_already_here(_seeded: ()) -> TestResult {
    let r = repo::import_borrowers(&[person("Amara O. Okafor", "amara.okafor@example.org")], true)?;
    assert_eq!(r, ImportResult { added: 0, updated: 1, skipped: 0 });
    assert!(named("Amara O. Okafor").is_some());
    Ok(())
}

#[rstest]
fn imported_borrowers_are_stamped_with_the_time(_fresh: ()) -> TestResult {
    repo::import_borrowers(&[person("New", "new@example.org")], false)?;
    let when = laptop_checkout::db::scalar("SELECT created_at FROM borrowers", []);
    assert_eq!(when, now());
    Ok(())
}

#[rstest]
fn a_borrower_can_be_looked_up_by_id(_fresh: ()) -> TestResult {
    let id = repo::add_borrower(&person("Amara", "amara@example.org"))?;
    assert_eq!(repo::borrower(id).map(|b| b.name), Some("Amara".into()));
    assert_eq!(repo::borrower(id + 1), None);
    Ok(())
}

#[rstest]
fn only_borrowers_who_have_borrowed_have_history(_fresh: ()) -> TestResult {
    let id = repo::add_borrower(&person("Amara", "amara@example.org"))?;
    assert!(!repo::borrower(id).ok_or("missing")?.has_history());
    let l = repo::add_laptop(&machine("LT-1"))?;
    let loan = repo::check_out(l, id, due_in(2), "")?;
    repo::check_in(loan, "")?;
    assert!(repo::borrower(id).ok_or("missing")?.has_history());
    Ok(())
}
