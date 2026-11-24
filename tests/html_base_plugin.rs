//! Port of upstream `test/HtmlBasePluginTest.js`.

mod common;

use twelvety::Value;
use twelvety::plugins::html_base::{self, apply_base_to_url};
use twelvety::plugins::resolve_builtin;
use twelvety::testing::json;
use twelvety::user_config::UserConfig;

use common::{content_for, options, to_json};

const BASE_TEMPLATE: &str = r#"---
permalink: /deep/
---
<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<meta name="description" content="">
<title></title>
<style>div { background-image: url(test.jpg); }</style>
<style>div { background-image: url({{ "/test.jpg" | htmlBaseUrl }}); }</style>
<link rel="stylesheet" href="/test.css">
<script src="/test.js"></script>
</head>
<body>
<a href="/">Home</a>
<a href="subdir/">Test</a>
<a href="./subdir/">Test</a>
<a href="../subdir/">Test</a>
</body>
</html>"#;

const BASE_TEMPLATE_2602: &str = r#"---
permalink: /deep/
---
<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<meta name="description" content="">
<title></title>
<style>div { background-image: url(test.jpg); }</style>
<style>div { background-image: url({{ "/test.jpg" | htmlBaseUrl }}); }</style>
<link rel="stylesheet" href="/test.css">
<script src="/test.js"></script>
</head>
<body>
<a href="/">Home</a>
<a href="subdir/">Test</a>
<a href="../subdir/">Test</a>
</body>
</html>"#;

const BASE_TEMPLATE_CASE_SENSITIVE: &str = r#"---
permalink: /deep/
---
<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<meta name="description" content="">
<title></title>
<style>div { background-image: url(test.jpg); }</style>
<style>div { background-image: url({{ "/test.jpg" | htmlBaseUrl }}); }</style>
<link rel="stylesheet" href="/test.css">
<script SrC="/test.js"></script>
</head>
<body>
<a hreF="/">Home</a>
<a HrEf="subdir/">Test</a>
<a href="../subdir/">Test</a>
</body>
</html>"#;

fn apply(url: &str, base: &str, path_prefix: Option<&str>, page_url: Option<&str>) -> String {
    apply_base_to_url(url, base, path_prefix, page_url).unwrap()
}

/// "Using the filter directly"
#[test]
fn using_the_filter_directly() {
    assert_eq!(apply("/", "/", None, None), "/");
    assert_eq!(apply("/test/", "/", None, None), "/test/");
    assert_eq!(apply("subdir/", "/", None, None), "subdir/");
    assert_eq!(apply("../subdir/", "/", None, None), "../subdir/");
    assert_eq!(apply("./subdir/", "/", None, None), "subdir/");
    assert_eq!(apply("http://example.com/", "/", None, None), "http://example.com/");
    assert_eq!(apply("http://example.com/test/", "/", None, None), "http://example.com/test/");
    assert_eq!(apply("/", "../", None, None), "/");
    assert_eq!(apply("/test/", "../", None, None), "/test/");
    assert_eq!(apply("subdir/", "../", None, None), "subdir/");
    assert_eq!(apply("../subdir/", "../", None, None), "../subdir/");
    assert_eq!(apply("./subdir/", "../", None, None), "subdir/");
    assert_eq!(apply("http://example.com/", "../", None, None), "http://example.com/");
    assert_eq!(apply("http://example.com/test/", "../", None, None), "http://example.com/test/");
    assert_eq!(apply("/", "/pathprefix/", None, None), "/pathprefix/");
    assert_eq!(apply("/test/", "/pathprefix/", None, None), "/pathprefix/test/");
    assert_eq!(apply("subdir/", "/pathprefix/", None, None), "subdir/");
    assert_eq!(apply("../subdir/", "/pathprefix/", None, None), "../subdir/");
    assert_eq!(apply("./subdir/", "/pathprefix/", None, None), "subdir/");
    assert_eq!(apply("#anchor", "/pathprefix/", None, None), "#anchor");
    assert_eq!(apply("/test/#anchor", "/pathprefix/", None, None), "/pathprefix/test/#anchor");
    assert_eq!(apply("/test/?param=value", "/pathprefix/", None, None), "/pathprefix/test/?param=value");
    assert_eq!(apply("http://url.com/", "/pathprefix/", None, None), "http://url.com/");
    assert_eq!(apply("http://url.com/test/", "/pathprefix/", None, None), "http://url.com/test/");
    assert_eq!(apply("/", "http://example.com/", None, None), "http://example.com/");
    assert_eq!(apply("/test/", "http://example.com/", None, None), "http://example.com/test/");
    assert_eq!(apply("subdir/", "http://example.com/", None, None), "http://example.com/subdir/");
    assert_eq!(apply("../subdir/", "http://example.com/", None, None), "http://example.com/subdir/");
    assert_eq!(apply("./subdir/", "http://example.com/", None, None), "http://example.com/subdir/");
    assert_eq!(apply("http://url.com/", "http://example.com/", None, None), "http://url.com/");
    assert_eq!(apply("http://url.com/test/", "http://example.com/", None, None), "http://url.com/test/");
    assert_eq!(apply("#anchor", "http://example.com/", None, None), "http://example.com/#anchor");
    assert_eq!(apply("/test/#anchor", "http://example.com/", None, None), "http://example.com/test/#anchor");
    assert_eq!(apply("/test/?param=value#anchor", "http://example.com/", None, None), "http://example.com/test/?param=value#anchor");
    assert_eq!(apply("/", "http://example.com/ignored/", None, None), "http://example.com/");
    assert_eq!(apply("/test/", "http://example.com/ignored/", None, None), "http://example.com/test/");
    assert_eq!(apply("subdir/", "http://example.com/deep/", None, None), "http://example.com/deep/subdir/");
    assert_eq!(apply("../subdir/", "http://example.com/deep/", None, None), "http://example.com/subdir/");
    assert_eq!(apply("./subdir/", "http://example.com/deep/", None, None), "http://example.com/deep/subdir/");
    assert_eq!(apply("http://url.com/", "http://example.com/ignored/", None, None), "http://url.com/");
    assert_eq!(apply("http://url.com/test/", "http://example.com/ignored/", None, None), "http://url.com/test/");
    assert_eq!(apply("/", "http://example.com/", Some("/"), None), "http://example.com/");
    assert_eq!(apply("/test/", "http://example.com/", Some("/"), None), "http://example.com/test/");
    assert_eq!(apply("subdir/", "http://example.com/", Some("/"), None), "http://example.com/subdir/");
    assert_eq!(apply("../subdir/", "http://example.com/", Some("/"), None), "http://example.com/subdir/");
    assert_eq!(apply("./subdir/", "http://example.com/", Some("/"), None), "http://example.com/subdir/");
    assert_eq!(apply("http://url.com/", "http://example.com/", Some("/"), None), "http://url.com/");
    assert_eq!(apply("http://url.com/test/", "http://example.com/", Some("/"), None), "http://url.com/test/");
    assert_eq!(apply("/", "http://example.com/", Some("/pathprefix/"), None), "http://example.com/pathprefix/");
    assert_eq!(apply("/test/", "http://example.com/", Some("/pathprefix/"), None), "http://example.com/pathprefix/test/");
    assert_eq!(apply("subdir/", "http://example.com/", Some("/pathprefix/"), None), "http://example.com/pathprefix/subdir/");
    assert_eq!(apply("../subdir/", "http://example.com/", Some("/pathprefix/"), None), "http://example.com/pathprefix/subdir/");
    assert_eq!(apply("./subdir/", "http://example.com/", Some("/pathprefix/"), None), "http://example.com/pathprefix/subdir/");
    assert_eq!(apply("http://url.com/", "http://example.com/", Some("/pathprefix/"), None), "http://url.com/");
    assert_eq!(apply("http://url.com/test/", "http://example.com/", Some("/pathprefix/"), None), "http://url.com/test/");
    assert_eq!(apply("/", "http://example.com/", Some("/pathprefix/"), Some("/deep/")), "http://example.com/pathprefix/");
    assert_eq!(apply("/test/", "http://example.com/", Some("/pathprefix/"), Some("/deep/")), "http://example.com/pathprefix/test/");
    assert_eq!(apply("subdir/", "http://example.com/", Some("/pathprefix/"), Some("/deep/")), "http://example.com/pathprefix/deep/subdir/");
    assert_eq!(apply("../subdir/", "http://example.com/", Some("/pathprefix/"), Some("/deep/")), "http://example.com/pathprefix/subdir/");
    assert_eq!(apply("./subdir/", "http://example.com/", Some("/pathprefix/"), Some("/deep/")), "http://example.com/pathprefix/deep/subdir/");
    assert_eq!(apply("http://url.com/", "http://example.com/", Some("/pathprefix/"), Some("/deep/")), "http://url.com/");
    assert_eq!(apply("http://url.com/test/", "http://example.com/", Some("/pathprefix/"), Some("/deep/")), "http://url.com/test/");
    assert_eq!(apply("/", "http://example.com/ignored/", Some("/pathprefix/"), Some("/deep/")), "http://example.com/pathprefix/");
    assert_eq!(apply("/test/", "http://example.com/ignored/", Some("/pathprefix/"), Some("/deep/")), "http://example.com/pathprefix/test/");
    assert_eq!(apply("subdir/", "http://example.com/ignored/", Some("/pathprefix/"), Some("/deep/")), "http://example.com/pathprefix/deep/subdir/");
    assert_eq!(apply("../subdir/", "http://example.com/ignored/", Some("/pathprefix/"), Some("/deep/")), "http://example.com/pathprefix/subdir/");
    assert_eq!(apply("./subdir/", "http://example.com/ignored/", Some("/pathprefix/"), Some("/deep/")), "http://example.com/pathprefix/deep/subdir/");
    assert_eq!(apply("http://url.com/", "http://example.com/ignored/", Some("/pathprefix/"), Some("/deep/")), "http://url.com/");
    assert_eq!(apply("http://url.com/test/", "http://example.com/ignored/", Some("/pathprefix/"), Some("/deep/")), "http://url.com/test/");
}

/// Render `template` (as `index.njk`) with the HTML base plugin and return `/deep/index.html`
fn render_with_plugin(template: &'static str, path_prefix: Option<&str>, plugin_options: Value) -> String {
    let mut opts = options(move |cfg: &mut UserConfig| {
        cfg.add_template("index.njk", template, Value::object())?;
        cfg.set_use_template_cache(false);
        cfg.add_plugin("HtmlBasePlugin", html_base::plugin, plugin_options.clone())?;
        Ok(())
    });
    opts.path_prefix = path_prefix.map(String::from);
    let results = to_json(
        "./test/stubs-virtual-nowrite/",
        Some("./test/stubs-virtual-nowrite/_site"),
        opts,
    )
    .unwrap();
    content_for(&results, "/deep/index.html")
}

/// "Using the HTML base plugin (default values)"
#[test]
fn using_the_html_base_plugin_default_values() {
    assert_eq!(
        render_with_plugin(BASE_TEMPLATE, None, Value::object()),
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<meta name="description" content="">
<title></title>
<style>div { background-image: url(test.jpg); }</style>
<style>div { background-image: url(/test.jpg); }</style>
<link rel="stylesheet" href="/test.css">
<script src="/test.js"></script>
</head>
<body>
<a href="/">Home</a>
<a href="subdir/">Test</a>
<a href="./subdir/">Test</a>
<a href="../subdir/">Test</a>
</body>
</html>"#
    );
}

/// "Using the HTML base plugin with pathPrefix: /test/"
#[test]
fn using_the_html_base_plugin_with_path_prefix_test() {
    assert_eq!(
        render_with_plugin(BASE_TEMPLATE, Some("/test/"), Value::object()),
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<meta name="description" content="">
<title></title>
<style>div { background-image: url(test.jpg); }</style>
<style>div { background-image: url(/test/test.jpg); }</style>
<link rel="stylesheet" href="/test/test.css">
<script src="/test/test.js"></script>
</head>
<body>
<a href="/test/">Home</a>
<a href="subdir/">Test</a>
<a href="subdir/">Test</a>
<a href="../subdir/">Test</a>
</body>
</html>"#
    );
}

/// "Using the HTML base plugin with pathPrefix: /test/ and base: http://example.com/"
#[test]
fn using_the_html_base_plugin_with_path_prefix_test_and_base() {
    assert_eq!(
        render_with_plugin(BASE_TEMPLATE, Some("/test/"), json(r#"{"baseHref": "http://example.com/"}"#)),
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<meta name="description" content="">
<title></title>
<style>div { background-image: url(test.jpg); }</style>
<style>div { background-image: url(http://example.com/test/test.jpg); }</style>
<link rel="stylesheet" href="http://example.com/test/test.css">
<script src="http://example.com/test/test.js"></script>
</head>
<body>
<a href="http://example.com/test/">Home</a>
<a href="http://example.com/test/deep/subdir/">Test</a>
<a href="http://example.com/test/deep/subdir/">Test</a>
<a href="http://example.com/test/subdir/">Test</a>
</body>
</html>"#
    );
}

/// "Using the HTML base plugin strips extra path in full URL base (default pathPrefix)"
#[test]
fn using_the_html_base_plugin_strips_extra_path_in_full_url_base_default_path_prefix() {
    assert_eq!(
        render_with_plugin(BASE_TEMPLATE, None, json(r#"{"baseHref": "http://example.com/hello/"}"#)),
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<meta name="description" content="">
<title></title>
<style>div { background-image: url(test.jpg); }</style>
<style>div { background-image: url(http://example.com/test.jpg); }</style>
<link rel="stylesheet" href="http://example.com/test.css">
<script src="http://example.com/test.js"></script>
</head>
<body>
<a href="http://example.com/">Home</a>
<a href="http://example.com/deep/subdir/">Test</a>
<a href="http://example.com/deep/subdir/">Test</a>
<a href="http://example.com/subdir/">Test</a>
</body>
</html>"#
    );
}

/// "Using the HTML base plugin strips extra path in full URL base (pathPrefix: /test/)"
#[test]
fn using_the_html_base_plugin_strips_extra_path_in_full_url_base_path_prefix_test() {
    assert_eq!(
        render_with_plugin(BASE_TEMPLATE, Some("/test/"), json(r#"{"baseHref": "http://example.com/hello/"}"#)),
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<meta name="description" content="">
<title></title>
<style>div { background-image: url(test.jpg); }</style>
<style>div { background-image: url(http://example.com/test/test.jpg); }</style>
<link rel="stylesheet" href="http://example.com/test/test.css">
<script src="http://example.com/test/test.js"></script>
</head>
<body>
<a href="http://example.com/test/">Home</a>
<a href="http://example.com/test/deep/subdir/">Test</a>
<a href="http://example.com/test/deep/subdir/">Test</a>
<a href="http://example.com/test/subdir/">Test</a>
</body>
</html>"#
    );
}

/// "Opt out of the transform with falsy extensions list"
#[test]
fn opt_out_of_the_transform_with_falsy_extensions_list() {
    assert_eq!(
        render_with_plugin(BASE_TEMPLATE, Some("/test/"), json(r#"{"extensions": false}"#)),
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<meta name="description" content="">
<title></title>
<style>div { background-image: url(test.jpg); }</style>
<style>div { background-image: url(/test/test.jpg); }</style>
<link rel="stylesheet" href="/test.css">
<script src="/test.js"></script>
</head>
<body>
<a href="/">Home</a>
<a href="subdir/">Test</a>
<a href="./subdir/">Test</a>
<a href="../subdir/">Test</a>
</body>
</html>"#
    );
}

/// "Base plugin with permalink: false, #2602"
#[test]
fn base_plugin_with_permalink_false_2602() {
    assert_eq!(
        render_with_plugin(BASE_TEMPLATE_2602, Some("/test/"), Value::object()),
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<meta name="description" content="">
<title></title>
<style>div { background-image: url(test.jpg); }</style>
<style>div { background-image: url(/test/test.jpg); }</style>
<link rel="stylesheet" href="/test/test.css">
<script src="/test/test.js"></script>
</head>
<body>
<a href="/test/">Home</a>
<a href="subdir/">Test</a>
<a href="../subdir/">Test</a>
</body>
</html>"#
    );
}

/// "Using the HTML base plugin with pathPrefix: /test/ and transformed attributes are *not* case sensitive"
#[test]
fn using_the_html_base_plugin_transformed_attributes_are_not_case_sensitive() {
    assert_eq!(
        render_with_plugin(BASE_TEMPLATE_CASE_SENSITIVE, Some("/test/"), Value::object()),
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<meta name="description" content="">
<title></title>
<style>div { background-image: url(test.jpg); }</style>
<style>div { background-image: url(/test/test.jpg); }</style>
<link rel="stylesheet" href="/test/test.css">
<script SrC="/test.js"></script>
</head>
<body>
<a hreF="/">Home</a>
<a HrEf="subdir/">Test</a>
<a href="../subdir/">Test</a>
</body>
</html>"#
    );
}

/// How many plugins are added by adding the HTML base plugin several times (by the same and
/// different names). Upstream also asserts no plugins exist beforehand: the port runs the
/// programmatic configuration after the default configuration, which already adds the
/// html-relative-copy plugin.
fn count_plugins_added(adds: &'static [&'static str]) -> usize {
    let counts = std::sync::Arc::new(std::sync::Mutex::new((usize::MAX, usize::MAX)));
    let c = counts.clone();
    let opts = options(move |cfg: &mut UserConfig| {
        let before = cfg.plugins.len();
        for name in adds {
            let plugin = resolve_builtin(name).unwrap();
            cfg.add_plugin_fn(name, plugin, Value::object())?;
        }
        *c.lock().unwrap() = (before, cfg.plugins.len());
        Ok(())
    });
    common::eleventy("./test/stubs-virtual-nowrite/", Some("./test/stubs-virtual-nowrite/_site"), opts)
        .to_json()
        .unwrap();
    let (before, after) = *counts.lock().unwrap();
    after - before
}

/// "HTML base plugin only adds once (unique)"
#[test]
fn html_base_plugin_only_adds_once_unique() {
    assert_eq!(count_plugins_added(&["HtmlBasePlugin"; 4]), 1);
}

/// "HTML base plugin only adds once when added from multiple nested plugins"
#[test]
fn html_base_plugin_only_adds_once_when_added_from_multiple_nested_plugins() {
    fn nested(cfg: &mut UserConfig, _: &Value) -> anyhow::Result<()> {
        let plugin = resolve_builtin("@11ty/eleventy/html-base-plugin").unwrap();
        cfg.add_plugin_fn("@11ty/eleventy/html-base-plugin", plugin, Value::object())
    }
    let mut opts = options(|cfg: &mut UserConfig| {
        cfg.add_template("index.html", r#"<a href="/foo/">Foo</a>"#, Value::object())?;
        cfg.add_plugin("nested", nested, Value::object())?;
        cfg.add_plugin("nested", nested, Value::object())?;
        Ok(())
    });
    opts.path_prefix = Some("/test/".into());
    let results = to_json("./test/stubs-virtual/", None, opts).unwrap();
    assert_eq!(results[0].content, r#"<a href="/test/foo/">Foo</a>"#);
}

/// "HTML base plugin can resolve by name"
#[test]
fn html_base_plugin_can_resolve_by_name() {
    assert_eq!(
        count_plugins_added(&[
            "@11ty/eleventy/html-base-plugin",
            "@11ty/eleventy/html-base-plugin",
            "HtmlBasePlugin",
            "HtmlBasePlugin",
        ]),
        1
    );
}

/// "Using recognizeNoValueAttribute for boolean attributes without quotes #2766"
#[test]
fn using_recognize_no_value_attribute_for_boolean_attributes_without_quotes_2766() {
    let mut opts = options(|cfg: &mut UserConfig| {
        cfg.set_use_template_cache(false);
        cfg.add_template(
            "index.njk",
            r#"---
permalink: /deep/
---
<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="description" content="">
<link rel="stylesheet" crossorigin>
<link rel="stylesheet" href="/test.css">
</head>
</html>"#,
            Value::object(),
        )?;
        cfg.add_plugin("HtmlBasePlugin", html_base::plugin, Value::object())
    });
    opts.path_prefix = Some("/prefixed/".into());
    let results = to_json("./test/stubs-virtual/", None, opts).unwrap();
    assert_eq!(
        content_for(&results, "/deep/index.html"),
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="description" content="">
<link rel="stylesheet" crossorigin>
<link rel="stylesheet" href="/prefixed/test.css">
</head>
</html>"#
    );
}

/// "HTML base plugin skips attributes with no value"
#[test]
fn html_base_plugin_skips_attributes_with_no_value() {
    let mut opts = options(|cfg: &mut UserConfig| {
        cfg.set_use_template_cache(false);
        cfg.add_template("index.html", r#"<a href>Test</a><img src><a href="/test/">Test</a>"#, Value::object())?;
        cfg.add_plugin("HtmlBasePlugin", html_base::plugin, Value::object())
    });
    opts.path_prefix = Some("/prefixed/".into());
    let results = to_json("./test/stubs-virtual/", None, opts).unwrap();
    assert_eq!(results[0].content, r#"<a href>Test</a><img src><a href="/prefixed/test/">Test</a>"#);
}
