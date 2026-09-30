//! Deciding which overdue alerts to raise.
//!
//! Each overdue loan triggers an alert, then stays quiet for the "remind again"
//! period in Settings. How alerts are shown (desktop notifications, in-page
//! toasts) is up to an [`AlertSink`], so the rules can be tested with a mock.

use crate::{
    models::{Loan, Settings},
    repo, time,
};

/// One desktop notification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    pub title: String,
    pub body: String,
    /// Notifications with the same tag replace each other.
    pub tag: String,
}

/// The button offered with the in-page alert.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlertAction {
    EmailLoan(i64),
    EmailAllLate,
}

/// What to show for a batch of overdue loans.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlertPlan {
    pub notes: Vec<Note>,
    pub toast: String,
    pub action_label: &'static str,
    pub action: AlertAction,
    pub loan_ids: Vec<i64>,
}

/// Up to this many loans get a notification each; more get one summary.
pub const MAX_SEPARATE_NOTES: usize = 3;
/// Names listed in the summary notification.
pub const SUMMARY_NAMES: usize = 4;

pub fn plan(loans: &[Loan], now: i64) -> Option<AlertPlan> {
    let first = loans.first()?;
    let notes = if loans.len() <= MAX_SEPARATE_NOTES {
        loans
            .iter()
            .map(|l| Note {
                title: format!("{} is overdue", l.asset_tag),
                body: format!(
                    "{} · {} · due {}",
                    l.borrower_name,
                    time::describe_due(l.due_at, now),
                    time::short(l.due_at)
                ),
                tag: format!("loan-{}", l.id),
            })
            .collect()
    } else {
        let names: Vec<&str> = loans.iter().take(SUMMARY_NAMES).map(|l| l.borrower_name.as_str()).collect();
        let more = if loans.len() > SUMMARY_NAMES { "…" } else { "" };
        vec![Note {
            title: format!("{} laptops are overdue", loans.len()),
            body: format!("{}{more}", names.join(", ")),
            tag: "overdue-summary".into(),
        }]
    };
    let single = loans.len() == 1;
    Some(AlertPlan {
        notes,
        toast: if single {
            format!(
                "{} is overdue ({}, {}).",
                first.asset_tag,
                first.borrower_name,
                time::describe_due(first.due_at, now)
            )
        } else {
            format!("{} laptops are overdue.", loans.len())
        },
        action_label: if single { "Email borrower" } else { "Email all late" },
        action: if single { AlertAction::EmailLoan(first.id) } else { AlertAction::EmailAllLate },
        loan_ids: loans.iter().map(|l| l.id).collect(),
    })
}

/// Shows alerts to the user.
#[cfg_attr(all(test, not(target_arch = "wasm32")), mockall::automock)]
pub trait AlertSink {
    /// Whether desktop notifications may be shown.
    fn can_notify(&self) -> bool;
    fn notify(&self, note: &Note);
    fn toast(&self, text: &str, action_label: &'static str, action: AlertAction);
}

/// Runs one check: finds loans that need an alert, shows them, and records
/// that they were alerted. Returns how many loans were alerted.
pub fn check(now: i64, settings: &Settings, sink: &dyn AlertSink) -> usize {
    if !settings.notify_enabled {
        return 0;
    }
    let loans = repo::loans_needing_alert(now, settings.renotify_hours);
    let Some(plan) = plan(&loans, now) else { return 0 };
    if sink.can_notify() {
        for note in &plan.notes {
            sink.notify(note);
        }
    }
    sink.toast(&plan.toast, plan.action_label, plan.action);
    repo::mark_notified(&plan.loan_ids, now);
    plan.loan_ids.len()
}

// Native only: these use test crates that don't build for WebAssembly.
#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
mod tests {
    use super::*;
    use crate::{
        test_support::{at, fresh, now, seeded},
        time::{clock, end_of_day, HOUR},
    };
    use mockall::predicate::{always, eq};
    use pretty_assertions::assert_eq;
    use rstest::rstest;

    fn loan(id: i64, tag: &str, who: &str, due: i64) -> Loan {
        Loan {
            id,
            laptop_id: id,
            borrower_id: id,
            out_at: due - 7 * DAY_MS,
            due_at: due,
            returned_at: None,
            note: String::new(),
            last_notified_at: None,
            last_emailed_at: None,
            emails_sent: 0,
            renewals: 0,
            asset_tag: tag.into(),
            model: String::new(),
            serial: String::new(),
            borrower_name: who.into(),
            borrower_email: String::new(),
            department: String::new(),
        }
    }

    const DAY_MS: i64 = crate::time::DAY;

    fn late_loans(n: usize) -> Vec<Loan> {
        let due = end_of_day(at(2026, 10, 2, 0, 0));
        (1..=n as i64).map(|i| loan(i, &format!("LT-{i}"), &format!("Person {i}"), due)).collect()
    }

    // ------------------------------------------------------------ plan

    #[test]
    fn nothing_overdue_means_no_plan() {
        assert_eq!(plan(&[], now()), None);
    }

    #[test]
    fn one_late_loan_gets_its_own_notification_and_an_email_button() {
        let p = plan(&late_loans(1), now()).unwrap();
        assert_eq!(
            p,
            AlertPlan {
                notes: vec![Note {
                    title: "LT-1 is overdue".into(),
                    body: "Person 1 · 4 days late · due Fri 2 Oct".into(),
                    tag: "loan-1".into(),
                }],
                toast: "LT-1 is overdue (Person 1, 4 days late).".into(),
                action_label: "Email borrower",
                action: AlertAction::EmailLoan(1),
                loan_ids: vec![1],
            }
        );
    }

    #[test]
    fn up_to_three_late_loans_get_a_notification_each() {
        let p = plan(&late_loans(MAX_SEPARATE_NOTES), now()).unwrap();
        let tags: Vec<&str> = p.notes.iter().map(|n| n.tag.as_str()).collect();
        assert_eq!(tags, ["loan-1", "loan-2", "loan-3"]);
        assert_eq!(p.toast, "3 laptops are overdue.");
        assert_eq!((p.action_label, p.action), ("Email all late", AlertAction::EmailAllLate));
    }

    #[test]
    fn four_late_loans_share_one_summary_listing_everyone() {
        let p = plan(&late_loans(4), now()).unwrap();
        assert_eq!(
            p.notes,
            vec![Note {
                title: "4 laptops are overdue".into(),
                body: "Person 1, Person 2, Person 3, Person 4".into(),
                tag: "overdue-summary".into(),
            }]
        );
    }

    #[test]
    fn a_long_summary_lists_four_names_then_trails_off() {
        let p = plan(&late_loans(6), now()).unwrap();
        assert_eq!(p.notes[0].body, "Person 1, Person 2, Person 3, Person 4…");
        assert_eq!(p.loan_ids, vec![1, 2, 3, 4, 5, 6]);
    }

    // ------------------------------------------------------------ check (with a mock sink)

    fn settings() -> Settings {
        repo::settings()
    }

    #[rstest]
    fn check_notifies_and_toasts_each_overdue_loan_once(_seeded: ()) {
        let mut sink = MockAlertSink::new();
        sink.expect_can_notify().times(1).return_const(true);
        sink.expect_notify().times(3).return_const(());
        sink.expect_toast()
            .with(eq("3 laptops are overdue."), eq("Email all late"), eq(AlertAction::EmailAllLate))
            .times(1)
            .return_const(());
        assert_eq!(check(now(), &settings(), &sink), 3);

        // A minute later nothing is repeated.
        let quiet = MockAlertSink::new();
        assert_eq!(check(now() + 60_000, &settings(), &quiet), 0);
    }

    #[rstest]
    fn check_reminds_again_after_the_quiet_period(_seeded: ()) {
        let mut first = MockAlertSink::new();
        first.expect_can_notify().return_const(false);
        first.expect_toast().return_const(());
        check(now(), &settings(), &first);

        // A day later the same three are due a reminder, and the laptop that
        // was due today has become late too.
        let hours = settings().renotify_hours;
        let mut later = MockAlertSink::new();
        later.expect_can_notify().return_const(false);
        later.expect_toast().with(eq("4 laptops are overdue."), always(), always()).times(1).return_const(());
        assert_eq!(check(now() + hours * HOUR + 1, &settings(), &later), 4);
    }

    #[rstest]
    fn check_only_toasts_when_notifications_are_blocked(_seeded: ()) {
        let mut sink = MockAlertSink::new();
        sink.expect_can_notify().return_const(false);
        sink.expect_notify().never();
        sink.expect_toast().with(always(), always(), always()).times(1).return_const(());
        check(now(), &settings(), &sink);
    }

    #[rstest]
    fn check_does_nothing_when_alerts_are_off(_seeded: ()) {
        repo::set_setting("notify_enabled", "0").unwrap();
        let sink = MockAlertSink::new();
        assert_eq!(check(now(), &settings(), &sink), 0);
    }

    #[rstest]
    fn check_does_nothing_when_nothing_is_late(_fresh: ()) {
        let sink = MockAlertSink::new();
        assert_eq!(check(now(), &settings(), &sink), 0);
    }

    #[rstest]
    fn a_loan_becomes_alertable_the_moment_it_is_late(_seeded: ()) {
        let mut sink = MockAlertSink::new();
        sink.expect_can_notify().return_const(false);
        sink.expect_toast().return_const(());
        check(now(), &settings(), &sink);
        // The loan due at the end of today is not late yet...
        let end_of_today = end_of_day(now());
        let quiet = MockAlertSink::new();
        assert_eq!(check(end_of_today, &settings(), &quiet), 0);
        // ...and is the only one alerted a millisecond later.
        clock::set_now(end_of_today + 1);
        let mut sink = MockAlertSink::new();
        sink.expect_can_notify().return_const(false);
        sink.expect_toast()
            .withf(|text, _, action| {
                text.starts_with("LT-0101 is overdue") && matches!(action, AlertAction::EmailLoan(_))
            })
            .times(1)
            .return_const(());
        assert_eq!(check(end_of_today + 1, &settings(), &sink), 1);
    }
}
