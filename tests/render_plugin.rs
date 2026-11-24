//! Tests for the Render plugin (ported from upstream `test/TemplateRenderPluginTest.js`).

use twelvety::Value;
use twelvety::testing::{TestOptions, json, render_string};

fn opts() -> TestOptions {
    TestOptions::input("./test/stubs-render-plugin/").with_config(|cfg| {
        cfg.add_plugin("render", twelvety::plugins::render::plugin, Value::object())
    })
}

#[test]
fn render_template_liquid_in_liquid() {
    let out = render_string(
        "liquid",
        "{% renderTemplate \"liquid\" %}{{ 1 | plus: 2 }}{% endrenderTemplate %}",
        Value::object(),
        opts(),
    )
    .unwrap();
    assert_eq!(out, "3");
}

#[test]
fn render_template_markdown_in_njk() {
    let out = render_string(
        "njk",
        "{% renderTemplate \"md\" %}# Hello {{ name }}{% endrenderTemplate %}",
        json(r#"{"name": "x"}"#),
        opts(),
    )
    .unwrap();
    assert_eq!(out.trim(), "<h1>Hello {{ name }}</h1>");
}

#[test]
fn render_template_with_data() {
    let out = render_string(
        "liquid",
        "{% assign d = 'Zach' %}{% renderTemplate \"njk\", data %}Hi {{ name }}{% endrenderTemplate %}",
        json(r#"{"data": {"name": "Zach"}}"#),
        opts(),
    )
    .unwrap();
    assert_eq!(out, "Hi Zach");
}

#[test]
fn render_content_filter() {
    let out = render_string(
        "liquid",
        "{{ \"# Title\" | renderContent: \"md\" }}",
        Value::object(),
        opts(),
    )
    .unwrap();
    assert_eq!(out.trim(), "<h1>Title</h1>");
}
