//! Date formatting with Java pattern letters, so maker and sender definitions
//! written for the Java edition keep their meaning:
//!
//! * [`JavaDateFormat::parse`] follows `java.text.SimpleDateFormat` (the Date
//!   maker), e.g. `yyyy-MM-dd HH:mm:ss.SSS`, `dd/MMM/yyyy:HH:mm:ss Z`.
//! * [`JavaDateFormat::parse_java_time`] follows `java.time.format.DateTimeFormatter`
//!   (the Kafka `indexPattern`), where e.g. `u` is the year and `S` a fraction.
//!
//! Week fields follow `Locale.ENGLISH` rules (weeks start on Sunday; the week
//! containing January 1st is week 1).

use std::fmt::Write as _;
use std::sync::OnceLock;

use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate, Offset, Timelike, Utc};
use chrono_tz::{OffsetName, Tz};

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
const DAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];
const SIMPLE_DATE_FORMAT_LETTERS: &str = "GyYMLwWDdFEuaHkKhmsSzZX";
const JAVA_TIME_LETTERS: &str = "GuyDMLdQqYwWEecFahKkHmsSAnNVvzOXxZ";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Flavor {
    SimpleDateFormat,
    JavaTime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Item {
    Literal(String),
    Field { letter: char, count: usize },
}

/// A compiled Java date pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaDateFormat {
    items: Vec<Item>,
    flavor: Flavor,
}

/// Point in time with the zone information needed for formatting.
pub struct ZonedTime {
    time: DateTime<FixedOffset>,
    zone_abbreviation: String,
    /// IANA zone id, or the offset (`+09:00`, `Z`) when unknown.
    zone_id: String,
}

impl ZonedTime {
    /// Current time in the process time zone (`TZ`, else the system zone).
    pub fn now() -> Self {
        let now = Utc::now();
        match local_zone() {
            Some(tz) => Self::from_zone(&now.with_timezone(tz)),
            None => Self::from_fixed(now.with_timezone(&chrono::Local).fixed_offset()),
        }
    }

    pub fn from_zone(time: &DateTime<Tz>) -> Self {
        let offset = time.offset();
        let abbreviation = offset.abbreviation().map(str::to_owned);
        let fixed = time.fixed_offset();
        Self {
            zone_abbreviation: abbreviation.unwrap_or_else(|| gmt_name(fixed.offset().local_minus_utc())),
            zone_id: time.timezone().name().to_owned(),
            time: fixed,
        }
    }

    pub fn from_fixed(time: DateTime<FixedOffset>) -> Self {
        let offset = time.offset().local_minus_utc();
        let mut zone_id = String::new();
        if offset == 0 {
            zone_id.push('Z');
        } else {
            write_offset(&mut zone_id, offset, true, true);
        }
        Self {
            zone_abbreviation: gmt_name(offset),
            zone_id,
            time,
        }
    }
}

fn local_zone() -> Option<&'static Tz> {
    static ZONE: OnceLock<Option<Tz>> = OnceLock::new();
    ZONE.get_or_init(|| {
        std::env::var("TZ")
            .ok()
            .map(|tz| tz.trim_start_matches(':').to_owned())
            .or_else(|| iana_time_zone::get_timezone().ok())
            .and_then(|name| name.parse().ok())
    })
    .as_ref()
}

fn gmt_name(offset_secs: i32) -> String {
    if offset_secs == 0 {
        return "GMT".into();
    }
    let sign = if offset_secs < 0 { '-' } else { '+' };
    let minutes = offset_secs.unsigned_abs() / 60;
    format!("GMT{sign}{:02}:{:02}", minutes / 60, minutes % 60)
}

impl JavaDateFormat {
    /// Compiles a `SimpleDateFormat` pattern, rejecting unknown letters and
    /// unterminated quotes like `SimpleDateFormat` does.
    pub fn parse(pattern: &str) -> Result<Self, String> {
        Self::compile(pattern, Flavor::SimpleDateFormat)
    }

    /// Compiles a `DateTimeFormatter.ofPattern` pattern. Optional sections
    /// (`[...]`) are always printed.
    pub fn parse_java_time(pattern: &str) -> Result<Self, String> {
        Self::compile(pattern, Flavor::JavaTime)
    }

    fn compile(pattern: &str, flavor: Flavor) -> Result<Self, String> {
        let letters = match flavor {
            Flavor::SimpleDateFormat => SIMPLE_DATE_FORMAT_LETTERS,
            Flavor::JavaTime => JAVA_TIME_LETTERS,
        };
        let chars: Vec<char> = pattern.chars().collect();
        let mut items = Vec::new();
        let mut literal = String::new();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c == '\'' {
                if chars.get(i + 1) == Some(&'\'') {
                    literal.push('\'');
                    i += 2;
                    continue;
                }
                i += 1;
                loop {
                    match chars.get(i) {
                        None => return Err("Unterminated quote".into()),
                        Some('\'') if chars.get(i + 1) == Some(&'\'') => {
                            literal.push('\'');
                            i += 2;
                        }
                        Some('\'') => {
                            i += 1;
                            break;
                        }
                        Some(&q) => {
                            literal.push(q);
                            i += 1;
                        }
                    }
                }
            } else if flavor == Flavor::JavaTime && matches!(c, '[' | ']') {
                i += 1;
            } else if flavor == Flavor::JavaTime && matches!(c, '#' | '{' | '}') {
                return Err(format!("Pattern includes reserved character: '{c}'"));
            } else if c.is_ascii_alphabetic() {
                if !letters.contains(c) {
                    return Err(format!("Illegal pattern character '{c}'"));
                }
                let start = i;
                while chars.get(i) == Some(&c) {
                    i += 1;
                }
                let count = i - start;
                let too_long = match (flavor, c) {
                    (Flavor::SimpleDateFormat, 'X') => count > 3,
                    (Flavor::JavaTime, 'X' | 'x' | 'Z') => count > 5,
                    (Flavor::JavaTime, 'O') => count != 1 && count != 4,
                    (Flavor::JavaTime, 'V') => count != 2,
                    (Flavor::JavaTime, 'S' | 'n') => count > 9,
                    _ => false,
                };
                if too_long {
                    return Err(format!("Invalid pattern: {} letters '{c}'", count));
                }
                if !literal.is_empty() {
                    items.push(Item::Literal(std::mem::take(&mut literal)));
                }
                items.push(Item::Field { letter: c, count });
            } else {
                literal.push(c);
                i += 1;
            }
        }
        if !literal.is_empty() {
            items.push(Item::Literal(literal));
        }
        Ok(Self { items, flavor })
    }

    pub fn format_now(&self) -> String {
        self.format(&ZonedTime::now())
    }

    pub fn format(&self, zoned: &ZonedTime) -> String {
        let t = &zoned.time;
        let mut out = String::with_capacity(32);
        for item in &self.items {
            match *item {
                Item::Literal(ref s) => out.push_str(s),
                Item::Field { letter, count } => match self.flavor {
                    Flavor::JavaTime => write_java_time_field(&mut out, zoned, letter, count),
                    Flavor::SimpleDateFormat => write_field(&mut out, t, &zoned.zone_abbreviation, letter, count),
                },
            }
        }
        out
    }
}

fn pad(out: &mut String, value: i64, width: usize) {
    if value < 0 {
        out.push('-');
    }
    let _ = write!(out, "{:0width$}", value.unsigned_abs());
}

fn text(out: &mut String, full: &str, count: usize) {
    if count >= 4 {
        out.push_str(full);
    } else {
        out.push_str(&full[..3]);
    }
}

fn write_offset(out: &mut String, offset_secs: i32, colon: bool, with_minutes: bool) {
    let sign = if offset_secs < 0 { '-' } else { '+' };
    let minutes = offset_secs.unsigned_abs() / 60;
    let _ = write!(out, "{sign}{:02}", minutes / 60);
    if with_minutes {
        if colon {
            out.push(':');
        }
        let _ = write!(out, "{:02}", minutes % 60);
    }
}

/// Sunday-based day index (Sunday = 0).
fn days_from_sunday(date: NaiveDate) -> i64 {
    i64::from(date.weekday().num_days_from_sunday())
}

/// (week-based year, week of year) with Sunday start and minimal days = 1.
fn week_of_year(date: NaiveDate) -> (i32, i64) {
    let saturday = date + Duration::days(6 - days_from_sunday(date));
    (saturday.year(), i64::from(saturday.ordinal0()) / 7 + 1)
}

fn write_field(out: &mut String, t: &DateTime<FixedOffset>, zone: &str, letter: char, count: usize) {
    let date = t.date_naive();
    let offset = t.offset().fix().local_minus_utc();
    match letter {
        'G' => out.push_str(if t.year() > 0 { "AD" } else { "BC" }),
        'y' | 'Y' => {
            let year = if letter == 'y' { t.year() } else { week_of_year(date).0 };
            if count == 2 {
                pad(out, i64::from(year.rem_euclid(100)), 2);
            } else {
                pad(out, i64::from(year), count);
            }
        }
        'M' | 'L' => {
            if count >= 3 {
                text(out, MONTHS[t.month0() as usize], count);
            } else {
                pad(out, i64::from(t.month()), count);
            }
        }
        'w' => pad(out, week_of_year(date).1, count),
        'W' => {
            let first = date.with_day(1).unwrap_or(date);
            pad(out, (i64::from(t.day0()) + days_from_sunday(first)) / 7 + 1, count);
        }
        'D' => pad(out, i64::from(t.ordinal()), count),
        'd' => pad(out, i64::from(t.day()), count),
        'F' => pad(out, i64::from(t.day0()) / 7 + 1, count),
        'E' => text(out, DAYS[t.weekday().num_days_from_monday() as usize], count),
        'u' => pad(out, i64::from(t.weekday().number_from_monday()), count),
        'a' => out.push_str(if t.hour() < 12 { "AM" } else { "PM" }),
        'H' => pad(out, i64::from(t.hour()), count),
        'k' => pad(out, i64::from(if t.hour() == 0 { 24 } else { t.hour() }), count),
        'K' => pad(out, i64::from(t.hour() % 12), count),
        'h' => pad(
            out,
            i64::from(match t.hour() % 12 {
                0 => 12,
                h => h,
            }),
            count,
        ),
        'm' => pad(out, i64::from(t.minute()), count),
        's' => pad(out, i64::from(t.second()), count),
        'S' => pad(out, i64::from(t.timestamp_subsec_millis()), count),
        'z' => out.push_str(zone),
        'Z' => write_offset(out, offset, false, true),
        'X' => {
            if offset == 0 {
                out.push('Z');
            } else {
                write_offset(out, offset, count == 3, count >= 2);
            }
        }
        _ => unreachable!("pattern letters are validated by parse"),
    }
}

/// `DateTimeFormatter` letters that differ from `SimpleDateFormat`; the rest
/// are shared.
fn write_java_time_field(out: &mut String, zoned: &ZonedTime, letter: char, count: usize) {
    let t = &zoned.time;
    let offset = t.offset().local_minus_utc();
    let nanos = t.timestamp_subsec_nanos().min(999_999_999);
    let seconds_of_day = i64::from(t.num_seconds_from_midnight());
    match letter {
        'u' => write_field(out, t, &zoned.zone_abbreviation, 'y', count),
        'S' => out.push_str(&format!("{nanos:09}")[..count]),
        'n' => pad(out, i64::from(nanos), count),
        'N' => pad(out, seconds_of_day * 1_000_000_000 + i64::from(nanos), count),
        'A' => pad(out, seconds_of_day * 1_000 + i64::from(nanos / 1_000_000), count),
        'Q' | 'q' => {
            let quarter = i64::from(t.month0() / 3 + 1);
            match count {
                1 | 2 => pad(out, quarter, count),
                3 => {
                    let _ = write!(out, "Q{quarter}");
                }
                _ => out.push_str(["1st quarter", "2nd quarter", "3rd quarter", "4th quarter"][quarter as usize - 1]),
            }
        }
        'e' | 'c' if count <= 2 => pad(out, days_from_sunday(t.date_naive()) + 1, count),
        'e' | 'c' => write_field(out, t, &zoned.zone_abbreviation, 'E', count),
        'V' => out.push_str(&zoned.zone_id),
        'v' => out.push_str(&zoned.zone_abbreviation),
        'O' => {
            out.push_str("GMT");
            if offset != 0 {
                if count == 4 {
                    write_offset(out, offset, true, true);
                } else {
                    let minutes = offset.unsigned_abs() / 60;
                    let _ = write!(out, "{}{}", if offset < 0 { '-' } else { '+' }, minutes / 60);
                    if minutes % 60 != 0 {
                        let _ = write!(out, ":{:02}", minutes % 60);
                    }
                }
            }
        }
        'X' | 'x' => {
            if letter == 'X' && offset == 0 {
                out.push('Z');
            } else {
                write_offset(out, offset, matches!(count, 3 | 5), count >= 2);
            }
        }
        'Z' => match count {
            4 => out.push_str(&gmt_name(offset)),
            5 if offset == 0 => out.push('Z'),
            5 => write_offset(out, offset, true, true),
            _ => write_offset(out, offset, false, true),
        },
        _ => write_field(out, t, &zoned.zone_abbreviation, letter, count),
    }
}

/// Parses an RFC 3339 time, for tests.
#[cfg(test)]
pub(crate) fn fixed_time(rfc3339: &str) -> ZonedTime {
    ZonedTime::from_fixed(DateTime::parse_from_rfc3339(rfc3339).expect("valid RFC 3339 time"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(pattern: &str, t: &ZonedTime) -> String {
        JavaDateFormat::parse(pattern).unwrap().format(t)
    }

    #[test]
    fn formats_common_log_patterns() {
        let t = fixed_time("2026-03-07T05:04:09.042+09:00");
        assert_eq!(fmt("yyyy-MM-dd HH:mm:ss.SSS", &t), "2026-03-07 05:04:09.042");
        assert_eq!(fmt("dd/MMM/yyyy:HH:mm:ss Z", &t), "07/Mar/2026:05:04:09 +0900");
        assert_eq!(fmt("yyyy-MM-dd'T'HH:mm:ssXXX", &t), "2026-03-07T05:04:09+09:00");
        assert_eq!(fmt("EEE, d MMM yy h:mm a", &t), "Sat, 7 Mar 26 5:04 AM");
        assert_eq!(fmt("EEEE MMMM", &t), "Saturday March");
        assert_eq!(fmt("yyyyMMddHHmmssSSS", &t), "20260307050409042");
        assert_eq!(fmt("X XX z", &t), "+09 +0900 GMT+09:00");
    }

    #[test]
    fn formats_hour_variants_and_zero_offset() {
        let t = fixed_time("2026-01-01T00:00:00Z");
        assert_eq!(fmt("H k K h a", &t), "0 24 0 12 AM");
        assert_eq!(fmt("X", &t), "Z");
        assert_eq!(fmt("Z", &t), "+0000");
        assert_eq!(fmt("D F u", &t), "1 1 4");
    }

    #[test]
    fn handles_quotes() {
        let t = fixed_time("2026-03-07T05:04:09+09:00");
        assert_eq!(fmt("'Date:' yyyy 'o''clock' ''", &t), "Date: 2026 o'clock '");
        assert_eq!(JavaDateFormat::parse("'open"), Err("Unterminated quote".into()));
    }

    #[test]
    fn rejects_unknown_letters() {
        assert!(JavaDateFormat::parse("yyyy-MM-dd T").is_err());
        assert!(JavaDateFormat::parse("XXXX").is_err());
    }

    #[test]
    fn computes_week_fields_with_sunday_start() {
        // 2025-12-28 is a Sunday; its week contains 2026-01-01, so it is week 1 of 2026.
        let t = fixed_time("2025-12-28T00:00:00Z");
        assert_eq!(fmt("Y w", &t), "2026 1");
        let t = fixed_time("2026-01-04T00:00:00Z"); // Sunday
        assert_eq!(fmt("w W", &t), "2 2");
    }

    fn java_time(pattern: &str, t: &ZonedTime) -> String {
        JavaDateFormat::parse_java_time(pattern).unwrap().format(t)
    }

    #[test]
    fn java_time_patterns() {
        let t = fixed_time("2026-10-01T18:05:09.123456789+09:00");
        assert_eq!(java_time("uuuuMMdd", &t), "20261001");
        assert_eq!(
            java_time("yyyy-MM-dd'T'HH:mm:ss.SSSSSS", &t),
            "2026-10-01T18:05:09.123456"
        );
        assert_eq!(
            java_time("x xx xxx X O OOOO ZZZZ ZZZZZ", &t),
            "+09 +0900 +09:00 +09 GMT+9 GMT+09:00 GMT+09:00 +09:00"
        );
        assert_eq!(java_time("Q QQQ QQQQ e eee VV", &t), "4 Q4 4th quarter 5 Thu +09:00");
        assert_eq!(java_time("yyyy[.MM]", &t), "2026.10");
        let utc = fixed_time("2026-01-01T00:00:00Z");
        assert_eq!(java_time("X x Z ZZZZZ O", &utc), "Z +00 +0000 Z GMT");
        assert!(JavaDateFormat::parse_java_time("yyyy{").is_err());
        assert!(JavaDateFormat::parse_java_time("V").is_err());
        assert_eq!(fmt("u", &t), "4", "SimpleDateFormat u is the day number");
    }

    #[test]
    fn two_digit_year_and_padding() {
        let t = fixed_time("2005-09-03T00:00:00.007Z");
        assert_eq!(fmt("yy y yyyyy M MM S SSSS", &t), "05 2005 02005 9 09 7 0007");
    }
}
