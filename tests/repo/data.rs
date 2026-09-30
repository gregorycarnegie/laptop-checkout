use laptop_checkout::db;
use laptop_checkout::repo;
use laptop_checkout::view_model::{self as vm, LaptopFilter};
use pretty_assertions::assert_eq;
use rstest::rstest;
use serde_json::json;

use crate::common::{fresh, now, seeded, TestResult};

#[rstest]
fn the_example_data_tells_a_realistic_story(_seeded: ()) {
    let open = repo::open_loans();
    let laptops = repo::laptops();
    assert_eq!(repo::borrowers().len(), 14);
    assert_eq!(laptops.len(), 16);
    assert_eq!(repo::all_loans().len(), 13);
    assert_eq!(open.len(), 8);
    assert_eq!(vm::late(&open, now()).len(), 3);
    assert_eq!(vm::due_soon(&open, now()).len(), 2);
    assert_eq!(vm::on_shelf_count(&laptops), 6);
    assert_eq!(laptops.iter().filter(|l| LaptopFilter::Repair.keep(l, now())).count(), 1);
    assert_eq!(laptops.iter().filter(|l| LaptopFilter::Retired.keep(l, now())).count(), 1);
    assert!(repo::settings().sample_data);
}

#[rstest]
fn the_example_data_has_one_emailed_late_loan(_seeded: ()) {
    let emailed: Vec<String> =
        repo::open_loans().into_iter().filter(|l| l.emails_sent > 0).map(|l| l.asset_tag).collect();
    assert_eq!(emailed, ["LT-0103"]);
}

#[rstest]
fn clearing_records_keeps_templates_and_settings(_seeded: ()) -> TestResult {
    repo::set_setting("org_name", "Hillside")?;
    repo::clear_records()?;
    for table in ["borrowers", "laptops", "loans", "email_log"] {
        assert_eq!(db::scalar(&format!("SELECT count(*) FROM {table}"), json!([])), 0, "{table}");
    }
    assert_eq!(repo::templates().len(), 4);
    let s = repo::settings();
    assert_eq!((s.org_name.as_str(), s.sample_data), ("Hillside", false));
    Ok(())
}

#[rstest]
fn the_example_data_cannot_be_loaded_twice(_seeded: ()) {
    assert!(repo::seed_sample().is_err(), "asset tags are unique");
    assert_eq!(repo::laptops().len(), 16);
}

#[rstest]
fn migrating_twice_is_harmless(_fresh: ()) -> TestResult {
    repo::migrate()?;
    repo::migrate()
}

#[rstest]
fn one_open_loan_per_laptop_is_enforced_by_the_database(_seeded: ()) {
    let r = db::exec(
        "INSERT INTO loans (laptop_id, borrower_id, out_at, due_at) SELECT laptop_id, borrower_id, 0, 1 FROM loans WHERE returned_at IS NULL LIMIT 1",
        json!([]),
    );
    assert!(r.is_err());
}

/// The whole example history, one line per loan, so any change to the story
/// the example data tells shows up in review.
#[rstest]
fn the_example_loan_history_is_stable(_seeded: ()) {
    use laptop_checkout::time;
    let lines: Vec<String> = repo::all_loans()
        .into_iter()
        .map(|l| {
            let (stamp, detail) = vm::stamp_text(l.due_at, l.returned_at, time::now());
            format!(
                "{} → {:<16} out {:<18} due {:<10} {} ({}) · {}× emailed",
                l.asset_tag,
                l.borrower_name,
                time::stamp(l.out_at),
                time::short(l.due_at),
                stamp,
                detail,
                l.emails_sent,
            )
        })
        .collect();
    insta::assert_snapshot!(lines.join("\n"));
}
