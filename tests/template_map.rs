//! Ported from upstream `test/TemplateMapTest.js`.
//!
//! Templates are compared by `inputPath` (instead of `Template` identity). Assertions inside
//! `addCollection`, linter and transform callbacks are collected and checked after the build.
//!
//! Skipped (JS-only): "Premature templateContent retries run in template order with concurrency"
//! (async `12ty.js` templates with timers and `setConcurrency`).

use std::sync::{Arc, Mutex};

use twelvety::Value;
use twelvety::build_context::BuildContext;
use twelvety::errors::UsingCircularTemplateContentReferenceError;
use twelvety::page_entry::PageEntry;
use twelvety::template::Template;
use twelvety::template_collection::TemplateCollection;
use twelvety::template_map::TemplateMap;
use twelvety::testing::{TestOptions, get_context, template_with_context};
use twelvety::user_config::UserConfig;

// Keyed by project-relative path; a shared date keeps the input path as the sort tiebreaker.
const FIXTURES: &[(&str, &str)] = &[
    (
        "test/stubs/templateMapCollection/test1.md",
        "---\ntitle: Test Title\ndate: 2020-01-01\ntags:\n  - post\n  - dog\n---\n\n# Test 1\n",
    ),
    (
        "test/stubs/templateMapCollection/test2.md",
        "---\ndate: 2020-01-01\ntags: cat\n---\n\n# Test 2\n",
    ),
    (
        "test/stubs/templateMapCollection/test3.md",
        "---\ndate: 2020-01-01\ntags: circle\n---\n\n# {{ collections.circle.length }}, {{ collections.circle[0].url }}\n",
    ),
    (
        "test/stubs/templateMapCollection/test4.md",
        "---\ntitle: Test Title 4\ndate: 2020-01-01\ntags:\n  - post\n  - dog\n---\n\n# Test 4\n",
    ),
    (
        "test/stubs/templateMapCollection/test5.md",
        "---\ntitle: Test Title 5\ndate: 2020-01-01\n---\n\n# Test 5\n",
    ),
    (
        "test/stubs/templateMapCollection/templateContent.md",
        "---\ntags: circle\n---\n\n# Test\n\n{{ collections.circle[0].templateContent }}\n",
    ),
    (
        "test/stubs/templateMapCollection/testWithLayout.md",
        "---\nlayout: layouts/templateMapCollection.njk\n---\n\n{{ upstream }}\n",
    ),
    (
        "test/stubs/templateMapCollection/paged-tag.md",
        "---\ntitle: Paged Test\npagination:\n  data: collections.dog\n  size: 1\n---\n\n# {{ title }}\n",
    ),
    (
        "test/stubs/templateMapCollection/paged-cfg.md",
        "---\ntitle: Paged Test\npagination:\n  data: collections.userCollection\n  size: 1\n---\n\n# {{ title }}\n",
    ),
    (
        "test/stubs/templateMapCollection/paged-cfg-permalink.md",
        "---\ntitle: Paged Test\npagination:\n  data: collections.userCollection\n  size: 1\n  alias: item\npermalink: /{{ item.data.title | slugify }}/hello/\n---\n\n# {{ title }}\n",
    ),
    (
        "test/stubs/templateMapCollection/paged-cfg-tagged.md",
        "---\ntitle: Paged Test\ntags:\n  - haha\npagination:\n  data: collections.userCollection\n  size: 1\n  alias: item\n---\n\n# {{ title }}\n",
    ),
    (
        "test/stubs/templateMapCollection/paged-cfg-tagged-apply-to-all.md",
        "---\ntitle: Paged Test\ntags:\n  - haha\npagination:\n  data: collections.userCollection\n  size: 1\n  alias: item\n  addAllPagesToCollections: true\n---\n\n# {{ title }}\n",
    ),
    (
        "test/stubs/templateMapCollection/paged-cfg-tagged-permalink.md",
        "---\ntitle: Paged Test\ntags:\n  - haha\npagination:\n  data: collections.userCollection\n  size: 1\n  alias: item\npermalink: /{{ item.data.title | slugify }}/goodbye/\n---\n\n# {{ title }}\n",
    ),
    (
        "test/stubs/templateMapCollection/paged-cfg-tagged-permalink-apply-to-all.md",
        "---\ntitle: Paged Test\ntags:\n  - haha\npagination:\n  data: collections.userCollection\n  size: 1\n  alias: item\n  addAllPagesToCollections: true\npermalink: /{{ item.data.title | slugify }}/goodbye/\n---\n\n# {{ title }}\n",
    ),
    (
        "test/stubs/templateMapCollection/paged-tag-dogs-templateContent.md",
        "---\ntitle: Paged Test\npagination:\n  data: collections.dog\n  size: 1\n---\n\nBefore\n{% for dog in pagination.items %}\n{{ dog.templateContent }}\n{% endfor %}\nAfter\n",
    ),
    (
        "test/stubs/templateMapCollection/paged-tag-dogs-templateContent-alias.md",
        "---\ntitle: Paged Test\npagination:\n  data: collections.dog\n  size: 2\n  alias: dogs\n---\n\nBefore\n{% for dog in dogs %}\n{{ dog.templateContent }}\n{% endfor %}\nAfter\n",
    ),
    (
        "test/stubs/_includes/layouts/templateMapCollection.njk",
        "---\nupstream: Inherited\n---\n\n{{ content | safe }}",
    ),
    (
        "test/stubs/issue-115/index.liquid",
        "---\ntitle: My index page\npagination:\n  data: collections.foos\n  size: 12\n---\n{% for foo in pagination.items -%}\n{{ foo.data.title }}\n{% endfor -%}\n{% for bar in collections.bars -%}\n{{ bar.data.title }}\n{% endfor -%}",
    ),
    (
        "test/stubs/issue-115/index-with-layout.liquid",
        "---\nlayout: layouts/issue-115.liquid\ntitle: My index page\npagination:\n  data: collections.foos\n  size: 12\n---\n",
    ),
    (
        "test/stubs/issue-115/template-bars.liquid",
        "---\ntitle: This page is bars\ntags:\n  - bars\n---\nBars",
    ),
    (
        "test/stubs/issue-115/template-foos.liquid",
        "---\ntitle: This page is foos\ntags:\n  - foos\n---\nFoos.",
    ),
    (
        "test/stubs/_includes/layouts/issue-115.liquid",
        "{% for foo in pagination.items -%}\n{{ foo.data.title }}\n{% endfor -%}\n{% for bar in collections.bars -%}\n{{ bar.data.title }}\n{% endfor -%}",
    ),
    (
        "test/stubs/tagged-pagination-multiples/test.njk",
        "---\ntitle: Paged Test\ntags:\n  - haha\npagination:\n  data: collections.userCollection\n  size: 1\n  alias: item\n  addAllPagesToCollections: true\n---\n\n{{ title }}",
    ),
    (
        "test/stubs/tagged-pagination-multiples-layout/test.njk",
        "---\ntitle: Paged Test\nlayout: layouts/div-wrapper-layout.njk\ntestdata:\n  - one\n  - two\n  - three\npagination:\n  data: testdata\n  size: 1\n  alias: item\n  addAllPagesToCollections: true\n---\n{{ item }}",
    ),
    (
        "test/stubs/collection-layout-wrap.njk",
        "---\ntitle: Layout Test\nlayout: layouts/div-wrapper-layout.njk\n---\n{{ title }}",
    ),
    (
        "test/stubs/_includes/layouts/div-wrapper-layout.njk",
        "<div>{{ content }}</div>",
    ),
    (
        "test/stubs/page-target-collections/paginateall.njk",
        "---\npagination:\n  data: collections.all\n  size: 1\n  alias: entry\n  filter:\n    - all\n  addAllPagesToCollections: true\n---\nINPUT PATH:{{ entry.inputPath }}",
    ),
    (
        "test/stubs/page-target-collections/tagpages.njk",
        "---\npagination:\n  data: collections\n  size: 1\n  alias: tag\n  filter:\n    - all\n---\n\n{{ tag }}",
    ),
    (
        "test/stubs/page-target-collections/tagpagesall.njk",
        "---\npagination:\n  data: collections\n  size: 1\n  alias: tag\n  filter:\n    - all\n  addAllPagesToCollections: true\n---\n\n{{ tag }}",
    ),
    (
        "test/stubs/twelvetyExcludeFromCollections.njk",
        "---\ntitle: Paged Test\ntwelvetyExcludeFromCollections: true\ntags:\n  - post\n  - dog\n---\n{{ title }}",
    ),
    (
        "test/stubs/twelvetyExcludeFromCollectionsPermalinkFalse.njk",
        "---\ntitle: Paged Test\ntwelvetyExcludeFromCollections: true\npermalink: false\ntags:\n  - post\n  - dog\n---\n{{ title }}",
    ),
    (
        "test/stubs/permalink-conflicts/test1.md",
        "---\ntitle: Test Title\ntags:\n  - post\n  - dog\npermalink: /permalink-conflicts/\n---\n\n# Test 1\n",
    ),
    (
        "test/stubs/permalink-conflicts/test2.md",
        "---\ntitle: Test Title\ntags:\n  - post\n  - dog\npermalink: /permalink-conflicts/\n---\n\n# Test 2\n",
    ),
    (
        "test/stubs/permalink-conflicts/test3.md",
        "---\ntitle: Test Title\ntags:\n  - post\n  - dog\npermalink: permalink-conflicts/\n---\n\n# Test 3\n",
    ),
    (
        "test/stubs/permalink-conflicts-false/test1.md",
        "---\ntitle: Test Title\ntags:\n  - post\n  - dog\npermalink: false\n---\n\n# Test 1\n",
    ),
    (
        "test/stubs/permalink-conflicts-false/test2.md",
        "---\ntitle: Test Title\ntags:\n  - post\n  - dog\npermalink: false\n---\n\n# Test 2\n",
    ),
    (
        "test/stubs/issue-522/excluded.md",
        "---\ntwelvetyExcludeFromCollections: true\n---\n\n# Test\n\n{{ collections.all[0].templateContent }}\n",
    ),
    ("test/stubs/issue-522/template.md", "# Test\n"),
    (
        "test/stubs-virtual-nowrite/_includes/layout.njk",
        "<html><body>{{ content | safe }}</body></html>",
    ),
    (
        "test/stubs-virtual-nowrite/transform-layout/transform-layout.njk",
        "---\nlayout: layout.njk\n---\nThis is content.",
    ),
    (
        "test/stubs-virtual-nowrite/_includes/first.liquid",
        "---\nlayout: second\n---\n{{ content }}",
    ),
    (
        "test/stubs-virtual-nowrite/_includes/second.liquid",
        "---\nlayout: third.liquid\n---\n{{ content }}",
    ),
    (
        "test/stubs-virtual-nowrite/_includes/third.liquid",
        "{{ content }}",
    ),
    (
        "test/stubs-virtual-nowrite/page.md",
        "---\nlayout: first\n---\n",
    ),
];

const STUBS: &str = "./test/stubs/";
const SITE: &str = "./test/stubs/_site";

/// `getTemplateConfigInstanceCustomCallback(dirs, callback)`: the fixtures under `input` are
/// added as virtual templates. Upstream’s templates have no `TemplateData` instance, so
/// template/directory data files are disabled.
fn config_with<F>(input: &str, output: &str, callback: F) -> Arc<BuildContext>
where
    F: Fn(&mut UserConfig) -> anyhow::Result<()> + Send + Sync + 'static,
{
    let prefix = input
        .trim_start_matches("./")
        .trim_end_matches('/')
        .to_string();
    get_context(TestOptions::new(input, output).with_config(move |cfg| {
        callback(cfg)?;
        cfg.set_data_file_suffixes(&[]);
        for (path, content) in FIXTURES {
            let relative = if prefix == "." || prefix.is_empty() {
                Some(*path)
            } else {
                path.strip_prefix(&format!("{prefix}/"))
            };
            if let Some(relative) = relative {
                cfg.add_template(relative, content, Value::object())?;
            }
        }
        Ok(())
    }))
    .unwrap()
}

/// `getTemplateConfigInstance()` (no directories: the input is the project root)
fn default_config() -> Arc<BuildContext> {
    config_with(".", SITE, |_| Ok(()))
}

fn stubs_config() -> Arc<BuildContext> {
    config_with(STUBS, SITE, |_| Ok(()))
}

fn stubs_config_with_collection<F>(callback: F) -> Arc<BuildContext>
where
    F: Fn(&TemplateCollection) -> anyhow::Result<Value> + Send + Sync + 'static,
{
    let callback = Arc::new(callback);
    config_with(STUBS, SITE, move |cfg| {
        let callback = callback.clone();
        cfg.add_collection("userCollection", move |c| callback(c))
    })
}

fn by_number(num: usize, ctx: &Arc<BuildContext>) -> Arc<Template> {
    template_with_context(
        &format!("./test/stubs/templateMapCollection/test{num}.md"),
        ctx,
    )
}

fn tmpl(path: &str, ctx: &Arc<BuildContext>) -> Arc<Template> {
    template_with_context(path, ctx)
}

fn template_map(ctx: &Arc<BuildContext>, templates: &[&Arc<Template>]) -> TemplateMap {
    let mut tm = TemplateMap::new(ctx.clone());
    for t in templates {
        tm.add((*t).clone()).unwrap();
    }
    tm
}

fn len(v: &Value) -> usize {
    v.len()
}

fn input_paths(items: &Value) -> Vec<String> {
    items
        .to_vec()
        .iter()
        .map(|i| i.get("inputPath").to_output_string())
        .collect()
}

/// A fresh page entry for `template` with `data` (`{ rawInput: await template.getPreRender(), data }`)
fn page_entry(template: &Arc<Template>, data: &Value) -> Arc<PageEntry> {
    let mut d = data.clone();
    let (_, raw_input) = template.run_preprocessors(&mut d).unwrap();
    PageEntry::new(
        template.clone(),
        String::new(),
        None,
        Value::Number(0.0),
        raw_input,
        data.clone(),
        None,
    )
}

/// `testRenderWithoutLayouts(template, data)`
fn render_without_layouts(template: &Arc<Template>, data: &Value) -> String {
    template
        .render_page_entry_without_layout(&page_entry(template, data))
        .unwrap()
}

/// `renderTemplate(page.template, page.data)` from `_getRenderedTemplates.js`
fn render_page(page: &Arc<PageEntry>) -> String {
    let content = page
        .template
        .render_page_entry_without_layout(page)
        .unwrap();
    let layout = page.data().get("layout");
    if layout.is_truthy() {
        let layout = page
            .template
            .get_layout(&layout.to_output_string())
            .unwrap();
        return layout.render_layout_page_entry(page, content).unwrap();
    }
    content
}

/// `getRenderedTemplates(template, data)`: `(page, templateContent)` for each page
fn rendered_templates(template: &Arc<Template>, data: &Value) -> Vec<(Arc<PageEntry>, String)> {
    let mut data = data.clone();
    let pages = template.get_templates(&mut data).unwrap();
    pages
        .into_iter()
        .map(|page| {
            let content = render_page(&page);
            (page, content)
        })
        .collect()
}

#[test]
fn template_map_has_collections_added() {
    let ctx = default_config();
    let mut tm = template_map(&ctx, &[&by_number(1, &ctx), &by_number(2, &ctx)]);
    tm.cache().unwrap();
    assert_eq!(tm.map.len(), 2);
    assert_eq!(tm.collection.get_all().len(), 2);
}

#[test]
fn template_map_compared_to_collection_api() {
    let ctx = default_config();
    let (tmpl1, tmpl4) = (by_number(1, &ctx), by_number(4, &ctx));
    let mut tm = template_map(&ctx, &[&tmpl1, &tmpl4]);
    tm.cache().unwrap();

    assert_eq!(tm.map[0].input_path, tmpl1.input_path);
    assert_eq!(
        tm.map[0].data.get_path("collections.post.0.inputPath"),
        Value::str(&tmpl1.input_path)
    );
    assert_eq!(tm.map[1].input_path, tmpl4.input_path);
    assert_eq!(
        tm.map[1].data.get_path("collections.post.1.inputPath"),
        Value::str(&tmpl4.input_path)
    );

    let mut c = TemplateCollection::new();
    for t in [&tmpl1, &tmpl4] {
        let mut data = t.get_data().unwrap();
        for page in t.get_templates(&mut data).unwrap() {
            c.add(page);
        }
    }
    assert_eq!(
        input_paths(&c.get_filtered_by_tag("post")),
        [tmpl1.input_path.clone(), tmpl4.input_path.clone()]
    );
}

#[test]
fn populating_the_collection_twice_should_clear_the_previous_values() {
    let ctx = default_config();
    let mut tm = template_map(&ctx, &[&by_number(1, &ctx), &by_number(2, &ctx)]);
    tm.cache().unwrap();
    tm.cache().unwrap();
    assert_eq!(tm.map.len(), 2);
    assert_eq!(tm.collection.len(), 2);
}

#[test]
fn template_map_adds_collections_data_and_has_template_content_values() {
    let ctx = default_config();
    let mut tm = template_map(&ctx, &[&by_number(1, &ctx), &by_number(2, &ctx)]);
    assert!(!tm.map[0].data.get("collections").is_truthy());
    assert!(!tm.map[1].data.get("collections").is_truthy());

    tm.cache().unwrap();

    for entry in &tm.map {
        assert!(
            entry.pages[0]
                .template_content()
                .is_some_and(|c| !c.is_empty())
        );
        assert!(entry.data.get("collections").is_truthy());
    }
    assert_eq!(len(&tm.map[0].data.get_path("collections.post")), 1);
    assert_eq!(len(&tm.map[0].data.get_path("collections.all")), 2);
    assert_eq!(len(&tm.map[1].data.get_path("collections.post")), 1);
    assert_eq!(len(&tm.map[1].data.get_path("collections.all")), 2);

    for entry in &tm.map {
        assert_eq!(
            &render_without_layouts(&entry.template, &entry.data),
            entry.pages[0].template_content().unwrap()
        );
    }
}

#[test]
fn template_map_circular_references_map_without_template_content() {
    let ctx = default_config();
    let mut tm = template_map(&ctx, &[&by_number(3, &ctx)]);
    assert!(!tm.map[0].data.get("collections").is_truthy());

    tm.cache().unwrap();
    assert!(
        tm.map[0].pages[0]
            .template_content()
            .is_some_and(|c| !c.is_empty())
    );
    assert!(tm.map[0].data.get("collections").is_truthy());
    assert_eq!(len(&tm.map[0].data.get_path("collections.all")), 1);
    assert_eq!(
        &render_without_layouts(&tm.map[0].template, &tm.map[0].data),
        tm.map[0].pages[0].template_content().unwrap()
    );
}

#[test]
fn template_map_circular_references_map_template_content() {
    let ctx = default_config();
    let mut tm = template_map(
        &ctx,
        &[&tmpl(
            "./test/stubs/templateMapCollection/templateContent.md",
            &ctx,
        )],
    );
    assert!(!tm.map[0].data.get("collections").is_truthy());

    let err = tm.cache().unwrap_err();
    assert!(
        err.downcast_ref::<UsingCircularTemplateContentReferenceError>()
            .is_some(),
        "{err:#}"
    );
}

#[test]
#[ignore = "JS-only: async `12ty.js` templates with timers and `setConcurrency`"]
fn premature_template_content_retries_run_in_template_order_with_concurrency() {}

fn check_issue_115(tm: &mut TemplateMap, index_path: &str) {
    assert_eq!(tm.map.len(), 3);
    assert_eq!(tm.map[2].input_path, index_path);

    let collections = tm.test_get_all_collections_data().unwrap();
    assert_eq!(len(&collections.get("all")), 3);
    assert_eq!(len(&collections.get("foos")), 1);
    assert_eq!(len(&collections.get("bars")), 1);
    let collections = tm.test_get_collections_data().unwrap();
    assert_eq!(len(&collections.get("all")), 3);
    assert_eq!(len(&collections.get("foos")), 1);
    assert_eq!(len(&collections.get("bars")), 1);

    for entry in &tm.map {
        let c = entry.data.get("collections");
        assert!(c.is_truthy());
        assert!(!c.keys().is_empty());
        assert_eq!(len(&c.get("all")), 3);
        assert_eq!(len(&c.get("foos")), 1);
        assert_eq!(len(&c.get("bars")), 1);
    }

    let rendered = rendered_templates(&tm.map[2].template, &tm.map[2].data);
    assert_eq!(
        rendered[0].1.replace("\r\n", "\n"),
        "This page is foos\nThis page is bars\n"
    );
}

#[test]
fn issue_115_mixing_pagination_and_collections() {
    let ctx = default_config();
    let mut tm = template_map(
        &ctx,
        &[
            &tmpl("./test/stubs/issue-115/template-foos.liquid", &ctx),
            &tmpl("./test/stubs/issue-115/template-bars.liquid", &ctx),
            &tmpl("./test/stubs/issue-115/index.liquid", &ctx),
        ],
    );
    tm.cache().unwrap();
    check_issue_115(&mut tm, "./test/stubs/issue-115/index.liquid");
}

#[test]
fn issue_115_with_layout_mixing_pagination_and_collections() {
    let ctx = stubs_config();
    let mut tm = template_map(
        &ctx,
        &[
            &tmpl("./test/stubs/issue-115/template-foos.liquid", &ctx),
            &tmpl("./test/stubs/issue-115/template-bars.liquid", &ctx),
            &tmpl("./test/stubs/issue-115/index-with-layout.liquid", &ctx),
        ],
    );
    tm.cache().unwrap();
    check_issue_115(&mut tm, "./test/stubs/issue-115/index-with-layout.liquid");
}

#[test]
fn template_map_adds_collections_data_and_has_page_data_values_using_cache() {
    let ctx = stubs_config();
    let mut tm = template_map(&ctx, &[&by_number(1, &ctx), &by_number(2, &ctx)]);
    tm.cache().unwrap();
    let page = tm.map[0].data.get("page");
    assert_eq!(page.get("url"), Value::str("/templateMapCollection/test1/"));
    assert_eq!(
        page.get("outputPath"),
        Value::str("./test/stubs/_site/templateMapCollection/test1/index.html")
    );
    assert_eq!(
        page.get("inputPath"),
        Value::str("./test/stubs/templateMapCollection/test1.md")
    );
    assert_eq!(page.get("fileSlug"), Value::str("test1"));
    assert!(page.get("date").is_truthy());
}

#[test]
fn template_map_adds_collections_data_and_has_page_data_values_using_test_get_collections_data() {
    let ctx = stubs_config();
    let mut tm = template_map(&ctx, &[&by_number(1, &ctx), &by_number(2, &ctx)]);
    let collections = tm.test_get_collections_data().unwrap();
    let first = collections.get("all").get_index(0);
    assert_eq!(
        first.get("url"),
        Value::str("/templateMapCollection/test1/")
    );
    assert_eq!(
        first.get("outputPath"),
        Value::str("./test/stubs/_site/templateMapCollection/test1/index.html")
    );
    assert_eq!(
        first.get_path("data.page.url"),
        Value::str("/templateMapCollection/test1/")
    );
    assert_eq!(
        first.get_path("data.page.outputPath"),
        Value::str("./test/stubs/_site/templateMapCollection/test1/index.html")
    );
    assert_eq!(
        first.get_path("data.page.inputPath"),
        Value::str("./test/stubs/templateMapCollection/test1.md")
    );
    assert_eq!(first.get_path("data.page.fileSlug"), Value::str("test1"));
}

#[test]
fn url_should_be_available_in_user_config_collections_api_calls() {
    let ctx = stubs_config_with_collection(|c| Ok(c.get_all()));
    let mut tm = template_map(&ctx, &[&by_number(1, &ctx), &by_number(2, &ctx)]);
    let collections = tm.test_get_collections_data().unwrap();
    let user = collections.get("userCollection");
    assert!(!user.is_empty());
    let first = user.get_index(0);
    assert_eq!(
        first.get("url"),
        Value::str("/templateMapCollection/test1/")
    );
    assert_eq!(
        first.get("outputPath"),
        Value::str("./test/stubs/_site/templateMapCollection/test1/index.html")
    );
    assert_eq!(
        first.get_path("data.page.url"),
        Value::str("/templateMapCollection/test1/")
    );
    assert_eq!(
        first.get_path("data.page.outputPath"),
        Value::str("./test/stubs/_site/templateMapCollection/test1/index.html")
    );
}

#[test]
fn url_should_be_available_in_user_config_collections_api_calls_test_in_callback() {
    let seen: Arc<Mutex<Vec<(Value, Value)>>> = Arc::default();
    let s = seen.clone();
    let ctx = stubs_config_with_collection(move |c| {
        let all = c.get_all();
        for item in all.to_vec() {
            s.lock()
                .unwrap()
                .push((item.get("url"), item.get("outputPath")));
        }
        Ok(all)
    });
    let mut tm = template_map(&ctx, &[&by_number(1, &ctx), &by_number(2, &ctx)]);
    tm.cache().unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(seen[0].0, Value::str("/templateMapCollection/test1/"));
    assert_eq!(
        seen[0].1,
        Value::str("./test/stubs/_site/templateMapCollection/test1/index.html")
    );
    assert_eq!(seen[1].0, Value::str("/templateMapCollection/test2/"));
    assert_eq!(
        seen[1].1,
        Value::str("./test/stubs/_site/templateMapCollection/test2/index.html")
    );
}

#[test]
fn should_be_able_to_paginate_a_tag_generated_collection() {
    let ctx = default_config();
    let mut tm = template_map(
        &ctx,
        &[
            &by_number(1, &ctx),
            &by_number(2, &ctx),
            &tmpl("./test/stubs/templateMapCollection/paged-tag.md", &ctx),
        ],
    );
    let collections = tm.test_get_collections_data().unwrap();
    assert!(!collections.get("dog").is_empty());
}

fn dog_collection(c: &TemplateCollection) -> anyhow::Result<Value> {
    Ok(c.get_filtered_by_tag("dog"))
}

#[test]
fn should_be_able_to_paginate_a_user_config_collection() {
    let ctx = stubs_config_with_collection(dog_collection);
    let mut tm = template_map(
        &ctx,
        &[
            &by_number(1, &ctx),
            &by_number(2, &ctx),
            &tmpl("./test/stubs/templateMapCollection/paged-cfg.md", &ctx),
        ],
    );
    let collections = tm.test_get_collections_data().unwrap();
    assert!(!collections.get("userCollection").is_empty());
}

#[test]
fn should_be_able_to_paginate_a_user_config_collection_uses_rendered_permalink() {
    let seen: Arc<Mutex<Vec<(Value, Value)>>> = Arc::default();
    let s = seen.clone();
    let ctx = stubs_config_with_collection(move |c| {
        let all = c.get_filtered_by_tag("dog");
        let first = all.get_index(0);
        s.lock()
            .unwrap()
            .push((first.get("url"), first.get("outputPath")));
        Ok(all)
    });
    let mut tm = template_map(
        &ctx,
        &[
            &by_number(1, &ctx),
            &by_number(2, &ctx),
            &tmpl(
                "./test/stubs/templateMapCollection/paged-cfg-permalink.md",
                &ctx,
            ),
        ],
    );
    let collections = tm.test_get_collections_data().unwrap();
    assert!(!collections.get("userCollection").is_empty());
    for (url, output_path) in seen.lock().unwrap().iter() {
        assert_eq!(*url, Value::str("/templateMapCollection/test1/"));
        assert_eq!(
            *output_path,
            Value::str("./test/stubs/_site/templateMapCollection/test1/index.html")
        );
    }
    let urls: Vec<String> = collections
        .get("all")
        .to_vec()
        .iter()
        .map(|i| i.get("url").to_output_string())
        .collect();
    assert!(urls.contains(&"/test-title/hello/".to_string()));
}

/// tmpl1 and tmpl4 have the `dog` tag, tmpl2 does not
fn paged_user_collection_map(paged_path: &str) -> Value {
    let ctx = stubs_config_with_collection(dog_collection);
    let mut tm = template_map(
        &ctx,
        &[
            &by_number(1, &ctx),
            &by_number(2, &ctx),
            &by_number(4, &ctx),
            &tmpl(paged_path, &ctx),
        ],
    );
    tm.test_get_collections_data().unwrap()
}

fn urls(items: &Value) -> Vec<String> {
    items
        .to_vec()
        .iter()
        .map(|i| i.get("url").to_output_string())
        .collect()
}

#[test]
fn paginate_user_config_collection_paged_template_is_also_tagged() {
    let collections =
        paged_user_collection_map("./test/stubs/templateMapCollection/paged-cfg-tagged.md");
    assert_eq!(collections.get("dog").len(), 2);
    assert_eq!(
        urls(&collections.get("haha")),
        ["/templateMapCollection/paged-cfg-tagged/"]
    );
}

#[test]
fn paginate_user_config_collection_paged_template_is_also_tagged_add_all_pages_to_collections() {
    let collections = paged_user_collection_map(
        "./test/stubs/templateMapCollection/paged-cfg-tagged-apply-to-all.md",
    );
    assert_eq!(collections.get("dog").len(), 2);
    assert_eq!(
        urls(&collections.get("haha")),
        [
            "/templateMapCollection/paged-cfg-tagged-apply-to-all/",
            "/templateMapCollection/paged-cfg-tagged-apply-to-all/1/"
        ]
    );
}

#[test]
fn paginate_user_config_collection_paged_template_is_also_tagged_uses_custom_rendered_permalink() {
    let collections = paged_user_collection_map(
        "./test/stubs/templateMapCollection/paged-cfg-tagged-permalink.md",
    );
    assert_eq!(urls(&collections.get("haha")), ["/test-title/goodbye/"]);
}

#[test]
fn paginate_user_config_collection_paged_template_is_also_tagged_custom_permalink_add_all_pages() {
    let collections = paged_user_collection_map(
        "./test/stubs/templateMapCollection/paged-cfg-tagged-permalink-apply-to-all.md",
    );
    assert_eq!(
        urls(&collections.get("haha")),
        ["/test-title/goodbye/", "/test-title-4/goodbye/"]
    );
}

#[test]
fn template_map_render_and_template_content_are_the_same() {
    let ctx = stubs_config();
    let mut tm = template_map(
        &ctx,
        &[&tmpl(
            "./test/stubs/templateMapCollection/testWithLayout.md",
            &ctx,
        )],
    );
    tm.cache().unwrap();
    assert_eq!(
        tm.map[0].pages[0].template_content().unwrap().trim(),
        "<p>Inherited</p>"
    );
    let page = page_entry(&tm.map[0].template, &tm.map[0].data);
    assert_eq!(render_page(&page).trim(), "<p>Inherited</p>");
}

fn paged_dogs_map(paged_path: &str) -> Vec<(Arc<PageEntry>, String)> {
    let ctx = default_config();
    let mut tm = template_map(
        &ctx,
        &[
            &by_number(1, &ctx),
            &by_number(2, &ctx),
            &by_number(4, &ctx),
            &tmpl(paged_path, &ctx),
        ],
    );
    tm.cache().unwrap();
    let entry = tm.get_map_entry_for_input_path(paged_path).unwrap();
    rendered_templates(&entry.template, &entry.data)
}

#[test]
fn should_be_able_to_paginate_a_tag_generated_collection_and_it_has_template_content() {
    let templates =
        paged_dogs_map("./test/stubs/templateMapCollection/paged-tag-dogs-templateContent.md");
    assert_eq!(templates.len(), 2);
    assert_eq!(
        templates[0].0.data().get_path("pagination.pageNumber"),
        Value::from(0)
    );
    assert_eq!(
        templates[1].0.data().get_path("pagination.pageNumber"),
        Value::from(1)
    );
    assert_eq!(
        templates[0].1.trim(),
        "<p>Before</p>\n<h1>Test 1</h1>\n<p>After</p>"
    );
    assert_eq!(
        templates[1].1.trim(),
        "<p>Before</p>\n<h1>Test 4</h1>\n<p>After</p>"
    );
}

#[test]
fn should_be_able_to_paginate_a_tag_generated_collection_when_aliased_and_it_has_template_content()
{
    let templates = paged_dogs_map(
        "./test/stubs/templateMapCollection/paged-tag-dogs-templateContent-alias.md",
    );
    assert_eq!(templates.len(), 1);
    assert_eq!(
        templates[0].0.data().get_path("pagination.pageNumber"),
        Value::from(0)
    );
    assert_eq!(
        templates[0].1.trim(),
        "<p>Before</p>\n<h1>Test 1</h1>\n<h1>Test 4</h1>\n<p>After</p>"
    );
}

#[test]
fn issue_253_paginated_template_with_a_tag_should_put_multiple_pages_into_a_collection() {
    let collections =
        paged_user_collection_map("./test/stubs/tagged-pagination-multiples/test.njk");
    assert_eq!(collections.get("dog").len(), 2);
    assert_eq!(
        urls(&collections.get("haha")),
        [
            "/tagged-pagination-multiples/test/",
            "/tagged-pagination-multiples/test/1/"
        ]
    );
}

#[test]
fn get_user_config_collection_names() {
    let ctx = config_with(".", SITE, |cfg| {
        cfg.add_collection("userCollection", |c| Ok(c.get_all()))?;
        cfg.add_collection("otherUserCollection", |c| Ok(c.get_all()))
    });
    let tm = TemplateMap::new(ctx);
    assert_eq!(
        tm.get_user_config_collection_names(),
        ["userCollection", "otherUserCollection"]
    );
}

#[test]
fn is_user_config_collection_name() {
    let ctx = config_with(".", SITE, |cfg| {
        cfg.add_collection("userCollection", |c| Ok(c.get_all()))
    });
    let tm = TemplateMap::new(ctx);
    assert!(tm.is_user_config_collection_name("userCollection"));
    assert!(!tm.is_user_config_collection_name("userCollection2"));
}

#[test]
fn dependency_map_should_have_nodes_that_have_no_dependencies_and_no_dependents() {
    let ctx = default_config();
    let mut tm = template_map(&ctx, &[&by_number(1, &ctx), &by_number(5, &ctx)]);
    tm.cache().unwrap();
    assert!(
        tm.get_template_order()
            .iter()
            .any(|dep| dep.contains("test5.md"))
    );
    assert_eq!(tm.test_get_collections_data().unwrap().get("all").len(), 2);
}

fn orphan_user_collection_order(
    callback: fn(&TemplateCollection) -> anyhow::Result<Value>,
    expected_len: usize,
) {
    let ctx = config_with(".", SITE, move |cfg| {
        cfg.add_collection("userCollection", callback)
    });
    let mut tm = template_map(&ctx, &[&by_number(1, &ctx), &by_number(5, &ctx)]);
    tm.cache().unwrap();
    assert_eq!(
        tm.get_template_order(),
        [
            "./test/stubs/templateMapCollection/test1.md",
            "__collection:post",
            "__collection:dog",
            "./test/stubs/templateMapCollection/test5.md",
            "__collection:userCollection",
            "__collection:[keys]",
            "__collection:all",
        ]
    );
    let collections = tm.test_get_collections_data().unwrap();
    assert_eq!(collections.get("all").len(), 2);
    assert_eq!(collections.get("userCollection").len(), expected_len);
}

#[test]
fn dependency_map_should_include_orphan_user_config_collections_in_the_correct_order_3711() {
    orphan_user_collection_order(|c| Ok(c.get_all()), 2);
}

#[test]
fn dependency_map_should_include_orphan_user_config_collections_mapped_by_api_to_tag_3711() {
    orphan_user_collection_order(|c| Ok(c.get_filtered_by_tag("post")), 1);
}

#[test]
fn template_pages_should_not_have_layouts_when_added_to_collections() {
    let ctx = stubs_config();
    let t = tmpl("./test/stubs/collection-layout-wrap.njk", &ctx);
    let mut tm = template_map(&ctx, &[&t]);
    assert_eq!(
        render_page(&page_entry(&t, &t.get_data().unwrap())),
        "<div>Layout Test</div>"
    );

    let collections = tm.test_get_collections_data().unwrap();
    assert_eq!(collections.get("all").len(), 1);
    assert_eq!(
        collections.get_path("all.0.templateContent"),
        Value::str("Layout Test")
    );
}

#[test]
fn paginated_template_pages_should_not_have_layouts_when_added_to_collections() {
    let ctx = stubs_config();
    let mut tm = template_map(
        &ctx,
        &[&tmpl(
            "./test/stubs/tagged-pagination-multiples-layout/test.njk",
            &ctx,
        )],
    );
    let collections = tm.test_get_collections_data().unwrap();
    let contents: Vec<String> = collections
        .get("all")
        .to_vec()
        .iter()
        .map(|i| i.get("templateContent").to_output_string())
        .collect();
    assert_eq!(contents, ["one", "two", "three"]);
}

fn tag_page_contents(collections: &Value, suffix: &str) -> std::collections::BTreeSet<String> {
    collections
        .get("all")
        .to_vec()
        .iter()
        .filter(|e| e.get("inputPath").to_output_string().ends_with(suffix))
        .map(|e| {
            e.get("templateContent")
                .to_output_string()
                .trim()
                .to_string()
        })
        .collect()
}

fn tag_pages_map(paged_paths: &[&str]) -> (TemplateMap, Value) {
    let ctx = default_config();
    let mut templates: Vec<Arc<Template>> = paged_paths.iter().map(|p| tmpl(p, &ctx)).collect();
    templates.push(by_number(1, &ctx));
    templates.push(by_number(2, &ctx));
    let mut tm = template_map(&ctx, &templates.iter().collect::<Vec<_>>());
    let collections = tm.test_get_collections_data().unwrap();
    (tm, collections)
}

#[test]
fn tag_pages_allow_pagination_over_all_collections() {
    let (_, collections) = tag_pages_map(&["./test/stubs/page-target-collections/tagpages.njk"]);
    assert_eq!(collections.get("all").len(), 3);
    assert_eq!(
        tag_page_contents(&collections, "tagpages.njk"),
        ["post".to_string()].into()
    );
}

#[test]
fn tag_pages_all_pages_added_to_collections_allow_pagination_over_all_collections() {
    let (_, collections) = tag_pages_map(&["./test/stubs/page-target-collections/tagpagesall.njk"]);
    assert_eq!(collections.get("all").len(), 5);
    assert_eq!(
        tag_page_contents(&collections, "tagpagesall.njk"),
        ["post", "dog", "cat"].map(String::from).into()
    );
}

fn excluded_map(path: &str) {
    let ctx = default_config();
    let mut tm = template_map(&ctx, &[&by_number(1, &ctx), &tmpl(path, &ctx)]);
    tm.cache().unwrap();
    assert_eq!(tm.map.len(), 2);
    let collections = tm.test_get_collections_data().unwrap();
    assert_eq!(collections.get("all").len(), 1);
    assert_eq!(collections.get("post").len(), 1);
    assert_eq!(collections.get("dog").len(), 1);
}

#[test]
fn twelvety_exclude_from_collections() {
    excluded_map("./test/stubs/twelvetyExcludeFromCollections.njk");
}

#[test]
fn twelvety_exclude_from_collections_and_permalink_false() {
    excluded_map("./test/stubs/twelvetyExcludeFromCollectionsPermalinkFalse.njk");
}

#[test]
fn paginate_over_collections_all() {
    let (tm, collections) =
        tag_pages_map(&["./test/stubs/page-target-collections/paginateall.njk"]);
    let all = input_paths(&collections.get("all"));
    assert_eq!(all.len(), 4);
    assert_eq!(all.iter().filter(|p| p.ends_with("test1.md")).count(), 1);
    assert_eq!(all.iter().filter(|p| p.ends_with("test2.md")).count(), 1);
    assert_eq!(
        all.iter()
            .filter(|p| p.ends_with("paginateall.njk"))
            .count(),
        2
    );

    let content = |i: usize, j: usize| tm.map[i].pages[j].template_content().unwrap().clone();
    assert_eq!(
        tm.map[0].input_path,
        "./test/stubs/page-target-collections/paginateall.njk"
    );
    assert_eq!(tm.map[0].pages.len(), 2);
    assert_eq!(
        content(0, 0),
        "INPUT PATH:./test/stubs/templateMapCollection/test1.md"
    );
    assert_eq!(
        content(0, 1),
        "INPUT PATH:./test/stubs/templateMapCollection/test2.md"
    );
    assert_eq!(
        tm.map[1].input_path,
        "./test/stubs/templateMapCollection/test1.md"
    );
    assert_eq!(content(1, 0).trim(), "<h1>Test 1</h1>");
    assert_eq!(
        tm.map[2].input_path,
        "./test/stubs/templateMapCollection/test2.md"
    );
    assert_eq!(content(2, 0).trim(), "<h1>Test 2</h1>");
}

#[test]
fn paginate_over_collections_tag_pages_3823() {
    let (_, collections) = tag_pages_map(&["./test/stubs/page-target-collections/tagpagesall.njk"]);
    let mut urls = urls(&collections.get("all"));
    urls.sort();
    // 2 individual templates, 3 pages for tagpagesall
    assert_eq!(
        urls,
        [
            "/test/stubs/page-target-collections/tagpagesall/",
            "/test/stubs/page-target-collections/tagpagesall/1/",
            "/test/stubs/page-target-collections/tagpagesall/2/",
            "/test/stubs/templateMapCollection/test1/",
            "/test/stubs/templateMapCollection/test2/",
        ]
    );
}

#[test]
fn paginate_over_collections_all_with_a_paginate_over_collections_tag_pages_3823() {
    let (_, collections) = tag_pages_map(&[
        "./test/stubs/page-target-collections/paginateall.njk",
        "./test/stubs/page-target-collections/tagpagesall.njk",
    ]);
    // 2 individual templates, 3 pages for tagpagesall, 5 pages from paginateall to paginate the 2+3
    assert_eq!(collections.get("all").len(), 10);
}

#[test]
fn test_a_transform_with_a_layout_via_template_map() {
    // upstream adds the linter and transform to the template instance, here they’re config-wide
    let seen: Arc<Mutex<Vec<String>>> = Arc::default();
    let (linted, transformed) = (seen.clone(), seen.clone());
    let ctx = config_with(
        "./test/stubs-virtual-nowrite/",
        "./test/stubs-virtual-nowrite/_site",
        move |cfg| {
            let linted = linted.clone();
            cfg.add_linter("linter", move |ctx, content| {
                // should be pre-transform content
                assert_eq!(content, "<html><body>This is content.</body></html>");
                assert!(ctx.input_path.ends_with("transform-layout.njk"));
                assert!(
                    ctx.output_path
                        .to_output_string()
                        .ends_with("transform-layout/index.html")
                );
                linted.lock().unwrap().push("linter".into());
                Ok(())
            });
            let transformed = transformed.clone();
            cfg.add_transform("transformName", move |ctx, content| {
                assert_eq!(content, "<html><body>This is content.</body></html>");
                assert!(
                    ctx.output_path
                        .to_output_string()
                        .ends_with("transform-layout/index.html")
                );
                transformed.lock().unwrap().push("transform".into());
                Ok("OVERRIDE BY A TRANSFORM".into())
            });
            Ok(())
        },
    );
    let mut tm = template_map(
        &ctx,
        &[&tmpl(
            "./test/stubs-virtual-nowrite/transform-layout/transform-layout.njk",
            &ctx,
        )],
    );
    tm.cache().unwrap();
    assert_eq!(tm.map.len(), 1);
    for entry in &tm.map {
        for page in &entry.pages {
            assert_eq!(
                page.template.render_page_entry(page).unwrap(),
                "OVERRIDE BY A TRANSFORM"
            );
        }
    }
    assert_eq!(*seen.lock().unwrap(), ["linter", "transform"]);
}

#[test]
fn async_user_collection_add_collection_method() {
    // upstream resolves the collection from a Promise, config collections are synchronous here
    let ctx = stubs_config_with_collection(|c| Ok(c.get_all()));
    let mut tm = template_map(&ctx, &[&by_number(1, &ctx)]);
    let collections = tm.test_get_collections_data().unwrap();
    let first = collections.get("userCollection").get_index(0);
    assert_eq!(
        first.get("url"),
        Value::str("/templateMapCollection/test1/")
    );
    assert_eq!(first.get_path("data.collections.userCollection").len(), 1);
}

#[test]
fn duplicate_permalinks_in_template_map() {
    let ctx = default_config();
    let mut tm = template_map(
        &ctx,
        &[
            &tmpl("./test/stubs/permalink-conflicts/test1.md", &ctx),
            &tmpl("./test/stubs/permalink-conflicts/test2.md", &ctx),
        ],
    );
    assert!(tm.cache().is_err());
}

#[test]
fn no_duplicate_permalinks_in_template_map_using_false() {
    let ctx = default_config();
    let mut tm = template_map(
        &ctx,
        &[
            &tmpl("./test/stubs/permalink-conflicts-false/test1.md", &ctx),
            &tmpl("./test/stubs/permalink-conflicts-false/test2.md", &ctx),
        ],
    );
    tm.cache().unwrap();
}

#[test]
fn duplicate_permalinks_in_template_map_no_leading_slash() {
    let ctx = default_config();
    let mut tm = template_map(
        &ctx,
        &[
            &tmpl("./test/stubs/permalink-conflicts/test1.md", &ctx),
            &tmpl("./test/stubs/permalink-conflicts/test3.md", &ctx),
        ],
    );
    assert!(tm.cache().is_err());
}

#[test]
fn circular_references_using_twelvety_exclude_from_collections_and_collections_all() {
    let ctx = default_config();
    let mut tm = template_map(
        &ctx,
        &[
            &tmpl("./test/stubs/issue-522/excluded.md", &ctx),
            &tmpl("./test/stubs/issue-522/template.md", &ctx),
        ],
    );
    assert!(!tm.map[0].data.get("collections").is_truthy());

    tm.cache().unwrap();
    assert_eq!(
        tm.get_template_order(),
        [
            "./test/stubs/issue-522/excluded.md",
            "./test/stubs/issue-522/template.md",
            "__collection:[keys]",
            "__collection:all",
        ]
    );
    assert_eq!(tm.map.len(), 2);
    assert_eq!(tm.test_get_collections_data().unwrap().get("all").len(), 1);
}

#[test]
fn twelvety_layouts_event() {
    let received: Arc<Mutex<Vec<Value>>> = Arc::default();
    let r = received.clone();
    let ctx = config_with(
        "./test/stubs-virtual-nowrite/",
        "./test/stubs-virtual-nowrite/_site",
        move |cfg| {
            let r = r.clone();
            cfg.on("buildawesome.layouts", move |layout_map| {
                r.lock().unwrap().push(layout_map.clone());
                Ok(())
            });
            Ok(())
        },
    );
    let mut tm = template_map(&ctx, &[&tmpl("./test/stubs-virtual-nowrite/page.md", &ctx)]);
    tm.cache().unwrap();
    let received = received.lock().unwrap();
    assert_eq!(received.len(), 1);
    assert_eq!(
        received[0],
        twelvety::testing::json(
            r#"{
              "./test/stubs-virtual-nowrite/_includes/first.liquid": ["./test/stubs-virtual-nowrite/page.md"],
              "./test/stubs-virtual-nowrite/_includes/second.liquid": [
                "./test/stubs-virtual-nowrite/page.md",
                "./test/stubs-virtual-nowrite/_includes/first.liquid"
              ],
              "./test/stubs-virtual-nowrite/_includes/third.liquid": [
                "./test/stubs-virtual-nowrite/page.md",
                "./test/stubs-virtual-nowrite/_includes/first.liquid",
                "./test/stubs-virtual-nowrite/_includes/second.liquid"
              ]
            }"#
        )
    );
}
