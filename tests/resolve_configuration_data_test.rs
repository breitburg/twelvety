//! Port of upstream `test/ResolveConfigurationDataTest.js`.

use twelvety::Value;
use twelvety::data::resolve_configuration_data::{get_alternate_property_name, get_value, normalize};
use twelvety::testing::json;

#[test]
fn alternate_location() {
    assert_eq!(get_alternate_property_name("permalink"), None);
    assert_eq!(get_alternate_property_name("eleventyComputed"), None);
    assert_eq!(get_alternate_property_name("buildawesomeComputed").as_deref(), Some("eleventyComputed"));
    assert_eq!(get_alternate_property_name("buildawesomeDataSchema").as_deref(), Some("eleventyDataSchema"));
    assert_eq!(
        get_alternate_property_name("buildawesomeExcludeFromCollections").as_deref(),
        Some("eleventyExcludeFromCollections")
    );
}

#[test]
fn permalink_empty_data() {
    assert!(get_value(&Value::object(), "permalink").is_undefined());
}

#[test]
fn permalink() {
    assert_eq!(get_value(&json(r#"{"permalink": "test"}"#), "permalink").as_str(), Some("test"));
}

#[test]
fn computed_data() {
    let data = json(r#"{"eleventyComputed": {"key": "value"}}"#);
    assert_eq!(get_value(&data, "eleventyComputed"), json(r#"{"key": "value"}"#));
}

#[test]
fn computed_data_alternate_location() {
    let data = json(r#"{"eleventyComputed": {"key": "value1"}}"#);
    assert_eq!(get_value(&data, "buildawesomeComputed"), json(r#"{"key": "value1"}"#));
}

#[test]
fn both_locations_exist_no_alternate() {
    let data = json(r#"{"eleventyComputed": {"key1": "value1"}, "buildawesomeComputed": {"key2": "value2"}}"#);
    assert_eq!(get_value(&data, "eleventyComputed"), json(r#"{"key1": "value1"}"#));
}

#[test]
fn both_locations_exist_no_merge_primitives() {
    let data = json(r#"{"eleventyComputed": 123, "buildawesomeComputed": 456}"#);
    assert_eq!(get_value(&data, "buildawesomeComputed").as_f64(), Some(456.0));
    let data = json(r#"{"eleventyComputed": "123", "buildawesomeComputed": "456"}"#);
    assert_eq!(get_value(&data, "buildawesomeComputed").as_str(), Some("456"));
    let data = json(r#"{"eleventyComputed": true, "buildawesomeComputed": false}"#);
    assert_eq!(get_value(&data, "buildawesomeComputed"), Value::Bool(false));
}

#[test]
fn both_locations_exist_merge_object() {
    let data = json(r#"{"eleventyComputed": {"key1": "value1"}, "buildawesomeComputed": {"key2": "value2"}}"#);
    assert_eq!(
        get_value(&data, "buildawesomeComputed"),
        json(r#"{"key1": "value1", "key2": "value2"}"#)
    );
}

#[test]
fn both_locations_exist_merge_array() {
    let data = json(r#"{"eleventyExcludeFromCollections": ["one"], "buildawesomeExcludeFromCollections": ["two"]}"#);
    assert_eq!(
        get_value(&data, "buildawesomeExcludeFromCollections"),
        json(r#"["two", "one"]"#)
    );
}

#[test]
fn normalize_folds_aliases_into_the_buildawesome_key() {
    let data = json(
        r#"{"eleventyComputed": {"key1": "value1"}, "buildawesomeComputed": {"key2": "value2"}, "eleventyExcludeFromCollections": true, "other": 1}"#,
    );
    assert_eq!(
        normalize(data.clone()),
        json(r#"{"buildawesomeComputed": {"key1": "value1", "key2": "value2"}, "buildawesomeExcludeFromCollections": true, "other": 1}"#)
    );
    assert!(data.has("eleventyComputed"));
}

#[test]
fn normalize_returns_the_same_object_without_aliases() {
    let data = json(r#"{"buildawesomeComputed": {}}"#);
    let Value::Object(before) = &data else { unreachable!() };
    let normalized = normalize(data.clone());
    let Value::Object(after) = &normalized else { unreachable!() };
    assert!(std::sync::Arc::ptr_eq(before, after));
}
