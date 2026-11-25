//! Port of upstream `test/TemplateTest-ComputedData.js`.
//!
//! JavaScript front matter (and the JS data files of the `stubs-computed-global` fixture) are not
//! supported by the port: those tests pass the same data (functions included) as virtual template
//! data or global data instead.

mod template_support;

use std::sync::Arc;

use template_support::*;
use twelvety::template::Template;
use twelvety::testing::{TestOptions, get_new_template, json};
use twelvety::{Twelvety, Options, Value};

fn computed_template(path: &str, input: &str, output: &str, content: &str) -> Arc<Template> {
    virtual_template(path, input, output, &[(&path[input.len()..], content)])
}

fn data_template(path: &str, input: &str, output: &str, content: &str, data: Value) -> Arc<Template> {
    let key = path[input.len()..].to_string();
    get_new_template(
        path,
        virtual_options(input, output, vec![(key, content.to_string(), data)], |_| Ok(())),
    )
    .unwrap()
}

fn function<F>(f: F) -> Value
where
    F: Fn(&Value) -> String + Send + Sync + 'static,
{
    Value::function(move |args| Ok(Value::str(f(&args[0]))))
}

fn object(entries: Vec<(&str, Value)>) -> Value {
    let mut o = Value::object();
    for (k, v) in entries {
        o.set(k, v);
    }
    o
}

/// `renderTemplate(tmpl, data)` for the first page
fn render_page(tmpl: &Arc<Template>) -> String {
    render(tmpl).unwrap().trim().to_string()
}

#[test]
fn twelvety_computed() {
    let tmpl = computed_template(
        "./test/stubs/twelvetyComputed/first.njk",
        "./test/stubs/",
        "./dist",
        "---\nkey1: value1\ntwelvetyComputed:\n  key2: value2-{{ key1 }}.css\n---\nhi:{{ key2 }}\n",
    );
    assert_eq!(render_page(&tmpl), "hi:value2-value1.css");
}

#[test]
fn twelvety_computed_overrides_existing_value() {
    let tmpl = computed_template(
        "./test/stubs/twelvetyComputed/override.njk",
        "./test/stubs/",
        "./dist",
        "---\nkey1: value1\ntwelvetyComputed:\n  key1: override\n---\nhi:{{ key1 }}\n",
    );
    assert_eq!(rendered_data(&tmpl, 0).get("key1").as_str(), Some("override"));
    assert_eq!(render_page(&tmpl), "hi:override");
}

#[test]
fn twelvety_computed_overrides_existing_value_and_reuses_upstream_value() {
    let tmpl = computed_template(
        "./test/stubs/twelvetyComputed/override-reuse.njk",
        "./test/stubs/",
        "./dist",
        "---\nkey1: value1\ntwelvetyComputed:\n  key1: \"over({{key1}})ride\"\n---\nhi:{{ key1 }}\n",
    );
    assert_eq!(rendered_data(&tmpl, 0).get("key1").as_str(), Some("over(value1)ride"));
    assert_eq!(render_page(&tmpl), "hi:over(value1)ride");
}

#[test]
fn twelvety_computed_permalink() {
    let tmpl = computed_template(
        "./test/stubs/twelvetyComputed/permalink.njk",
        "./test/stubs/",
        "./dist",
        "---\nkey1: value1\ntwelvetyComputed:\n  permalink: \"haha-{{key2}}.html\"\n  key2: \"{{key1}}\"\n  dependsOnPage: \"depends:{{page.url}}\"\n  nested:\n    key3: \"{{key1}}\"\n    key4: \"depends on computed {{key2}}\"\n---\nhi:{{ key2 }}\n",
    );
    let data = rendered_data(&tmpl, 0);
    assert_eq!(data.get_path("page.url").as_str(), Some("/haha-value1.html"));
    assert_eq!(data.get_path("page.outputPath").as_str(), Some("./dist/haha-value1.html"));
    assert_eq!(data.get("permalink").as_str(), Some("haha-value1.html"));
    assert_eq!(data.get_path("nested.key3").as_str(), Some("value1"));
    assert_eq!(data.get_path("nested.key4").as_str(), Some("depends on computed value1"));
    assert_eq!(data.get("dependsOnPage").as_str(), Some("depends:/haha-value1.html"));
}

#[test]
fn twelvety_computed_simple_permalink() {
    let tmpl = computed_template(
        "./test/stubs/twelvetyComputed/permalink-simple.njk",
        "./test/stubs/",
        "./dist",
        "---\nkey1: value1\ntwelvetyComputed:\n  permalink: \"haha-{{key1}}.html\"\n---\nhi:{{ key2 }}",
    );
    let data = rendered_data(&tmpl, 0);
    assert_eq!(data.get_path("page.url").as_str(), Some("/haha-value1.html"));
    assert_eq!(data.get_path("page.outputPath").as_str(), Some("./dist/haha-value1.html"));
    assert_eq!(data.get("permalink").as_str(), Some("haha-value1.html"));
}

#[test]
fn twelvety_computed_permalink_using_slug() {
    let tmpl = computed_template(
        "./test/stubs/twelvetyComputed/permalink-slug.njk",
        "./test/stubs/",
        "./dist",
        "---\nkey1: \"This is a string\"\ntwelvetyComputed:\n  permalink: \"haha-{{key1 | slugify}}.html\"\n---",
    );
    let data = rendered_data(&tmpl, 0);
    assert_eq!(data.get_path("page.url").as_str(), Some("/haha-this-is-a-string.html"));
    assert_eq!(data.get_path("page.outputPath").as_str(), Some("./dist/haha-this-is-a-string.html"));
    assert_eq!(data.get("permalink").as_str(), Some("haha-this-is-a-string.html"));
}

#[test]
fn twelvety_computed_js_front_matter_function() {
    let data = object(vec![
        ("key1", Value::str("value1")),
        (
            "twelvetyComputed",
            object(vec![
                ("key3", function(|d| format!("value3-{}", d.get("key2").to_js_string()))),
                ("key2", function(|d| format!("value2-{}.css", d.get("key1").to_js_string()))),
            ]),
        ),
    ]);
    let tmpl = data_template("./test/stubs/twelvetyComputed/second.njk", "./test/stubs/", "./dist", "hi:{{ key2 }}\n", data);
    assert_eq!(rendered_data(&tmpl, 0).get("key3").as_str(), Some("value3-value2-value1.css"));
    assert_eq!(render_page(&tmpl), "hi:value2-value1.css");
}

#[test]
fn twelvety_computed_js_front_matter_key_reuses_and_overrides() {
    let data = object(vec![
        ("key1", Value::str("value1")),
        (
            "twelvetyComputed",
            object(vec![("key1", function(|d| format!("value2-{}", d.get("key1").to_js_string())))]),
        ),
    ]);
    let tmpl = data_template("./test/stubs/twelvetyComputed/third.njk", "./test/stubs/", "./dist", "hi:{{ key1 }}\n", data);
    assert_eq!(rendered_data(&tmpl, 0).get("key1").as_str(), Some("value2-value1"));
    assert_eq!(render_page(&tmpl), "hi:value2-value1");
}

#[test]
fn twelvety_computed_true_primitive() {
    let tmpl = computed_template(
        "./test/stubs/twelvetyComputed/true.njk",
        "./test/stubs/",
        "./dist",
        "---\nkey1: value1\ntwelvetyComputed:\n  key2: true\n  key3: false\n  key4: 324\n---\n",
    );
    let data = rendered_data(&tmpl, 0);
    assert_eq!(data.get("key1").as_str(), Some("value1"));
    assert_eq!(data.get("key2"), Value::Bool(true));
    assert_eq!(data.get("key3"), Value::Bool(false));
    assert_eq!(data.get("key4").as_f64(), Some(324.0));
}

#[test]
fn twelvety_computed_relies_on_global_data() {
    let data = object(vec![(
        "twelvetyComputed",
        object(vec![("image", function(|d| d.get_path("globalData.datakey1").to_js_string()))]),
    )]);
    let tmpl = data_template(
        "./test/stubs/twelvetyComputed/use-global-data.njk",
        "./test/stubs/",
        "./dist",
        "Issue #1043",
        data,
    );
    assert_eq!(rendered_data(&tmpl, 0).get("image").as_str(), Some("datavalue1"));
}

#[test]
fn twelvety_computed_intermixes_with_global_data() {
    let data = object(vec![(
        "twelvetyComputed",
        object(vec![
            ("image", function(|_| "first".into())),
            ("image2", function(|_| "second".into())),
        ]),
    )]);
    let options = virtual_options(
        "test/stubs-computed-global",
        "dist",
        vec![("intermix.njk".into(), "Issue #1043".into(), data)],
        |cfg| {
            // stands in for `_data/twelvetyComputed.cjs`
            cfg.add_global_data(
                "twelvetyComputed",
                object(vec![
                    (
                        "twelvetyNavigation",
                        object(vec![("key", function(|_| "nested-first-global".into()))]),
                    ),
                    ("image2", function(|_| "second-global".into())),
                    ("image3", function(|_| "third-global".into())),
                ]),
            );
            Ok(())
        },
    );
    let tmpl = get_new_template("./test/stubs-computed-global/intermix.njk", options).unwrap();
    let fetched = tmpl.get_data().unwrap();
    let computed = fetched.get("buildawesomeComputed");
    assert!(computed.get("image").is_truthy());
    assert!(computed.get("image2").is_truthy());
    assert!(computed.get("image3").is_truthy());
    assert!(computed.get_path("twelvetyNavigation.key").is_truthy());

    let data = rendered_data(&tmpl, 0);
    assert_eq!(data.get("image").as_str(), Some("first"));
    assert_eq!(data.get("image2").as_str(), Some("second"));
    assert_eq!(data.get("image3").as_str(), Some("third-global"));
    assert_eq!(data.get_path("twelvetyNavigation.key").as_str(), Some("nested-first-global"));
}

fn fail_filter(_ctx: &twelvety::user_config::CallContext, input: Value, _args: &[Value]) -> anyhow::Result<Value> {
    // Filter expects a certain String format
    match input.as_str() {
        Some(s) if s.chars().count() == 1 => Ok(input),
        _ => anyhow::bail!("Expect a one character string: {}", input.to_js_string()),
    }
}

fn symbol_parsing(file: &'static str, liquid: bool) {
    let options = TestOptions::new("test/stubs-virtual-nowrite", "dist").with_config(move |cfg| {
        cfg.add_template(
            file,
            "---\ntwelvetyComputed:\n  c: \"{{ a | fail }}{{ b | fail }}\"\n  a: \"a\"\n  b: \"b\"\n---",
            Value::object(),
        )?;
        if liquid {
            cfg.add_liquid_filter("fail", fail_filter);
        } else {
            cfg.add_nunjucks_filter("fail", fail_filter);
        }
        Ok(())
    });
    let tmpl = get_new_template(&format!("./test/stubs-virtual-nowrite/{file}"), options).unwrap();
    let data = rendered_data(&tmpl, 0);
    assert_eq!(data.get("a").as_str(), Some("a"));
    assert_eq!(data.get("b").as_str(), Some("b"));
    assert_eq!(data.get("c").as_str(), Some("ab"));
}

#[test]
fn twelvety_computed_using_symbol_parsing_on_template_strings_nunjucks() {
    symbol_parsing("test.njk", false);
}

#[test]
fn twelvety_computed_using_symbol_parsing_on_template_strings_liquid() {
    symbol_parsing("test.liquid", true);
}

#[test]
fn twelvety_computed_render_strings_in_arrays() {
    let tmpl = computed_template(
        "./test/stubs-virtual-nowrite/test.liquid",
        "./test/stubs-virtual-nowrite/",
        "./test/stubs-virtual-nowrite/_site",
        "---\ndynamicValue: \"test\"\ntwelvetyComputed:\n  notArray: \"{{ dynamicValue }}\"\n  array:\n    - \"static value\"\n    - \"{{ dynamicValue }}\"\n---",
    );
    let data = rendered_data(&tmpl, 0);
    assert_eq!(data.get("array"), json(r#"["static value", "test"]"#));
    assert_eq!(data.get("notArray").as_str(), Some("test"));
}

/// Build `./test/stubs-virtual/` with virtual templates (path, content, data) and return
/// `(inputPath, content)` pairs sorted like `sortTwelvetyResults`
fn to_json(templates: Vec<(&'static str, &'static str, Value)>) -> Vec<(String, String)> {
    let templates: Vec<(&'static str, &'static str, Value)> = templates;
    let options = Options {
        config_path: Some(String::new()),
        ..Default::default()
    }
    .with_config(move |cfg| {
        cfg.set_use_template_cache(false);
        for (path, content, data) in &templates {
            cfg.add_template(path, content, data.clone())?;
        }
        Ok(())
    });
    let mut elev = Twelvety::new(Some("./test/stubs-virtual/"), None, options);
    elev.disable_logger();
    let mut out: Vec<(String, String)> = elev
        .to_json()
        .unwrap()
        .into_iter()
        .map(|r| (r.input_path, r.content))
        .collect();
    out.sort();
    out
}

#[test]
fn issue_3728_computed_data_arrays_of_different_sizes_are_a_single_unit() {
    let result = to_json(vec![(
        "index.njk",
        "{{ arr }}",
        json(r#"{"arr": [1, 2, 3, 4, 5], "twelvetyComputed": {"arr": ["a", "b", "c"]}}"#),
    )]);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].1, "a,b,c");
}

#[test]
fn issue_3827_computed_data_with_arrays_and_layouts_two_twelvety_computed() {
    let result = to_json(vec![
        (
            "_includes/base.njk",
            "{{ content | safe }}",
            json(r#"{"twelvetyComputed": {"arr": ["base", "base2"]}}"#),
        ),
        ("page1.njk", "{{ arr }}", json(r#"{"layout": "base.njk"}"#)),
        (
            "page2.njk",
            "{{ arr }}",
            json(r#"{"layout": "base.njk", "twelvetyComputed": {"arr": ["override", "override2"]}}"#),
        ),
    ]);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].0, "./test/stubs-virtual/page1.njk");
    assert_eq!(result[0].1, "base,base2");
    assert_eq!(result[1].0, "./test/stubs-virtual/page2.njk");
    // A merge happened here because it was twelvetyComputed -> twelvetyComputed array merge before computed data
    assert_eq!(result[1].1, "base,base2,override,override2");
}

#[test]
fn issue_3728_computed_data_with_arrays_and_layouts_one_twelvety_computed() {
    let result = to_json(vec![
        ("_includes/base.njk", "{{ content | safe }}", json(r#"{"arr": ["base", 1]}"#)),
        ("page1.njk", "{{ arr }}", json(r#"{"layout": "base.njk"}"#)),
        (
            "page2.njk",
            "{{ arr }}",
            json(r#"{"layout": "base.njk", "twelvetyComputed": {"arr": ["override", 2]}}"#),
        ),
    ]);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].1, "base,1");
    assert_eq!(result[1].1, "override,2");
}
