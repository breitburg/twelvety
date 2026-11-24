//! Port of upstream `test/IdAttributePluginTest.js`.

mod common;

use twelvety::Value;
use twelvety::plugins::id_attribute;
use twelvety::template::PageResult;
use twelvety::user_config::UserConfig;

use common::{options, to_json};

fn build(templates: &'static [(&'static str, &'static str)], plugin_options: Value) -> anyhow::Result<Vec<PageResult>> {
    to_json(
        "./test/stubs-virtual/",
        Some("./test/stubs-virtual/_site"),
        options(move |cfg: &mut UserConfig| {
            cfg.add_plugin("IdAttributePlugin", id_attribute::plugin, plugin_options.clone())?;
            for (path, content) in templates {
                cfg.add_template(path, content, Value::object())?;
            }
            Ok(())
        }),
    )
}

fn first_content(templates: &'static [(&'static str, &'static str)]) -> String {
    build(templates, Value::object()).unwrap()[0].content.trim().to_string()
}

/// "Using the IdAttribute plugin #3356"
#[test]
fn using_the_id_attribute_plugin_3356() {
    assert_eq!(
        first_content(&[(
            "test.njk",
            r#"<h1>This is a heading</h1><h2 id="already">This is another heading</h2><h2>This is another heading</h2><h3>This is another heading</h3>"#
        )]),
        r#"<h1 id="this-is-a-heading">This is a heading</h1><h2 id="already">This is another heading</h2><h2 id="this-is-another-heading">This is another heading</h2><h3 id="this-is-another-heading-2">This is another heading</h3>"#
    );
}

/// "Using the IdAttribute plugin, eleventy:ignore attribute #3356"
#[test]
fn using_the_id_attribute_plugin_eleventy_ignore_attribute_3356() {
    assert_eq!(
        first_content(&[(
            "test.njk",
            r#"<h1>This is a heading</h1><h2 id="already">This is another heading</h2><h2>This is another heading</h2><h3>This is another <span eleventy:id-ignore>heading</span></h3>"#
        )]),
        r#"<h1 id="this-is-a-heading">This is a heading</h1><h2 id="already">This is another heading</h2><h2 id="this-is-another-heading">This is another heading</h2><h3 id="this-is-another">This is another <span>heading</span></h3>"#
    );
}

/// "Using the IdAttribute plugin, buildawesome:ignore attribute #3356"
#[test]
fn using_the_id_attribute_plugin_buildawesome_ignore_attribute_3356() {
    assert_eq!(
        first_content(&[(
            "test.njk",
            r#"<h1>This is a heading</h1><h2 id="already">This is another heading</h2><h2>This is another heading</h2><h3>This is another <span buildawesome:id-ignore>heading</span></h3>"#
        )]),
        r#"<h1 id="this-is-a-heading">This is a heading</h1><h2 id="already">This is another heading</h2><h2 id="this-is-another-heading">This is another heading</h2><h3 id="this-is-another">This is another <span>heading</span></h3>"#
    );
}

/// "Using the IdAttribute plugin with escaped quoted text"
#[test]
fn using_the_id_attribute_plugin_with_escaped_quoted_text() {
    assert_eq!(
        first_content(&[("test.md", "# This is a `\"heading\"`")]),
        r#"<h1 id="this-is-a-heading">This is a <code>&quot;heading&quot;</code></h1>"#
    );
}

macro_rules! conflict_test {
    ($name:ident, $doc:literal, $input:literal, $expected:literal) => {
        #[doc = $doc]
        #[test]
        fn $name() {
            assert_eq!(first_content(&[("test.html", $input)]), $expected);
        }
    };
}

conflict_test!(
    id_attribute_conflicts_supplied_first,
    "\"Issue #3424, id attribute conflicts (id attribute supplied first)\"",
    r#"<div id="testing"></div><h1>Testing</h1>"#,
    r#"<div id="testing"></div><h1 id="testing-2">Testing</h1>"#
);

conflict_test!(
    id_attribute_conflicts_supplied_last,
    "\"Issue #3424, id attribute conflicts (id attribute supplied last)\"",
    r#"<h1>Testing</h1><div id="testing"></div>"#,
    r#"<h1 id="testing-2">Testing</h1><div id="testing"></div>"#
);

conflict_test!(
    id_attribute_conflicts_hard_coded,
    "\"Issue #3424, id attribute conflicts (hard coded id conflicts)\"",
    r#"<h1>Testing</h1><h1 id="testing">Testing</h1>"#,
    r#"<h1 id="testing-2">Testing</h1><h1 id="testing">Testing</h1>"#
);

conflict_test!(
    id_attribute_conflicts_three_deep,
    "\"Issue #3424, id attribute conflicts (three deep, hard coded id conflicts)\"",
    r#"<h1>Testing</h1><h1>Testing</h1><h1 id="testing">Testing</h1>"#,
    r#"<h1 id="testing-2">Testing</h1><h1 id="testing-3">Testing</h1><h1 id="testing">Testing</h1>"#
);

conflict_test!(
    id_attribute_conflicts_four_deep,
    "\"Issue #3424, id attribute conflicts (four deep, hard coded id conflicts)\"",
    r#"<h1>Testing</h1><h1>Testing</h1><h1>Testing</h1><h1 id="testing">Testing</h1>"#,
    r#"<h1 id="testing-2">Testing</h1><h1 id="testing-3">Testing</h1><h1 id="testing-4">Testing</h1><h1 id="testing">Testing</h1>"#
);

conflict_test!(
    id_attribute_conflicts_five_deep,
    "\"Issue #3424, id attribute conflicts (five deep, hard coded id conflicts)\"",
    r#"<h1>Testing</h1><h1>Testing</h1><h1>Testing</h1><h1>Testing</h1><h1 id="testing">Testing</h1>"#,
    r#"<h1 id="testing-2">Testing</h1><h1 id="testing-3">Testing</h1><h1 id="testing-4">Testing</h1><h1 id="testing-5">Testing</h1><h1 id="testing">Testing</h1>"#
);

/// "Issue #3424, id attribute conflicts (two hard coded id conflicts)"
#[test]
fn id_attribute_conflicts_two_hard_coded() {
    let err = build(
        &[("test.html", r#"<h1 id="testing">Testing</h1><h1 id="testing">Testing</h1>"#)],
        Value::object(),
    )
    .unwrap_err();
    let root = err.root_cause().to_string();
    assert_eq!(
        root,
        "You have more than one HTML `id` attribute using the same value (id=\"testing\") in your template (./test/stubs-virtual/test.html). You can disable this error in the IdAttribute plugin with the `checkDuplicates: false` option."
    );
}

/// "Issue #3424, filter callback skips"
#[test]
fn filter_callback_skips() {
    let mut plugin_options = Value::object();
    plugin_options.set(
        "filter",
        Value::function(|args| {
            let input_path = args[0].get("page").get("inputPath").to_output_string();
            Ok(Value::Bool(!input_path.ends_with("test-skipped.html")))
        }),
    );
    let results = build(
        &[
            ("test.html", r#"<h1>Testing</h1><h1 id="testing">Testing</h1>"#),
            ("test-skipped.html", r#"<h1 id="testing">Testing</h1><h1 id="testing">Testing</h1>"#),
        ],
        plugin_options,
    )
    .unwrap();
    let content = |name: &str| {
        results
            .iter()
            .find(|r| r.input_path.ends_with(name))
            .unwrap()
            .content
            .trim()
            .to_string()
    };
    assert_eq!(content("/test.html"), r#"<h1 id="testing-2">Testing</h1><h1 id="testing">Testing</h1>"#);
    assert_eq!(content("/test-skipped.html"), r#"<h1 id="testing">Testing</h1><h1 id="testing">Testing</h1>"#);
}

conflict_test!(
    issue_4445_generated_suffix_conflicts_with_later_heading,
    "\"Issue #4445, id attribute conflicts (generated suffix conflicts with later heading)\"",
    r#"<h1>Foo</h1><h1>Foo</h1><h1>Foo 2</h1>"#,
    r#"<h1 id="foo">Foo</h1><h1 id="foo-3">Foo</h1><h1 id="foo-2">Foo 2</h1>"#
);

conflict_test!(
    issue_4445_generated_suffix_conflicts_with_later_id_attribute,
    "\"Issue #4445, id attribute conflicts (generated suffix conflicts with later id attribute)\"",
    r#"<h1>Foo</h1><h1>Foo</h1><div id="foo-2"></div>"#,
    r#"<h1 id="foo">Foo</h1><h1 id="foo-3">Foo</h1><div id="foo-2"></div>"#
);

conflict_test!(
    issue_4445_renamed_heading_conflicts_with_later_heading,
    "\"Issue #4445, id attribute conflicts (renamed heading conflicts with later heading)\"",
    r#"<h1>Foo</h1><div id="foo"></div><h1>Foo 2</h1>"#,
    r#"<h1 id="foo-3">Foo</h1><div id="foo"></div><h1 id="foo-2">Foo 2</h1>"#
);
