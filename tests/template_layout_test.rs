//! Port of upstream `test/TemplateLayoutTest.js`.
//!
//! Skipped: "Render Layout (Pass in undefined template content)" and "… (Pass in null template
//! content)": page content is always a string in the port, so `undefined`/`null` content can’t be
//! passed to a layout.

mod template_support;

use std::sync::Arc;

use template_support::{templates, virtual_options};
use twelvety::Value;
use twelvety::build_context::BuildContext;
use twelvety::page_entry::PageEntry;
use twelvety::template_layout::TemplateLayout;
use twelvety::testing::{TestOptions, get_context, json, template_with_context};

const INHERIT_LAYOUTS: &[(&str, &str)] = &[
    (
        "_includes/layouts/layout-inherit-a.njk",
        "---\ninherits: a\nlayout: layouts/layout-inherit-b.njk\n---\n{{content}} {{inherits}}",
    ),
    (
        "_includes/layouts/layout-inherit-b.njk",
        "---\ninherits: b\nsecondinherits: b\nlayout: layouts/layout-inherit-c.njk\n---\n{{ content | safe }} {{secondinherits}}",
    ),
    (
        "_includes/layouts/layout-inherit-c.njk",
        "---\ninherits: c\nsecondinherits: c\nthirdinherits: c\n---\n{{ content | safe }} {{inherits}} {{thirdinherits}}",
    ),
    (
        "_includes/layouts/layout-contentdump.njk",
        "---\ninherits: a\nlayout: layouts/layout-inherit-b.njk\n---\n{{content | default(\"this is bad\")}} {{inherits}}",
    ),
];

fn context(input: &str, list: &[(&str, &str)]) -> Arc<BuildContext> {
    get_context(virtual_options(input, "_site", templates(list), |_| Ok(()))).unwrap()
}

/// `getTemplateLayoutInstance(key, inputDir, map, virtualTemplates)`
fn layout(key: &str, input: &str, list: &[(&str, &str)]) -> Arc<TemplateLayout> {
    TemplateLayout::get_template(key, &context(input, list)).unwrap()
}

/// `renderLayoutViaLayout(layout, data, templateContent)`: renders for a (virtual) page with `data`
fn render_via_layout(layout: &Arc<TemplateLayout>, data: Value, template_content: &str) -> String {
    let ctx = &layout.content.ctx;
    let page = template_with_context(&format!("{}layout-test-page.njk", ctx.dirs.input()), ctx);
    let entry = PageEntry::new(page, String::new(), None, Value::Number(0.0), String::new(), data, None);
    layout.render_layout_page_entry(&entry, template_content.to_string()).unwrap()
}

fn inherit_data() -> Value {
    json(r#"{"inherits": "a", "secondinherits": "b", "thirdinherits": "c"}"#)
}

const CHAIN: [&str; 3] = [
    "./test/stubs/_includes/layouts/layout-inherit-a.njk",
    "./test/stubs/_includes/layouts/layout-inherit-b.njk",
    "./test/stubs/_includes/layouts/layout-inherit-c.njk",
];

#[test]
fn creation() {
    let tl = layout("base", "./test/stubs", &[]);
    assert_eq!(tl.input_path, "./test/stubs/_includes/base.njk");
    let ctx = get_context(TestOptions::input("./test/stubs")).unwrap();
    assert!(TemplateLayout::get_template("doesnotexist", &ctx).is_err());
}

#[test]
fn get_layout_chain() {
    let tl = layout("layouts/layout-inherit-a.njk", "./test/stubs", INHERIT_LAYOUTS);
    tl.get_data().unwrap();
    assert_eq!(tl.get_layout_chain().unwrap(), CHAIN);
}

#[test]
fn get_front_matter_data() {
    let tl = layout("layouts/layout-inherit-a.njk", "./test/stubs", INHERIT_LAYOUTS);
    assert_eq!(tl.input_path, "./test/stubs/_includes/layouts/layout-inherit-a.njk");
    assert_eq!(tl.get_data().unwrap(), inherit_data());
    assert_eq!(tl.get_layout_chain().unwrap(), CHAIN);
    assert_eq!(tl.get_data().unwrap(), inherit_data());
}

#[test]
fn render_layout() {
    let tl = layout("layouts/layout-inherit-a.njk", "./test/stubs", INHERIT_LAYOUTS);
    assert_eq!(render_via_layout(&tl, inherit_data(), "").trim(), "a b a c");
}

#[test]
fn render_layout_pass_in_template_content() {
    let tl = layout("layouts/layout-inherit-a.njk", "./test/stubs", INHERIT_LAYOUTS);
    assert_eq!(
        render_via_layout(&tl, inherit_data(), "TEMPLATE_CONTENT").trim(),
        "TEMPLATE_CONTENT a b a c"
    );
}

#[test]
fn render_layout_pass_in_empty_template_content() {
    let tl = layout("layouts/layout-contentdump.njk", "./test/stubs", INHERIT_LAYOUTS);
    assert_eq!(render_via_layout(&tl, inherit_data(), ""), " a b a c");
}

#[test]
fn cache_duplicates_use_full_key_for_cache() {
    // two layouts with the same file name in different input dirs must not share a cache entry
    let tla = layout("layout.njk", "./test/stubs-virtual-nowrite", &[("_includes/layout.njk", "Hello A")]);
    assert_eq!(render_via_layout(&tla, Value::object(), "").trim(), "Hello A");
    let tlb = layout("layout.njk", "./test/stubs-empty", &[("_includes/layout.njk", "Hello B")]);
    assert_eq!(render_via_layout(&tlb, Value::object(), "").trim(), "Hello B");
    assert_eq!(render_via_layout(&tla, Value::object(), "").trim(), "Hello A");
}

#[test]
fn throw_an_error_if_a_layout_references_itself_as_the_layout() {
    let tl = layout(
        "layout-cycle-self.njk",
        "./test/stubs-virtual-nowrite",
        &[("_includes/layout-cycle-self.njk", "---\nlayout: layout-cycle-self.njk\n---")],
    );
    let err = tl.get_layout_chain().unwrap_err();
    assert!(err.to_string().contains("circular reference"), "{err}");
}

#[test]
fn throw_an_error_if_a_circular_layout_chain_is_detected() {
    let tl = layout(
        "layout-cycle-a.njk",
        "./test/stubs-virtual-nowrite",
        &[
            ("_includes/layout-cycle-a.njk", "---\nlayout: layout-cycle-b.njk\n---"),
            ("_includes/layout-cycle-b.njk", "---\nlayout: layout-cycle-c.njk\n---"),
            ("_includes/layout-cycle-c.njk", "---\nlayout: layout-cycle-a.njk\n---"),
        ],
    );
    let err = tl.get_layout_chain().unwrap_err();
    assert!(err.to_string().contains("circular reference"), "{err}");
}
