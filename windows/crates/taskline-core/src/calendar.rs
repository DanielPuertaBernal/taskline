//! The month calendar behind the date picker, and the `due:` / `priority:` words in the
//! edit text. Pure functions, so they are tested without a window.

use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};

use crate::dates::Zone;
use crate::input::{PRIORITIES, parse_due_day};

/// 42 days (six weeks), Monday first, covering `month`.
pub fn month_grid(year: i32, month: u32) -> Vec<NaiveDate> {
    let first = NaiveDate::from_ymd_opt(year, month, 1).expect("valid month");
    let start = first - Duration::days(first.weekday().num_days_from_monday() as i64);
    (0..42).map(|i| start + Duration::days(i)).collect()
}

pub fn shift_month(year: i32, month: u32, delta: i32) -> (i32, u32) {
    let index = year * 12 + (month as i32 - 1) + delta;
    (index.div_euclid(12), index.rem_euclid(12) as u32 + 1)
}

pub fn days_in_month(year: i32, month: u32) -> u32 {
    let (next_year, next_month) = shift_month(year, month, 1);
    let first_next = NaiveDate::from_ymd_opt(next_year, next_month, 1).expect("valid month");
    (first_next - Duration::days(1)).day()
}

/// The same day `delta` months away, clamped to that month's length.
pub fn move_month(day: NaiveDate, delta: i32) -> NaiveDate {
    let (year, month) = shift_month(day.year(), day.month(), delta);
    let last = days_in_month(year, month);
    NaiveDate::from_ymd_opt(year, month, day.day().min(last)).expect("clamped day")
}

/// The local day quick input points at, or `None` when it is not a date.
pub fn quick_date(text: &str, now: DateTime<Utc>, zone: Zone) -> Option<NaiveDate> {
    if text.trim().is_empty() {
        return None;
    }
    parse_due_day(text, now, zone).ok()
}

fn is_due_word(word: &str) -> bool {
    word.starts_with("due:")
}

fn is_priority_word(word: &str) -> bool {
    word.starts_with("priority:") || word.starts_with("pri:")
}

/// Writes `due:<day>` into edit text, replacing any `due:` word; `None` removes it.
pub fn set_due_token(text: &str, day: Option<NaiveDate>) -> String {
    let mut words: Vec<String> = text
        .split_whitespace()
        .filter(|w| !is_due_word(w))
        .map(str::to_owned)
        .collect();
    if let Some(day) = day {
        let at = words
            .iter()
            .position(|w| is_priority_word(w))
            .unwrap_or(words.len());
        words.insert(at, format!("due:{}", day.format("%Y-%m-%d")));
    }
    words.join(" ")
}

/// Writes `priority:<H|M|L>` into edit text, replacing `priority:`/`pri:`; `None` removes it.
pub fn set_priority_token(text: &str, priority: Option<&str>) -> String {
    let mut words: Vec<String> = text
        .split_whitespace()
        .filter(|w| !is_priority_word(w))
        .map(str::to_owned)
        .collect();
    if let Some(priority) = priority {
        words.push(format!("priority:{priority}"));
    }
    words.join(" ")
}

/// The day of the `due:` word in edit text, if the calendar can show it.
pub fn due_in_text(text: &str, now: DateTime<Utc>, zone: Zone) -> Option<NaiveDate> {
    let value = text.split_whitespace().find_map(|w| w.strip_prefix("due:"))?;
    quick_date(value, now, zone)
}

pub fn priority_in_text(text: &str) -> Option<&'static str> {
    let value = text
        .split_whitespace()
        .find_map(|w| w.strip_prefix("priority:").or_else(|| w.strip_prefix("pri:")))?
        .to_uppercase();
    PRIORITIES.into_iter().find(|p| *p == value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dates::parse_tw;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn now() -> DateTime<Utc> {
        parse_tw("20261124T150000Z").unwrap()
    }

    #[test]
    fn grid_is_six_weeks_monday_first() {
        let grid = month_grid(2026, 11);
        assert_eq!(grid.len(), 42);
        assert_eq!(grid[0], day(2026, 10, 26));
        assert_eq!(grid[6], day(2026, 11, 1));
        assert_eq!(grid[41], day(2026, 12, 6));
        assert_eq!(month_grid(2026, 6)[0], day(2026, 6, 1)); // starts on a Monday
    }

    #[test]
    fn months_shift_and_clamp() {
        assert_eq!(shift_month(2026, 12, 1), (2027, 1));
        assert_eq!(shift_month(2026, 1, -1), (2025, 12));
        assert_eq!(shift_month(2026, 11, -23), (2024, 12));
        assert_eq!(move_month(day(2026, 1, 31), 1), day(2026, 2, 28));
        assert_eq!(move_month(day(2028, 3, 31), -1), day(2028, 2, 29));
        assert_eq!(days_in_month(2026, 2), 28);
    }

    #[test]
    fn quick_dates() {
        let zone = Zone::utc();
        assert_eq!(quick_date("hoy", now(), zone), Some(day(2026, 11, 24)));
        assert_eq!(quick_date("+1w", now(), zone), Some(day(2026, 12, 1)));
        assert_eq!(quick_date("2026-12-25", now(), zone), Some(day(2026, 12, 25)));
        assert_eq!(quick_date("friday", now(), zone), None);
        assert_eq!(quick_date("  ", now(), zone), None);
    }

    #[test]
    fn tokens_in_edit_text() {
        let text = "Pay rent project:Home due:2026-12-01 priority:H";
        assert_eq!(
            set_due_token(text, Some(day(2026, 12, 5))),
            "Pay rent project:Home due:2026-12-05 priority:H"
        );
        assert_eq!(set_due_token(text, None), "Pay rent project:Home priority:H");
        assert_eq!(
            set_due_token("Call mom", Some(day(2026, 12, 5))),
            "Call mom due:2026-12-05"
        );
        assert_eq!(
            set_priority_token(text, Some("L")),
            "Pay rent project:Home due:2026-12-01 priority:L"
        );
        assert_eq!(set_priority_token("x pri:m", None), "x");
        assert_eq!(due_in_text(text, now(), Zone::utc()), Some(day(2026, 12, 1)));
        assert_eq!(due_in_text("x due:someday", now(), Zone::utc()), None);
        assert_eq!(priority_in_text("x pri:m"), Some("M"));
        assert_eq!(priority_in_text("x priority:urgent"), None);
    }
}
