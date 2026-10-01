//! Loan lists, due-date stamps and loan messages.

use super::*;

/// Open loans past their due date.
pub fn late(open: &[Loan], now: i64) -> Vec<Loan> {
    open.iter().filter(|l| l.due_at < now).cloned().collect()
}

/// Open loans due today or tomorrow that aren't late yet.
pub fn due_soon(open: &[Loan], now: i64) -> Vec<Loan> {
    open.iter().filter(|l| l.due_at >= now && time::calendar_days(now, l.due_at) <= 1).cloned().collect()
}

/// IDs of loans whose borrower has an email address to write to.
pub fn emailable(loans: &[Loan]) -> Vec<i64> {
    loans.iter().filter(|l| !l.borrower_email.is_empty()).map(|l| l.id).collect()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LoanFilter {
    Open,
    Late,
    Returned,
    All,
}

impl LoanFilter {
    pub const OPTIONS: [(LoanFilter, &'static str); 4] = [
        (LoanFilter::Open, "Out now"),
        (LoanFilter::Late, "Overdue"),
        (LoanFilter::Returned, "Returned"),
        (LoanFilter::All, "All"),
    ];

    pub fn keep(self, l: &Loan, now: i64) -> bool {
        match self {
            LoanFilter::Open => l.returned_at.is_none(),
            LoanFilter::Late => l.returned_at.is_none() && l.due_at < now,
            LoanFilter::Returned => l.returned_at.is_some(),
            LoanFilter::All => true,
        }
    }
}

/// Loans the Loans page shows for a filter and search.
pub fn filter_loans(loans: &[Loan], f: LoanFilter, query: &str, now: i64) -> Vec<Loan> {
    loans.iter().filter(|l| f.keep(l, now) && loan_matches(l, query)).cloned().collect()
}

pub fn find_loan(loans: &[Loan], id: i64) -> Option<Loan> {
    loans.iter().find(|l| l.id == id).cloned()
}

/// The "Email receipt" button offered after a check-out, if the borrower has an address.
pub fn receipt_action(l: &Loan) -> Option<(String, ToastAction)> {
    (!l.borrower_email.is_empty())
        .then(|| ("Email receipt".to_string(), ToastAction::EmailLoan { loan_id: l.id, purpose: "receipt" }))
}

pub fn loan_matches(l: &Loan, query: &str) -> bool {
    matches(&format!("{} {} {} {} {}", l.asset_tag, l.model, l.borrower_name, l.borrower_email, l.department), query)
}

/// Headline and detail text on a due-date stamp.
pub fn stamp_text(due: i64, returned: Option<i64>, now: i64) -> (String, String) {
    match returned {
        Some(r) => {
            let late = time::days_late(due, r);
            let note = if late > 0 { format!("{} late", time::plural(late, "day")) } else { "on time".into() };
            (format!("In {}", time::short(r)), note)
        }
        None => (time::describe_due(due, now), format!("due {}", time::short(due))),
    }
}

pub fn stamp_class(due: i64, returned: Option<i64>, now: i64) -> &'static str {
    DueState::of(due, returned, now).class()
}

/// "Emailed 2× · last Mon 28 Sep", if the borrower has been emailed.
pub fn emailed_text(l: &Loan) -> Option<String> {
    (l.emails_sent > 0).then(|| {
        format!("Emailed {}× · last {}", l.emails_sent, l.last_emailed_at.map(time::short).unwrap_or_default())
    })
}

pub fn renewed_text(l: &Loan) -> Option<String> {
    (l.renewals > 0).then(|| format!("Renewed {}", time::plural(l.renewals, "time")))
}

/// Which template kind to offer when emailing about a loan.
pub fn email_purpose(l: &Loan, now: i64) -> &'static str {
    if l.returned_at.is_none() && l.due_at < now {
        "overdue"
    } else {
        "reminder"
    }
}

pub fn returned_message(l: &Loan) -> String {
    let late = time::days_late(l.due_at, l.returned_at.unwrap_or_else(time::now));
    if late > 0 {
        format!("{} returned by {}, {} late.", l.asset_tag, l.borrower_name, time::plural(late, "day"))
    } else {
        format!("{} returned by {}. Thanks!", l.asset_tag, l.borrower_name)
    }
}

pub fn checked_out_message(l: &Loan) -> String {
    format!("{} checked out to {}. Due {}.", l.asset_tag, l.borrower_name, time::short(l.due_at))
}

pub fn renewed_message(tag: &str, due: i64) -> String {
    format!("Renewed {tag}. Now due {}.", time::short(due))
}
