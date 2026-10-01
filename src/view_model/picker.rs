//! The type-ahead pickers on the Desk.

use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickItem {
    pub id: i64,
    pub label: String,
    pub sub: String,
    /// Lower-case text searched when typing.
    pub search: String,
    /// Lower-case values that select this item outright on Enter (asset tag, email, ID).
    pub exact: Vec<String>,
    pub warning: Option<String>,
}

pub fn find_item(items: &[PickItem], id: i64) -> Option<PickItem> {
    items.iter().find(|i| i.id == id).cloned()
}

/// Picker suggestions shown at once.
pub const PICKER_LIMIT: usize = 8;

pub fn pick_matches(items: &[PickItem], query: &str) -> Vec<PickItem> {
    let words: Vec<String> = query.to_lowercase().split_whitespace().map(String::from).collect();
    items.iter().filter(|i| words.iter().all(|w| i.search.contains(w.as_str()))).take(PICKER_LIMIT).cloned().collect()
}

/// What Enter selects: an exact tag/email/ID match first, else the highlighted suggestion.
pub fn pick_on_enter(items: &[PickItem], query: &str, cursor: usize) -> Option<i64> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return None;
    }
    if let Some(hit) = items.iter().find(|i| i.exact.contains(&q)) {
        return Some(hit.id);
    }
    pick_matches(items, &q).get(cursor).map(|i| i.id)
}

/// Moves the highlighted suggestion, staying within `len` items.
pub fn move_cursor(cursor: usize, down: bool, len: usize) -> usize {
    if down {
        if len == 0 {
            cursor
        } else {
            (cursor + 1).min(len - 1)
        }
    } else {
        cursor.saturating_sub(1)
    }
}

fn nonempty_lower(values: &[&str]) -> Vec<String> {
    values.iter().filter(|s| !s.is_empty()).map(|s| s.to_lowercase()).collect()
}

/// Laptops that can be checked out right now.
pub fn available_laptop_items(laptops: &[Laptop]) -> Vec<PickItem> {
    laptops
        .iter()
        .filter(|l| l.status == ServiceStatus::Available && !l.on_loan())
        .map(|l| PickItem {
            id: l.id,
            label: l.asset_tag.clone(),
            sub: l.model.clone(),
            search: format!("{} {} {}", l.asset_tag, l.model, l.serial).to_lowercase(),
            exact: nonempty_lower(&[&l.asset_tag, &l.serial]),
            warning: None,
        })
        .collect()
}

/// A heads-up shown when picking a borrower who already has laptops out.
pub fn borrower_warning(open: i64, late: i64) -> Option<String> {
    match (open, late) {
        (0, _) => None,
        (n, 0) => Some(format!("Already has {} out", time::plural(n, "laptop"))),
        (n, l) => Some(format!("Has {} out, {l} overdue", time::plural(n, "laptop"))),
    }
}

pub fn borrower_items(borrowers: &[Borrower]) -> Vec<PickItem> {
    borrowers
        .iter()
        .filter(|b| b.is_active())
        .map(|b| PickItem {
            id: b.id,
            label: b.name.clone(),
            sub: [b.email.as_str(), b.department.as_str()]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" · "),
            search: format!("{} {} {} {}", b.name, b.email, b.department, b.external_id).to_lowercase(),
            exact: nonempty_lower(&[&b.email, &b.external_id]),
            warning: borrower_warning(b.open_loans, b.late_loans),
        })
        .collect()
}

/// Laptops on loan, for the check-in picker.
pub fn return_items(open: &[Loan]) -> Vec<PickItem> {
    open.iter()
        .map(|l| PickItem {
            id: l.id,
            label: l.asset_tag.clone(),
            sub: format!("{} · due {}", l.borrower_name, time::short(l.due_at)),
            search: format!("{} {} {} {}", l.asset_tag, l.serial, l.borrower_name, l.borrower_email).to_lowercase(),
            exact: nonempty_lower(&[&l.asset_tag, &l.serial]),
            warning: None,
        })
        .collect()
}
