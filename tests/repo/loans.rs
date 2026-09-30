use laptop_checkout::{
    db, repo,
    time::{clock, end_of_day, DAY, HOUR},
};
use pretty_assertions::assert_eq;
use rstest::rstest;
use serde_json::json;

use crate::common::{at, due_in, fresh, lend, machine, now, person, seeded, TestResult};

fn setup() -> Result<(i64, i64), String> {
    Ok((repo::add_laptop(&machine("LT-1"))?, repo::add_borrower(&person("Amara Okafor", "amara@example.org"))?))
}

// ---------------------------------------------------------------- checking out

#[rstest]
fn checking_out_records_the_loan(_fresh: ()) -> TestResult {
    let (laptop, borrower) = setup()?;
    let id = repo::check_out(laptop, borrower, due_in(7), "  with charger ")?;
    let l = repo::loan(id).ok_or("missing")?;
    assert_eq!(
        (l.laptop_id, l.borrower_id, l.out_at, l.due_at, l.returned_at, l.note.as_str()),
        (laptop, borrower, now(), due_in(7), None, "with charger")
    );
    assert_eq!(
        (l.asset_tag.as_str(), l.borrower_name.as_str(), l.borrower_email.as_str()),
        ("LT-1", "Amara Okafor", "amara@example.org")
    );
    assert_eq!((l.emails_sent, l.renewals, l.last_emailed_at, l.last_notified_at), (0, 0, None, None));
    Ok(())
}

#[rstest]
fn a_laptop_already_out_cannot_be_checked_out_again(_fresh: ()) -> TestResult {
    let (laptop, borrower) = setup()?;
    repo::check_out(laptop, borrower, due_in(7), "")?;
    assert_eq!(
        repo::check_out(laptop, borrower, due_in(7), ""),
        Err("LT-1 is already checked out. Check it in first.".into())
    );
    Ok(())
}

#[rstest]
#[case("repair", "LT-1 is marked as in repair.")]
#[case("retired", "LT-1 is retired.")]
fn laptops_out_of_service_cannot_be_lent(_fresh: (), #[case] status: &str, #[case] error: &str) -> TestResult {
    let (laptop, borrower) = setup()?;
    repo::set_laptop_status(laptop, status)?;
    assert_eq!(repo::check_out(laptop, borrower, due_in(7), ""), Err(error.into()));
    Ok(())
}

#[rstest]
fn a_missing_laptop_cannot_be_lent(_fresh: ()) -> TestResult {
    let (_, borrower) = setup()?;
    assert_eq!(repo::check_out(99, borrower, due_in(7), ""), Err("That laptop no longer exists.".into()));
    Ok(())
}

#[rstest]
#[case::now(0)]
#[case::yesterday(-DAY)]
fn the_due_date_must_be_in_the_future(_fresh: (), #[case] offset: i64) -> TestResult {
    let (laptop, borrower) = setup()?;
    assert_eq!(repo::check_out(laptop, borrower, now() + offset, ""), Err("Pick a due date in the future.".into()));
    assert!(repo::check_out(laptop, borrower, now() + 1, "").is_ok());
    Ok(())
}

// ---------------------------------------------------------------- checking in

#[rstest]
fn checking_in_records_the_return_time(_fresh: ()) -> TestResult {
    let id = lend("LT-1", "Amara", 7)?;
    clock::advance(2 * HOUR);
    let l = repo::check_in(id, "")?;
    assert_eq!(l.returned_at, Some(now() + 2 * HOUR));
    assert!(repo::open_loans().is_empty());
    Ok(())
}

#[rstest]
#[case::no_notes("", "", "")]
#[case::return_note_only("", " no charger ", "Returned: no charger")]
#[case::both("with case", "scratched", "with case\nReturned: scratched")]
#[case::keeps_the_old_note("with case", "   ", "with case")]
fn a_condition_note_is_added_to_the_loan(
    _fresh: (),
    #[case] out_note: &str,
    #[case] in_note: &str,
    #[case] expected: &str,
) -> TestResult {
    let (laptop, borrower) = setup()?;
    let id = repo::check_out(laptop, borrower, due_in(7), out_note)?;
    assert_eq!(repo::check_in(id, in_note)?.note, expected);
    Ok(())
}

#[rstest]
fn checking_in_twice_keeps_the_first_return(_fresh: ()) -> TestResult {
    let id = lend("LT-1", "Amara", 7)?;
    repo::check_in(id, "first")?;
    clock::advance(DAY);
    let l = repo::check_in(id, "second")?;
    assert_eq!((l.returned_at, l.note.as_str()), (Some(now()), "Returned: first"));
    Ok(())
}

#[rstest]
fn checking_in_a_missing_loan_is_an_error(_fresh: ()) {
    assert_eq!(repo::check_in(42, "").unwrap_err(), "That loan no longer exists.");
}

// ---------------------------------------------------------------- renewing

#[rstest]
fn renewing_a_loan_on_time_extends_it_from_the_due_date(_fresh: ()) -> TestResult {
    let id = lend("LT-1", "Amara", 3)?;
    let due = repo::renew(id, 7)?;
    assert_eq!(due, end_of_day(now() + 10 * DAY));
    let l = repo::loan(id).ok_or("missing")?;
    assert_eq!((l.due_at, l.renewals), (due, 1));
    Ok(())
}

#[rstest]
fn renewing_a_late_loan_counts_from_today(_fresh: ()) -> TestResult {
    let id = lend("LT-1", "Amara", -5)?;
    assert_eq!(repo::renew(id, 7)?, end_of_day(now() + 7 * DAY));
    Ok(())
}

#[rstest]
fn renewing_clears_the_last_overdue_alert(_fresh: ()) -> TestResult {
    let id = lend("LT-1", "Amara", -5)?;
    repo::mark_notified(&[id], now());
    repo::renew(id, 7)?;
    assert_eq!(repo::loan(id).ok_or("missing")?.last_notified_at, None);
    Ok(())
}

#[rstest]
fn renewing_a_missing_loan_is_an_error(_fresh: ()) {
    assert_eq!(repo::renew(42, 7), Err("That loan no longer exists.".into()));
}

// ---------------------------------------------------------------- listing

#[rstest]
fn open_loans_are_listed_soonest_due_first(_seeded: ()) {
    let dues: Vec<i64> = repo::open_loans().iter().map(|l| l.due_at).collect();
    let mut sorted = dues.clone();
    sorted.sort();
    assert_eq!(dues, sorted);
    assert_eq!(dues.len(), 8);
}

#[rstest]
fn all_loans_list_open_ones_first_then_most_recent(_seeded: ()) {
    let all = repo::all_loans();
    let open = all.iter().take_while(|l| l.returned_at.is_none()).count();
    assert_eq!((open, all.len()), (8, 13));
    let returned: Vec<i64> = all[open..].iter().filter_map(|l| l.returned_at).collect();
    let mut newest_first = returned.clone();
    newest_first.sort_by(|a, b| b.cmp(a));
    assert_eq!(returned, newest_first);
    let open_dues: Vec<i64> = all[..open].iter().map(|l| l.due_at).collect();
    let mut latest_first = open_dues.clone();
    latest_first.sort_by(|a, b| b.cmp(a));
    assert_eq!(open_dues, latest_first);
}

#[rstest]
fn a_missing_loan_is_none(_fresh: ()) {
    assert_eq!(repo::loan(1), None);
}

// ---------------------------------------------------------------- overdue alerts

#[rstest]
fn only_late_open_loans_need_alerts(_fresh: ()) -> TestResult {
    let late = lend("LT-1", "Amara", -1)?;
    lend("LT-2", "Liam", 3)?;
    let returned = lend("LT-3", "Grace", -2)?;
    repo::check_in(returned, "")?;
    let ids: Vec<i64> = repo::loans_needing_alert(now(), 24).iter().map(|l| l.id).collect();
    assert_eq!(ids, [late]);
    Ok(())
}

#[rstest]
fn an_alerted_loan_waits_for_the_reminder_period(_fresh: ()) -> TestResult {
    let id = lend("LT-1", "Amara", -1)?;
    repo::mark_notified(&[id], now());
    assert!(repo::loans_needing_alert(now() + 4 * HOUR, 4).is_empty());
    assert_eq!(repo::loans_needing_alert(now() + 4 * HOUR + 1, 4).len(), 1);
    Ok(())
}

#[rstest]
fn the_reminder_period_is_at_least_an_hour(_fresh: ()) -> TestResult {
    let id = lend("LT-1", "Amara", -1)?;
    repo::mark_notified(&[id], now());
    assert!(repo::loans_needing_alert(now() + HOUR, 0).is_empty());
    assert_eq!(repo::loans_needing_alert(now() + HOUR + 1, 0).len(), 1);
    Ok(())
}

#[rstest]
fn mark_notified_stamps_each_loan(_fresh: ()) -> TestResult {
    let a = lend("LT-1", "Amara", -1)?;
    let b = lend("LT-2", "Liam", -1)?;
    let c = lend("LT-3", "Grace", -1)?;
    repo::mark_notified(&[a, b], at(2026, 10, 6, 11, 0));
    let stamped = db::scalar("SELECT count(*) FROM loans WHERE last_notified_at = ?", json!([at(2026, 10, 6, 11, 0)]));
    assert_eq!(stamped, 2);
    assert_eq!(repo::loan(c).ok_or("missing")?.last_notified_at, None);
    Ok(())
}
