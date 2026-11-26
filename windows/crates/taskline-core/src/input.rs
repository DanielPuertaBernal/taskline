//! Quick input: `Pay rent due:+3d priority:H project:Home`, due shortcuts and the bar text.

use chrono::{DateTime, Duration, NaiveDate, Utc};

use crate::dates::{Zone, format_date};

pub const FILLED: char = '▰';
pub const EMPTY: char = '▱';
pub const PRIORITIES: [&str; 3] = ["H", "M", "L"];

const TODAY_WORDS: [&str; 2] = ["today", "hoy"];
const TOMORROW_WORDS: [&str; 3] = ["tomorrow", "mañana", "manana"];

/// A due date or priority the user typed that cannot be understood.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InputError {
    #[error("could not understand due date '{0}'")]
    Due(String),
    #[error("priority must be H, M or L, not '{0}'")]
    Priority(String),
    #[error("description must not be empty")]
    EmptyDescription,
}

/// `+3d`, `3d`, `+2w` -> days.
fn offset_days(value: &str) -> Option<i64> {
    let rest = value.strip_prefix('+').unwrap_or(value);
    let unit = rest.chars().last()?;
    let digits = &rest[..rest.len() - unit.len_utf8()];
    if digits.is_empty() || digits.len() > 4 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let amount: i64 = digits.parse().ok()?;
    match unit {
        'd' => Some(amount),
        'w' => Some(amount * 7),
        _ => None,
    }
}

/// The local day quick input points at: today/hoy, tomorrow/mañana, +3d, +2w, YYYY-MM-DD.
pub fn parse_due_day(text: &str, now: DateTime<Utc>, zone: Zone) -> Result<NaiveDate, InputError> {
    let value = text.trim().to_lowercase();
    let today = zone.date_of(now);
    if TODAY_WORDS.contains(&value.as_str()) {
        return Ok(today);
    }
    if TOMORROW_WORDS.contains(&value.as_str()) {
        return Ok(today + Duration::days(1));
    }
    if let Some(days) = offset_days(&value) {
        return Ok(today + Duration::days(days));
    }
    NaiveDate::parse_from_str(&value, "%Y-%m-%d")
        .or_else(|_| NaiveDate::parse_from_str(&value, "%Y%m%d"))
        .map_err(|_| InputError::Due(text.to_owned()))
}

/// Quick input as a due moment at local midnight (what `task add due:...` stores).
pub fn parse_due(text: &str, now: DateTime<Utc>, zone: Zone) -> Result<DateTime<Utc>, InputError> {
    parse_due_day(text, now, zone).map(|day| zone.midnight(day))
}

/// H, M or L in any case; an empty string clears it (`None`).
pub fn parse_priority(text: &str) -> Result<Option<String>, InputError> {
    let value = text.trim().to_uppercase();
    if value.is_empty() {
        return Ok(None);
    }
    if PRIORITIES.contains(&value.as_str()) {
        Ok(Some(value))
    } else {
        Err(InputError::Priority(text.to_owned()))
    }
}

/// Attributes read from the text. `Some(None)` means "clear it" (`due:` with no value).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Attrs {
    pub project: Option<String>,
    pub due: Option<Option<String>>,
    pub priority: Option<Option<String>>,
}

/// Splits the description from `project:`/`pro:`, `due:` and `priority:`/`pri:`.
/// Every other word stays in the description.
pub fn parse_input(text: &str, now: DateTime<Utc>, zone: Zone) -> Result<(String, Attrs), InputError> {
    let mut attrs = Attrs::default();
    let mut rest: Vec<&str> = Vec::new();
    for word in text.split_whitespace() {
        let Some((key, value)) = word.split_once(':') else {
            rest.push(word);
            continue;
        };
        match key {
            "project" | "pro" if !value.is_empty() => attrs.project = Some(value.to_owned()),
            "due" => {
                attrs.due = Some(if value.is_empty() {
                    None
                } else {
                    Some(format_date(parse_due(value, now, zone)?))
                })
            }
            "priority" | "pri" => attrs.priority = Some(parse_priority(value)?),
            _ => rest.push(word),
        }
    }
    Ok((rest.join(" "), attrs))
}

/// The bar text, exactly like the Omarchy widget: `▰▰▱▱▱▱▱▱▱▱  1/4`.
pub fn progress_bar(done: usize, total: usize, segments: usize) -> String {
    let ratio = if total > 0 {
        done as f64 / total as f64
    } else {
        0.0
    };
    // JavaScript's Math.round (.5 rounds up), as in the QML widget.
    let filled = ((ratio * segments as f64 + 0.5).floor() as usize).min(segments);
    let mut bar = String::new();
    bar.extend(std::iter::repeat_n(FILLED, filled));
    bar.extend(std::iter::repeat_n(EMPTY, segments - filled));
    format!("{bar}  {done}/{total}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dates::parse_tw;

    fn now() -> DateTime<Utc> {
        parse_tw("20261124T150000Z").unwrap()
    }

    #[test]
    fn progress_bar_matches_the_widget() {
        assert_eq!(progress_bar(0, 0, 10), "▱▱▱▱▱▱▱▱▱▱  0/0");
        assert_eq!(progress_bar(1, 4, 10), "▰▰▰▱▱▱▱▱▱▱  1/4");
        assert_eq!(progress_bar(1, 2, 10), "▰▰▰▰▰▱▱▱▱▱  1/2");
        assert_eq!(progress_bar(3, 3, 10), "▰▰▰▰▰▰▰▰▰▰  3/3");
        assert_eq!(progress_bar(1, 8, 4), "▰▱▱▱  1/8");
    }

    #[test]
    fn due_quick_inputs_are_local_midnight() {
        let zone = Zone::utc();
        for (text, expected) in [
            ("today", "20261124T000000Z"),
            ("HOY", "20261124T000000Z"),
            ("mañana", "20261125T000000Z"),
            ("tomorrow", "20261125T000000Z"),
            ("+3d", "20261127T000000Z"),
            ("3d", "20261127T000000Z"),
            ("+2w", "20261208T000000Z"),
            ("2026-12-01", "20261201T000000Z"),
        ] {
            assert_eq!(
                format_date(parse_due(text, now(), zone).unwrap()),
                expected,
                "{text}"
            );
        }
        let bogota = Zone::hours(-5);
        assert_eq!(
            format_date(parse_due("today", now(), bogota).unwrap()),
            "20261124T050000Z"
        );
    }

    #[test]
    fn unknown_due_input_is_rejected() {
        for text in ["someday", "+d", "+12345d", "2026-13-01", ""] {
            assert!(parse_due(text, now(), Zone::utc()).is_err(), "{text}");
        }
        assert_eq!(
            parse_due("soon", now(), Zone::utc()).unwrap_err().to_string(),
            "could not understand due date 'soon'"
        );
    }

    #[test]
    fn input_reads_project_due_and_priority() {
        let (text, attrs) = parse_input("Pay rent due:+3d pri:h project:Home", now(), Zone::utc()).unwrap();
        assert_eq!(text, "Pay rent");
        assert_eq!(attrs.project.as_deref(), Some("Home"));
        assert_eq!(attrs.due, Some(Some("20261127T000000Z".into())));
        assert_eq!(attrs.priority, Some(Some("H".into())));
        let (text, attrs) = parse_input("Fix due: priority: http://x tag:a", now(), Zone::utc()).unwrap();
        assert_eq!(text, "Fix http://x tag:a");
        assert_eq!(attrs.due, Some(None));
        assert_eq!(attrs.priority, Some(None));
        assert!(parse_input("x priority:urgent", now(), Zone::utc()).is_err());
        assert!(parse_input("x due:someday", now(), Zone::utc()).is_err());
    }
}
