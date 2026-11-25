//! Ported from upstream `test/ProxyWrapTest.js`. The port merges eagerly instead of
//! returning a JS `Proxy`, so lookups are checked on the merged value.

use twelvety::Value;
use twelvety::plugins::pagination::try_proxy_wrap;
use twelvety::testing::json;

#[test]
fn basic_wrap() {
    let test = try_proxy_wrap(&Value::object(), &json(r#"{"a": 1}"#)).unwrap();
    assert_eq!(test.get("a"), Value::from(1));
}

#[test]
fn nested_wrap() {
    let test = try_proxy_wrap(&Value::object(), &json(r#"{"child": {"a": 1, "b": 1}}"#)).unwrap();
    assert!(test.get("child").is_truthy());
    assert_eq!(test.get("child"), json(r#"{"a": 1, "b": 1}"#));
    assert_eq!(test.get("child").get("a"), Value::from(1));
    assert_eq!(test.get("child").get("b"), Value::from(1));
}

#[test]
fn double_nested_wrap() {
    let child = json(r#"{"a": 1, "b": 1, "c": {"grandchild": 1}}"#);
    let mut fallback = Value::object();
    fallback.set("child", child.clone());
    let test = try_proxy_wrap(&Value::object(), &fallback).unwrap();
    assert!(test.get("child").is_truthy());
    assert_eq!(test.get("child"), child);
    assert_eq!(test.get("child").get("a"), Value::from(1));
    assert_eq!(test.get("child").get("b"), Value::from(1));
    assert_eq!(test.get_path("child.c.grandchild"), Value::from(1));
}

#[test]
fn array() {
    let test = try_proxy_wrap(&Value::object(), &json(r#"{"child": [1, 2, 3]}"#)).unwrap();
    assert_eq!(test.get("child"), json("[1, 2, 3]"));
    assert_eq!(test.get("child").get_index(1), Value::from(2));
}

#[test]
fn array_nested() {
    let test = try_proxy_wrap(&Value::object(), &json(r#"{"child": [1, [2], 3]}"#)).unwrap();
    assert_eq!(test.get("child"), json("[1, [2], 3]"));
    assert_eq!(test.get("child").get_index(1), json("[2]"));
    assert_eq!(test.get("child").get_index(1).get_index(0), Value::from(2));
}

#[test]
fn fails_for_invalid_target() {
    assert!(try_proxy_wrap(&Value::Bool(true), &Value::object()).is_err());
}

#[test]
fn fails_for_invalid_fallback() {
    assert!(try_proxy_wrap(&Value::object(), &Value::Bool(true)).is_err());
}

#[test]
fn frozen_object() {
    // `Object.freeze` has no equivalent; the lookup through the fallback must still work
    let test = try_proxy_wrap(&Value::object(), &json(r#"{"twelvety": {"generator": "Twelvety v3.0.0"}}"#)).unwrap();
    assert_eq!(test.get_path("twelvety.generator"), Value::str("Twelvety v3.0.0"));
}

#[test]
fn frozen_nested_object() {
    let test = try_proxy_wrap(
        &json(r#"{"twelvety": {}}"#),
        &json(r#"{"twelvety": {"generator": "Twelvety v3.0.0"}}"#),
    )
    .unwrap();
    assert_eq!(test.get_path("twelvety.generator"), Value::str("Twelvety v3.0.0"));
}
