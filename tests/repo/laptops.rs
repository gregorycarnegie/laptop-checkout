use laptop_checkout::{
    models::{Laptop, LaptopInput},
    repo::{self, ImportResult},
};
use pretty_assertions::assert_eq;
use rstest::rstest;

use crate::common::{due_in, fresh, lend, machine, TestResult};

fn tagged(tag: &str) -> Option<Laptop> {
    repo::laptops().into_iter().find(|l| l.asset_tag == tag)
}

#[rstest]
fn adding_a_laptop_trims_its_details(_fresh: ()) -> TestResult {
    let id = repo::add_laptop(&LaptopInput {
        asset_tag: " LT-1 ".into(),
        model: " Dell ".into(),
        serial: " SN1 ".into(),
        notes: " dock ".into(),
    })?;
    assert_eq!(
        tagged("LT-1").unwrap(),
        Laptop {
            id,
            asset_tag: "LT-1".into(),
            model: "Dell".into(),
            serial: "SN1".into(),
            notes: "dock".into(),
            status: "available".into(),
            loan_id: None,
            due_at: None,
            borrower_name: None,
            total_loans: 0,
        }
    );
    Ok(())
}

#[rstest]
fn a_laptop_needs_an_asset_tag(_fresh: ()) {
    assert_eq!(repo::add_laptop(&machine(" ")), Err("Enter the laptop's asset tag.".into()));
}

#[rstest]
fn asset_tags_are_unique_ignoring_case(_fresh: ()) -> TestResult {
    repo::add_laptop(&machine("LT-1"))?;
    assert_eq!(repo::add_laptop(&machine("lt-1")), Err("Asset tag lt-1 is already in use.".into()));
    Ok(())
}

#[rstest]
#[case("LT-1", true)]
#[case(" lt-1 ", true)]
#[case("LT-2", false)]
fn laptops_are_found_by_tag(_fresh: (), #[case] tag: &str, #[case] found: bool) -> TestResult {
    repo::add_laptop(&machine("LT-1"))?;
    assert_eq!(repo::find_laptop_by_tag(tag).is_some(), found);
    Ok(())
}

#[rstest]
fn laptops_are_listed_by_tag(_fresh: ()) -> TestResult {
    for t in ["LT-10", "lt-02", "LT-01"] {
        repo::add_laptop(&machine(t))?;
    }
    let tags: Vec<String> = repo::laptops().into_iter().map(|l| l.asset_tag).collect();
    assert_eq!(tags, ["LT-01", "lt-02", "LT-10"]);
    Ok(())
}

#[rstest]
fn a_laptop_on_loan_shows_who_has_it(_fresh: ()) -> TestResult {
    let loan = lend("LT-1", "Amara Okafor", 7)?;
    let l = tagged("LT-1").unwrap();
    assert_eq!(
        (l.loan_id, l.due_at, l.borrower_name.as_deref(), l.total_loans),
        (Some(loan), Some(due_in(7)), Some("Amara Okafor"), 1)
    );
    assert!(l.on_loan());
    Ok(())
}

#[rstest]
fn a_returned_laptop_is_back_on_the_shelf(_fresh: ()) -> TestResult {
    let loan = lend("LT-1", "Amara Okafor", 7)?;
    repo::check_in(loan, "")?;
    let l = tagged("LT-1").unwrap();
    assert_eq!((l.loan_id, l.total_loans), (None, 1));
    Ok(())
}

#[rstest]
fn updating_a_laptop_changes_its_details(_fresh: ()) -> TestResult {
    let id = repo::add_laptop(&machine("LT-1"))?;
    repo::update_laptop(
        id,
        &LaptopInput { asset_tag: "LT-9".into(), model: "HP".into(), serial: "S".into(), notes: "N".into() },
    )?;
    let l = tagged("LT-9").unwrap();
    assert_eq!((l.id, l.model.as_str(), l.serial.as_str(), l.notes.as_str()), (id, "HP", "S", "N"));
    Ok(())
}

#[rstest]
fn a_laptop_can_keep_its_own_tag_when_updated(_fresh: ()) -> TestResult {
    let id = repo::add_laptop(&machine("LT-1"))?;
    repo::update_laptop(id, &LaptopInput { model: "HP".into(), ..machine("lt-1") })
}

#[rstest]
fn an_update_cannot_take_another_laptops_tag(_fresh: ()) -> TestResult {
    repo::add_laptop(&machine("LT-1"))?;
    let two = repo::add_laptop(&machine("LT-2"))?;
    assert_eq!(repo::update_laptop(two, &machine("LT-1")), Err("Asset tag LT-1 is already in use.".into()));
    assert_eq!(repo::update_laptop(two, &machine("")), Err("Enter the laptop's asset tag.".into()));
    Ok(())
}

#[rstest]
#[case("repair")]
#[case("retired")]
#[case("available")]
fn a_laptop_status_can_be_set(_fresh: (), #[case] status: &str) -> TestResult {
    let id = repo::add_laptop(&machine("LT-1"))?;
    repo::set_laptop_status(id, status)?;
    assert_eq!(tagged("LT-1").unwrap().status, status);
    Ok(())
}

#[rstest]
fn an_unknown_status_is_refused_by_the_database(_fresh: ()) -> TestResult {
    let id = repo::add_laptop(&machine("LT-1"))?;
    assert!(repo::set_laptop_status(id, "lost").is_err());
    Ok(())
}

#[rstest]
fn a_laptop_with_no_loans_can_be_deleted(_fresh: ()) -> TestResult {
    let id = repo::add_laptop(&machine("LT-1"))?;
    repo::delete_laptop(id)?;
    assert!(repo::laptops().is_empty());
    Ok(())
}

#[rstest]
fn a_laptop_with_loan_history_cannot_be_deleted(_fresh: ()) -> TestResult {
    lend("LT-1", "Amara", 3)?;
    let id = tagged("LT-1").unwrap().id;
    assert_eq!(repo::delete_laptop(id), Err("This laptop has loan history, so it can only be retired.".into()));
    Ok(())
}

#[rstest]
fn importing_laptops_adds_updates_and_skips(_fresh: ()) -> TestResult {
    repo::add_laptop(&machine("LT-1"))?;
    let rows = [
        LaptopInput { model: "HP".into(), serial: "S1".into(), notes: "n".into(), ..machine("lt-1") },
        machine("LT-2"),
        machine(""),
    ];
    assert_eq!(repo::import_laptops(&rows, false)?, ImportResult { added: 1, updated: 0, skipped: 2 });
    assert_eq!(tagged("LT-1").unwrap().model, "Dell Latitude 3440");
    assert_eq!(repo::import_laptops(&rows, true)?, ImportResult { added: 0, updated: 2, skipped: 1 });
    let l = tagged("LT-1").unwrap();
    assert_eq!((l.model.as_str(), l.serial.as_str(), l.notes.as_str()), ("HP", "S1", "n"));
    Ok(())
}

#[rstest]
fn an_import_is_all_or_nothing(_fresh: ()) -> TestResult {
    // Break the table so the second insert fails half-way through.
    laptop_checkout::db::run_script("CREATE TRIGGER no_lt2 BEFORE INSERT ON laptops WHEN NEW.asset_tag = 'LT-2' BEGIN SELECT RAISE(ABORT, 'nope'); END")?;
    assert!(repo::import_laptops(&[machine("LT-1"), machine("LT-2")], false).is_err());
    assert!(repo::laptops().is_empty());
    Ok(())
}

#[rstest]
fn a_laptop_can_be_looked_up_by_id(_fresh: ()) -> TestResult {
    let id = repo::add_laptop(&machine("LT-1"))?;
    assert_eq!(repo::laptop(id).map(|l| l.asset_tag), Some("LT-1".into()));
    assert_eq!(repo::laptop(id + 1), None);
    Ok(())
}

#[rstest]
fn only_laptops_that_have_been_lent_have_history(_fresh: ()) -> TestResult {
    let id = repo::add_laptop(&machine("LT-1"))?;
    assert!(!repo::laptop(id).ok_or("missing")?.has_history());
    lend("LT-2", "Amara", 3)?;
    assert!(tagged("LT-2").ok_or("missing")?.has_history());
    Ok(())
}
