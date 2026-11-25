//! Ported from upstream `test/TemplateMapTest-ComputedData.js`. The `---js` front matter
//! (and the `child.12ty.cjs` template’s data) is passed to `addTemplate` as data with Rust
//! closures for the computed data functions.
//!
//! Skipped (JS-only): "Computed data in directory data file consumes data file data, Issue #1137"
//! (`.12ty.cjs` templates and a `dir.data.cjs` directory data file).

use std::sync::Arc;

use twelvety::Value;
use twelvety::build_context::BuildContext;
use twelvety::template_map::TemplateMap;
use twelvety::testing::{TestOptions, get_context, json, template_with_context};

const INPUT: &str = "./test/stubs-virtual-nowrite/";
const DOG_TEMPLATE: &str = "---\ntags: dog\n---\nHi from dog";

fn virtual_config(templates: Vec<(&'static str, &'static str, Value)>) -> Arc<BuildContext> {
    get_context(TestOptions::new(INPUT, "./dist").with_config(move |cfg| {
        cfg.set_data_file_suffixes(&[]);
        for (path, content, data) in &templates {
            cfg.add_template(path, content, data.clone())?;
        }
        Ok(())
    }))
    .unwrap()
}

/// `twelvetyComputed` with `test: "hello"` and `dogCollection` from `f(collections.dog)`
fn computed_collections_data(f: fn(Value) -> Value) -> Value {
    let mut computed = json(r#"{"test": "hello"}"#);
    computed.set("dogCollection", Value::function(move |args| Ok(f(args[0].get_path("collections.dog")))));
    let mut data = Value::object();
    data.set("twelvetyComputed", computed);
    data
}

fn build(ctx: &Arc<BuildContext>, paths: &[&str]) -> TemplateMap {
    let mut tm = TemplateMap::new(ctx.clone());
    for path in paths {
        tm.add(template_with_context(&format!("{INPUT}{path}"), ctx)).unwrap();
    }
    tm.cache().unwrap();
    tm
}

fn check_collections_entry(tm: &TemplateMap) -> (Value, Value) {
    assert!(tm.map[0].input_path.ends_with("collections.njk"));
    let collections = tm.map[0].data.get("collections");
    assert_eq!(collections.get("all").len(), 2);
    assert_eq!(collections.get("dog").len(), 1);
    let data = tm.map[0].pages[0].data();
    assert_eq!(data.get("dogCollection").len(), 1);
    assert_eq!(data.get("test"), Value::str("hello"));
    (data.get("dogCollection"), collections.get("dog"))
}

#[test]
fn computed_data_can_see_tag_generated_collections() {
    let ctx = virtual_config(vec![
        ("collections.njk", "Issue #1114", computed_collections_data(|dog| dog)),
        ("dog.njk", DOG_TEMPLATE, Value::object()),
    ]);
    let tm = build(&ctx, &["collections.njk", "dog.njk"]);
    let (dog_collection, dog) = check_collections_entry(&tm);
    // THEY ARE THE SAME
    assert_eq!(dog_collection, dog);
}

#[test]
fn computed_data_can_see_paginated_data_issue_1138() {
    let mut child_computed = Value::object();
    child_computed.set("venues", Value::function(|args| Ok(args[0].get_path("collections.venue"))));
    let mut child_data = Value::object();
    child_data.set("twelvetyComputed", child_computed);

    // upstream’s consumer is a `child.12ty.cjs` template with the same data
    let ctx = virtual_config(vec![
        (
            "paginated.njk",
            "---\nvenues:\n  - first\n  - second\npagination:\n  data: venues\n  size: 1\n  alias: venue\n  addAllPagesToCollections: true\npermalink: \"venues/{{ venue }}/\"\ntags: venue\ntwelvetyComputed:\n  title: \"{{ venue }}\"\n---",
            Value::object(),
        ),
        ("child.njk", "", child_data),
    ]);
    let tm = build(&ctx, &["paginated.njk", "child.njk"]);
    assert_eq!(tm.map.len(), 2);

    // paginated template tests
    assert!(tm.map[0].input_path.ends_with("paginated.njk"));
    assert_eq!(tm.map[0].pages.len(), 2);
    for (page, venue) in tm.map[0].pages.iter().zip(["first", "second"]) {
        let data = page.data();
        assert_eq!(data.get("venue"), Value::str(venue));
        assert_eq!(data.get("title"), Value::str(venue));
        assert_eq!(page.url(), Value::str(format!("/venues/{venue}/")));
        assert_eq!(data.get_path("collections.venue").len(), 2);
    }

    // consumer of paginated template tests
    assert_eq!(tm.map[1].pages.len(), 1);
    // computed prop from venues
    assert_eq!(tm.map[1].pages[0].data().get("venues").len(), 2);
}

#[test]
#[ignore = "JS-only: `.12ty.cjs` templates and a `dir.data.cjs` directory data file"]
fn computed_data_in_directory_data_file_consumes_data_file_data_issue_1137() {}

#[test]
fn computed_data_can_filter_collections() {
    let ctx = virtual_config(vec![
        (
            "collections.njk",
            "Issue #1114",
            computed_collections_data(|dog| Value::array(dog.to_vec().into_iter().filter(|_| true).collect())),
        ),
        ("dog.njk", DOG_TEMPLATE, Value::object()),
    ]);
    let tm = build(&ctx, &["collections.njk", "dog.njk"]);
    let (dog_collection, dog) = check_collections_entry(&tm);
    // Deeply equal (the port’s arrays have no identity distinct from their contents)
    assert_eq!(dog_collection, dog);
}
