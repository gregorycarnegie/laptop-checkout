//! Row types decoded from SQLite query results.

use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize)]
pub struct Borrower {
    pub id: i64,
    pub name: String,
    pub email: String,
    pub department: String,
    pub external_id: String,
    pub phone: String,
    pub notes: String,
    pub active: i64,
    #[serde(default)]
    pub open_loans: i64,
    #[serde(default)]
    pub late_loans: i64,
    #[serde(default)]
    pub total_loans: i64,
}

impl Borrower {
    pub fn is_active(&self) -> bool {
        self.active != 0
    }

    /// Borrowers who have ever borrowed can't be deleted, only deactivated.
    pub fn has_history(&self) -> bool {
        self.total_loans > 0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize)]
pub struct Laptop {
    pub id: i64,
    pub asset_tag: String,
    pub model: String,
    pub serial: String,
    pub notes: String,
    /// `available`, `repair` or `retired`. Being on loan is worked out from `loans`.
    pub status: String,
    pub loan_id: Option<i64>,
    pub due_at: Option<i64>,
    pub borrower_name: Option<String>,
    #[serde(default)]
    pub total_loans: i64,
}

impl Laptop {
    pub fn on_loan(&self) -> bool {
        self.loan_id.is_some()
    }

    /// Laptops that have ever been lent can't be deleted, only retired.
    pub fn has_history(&self) -> bool {
        self.total_loans > 0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize)]
pub struct Loan {
    pub id: i64,
    pub laptop_id: i64,
    pub borrower_id: i64,
    pub out_at: i64,
    pub due_at: i64,
    pub returned_at: Option<i64>,
    pub note: String,
    pub last_notified_at: Option<i64>,
    pub last_emailed_at: Option<i64>,
    pub emails_sent: i64,
    pub renewals: i64,
    pub asset_tag: String,
    pub model: String,
    pub serial: String,
    pub borrower_name: String,
    pub borrower_email: String,
    pub department: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize)]
pub struct Template {
    pub id: i64,
    pub name: String,
    pub subject: String,
    pub body: String,
    /// `overdue`, `reminder`, `receipt` or `general`.
    pub purpose: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Deserialize)]
pub struct EmailLogEntry {
    pub id: i64,
    pub to_addr: String,
    pub subject: String,
    pub template: String,
    pub sent_at: i64,
    pub borrower_name: Option<String>,
    pub asset_tag: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub org_name: String,
    pub sender_name: String,
    pub return_location: String,
    pub loan_days: i64,
    pub email_app: String,
    pub notify_enabled: bool,
    pub renotify_hours: i64,
    pub late_template: Option<i64>,
    pub sample_data: bool,
}

/// Fields shared by the add/edit form and CSV import.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BorrowerInput {
    pub name: String,
    pub email: String,
    pub department: String,
    pub external_id: String,
    pub phone: String,
    pub notes: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LaptopInput {
    pub asset_tag: String,
    pub model: String,
    pub serial: String,
    pub notes: String,
}
