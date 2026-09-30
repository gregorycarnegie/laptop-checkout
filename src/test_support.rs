//! Fixtures shared by the unit tests: a fresh or seeded in-memory database and
//! a frozen clock. Each test runs on its own thread, so each gets its own
//! database and clock.

use std::cell::RefCell;
use std::rc::Rc;

use rstest::fixture;

use crate::db::{self, Change};
use crate::repo;
use crate::time::{clock, days_from_civil, DAY, HOUR, MINUTE};

/// A UTC instant (tests run with a zero timezone offset unless they set one).
pub fn at(y: i64, m: u32, d: u32, h: i64, min: i64) -> i64 {
    days_from_civil(y, m, d) * DAY + h * HOUR + min * MINUTE
}

/// "Now" in tests: Tuesday 6 October 2026, 10:00.
pub fn now() -> i64 {
    at(2026, 10, 6, 10, 0)
}

/// An empty, migrated database with the clock frozen at [`now`].
#[fixture]
pub fn fresh() {
    clock::set_now(now());
    db::open_empty().expect("open");
    repo::migrate().expect("migrate");
}

/// The example data the app starts with, relative to [`now`].
#[fixture]
pub fn seeded() {
    fresh();
    repo::seed_sample().expect("seed");
}

/// Records every change the database reports.
pub fn record_changes() -> Rc<RefCell<Vec<Change>>> {
    let log = Rc::new(RefCell::new(Vec::new()));
    let sink = log.clone();
    db::set_change_hook(move |c| sink.borrow_mut().push(c));
    log
}
