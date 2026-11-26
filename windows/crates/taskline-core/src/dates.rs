//! Taskwarrior dates and the local time zone.
//!
//! Taskwarrior stores every date as UTC in the compact form `20261124T150000Z`. Due dates
//! picked as a day (`today`, `2026-12-01`) mean local midnight, like `task add due:...`.

use chrono::{DateTime, Datelike, FixedOffset, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};

/// The compact UTC form Taskwarrior writes.
pub const TW_FORMAT: &str = "%Y%m%dT%H%M%SZ";

/// Formats a moment the way `task export` does.
pub fn format_date(moment: DateTime<Utc>) -> String {
    moment.format(TW_FORMAT).to_string()
}

/// Reads only the Taskwarrior form (what the app itself writes).
pub fn parse_tw(value: &str) -> Option<DateTime<Utc>> {
    NaiveDateTime::parse_from_str(value, TW_FORMAT)
        .ok()
        .map(|n| n.and_utc())
}

/// Reads the Taskwarrior form, or an ISO 8601 date/time written by older builds or by
/// hand. A value without an offset is taken as UTC (like Python's `fromisoformat` path in
/// v1.x); anything else is `None`.
pub fn parse_date(value: &str) -> Option<DateTime<Utc>> {
    if value.is_empty() {
        return None;
    }
    if let Some(moment) = parse_tw(value) {
        return Some(moment);
    }
    if let Ok(moment) = DateTime::parse_from_rfc3339(value) {
        return Some(moment.with_timezone(&Utc));
    }
    for format in [
        "%Y-%m-%dT%H:%M:%S%:z",
        "%Y-%m-%dT%H:%M:%S%.f%:z",
        "%Y-%m-%d %H:%M:%S%:z",
    ] {
        if let Ok(moment) = DateTime::parse_from_str(value, format) {
            return Some(moment.with_timezone(&Utc));
        }
    }
    for format in [
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M",
    ] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(value, format) {
            return Some(naive.and_utc());
        }
    }
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .ok()
        .and_then(|day| day.and_hms_opt(0, 0, 0))
        .map(|naive| naive.and_utc())
}

/// The time zone days are counted in: the system's, or a fixed offset (tests).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    Local,
    Fixed(FixedOffset),
}

impl Zone {
    pub fn utc() -> Self {
        Zone::Fixed(FixedOffset::east_opt(0).expect("zero offset"))
    }

    /// `hours` east of UTC, e.g. -5 for Bogotá.
    pub fn hours(hours: i32) -> Self {
        Zone::Fixed(FixedOffset::east_opt(hours * 3600).expect("valid offset"))
    }

    pub fn naive(self, moment: DateTime<Utc>) -> NaiveDateTime {
        match self {
            Zone::Local => moment.with_timezone(&Local).naive_local(),
            Zone::Fixed(offset) => moment.with_timezone(&offset).naive_local(),
        }
    }

    /// The local calendar day of `moment`.
    pub fn date_of(self, moment: DateTime<Utc>) -> NaiveDate {
        self.naive(moment).date()
    }

    /// Local midnight at the start of `day`, in UTC.
    pub fn midnight(self, day: NaiveDate) -> DateTime<Utc> {
        let naive = day.and_hms_opt(0, 0, 0).expect("midnight exists");
        match self {
            Zone::Local => Local
                .from_local_datetime(&naive)
                .earliest()
                // A DST gap at midnight: the first valid moment of that day.
                .or_else(|| {
                    Local
                        .from_local_datetime(&(naive + chrono::Duration::hours(1)))
                        .earliest()
                })
                .map(|moment| moment.with_timezone(&Utc))
                .unwrap_or_else(|| naive.and_utc()),
            Zone::Fixed(offset) => offset
                .from_local_datetime(&naive)
                .single()
                .expect("fixed offsets have no gaps")
                .with_timezone(&Utc),
        }
    }
}

/// Days from `today` to `day` (negative in the past).
pub fn day_diff(day: NaiveDate, today: NaiveDate) -> i64 {
    (day - today).num_days()
}

/// Monday = 0 ... Sunday = 6.
pub fn weekday_index(day: NaiveDate) -> usize {
    day.weekday().num_days_from_monday() as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taskwarrior_round_trip() {
        let moment = parse_tw("20261124T150000Z").unwrap();
        assert_eq!(format_date(moment), "20261124T150000Z");
        assert!(parse_tw("2026-11-24").is_none());
    }

    #[test]
    fn iso_values_are_read_as_utc_without_offset() {
        assert_eq!(format_date(parse_date("2026-12-01").unwrap()), "20261201T000000Z");
        assert_eq!(
            format_date(parse_date("2026-12-02T08:30:00+00:00").unwrap()),
            "20261202T083000Z"
        );
        assert_eq!(
            format_date(parse_date("2026-12-02T08:30:00-05:00").unwrap()),
            "20261202T133000Z"
        );
        assert_eq!(
            format_date(parse_date("2026-12-02T08:30:00").unwrap()),
            "20261202T083000Z"
        );
        assert!(parse_date("whenever").is_none());
        assert!(parse_date("").is_none());
    }

    #[test]
    fn midnight_in_a_fixed_zone() {
        let bogota = Zone::hours(-5);
        let day = NaiveDate::from_ymd_opt(2026, 11, 26).unwrap();
        assert_eq!(format_date(bogota.midnight(day)), "20261126T050000Z");
        assert_eq!(bogota.date_of(parse_tw("20261127T040000Z").unwrap()), day);
    }
}
