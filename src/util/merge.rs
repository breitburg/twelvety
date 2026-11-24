//! Port of `Merge` and `DeepCopy` from `@11ty/eleventy-utils`.
//!
//! Plain objects are merged deeply, arrays are concatenated and the `override:`
//! key prefix replaces the target value instead of merging it.

use crate::value::{Map, Value};

const OVERRIDE_PREFIX: &str = "override:";
const UNSAFE_KEY: &str = "__proto__";

fn clean_key<'a>(key: &'a str, prefix: Option<&str>) -> &'a str {
    if let Some(prefix) = prefix {
        if let Some(stripped) = key.strip_prefix(prefix) {
            return stripped;
        }
    }
    key
}

fn get_merged_item(target: Value, source: &Value, override_prefix: Option<&str>) -> Value {
    let source_is_object = source.is_plain_object();
    let mut target = target;
    let mut target_shortcut = false;
    if !target.is_truthy() && source_is_object {
        target_shortcut = true;
        target = Value::object();
    }

    if let (Value::Array(t), Value::Array(s)) = (&target, source) {
        let mut out = t.as_ref().clone();
        out.extend(s.iter().cloned());
        return Value::array(out);
    }

    if target_shortcut || target.is_plain_object() {
        if let Value::Object(source_map) = source {
            let map = target.as_object_mut();
            for (key, value) in source_map.iter() {
                let override_key = clean_key(key, override_prefix);
                if override_key == UNSAFE_KEY {
                    continue;
                }
                let target_value = map.get(key).cloned().unwrap_or_default();
                let merged = get_merged_item(target_value, value, override_prefix);
                map.insert(override_key.to_string(), merged);
            }
        }
        return target;
    }
    source.clone()
}

/// `DeepCopy(target, ...sources)`
pub fn deep_copy(target: Value, sources: &[&Value]) -> Value {
    let mut target = target;
    for source in sources {
        if !source.is_truthy() {
            continue;
        }
        target = get_merged_item(target, source, None);
    }
    target
}

/// `Merge(target, ...sources)`
pub fn merge(target: Value, sources: &[&Value]) -> Value {
    let mut target = target;
    if let Value::Object(map) = &target {
        if map.keys().any(|k| k.starts_with(OVERRIDE_PREFIX)) {
            let mut cleaned = Map::new();
            for (key, value) in map.iter() {
                if let Some(stripped) = key.strip_prefix(OVERRIDE_PREFIX) {
                    if stripped != UNSAFE_KEY {
                        cleaned.insert(stripped.to_string(), value.clone());
                    }
                } else {
                    cleaned.insert(key.clone(), value.clone());
                }
            }
            target = Value::from_map(cleaned);
        }
    }
    for source in sources {
        if !source.is_truthy() {
            continue;
        }
        target = get_merged_item(target, source, Some(OVERRIDE_PREFIX));
    }
    target
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(s: &str) -> Value {
        Value::from_json(serde_json::from_str(s).unwrap())
    }

    #[test]
    fn merges_deeply() {
        let merged = merge(
            Value::object(),
            &[&json(r#"{"a":{"b":1},"tags":["x"]}"#), &json(r#"{"a":{"c":2},"tags":["y"]}"#)],
        );
        assert_eq!(merged, json(r#"{"a":{"b":1,"c":2},"tags":["x","y"]}"#));
    }

    #[test]
    fn override_prefix() {
        let merged = merge(
            Value::object(),
            &[&json(r#"{"tags":["x"]}"#), &json(r#"{"override:tags":["y"]}"#)],
        );
        assert_eq!(merged, json(r#"{"tags":["y"]}"#));
    }
}
