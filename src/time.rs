//! Dates as Unix milliseconds, formatted in the browser's own locale.
//!
//! Loans are due at the end of the chosen day, the way a library date stamp works.

use js_sys::{Array, Date, Intl, Object, Reflect};
use wasm_bindgen::JsValue;

pub const HOUR: i64 = 3_600_000;
pub const DAY: i64 = 24 * HOUR;

pub fn now() -> i64 {
    Date::now() as i64
}

fn date(ms: i64) -> Date {
    Date::new(&JsValue::from_f64(ms as f64))
}

fn format(ms: i64, opts: &[(&str, &str)]) -> String {
    let o = Object::new();
    for (k, v) in opts {
        let _ = Reflect::set(&o, &JsValue::from_str(k), &JsValue::from_str(v));
    }
    // An empty locale list means "the browser's default", which never throws
    // (unlike passing navigator.language, which can be an odd tag).
    let fmt = Intl::DateTimeFormat::new(&Array::new(), &o);
    fmt.format()
        .call1(&JsValue::UNDEFINED, &date(ms))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default()
}

/// "Tue 7 Oct"
pub fn short(ms: i64) -> String {
    format(ms, &[("weekday", "short"), ("day", "numeric"), ("month", "short")])
}

/// "Tuesday 7 October 2026"
pub fn long(ms: i64) -> String {
    format(
        ms,
        &[("weekday", "long"), ("day", "numeric"), ("month", "long"), ("year", "numeric")],
    )
}

/// "7 Oct 2026, 14:05"
pub fn stamp(ms: i64) -> String {
    format(
        ms,
        &[
            ("day", "numeric"),
            ("month", "short"),
            ("year", "numeric"),
            ("hour", "2-digit"),
            ("minute", "2-digit"),
        ],
    )
}

pub fn start_of_day(ms: i64) -> i64 {
    let d = date(ms);
    d.set_hours(0);
    d.set_minutes(0);
    d.set_seconds(0);
    d.set_milliseconds(0);
    d.get_time() as i64
}

pub fn end_of_day(ms: i64) -> i64 {
    let d = date(ms);
    d.set_hours(23);
    d.set_minutes(59);
    d.set_seconds(59);
    d.set_milliseconds(0);
    d.get_time() as i64
}

/// Whole calendar days from `a` to `b` (negative if `b` is earlier).
pub fn calendar_days(a: i64, b: i64) -> i64 {
    ((start_of_day(b) - start_of_day(a)) as f64 / DAY as f64).round() as i64
}

/// Due date `days` calendar days from now, at the end of that day.
pub fn due_in_days(days: i64) -> i64 {
    end_of_day(now() + days * DAY)
}

/// `YYYY-MM-DD` for `<input type="date">`.
pub fn to_input(ms: i64) -> String {
    let d = date(ms);
    format!("{:04}-{:02}-{:02}", d.get_full_year(), d.get_month() + 1, d.get_date())
}

/// Parses `<input type="date">` into the end of that local day.
pub fn from_input(s: &str) -> Option<i64> {
    let mut parts = s.trim().split('-').map(|p| p.parse::<u32>().ok());
    let (y, m, d) = (parts.next()??, parts.next()??, parts.next()??);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let dt = Date::new_with_year_month_day_hr_min_sec(y, m as i32 - 1, d as i32, 23, 59, 59);
    let t = dt.get_time();
    (!t.is_nan()).then_some(t as i64)
}

pub fn plural(n: i64, word: &str) -> String {
    if n == 1 {
        format!("1 {word}")
    } else {
        format!("{n} {word}s")
    }
}

/// Calendar days a loan is late (0 if not yet due).
pub fn days_late(due: i64, at: i64) -> i64 {
    if at <= due {
        0
    } else {
        calendar_days(due, at).max(1)
    }
}

/// "Due today", "Due in 3 days", "2 days late"...
pub fn describe_due(due: i64, at: i64) -> String {
    if at > due {
        return format!("{} late", plural(days_late(due, at), "day"));
    }
    match calendar_days(at, due) {
        0 => "Due today".into(),
        1 => "Due tomorrow".into(),
        n => format!("Due in {n} days"),
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DueState {
    Late,
    Soon,
    Fine,
    Returned,
}

impl DueState {
    pub fn of(due: i64, returned: Option<i64>, at: i64) -> Self {
        if returned.is_some() {
            DueState::Returned
        } else if at > due {
            DueState::Late
        } else if calendar_days(at, due) <= 1 {
            DueState::Soon
        } else {
            DueState::Fine
        }
    }

    pub fn class(self) -> &'static str {
        match self {
            DueState::Late => "late",
            DueState::Soon => "soon",
            DueState::Fine => "fine",
            DueState::Returned => "returned",
        }
    }
}
