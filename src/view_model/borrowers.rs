//! The Borrowers page: filters and labels.

use super::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BorrowerFilter {
    Active,
    WithLaptop,
    Late,
    Inactive,
}

impl BorrowerFilter {
    pub const OPTIONS: [(BorrowerFilter, &'static str); 4] = [
        (BorrowerFilter::Active, "Active"),
        (BorrowerFilter::WithLaptop, "Has a laptop"),
        (BorrowerFilter::Late, "Overdue"),
        (BorrowerFilter::Inactive, "Inactive"),
    ];

    pub fn keep(self, b: &Borrower) -> bool {
        match self {
            BorrowerFilter::Active => b.is_active(),
            BorrowerFilter::WithLaptop => b.open_loans > 0,
            BorrowerFilter::Late => b.late_loans > 0,
            BorrowerFilter::Inactive => !b.is_active(),
        }
    }
}

pub fn borrower_matches(b: &Borrower, query: &str) -> bool {
    matches(&format!("{} {} {} {}", b.name, b.email, b.department, b.external_id), query)
}

pub fn filter_borrowers(borrowers: &[Borrower], f: BorrowerFilter, query: &str) -> Vec<Borrower> {
    borrowers.iter().filter(|b| f.keep(b) && borrower_matches(b, query)).cloned().collect()
}

/// The "Laptops out" cell for a borrower: (CSS class, text).
pub fn loans_out_label(open: i64, late: i64) -> (&'static str, String) {
    match (open, late) {
        (0, _) => ("muted", "None".into()),
        (n, 0) => ("pill", format!("{n} out")),
        (n, l) => ("pill late", format!("{n} out · {l} late")),
    }
}

/// Whether a saved add/edit form stays open for the next entry.
pub fn keep_form_open(adding: bool, add_another: bool) -> bool {
    adding && add_another
}
