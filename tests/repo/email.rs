use laptop_checkout::db;
use laptop_checkout::email;
use laptop_checkout::repo;
use laptop_checkout::time::clock;
use pretty_assertions::assert_eq;
use rstest::rstest;
use serde_json::json;

use crate::common::{fresh, lend, now, TestResult};

#[rstest]
fn a_new_database_gets_the_standard_templates(_fresh: ()) {
    let names: Vec<String> = repo::templates().into_iter().map(|t| t.name).collect();
    assert_eq!(names, ["Final notice", "Overdue notice", "Due soon reminder", "Check-out receipt"]);
}

#[rstest]
fn templates_are_grouped_by_purpose_then_name(_fresh: ()) -> TestResult {
    repo::save_template(None, "zz general", "s", "b", "general")?;
    repo::save_template(None, "aa receipt", "s", "b", "receipt")?;
    repo::save_template(None, "A reminder", "s", "b", "reminder")?;
    let order: Vec<(String, String)> = repo::templates().into_iter().map(|t| (t.purpose, t.name)).collect();
    let purposes: Vec<&str> = order.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(purposes, ["overdue", "overdue", "reminder", "reminder", "receipt", "receipt", "general"]);
    assert_eq!(order[2].1, "A reminder");
    assert_eq!(order[4].1, "aa receipt");
    Ok(())
}

#[rstest]
fn running_migrations_again_does_not_duplicate_templates(_fresh: ()) -> TestResult {
    repo::migrate()?;
    assert_eq!(repo::templates().len(), email::default_templates().len());
    Ok(())
}

#[rstest]
fn a_template_can_be_added_and_edited(_fresh: ()) -> TestResult {
    let id = repo::save_template(None, "  Lost laptop ", "Lost: {{asset_tag}}", "Body", "general")?;
    repo::save_template(Some(id), "Lost laptop", "Still lost", "New body", "overdue")?;
    let t = repo::templates().into_iter().find(|t| t.id == id).ok_or("missing")?;
    assert_eq!(
        (t.name.as_str(), t.subject.as_str(), t.body.as_str(), t.purpose.as_str()),
        ("Lost laptop", "Still lost", "New body", "overdue")
    );
    Ok(())
}

#[rstest]
#[case("", "Subject", "Give the template a name.")]
#[case("Name", "  ", "Add a subject line.")]
fn a_template_needs_a_name_and_subject(_fresh: (), #[case] name: &str, #[case] subject: &str, #[case] error: &str) {
    assert_eq!(repo::save_template(None, name, subject, "", "general"), Err(error.into()));
}

#[rstest]
fn deleted_standard_templates_can_be_restored(_fresh: ()) -> TestResult {
    for t in repo::templates().into_iter().filter(|t| t.purpose == "overdue") {
        repo::delete_template(t.id)?;
    }
    assert_eq!(repo::templates().len(), 2);
    assert_eq!(repo::restore_default_templates()?, 2);
    assert_eq!(repo::templates().len(), 4);
    assert_eq!(repo::restore_default_templates()?, 0);
    Ok(())
}

#[rstest]
fn logging_an_email_updates_the_loan_and_the_log(_fresh: ()) -> TestResult {
    let id = lend("LT-1", "Amara Okafor", -2)?;
    let loan = repo::loan(id).ok_or("missing")?;
    repo::log_email(&loan, "amara.okafor@example.org", "Overdue", "Overdue notice")?;
    clock::advance(60_000);
    repo::log_email(&loan, "amara.okafor@example.org", "Final", "Final notice")?;
    let l = repo::loan(id).ok_or("missing")?;
    assert_eq!((l.emails_sent, l.last_emailed_at), (2, Some(now() + 60_000)));
    let log = repo::email_log(10);
    assert_eq!(log.len(), 2);
    let latest = &log[0];
    assert_eq!(
        (latest.subject.as_str(), latest.template.as_str(), latest.to_addr.as_str(), latest.sent_at),
        ("Final", "Final notice", "amara.okafor@example.org", now() + 60_000)
    );
    assert_eq!((latest.borrower_name.as_deref(), latest.asset_tag.as_deref()), (Some("Amara Okafor"), Some("LT-1")));
    Ok(())
}

#[rstest]
fn the_email_log_is_limited(_fresh: ()) -> TestResult {
    let id = lend("LT-1", "Amara", -2)?;
    let loan = repo::loan(id).ok_or("missing")?;
    for i in 0..5 {
        clock::advance(1);
        repo::log_email(&loan, "a@b.org", &format!("#{i}"), "")?;
    }
    let subjects: Vec<String> = repo::email_log(3).into_iter().map(|e| e.subject).collect();
    assert_eq!(subjects, ["#4", "#3", "#2"]);
    Ok(())
}

#[rstest]
fn a_failed_log_changes_nothing(_fresh: ()) -> TestResult {
    let id = lend("LT-1", "Amara", -2)?;
    let loan = repo::loan(id).ok_or("missing")?;
    db::run_script("DROP TABLE email_log")?;
    assert!(repo::log_email(&loan, "a@b.org", "s", "t").is_err());
    assert_eq!(db::scalar("SELECT emails_sent FROM loans WHERE id = ?", json!([id])), 0);
    Ok(())
}
