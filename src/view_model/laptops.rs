//! The Laptops page: filters and status.

use super::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LaptopFilter {
    InService,
    Shelf,
    Out,
    Late,
    Repair,
    Retired,
}

impl LaptopFilter {
    pub const OPTIONS: [(LaptopFilter, &'static str); 6] = [
        (LaptopFilter::InService, "In service"),
        (LaptopFilter::Shelf, "On the shelf"),
        (LaptopFilter::Out, "On loan"),
        (LaptopFilter::Late, "Overdue"),
        (LaptopFilter::Repair, "In repair"),
        (LaptopFilter::Retired, "Retired"),
    ];

    pub fn keep(self, l: &Laptop, now: i64) -> bool {
        match self {
            LaptopFilter::InService => l.status != ServiceStatus::Retired,
            LaptopFilter::Shelf => l.status == ServiceStatus::Available && !l.on_loan(),
            LaptopFilter::Out => l.on_loan(),
            LaptopFilter::Late => l.due_at.is_some_and(|d| d < now),
            LaptopFilter::Repair => l.status == ServiceStatus::Repair,
            LaptopFilter::Retired => l.status == ServiceStatus::Retired,
        }
    }
}

pub fn laptop_matches(l: &Laptop, query: &str) -> bool {
    matches(&format!("{} {} {} {}", l.asset_tag, l.model, l.serial, l.borrower_name.as_deref().unwrap_or("")), query)
}

pub fn filter_laptops(laptops: &[Laptop], f: LaptopFilter, query: &str, now: i64) -> Vec<Laptop> {
    laptops.iter().filter(|l| f.keep(l, now) && laptop_matches(l, query)).cloned().collect()
}

/// What the Status column shows for a laptop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaptopStatus {
    OnLoan,
    Repair,
    Retired,
    Shelf,
}

pub fn laptop_status(l: &Laptop) -> LaptopStatus {
    match (l.on_loan(), l.status) {
        (true, _) => LaptopStatus::OnLoan,
        (false, ServiceStatus::Repair) => LaptopStatus::Repair,
        (false, ServiceStatus::Retired) => LaptopStatus::Retired,
        (false, ServiceStatus::Available) => LaptopStatus::Shelf,
    }
}

pub fn on_shelf_count(laptops: &[Laptop]) -> usize {
    laptops.iter().filter(|l| LaptopFilter::Shelf.keep(l, 0)).count()
}

/// Toast after changing a laptop's status.
pub fn status_message(tag: &str, status: ServiceStatus) -> String {
    let label = match status {
        ServiceStatus::Repair => "in repair",
        ServiceStatus::Retired => "retired",
        ServiceStatus::Available => "back in service",
    };
    format!("{tag} is {label}.")
}
