//! Ported from upstream `test/SortableTest.js`. Luxon `DateTime`s and JS `Date`s are both
//! modelled as `Value::Date` (or their millisecond timestamp).

use chrono::{TimeZone, Utc};
use twelvety::Value;
use twelvety::template_collection::{SortFunction, Sortable};

fn strs(items: &[&str]) -> Vec<Value> {
    items.iter().map(Value::str).collect()
}

fn nums(items: &[i64]) -> Vec<Value> {
    items.iter().map(|n| Value::from(*n)).collect()
}

/// `DateTime.fromISO("YYYY-MM-DD")`
fn date(y: i32, m: u32, d: u32) -> Value {
    Value::Date(Utc.with_ymd_and_hms(y, m, d, 0, 0, 0).unwrap())
}

/// `new Date(y, m - 1, d).getTime()`
fn time(y: i32, m: u32, d: u32) -> Value {
    Value::from(
        Utc.with_ymd_and_hms(y, m, d, 0, 0, 0)
            .unwrap()
            .timestamp_millis(),
    )
}

fn amz() -> Sortable {
    let mut s = Sortable::new();
    s.add("a");
    s.add("z");
    s.add("m");
    s
}

#[test]
fn get_sort_function() {
    assert_eq!(
        Sortable::new().get_sort_function(),
        SortFunction::AlphabeticAscending
    );
}

#[test]
fn alphabetic_ascending() {
    let s = amz();
    assert!(!s.is_sort_numeric);
    assert!(s.is_sort_ascending);
    assert_eq!(s.sort(), strs(&["a", "m", "z"]));
}

#[test]
fn alphabetic_ascending_shortcut() {
    assert_eq!(amz().sort_ascending(), strs(&["a", "m", "z"]));
}

#[test]
fn alphabetic_descending() {
    let mut s = Sortable::new();
    s.set_sort_descending(true);
    assert!(!s.is_sort_numeric);
    assert!(!s.is_sort_ascending);
    s.add("a");
    s.add("z");
    s.add("m");
    assert_eq!(s.sort(), strs(&["z", "m", "a"]));
}

#[test]
fn alphabetic_descending_shortcut() {
    assert_eq!(amz().sort_descending(), strs(&["z", "m", "a"]));
}

#[test]
fn numeric_ascending() {
    let mut s = Sortable::new();
    s.set_sort_numeric(true);
    assert!(s.is_sort_numeric);
    assert!(s.is_sort_ascending);
    s.add(1);
    s.add(4);
    s.add(2);
    assert_eq!(s.sort(), nums(&[1, 2, 4]));
}

#[test]
fn numeric_descending() {
    let mut s = Sortable::new();
    s.set_sort_numeric(true);
    s.set_sort_descending(true);
    assert!(s.is_sort_numeric);
    assert!(!s.is_sort_ascending);
    s.add(1);
    s.add(4);
    s.add(2);
    assert_eq!(s.sort(), nums(&[4, 2, 1]));
}

#[test]
fn date_assumptions() {
    assert_eq!(
        date(2007, 10, 10).to_number() - time(2007, 10, 10).to_number(),
        0.0
    );
    assert_eq!(
        date(2008, 10, 10).to_number() - time(2008, 10, 10).to_number(),
        0.0
    );
    assert_ne!(
        date(2008, 10, 10).to_number() - time(2007, 10, 10).to_number(),
        0.0
    );
}

#[test]
fn date_and_sortable_assumptions() {
    use SortFunction::*;
    assert_eq!(
        AlphabeticAscending.compare(&date(2007, 10, 10), &time(2007, 10, 10)),
        0.0
    );
    assert_eq!(
        AlphabeticDescending.compare(&date(2007, 10, 10), &time(2007, 10, 10)),
        0.0
    );
    assert_eq!(
        AlphabeticAscending.compare(&date(2008, 10, 10), &time(2007, 10, 10)),
        1.0
    );
    assert_eq!(
        AlphabeticDescending.compare(&date(2008, 10, 10), &time(2007, 10, 10)),
        -1.0
    );

    assert_eq!(
        NumericAscending.compare(&date(2008, 10, 10), &time(2008, 10, 10)),
        0.0
    );
    assert_eq!(
        NumericDescending.compare(&date(2008, 10, 10), &time(2008, 10, 10)),
        0.0
    );
    assert!(NumericAscending.compare(&date(2008, 10, 10), &time(2007, 10, 10)) > 0.0);
    assert!(NumericDescending.compare(&date(2008, 10, 10), &time(2007, 10, 10)) < 0.0);
}

#[test]
fn date_ascending() {
    let mut s = Sortable::new();
    s.add(date(2009, 10, 10));
    s.add(date(2008, 10, 10));
    s.add(date(2007, 10, 10));
    assert_eq!(
        s.sort(),
        vec![date(2007, 10, 10), date(2008, 10, 10), date(2009, 10, 10)]
    );
}

#[test]
fn date_descending() {
    let mut s = Sortable::new();
    s.set_sort_descending(true);
    s.add(date(2008, 10, 10));
    s.add(date(2009, 10, 10));
    s.add(date(2007, 10, 10));
    assert_eq!(
        s.sort(),
        vec![date(2009, 10, 10), date(2008, 10, 10), date(2007, 10, 10)]
    );
}

#[test]
fn alphabetic_ascending_short_str_sort_arg() {
    assert_eq!(amz().sort_by_name("A-Z").unwrap(), strs(&["a", "m", "z"]));
}

#[test]
fn alphabetic_descending_short_str_sort_arg() {
    assert_eq!(amz().sort_by_name("Z-A").unwrap(), strs(&["z", "m", "a"]));
}

#[test]
fn invalid_sort_function_name() {
    assert!(Sortable::new().sort_by_name("INVALID SORT STRING").is_err());
}

#[test]
fn sorting_setters_asc_no_param() {
    let mut s = Sortable::new();
    s.is_sort_ascending = false;
    s.set_sort_ascending(true);
    assert!(s.is_sort_ascending);
}

#[test]
fn sorting_setters_desc_no_param() {
    let mut s = Sortable::new();
    s.is_sort_ascending = true;
    s.set_sort_descending(true);
    assert!(!s.is_sort_ascending);
}

#[test]
fn sorting_setters_asc_true() {
    let mut s = Sortable::new();
    s.is_sort_ascending = false;
    s.set_sort_ascending(true);
    assert!(s.is_sort_ascending);
}

#[test]
fn sorting_setters_asc_false() {
    let mut s = Sortable::new();
    s.is_sort_ascending = true;
    s.set_sort_ascending(false);
    assert!(!s.is_sort_ascending);
}

#[test]
fn sorting_setters_desc_true() {
    let mut s = Sortable::new();
    s.is_sort_ascending = true;
    s.set_sort_descending(true);
    assert!(!s.is_sort_ascending);
}

#[test]
fn sorting_setters_desc_false() {
    let mut s = Sortable::new();
    s.is_sort_ascending = false;
    s.set_sort_descending(false);
    assert!(s.is_sort_ascending);
}
