use laptop_checkout::{db, models::Settings, repo};
use pretty_assertions::assert_eq;
use rstest::rstest;
use serde_json::json;

use crate::common::{fresh, TestResult};

#[rstest]
fn settings_have_sensible_defaults(_fresh: ()) {
    assert_eq!(
        repo::settings(),
        Settings {
            org_name: "IT Services".into(),
            sender_name: "The IT Desk".into(),
            return_location: "the IT desk".into(),
            loan_days: 7,
            email_app: "mailto".into(),
            notify_enabled: true,
            renotify_hours: 24,
            late_template: None,
            sample_data: false,
        }
    );
}

#[rstest]
fn every_setting_can_be_changed(_fresh: ()) -> TestResult {
    for (k, v) in [
        ("org_name", "Hillside Academy"),
        ("sender_name", "Ms Patel"),
        ("return_location", "the library"),
        ("loan_days", "14"),
        ("email_app", "gmail"),
        ("notify_enabled", "0"),
        ("renotify_hours", "4"),
        ("late_template", "2"),
        ("sample_data", "1"),
    ] {
        repo::set_setting(k, v)?;
    }
    assert_eq!(
        repo::settings(),
        Settings {
            org_name: "Hillside Academy".into(),
            sender_name: "Ms Patel".into(),
            return_location: "the library".into(),
            loan_days: 14,
            email_app: "gmail".into(),
            notify_enabled: false,
            renotify_hours: 4,
            late_template: Some(2),
            sample_data: true,
        }
    );
    Ok(())
}

#[rstest]
fn setting_a_value_twice_keeps_the_latest(_fresh: ()) -> TestResult {
    repo::set_setting("loan_days", "3")?;
    repo::set_setting("loan_days", "5")?;
    assert_eq!(repo::settings().loan_days, 5);
    assert_eq!(db::scalar("SELECT count(*) FROM settings WHERE key = 'loan_days'", json!([])), 1);
    Ok(())
}

#[rstest]
#[case("loan_days", "a week")]
#[case("renotify_hours", "")]
#[case("late_template", "none")]
fn unreadable_numbers_fall_back_to_defaults(_fresh: (), #[case] key: &str, #[case] value: &str) -> TestResult {
    repo::set_setting(key, value)?;
    let s = repo::settings();
    assert_eq!((s.loan_days, s.renotify_hours, s.late_template), (7, 24, None));
    Ok(())
}

#[rstest]
fn notifications_are_on_unless_switched_off(_fresh: ()) -> TestResult {
    repo::set_setting("notify_enabled", "yes")?;
    assert!(!repo::settings().notify_enabled, "only \"1\" means on once the setting exists");
    repo::set_setting("notify_enabled", "1")?;
    assert!(repo::settings().notify_enabled);
    Ok(())
}
