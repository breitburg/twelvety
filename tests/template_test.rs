//! Port of upstream `test/TemplateTest.js`.
//!
//! Skipped (JS-only features):
//! - "Test raw front matter from template (js)": JavaScript front matter.
//! - "Fileslug in an 12ty.js template Issue #588", "Override base templating engine from
//!   markdown to 12ty.js, then markdown", "Throws a Premature Template Content Error (12ty.js)": 12ty.js templates.
//! - The "Data Cascade …" and "Local data inherits tags string" tests use JS data file fixtures;
//!   they run against scratch copies of the fixtures with equivalent JSON data files.
//! - "Front matter date with quotes (liquid|njk), issue #258": custom js-yaml loader via
//!   `setFrontMatterParsingOptions({ engines })` (ported as the default YAML timestamp behavior instead).
//! - "Recognizes a Premature Template Content Error wrapped by a Nunjucks filter (dev: false)":
//!   Nunjucks `dev` environment option is JS-only.
//! - "Premature Template Content Error detection handles cycles": JS error `cause` cycles cannot occur
//!   with `anyhow` chains.
//! - "Make sure layout cache takes new changes during watch (nunjucks|liquid)": needs the
//!   `buildawesome#templatemodified` event / `internalTriggerTemplateModifiedPath` API (not ported).
//! - "Add Extension via Configuration (txt file)": `getExtensionEntries`/`isFileRelevantToThisTemplate`
//!   are not part of the ported `Template` API.
//! - "Test a single asynchronous transform", "Test multiple asynchronous transforms": async transforms
//!   are plain synchronous closures in the port (ported as synchronous transforms).

mod template_support;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use template_support::*;
use twelvety::errors::is_premature_template_content_error;
use twelvety::page_entry::take_premature_use;
use twelvety::template_content::RenderOptions;
use twelvety::testing::{TestOptions, get_context, get_new_template, json, template_with_context};
use twelvety::value::to_js_date_string;
use twelvety::Value;

const TEMPLATE_FRONT_MATTER: &str = "---
key1: value1
key2: value2
key3: value3
---
c:{{ key1 }}:{{ key2 }}:{{ key3 }}";

const TEMPLATE_WITH_LAYOUT_CONTENT: &str = "---
layout: defaultLayoutLayoutContent
keymain: valuemain
title: 'Font Aliasing, or How to Rename a Font in CSS'
permalink: /rename-font/
---
<p>Hello.</p>
";

const HTML_TEMPLATE: &str = "<html>
<body>
\tThis is an html template.
</body>
</html>";

const LAYOUT_DATA: &str = "---
keylayout: valuelayout
postRank: 4
daysPosted: 152
yearsPosted: 0.4
---

<div id=\"layout\">
  {{ content }}
</div>";

const PAGINATED: &str = "---
pagination:
  data: testdata
  size: 4
testdata:
 - item1
 - item2
 - item3
 - item4
 - item5
 - item6
 - item7
 - item8
---
<ol>{% for item in pagination.items %}<li>{{ item }}</li>{% endfor %}</ol>
";

fn custom_front_matter(name: &str) -> &'static str {
    match name {
        "template-excerpt-comment.njk" => "---\nfront: hello\n---\nThis is an excerpt.\n<!-- excerpt -->\nThis is content.",
        "template-newline1.njk" => "---\nfront: hello\n---\nThis is an excerpt.---\nThis is content.",
        "template-newline2.njk" => "---\nfront: hello\n---\nThis is an excerpt.---This is content.",
        "template-newline3.njk" => "---\nfront: hello\n---\nThis is an excerpt.\n---This is content.",
        "template-toml.njk" => "---toml\nfront = \"hello\"\n---\nThis is content.",
        "template.njk" => "---\nfront: hello\n---\nThis is an excerpt.\n---\nThis is content.",
        _ => unreachable!(),
    }
}

#[test]
fn get_template_sub_folder() {
    let tmpl = virtual_template("./test/stubs/template.liquid", "./test/stubs/", "./dist", &[("template.liquid", "")]);
    assert_eq!(tmpl.get_template_subfolder(), "");
}

#[test]
fn get_template_sub_folder_output_is_a_subdir_of_input() {
    let tmpl = virtual_template(
        "./test/stubs/template.liquid",
        "./test/stubs/",
        "./test/stubs/_site",
        &[("template.liquid", "")],
    );
    assert_eq!(tmpl.get_template_subfolder(), "");
}

#[test]
fn output_path_maps_to_an_html_file() {
    let tmpl = virtual_template("./test/stubs/template.liquid", "./test/stubs/", "./dist", &[("template.liquid", "")]);
    assert_eq!(tmpl.content.input_dir(), "./test/stubs/");
    assert_eq!(tmpl.content.output_dir(), "./dist/");
    assert_eq!(tmpl.get_template_subfolder(), "");
    let data = tmpl.get_data().unwrap();
    assert_eq!(output_path(&tmpl, &data).as_str(), Some("./dist/template/index.html"));
}

#[test]
fn subfolder_outputs_to_a_subfolder() {
    let tmpl = virtual_template(
        "./test/stubs/subfolder/subfolder.liquid",
        "./test/stubs/",
        "./dist",
        &[("subfolder/subfolder.liquid", "subfolder")],
    );
    let data = tmpl.get_data().unwrap();
    assert_eq!(tmpl.parsed.dir, "./test/stubs/subfolder");
    assert_eq!(tmpl.get_template_subfolder(), "subfolder");
    assert_eq!(output_path(&tmpl, &data).as_str(), Some("./dist/subfolder/subfolder/index.html"));
}

#[test]
fn subfolder_outputs_to_double_subfolder() {
    let tmpl = virtual_template(
        "./test/stubs/subfolder/subfolder/subfolder.liquid",
        "./test/stubs/",
        "./dist",
        &[("subfolder/subfolder/subfolder.liquid", "")],
    );
    let data = tmpl.get_data().unwrap();
    assert_eq!(tmpl.parsed.dir, "./test/stubs/subfolder/subfolder");
    assert_eq!(tmpl.get_template_subfolder(), "subfolder/subfolder");
    assert_eq!(
        output_path(&tmpl, &data).as_str(),
        Some("./dist/subfolder/subfolder/subfolder/index.html")
    );
}

#[test]
fn html_output_to_input_dir_not_index() {
    let tmpl = virtual_template("./test/stubs/testing.html", "./test/stubs", "./test/stubs", &[("testing.html", HTML_TEMPLATE)]);
    let data = tmpl.get_data().unwrap();
    assert_eq!(output_path(&tmpl, &data).as_str(), Some("./test/stubs/testing/index.html"));
}

#[test]
fn html_output_to_input_dir_is_index() {
    let tmpl = virtual_template("./test/stubs/index.html", "./test/stubs", "./test/stubs", &[("index.html", HTML_TEMPLATE)]);
    let data = tmpl.get_data().unwrap();
    assert_eq!(output_path(&tmpl, &data).as_str(), Some("./test/stubs/index.html"));
}

#[test]
fn html_output_to_input_dir_is_index_subfolder() {
    let tmpl = virtual_template(
        "./test/stubs/subfolder/index.html",
        "./test/stubs",
        "./test/stubs",
        &[("subfolder/index.html", "")],
    );
    let data = tmpl.get_data().unwrap();
    assert_eq!(output_path(&tmpl, &data).as_str(), Some("./test/stubs/subfolder/index.html"));
}

#[test]
fn raw_front_matter_yaml() {
    let tmpl = virtual_template(
        "./test/stubs/templateFrontMatter.liquid",
        "./test/stubs/",
        "./dist",
        &[("templateFrontMatter.liquid", TEMPLATE_FRONT_MATTER)],
    );
    assert!(!tmpl.content.get_input_content().unwrap().is_empty());
    let fm = tmpl.content.read().unwrap();
    assert_eq!(fm.data.get("key1").as_str(), Some("value1"));
    assert_eq!(fm.data.get("key3").as_str(), Some("value3"));

    let data = tmpl.get_data().unwrap();
    assert_eq!(data.get("key1").as_str(), Some("value1"));
    assert_eq!(data.get("key3").as_str(), Some("value3"));

    let pages = rendered_templates(&tmpl).unwrap();
    assert_eq!(pages[0].template_content().unwrap().trim(), "c:value1:value2:value3");
}

#[test]
fn raw_front_matter_json() {
    let tmpl = virtual_template(
        "./test/stubs/templateFrontMatterJson.liquid",
        "./test/stubs/",
        "./dist",
        &[(
            "templateFrontMatterJson.liquid",
            "---json\n{\n  \"key1\": \"value1\",\n  \"key2\": \"value2\",\n  \"key3\": \"value3\"\n}\n---\nc:{{ key1 }}:{{ key2 }}:{{ key3 }}",
        )],
    );
    let fm = tmpl.content.read().unwrap();
    assert_eq!(fm.data.get("key1").as_str(), Some("value1"));
    assert_eq!(fm.data.get("key3").as_str(), Some("value3"));

    let data = tmpl.get_data().unwrap();
    assert_eq!(data.get("key1").as_str(), Some("value1"));
    assert_eq!(data.get("key3").as_str(), Some("value3"));

    let pages = rendered_templates(&tmpl).unwrap();
    assert_eq!(pages[0].template_content().unwrap().trim(), "c:value1:value2:value3");
}

#[test]
fn get_data_works() {
    let tmpl = virtual_template(
        "./test/stubs/templateFrontMatter.liquid",
        "./test/stubs/",
        "./dist",
        &[("templateFrontMatter.liquid", TEMPLATE_FRONT_MATTER)],
    );
    let data = tmpl.get_data().unwrap();
    assert_eq!(data.get("key1").as_str(), Some("value1"));
    assert_eq!(data.get("key3").as_str(), Some("value3"));
}

fn check_one_layout(path: &str, layout_name: &str, list: &[(&str, &str)]) {
    let tmpl = virtual_template(path, "./test/stubs/", "dist", list);
    assert_eq!(tmpl.content.read().unwrap().data.get("layout").as_str(), Some(layout_name));
    let data = tmpl.get_data().unwrap();
    assert_eq!(data.get("layout").as_str(), Some(layout_name));
    assert_eq!(squash(&render(&tmpl).unwrap()), "<div id=\"layout\"> <p>Hello.</p> </div>");
    assert_eq!(data.get("keymain").as_str(), Some("valuemain"));
    assert_eq!(data.get("keylayout").as_str(), Some("valuelayout"));
}

#[test]
fn one_layout_using_new_content_var() {
    let layout = format!("{LAYOUT_DATA}\n");
    check_one_layout(
        "./test/stubs/templateWithLayoutKey.liquid",
        "defaultLayout",
        &[
            (
                "templateWithLayoutKey.liquid",
                "---\nlayout: defaultLayout\nkeymain: valuemain\ntitle: 'Font Aliasing, or How to Rename a Font in CSS'\npermalink: /rename-font/\n---\n\n<p>Hello.</p>",
            ),
            ("_includes/defaultLayout.liquid", &layout),
        ],
    );
}

#[test]
fn one_layout_using_content() {
    check_one_layout(
        "./test/stubs/templateWithLayoutContent.liquid",
        "defaultLayoutLayoutContent",
        &[
            ("templateWithLayoutContent.liquid", TEMPLATE_WITH_LAYOUT_CONTENT),
            ("_includes/defaultLayoutLayoutContent.liquid", LAYOUT_DATA),
        ],
    );
}

#[test]
fn one_layout_layouts_disabled() {
    let tmpl = virtual_template(
        "./test/stubs/templateWithLayoutContent.liquid",
        "./test/stubs/",
        "dist",
        &[
            ("templateWithLayoutContent.liquid", TEMPLATE_WITH_LAYOUT_CONTENT),
            ("_includes/defaultLayoutLayoutContent.liquid", LAYOUT_DATA),
        ],
    );
    let mut data = tmpl.get_data().unwrap();
    assert_eq!(data.get("layout").as_str(), Some("defaultLayoutLayoutContent"));
    let pages = tmpl.get_templates(&mut data).unwrap();
    assert_eq!(tmpl.render_page_entry_without_layout(&pages[0]).unwrap().trim(), "<p>Hello.</p>");
    assert_eq!(data.get("keymain").as_str(), Some("valuemain"));
    assert_eq!(data.get("keylayout").as_str(), Some("valuelayout"));
}

#[test]
fn one_layout_liquid_test() {
    check_one_layout(
        "./test/stubs/templateWithLayout.liquid",
        "layoutLiquid.liquid",
        &[
            (
                "templateWithLayout.liquid",
                "---\nlayout: layoutLiquid.liquid\nkeymain: valuemain\ntitle: 'Font Aliasing, or How to Rename a Font in CSS'\npermalink: /rename-font/\n---\n\n<p>Hello.</p>",
            ),
            (
                "_includes/layoutLiquid.liquid",
                "---\nkeylayout: valuelayout\n---\n\n<div id=\"layout\">\n  {{ content }}\n</div>\n",
            ),
        ],
    );
}

#[test]
fn two_layouts() {
    let tmpl = virtual_template(
        "./test/stubs/templateTwoLayouts.liquid",
        "./test/stubs/",
        "dist",
        &[
            (
                "templateTwoLayouts.liquid",
                "---\nlayout: layout-a\nkey1: value1\ntitle: 'Font Aliasing, or How to Rename a Font in CSS'\npermalink: /rename-font/\npostRank: 4\ndaysPosted: 152\nyearsPosted: 0.4\n---\n\n<p>{{ upstream }}</p>",
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
    );
    assert_eq!(tmpl.content.read().unwrap().data.get("layout").as_str(), Some("layout-a"));
    let data = tmpl.get_data().unwrap();
    assert_eq!(data.get("layout").as_str(), Some("layout-a"));
    assert_eq!(data.get("key1").as_str(), Some("value1"));
    assert_eq!(
        squash(&render(&tmpl).unwrap()),
        "<div id=\"layout-b\"> <div id=\"layout-a\"> <p>value2-a</p> </div> </div>"
    );
    assert_eq!(data.get("daysPosted").as_f64(), Some(152.0));
}

#[test]
fn liquid_template() {
    let tmpl = virtual_template(
        "./test/stubs/formatTest.liquid",
        "./test/stubs/",
        "dist",
        &[("formatTest.liquid", "---\nname: zach\n---\n<p>{{name | capitalize}}</p>")],
    );
    assert_eq!(render(&tmpl).unwrap(), "<p>Zach</p>");
}

#[test]
fn liquid_template_with_include() {
    let tmpl = get_new_template("./test/stubs/includer.liquid", TestOptions::new("./test/stubs/", "dist")).unwrap();
    assert_eq!(render(&tmpl).unwrap().trim(), "<p>This is an include.</p>");
}

#[test]
fn permalink_output_directory() {
    let tmpl = virtual_template(
        "./test/stubs/permalinked.liquid",
        "./test/stubs/",
        "./dist",
        &[("permalinked.liquid", "---\npermalink: permalinksubfolder/index.html\n---\nCurrent url: {{ permalink }}")],
    );
    let data = tmpl.get_data().unwrap();
    assert_eq!(output_path(&tmpl, &data).as_str(), Some("./dist/permalinksubfolder/index.html"));
}

#[test]
fn permalink_output_directory_from_layout() {
    let tmpl = virtual_template(
        "./test/stubs/permalink-in-layout.liquid",
        "./test/stubs/",
        "./dist",
        &[
            ("permalink-in-layout.liquid", "---\nlayout: permalink-in-layout/layout.liquid\n---\nCurrent url: {{ permalink }}"),
            ("_includes/permalink-in-layout/layout.liquid", "---\npermalink: hello/index.html\n---\n{{ content }}"),
        ],
    );
    let data = tmpl.get_data().unwrap();
    assert_eq!(output_path(&tmpl, &data).as_str(), Some("./dist/hello/index.html"));
}

#[test]
fn permalink_output_directory_from_layout_fileslug() {
    let tmpl = virtual_template(
        "./test/stubs/permalink-in-layout-fileslug.liquid",
        "./test/stubs/",
        "./dist",
        &[
            (
                "permalink-in-layout-fileslug.liquid",
                "---\nlayout: permalink-in-layout/layout-fileslug.liquid\n---\nCurrent url: {{ permalink }}",
            ),
            (
                "_includes/permalink-in-layout/layout-fileslug.liquid",
                "---\npermalink: test/{{ page.fileSlug }}/\n---\n{{ content }}",
            ),
        ],
    );
    let data = tmpl.get_data().unwrap();
    assert_eq!(
        output_path(&tmpl, &data).as_str(),
        Some("./dist/test/permalink-in-layout-fileslug/index.html")
    );
}

#[test]
fn layout_from_template_data_file_that_has_a_permalink_issue_121() {
    let tmpl = get_new_template(
        "./test/stubs/permalink-data-layout/test.njk",
        TestOptions::new("test/stubs", "dist"),
    )
    .unwrap();
    let data = tmpl.get_data().unwrap();
    let pages = rendered_templates(&tmpl).unwrap();
    assert_eq!(pages[0].template_content().unwrap(), "Wrapper:Test 1:test");
    assert_eq!(output_path(&tmpl, &data).as_str(), Some("./dist/test/index.html"));
}

/// The JS/TS data file paths upstream lists are not searched by the port.
fn json_data_paths(dirs: &[(&str, &str)]) -> Vec<String> {
    let mut out = vec![];
    for (dir, name) in dirs {
        out.push(format!("{dir}/{name}.json"));
        out.push(format!("{dir}/{name}.12tydata.json"));
        out.push(format!("{dir}/{name}.data.json"));
    }
    out
}

fn local_data_paths(tmpl: &twelvety::template::Template) -> Vec<String> {
    tmpl.ctx()
        .template_data
        .get_local_data_paths(&tmpl.input_path)
        .into_iter()
        .filter(|p| p.ends_with(".json"))
        .collect()
}

#[test]
fn local_template_data_file_import_without_a_global_data_json() {
    let tmpl = get_new_template("./test/stubs/component/component.njk", TestOptions::new("test/stubs", "dist")).unwrap();
    let data = tmpl.get_data().unwrap();
    assert_eq!(
        local_data_paths(&tmpl),
        json_data_paths(&[("./test/stubs", "stubs"), ("./test/stubs/component", "component")])
    );
    assert_eq!(data.get("localdatakey1").as_str(), Some("localdatavalue1"));
    assert_eq!(render(&tmpl).unwrap(), "localdatavalue1");
}

#[test]
fn local_template_data_file_import_two_subdirectories_deep() {
    let tmpl = get_new_template(
        "./test/stubs/firstdir/seconddir/component.njk",
        TestOptions::new("test/stubs", "dist"),
    )
    .unwrap();
    assert_eq!(
        local_data_paths(&tmpl),
        json_data_paths(&[
            ("./test/stubs", "stubs"),
            ("./test/stubs/firstdir", "firstdir"),
            ("./test/stubs/firstdir/seconddir", "seconddir"),
            ("./test/stubs/firstdir/seconddir", "component"),
        ])
    );
}

#[test]
fn posts_inherits_local_json_layouts() {
    let tmpl = get_new_template("./test/stubs/posts/post1.njk", TestOptions::new("test/stubs", "dist")).unwrap();
    assert_eq!(
        local_data_paths(&tmpl),
        json_data_paths(&[
            ("./test/stubs", "stubs"),
            ("./test/stubs/posts", "posts"),
            ("./test/stubs/posts", "post1"),
        ])
    );
    let td = &tmpl.ctx().template_data;
    let local = td.get_template_directory_data(&tmpl.input_path).unwrap();
    assert_eq!(local.get("layout").as_str(), Some("mylocallayout.njk"));
    assert!(td.get_global_data().unwrap().get("pkg").is_truthy());
    assert_eq!(render(&tmpl).unwrap().trim(), "<div id=\"locallayout\">Post1\n</div>");
}

#[test]
fn template_and_folder_name_are_the_same() {
    let tmpl = get_new_template("./test/stubs/posts/posts.njk", TestOptions::new("test/stubs", "dist")).unwrap();
    assert_eq!(
        local_data_paths(&tmpl),
        json_data_paths(&[("./test/stubs", "stubs"), ("./test/stubs/posts", "posts")])
    );
    let td = &tmpl.ctx().template_data;
    let local = td.get_template_directory_data(&tmpl.input_path).unwrap();
    assert_eq!(local.get("layout").as_str(), Some("mylocallayout.njk"));
    assert!(td.get_global_data().unwrap().get("pkg").is_truthy());
    assert_eq!(render(&tmpl).unwrap().trim(), "<div id=\"locallayout\">Posts\n</div>");
}

#[test]
fn clone_the_template() {
    let tmpl = virtual_template(
        "./test/stubs/default.liquid",
        "./test/stubs/",
        "./dist",
        &[("default.liquid", "{{ \"hi\" }}")],
    );
    let data = tmpl.get_data().unwrap();
    let cloned = template_with_context(&tmpl.input_path, tmpl.ctx());
    let cloned_data = cloned.get_data().unwrap();
    assert_eq!(output_path(&tmpl, &data).as_str(), Some("./dist/default/index.html"));
    assert_eq!(output_path(&cloned, &cloned_data).as_str(), Some("./dist/default/index.html"));
    assert!(Arc::ptr_eq(&cloned.ctx().extension_map, &tmpl.ctx().extension_map));
}

#[test]
fn get_rendered_data_has_all_the_page_variables() {
    let tmpl = virtual_template("./test/stubs/template.liquid", "./test/stubs/", "./dist", &[("template.liquid", "")]);
    let data = rendered_data(&tmpl, 0);
    let page = data.get("page");
    assert_eq!(page.get("url").as_str(), Some("/template/"));
    assert_eq!(page.get("fileSlug").as_str(), Some("template"));
    assert_eq!(page.get("filePathStem").as_str(), Some("/template"));
    assert_eq!(page.get("relativeInputPath").as_str(), Some("template.liquid"));
    assert!(matches!(page.get("date"), Value::Date(_)));
    assert_eq!(page.get("inputPath").as_str(), Some("./test/stubs/template.liquid"));
    assert_eq!(page.get("outputPath").as_str(), Some("./dist/template/index.html"));
}

fn page_date_string(path: &str) -> (String, chrono::DateTime<chrono::Utc>) {
    let tmpl = get_new_template(path, TestOptions::new("./test/stubs/", "./dist")).unwrap();
    let data = tmpl.get_data().unwrap();
    let Value::Date(date) = data.get("page").get("date") else {
        panic!("page.date is not a Date");
    };
    let pages = rendered_templates(&tmpl).unwrap();
    (pages[0].template_content().unwrap().trim().to_string(), date)
}

#[test]
fn issue_603_page_date_liquid() {
    let (content, date) = page_date_string("./test/stubs/pagedate.liquid");
    assert_eq!(content, to_js_date_string(&date));
}

#[test]
fn issue_603_page_date_nunjucks() {
    let (content, date) = page_date_string("./test/stubs/pagedate.njk");
    assert_eq!(content, to_js_date_string(&date));
}

#[test]
fn issue_603_page_date_to_utc_string_nunjucks() {
    let (content, date) = page_date_string("./test/stubs/pagedateutc.njk");
    assert_eq!(content, date.format("%a, %d %b %Y %H:%M:%S GMT").to_string());
}

#[test]
fn get_templates_data_has_all_the_root_and_page_variables() {
    let tmpl = virtual_template("./test/stubs/template.liquid", "./test/stubs/", "./dist", &[("template.liquid", "")]);
    let mut data = tmpl.get_data().unwrap();
    let templates = tmpl.get_templates(&mut data).unwrap();
    let t = &templates[0];
    assert_eq!(t.url().as_str(), Some("/template/"));
    assert_eq!(t.file_slug, "template");
    assert_eq!(t.file_path_stem, "/template");
    assert!(matches!(t.date(), Value::Date(_)));
    assert_eq!(t.input_path, "./test/stubs/template.liquid");
    assert_eq!(t.output_path().as_str(), Some("./dist/template/index.html"));

    let page = t.data().get("page");
    assert_eq!(page.get("url").as_str(), Some("/template/"));
    assert_eq!(page.get("fileSlug").as_str(), Some("template"));
    assert!(matches!(page.get("date"), Value::Date(_)));
    assert_eq!(page.get("inputPath").as_str(), Some("./test/stubs/template.liquid"));
    assert_eq!(page.get("outputPath").as_str(), Some("./dist/template/index.html"));
}

#[test]
fn get_rendered_templates_data_has_all_the_page_variables() {
    let tmpl = virtual_template("./test/stubs/template.liquid", "./test/stubs/", "./dist", &[("template.liquid", "")]);
    let templates = rendered_templates(&tmpl).unwrap();
    let page = templates[0].data().get("page");
    assert_eq!(page.get("url").as_str(), Some("/template/"));
    assert_eq!(page.get("fileSlug").as_str(), Some("template"));
    assert_eq!(templates[0].file_path_stem, "/template");
    assert!(matches!(page.get("date"), Value::Date(_)));
    assert_eq!(page.get("inputPath").as_str(), Some("./test/stubs/template.liquid"));
    assert_eq!(page.get("outputPath").as_str(), Some("./dist/template/index.html"));
}

#[test]
fn get_rendered_data_has_good_slug_empty_index() {
    let tmpl = virtual_template("./test/stubs/index.liquid", "./test/stubs/", "./dist", &[("index.liquid", "")]);
    let data = rendered_data(&tmpl, 0);
    assert_eq!(data.get("page").get("fileSlug").as_str(), Some(""));
    assert_eq!(data.get("page").get("filePathStem").as_str(), Some("/index"));
}

#[test]
fn get_rendered_data_has_good_slug() {
    let tmpl = virtual_template(
        "./test/stubs/includer.liquid",
        "./test/stubs/",
        "./dist",
        &[("includer.liquid", "<p>{% include 'included' %}</p>\n")],
    );
    let data = rendered_data(&tmpl, 0);
    assert_eq!(data.get("page").get("fileSlug").as_str(), Some("includer"));
    assert_eq!(data.get("page").get("filePathStem").as_str(), Some("/includer"));
}

fn render_override(path: &str, key: &str, content: &str) -> anyhow::Result<String> {
    let tmpl = virtual_template(path, "./test/stubs/", "./dist", &[(key, content)]);
    render(&tmpl).map(|s| s.trim().to_string())
}

#[test]
fn override_liquid_to_njk() {
    assert_eq!(
        render_override(
            "./test/stubs/overrides/test-njk.liquid",
            "overrides/test-njk.liquid",
            "---\ntemplateEngineOverride: njk\ntitle: My Title\n---\n{{ title }}\n"
        )
        .unwrap(),
        "My Title"
    );
}

#[test]
fn override_liquid_to_md() {
    assert_eq!(
        render_override(
            "./test/stubs/overrides/test-md.liquid",
            "overrides/test-md.liquid",
            "---\ntemplateEngineOverride: md\n---\n# My Title\n"
        )
        .unwrap(),
        "<h1>My Title</h1>"
    );
}

#[test]
fn override_md_to_njk_md() {
    assert_eq!(
        render_override(
            "./test/stubs/overrides/test-multiple.md",
            "overrides/test-multiple.md",
            "---\ntemplateEngineOverride: njk,md\ntitle: My Title\n---\n\n# {{ title }}\n"
        )
        .unwrap(),
        "<h1>My Title</h1>"
    );
}

#[test]
fn override_njk_to_liquid_md() {
    assert_eq!(
        render_override(
            "./test/stubs/overrides/test-multiple2.njk",
            "overrides/test-multiple2.njk",
            "---\ntemplateEngineOverride: liquid,md\ntitle: My Title\n---\n# {{ title }}\n"
        )
        .unwrap(),
        "<h1>My Title</h1>"
    );
}

#[test]
fn override_html_to_njk() {
    assert_eq!(
        render_override(
            "./test/stubs/overrides/test.html",
            "overrides/test.html",
            "---\ntemplateEngineOverride: njk\ntitle: My Title\n---\n<h2>{{ title }}</h2>\n"
        )
        .unwrap(),
        "<h2>My Title</h2>"
    );
}

#[test]
fn override_html_to_nothing() {
    assert_eq!(
        render_override(
            "./test/stubs/overrides/test-empty.html",
            "overrides/test-empty.html",
            "---\ntemplateEngineOverride:\ntitle: My Title\n---\n<h2>{{ title }}</h2>\n"
        )
        .unwrap(),
        "<h2>{{ title }}</h2>"
    );
}

#[test]
fn override_should_error_with_bad_string() {
    assert!(
        render_override(
            "./test/stubs/overrides/test-error.njk",
            "overrides/test-error.njk",
            "---\ntemplateEngineOverride: liquid,njk\ntitle: My Title\n---\n# {{ title }}\n"
        )
        .is_err()
    );
}

#[test]
fn override_bypasses_markdown() {
    assert_eq!(
        render_override(
            "./test/stubs/overrides/test-bypass.md",
            "overrides/test-bypass.md",
            "---\ntemplateEngineOverride: njk\ntitle: My Title\n---\n\n# {{ title }}\n"
        )
        .unwrap(),
        "# My Title"
    );
}

#[test]
fn override_to_nothing() {
    assert_eq!(
        render_override(
            "./test/stubs/overrides/test-empty.md",
            "overrides/test-empty.md",
            "---\ntemplateEngineOverride:\ntitle: My Title\n---\n\n# {{ title }}\n"
        )
        .unwrap(),
        "# {{ title }}"
    );
}

const ENGINE_OVERRIDES_LAYOUT: &str = "---\nlayoutkey: layoutvalue\n---\n<div id=\"{{layoutkey}}\">{{ content | safe }}</div>\n";

#[test]
fn override_njk_to_liquid_with_njk_layout() {
    let tmpl = virtual_template(
        "./test/stubs/overrides/layout.njk",
        "./test/stubs/",
        "./dist",
        &[
            (
                "overrides/layout.njk",
                "---\ntemplateEngineOverride: liquid\ntitle: My Title\nlayout: layouts/engineOverrides.njk\n---\n<h2>{{ title | size }}</h2>",
            ),
            ("_includes/layouts/engineOverrides.njk", ENGINE_OVERRIDES_LAYOUT),
        ],
    );
    assert_eq!(render(&tmpl).unwrap().trim(), "<div id=\"layoutvalue\"><h2>8</h2></div>");
}

#[test]
fn override_njk_to_nothing_with_njk_layout() {
    let tmpl = virtual_template(
        "./test/stubs/overrides/layoutfalse.njk",
        "./test/stubs/",
        "./dist",
        &[
            (
                "overrides/layoutfalse.njk",
                "---\ntemplateEngineOverride: false\ntitle: My Title\nlayout: layouts/engineOverrides.njk\n---\n<h2><%= title %></h2>",
            ),
            ("_includes/layouts/engineOverrides.njk", ENGINE_OVERRIDES_LAYOUT),
        ],
    );
    assert_eq!(render(&tmpl).unwrap().trim(), "<div id=\"layoutvalue\"><h2><%= title %></h2></div>");
}

#[test]
fn markdown_source_with_njk_layout_does_not_render_markdown_in_layout() {
    let tmpl = virtual_template(
        "./test/stubs/overrides/test.md",
        "./test/stubs/",
        "./dist",
        &[
            ("overrides/test.md", "---\ntitle: My Title\nlayout: layouts/engineOverridesMd.njk\n---\n\n# {{ title }}\n"),
            (
                "_includes/layouts/engineOverridesMd.njk",
                "---\nlayoutkey: layoutvalue\n---\n# Layout header\n\n<div id=\"{{layoutkey}}\">{{ content | safe }}</div>",
            ),
        ],
    );
    assert_eq!(
        render(&tmpl).unwrap().trim(),
        "# Layout header\n\n<div id=\"layoutvalue\"><h1>My Title</h1>\n</div>"
    );
}

fn render_direct(tmpl: &twelvety::template::Template, s: &str, data: &Value) -> String {
    tmpl.content
        .render_string(
            s,
            data,
            RenderOptions {
                template_only: true,
                kind: "Permalink",
                step_inputs: None,
            },
        )
        .unwrap()
}

#[test]
fn render_direct_on_markdown_permalink_should_not_render_markdown() {
    let tmpl = virtual_template(
        "./test/stubs/permalink-markdown.md",
        "./test/stubs/",
        "./dist",
        &[(
            "permalink-markdown.md",
            "---\ntitle: My Title\npermalink: /news/my-test-file/index.html\n---\n\n# <%= title %>\n",
        )],
    );
    let data = tmpl.get_data().unwrap();
    assert_eq!(
        render_direct(&tmpl, "/news/my-test-file/index.html", &Value::object()),
        "/news/my-test-file/index.html"
    );
    assert_eq!(raw_output_path(&tmpl, &data).as_deref(), Some("/news/my-test-file/index.html"));
}

#[test]
fn render_direct_on_markdown_permalink_with_variable() {
    let tmpl = virtual_template(
        "./test/stubs/permalink-markdown-var.md",
        "./test/stubs/",
        "./dist",
        &[(
            "permalink-markdown-var.md",
            "---\ntitle: My Title\npermalink: /news/{{ title | slugify }}/index.html\n---\n\n# <%= title %>\n",
        )],
    );
    let data = tmpl.get_data().unwrap();
    assert_eq!(
        render_direct(&tmpl, "/news/{{ slug }}/index.html", &json(r#"{"slug":"my-title"}"#)),
        "/news/my-title/index.html"
    );
    assert_eq!(raw_output_path(&tmpl, &data).as_deref(), Some("/news/my-title/index.html"));
}

#[test]
fn render_direct_on_markdown_permalink_has_override() {
    let tmpl = virtual_template(
        "./test/stubs/permalink-markdown-override.md",
        "./test/stubs/",
        "./dist",
        &[(
            "permalink-markdown-override.md",
            "---\ntitle: My Title\npermalink: /news/my-test-file/index.html\ntemplateEngineOverride: html,md\n---\n\n# <%= title %>\n",
        )],
    );
    let data = tmpl.get_data().unwrap();
    assert_eq!(
        render_direct(&tmpl, "/news/my-test-file/index.html", &Value::object()),
        "/news/my-test-file/index.html"
    );
    assert_eq!(raw_output_path(&tmpl, &data).as_deref(), Some("/news/my-test-file/index.html"));
}

/* Transforms */

fn transform_template<F>(path: &str, input: &str, output: &str, list: &[(&str, &str)], config: F) -> Arc<twelvety::template::Template>
where
    F: Fn(&mut twelvety::UserConfig) -> anyhow::Result<()> + Send + Sync + 'static,
{
    get_new_template(path, virtual_options(input, output, templates(list), config)).unwrap()
}

#[test]
fn test_a_transform() {
    let calls = Arc::new(AtomicUsize::new(0));
    let c = calls.clone();
    let tmpl = transform_template(
        "./test/stubs/template.liquid",
        "./test/stubs/",
        "./test/stubs/_site",
        &[("template.liquid", "")],
        move |cfg| {
            let c = c.clone();
            cfg.add_transform("transformName", move |ctx, _content| {
                assert!(ctx.output_path.to_output_string().ends_with(".html"));
                assert!(!ctx.input_path.is_empty());
                c.fetch_add(1, Ordering::SeqCst);
                Ok("OVERRIDE BY A TRANSFORM".into())
            });
            Ok(())
        },
    );
    let renders = complete_render(&tmpl).unwrap();
    assert_eq!(renders[0], "OVERRIDE BY A TRANSFORM");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn test_a_transform_with_pages() {
    let calls = Arc::new(AtomicUsize::new(0));
    let c = calls.clone();
    let tmpl = transform_template(
        "./test/stubs/transform-pages/template.njk",
        "./test/stubs/",
        "./test/stubs/_site",
        &[("transform-pages/template.njk", PAGINATED)],
        move |cfg| {
            let c = c.clone();
            cfg.add_transform("transformName", move |ctx, content| {
                assert!(!content.is_empty());
                assert!(ctx.output_path.to_output_string().ends_with(".html"));
                c.fetch_add(1, Ordering::SeqCst);
                Ok("OVERRIDE BY A TRANSFORM".into())
            });
            Ok(())
        },
    );
    let renders = complete_render(&tmpl).unwrap();
    assert_eq!(renders[0], "OVERRIDE BY A TRANSFORM");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn test_a_transform_with_a_layout() {
    let calls = Arc::new(AtomicUsize::new(0));
    let c = calls.clone();
    let tmpl = transform_template(
        "./test/stubs-virtual-nowrite/transform-layout/transform-layout.njk",
        "./test/stubs-virtual-nowrite/",
        "./test/stubs-virtual-nowrite/_site",
        &[
            ("transform-layout/transform-layout.njk", "---\nlayout: layout.njk\n---\nThis is content."),
            ("_includes/layout.njk", "<html><body>{{ content | safe }}</body></html>"),
        ],
        move |cfg| {
            let c = c.clone();
            cfg.add_transform("transformName", move |ctx, content| {
                assert_eq!(content, "<html><body>This is content.</body></html>");
                assert!(ctx.output_path.to_output_string().ends_with(".html"));
                c.fetch_add(1, Ordering::SeqCst);
                Ok("OVERRIDE BY A TRANSFORM".into())
            });
            Ok(())
        },
    );
    let renders = complete_render(&tmpl).unwrap();
    assert_eq!(renders[0], "OVERRIDE BY A TRANSFORM");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn test_multiple_transforms() {
    let tmpl = transform_template(
        "./test/stubs/template.liquid",
        "./test/stubs/",
        "./test/stubs/_site",
        &[("template.liquid", "")],
        |cfg| {
            cfg.add_transform("transformName1", |ctx, _| {
                assert!(ctx.output_path.to_output_string().ends_with("template/index.html"));
                Ok("lowercase transform".into())
            });
            cfg.add_transform("transformName2", |ctx, s| {
                assert!(ctx.output_path.to_output_string().ends_with("template/index.html"));
                Ok(s.to_uppercase())
            });
            Ok(())
        },
    );
    assert_eq!(complete_render(&tmpl).unwrap()[0], "LOWERCASE TRANSFORM");
}

#[test]
fn test_a_linter() {
    let calls = Arc::new(AtomicUsize::new(0));
    let c = calls.clone();
    let tmpl = transform_template(
        "./test/stubs/transform-pages/template.njk",
        "./test/stubs/",
        "./test/stubs/_site",
        &[("transform-pages/template.njk", PAGINATED)],
        move |cfg| {
            let c = c.clone();
            cfg.add_linter("linter", move |ctx, _content| {
                assert!(ctx.input_path.ends_with("template.njk"));
                assert!(ctx.output_path.to_output_string().ends_with("index.html"));
                c.fetch_add(1, Ordering::SeqCst);
                Ok(())
            });
            Ok(())
        },
    );
    complete_render(&tmpl).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn front_matter_tags_single() {
    let tmpl = virtual_template(
        "./test/stubs/templatetest-frontmatter/single.njk",
        "./test/stubs/",
        "dist",
        &[(
            "templatetest-frontmatter/single.njk",
            "---\ntags: single-tag\n---\n{% if \"single-tag\" in tags %}Has single-tag{% endif %}",
        )],
    );
    let fm = tmpl.content.get_front_matter_data().unwrap();
    assert_eq!(strs(&fm.get("tags")), ["single-tag"]);
    let data = tmpl.get_data().unwrap();
    assert_eq!(strs(&data.get("tags")), ["single-tag"]);
    let pages = rendered_templates(&tmpl).unwrap();
    assert_eq!(pages[0].template_content().unwrap().trim(), "Has single-tag");
}

#[test]
fn front_matter_tags_multiple() {
    let tmpl = virtual_template(
        "./test/stubs/templatetest-frontmatter/multiple.njk",
        "./test/stubs/",
        "dist",
        &[(
            "templatetest-frontmatter/multiple.njk",
            "---\ntags:\n  - multi-tag\n  - multi-tag-2\n---\n{% if \"multi-tag-2\" in tags %}Has multi-tag-2{% endif %}",
        )],
    );
    let fm = tmpl.content.get_front_matter_data().unwrap();
    assert_eq!(strs(&fm.get("tags")), ["multi-tag", "multi-tag-2"]);
    let data = tmpl.get_data().unwrap();
    assert_eq!(strs(&data.get("tags")), ["multi-tag", "multi-tag-2"]);
    let pages = rendered_templates(&tmpl).unwrap();
    assert_eq!(pages[0].template_content().unwrap().trim(), "Has multi-tag-2");
}

/// Port of "Front matter date with quotes (liquid), issue #258" (YAML timestamps are parsed by default)
#[test]
fn front_matter_date_liquid_issue_258() {
    let tmpl = virtual_template(
        "./test/stubs-virtual-nowrite/test.liquid",
        "./test/stubs-virtual-nowrite/",
        "dist",
        &[("test.liquid", "---\nmydate: 2009-04-15T12:34:34+01:00\n---\n{{ mydate | date: \"%Y-%m-%d\" }}")],
    );
    let data = tmpl.get_data().unwrap();
    let Value::Date(d) = data.get("mydate") else {
        panic!("mydate is not a date: {:?}", data.get("mydate"));
    };
    assert_eq!(twelvety::value::to_iso_string(&d), "2009-04-15T11:34:34.000Z");
    let pages = rendered_templates(&tmpl).unwrap();
    assert_eq!(pages[0].template_content().unwrap().trim(), "2009-04-15");
}

#[test]
fn throws_a_premature_template_content_error() {
    for (key, content) in [
        ("prematureTemplateContent/test.njk", "{{ sample.templateContent }}"),
        ("prematureTemplateContent/test.liquid", "{{ collections.all[0].templateContent }}"),
        ("prematureTemplateContent/test.md", "{{ sample.templateContent }}\n"),
    ] {
        let tmpl = virtual_template(
            &format!("./test/stubs/{key}"),
            "./test/stubs/",
            "./test/stubs/_site",
            &[(key, content)],
        );
        let mut data = tmpl.get_data().unwrap();
        let entries = tmpl.get_templates(&mut data).unwrap();
        take_premature_use();
        assert!(entries[0].as_value().get("templateContent").is_undefined());
        assert!(take_premature_use().is_some(), "{key}");
    }
}

/// `getPrematureErrorFromRendering`: the sample is an unrendered page entry
fn premature_error_from_rendering<F>(input_path: &str, content: &str, config: F) -> anyhow::Error
where
    F: Fn(&mut twelvety::UserConfig) -> anyhow::Result<()> + Send + Sync + 'static,
{
    let key = input_path.trim_start_matches("./test/stubs/");
    let options = virtual_options("./test/stubs/", "./test/stubs/_site", templates(&[(key, content), ("sample.njk", "")]), config);
    let ctx = get_context(options).unwrap();
    let sample_tmpl = template_with_context("./test/stubs/sample.njk", &ctx);
    let mut sample_data = sample_tmpl.get_data().unwrap();
    let sample = sample_tmpl.get_templates(&mut sample_data).unwrap()[0].as_value();

    let tmpl = template_with_context(input_path, &ctx);
    let mut data = tmpl.get_data().unwrap();
    data.set("sample", sample.clone());
    data.set("samples", Value::array(vec![sample.clone()]));
    let mut collections = Value::object();
    collections.set("all", Value::array(vec![sample]));
    data.set("collections", collections);
    let entries = tmpl.get_templates(&mut data).unwrap();
    tmpl.render_page_entry_without_layout(&entries[0]).unwrap_err()
}

#[test]
fn throws_a_premature_template_content_error_from_rendering() {
    for (path, content) in [
        ("./test/stubs/prematureTemplateContent/test.njk", "{{ sample.templateContent }}"),
        ("./test/stubs/prematureTemplateContent/test.liquid", "{{ collections.all[0].templateContent }}"),
        ("./test/stubs/prematureTemplateContent/test.md", "{{ sample.templateContent }}\n"),
    ] {
        let error = premature_error_from_rendering(path, content, |_| Ok(()));
        assert!(is_premature_template_content_error(&error), "{path}: {error:?}");
    }
}

#[test]
fn recognizes_a_premature_template_content_error_wrapped_by_a_nunjucks_filter() {
    let error = premature_error_from_rendering(
        "./test/stubs/prematureTemplateContent/filter.njk",
        "{{ samples | renderedContent }}\n",
        |cfg| {
            cfg.add_filter("renderedContent", |_ctx, items, _args| Ok(items.get_index(0).get("templateContent")));
            Ok(())
        },
    );
    assert!(is_premature_template_content_error(&error), "{error:?}");
}

#[test]
fn recognizes_a_premature_template_content_error_wrapped_by_a_custom_extension_2328() {
    let error = premature_error_from_rendering(
        "./test/stubs/prematureTemplateContent/test.wrapped",
        "sample\n",
        |cfg| {
            cfg.add_extension(
                "wrapped",
                twelvety::user_config::ExtensionOptions {
                    compile: Some(Arc::new(|_src, _path| {
                        Ok(Arc::new(|data: &Value| {
                            Ok(data.get("sample").get("templateContent").to_output_string())
                        }))
                    })),
                    ..Default::default()
                },
            )
        },
    );
    assert!(is_premature_template_content_error(&error), "{error:?}");
}

#[test]
fn issue_413_weird_date_format() {
    let tmpl = virtual_template(
        "./test/stubs-virtual-nowrite/date-frontmatter.md",
        "./test/stubs-virtual-nowrite/",
        "./dist",
        &[(
            "date-frontmatter.md",
            "---\nsubtitle: New doc page\ndate: 2019-03-13 20:18:42 +0000\ntags:\n  - docs\n---\n",
        )],
    );
    let err = tmpl.get_data().unwrap_err();
    assert_eq!(
        err.to_string(),
        "Data cascade value for `date` (2019-03-13 20:18:42 +0000) is invalid for ./test/stubs-virtual-nowrite/date-frontmatter.md"
    );
}

fn custom_front_matter_template(name: &str, options: &'static str) -> Arc<twelvety::template::Template> {
    let key = format!("custom-frontmatter/{name}");
    get_new_template(
        &format!("./test/stubs/{key}"),
        virtual_options(
            "./test/stubs/",
            "./dist",
            templates(&[(&key, custom_front_matter(name))]),
            move |cfg| {
                cfg.set_front_matter_parsing_options(json(options));
                Ok(())
            },
        ),
    )
    .unwrap()
}

#[test]
fn custom_front_matter_parsing_options() {
    let tmpl = custom_front_matter_template("template.njk", r#"{"excerpt": true}"#);
    let fm = tmpl.content.read().unwrap();
    assert_eq!(fm.data.get("front").as_str(), Some("hello"));
    assert_eq!(fm.excerpt.as_deref().unwrap().trim(), "This is an excerpt.");
    assert_eq!(fm.content.trim(), "This is an excerpt.\nThis is content.");
    let data = tmpl.get_data().unwrap();
    assert_eq!(data.get("page").get("excerpt").to_output_string().trim(), "This is an excerpt.");
}

#[test]
fn custom_front_matter_parsing_options_using_alias() {
    let tmpl = custom_front_matter_template("template.njk", r#"{"excerpt": true, "excerpt_alias": "my_excerpt"}"#);
    let fm = tmpl.content.read().unwrap();
    assert_eq!(fm.data.get("front").as_str(), Some("hello"));
    assert_eq!(fm.content.trim(), "This is an excerpt.\nThis is content.");
    let data = tmpl.get_data().unwrap();
    assert_eq!(data.get("my_excerpt").to_output_string().trim(), "This is an excerpt.");
}

#[test]
fn custom_front_matter_parsing_options_no_newline_before_separator() {
    let tmpl = custom_front_matter_template("template-newline1.njk", r#"{"excerpt": true}"#);
    let fm = tmpl.content.read().unwrap();
    assert_eq!(fm.data.get("front").as_str(), Some("hello"));
    assert_eq!(fm.excerpt.as_deref().unwrap().trim(), "This is an excerpt.");
    assert_eq!(fm.content.trim(), "This is an excerpt.\nThis is content.");
    let data = tmpl.get_data().unwrap();
    assert_eq!(data.get("page").get("excerpt").to_output_string().trim(), "This is an excerpt.");
}

#[test]
fn custom_front_matter_parsing_options_no_newline_after_separator() {
    let tmpl = custom_front_matter_template("template-newline3.njk", r#"{"excerpt": true}"#);
    assert_eq!(tmpl.content.read().unwrap().content.trim(), "This is an excerpt.\nThis is content.");
}

#[test]
fn custom_front_matter_parsing_options_no_newlines_around_separator() {
    let tmpl = custom_front_matter_template("template-newline2.njk", r#"{"excerpt": true}"#);
    assert_eq!(tmpl.content.read().unwrap().content.trim(), "This is an excerpt.This is content.");
}

#[test]
fn custom_front_matter_parsing_options_html_comment_separator() {
    let tmpl = custom_front_matter_template(
        "template-excerpt-comment.njk",
        r#"{"excerpt": true, "excerpt_separator": "<!-- excerpt -->"}"#,
    );
    let fm = tmpl.content.read().unwrap();
    assert_eq!(fm.data.get("front").as_str(), Some("hello"));
    assert_eq!(fm.excerpt.as_deref().unwrap().trim(), "This is an excerpt.");
    assert_eq!(fm.content.trim(), "This is an excerpt.\nThis is content.");
    let data = tmpl.get_data().unwrap();
    assert_eq!(data.get("page").get("excerpt").to_output_string().trim(), "This is an excerpt.");
}

#[test]
fn custom_front_matter_parsing_options_using_toml() {
    let tmpl = custom_front_matter_template("template-toml.njk", "{}");
    let fm = tmpl.content.read().unwrap();
    assert_eq!(fm.data, json(r#"{"front": "hello"}"#));
    assert_eq!(fm.content.trim(), "This is content.");
    assert_eq!(tmpl.get_data().unwrap().get("front").as_str(), Some("hello"));
}

#[test]
fn global_variable_with_dashes_issue_567_liquid() {
    let tmpl = virtual_template(
        "./test/stubs/global-dash-variable.liquid",
        "./test/stubs/",
        "./dist",
        &[("global-dash-variable.liquid", "---\nis-it-tasty: Yes\n---\n{{ is-it-tasty }}")],
    );
    let data = tmpl.get_data().unwrap();
    assert_eq!(data.get("is-it-tasty").as_str(), Some("Yes"));
    let pages = rendered_templates(&tmpl).unwrap();
    assert_eq!(pages[0].template_content().unwrap().trim(), "Yes");
}

#[test]
fn issue_446_layout_has_a_permalink_with_a_different_template_language() {
    let tmpl = virtual_template(
        "./test/stubs-virtual-nowrite/test.md",
        "./test/stubs-virtual-nowrite/",
        "dist",
        &[
            ("test.md", "---\nlayout: test.njk\ntemplateEngineOverride: md\n---\n\n# Title\n"),
            ("_includes/test.njk", "---\npermalink: \"/{{ page.fileSlug }}/\"\n---\n{{ content }}"),
        ],
    );
    let pages = rendered_templates(&tmpl).unwrap();
    let data = pages[0].data();
    assert_eq!(data.get("permalink").as_str(), Some("/{{ page.fileSlug }}/"));
    assert_eq!(data.get("page").get("url").as_str(), Some("/test/"));
}

#[test]
fn get_layout_chain() {
    let tmpl = virtual_template(
        "./test/stubs-virtual-nowrite/test.njk",
        "./test/stubs-virtual-nowrite/",
        "./dist",
        &[
            ("test.njk", "---\nlayout: base.njk\n---"),
            ("_includes/base.njk", "---\nlayout: parent.njk\n---"),
            ("_includes/parent.njk", ""),
        ],
    );
    let data = tmpl.get_data().unwrap();
    let layout = tmpl.get_layout(data.get("layout").as_str().unwrap()).unwrap();
    assert_eq!(
        layout.get_layout_chain().unwrap(),
        [
            "./test/stubs-virtual-nowrite/_includes/base.njk",
            "./test/stubs-virtual-nowrite/_includes/parent.njk"
        ]
    );
}

#[test]
fn engine_singletons() {
    let ctx = get_context(virtual_options(
        "test/stubs-virtual-nowrite",
        "dist",
        templates(&[("first.njk", ""), ("second.njk", "")]),
        |_| Ok(()),
    ))
    .unwrap();
    let tmpl1 = template_with_context("./test/stubs-virtual-nowrite/first.njk", &ctx);
    let tmpl2 = template_with_context("./test/stubs-virtual-nowrite/second.njk", &ctx);
    assert!(Arc::ptr_eq(&tmpl1.content.engine().unwrap(), &tmpl2.content.engine().unwrap()));
}

#[test]
fn permalink_object_with_build() {
    let tmpl = virtual_template(
        "./test/stubs/permalink-build/permalink-build.md",
        "./test/stubs/",
        "./test/stubs/_site",
        &[(
            "permalink-build/permalink-build.md",
            "---\npermalink:\n  build: /url/\n---\n\nThis should be the same as `permalink: /url/`\n",
        )],
    );
    let data = tmpl.get_data().unwrap();
    assert_eq!(raw_output_path(&tmpl, &data).as_deref(), Some("/url/index.html"));
    assert_eq!(output_href(&tmpl, &data).as_str(), Some("/url/"));
    let loc = tmpl.get_output_locations(&json(r#"{"permalink":{"build":false}}"#), "").unwrap();
    assert_eq!(loc.raw_path, None);
}

const EMPTY_OBJECT_PERMALINK: &str =
    "---\npermalink: {}\n---\n\nThis should be the same as if permalink was not set at all.\n";

#[test]
fn permalink_object_without_build_throws() {
    let tmpl = virtual_template(
        "./test/stubs/permalink-empty-object/empty-object.md",
        "./test/stubs/",
        "./test/stubs/_site",
        &[("permalink-empty-object/empty-object.md", EMPTY_OBJECT_PERMALINK)],
    );
    let err = tmpl
        .get_output_locations(&json(r#"{"permalink":{"someotherkey":"/build/"}}"#), "")
        .err()
        .unwrap();
    assert!(err.to_string().contains("Permalink objects"));
}

#[test]
fn permalink_is_an_empty_object_inherit_default_behavior() {
    let tmpl = virtual_template(
        "./test/stubs/permalink-empty-object/empty-object.md",
        "./test/stubs/",
        "./test/stubs/_site",
        &[("permalink-empty-object/empty-object.md", EMPTY_OBJECT_PERMALINK)],
    );
    let data = tmpl.get_data().unwrap();
    let loc = tmpl.get_output_locations(&data, "").unwrap();
    assert_eq!(loc.href.as_str(), Some("/permalink-empty-object/empty-object/"));
    assert_eq!(loc.raw_path.as_deref(), Some("permalink-empty-object/empty-object/index.html"));
    assert_eq!(
        loc.path.as_str(),
        Some("./test/stubs/_site/permalink-empty-object/empty-object/index.html")
    );
}

fn computed_permalink_object(file: &str, data: Value) {
    let key = format!("{file}.njk");
    let tmpl = get_new_template(
        &format!("./test/stubs-virtual-nowrite/{key}"),
        virtual_options(
            "./test/stubs-virtual-nowrite/",
            "./test/stubs/stubs-computed-permalink/_site",
            vec![(key.clone(), String::new(), data)],
            |_| Ok(()),
        ),
    )
    .unwrap();
    let mut data = tmpl.get_data().unwrap();
    let pages = tmpl.get_templates(&mut data).unwrap();
    assert_eq!(pages[0].url().as_str(), Some("/i18n/en/"), "{file}");
    assert_eq!(
        pages[0].output_path().as_str(),
        Some("./test/stubs/stubs-computed-permalink/_site/i18n/en/index.html"),
        "{file}"
    );
}

#[test]
fn twelvety_computed_returns_permalink_object_issue_1898() {
    let mut permalink = Value::object();
    permalink.set(
        "build",
        Value::function(|args| Ok(Value::str(format!("/i18n/{}/", args[0].get("lang").to_js_string())))),
    );
    let mut computed = Value::object();
    computed.set("permalink", permalink);
    let mut data = json(r#"{"lang":"en"}"#);
    data.set("twelvetyComputed", computed);
    computed_permalink_object("twelvetycomputed-object", data);
}

#[test]
fn twelvety_computed_returns_nested_permalink_object_issue_1898() {
    let mut computed = Value::object();
    computed.set(
        "permalink",
        Value::function(|args| {
            let mut o = Value::object();
            o.set("build", Value::str(format!("/i18n/{}/", args[0].get("lang").to_js_string())));
            Ok(o)
        }),
    );
    let mut data = json(r#"{"lang":"en"}"#);
    data.set("twelvetyComputed", computed);
    computed_permalink_object("twelvetycomputed-nested-object", data);
}

#[test]
fn twelvety_computed_returns_permalink_object_using_permalink_string_issue_1898() {
    let mut computed = Value::object();
    computed.set(
        "permalink",
        Value::function(|args| {
            let mut o = Value::object();
            o.set(
                "build",
                Value::str(args[0].get("permalink").to_js_string().replace("{{lang}}", "en")),
            );
            Ok(o)
        }),
    );
    let mut data = json(r#"{"lang":"en","permalink":"/i18n/{{lang}}/"}"#);
    data.set("twelvetyComputed", computed);
    computed_permalink_object("twelvetycomputed-object-replace", data);
}

#[test]
fn page_template_syntax_works_with_template_engine_override() {
    assert_eq!(
        render_override(
            "./test/stubs/overrides/page-templatesyntax.md",
            "overrides/page-templatesyntax.md",
            "---\ntemplateEngineOverride: njk,md\n---\n\n{{ page.templateSyntax }}\n"
        )
        .unwrap(),
        "<p>njk,md</p>"
    );
}

#[test]
fn error_messaging_returning_literals_from_custom_data_extension() {
    let tmpl = get_new_template(
        "./test/stubs-1691/template.njk",
        TestOptions::new("test/stubs-1691", "dist").with_config(|cfg| {
            cfg.add_data_extension("txt", |s, _path| Ok(Value::str(s)));
            Ok(())
        }),
    )
    .unwrap();
    let data = tmpl.get_data().unwrap();
    assert_eq!(data.get("str").as_str(), Some("Testing"));
}

const DATA_CASCADE_DATA_FILE: &str =
    r#"{"parent": {"child": 2, "datafile": true}, "datafile": true, "tags": ["tagC", "tagD", "tagD"]}"#;

#[test]
fn data_cascade_deep_merge() {
    let dir = fixture_copy(
        "./test/stubs/data-cascade",
        "template-data-cascade/stubs/data-cascade",
        &[("template.data.json", DATA_CASCADE_DATA_FILE)],
    );
    let input = dir.trim_end_matches("stubs/data-cascade");
    let data = get_new_template(&format!("{dir}/template.njk"), TestOptions::new(input, "./dist"))
        .unwrap()
        .get_data()
        .unwrap();
    assert_eq!(
        sorted_keys(&data),
        ["buildawesome", "datafile", "frontmatter", "page", "parent", "pkg", "tags", "twelvety"]
    );
    assert_eq!(sorted_keys(&data.get("parent")), ["child", "datafile", "frontmatter"]);
    assert_eq!(data.get("parent").get("child").as_f64(), Some(-2.0));
}

#[test]
fn data_cascade_tag_merge_deep_merge_with_deduplication() {
    let dir = fixture_copy(
        "./test/stubs/data-cascade",
        "template-data-cascade-tags/data-cascade",
        &[("template.data.json", DATA_CASCADE_DATA_FILE)],
    );
    let input = dir.trim_end_matches("data-cascade");
    let data = get_new_template(&format!("{dir}/template.njk"), TestOptions::new(input, "./dist"))
        .unwrap()
        .get_data()
        .unwrap();
    let mut tags = strs(&data.get("tags"));
    tags.sort();
    assert_eq!(tags, ["tagA", "tagB", "tagC", "tagD"]);
}

#[test]
fn local_data_inherits_tags_string_deep_merge() {
    let dir = fixture_copy(
        "./test/stubs/local-data-tags",
        "template-local-data-tags/local-data-tags",
        &[("component.data.json", r#"{"tags": "tag3"}"#)],
    );
    let input = dir.trim_end_matches("local-data-tags");
    let data = get_new_template(&format!("{dir}/component.njk"), TestOptions::new(input, "./dist"))
        .unwrap()
        .get_data()
        .unwrap();
    let mut tags = strs(&data.get("tags"));
    tags.sort();
    assert_eq!(tags, ["tag1", "tag2", "tag3"]);
}
