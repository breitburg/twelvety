//! Built-in Liquid filters (after liquidjs).

use anyhow::{Result, bail};
use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, TimeZone, Utc};

use super::render::{LiquidOptions, compare, read_property, stringify, to_enumerable};
use super::strftime::strftime;
use crate::value::{Map, Value};

fn arg(args: &[Value], i: usize) -> Value {
    args.get(i).cloned().unwrap_or_default()
}

fn to_str(v: &Value) -> String {
    stringify(v)
}

fn to_array(v: &Value) -> Vec<Value> {
    match v {
        Value::Array(a) => a.as_ref().clone(),
        Value::Undefined | Value::Null => vec![],
        Value::Dynamic(_) => match v.resolve() {
            Value::Array(a) => a.as_ref().clone(),
            _ => vec![v.clone()],
        },
        other => vec![other.clone()],
    }
}

fn num(v: &Value) -> f64 {
    let n = v.to_number();
    if n.is_nan() { 0.0 } else { n }
}

pub fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&#34;")
        .replace('\'', "&#39;")
}

fn unescape_html(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&#34;", "\"")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

/// Parse a date input for the `date` filter
pub fn parse_date_input(v: &Value) -> Option<DateTime<Utc>> {
    match v {
        Value::Date(d) => Some(*d),
        Value::Number(n) => Utc.timestamp_millis_opt((*n * 1000.0) as i64).single(),
        Value::String(s) => {
            let s = s.trim();
            if s == "now" || s == "today" {
                return Some(Utc::now());
            }
            if !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()) {
                return s
                    .parse::<i64>()
                    .ok()
                    .and_then(|n| Utc.timestamp_opt(n, 0).single());
            }
            if let Some(d) = crate::util::date_parse::parse_iso(s) {
                return Some(d);
            }
            if let Ok(d) = DateTime::parse_from_rfc2822(s) {
                return Some(d.with_timezone(&Utc));
            }
            for fmt in ["%Y-%m-%d %H:%M:%S", "%Y-%m-%dT%H:%M", "%Y/%m/%d %H:%M:%S"] {
                if let Ok(d) = NaiveDateTime::parse_from_str(s, fmt) {
                    return Some(Utc.from_utc_datetime(&d));
                }
            }
            for fmt in ["%Y/%m/%d", "%B %d, %Y", "%b %d, %Y", "%d %B %Y", "%m/%d/%Y"] {
                if let Ok(d) = NaiveDate::parse_from_str(s, fmt) {
                    return Some(Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0)?));
                }
            }
            None
        }
        _ => None,
    }
}

pub fn to_display_tz(d: &DateTime<Utc>, options: &LiquidOptions) -> DateTime<FixedOffset> {
    if let Some(tz) = &options.timezone
        && let Ok(tz) = tz.parse::<chrono_tz::Tz>()
    {
        let local = d.with_timezone(&tz);
        return local.fixed_offset();
    }
    let offset = FixedOffset::east_opt(options.timezone_offset * 60)
        .unwrap_or_else(|| FixedOffset::east_opt(0).unwrap());
    d.with_timezone(&offset)
}

fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'~'
            | b'!'
            | b'*'
            | b'\''
            | b'('
            | b')' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn url_decode(s: &str) -> String {
    let s = s.replace('+', " ");
    percent_encoding::percent_decode_str(&s)
        .decode_utf8_lossy()
        .to_string()
}

fn strip_html(s: &str) -> String {
    static RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"(?s)<script.*?</script>|<style.*?</style>|<.*?>|<!--.*?-->").unwrap()
    });
    RE.replace_all(s, "").to_string()
}

fn sort_compare(a: &Value, b: &Value) -> std::cmp::Ordering {
    match (a.is_nullish(), b.is_nullish()) {
        (true, true) => std::cmp::Ordering::Equal,
        (true, false) => std::cmp::Ordering::Greater,
        (false, true) => std::cmp::Ordering::Less,
        _ => a.js_compare(b).unwrap_or(std::cmp::Ordering::Equal),
    }
}

fn round_to(n: f64, digits: i32) -> f64 {
    let f = 10f64.powi(digits);
    (n * f).round() / f
}

/// Apply a built-in filter. Returns `Ok(None)` if the filter does not exist.
pub fn apply_builtin(
    name: &str,
    input: &Value,
    args: &[Value],
    options: &LiquidOptions,
) -> Result<Option<Value>> {
    let v = match name {
        "abs" => Value::Number(num(input).abs()),
        "append" => Value::str(to_str(input) + &to_str(&arg(args, 0))),
        "prepend" => Value::str(to_str(&arg(args, 0)) + &to_str(input)),
        "at_least" => Value::Number(num(input).max(num(&arg(args, 0)))),
        "at_most" => Value::Number(num(input).min(num(&arg(args, 0)))),
        "capitalize" => {
            let s = to_str(input);
            let mut c = s.chars();
            match c.next() {
                Some(f) => {
                    Value::str(f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase())
                }
                None => Value::str(""),
            }
        }
        "ceil" => Value::Number(num(input).ceil()),
        "floor" => Value::Number(num(input).floor()),
        "round" => Value::Number(round_to(num(input), num(&arg(args, 0)) as i32)),
        "plus" => Value::Number(num(input) + num(&arg(args, 0))),
        "minus" => Value::Number(num(input) - num(&arg(args, 0))),
        "times" => Value::Number(num(input) * num(&arg(args, 0))),
        "divided_by" => {
            let d = num(&arg(args, 0));
            let integer = arg(args, 1).is_truthy();
            let r = num(input) / d;
            Value::Number(if integer { r.floor() } else { r })
        }
        "modulo" => Value::Number(num(input) % num(&arg(args, 0))),
        "compact" => Value::array(
            to_array(input)
                .into_iter()
                .filter(|x| !x.is_nullish())
                .collect(),
        ),
        "concat" => {
            let mut a = to_array(input);
            a.extend(to_array(&arg(args, 0)));
            Value::array(a)
        }
        "push" => {
            let mut a = to_array(input);
            a.push(arg(args, 0));
            Value::array(a)
        }
        "pop" => {
            let mut a = to_array(input);
            a.pop();
            Value::array(a)
        }
        "shift" => {
            let mut a = to_array(input);
            if !a.is_empty() {
                a.remove(0);
            }
            Value::array(a)
        }
        "unshift" => {
            let mut a = to_array(input);
            a.insert(0, arg(args, 0));
            Value::array(a)
        }
        "date" => {
            let Some(d) = parse_date_input(input) else {
                return Ok(Some(input.clone()));
            };
            let format = arg(args, 0);
            let d = to_display_tz(&d, options);
            if format.is_nullish() || to_str(&format).is_empty() {
                Value::str(strftime(&d, "%A, %B %-e, %Y at %-l:%M %P %z"))
            } else {
                Value::str(strftime(&d, &to_str(&format)))
            }
        }
        "date_to_xmlschema" => match parse_date_input(input) {
            Some(d) => Value::str(strftime(
                &to_display_tz(&d, options),
                "%Y-%m-%dT%H:%M:%S%:z",
            )),
            None => input.clone(),
        },
        "date_to_rfc822" => match parse_date_input(input) {
            Some(d) => Value::str(strftime(
                &to_display_tz(&d, options),
                "%a, %d %b %Y %H:%M:%S %z",
            )),
            None => input.clone(),
        },
        "date_to_string" => match parse_date_input(input) {
            Some(d) => Value::str(strftime(&to_display_tz(&d, options), "%d %b %Y")),
            None => input.clone(),
        },
        "date_to_long_string" => match parse_date_input(input) {
            Some(d) => Value::str(strftime(&to_display_tz(&d, options), "%d %B %Y")),
            None => input.clone(),
        },
        "default" => {
            let allow_false = args.iter().any(|a| {
                a.as_array()
                    .map(|kv| {
                        kv.len() == 2 && kv[0].as_str() == Some("allow_false") && kv[1].is_truthy()
                    })
                    .unwrap_or(false)
            });
            let is_empty = match input {
                Value::Bool(false) => !allow_false,
                Value::Undefined | Value::Null => true,
                Value::String(s) => s.is_empty(),
                Value::Array(a) => a.is_empty(),
                Value::Object(o) => o.is_empty(),
                _ => false,
            };
            if is_empty {
                arg(args, 0)
            } else {
                input.clone()
            }
        }
        "downcase" => Value::str(to_str(input).to_lowercase()),
        "upcase" => Value::str(to_str(input).to_uppercase()),
        "escape" | "xml_escape" => Value::str(escape_html(&to_str(input))),
        "escape_once" => Value::str(escape_html(&unescape_html(&to_str(input)))),
        "first" => match input {
            Value::String(s) => {
                Value::str(s.chars().next().map(|c| c.to_string()).unwrap_or_default())
            }
            _ => to_array(input).first().cloned().unwrap_or_default(),
        },
        "last" => match input {
            Value::String(s) => {
                Value::str(s.chars().last().map(|c| c.to_string()).unwrap_or_default())
            }
            _ => to_array(input).last().cloned().unwrap_or_default(),
        },
        "join" => {
            let sep = if args.is_empty() {
                " ".to_string()
            } else {
                to_str(&arg(args, 0))
            };
            Value::str(
                to_array(input)
                    .iter()
                    .map(to_str)
                    .collect::<Vec<_>>()
                    .join(&sep),
            )
        }
        "json" | "jsonify" => {
            let indent = num(&arg(args, 0)) as usize;
            if indent > 0 {
                Value::str(input.to_json_string_pretty(indent))
            } else {
                Value::str(input.to_json_string())
            }
        }
        "inspect" => Value::str(input.to_json_string()),
        "lstrip" => Value::str(to_str(input).trim_start()),
        "rstrip" => Value::str(to_str(input).trim_end()),
        "strip" => Value::str(to_str(input).trim()),
        "strip_newlines" => Value::str(to_str(input).replace(['\r', '\n'], "")),
        "newline_to_br" => Value::str(
            to_str(input)
                .replace("\r\n", "\n")
                .replace('\n', "<br />\n"),
        ),
        "normalize_whitespace" => Value::str(
            to_str(input)
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
        ),
        "number_of_words" => Value::Number(to_str(input).split_whitespace().count() as f64),
        "map" => {
            let key = to_str(&arg(args, 0));
            Value::array(
                to_array(input)
                    .iter()
                    .map(|x| read_property(x, &Value::str(&key)))
                    .collect(),
            )
        }
        "sum" => {
            let key = arg(args, 0);
            let total: f64 = to_array(input)
                .iter()
                .map(|x| {
                    if key.is_nullish() {
                        num(x)
                    } else {
                        num(&read_property(x, &key))
                    }
                })
                .sum();
            Value::Number(total)
        }
        "remove" => Value::str(to_str(input).replace(&to_str(&arg(args, 0)), "")),
        "remove_first" => Value::str(to_str(input).replacen(&to_str(&arg(args, 0)), "", 1)),
        "remove_last" => {
            let s = to_str(input);
            let needle = to_str(&arg(args, 0));
            match s.rfind(&needle) {
                Some(i) if !needle.is_empty() => {
                    Value::str(format!("{}{}", &s[..i], &s[i + needle.len()..]))
                }
                _ => Value::str(s),
            }
        }
        "replace" => {
            Value::str(to_str(input).replace(&to_str(&arg(args, 0)), &to_str(&arg(args, 1))))
        }
        "replace_first" => {
            Value::str(to_str(input).replacen(&to_str(&arg(args, 0)), &to_str(&arg(args, 1)), 1))
        }
        "replace_last" => {
            let s = to_str(input);
            let needle = to_str(&arg(args, 0));
            match s.rfind(&needle) {
                Some(i) if !needle.is_empty() => Value::str(format!(
                    "{}{}{}",
                    &s[..i],
                    to_str(&arg(args, 1)),
                    &s[i + needle.len()..]
                )),
                _ => Value::str(s),
            }
        }
        "reverse" => {
            let mut a = to_array(input);
            a.reverse();
            Value::array(a)
        }
        "size" => Value::Number(match input {
            Value::Undefined | Value::Null => 0.0,
            other => other.len() as f64,
        }),
        "slice" => {
            let start = num(&arg(args, 0)) as i64;
            let length = if args.len() > 1 {
                num(&arg(args, 1)) as i64
            } else {
                1
            };
            match input {
                Value::String(s) => {
                    let chars: Vec<char> = s.chars().collect();
                    let len = chars.len() as i64;
                    let begin = if start < 0 {
                        (len + start).max(0)
                    } else {
                        start.min(len)
                    };
                    let end = (begin + length).min(len).max(begin);
                    Value::str(
                        chars[begin as usize..end as usize]
                            .iter()
                            .collect::<String>(),
                    )
                }
                _ => {
                    let a = to_array(input);
                    let len = a.len() as i64;
                    let begin = if start < 0 {
                        (len + start).max(0)
                    } else {
                        start.min(len)
                    };
                    let end = (begin + length).min(len).max(begin);
                    Value::array(a[begin as usize..end as usize].to_vec())
                }
            }
        }
        "sort" => {
            let mut a = to_array(input);
            let key = arg(args, 0);
            if key.is_nullish() {
                a.sort_by(sort_compare);
            } else {
                a.sort_by(|x, y| sort_compare(&read_property(x, &key), &read_property(y, &key)));
            }
            Value::array(a)
        }
        "sort_natural" => {
            let mut a = to_array(input);
            let key = arg(args, 0);
            let lower = |v: &Value| -> String {
                if key.is_nullish() {
                    to_str(v).to_lowercase()
                } else {
                    to_str(&read_property(v, &key)).to_lowercase()
                }
            };
            a.sort_by_key(|x| lower(x));
            Value::array(a)
        }
        "split" => {
            let s = to_str(input);
            let sep = to_str(&arg(args, 0));
            let mut parts: Vec<Value> = if sep.is_empty() {
                s.chars().map(|c| Value::str(c.to_string())).collect()
            } else {
                s.split(sep.as_str()).map(Value::str).collect()
            };
            // Like Ruby/liquidjs: trailing empty strings are removed
            while parts
                .last()
                .map(|p| p.as_str() == Some(""))
                .unwrap_or(false)
            {
                parts.pop();
            }
            Value::array(parts)
        }
        "strip_html" => Value::str(strip_html(&to_str(input))),
        "truncate" => {
            let s = to_str(input);
            let length = if args.is_empty() {
                50
            } else {
                num(&arg(args, 0)) as usize
            };
            let ellipsis = if args.len() > 1 {
                to_str(&arg(args, 1))
            } else {
                "...".into()
            };
            let chars: Vec<char> = s.chars().collect();
            if chars.len() <= length {
                Value::str(s)
            } else {
                let keep = length.saturating_sub(ellipsis.chars().count());
                Value::str(chars[..keep].iter().collect::<String>() + &ellipsis)
            }
        }
        "truncatewords" => {
            let s = to_str(input);
            let words = if args.is_empty() {
                15
            } else {
                (num(&arg(args, 0)) as usize).max(1)
            };
            let ellipsis = if args.len() > 1 {
                to_str(&arg(args, 1))
            } else {
                "...".into()
            };
            let parts: Vec<&str> = s.split_whitespace().collect();
            if parts.len() <= words {
                Value::str(s)
            } else {
                Value::str(parts[..words].join(" ") + &ellipsis)
            }
        }
        "uniq" => {
            let mut out: Vec<Value> = vec![];
            for x in to_array(input) {
                if !out.iter().any(|y| y.strict_equals(&x)) {
                    out.push(x);
                }
            }
            Value::array(out)
        }
        "url_encode" | "cgi_escape" => Value::str(url_encode(&to_str(input))),
        "uri_escape" => Value::str(
            percent_encoding::utf8_percent_encode(&to_str(input), &URI_ESCAPE_SET).to_string(),
        ),
        "url_decode" => Value::str(url_decode(&to_str(input))),
        "where" => {
            let key = arg(args, 0);
            let target = args.get(1).cloned();
            Value::array(
                to_array(input)
                    .into_iter()
                    .filter(|item| {
                        let v = read_property(item, &key);
                        match &target {
                            None => v.is_truthy(),
                            Some(t) => compare("==", &v, t),
                        }
                    })
                    .collect(),
            )
        }
        "reject" => {
            let key = arg(args, 0);
            let target = args.get(1).cloned();
            Value::array(
                to_array(input)
                    .into_iter()
                    .filter(|item| {
                        let v = read_property(item, &key);
                        match &target {
                            None => !v.is_truthy(),
                            Some(t) => !compare("==", &v, t),
                        }
                    })
                    .collect(),
            )
        }
        "find" => {
            let key = arg(args, 0);
            let target = arg(args, 1);
            to_array(input)
                .into_iter()
                .find(|item| compare("==", &read_property(item, &key), &target))
                .unwrap_or_default()
        }
        "find_index" => {
            let key = arg(args, 0);
            let target = arg(args, 1);
            to_array(input)
                .iter()
                .position(|item| compare("==", &read_property(item, &key), &target))
                .map(|i| Value::Number(i as f64))
                .unwrap_or_default()
        }
        "has" => {
            let key = arg(args, 0);
            let target = args.get(1).cloned();
            Value::Bool(to_array(input).iter().any(|item| {
                let v = read_property(item, &key);
                match &target {
                    None => v.is_truthy(),
                    Some(t) => compare("==", &v, t),
                }
            }))
        }
        "group_by" => {
            let key = arg(args, 0);
            let mut groups: Vec<(Value, Vec<Value>)> = vec![];
            for item in to_array(input) {
                let k = read_property(&item, &key);
                match groups.iter_mut().find(|(g, _)| g.strict_equals(&k)) {
                    Some((_, items)) => items.push(item),
                    None => groups.push((k, vec![item])),
                }
            }
            Value::array(
                groups
                    .into_iter()
                    .map(|(name, items)| {
                        let mut m = Map::new();
                        m.insert("name".into(), name);
                        m.insert("items".into(), Value::array(items));
                        Value::from_map(m)
                    })
                    .collect(),
            )
        }
        "array_to_sentence_string" => {
            let a: Vec<String> = to_array(input).iter().map(to_str).collect();
            let connector = if args.is_empty() {
                "and".to_string()
            } else {
                to_str(&arg(args, 0))
            };
            Value::str(match a.len() {
                0 => String::new(),
                1 => a[0].clone(),
                2 => format!("{} {} {}", a[0], connector, a[1]),
                n => format!("{}, {} {}", a[..n - 1].join(", "), connector, a[n - 1]),
            })
        }
        "to_integer" => Value::Number(num(input).trunc()),
        "base64_encode" => Value::str(base64_encode(to_str(input).as_bytes())),
        "base64_decode" => Value::str(String::from_utf8_lossy(&base64_decode(&to_str(input)))),
        "raw" => input.clone(),
        _ => return Ok(None),
    };
    if name == "divided_by" && num(&arg(args, 0)) == 0.0 {
        bail!("divided by 0");
    }
    Ok(Some(v))
}

const URI_ESCAPE_SET: percent_encoding::AsciiSet = percent_encoding::NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'!')
    .remove(b'~')
    .remove(b'*')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')')
    .remove(b';')
    .remove(b'/')
    .remove(b'?')
    .remove(b':')
    .remove(b'@')
    .remove(b'&')
    .remove(b'=')
    .remove(b'+')
    .remove(b'$')
    .remove(b',')
    .remove(b'#')
    .remove(b'[')
    .remove(b']');

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64_encode(input: &[u8]) -> String {
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(B64[(n >> 18) as usize & 63] as char);
        out.push(B64[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            B64[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            B64[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

pub fn base64_decode(input: &str) -> Vec<u8> {
    let mut out = vec![];
    let mut buf = 0u32;
    let mut bits = 0;
    for c in input.bytes() {
        let v = match B64.iter().position(|&x| x == c) {
            Some(v) => v as u32,
            None => continue,
        };
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    out
}

#[allow(dead_code)]
fn enumerable(v: &Value) -> Vec<Value> {
    to_enumerable(v)
}
