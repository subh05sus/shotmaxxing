//! Durations the way people write them.
//!
//! Following Postel's law, input is forgiving: `1:05`, `01:02:03.5`, `65s`, `1m5s`, `1.5s`, `500ms`, `2 min` and a
//! bare `90` (seconds) are all accepted. Output is always one clean format.

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TimeParseError {
    #[error("enter a duration, for example 1:05 or 65s")]
    Empty,
    #[error("'{0}' is not a duration; try 1:05, 65s or 500ms")]
    Invalid(String),
    #[error("durations can't be negative")]
    Negative,
    #[error("that duration is too long")]
    TooLong,
}

const US_PER_SEC: f64 = 1_000_000.0;
/// Upper limit: 24 hours.
const MAX_US: f64 = 24.0 * 3600.0 * US_PER_SEC;

/// Parses a duration into microseconds.
pub fn parse_duration_us(input: &str) -> Result<u64, TimeParseError> {
    // Two numbers separated only by spaces ("10 20") are ambiguous, so they are rejected rather than joined.
    let chars: Vec<char> = input.trim().chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if c.is_whitespace() && i > 0 && chars[i - 1].is_ascii_digit() {
            let next = chars[i..].iter().find(|c| !c.is_whitespace());
            if next.is_some_and(char::is_ascii_digit) {
                return Err(TimeParseError::Invalid(input.trim().to_string()));
            }
        }
    }
    let cleaned: String = input.trim().to_lowercase().chars().filter(|c| !c.is_whitespace()).collect();
    if cleaned.is_empty() {
        return Err(TimeParseError::Empty);
    }
    if cleaned.starts_with('-') {
        return Err(TimeParseError::Negative);
    }
    let seconds = if cleaned.contains(':') { parse_clock(&cleaned) } else { parse_units(&cleaned) }
        .ok_or_else(|| TimeParseError::Invalid(input.trim().to_string()))?;
    let us = seconds * US_PER_SEC;
    if !us.is_finite() {
        return Err(TimeParseError::Invalid(input.trim().to_string()));
    }
    if us > MAX_US {
        return Err(TimeParseError::TooLong);
    }
    Ok(us.round() as u64)
}

fn parse_clock(s: &str) -> Option<f64> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() > 3 || parts.iter().any(|p| p.is_empty()) {
        return None;
    }
    let (last, rest) = parts.split_last()?;
    let seconds: f64 = parse_number(last)?;
    let mut whole = Vec::with_capacity(rest.len());
    for part in rest {
        if !part.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        whole.push(part.parse::<f64>().ok()?);
    }
    // Minutes and seconds after the first field are clock fields, so they must stay below 60.
    if !rest.is_empty() && seconds >= 60.0 {
        return None;
    }
    if whole.len() == 2 && whole[1] >= 60.0 {
        return None;
    }
    Some(match whole.as_slice() {
        [] => seconds,
        [m] => m * 60.0 + seconds,
        [h, m] => h * 3600.0 + m * 60.0 + seconds,
        _ => return None,
    })
}

fn parse_units(s: &str) -> Option<f64> {
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut total = 0.0;
    let mut seen_any = false;
    while i < bytes.len() {
        let start = i;
        while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
            i += 1;
        }
        if start == i {
            return None;
        }
        let value = parse_number(&s[start..i])?;
        let unit_start = i;
        while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
            i += 1;
        }
        let multiplier = match &s[unit_start..i] {
            "" if i == bytes.len() && !seen_any => 1.0,
            "ms" | "msec" | "millis" => 0.001,
            "s" | "sec" | "secs" | "second" | "seconds" => 1.0,
            "m" | "min" | "mins" | "minute" | "minutes" => 60.0,
            "h" | "hr" | "hrs" | "hour" | "hours" => 3600.0,
            _ => return None,
        };
        total += value * multiplier;
        seen_any = true;
    }
    seen_any.then_some(total)
}

fn parse_number(s: &str) -> Option<f64> {
    if s.is_empty() || s.matches('.').count() > 1 || s == "." {
        return None;
    }
    s.parse::<f64>().ok().filter(|v| v.is_finite() && *v >= 0.0)
}

/// Formats microseconds as a timecode: `0:42`, `1:05.3` with a fraction, `1:02:03` past an hour.
pub fn format_timecode(us: u64, tenths: bool) -> String {
    let total_tenths = (us + 50_000) / 100_000;
    let (whole_secs, tenth) =
        if tenths { (total_tenths / 10, total_tenths % 10) } else { ((us + 500_000) / 1_000_000, 0) };
    let hours = whole_secs / 3600;
    let minutes = (whole_secs % 3600) / 60;
    let seconds = whole_secs % 60;
    let mut out = if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    };
    if tenths {
        out.push('.');
        out.push_str(&tenth.to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(s: &str) -> f64 {
        parse_duration_us(s).unwrap() as f64 / 1e6
    }

    #[test]
    fn accepts_many_shapes() {
        assert_eq!(secs("1:05"), 65.0);
        assert_eq!(secs("01:02:03.5"), 3723.5);
        assert_eq!(secs("65s"), 65.0);
        assert_eq!(secs("1m5s"), 65.0);
        assert_eq!(secs("1.5s"), 1.5);
        assert_eq!(secs("500ms"), 0.5);
        assert_eq!(secs("90"), 90.0);
        assert_eq!(secs(" 2 min "), 120.0);
        assert_eq!(secs("1h 2m"), 3720.0);
        assert_eq!(secs("0:00.25"), 0.25);
    }

    #[test]
    fn rejects_nonsense_politely() {
        assert_eq!(parse_duration_us(""), Err(TimeParseError::Empty));
        assert_eq!(parse_duration_us("-5s"), Err(TimeParseError::Negative));
        assert!(matches!(parse_duration_us("abc"), Err(TimeParseError::Invalid(_))));
        assert!(matches!(parse_duration_us("1:75"), Err(TimeParseError::Invalid(_))));
        assert!(matches!(parse_duration_us("5x"), Err(TimeParseError::Invalid(_))));
        assert!(matches!(parse_duration_us("1..2s"), Err(TimeParseError::Invalid(_))));
        assert!(matches!(parse_duration_us("10 20"), Err(TimeParseError::Invalid(_))));
        assert_eq!(parse_duration_us("48h"), Err(TimeParseError::TooLong));
    }

    #[test]
    fn formats_cleanly() {
        assert_eq!(format_timecode(42_000_000, false), "0:42");
        assert_eq!(format_timecode(65_300_000, true), "1:05.3");
        assert_eq!(format_timecode(3_723_000_000, false), "1:02:03");
        assert_eq!(format_timecode(59_960_000, true), "1:00.0");
    }
}
