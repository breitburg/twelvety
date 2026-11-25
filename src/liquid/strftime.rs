//! `strftime` as implemented by liquidjs (used by the `date` filter).

use chrono::{DateTime, Datelike, FixedOffset, Timelike};

const DAY_NAMES: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
const MONTH_NAMES: [&str; 12] = [
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

fn pad(n: i64, width: usize, ch: char) -> String {
    let s = n.abs().to_string();
    let mut out = String::new();
    if n < 0 {
        out.push('-');
    }
    for _ in s.len()..width {
        out.push(ch);
    }
    out.push_str(&s);
    out
}

fn day_of_year(d: &DateTime<FixedOffset>) -> i64 {
    d.ordinal() as i64
}

fn week_number(d: &DateTime<FixedOffset>, start_day: u32) -> i64 {
    // liquidjs getWeekOfYear
    let jan1 = d.with_ordinal(1).unwrap();
    let wday = jan1.weekday().num_days_from_sunday();
    let offset = (7 + wday as i64 - start_day as i64) % 7;
    let yday = day_of_year(d) - 1;
    (yday + offset) / 7
}

pub fn strftime(d: &DateTime<FixedOffset>, format: &str) -> String {
    let chars: Vec<char> = format.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c != '%' {
            out.push(c);
            i += 1;
            continue;
        }
        i += 1;
        // flags
        let mut flags = String::new();
        while i < chars.len() && "-_0^#:".contains(chars[i]) {
            flags.push(chars[i]);
            i += 1;
        }
        // width
        let mut width = String::new();
        while i < chars.len() && chars[i].is_ascii_digit() {
            width.push(chars[i]);
            i += 1;
        }
        if i >= chars.len() {
            out.push('%');
            out.push_str(&flags);
            out.push_str(&width);
            break;
        }
        let conv = chars[i];
        i += 1;
        let width: Option<usize> = width.parse().ok();
        let no_pad = flags.contains('-');
        let space_pad = flags.contains('_');
        let upper = flags.contains('^');
        let num = |n: i64, default_width: usize, default_pad: char| -> String {
            if no_pad {
                return n.to_string();
            }
            let ch = if space_pad {
                ' '
            } else if flags.contains('0') {
                '0'
            } else {
                default_pad
            };
            pad(n, width.unwrap_or(default_width), ch)
        };
        let s: String = match conv {
            'a' => DAY_NAMES[d.weekday().num_days_from_sunday() as usize][..3].to_string(),
            'A' => DAY_NAMES[d.weekday().num_days_from_sunday() as usize].to_string(),
            'b' | 'h' => MONTH_NAMES[d.month0() as usize][..3].to_string(),
            'B' => MONTH_NAMES[d.month0() as usize].to_string(),
            'c' => strftime(d, "%a %b %e %H:%M:%S %Y"),
            'C' => num((d.year() / 100) as i64, 2, '0'),
            'd' => num(d.day() as i64, 2, '0'),
            'e' => num(d.day() as i64, 2, ' '),
            'F' => strftime(d, "%Y-%m-%d"),
            'H' => num(d.hour() as i64, 2, '0'),
            'I' => num(((d.hour() + 11) % 12 + 1) as i64, 2, '0'),
            'j' => num(day_of_year(d), 3, '0'),
            'k' => num(d.hour() as i64, 2, ' '),
            'l' => num(((d.hour() + 11) % 12 + 1) as i64, 2, ' '),
            'L' => num((d.nanosecond() / 1_000_000) as i64, 3, '0'),
            'm' => num(d.month() as i64, 2, '0'),
            'M' => num(d.minute() as i64, 2, '0'),
            'N' => {
                let w = width.unwrap_or(9);
                let ns = format!("{:09}", d.nanosecond());
                ns[..w.min(9)].to_string()
            }
            'p' => {
                if d.hour() < 12 {
                    "AM".into()
                } else {
                    "PM".into()
                }
            }
            'P' => {
                if d.hour() < 12 {
                    "am".into()
                } else {
                    "pm".into()
                }
            }
            'q' => {
                let day = d.day();
                let suffix = match (day % 10, day % 100) {
                    (1, x) if x != 11 => "st",
                    (2, x) if x != 12 => "nd",
                    (3, x) if x != 13 => "rd",
                    _ => "th",
                };
                suffix.to_string()
            }
            'r' => strftime(d, "%I:%M:%S %p"),
            'R' => strftime(d, "%H:%M"),
            's' => d.timestamp().to_string(),
            'S' => num(d.second() as i64, 2, '0'),
            't' => "\t".into(),
            'T' => strftime(d, "%H:%M:%S"),
            'u' => {
                let w = d.weekday().num_days_from_sunday();
                (if w == 0 { 7 } else { w }).to_string()
            }
            'U' => num(week_number(d, 0), 2, '0'),
            'V' => num(d.iso_week().week() as i64, 2, '0'),
            'w' => d.weekday().num_days_from_sunday().to_string(),
            'W' => num(week_number(d, 1), 2, '0'),
            'x' => strftime(d, "%m/%d/%y"),
            'X' => strftime(d, "%H:%M:%S"),
            'y' => num((d.year() % 100) as i64, 2, '0'),
            'Y' => {
                if no_pad {
                    d.year().to_string()
                } else {
                    num(d.year() as i64, width.unwrap_or(4), '0')
                }
            }
            'z' => {
                let offset = d.offset().local_minus_utc() / 60;
                let sign = if offset < 0 { '-' } else { '+' };
                let offset = offset.abs();
                let h = offset / 60;
                let m = offset % 60;
                if flags.contains(':') {
                    format!("{sign}{h:02}:{m:02}")
                } else {
                    format!("{sign}{h:02}{m:02}")
                }
            }
            'Z' => {
                let offset = d.offset().local_minus_utc();
                if offset == 0 {
                    "UTC".into()
                } else {
                    strftime(d, "%z")
                }
            }
            '%' => "%".into(),
            'n' => "\n".into(),
            other => format!("%{flags}{other}"),
        };
        if upper {
            out.push_str(&s.to_uppercase());
        } else {
            out.push_str(&s);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn formats() {
        let d = FixedOffset::east_opt(0)
            .unwrap()
            .with_ymd_and_hms(2018, 1, 5, 9, 3, 7)
            .unwrap();
        assert_eq!(strftime(&d, "%Y-%m-%d"), "2018-01-05");
        assert_eq!(strftime(&d, "%b %-d, %Y"), "Jan 5, 2018");
        assert_eq!(strftime(&d, "%A %B %e"), "Friday January  5");
        assert_eq!(strftime(&d, "%I:%M %p"), "09:03 AM");
    }
}
