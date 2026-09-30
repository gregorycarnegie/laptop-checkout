//! Shared set-up for the integration tests. Each test runs on its own thread,
//! and the database and clock are per-thread, so tests never interfere.
#![allow(dead_code)]

use laptop_checkout::models::{BorrowerInput, LaptopInput};
use laptop_checkout::time::{clock, days_from_civil, end_of_day, DAY, HOUR, MINUTE};
use laptop_checkout::{db, repo};
use rstest::fixture;

pub type TestResult = Result<(), String>;

/// A UTC instant; tests run with a zero timezone offset.
pub fn at(y: i64, m: u32, d: u32, h: i64, min: i64) -> i64 {
    days_from_civil(y, m, d) * DAY + h * HOUR + min * MINUTE
}

/// "Now" for every test: Tuesday 6 October 2026, 10:00.
pub fn now() -> i64 {
    at(2026, 10, 6, 10, 0)
}

/// The end of the day `days` from now.
pub fn due_in(days: i64) -> i64 {
    end_of_day(now() + days * DAY)
}

/// An empty, migrated database with the clock frozen at [`now`].
#[fixture]
pub fn fresh() {
    clock::set_now(now());
    db::open_empty().expect("open");
    repo::migrate().expect("migrate");
}

/// The example data the app starts with.
#[fixture]
pub fn seeded() {
    fresh();
    repo::seed_sample().expect("seed");
}

pub fn person(name: &str, email: &str) -> BorrowerInput {
    BorrowerInput { name: name.into(), email: email.into(), ..Default::default() }
}

pub fn machine(tag: &str) -> LaptopInput {
    LaptopInput { asset_tag: tag.into(), model: "Dell Latitude 3440".into(), ..Default::default() }
}

/// Adds a borrower and a laptop and lends one to the other, due in `days`
/// (negative for a loan that is already late: it is lent a week before it
/// fell due, by winding the clock back).
pub fn lend(tag: &str, name: &str, days: i64) -> Result<i64, String> {
    let email = format!("{}@example.org", name.to_lowercase().replace(' ', "."));
    let b = repo::add_borrower(&person(name, &email))?;
    let l = repo::add_laptop(&machine(tag))?;
    let due = due_in(days);
    if due <= now() {
        clock::set_now(due - 7 * DAY);
    }
    let id = repo::check_out(l, b, due, "");
    clock::set_now(now());
    id
}
