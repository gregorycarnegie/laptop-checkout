//! Settings and backups.

use super::*;

/// The Desk suggests turning notifications on only while the browser hasn't been asked yet.
pub fn show_notify_nudge(alerts_enabled: bool, permission: &str) -> bool {
    alerts_enabled && permission == "default"
}

pub const MAX_LOAN_DAYS: i64 = 365;

pub fn parse_loan_days(s: &str) -> Result<i64, String> {
    match s.trim().parse::<i64>() {
        Ok(n) if (1..=MAX_LOAN_DAYS).contains(&n) => Ok(n),
        _ => Err(format!("Enter a number of days between 1 and {MAX_LOAN_DAYS}.")),
    }
}

/// True when the user dismissed a file picker, which isn't worth an error message.
pub fn is_cancel(err: &str) -> bool {
    err.contains("aborted") || err.contains("AbortError") || err.contains("cancel")
}

/// File name for a backup downloaded on the day containing `now`.
pub fn backup_name(now: i64) -> String {
    format!("laptop-library-{}.sqlite", time::to_input(now))
}
