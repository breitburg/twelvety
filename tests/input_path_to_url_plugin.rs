//! Port of upstream `test/InputPathToUrlPluginTest.js`.

mod common;

use twelvety::Value;
use twelvety::plugins::{html_base, input_path_to_url};
use twelvety::template::PageResult;
use twelvety::testing::json;
use twelvety::user_config::UserConfig;

use common::{content_for, options, to_json};

const OUTPUT_HTML_STD: &str = r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<meta name="description" content="">
<title></title>
<link rel="stylesheet" href="/output.css">
<script src="/output.css"></script>
</head>
<body>
<a href="/">Home</a>
<a href="/tmpl/">Test</a>
<a href="/tmpl/#anchor">Anchor</a>
<a href="#anchor">Anchor</a>
<a href="./#anchor">Anchor</a>
<a href="?search=1">Search</a>
<a href="./?search=1">Search</a>
</body>
</html>"##;

const OUTPUT_HTML_BASE: &str = r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<meta name="description" content="">
<title></title>
<link rel="stylesheet" href="/gh-pages/output.css">
<script src="/gh-pages/output.css"></script>
</head>
<body>
<a href="/gh-pages/">Home</a>
<a href="/gh-pages/tmpl/">Test</a>
<a href="/gh-pages/tmpl/#anchor">Anchor</a>
<a href="#anchor">Anchor</a>
<a href="#anchor">Anchor</a>
<a href="?search=1">Search</a>
<a href="?search=1">Search</a>
</body>
</html>"##;

fn add_templates(cfg: &mut UserConfig) -> anyhow::Result<()> {
    cfg.add_template("css.njk", "---\npermalink: output.css\n---", Value::object())?;
    cfg.add_template("tmpl.njk", "", Value::object())?;
    cfg.add_template("filter.njk", r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<meta name="description" content="">
<title></title>
<link rel="stylesheet" href="{{ 'css.njk' | inputPathToUrl }}">
<script src="{{ 'css.njk' | inputPathToUrl }}"></script>
</head>
<body>
<a href="/">Home</a>
<a href="{{ 'tmpl.njk' | inputPathToUrl }}">Test</a>
<a href="{{ 'tmpl.njk#anchor' | inputPathToUrl }}">Anchor</a>
<a href="#anchor">Anchor</a>
<a href="./#anchor">Anchor</a>
<a href="?search=1">Search</a>
<a href="./?search=1">Search</a>
</body>
</html>"##, Value::object())?;
    cfg.add_template("transform.njk", r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<meta name="description" content="">
<title></title>
<link rel="stylesheet" href="css.njk">
<script src="css.njk"></script>
</head>
<body>
<a href="/">Home</a>
<a href="tmpl.njk">Test</a>
<a href="tmpl.njk#anchor">Anchor</a>
<a href="#anchor">Anchor</a>
<a href="./#anchor">Anchor</a>
<a href="?search=1">Search</a>
<a href="./?search=1">Search</a>
</body>
</html>"##, Value::object())?;
    Ok(())
}

fn add_transform_plugin(cfg: &mut UserConfig) -> anyhow::Result<()> {
    cfg.add_plugin("InputPathToUrlTransformPlugin", input_path_to_url::transform_plugin, Value::object())
}

fn add_base_plugin(cfg: &mut UserConfig) -> anyhow::Result<()> {
    cfg.add_plugin("HtmlBasePlugin", html_base::plugin, Value::object())
}

fn build_nowrite(path_prefix: Option<&str>, config: impl Fn(&mut UserConfig) -> anyhow::Result<()> + Send + Sync + 'static) -> Vec<PageResult> {
    let mut opts = options(config);
    opts.path_prefix = path_prefix.map(String::from);
    to_json("./test/stubs-virtual-nowrite/", Some("./test/stubs-virtual-nowrite/_site"), opts).unwrap()
}

fn build_virtual(config: impl Fn(&mut UserConfig) -> anyhow::Result<()> + Send + Sync + 'static) -> Vec<PageResult> {
    to_json("./test/stubs-virtual/", Some("./test/stubs-virtual/_site"), options(config)).unwrap()
}

/// "Using the transform (and the filter too)"
#[test]
fn using_the_transform_and_the_filter_too() {
    let results = build_nowrite(None, |cfg| {
        add_templates(cfg)?;
        add_transform_plugin(cfg)
    });
    assert_eq!(content_for(&results, "/filter/index.html"), OUTPUT_HTML_STD);
    assert_eq!(content_for(&results, "/transform/index.html"), OUTPUT_HTML_STD);
}

/// "Using the filter"
#[test]
fn using_the_filter() {
    let results = build_nowrite(None, add_templates);
    assert_eq!(content_for(&results, "/filter/index.html"), OUTPUT_HTML_STD);
    assert_ne!(content_for(&results, "/transform/index.html"), OUTPUT_HTML_STD);
}

/// "Using the transform and the base plugin"
#[test]
fn using_the_transform_and_the_base_plugin() {
    let results = build_nowrite(Some("/gh-pages/"), |cfg| {
        add_templates(cfg)?;
        add_transform_plugin(cfg)?;
        add_base_plugin(cfg)
    });
    assert_eq!(content_for(&results, "/filter/index.html"), OUTPUT_HTML_BASE);
    assert_eq!(content_for(&results, "/transform/index.html"), OUTPUT_HTML_BASE);
}

/// "Using the transform and the base plugin, reverse order"
#[test]
fn using_the_transform_and_the_base_plugin_reverse_order() {
    let results = build_nowrite(Some("/gh-pages/"), |cfg| {
        add_templates(cfg)?;
        add_base_plugin(cfg)?;
        add_transform_plugin(cfg)
    });
    assert_eq!(content_for(&results, "/filter/index.html"), OUTPUT_HTML_BASE);
    assert_eq!(content_for(&results, "/transform/index.html"), OUTPUT_HTML_BASE);
}

/// "Issue #3417 Using the transform with relative path (dot slash)"
#[test]
fn issue_3417_using_the_transform_with_relative_path_dot_slash() {
    let results = build_virtual(|cfg| {
        add_transform_plugin(cfg)?;
        cfg.add_template("source/test.njk", r##"<a href="./target.njk">Target</a>"##, Value::object())?;
        cfg.add_template("source/target.njk", "lol", Value::object())
    });
    assert_eq!(content_for(&results, "/source/test/index.html"), r##"<a href="/source/target/">Target</a>"##);
}

/// "Issue #3417 Using the transform with relative path (no dot slash)"
#[test]
fn issue_3417_using_the_transform_with_relative_path_no_dot_slash() {
    let results = build_virtual(|cfg| {
        add_transform_plugin(cfg)?;
        cfg.add_template("source/test.njk", r##"<a href="target.njk">Target</a>"##, Value::object())?;
        cfg.add_template("source/target.njk", "lol", Value::object())
    });
    assert_eq!(content_for(&results, "/source/test/index.html"), r##"<a href="/source/target/">Target</a>"##);
}

/// "Issue #3581 #build-cost-🧰"
#[test]
fn issue_3581_build_cost() {
    let results = build_virtual(|cfg| {
        add_transform_plugin(cfg)?;
        cfg.add_template("source/test.njk", r##"<a href="#built-cost-🧰">Target</a>"##, Value::object())
    });
    assert_eq!(content_for(&results, "/source/test/index.html"), r##"<a href="#built-cost-🧰">Target</a>"##);
}

/// "Issue #3583 Markdown diacritics (no plugin)"
#[test]
fn issue_3583_markdown_diacritics_no_plugin() {
    let results = build_virtual(|cfg| cfg.add_template("test.md", "[Target](</hypothèse/>)", Value::object()));
    assert_eq!(content_for(&results, "/test/index.html"), r##"<p><a href="/hypoth%C3%A8se/">Target</a></p>"##);
}

/// "Issue #3583 Diacritics"
#[test]
fn issue_3583_diacritics() {
    let results = build_virtual(|cfg| {
        add_transform_plugin(cfg)?;
        cfg.add_template("test.md", "[Target](/hypothèse.md)", Value::object())?;
        cfg.add_template("hypothèse.md", "lol", Value::object())
    });
    assert_eq!(content_for(&results, "/test/index.html"), r##"<p><a href="/hypothèse/">Target</a></p>"##);
}

/// "Issue #3583 Diacritics Markdown raw"
#[test]
fn issue_3583_diacritics_markdown_raw() {
    let results = build_virtual(|cfg| {
        add_transform_plugin(cfg)?;
        cfg.add_template("test.md", "[Target](</hypothèse.md>)", Value::object())?;
        cfg.add_template("hypothèse.md", "lol", Value::object())
    });
    assert_eq!(content_for(&results, "/test/index.html"), r##"<p><a href="/hypothèse/">Target</a></p>"##);
}

/// "Issue #3583 #3559 Markdown link with spaces (no plugin)"
#[test]
fn issue_3583_3559_markdown_link_with_spaces_no_plugin() {
    let results = build_virtual(|cfg| cfg.add_template("test.md", "[Target](</target 1/>)", Value::object()));
    assert_eq!(content_for(&results, "/test/index.html"), r##"<p><a href="/target%201/">Target</a></p>"##);
}

/// "Issue #3583 #3559 Markdown spaces"
#[test]
fn issue_3583_3559_markdown_spaces() {
    let results = build_virtual(|cfg| {
        add_transform_plugin(cfg)?;
        cfg.add_template("test.md", "[Target](<target 1.md>)", Value::object())?;
        cfg.add_template("target 1.md", "lol", json(r##"{"permalink": "/{{ page.fileSlug | slugify }}/"}"##))
    });
    assert_eq!(content_for(&results, "/test/index.html"), r##"<p><a href="/target-1/">Target</a></p>"##);
}
