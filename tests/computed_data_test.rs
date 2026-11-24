//! Port of upstream `test/ComputedDataTest.js`.
//!
//! Template string entries (`addTemplateString`) are `ComputedValue::Template` values whose
//! rendering and symbol parsing are supplied by a `ComputedEvaluator`. The test evaluator below
//! renders `{"render": fn, "symbols": [...]}` objects; upstream’s “placeholder rendering” symbol
//! discovery (no `symbolParseFn`) is engine specific, so those tests declare their symbols.
//!
//! Skipped:
//! - "use a computed value in another computed (out of order), async callbacks": async functions
//!   are synchronous closures in the port (covered by the non-async variant).
//! - "Use JavaScript functions (filters) in computed data functions": JS `this` context.
//! - "Enumerating data doesn’t create false cycles": `test.skip` upstream.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::Result;
use twelvety::Value;
use twelvety::data::computed_data::{ComputedData, ComputedDataBuilder, ComputedEvaluator, ComputedValue};
use twelvety::testing::json;

struct Evaluator;

impl ComputedEvaluator for Evaluator {
    fn evaluate(&self, _key: &str, value: &ComputedValue, data: &Value) -> Result<Value> {
        match value {
            ComputedValue::Template(t) => t.get("render").call(&[data.clone()]),
            ComputedValue::Function(f) => f(&[data.clone()]),
            ComputedValue::Static(v) => Ok(v.clone()),
            _ => Ok(Value::Undefined),
        }
    }

    fn symbols(&self, value: &ComputedValue, _data: &Value) -> Vec<String> {
        match value {
            ComputedValue::Template(t) => t.get("symbols").to_vec().iter().map(|v| v.to_js_string()).collect(),
            _ => vec![],
        }
    }
}

fn f<F>(func: F) -> ComputedValue
where
    F: Fn(&Value) -> Value + Send + Sync + 'static,
{
    ComputedValue::Function(Arc::new(move |args: &[Value]| Ok(func(&args[0]))))
}

/// `addTemplateString(key, fn, [], () => symbols)`
fn template_string<F>(func: F, symbols: &[&str]) -> ComputedValue
where
    F: Fn(&Value) -> Value + Send + Sync + 'static,
{
    let mut t = Value::object();
    t.set("render", Value::function(move |args| Ok(func(&args[0]))));
    t.set("symbols", Value::array(symbols.iter().map(|s| Value::str(*s)).collect()));
    ComputedValue::Template(t)
}

fn s(v: impl Into<String>) -> Value {
    Value::str(v.into())
}

fn js(v: &Value) -> String {
    v.to_js_string()
}

struct Cd(ComputedDataBuilder);

impl Cd {
    fn new() -> Self {
        Cd(ComputedDataBuilder::new())
    }
    fn add(mut self, key: &str, value: ComputedValue) -> Self {
        self.0.add(key, value, vec![]);
        self
    }
    fn add_str(self, key: &str, value: &str) -> Self {
        self.add(key, ComputedValue::Static(s(value)))
    }
    fn build(self) -> ComputedData {
        self.0.build()
    }
}

fn unavailable(paths: &[&str]) -> Vec<String> {
    paths.iter().map(|s| s.to_string()).collect()
}

fn setup(cd: &mut ComputedData, data: &mut Value) -> Result<()> {
    cd.setup_data(data, &[], None, &Evaluator)
}

fn setup_unavailable(cd: &mut ComputedData, data: &mut Value, paths: &[&str]) {
    cd.setup_data(data, &unavailable(paths), None, &Evaluator).unwrap();
}

#[test]
fn basic_get_set() {
    for reverse in [false, true] {
        let key1 = f(|d| s(format!("this is a test {}{}", js(&d.get("key2")), js(&d.get("keystr")))));
        let cd = if reverse {
            Cd::new().add("key1", key1).add_str("keystr", "this is a str")
        } else {
            Cd::new().add_str("keystr", "this is a str").add("key1", key1)
        };
        let mut cd = cd.build();
        let mut data = json(r#"{"key2": "inject me"}"#);
        setup(&mut cd, &mut data).unwrap();
        assert_eq!(js(&data.get("key1")), "this is a test inject methis is a str");
        assert_eq!(js(&data.get("key2")), "inject me");
        assert_eq!(js(&data.get("keystr")), "this is a str");
    }
}

#[test]
fn basic_get_set_reverse_order_nested_two_deep() {
    let mut cd = Cd::new()
        .add("key1.key3", f(|d| s(format!("this is a test {}{}", js(&d.get("key2")), js(&d.get("keystr"))))))
        .add("key1.key4", f(|d| s(format!("this is a test {}", js(&d.get_path("key1.key3"))))))
        .add_str("keystr", "this is a str")
        .build();
    let mut data = json(r#"{"key2": "inject me"}"#);
    setup(&mut cd, &mut data).unwrap();
    assert_eq!(js(&data.get_path("key1.key3")), "this is a test inject methis is a str");
    assert_eq!(
        js(&data.get_path("key1.key4")),
        "this is a test this is a test inject methis is a str"
    );
    assert_eq!(js(&data.get("key2")), "inject me");
    assert_eq!(js(&data.get("keystr")), "this is a str");
}

#[test]
fn use_a_computed_value_in_another_computed() {
    for out_of_order in [false, true] {
        let computed = f(|d| s(format!("this is a test {}", js(&d.get("keyOriginal")))));
        let second = f(|d| s(format!("using computed {}", js(&d.get("keyComputed")))));
        let cd = if out_of_order {
            Cd::new().add("keyComputed2nd", second).add("keyComputed", computed)
        } else {
            Cd::new().add("keyComputed", computed).add("keyComputed2nd", second)
        };
        let mut cd = cd.build();
        let mut data = json(r#"{"keyOriginal": "inject me"}"#);
        setup(&mut cd, &mut data).unwrap();
        assert_eq!(js(&data.get("keyComputed2nd")), "using computed this is a test inject me");
    }
}

#[test]
fn basic_get_set_nested() {
    let mut cd = Cd::new()
        .add("key1.nested", f(|d| s(js(&d.get("key2")))))
        .add("key2", f(|_| s("hi")))
        .build();
    let mut data = json(r#"{"key2": "inject me"}"#);
    setup(&mut cd, &mut data).unwrap();
    assert_eq!(data.get("key1"), json(r#"{"nested": "hi"}"#));
    assert_eq!(js(&data.get("key2")), "hi");
}

#[test]
fn basic_get_set_nested_deeper() {
    let mut cd = Cd::new()
        .add("key1.nested.deeperA", f(|d| s(js(&d.get("key2")))))
        .add("key1.nested.deeperB", f(|d| s(js(&d.get("key2")))))
        .add("key1.nested.deeperC.wow", f(|d| s(js(&d.get("key2")))))
        .add("key2", f(|_| s("hi")))
        .build();
    let mut data = json(r#"{"key1": {"nonComputed": "hi"}, "key2": "inject me"}"#);
    setup(&mut cd, &mut data).unwrap();
    assert_eq!(
        data.get("key1"),
        json(r#"{"nonComputed": "hi", "nested": {"deeperA": "hi", "deeperB": "hi", "deeperC": {"wow": "hi"}}}"#)
    );
    assert_eq!(js(&data.get("key2")), "hi");
}

#[test]
fn template_string_versus_function_types() {
    let mut cd = Cd::new()
        .add("key1.nested.deeperA", f(|d| s(js(&d.get("key2")))))
        .add("key2", f(|_| s("hi")))
        .build();
    let mut data = json(r#"{"key1": {"nonComputed": "hi"}, "key2": "inject me"}"#);
    setup(&mut cd, &mut data).unwrap();
    assert_eq!(data.get("key1"), json(r#"{"nonComputed": "hi", "nested": {"deeperA": "hi"}}"#));
}

#[test]
fn basic_get_set_with_template_string() {
    let mut cd = Cd::new()
        .add("keystr", template_string(|_| s("this is a str"), &[]))
        .add(
            "key1",
            template_string(
                |d| s(format!("this is a test {}{}", js(&d.get("key2")), js(&d.get("keystr")))),
                &["key2", "keystr"],
            ),
        )
        .build();
    let mut data = json(r#"{"key2": "inject me"}"#);
    setup(&mut cd, &mut data).unwrap();
    assert_eq!(js(&data.get("key1")), "this is a test inject methis is a str");
    assert_eq!(js(&data.get("key2")), "inject me");
    assert_eq!(js(&data.get("keystr")), "this is a str");
}

#[test]
fn basic_get_set_using_array_data() {
    let mut cd = Cd::new()
        .add_str("keystr", "this is a str")
        .add(
            "key1",
            f(|d| {
                assert!(d.get("arr").is_array());
                s(format!("this is a test {}{}", js(&d.get("arr").get_index(0)), js(&d.get("keystr"))))
            }),
        )
        .build();
    let mut data = json(r#"{"arr": ["inject me"], "collections": {"first": [], "second": []}}"#);
    setup(&mut cd, &mut data).unwrap();
    assert_eq!(js(&data.get("key1")), "this is a test inject methis is a str");
    assert_eq!(js(&data.get("arr").get_index(0)), "inject me");
    assert_eq!(js(&data.get("keystr")), "this is a str");
}

#[test]
fn computed_returns_deep_object() {
    let mut cd = Cd::new()
        .add("returnobj", f(|_| json(r#"{"key1": "value1", "nest": {"key2": "value2"}}"#)))
        .build();
    let mut data = json(r#"{"returnobj": {"key1": "bad1", "nest": {"key2": "bad2"}}}"#);
    setup(&mut cd, &mut data).unwrap();
    assert_eq!(js(&data.get_path("returnobj.key1")), "value1");
    assert_eq!(js(&data.get_path("returnobj.nest.key2")), "value2");
}

#[test]
fn boolean_computed_value_issue_1114() {
    let mut cd = Cd::new().add("bool1", ComputedValue::Static(Value::Bool(true))).build();
    let mut data = json(r#"{"key2": "inject me"}"#);
    setup(&mut cd, &mut data).unwrap();
    assert_eq!(data.get("bool1"), Value::Bool(true));
    assert_eq!(js(&data.get("key2")), "inject me");
}

#[test]
fn reading_an_unavailable_path_stops_the_function_and_defers_it() {
    let runs = Arc::new(AtomicUsize::new(0));
    let r = runs.clone();
    let mut cd = Cd::new()
        .add(
            "key1",
            f(move |d| {
                r.fetch_add(1, Ordering::SeqCst);
                d.get_path("collections.first.length")
            }),
        )
        .build();
    let mut data = json(r#"{"collections": {}}"#);
    setup_unavailable(&mut cd, &mut data, &["collections"]);
    assert_eq!(runs.load(Ordering::SeqCst), 1);
    assert!(!data.has("key1"));

    data.set_path("collections.first", json("[1, 2]"));
    cd.process_remaining_data(&mut data, &Evaluator).unwrap();
    assert_eq!(runs.load(Ordering::SeqCst), 2);
    assert_eq!(data.get("key1").as_f64(), Some(2.0));
}

#[test]
fn swallowed_reads_of_an_unavailable_path_still_defer() {
    let mut cd = Cd::new()
        .add(
            "key1",
            f(|d| match d.get_path("page.rawInput").as_str() {
                Some(raw) => Value::from(raw.len()),
                None => Value::from(-1),
            }),
        )
        .build();
    let mut data = json(r#"{"page": {}}"#);
    setup_unavailable(&mut cd, &mut data, &["page.rawInput"]);
    assert!(!data.has("key1"));
}

#[test]
fn enumerating_a_parent_object_reads_its_unavailable_children() {
    let mut cd = Cd::new().add("key1", f(|d| s(d.get("page").to_json_string()))).build();
    let mut data = json(r#"{"page": {"inputPath": "./in.md"}}"#);
    setup_unavailable(&mut cd, &mut data, &["page.rawInput"]);
    assert!(!data.has("key1"));

    data.set_path("page.rawInput", s("raw"));
    cd.process_remaining_data(&mut data, &Evaluator).unwrap();
    assert_eq!(js(&data.get("key1")), r#"{"inputPath":"./in.md","rawInput":"raw"}"#);
}

#[test]
fn template_strings_that_use_an_unavailable_path_are_deferred() {
    let mut cd = Cd::new()
        .add("key1", template_string(|d| s(js(&d.get_path("collections.posts"))), &["collections.posts"]))
        .build();
    let mut data = Value::object();
    setup_unavailable(&mut cd, &mut data, &["collections"]);
    assert!(!data.has("key1"));
}

#[test]
fn template_strings_that_use_an_unavailable_path_are_deferred_symbol_parsing() {
    let mut cd = Cd::new()
        .add("key1", template_string(|d| s(js(&d.get_path("page.rawInput"))), &["page.rawInput"]))
        .add("key2", template_string(|d| s(format!("{}!", js(&d.get("key1")))), &["key1"]))
        .build();
    let mut data = json(r#"{"page": {}}"#);
    setup_unavailable(&mut cd, &mut data, &["page.rawInput"]);
    assert!(!data.has("key1"));
    assert!(!data.has("key2"));

    data.set_path("page.rawInput", s("raw"));
    cd.process_remaining_data(&mut data, &Evaluator).unwrap();
    assert_eq!(js(&data.get("key2")), "raw!");
}

#[test]
fn inherits_resolved_signals_from_another_data_object() {
    let runs_a = Arc::new(AtomicUsize::new(0));
    let runs_b = Arc::new(AtomicUsize::new(0));
    let (a, b) = (runs_a.clone(), runs_b.clone());
    let mut parent = Cd::new()
        .add("a", f(move |_| Value::from(a.fetch_add(1, Ordering::SeqCst) + 1)))
        .add(
            "b",
            f(move |d| s(format!("{}:{}", js(&d.get("item")), b.fetch_add(1, Ordering::SeqCst) + 1))),
        )
        .build();
    let mut parent_data = Value::object();
    setup_unavailable(&mut parent, &mut parent_data, &["item"]);
    assert_eq!(parent_data.get("a").as_f64(), Some(1.0));
    assert!(!parent_data.has("b"));

    let mut child = parent.inherit();
    let mut child_data = parent_data.clone();
    child_data.set("item", s("x"));
    setup(&mut child, &mut child_data).unwrap();
    assert_eq!(child_data.get("a").as_f64(), Some(1.0));
    assert_eq!(runs_a.load(Ordering::SeqCst), 1);
    // Upstream the read of `item` throws before `++runs.b`; a Rust closure can’t be interrupted
    // mid-run, so the deferred (discarded) parent run is counted too.
    assert_eq!(js(&child_data.get("b")), "x:2");
}

#[test]
fn expect_collections_to_be_arrays_in_data_callback_1114() {
    let checks = Arc::new(AtomicUsize::new(0));
    let c = checks.clone();
    let mut cd = Cd::new()
        .add(
            "key1",
            f(move |d| {
                if d.get_path("collections.first.length").is_truthy() {
                    assert_eq!(d.get_path("collections.first.0").as_f64(), Some(1.0));
                    assert_eq!(d.get_path("collections.second.0").as_f64(), Some(2.0));
                    c.fetch_add(1, Ordering::SeqCst);
                }
                s("")
            }),
        )
        .build();
    let mut data = json(r#"{"collections": {"first": [1], "second": [2]}}"#);
    setup(&mut cd, &mut data).unwrap();
    assert_eq!(checks.load(Ordering::SeqCst), 1);
}

#[test]
fn tracks_dependencies() {
    let mut cd = Cd::new()
        .add("key1", f(|d| d.get_path("collections.all")))
        .add("key2", f(|d| s(format!("{}{}", js(&d.get("key1")), js(&d.get("key3"))))))
        .add("key3", f(|_| s("")))
        .build();
    let mut data = json(r#"{"collections": {"all": [1]}}"#);
    setup(&mut cd, &mut data).unwrap();
    assert!(cd.get_dependencies("key1").is_empty());
    assert_eq!(cd.get_dependencies("key2"), ["key1", "key3"]);
    assert!(cd.get_dependencies("key3").is_empty());
}

#[test]
fn process_in_two_stages_deferring_collections() {
    let mut cd = Cd::new()
        .add("page.url", f(|d| d.get("key2")))
        .add("page.outputPath", f(|d| d.get("key2")))
        .add("key0", f(|_| s("hi")))
        .add("key1", f(|d| d.get_path("collections.dog.0")))
        .add("key3", f(|d| Value::from(d.get("key1").to_number() + 1.0)))
        .add("collections.processed", f(|_| s("hi")))
        .build();
    let mut data = json(r#"{"key2": "/my-path/", "collections": {"dog": [2]}}"#);
    setup_unavailable(&mut cd, &mut data, &["collections"]);
    assert_eq!(
        data,
        json(r#"{"collections": {"dog": [2]}, "key0": "hi", "key2": "/my-path/", "page": {"url": "/my-path/", "outputPath": "/my-path/"}}"#)
    );

    cd.process_remaining_data(&mut data, &Evaluator).unwrap();
    assert_eq!(
        data,
        json(r#"{"collections": {"dog": [2], "processed": "hi"}, "key0": "hi", "key1": 2, "key2": "/my-path/", "key3": 3, "page": {"url": "/my-path/", "outputPath": "/my-path/"}}"#)
    );
}

fn assert_circular(mut cd: ComputedData) {
    let err = setup(&mut cd, &mut Value::object()).unwrap_err();
    assert_eq!(err.to_string(), "Circular computed data dependency: a -> b -> a");
}

#[test]
fn circular_dependencies_throw() {
    assert_circular(Cd::new().add("a", f(|d| d.get("b"))).add("b", f(|d| d.get("a"))).build());
}

#[test]
fn circular_dependencies_throw_template_strings() {
    assert_circular(
        Cd::new()
            .add("a", template_string(|d| s(js(&d.get("b"))), &["b"]))
            .add("b", template_string(|d| s(js(&d.get("a"))), &["a"]))
            .build(),
    );
}

#[test]
fn circular_dependencies_throw_function_and_template_string() {
    assert_circular(
        Cd::new()
            .add("a", f(|d| d.get("b")))
            .add("b", template_string(|d| s(js(&d.get("a"))), &["a"]))
            .build(),
    );
}

#[test]
fn template_string_reads_its_own_upstream_value() {
    let mut cd = Cd::new()
        .add("title", template_string(|d| s(format!("({})", js(&d.get("title")))), &["title"]))
        .build();
    let mut data = json(r#"{"title": "hi"}"#);
    setup(&mut cd, &mut data).unwrap();
    assert_eq!(js(&data.get("title")), "(hi)");
}

#[test]
fn reads_its_own_upstream_value() {
    let mut cd = Cd::new().add("title", f(|d| s(format!("({})", js(&d.get("title")))))).build();
    let mut data = json(r#"{"title": "hi"}"#);
    setup(&mut cd, &mut data).unwrap();
    assert_eq!(js(&data.get("title")), "(hi)");
}

#[test]
fn swallowed_reads_of_pending_computed_data_still_re_run() {
    let mut cd = Cd::new()
        .add(
            "a",
            f(|d| match d.get("b").as_str() {
                Some(b) => s(b.to_uppercase()),
                None => s("fallback"),
            }),
        )
        .add("b", f(|_| s("b")))
        .build();
    let mut data = Value::object();
    setup(&mut cd, &mut data).unwrap();
    assert_eq!(js(&data.get("a")), "B");
}

#[test]
fn reads_nested_computed_data_through_a_parent_object() {
    let mut cd = Cd::new()
        .add("summary", f(|d| s(d.get("page").to_json_string())))
        .add("page.url", f(|_| s("/url/")))
        .build();
    let mut data = json(r#"{"page": {"inputPath": "./in.md"}}"#);
    setup(&mut cd, &mut data).unwrap();
    assert_eq!(js(&data.get("summary")), r#"{"inputPath":"./in.md","url":"/url/"}"#);
}

#[test]
fn computed_functions_without_dependencies_run_once_issue_2771() {
    let calls = Arc::new(std::sync::Mutex::new(vec![]));
    let c = calls.clone();
    let mut cd = Cd::new()
        .add(
            "test",
            f(move |d| {
                c.lock().unwrap().push(js(&d.get_path("page.inputPath")));
                Value::Undefined
            }),
        )
        .build();
    let mut data = json(r#"{"page": {"inputPath": "./in.md"}}"#);
    setup_unavailable(&mut cd, &mut data, &["collections"]);
    cd.process_remaining_data(&mut data, &Evaluator).unwrap();
    assert_eq!(*calls.lock().unwrap(), ["./in.md"]);
}
