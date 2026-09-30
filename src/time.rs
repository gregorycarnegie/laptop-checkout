//! Dates as Unix milliseconds, with calendar maths done in pure Rust.
//!
//! Loans are due at the end of the chosen day, the way a library date stamp
//! works. Only two things come from the outside world: the current time and the
//! local timezone offset. In the browser those come from JavaScript's `Date`;
//! natively (in tests) they come from [`clock`], which tests can set.
//!
//! ```
//! use laptop_checkout::time;
//! let tuesday = time::from_input("2026-10-06").unwrap();
//! assert_eq!(time::to_input(tuesday), "2026-10-06");
//! assert_eq!(time::short(tuesday), "Tue 6 Oct");
//! ```

pub const SECOND: i64 = 1_000;
pub const MINUTE: i64 = 60 * SECOND;
pub const HOUR: i64 = 60 * MINUTE;
pub const DAY: i64 = 24 * HOUR;

const WEEKDAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// The current time in Unix milliseconds.
pub fn now() -> i64 {
    platform::now()
}

/// Milliseconds to add to a UTC instant to get local wall-clock time.
fn offset(utc_ms: i64) -> i64 {
    platform::offset(utc_ms)
}

#[cfg(target_arch = "wasm32")]
mod platform {
    use wasm_bindgen::JsValue;

    #[mutants::skip] // Browser-only; covered by the end-to-end tests.
    pub fn now() -> i64 {
        js_sys::Date::now() as i64
    }

    #[mutants::skip] // Browser-only; covered by the end-to-end tests.
    pub fn offset(utc_ms: i64) -> i64 {
        let d = js_sys::Date::new(&JsValue::from_f64(utc_ms as f64));
        -(d.get_timezone_offset() as i64) * super::MINUTE
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod platform {
    pub fn now() -> i64 {
        super::clock::now()
    }

    pub fn offset(utc_ms: i64) -> i64 {
        super::clock::offset(utc_ms)
    }
}

/// A controllable clock for native builds (tests). Each test thread has its own.
#[cfg(not(target_arch = "wasm32"))]
pub mod clock {
    use std::cell::Cell;

    thread_local! {
        static NOW: Cell<Option<i64>> = const { Cell::new(None) };
        /// (switch instant, offset before, offset after), to model daylight saving.
        static ZONE: Cell<(i64, i64, i64)> = const { Cell::new((0, 0, 0)) };
    }

    /// Freezes "now" at `ms`.
    pub fn set_now(ms: i64) {
        NOW.with(|n| n.set(Some(ms)));
    }

    /// Moves the frozen clock forward (or back) by `ms`.
    pub fn advance(ms: i64) {
        set_now(now() + ms);
    }

    /// A fixed local offset from UTC, e.g. `set_offset(time::HOUR)` for UTC+1.
    pub fn set_offset(ms: i64) {
        ZONE.with(|z| z.set((0, ms, ms)));
    }

    /// Offset `before` until the UTC instant `at`, then `after` (a DST change).
    pub fn set_zone_change(at: i64, before: i64, after: i64) {
        ZONE.with(|z| z.set((at, before, after)));
    }

    pub fn now() -> i64 {
        NOW.with(Cell::get).unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0)
        })
    }

    pub fn offset(utc_ms: i64) -> i64 {
        let (at, before, after) = ZONE.with(Cell::get);
        if utc_ms < at {
            before
        } else {
            after
        }
    }
}

// ---------------------------------------------------------------- civil calendar

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's algorithm).
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let m = m as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The date for a count of days since 1970-01-01.
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

pub fn is_leap_year(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

pub fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        2 if is_leap_year(y) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

/// Local calendar fields for a UTC instant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Local {
    pub year: i64,
    pub month: u32,
    pub day: u32,
    /// 0 = Sunday.
    pub weekday: u32,
    pub hour: u32,
    pub minute: u32,
}

/// Days since the epoch of the local calendar day containing `ms`.
fn local_day(ms: i64) -> i64 {
    (ms + offset(ms)).div_euclid(DAY)
}

pub fn local(ms: i64) -> Local {
    let wall = ms + offset(ms);
    let day = wall.div_euclid(DAY);
    let in_day = wall.rem_euclid(DAY);
    let (year, month, d) = civil_from_days(day);
    Local {
        year,
        month,
        day: d,
        weekday: (day + 4).rem_euclid(7) as u32,
        hour: (in_day / HOUR) as u32,
        minute: (in_day % HOUR / MINUTE) as u32,
    }
}

/// Converts local wall-clock milliseconds to a UTC instant.
///
/// Around a clock change a wall time can happen twice (clocks go back: the
/// first one is used) or not at all (clocks go forward: the moment the clocks
/// jump is used), matching JavaScript's `Date`.
fn from_wall(wall: i64) -> i64 {
    // The offsets in force a day either side cover any change that day.
    let early = offset(wall - DAY);
    let late = offset(wall + DAY);
    for o in [early, late] {
        let t = wall - o;
        if t + offset(t) == wall {
            return t;
        }
    }
    wall - early.min(late)
}

pub fn start_of_day(ms: i64) -> i64 {
    from_wall(local_day(ms) * DAY)
}

/// 23:59:59 local time on the day containing `ms`.
pub fn end_of_day(ms: i64) -> i64 {
    from_wall(local_day(ms) * DAY + DAY - SECOND)
}

/// Whole calendar days from `a` to `b` (negative if `b` is earlier).
pub fn calendar_days(a: i64, b: i64) -> i64 {
    local_day(b) - local_day(a)
}

/// The end of the local day `days` calendar days after `from`.
pub fn end_of_day_after(from: i64, days: i64) -> i64 {
    from_wall((local_day(from) + days) * DAY + DAY - SECOND)
}

/// Due date `days` calendar days from now, at the end of that day.
pub fn due_in_days(days: i64) -> i64 {
    end_of_day_after(now(), days)
}

/// `YYYY-MM-DD` for `<input type="date">`.
pub fn to_input(ms: i64) -> String {
    let l = local(ms);
    format!("{:04}-{:02}-{:02}", l.year, l.month, l.day)
}

/// Years a date box accepts. Wider years would overflow the day arithmetic.
pub const YEARS: std::ops::RangeInclusive<i64> = 1..=9999;

/// Parses `<input type="date">` into the end of that local day.
pub fn from_input(s: &str) -> Option<i64> {
    let mut parts = s.trim().splitn(3, '-');
    let y: i64 = parts.next()?.parse().ok()?;
    let m: u32 = parts.next()?.parse().ok()?;
    let d: u32 = parts.next()?.parse().ok()?;
    if !YEARS.contains(&y) || !(1..=12).contains(&m) || d == 0 || d > days_in_month(y, m) {
        return None;
    }
    Some(from_wall(days_from_civil(y, m, d) * DAY + DAY - SECOND))
}

fn weekday_short(l: &Local) -> &'static str {
    &WEEKDAYS[l.weekday as usize][..3]
}

fn month_short(l: &Local) -> &'static str {
    &MONTHS[l.month as usize - 1][..3]
}

/// "Tue 6 Oct"
pub fn short(ms: i64) -> String {
    let l = local(ms);
    format!("{} {} {}", weekday_short(&l), l.day, month_short(&l))
}

/// "Tuesday 6 October 2026"
pub fn long(ms: i64) -> String {
    let l = local(ms);
    format!("{} {} {} {}", WEEKDAYS[l.weekday as usize], l.day, MONTHS[l.month as usize - 1], l.year)
}

/// "6 Oct 2026, 14:05"
pub fn stamp(ms: i64) -> String {
    let l = local(ms);
    format!("{} {} {}, {:02}:{:02}", l.day, month_short(&l), l.year, l.hour, l.minute)
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

// Native only: these use test crates that don't build for WebAssembly.
#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use proptest::prelude::*;
    use rstest::rstest;

    /// A UTC instant. Tests run with a zero offset unless they set one.
    fn at(y: i64, m: u32, d: u32, h: i64, min: i64) -> i64 {
        days_from_civil(y, m, d) * DAY + h * HOUR + min * MINUTE
    }

    // ------------------------------------------------------------ civil calendar

    #[rstest]
    #[case((1970, 1, 1), 0)]
    #[case((1969, 12, 31), -1)]
    #[case((2000, 2, 29), 11_016)]
    #[case((2000, 3, 1), 11_017)]
    #[case((2026, 9, 30), 20_726)]
    #[case((1900, 3, 1), -25_508)]
    #[case((2400, 12, 31), 157_419)]
    #[case((1600, 1, 1), -135_140)]
    fn days_from_civil_matches_the_gregorian_calendar(#[case] date: (i64, u32, u32), #[case] days: i64) {
        assert_eq!(days_from_civil(date.0, date.1, date.2), days);
        assert_eq!(civil_from_days(days), date);
    }

    #[rstest]
    #[case(2024, true)]
    #[case(2023, false)]
    #[case(1900, false)]
    #[case(2000, true)]
    fn leap_years_follow_the_400_year_rule(#[case] year: i64, #[case] leap: bool) {
        assert_eq!(is_leap_year(year), leap);
    }

    #[rstest]
    #[case(2024, 2, 29)]
    #[case(2023, 2, 28)]
    #[case(2026, 4, 30)]
    #[case(2026, 6, 30)]
    #[case(2026, 9, 30)]
    #[case(2026, 11, 30)]
    #[case(2026, 1, 31)]
    #[case(2026, 12, 31)]
    fn days_in_month_knows_short_months(#[case] y: i64, #[case] m: u32, #[case] days: u32) {
        assert_eq!(days_in_month(y, m), days);
    }

    proptest! {
        #[test]
        fn civil_dates_round_trip(y in -2000i64..4000, m in 1u32..=12, d in 1u32..=28) {
            prop_assert_eq!(civil_from_days(days_from_civil(y, m, d)), (y, m, d));
        }

        #[test]
        fn day_numbers_round_trip(z in -1_000_000i64..1_000_000) {
            let (y, m, d) = civil_from_days(z);
            prop_assert_eq!(days_from_civil(y, m, d), z);
        }
    }

    // ------------------------------------------------------------ local time

    #[test]
    fn local_splits_an_instant_into_calendar_fields() {
        assert_eq!(
            local(at(2026, 10, 6, 14, 5)),
            Local { year: 2026, month: 10, day: 6, weekday: 2, hour: 14, minute: 5 }
        );
    }

    #[test]
    fn local_applies_the_timezone_offset() {
        clock::set_offset(-5 * HOUR);
        let l = local(at(2026, 10, 6, 2, 30));
        assert_eq!((l.day, l.hour, l.minute), (5, 21, 30));
    }

    #[rstest]
    #[case::utc(0)]
    #[case::ahead(9 * HOUR)]
    #[case::behind(-7 * HOUR)]
    #[case::half_hour(5 * HOUR + 30 * MINUTE)]
    fn end_of_day_is_one_second_before_local_midnight(#[case] offset: i64) {
        clock::set_offset(offset);
        let end = end_of_day(at(2026, 10, 6, 12, 0));
        let l = local(end);
        assert_eq!((l.day, l.hour, l.minute), (6, 23, 59));
        assert_eq!(local(end + SECOND).day, 7);
    }

    #[rstest]
    #[case::utc(0)]
    #[case::ahead(9 * HOUR)]
    #[case::behind(-7 * HOUR)]
    fn start_of_day_is_local_midnight(#[case] offset: i64) {
        clock::set_offset(offset);
        let start = start_of_day(at(2026, 10, 6, 12, 0));
        let l = local(start);
        assert_eq!((l.day, l.hour, l.minute), (6, 0, 0));
        assert_eq!(local(start - 1).day, 5);
    }

    #[test]
    fn end_of_day_follows_a_clock_change_that_day() {
        // UK clocks go forward at 01:00 UTC on 29 March 2026.
        clock::set_zone_change(at(2026, 3, 29, 1, 0), 0, HOUR);
        let end = end_of_day(at(2026, 3, 29, 12, 0));
        assert_eq!(end, at(2026, 3, 29, 22, 59) + 59 * SECOND);
        assert_eq!(start_of_day(at(2026, 3, 29, 12, 0)), at(2026, 3, 29, 0, 0));
    }

    #[test]
    fn a_day_that_starts_with_the_clocks_going_forward_starts_when_they_jump() {
        // Brazil once put clocks forward at midnight: 00:00 BRT (UTC-3) became 01:00 (UTC-2).
        let jump = at(2018, 11, 4, 3, 0);
        clock::set_zone_change(jump, -3 * HOUR, -2 * HOUR);
        let start = start_of_day(at(2018, 11, 4, 15, 0));
        assert_eq!(start, jump);
        assert_eq!(local(start).day, 4);
        assert_eq!(local(start - 1).day, 3);
    }

    #[test]
    fn a_time_skipped_by_the_clocks_moves_forward() {
        // UK: 01:00 local jumps to 02:00, so 01:30 doesn't exist and reads as 02:30.
        clock::set_zone_change(at(2026, 3, 29, 1, 0), 0, HOUR);
        let t = from_wall(at(2026, 3, 29, 1, 30));
        assert_eq!((local(t).hour, local(t).minute), (2, 30));
    }

    #[test]
    fn a_time_that_happens_twice_uses_the_first() {
        // UK: 02:00 BST goes back to 01:00 GMT, so 01:30 happens twice.
        clock::set_zone_change(at(2026, 10, 25, 1, 0), HOUR, 0);
        assert_eq!(from_wall(at(2026, 10, 25, 1, 30)), at(2026, 10, 25, 0, 30));
    }

    #[test]
    fn end_of_day_is_right_after_the_clocks_go_back() {
        clock::set_zone_change(at(2026, 10, 25, 1, 0), HOUR, 0);
        assert_eq!(end_of_day(at(2026, 10, 25, 12, 0)), at(2026, 10, 25, 23, 59) + 59 * SECOND);
    }

    #[test]
    fn calendar_days_counts_midnights_not_hours() {
        assert_eq!(calendar_days(at(2026, 10, 6, 23, 0), at(2026, 10, 7, 1, 0)), 1);
        assert_eq!(calendar_days(at(2026, 10, 6, 1, 0), at(2026, 10, 6, 23, 0)), 0);
        assert_eq!(calendar_days(at(2026, 10, 7, 1, 0), at(2026, 10, 6, 23, 0)), -1);
    }

    #[test]
    fn end_of_day_after_skips_whole_days() {
        assert_eq!(end_of_day_after(at(2026, 10, 6, 9, 0), 7), end_of_day(at(2026, 10, 13, 9, 0)));
    }

    #[test]
    fn due_in_days_counts_from_the_clock() {
        clock::set_now(at(2026, 10, 6, 9, 0));
        assert_eq!(due_in_days(1), end_of_day(at(2026, 10, 7, 0, 0)));
    }

    proptest! {
        #[test]
        fn a_day_runs_from_start_to_end(ms in 0i64..4_000_000_000_000, quarter_hours in -48i64..56) {
            clock::set_offset(quarter_hours * 15 * MINUTE);
            let (start, end) = (start_of_day(ms), end_of_day(ms));
            prop_assert!(start <= ms && ms < start + DAY);
            prop_assert_eq!(end - start, DAY - SECOND);
        }

        #[test]
        fn date_inputs_round_trip_to_the_end_of_the_same_day(ms in 0i64..4_000_000_000_000, quarter_hours in -48i64..56) {
            clock::set_offset(quarter_hours * 15 * MINUTE);
            prop_assert_eq!(from_input(&to_input(ms)), Some(end_of_day(ms)));
        }

        #[test]
        fn calendar_days_is_antisymmetric(a in 0i64..4_000_000_000_000, b in 0i64..4_000_000_000_000) {
            prop_assert_eq!(calendar_days(a, b), -calendar_days(b, a));
        }

        #[test]
        fn a_returned_late_loan_is_at_least_a_day_late(due in 0i64..4_000_000_000_000, extra in 1i64..100 * DAY) {
            prop_assert!(days_late(due, due + extra) >= 1);
        }
    }

    // ------------------------------------------------------------ date inputs

    #[test]
    fn to_input_pads_months_and_days() {
        assert_eq!(to_input(at(2026, 3, 7, 12, 0)), "2026-03-07");
    }

    #[test]
    fn from_input_gives_the_end_of_that_day() {
        assert_eq!(from_input(" 2026-10-06 "), Some(end_of_day(at(2026, 10, 6, 0, 0))));
    }

    #[rstest]
    #[case::empty("")]
    #[case::words("next tuesday")]
    #[case::missing_day("2026-10")]
    #[case::month_zero("2026-00-10")]
    #[case::month_thirteen("2026-13-10")]
    #[case::day_zero("2026-10-00")]
    #[case::thirty_first_of_april("2026-04-31")]
    #[case::not_a_leap_year("2026-02-29")]
    #[case::trailing_text("2026-10-06x")]
    #[case::year_zero("0-01-01")]
    #[case::five_digit_year("10000-01-01")]
    #[case::huge_year_found_by_fuzzing("5888888888888888888-05-08")]
    #[case::negative_year("-5-01-01")]
    fn from_input_rejects_invalid_dates(#[case] input: &str) {
        assert_eq!(from_input(input), None);
    }

    #[rstest]
    #[case("0001-01-01")]
    #[case("9999-12-31")]
    fn from_input_accepts_the_first_and_last_supported_days(#[case] input: &str) {
        assert_eq!(from_input(input).map(to_input).as_deref(), Some(input));
    }

    proptest! {
        #[test]
        fn from_input_never_panics(y in any::<i64>(), m in any::<u32>(), d in any::<u32>()) {
            let _ = from_input(&format!("{y}-{m}-{d}"));
        }
    }

    #[test]
    fn from_input_accepts_leap_days() {
        assert!(from_input("2024-02-29").is_some());
    }

    // ------------------------------------------------------------ formatting

    macro_rules! formats {
        ($($name:ident: $f:ident($y:literal, $m:literal, $d:literal, $h:literal, $min:literal) => $expected:literal)*) => {
            $(
                #[test]
                fn $name() {
                    assert_eq!($f(at($y, $m, $d, $h, $min)), $expected);
                }
            )*
        };
    }

    formats! {
        short_shows_weekday_day_and_month: short(2026, 10, 6, 9, 0) => "Tue 6 Oct"
        short_on_a_sunday: short(2026, 10, 4, 9, 0) => "Sun 4 Oct"
        short_on_a_saturday: short(2026, 10, 3, 9, 0) => "Sat 3 Oct"
        long_spells_everything_out: long(2026, 10, 6, 9, 0) => "Tuesday 6 October 2026"
        long_in_january: long(2027, 1, 1, 9, 0) => "Friday 1 January 2027"
        long_in_december: long(2026, 12, 31, 9, 0) => "Thursday 31 December 2026"
        stamp_uses_a_24_hour_clock: stamp(2026, 10, 6, 14, 5) => "6 Oct 2026, 14:05"
        stamp_pads_the_hour: stamp(2026, 10, 6, 7, 30) => "6 Oct 2026, 07:30"
    }

    #[rstest]
    #[case(0, "0 days")]
    #[case(1, "1 day")]
    #[case(2, "2 days")]
    fn plural_adds_an_s_except_for_one(#[case] n: i64, #[case] text: &str) {
        assert_eq!(plural(n, "day"), text);
    }

    // ------------------------------------------------------------ due dates

    #[test]
    fn a_loan_returned_on_the_due_instant_is_not_late() {
        let due = end_of_day(at(2026, 10, 6, 0, 0));
        assert_eq!(days_late(due, due), 0);
    }

    #[test]
    fn a_loan_returned_minutes_after_midnight_is_one_day_late() {
        let due = end_of_day(at(2026, 10, 6, 0, 0));
        assert_eq!(days_late(due, due + 2 * MINUTE), 1);
    }

    #[test]
    fn days_late_counts_calendar_days() {
        let due = end_of_day(at(2026, 10, 6, 0, 0));
        assert_eq!(days_late(due, at(2026, 10, 9, 10, 0)), 3);
    }

    #[rstest]
    #[case::today(at(2026, 10, 6, 9, 0), "Due today")]
    #[case::tomorrow(at(2026, 10, 5, 9, 0), "Due tomorrow")]
    #[case::in_four_days(at(2026, 10, 2, 9, 0), "Due in 4 days")]
    #[case::one_day_late(at(2026, 10, 7, 9, 0), "1 day late")]
    #[case::three_days_late(at(2026, 10, 9, 9, 0), "3 days late")]
    fn describe_due_reads_like_a_person_would_say_it(#[case] now: i64, #[case] text: &str) {
        let due = end_of_day(at(2026, 10, 6, 0, 0));
        assert_eq!(describe_due(due, now), text);
    }

    #[rstest]
    #[case::returned(Some(1), at(2026, 10, 9, 9, 0), DueState::Returned, "returned")]
    #[case::late(None, at(2026, 10, 7, 9, 0), DueState::Late, "late")]
    #[case::due_today(None, at(2026, 10, 6, 9, 0), DueState::Soon, "soon")]
    #[case::due_tomorrow(None, at(2026, 10, 5, 9, 0), DueState::Soon, "soon")]
    #[case::due_later(None, at(2026, 10, 4, 9, 0), DueState::Fine, "fine")]
    fn due_state_classifies_each_loan(
        #[case] returned: Option<i64>,
        #[case] now: i64,
        #[case] state: DueState,
        #[case] class: &str,
    ) {
        let due = end_of_day(at(2026, 10, 6, 0, 0));
        assert_eq!(DueState::of(due, returned, now), state);
        assert_eq!(state.class(), class);
    }

    #[test]
    fn a_loan_is_late_one_millisecond_after_it_is_due() {
        let due = end_of_day(at(2026, 10, 6, 0, 0));
        assert_eq!(DueState::of(due, None, due), DueState::Soon);
        assert_eq!(DueState::of(due, None, due + 1), DueState::Late);
    }

    // ------------------------------------------------------------ clock

    #[test]
    fn the_test_clock_can_be_frozen_and_advanced() {
        clock::set_now(1_000);
        clock::advance(500);
        assert_eq!(now(), 1_500);
    }

    #[test]
    fn a_zone_change_takes_effect_at_the_given_instant() {
        clock::set_zone_change(1_000, 0, HOUR);
        assert_eq!((clock::offset(999), clock::offset(1_000)), (0, HOUR));
    }

    #[test]
    fn the_clock_defaults_to_the_system_time() {
        let jan_2020 = at(2020, 1, 1, 0, 0);
        assert!(now() > jan_2020);
    }
}
