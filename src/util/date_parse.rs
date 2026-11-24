//! Port of `Util/DateParse.js` and `@11ty/parse-date-strings`.

use std::sync::LazyLock;

use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, TimeZone, Utc};
use regex::Regex;

static FULL_DATE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([+-]\d{6}|\d{4})-?([01]\d)-?([0-3]\d)$").unwrap());
static DATETIME_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([+-]\d{6}|\d{4})-?([01]\d)-?([0-3]\d)[Tt ]([0-2]\d(?:[.,]\d+)?)(?::?([0-5]\d(?:[.,]\d+)?)(?::?([0-5]\d))?(?:[.,](\d{1,9}))?)?(Z|[+-][0-2]\d(?::?[0-5]\d)?)?$").unwrap()
});
static TIMEZONE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([+-]\d{2})(?::?(\d{2}))?$").unwrap());
static IS_FRACTIONAL_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d+[.,]\d+$").unwrap());

// js-yaml timestamp formats (YAML 1.1)
static YAML_DATE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([0-9][0-9][0-9][0-9])-([0-9][0-9])-([0-9][0-9])$").unwrap());
static YAML_TIMESTAMP_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([0-9][0-9][0-9][0-9])-([0-9][0-9]?)-([0-9][0-9]?)(?:[Tt]|[ \t]+)([0-9][0-9]?):([0-9][0-9]):([0-9][0-9])(?:\.([0-9]*))?(?:[ \t]*(Z|([-+])([0-9][0-9]?)(?::([0-9][0-9]))?))?$").unwrap()
});

fn get_timezone_offset(offset: &str) -> (i64, i64) {
    let Some(caps) = TIMEZONE_REGEX.captures(offset) else {
        return (0, 0);
    };
    let hours_str = caps.get(1).map(|m| m.as_str()).unwrap_or("0");
    let minutes_str = caps.get(2).map(|m| m.as_str()).unwrap_or("0");
    let sign = if hours_str.starts_with('-') { -1 } else { 1 };
    let hours: i64 = hours_str.parse().unwrap_or(0);
    let minutes: i64 = minutes_str.parse::<i64>().unwrap_or(0) * sign;
    (hours, minutes)
}

/// `IsoDate.parse` from `@11ty/parse-date-strings`: times without an offset are UTC.
pub fn iso_date_parse(s: &str) -> Result<DateTime<Utc>, String> {
    let caps = FULL_DATE_REGEX
        .captures(s)
        .or_else(|| DATETIME_REGEX.captures(s))
        .ok_or_else(|| format!("Unsupported date format: {s}"))?;
    for i in 4..6 {
        if let Some(m) = caps.get(i) {
            if IS_FRACTIONAL_REGEX.is_match(m.as_str()) {
                return Err(format!(
                    "Unsupported date format (fractional hours or minutes): {s}"
                ));
            }
        }
    }
    let get = |i: usize, default: &str| -> String {
        caps.get(i)
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| default.to_string())
    };
    let year: i32 = get(1, "").parse().map_err(|_| format!("Unsupported date format: {s}"))?;
    let month: i64 = get(2, "1").parse::<i64>().unwrap_or(1) - 1;
    let day: i64 = get(3, "1").parse().unwrap_or(1);
    let (off_h, off_m) = get_timezone_offset(&get(8, "Z"));
    let hours: i64 = get(4, "0").parse::<i64>().unwrap_or(0) - off_h;
    let minutes: i64 = get(5, "0").parse::<i64>().unwrap_or(0) - off_m;
    let seconds: i64 = get(6, "0").parse().unwrap_or(0);
    let ms_str = get(7, "0");
    let ms: i64 = ms_str[..ms_str.len().min(3)].parse().unwrap_or(0);
    // Only the first 3 digits count as milliseconds (and "5" is 5ms, like parseInt)

    if !(0..=11).contains(&month) {
        return Err(format!("Unsupported date format (invalid month): {s}"));
    }
    let base = NaiveDate::from_ymd_opt(year, (month + 1) as u32, 1)
        .ok_or_else(|| format!("Unsupported date format: {s}"))?;
    if day < 1 {
        return Err(format!("Unsupported date format (invalid days for month): {s}"));
    }
    let date = base + Duration::days(day - 1);
    if date.month0() as i64 != month {
        return Err(format!("Unsupported date format (invalid days for month): {s}"));
    }
    let naive = date.and_hms_opt(0, 0, 0).unwrap()
        + Duration::hours(hours)
        + Duration::minutes(minutes)
        + Duration::seconds(seconds)
        + Duration::milliseconds(ms);
    Ok(Utc.from_utc_datetime(&naive))
}

use chrono::Datelike;

pub fn parse_iso(s: &str) -> Option<DateTime<Utc>> {
    iso_date_parse(s).ok().or_else(|| {
        DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|d| d.with_timezone(&Utc))
    })
}

/// Parse YAML timestamps (js-yaml resolves these into `Date` instances)
pub fn parse_yaml_timestamp(s: &str) -> Option<DateTime<Utc>> {
    if let Some(c) = YAML_DATE_REGEX.captures(s) {
        let y = c[1].parse().ok()?;
        let m = c[2].parse().ok()?;
        let d = c[3].parse().ok()?;
        let date = NaiveDate::from_ymd_opt(y, m, d)?;
        return Some(Utc.from_utc_datetime(&date.and_hms_opt(0, 0, 0)?));
    }
    let c = YAML_TIMESTAMP_REGEX.captures(s)?;
    let y = c[1].parse().ok()?;
    let mo = c[2].parse().ok()?;
    let d = c[3].parse().ok()?;
    let h = c[4].parse().ok()?;
    let mi = c[5].parse().ok()?;
    let se = c[6].parse().ok()?;
    let mut ms: i64 = 0;
    if let Some(frac) = c.get(7) {
        let mut f = frac.as_str().chars().take(3).collect::<String>();
        while f.len() < 3 {
            f.push('0');
        }
        ms = f.parse().unwrap_or(0);
    }
    let naive = NaiveDate::from_ymd_opt(y, mo, d)?.and_hms_opt(h, mi, se)?
        + Duration::milliseconds(ms);
    let mut dt = Utc.from_utc_datetime(&naive);
    if let Some(sign) = c.get(9) {
        let tz_h: i64 = c.get(10).map(|m| m.as_str().parse().unwrap_or(0)).unwrap_or(0);
        let tz_m: i64 = c.get(11).map(|m| m.as_str().parse().unwrap_or(0)).unwrap_or(0);
        let mut delta = Duration::minutes(tz_h * 60 + tz_m);
        if sign.as_str() == "-" {
            delta = -delta;
        }
        dt -= delta;
    }
    Some(dt)
}

/// A timezone used for `setDateTimeZone`
#[derive(Clone, Debug)]
pub struct TimeZoneParser {
    pub zone: String,
    tz: chrono_tz::Tz,
}

impl TimeZoneParser {
    pub fn new(zone: &str) -> anyhow::Result<Self> {
        let tz: chrono_tz::Tz = zone
            .parse()
            .map_err(|_| anyhow::anyhow!("Invalid time zone passed to setDateTimeZone: {zone}"))?;
        Ok(Self {
            zone: zone.to_string(),
            tz,
        })
    }

    /// Re-interpret a UTC wall time as a wall time in this zone
    pub fn to_date(&self, wall: DateTime<Utc>) -> DateTime<Utc> {
        let naive: NaiveDateTime = wall.naive_utc();
        match self.tz.from_local_datetime(&naive) {
            chrono::LocalResult::Single(d) => d.with_timezone(&Utc),
            chrono::LocalResult::Ambiguous(d, _) => d.with_timezone(&Utc),
            chrono::LocalResult::None => wall,
        }
    }
}

fn has_offset(date_value: &str) -> bool {
    DATETIME_REGEX
        .captures(date_value)
        .and_then(|c| c.get(8))
        .is_some()
}

/// `fromISOtoDateUTC`
pub fn from_iso_to_date_utc(
    date_value: &str,
    input_path: &str,
    time_zone: Option<&TimeZoneParser>,
) -> anyhow::Result<DateTime<Utc>> {
    match iso_date_parse(date_value) {
        Ok(mut date) => {
            if let Some(tz) = time_zone {
                if !has_offset(date_value) {
                    date = tz.to_date(date);
                }
            }
            Ok(date)
        }
        // The parse error is the cause (`{ cause: e }` upstream)
        Err(e) => Err(anyhow::anyhow!(e).context(format!(
            "Data cascade value for `date` ({date_value}) is invalid{}",
            if input_path.is_empty() {
                String::new()
            } else {
                format!(" for {input_path}")
            }
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::to_iso_string;

    #[test]
    fn parses_dates() {
        assert_eq!(
            to_iso_string(&iso_date_parse("2018-01-01").unwrap()),
            "2018-01-01T00:00:00.000Z"
        );
        assert_eq!(
            to_iso_string(&iso_date_parse("2018-01-01T12:30:00-05:00").unwrap()),
            "2018-01-01T17:30:00.000Z"
        );
        assert!(iso_date_parse("2018-02-31").is_err());
        assert!(iso_date_parse("not a date").is_err());
    }

    #[test]
    fn yaml_timestamps() {
        assert_eq!(
            to_iso_string(&parse_yaml_timestamp("2016-01-01").unwrap()),
            "2016-01-01T00:00:00.000Z"
        );
        assert_eq!(
            to_iso_string(&parse_yaml_timestamp("2001-12-14 21:59:43.10 -5").unwrap()),
            "2001-12-15T02:59:43.100Z"
        );
        assert!(parse_yaml_timestamp("hello").is_none());
    }
}
