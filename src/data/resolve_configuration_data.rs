//! Port of `Data/ResolveConfigurationData.js`: `buildawesome*` keys with `twelvety*` aliases.

use crate::util::merge::deep_copy;
use crate::value::Value;

pub const EXCLUDE_FROM_COLLECTIONS_KEY: &str = "buildawesomeExcludeFromCollections";

const ELIGIBLE_PROPS: &[&str] = &[
    "buildawesomeComputed",
    "buildawesomeDataSchema",
    EXCLUDE_FROM_COLLECTIONS_KEY,
    "buildawesomeImport",
];

const BUILDAWESOME_PREFIX: &str = "buildawesome";
const TWELVETY_PREFIX: &str = "twelvety";

pub fn get_alternate_property_name(prop: &str) -> Option<String> {
    if ELIGIBLE_PROPS.contains(&prop) {
        return Some(format!("{TWELVETY_PREFIX}{}", &prop[BUILDAWESOME_PREFIX.len()..]));
    }
    None
}

pub fn get_eligible_locations(original: &str) -> Vec<String> {
    let mut out = vec![original.to_string()];
    if let Some(alt) = get_alternate_property_name(original) {
        if alt != original {
            out.push(alt);
        }
    }
    out
}

/// Rename `twelvety*` keys to their `buildawesome*` equivalents
pub fn normalize(data: Value) -> Value {
    if !data.is_plain_object() {
        return data;
    }
    let mut normalized: Option<Value> = None;
    for prop in ELIGIBLE_PROPS {
        let alt = get_alternate_property_name(prop).unwrap();
        if !data.has(&alt) {
            continue;
        }
        let value = get_value(&data, prop);
        let target = normalized.get_or_insert_with(|| data.clone());
        target.set(*prop, value);
        target.remove(&alt);
    }
    normalized.unwrap_or(data)
}

/// Read a (possibly aliased) configuration value, merging when both are present
pub fn get_value(data: &Value, location: &str) -> Value {
    let mut values = vec![];
    for loc in get_eligible_locations(location) {
        let v = data.get_path(&loc);
        if !v.is_undefined() {
            values.push(v);
        }
    }
    if values.len() < 2 {
        return values.into_iter().next().unwrap_or_default();
    }
    let mut merging: Option<Value> = None;
    for v in values {
        if v.is_plain_object() {
            merging = Some(deep_copy(merging.unwrap_or_else(Value::object), &[&v]));
        } else if v.is_array() {
            merging = Some(deep_copy(merging.unwrap_or_else(|| Value::array(vec![])), &[&v]));
        } else if merging.is_none() {
            return v;
        }
    }
    merging.unwrap_or_default()
}
