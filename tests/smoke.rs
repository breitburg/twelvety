//! Smoke tests ported from upstream `test/TemplateTest.js`.

use twelvety::testing::{TestOptions, get_new_template, render_template};

fn virtual_options(
    input: &str,
    output: &str,
    templates: &[(&'static str, &'static str)],
) -> TestOptions {
    let templates: Vec<(&'static str, &'static str)> = templates.to_vec();
    TestOptions::new(input, output).with_config(move |cfg| {
        for (path, content) in &templates {
            cfg.add_template(path, content, twelvety::Value::object())?;
        }
        Ok(())
    })
}

#[test]
fn output_path_maps_to_an_html_file() {
    let tmpl = get_new_template(
        "./test/stubs/template.liquid",
        virtual_options("./test/stubs/", "./dist", &[("template.liquid", "")]),
    )
    .unwrap();
    assert_eq!(tmpl.get_template_subfolder(), "");
    let data = tmpl.get_data().unwrap();
    let loc = tmpl.get_output_locations(&data, "").unwrap();
    assert_eq!(loc.path.as_str(), Some("./dist/template/index.html"));
}

#[test]
fn subfolder_outputs_to_double_subfolder() {
    let tmpl = get_new_template(
        "./test/stubs/subfolder/subfolder/subfolder.liquid",
        virtual_options(
            "./test/stubs/",
            "./dist",
            &[("subfolder/subfolder/subfolder.liquid", "")],
        ),
    )
    .unwrap();
    let data = tmpl.get_data().unwrap();
    assert_eq!(tmpl.get_template_subfolder(), "subfolder/subfolder");
    let loc = tmpl.get_output_locations(&data, "").unwrap();
    assert_eq!(
        loc.path.as_str(),
        Some("./dist/subfolder/subfolder/subfolder/index.html")
    );
}

#[test]
fn html_index_in_same_dir() {
    let tmpl = get_new_template(
        "./test/stubs/index.html",
        virtual_options(
            "./test/stubs",
            "./test/stubs",
            &[("index.html", "<html></html>")],
        ),
    )
    .unwrap();
    let data = tmpl.get_data().unwrap();
    let loc = tmpl.get_output_locations(&data, "").unwrap();
    assert_eq!(loc.path.as_str(), Some("./test/stubs/index.html"));
}

#[test]
fn front_matter_render() {
    let tmpl = get_new_template(
        "./test/stubs/fm.liquid",
        virtual_options(
            "./test/stubs/",
            "./dist",
            &[("fm.liquid", "---\nkey1: value1\nkey2: value2\nkey3: value3\n---\nc:{{ key1 }}:{{ key2 }}:{{ key3 }}")],
        ),
    )
    .unwrap();
    let pages = render_template(&tmpl).unwrap();
    assert_eq!(pages[0].content, "c:value1:value2:value3");
}

#[test]
fn two_layouts() {
    let tmpl = get_new_template(
        "./test/stubs/templateTwoLayouts.liquid",
        virtual_options(
            "./test/stubs/",
            "dist",
            &[
                (
                    "templateTwoLayouts.liquid",
                    "---\nlayout: layout-a\nkey1: value1\npermalink: /rename-font/\ndaysPosted: 152\n---\n\n<p>{{ upstream }}</p>",
                ),
                (
                    "_includes/layout-a.liquid",
                    "---\nlayout: layout-b\nkey1: value1-a\nupstream: value2-a\n---\n\n<div id=\"layout-a\">\n  {{ content }}\n</div>\n",
                ),
                (
                    "_includes/layout-b.liquid",
                    "---\nkey1: value1-b\nupstream: value2-b\ndaysPosted: 154\n---\n\n<div id=\"layout-b\">\n  {{ content }}\n</div>\n",
                ),
            ],
        ),
    )
    .unwrap();
    let data = tmpl.get_data().unwrap();
    assert_eq!(data.get("layout").as_str(), Some("layout-a"));
    assert_eq!(data.get("key1").as_str(), Some("value1"));
    assert_eq!(data.get("daysPosted").as_f64(), Some(152.0));
    let pages = render_template(&tmpl).unwrap();
    let squashed: String = pages[0]
        .content
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert_eq!(
        squashed,
        "<div id=\"layout-b\"> <div id=\"layout-a\"> <p>value2-a</p> </div> </div>"
    );
}
