//! Port of upstream `test/TemplateWriterTest.js`.

use std::path::Path;
use std::sync::Arc;

use twelvety::Value;
use twelvety::build_context::BuildContext;
use twelvety::files::Files;
use twelvety::page_entry::PageEntry;
use twelvety::template::Template;
use twelvety::template_collection::TemplateCollection;
use twelvety::template_config::TemplateConfig;
use twelvety::template_map::TemplateMap;
use twelvety::template_passthrough::TemplatePassthroughManager;
use twelvety::template_writer::TemplateWriter;
use twelvety::testing::quiet_logger;
use twelvety::user_config::UserConfig;
use twelvety::util::project_directories::{ConfigDirs, ProjectDirectories};
use twelvety::util::project_template_formats::ProjectTemplateFormats;
use twelvety::util::template_path;

const COLLECTION_TEMPLATES: &[(&str, &str)] = &[
    ("test1.md", "---\ntitle: Test Title\ntags:\n  - post\n  - dog\n---\n\n# Test 1\n"),
    ("test2.md", "---\ntags: cat\n---\n\n# Test 2\n"),
    ("test3.md", "---\ntags:\n  - post\n  - cat\n---\n\n# Test 3\n"),
    ("test4.md", "---\ndate: 1983-01-01\n---\n\n# Test 3\n"),
    ("test5.md", "---\ndate: 2038-01-01\n---\n\n# Test 3\n"),
    ("test8.md", "---\ntitle: Test Title\ntags:\n  - post\n  - office\neleventyExcludeFromCollections: true\n---\n\n# Test 1\n"),
    ("test9.md", "---\ntitle: Test Title\ntags:\n  - post\n  - office\neleventyExcludeFromCollections: post\n---\n\n# Test 1\n"),
    ("test10.md", "---\ntitle: Test Title\ntags:\n  - post\n  - office\neleventyExcludeFromCollections:\n  - post\n---\n\n# Test 1\n"),
];

const COLLECTION2_TEMPLATES: &[(&str, &str)] = &[
    ("test1.md", "---\ntitle: Test Title\ntags:\n  - post\n  - dog\ndate: 2009-01-01\n---\n\n# Test 1\n"),
    ("test2.md", "---\ntags:\n  - post\n  - cat\ndate: 2010-01-01\n---\n\n# Test 2\n"),
];

const PAGED_CONSUMER: &str = "{% for item in collections.pagingtag %}{{ item.templateContent | safe }}{% endfor %}\n";

// A shared date keeps the input path as the sort tiebreaker.
const PAGED_TAG1_TEMPLATES: &[(&str, &str)] = &[
    ("test1.njk", "---\ntitle: Testing 1\ndate: 2020-01-01\ntags:\n  - tag1\n---\n{{ title }}"),
    ("test2.njk", "---\ntitle: Testing 2\ndate: 2020-01-01\ntags:\n  - tag1\n---\n{{ title }}"),
    ("test3.njk", "---\ntitle: Testing 3\ndate: 2020-01-01\ntags:\n  - tag1\n---\n{{ title }}"),
];

/// `getTemplateConfigInstanceCustomCallback` + `getTemplateWriterInstance(formats, config)`
fn get_template_writer(
    input: &str,
    output: &str,
    formats: &[&str],
    callback: impl Fn(&mut UserConfig) -> anyhow::Result<()> + Send + Sync + 'static,
) -> TemplateWriter {
    let mut tc = TemplateConfig::new(Some(""));
    let logger = quiet_logger();
    tc.set_logger(logger.clone());
    let mut dirs = ProjectDirectories::new();
    dirs.set_via_config_object(&ConfigDirs {
        input: Some(input.into()),
        output: Some(output.into()),
        ..Default::default()
    })
    .unwrap();
    tc.set_directories(dirs);
    let mut f = ProjectTemplateFormats::new();
    f.set_via_command_line(Some(&formats.join(",")));
    tc.set_template_formats(f);
    tc.programmatic_config = Some(Arc::new(callback));
    tc.init().unwrap();
    let mut env = twelvety::value::Map::new();
    env.insert("source".into(), Value::str("script"));
    env.insert("runMode".into(), Value::str("build"));
    let (ctx, search) = BuildContext::create(&tc, logger.clone(), "build", Value::from_map(env)).unwrap();
    let config = tc.get_config();
    let files = Files::new(config.clone(), tc.directories.clone(), ctx.extension_map.clone(), search.clone());
    let passthrough = TemplatePassthroughManager::new(config, tc.directories.clone(), search, tc.exists_cache.clone(), logger);
    TemplateWriter::new(ctx, files, Arc::new(passthrough))
}

/// `getVirtualConfig`: virtual inputs use an empty directory so they never collide with files on disk
fn virtual_writer(
    output: &str,
    formats: &[&str],
    templates: &'static [(&'static str, &'static str)],
    callback: impl Fn(&mut UserConfig) -> anyhow::Result<()> + Send + Sync + 'static,
) -> TemplateWriter {
    get_template_writer("test/stubs-virtual-nowrite", output, formats, move |cfg| {
        callback(cfg)?;
        for (path, content) in templates {
            cfg.add_template(path, content, Value::object())?;
        }
        Ok(())
    })
}

/// `tw._getAllPaths()` + `tw.createTemplateMap(paths)`
fn template_map(tw: &mut TemplateWriter) -> &TemplateMap {
    let paths = tw.get_all_paths().unwrap();
    tw.create_template_map(&paths).unwrap();
    tw.template_map.as_ref().unwrap()
}

/// `templateMap._testGetCollectionsData()`
fn collection(map: &TemplateMap, name: &str) -> Value {
    map.collections.snapshot().get(name).cloned().unwrap_or_default()
}

fn len(v: &Value) -> usize {
    v.to_vec().len()
}

fn base(input_path: &Value) -> String {
    template_path::get_last_path_segment(&input_path.to_output_string())
}

fn output_path_of(tmpl: &Template) -> String {
    let data = tmpl.get_data().unwrap();
    tmpl.get_output_locations(&data, "").unwrap().path.to_output_string()
}

fn pages<'a>(map: &'a TemplateMap, input_path: &str) -> &'a [Arc<PageEntry>] {
    let entry = map.get_map_entry_for_input_path(input_path).expect("map entry");
    assert_eq!(entry.input_path, input_path);
    &entry.pages
}

fn template_content(page: &PageEntry) -> String {
    page.template_content().cloned().unwrap_or_default().trim().to_string()
}

/// "Output is a subdir of input"
#[test]
fn output_is_a_subdir_of_input() {
    let tw = get_template_writer("test/stubs/writeTest", "test/stubs/writeTest/_writeTestSite", &["liquid", "md"], |_| Ok(()));
    let files = tw.files().get_files().unwrap();
    assert_eq!(tw.files().get_file_globs(), ["./test/stubs/writeTest/**/*.{liquid,md}".to_string()]);
    assert!(!files.is_empty());
    let tmpl = Template::new(&files[0], tw.ctx().clone(), false);
    assert_eq!(tmpl.ctx().dirs.input(), "./test/stubs/writeTest/");
    assert_eq!(output_path_of(&tmpl), "./test/stubs/writeTest/_writeTestSite/test/index.html");
}

/// "createTemplateMap"
#[test]
fn create_template_map() {
    let mut tw = get_template_writer("test/stubs/writeTest", "test/stubs/writeTest/_writeTestSite", &["liquid", "md"], |_| Ok(()));
    let paths = tw.get_all_paths().unwrap();
    assert_eq!(paths[0], "./test/stubs/writeTest/test.md");
    tw.create_template_map(&paths).unwrap();
    let map = tw.template_map.as_ref().unwrap();
    assert!(!map.map.is_empty());
    assert!(map.map[0].data.is_object());
}

/// "createTemplateMap (no leading dot slash)"
#[test]
fn create_template_map_no_leading_dot_slash() {
    let tw = get_template_writer("test/stubs/writeTest", "test/stubs/_writeTestSite", &["liquid", "md"], |_| Ok(()));
    let paths = tw.get_all_paths().unwrap();
    assert_eq!(paths[0], "./test/stubs/writeTest/test.md");
}

/// "_testGetCollectionsData"
#[test]
fn test_get_collections_data() {
    let mut tw = virtual_writer("test/stubs/_site", &["md"], COLLECTION_TEMPLATES, |_| Ok(()));
    let map = template_map(&mut tw);
    assert_eq!(len(&collection(map, "post")), 2);
    assert_eq!(len(&collection(map, "cat")), 2);
    assert_eq!(len(&collection(map, "dog")), 1);
}

/// "_testGetAllTags"
#[test]
fn test_get_all_tags() {
    let mut tw = virtual_writer("test/stubs/_site", &["md"], COLLECTION_TEMPLATES, |_| Ok(()));
    let map = template_map(&mut tw);
    let mut tags = map.collection.get_tag_names();
    tags.sort();
    assert_eq!(tags, ["cat", "dog", "office", "post"]);
}

/// "Collection of files sorted by date"
#[test]
fn collection_of_files_sorted_by_date() {
    let mut tw = get_template_writer("test/stubs/dates", "test/stubs/_site", &["md"], |_| Ok(()));
    let map = template_map(&mut tw);
    assert_eq!(len(&collection(map, "dateTestTag")), 6);
}

fn sorted_posts(c: &TemplateCollection, descending: bool) -> anyhow::Result<Value> {
    let mut posts = c.get_filtered_by_tag("post").to_vec();
    posts.sort_by(|a, b| a.get("date").to_number().total_cmp(&b.get("date").to_number()));
    if descending {
        posts.reverse();
    }
    Ok(Value::array(posts))
}

/// "__testGetCollectionsData with custom collection (ascending)"
#[test]
fn test_get_collections_data_with_custom_collection_ascending() {
    let mut tw = virtual_writer("test/stubs/_site", &["md"], COLLECTION2_TEMPLATES, |cfg| {
        cfg.add_collection("customPostsAsc", |c| sorted_posts(c, false))
    });
    let map = template_map(&mut tw);
    let items = collection(map, "customPostsAsc").to_vec();
    assert_eq!(items.len(), 2);
    assert_eq!(base(&items[0].get("inputPath")), "test1.md");
    assert_eq!(base(&items[1].get("inputPath")), "test2.md");
}

/// "__testGetCollectionsData with custom collection (descending)"
#[test]
fn test_get_collections_data_with_custom_collection_descending() {
    let mut tw = virtual_writer("test/stubs/_site", &["md"], COLLECTION2_TEMPLATES, |cfg| {
        cfg.add_collection("customPosts", |c| sorted_posts(c, true))
    });
    let map = template_map(&mut tw);
    let items = collection(map, "customPosts").to_vec();
    assert_eq!(items.len(), 2);
    assert_eq!(base(&items[0].get("inputPath")), "test2.md");
    assert_eq!(base(&items[1].get("inputPath")), "test1.md");
}

/// "__testGetCollectionsData with custom collection (filter only to markdown input)"
#[test]
fn test_get_collections_data_with_custom_collection_filter_only_to_markdown_input() {
    let mut tw = virtual_writer("test/stubs/_site", &["md"], COLLECTION2_TEMPLATES, |cfg| {
        cfg.add_collection("onlyMarkdown", |c| {
            Ok(Value::array(
                c.get_all_sorted()
                    .to_vec()
                    .into_iter()
                    .filter(|item| item.get("inputPath").to_output_string().ends_with(".md"))
                    .collect(),
            ))
        })
    });
    let map = template_map(&mut tw);
    let items = collection(map, "onlyMarkdown").to_vec();
    assert_eq!(items.len(), 2);
    assert_eq!(base(&items[0].get("inputPath")), "test1.md");
    assert_eq!(base(&items[1].get("inputPath")), "test2.md");
}

const PAGED_MAIN: &str = "---\npagination:\n  data: collections.tag1\n  size: 2\ntags:\n  - pagingtag\n---\n<ol>{% for item in pagination.items %}<li>{{ item.url }}</li>{% endfor %}</ol>\n";

const PAGED_MAIN_ALL: &str = "---\npagination:\n  data: collections.tag1\n  size: 2\n  addAllPagesToCollections: true\ntags:\n  - pagingtag\n---\n<ol>{% for item in pagination.items %}<li>{{ item.url }}</li>{% endfor %}</ol>";

fn with_paged(extra: &'static [(&'static str, &'static str)]) -> &'static [(&'static str, &'static str)] {
    let mut all: Vec<(&'static str, &'static str)> = PAGED_TAG1_TEMPLATES.to_vec();
    all.push(("consumer.njk", PAGED_CONSUMER));
    all.extend_from_slice(extra);
    Box::leak(all.into_boxed_slice())
}

/// "Pagination with a Collection"
#[test]
fn pagination_with_a_collection() {
    let mut tw = virtual_writer("test/stubs/_site", &["njk"], with_paged(&[("main.njk", PAGED_MAIN)]), |_| Ok(()));
    let map = template_map(&mut tw);
    assert_eq!(len(&collection(map, "tag1")), 3);
    assert_eq!(len(&collection(map, "pagingtag")), 1);
    let pages = pages(map, "./test/stubs-virtual-nowrite/main.njk");
    assert_eq!(pages.len(), 2);
    assert_eq!(pages[0].output_path(), Value::str("./test/stubs/_site/main/index.html"));
    assert_eq!(pages[1].output_path(), Value::str("./test/stubs/_site/main/1/index.html"));
    assert_eq!(template_content(&pages[0]), "<ol><li>/test1/</li><li>/test2/</li></ol>");
    assert_eq!(template_content(&pages[1]), "<ol><li>/test3/</li></ol>");
}

/// "Pagination with a Collection from another Paged Template"
#[test]
fn pagination_with_a_collection_from_another_paged_template() {
    let mut tw = virtual_writer(
        "test/stubs/_site",
        &["njk"],
        with_paged(&[
            (
                "paged-downstream.njk",
                "---\npagination:\n  data: collections.pagingtag\n  size: 1\n---\n<ol>{% for item in pagination.items %}<li>{{ item.url }}</li>{% endfor %}</ol>",
            ),
            ("paged-main.njk", PAGED_MAIN_ALL),
        ]),
        |_| Ok(()),
    );
    let map = template_map(&mut tw);
    assert_eq!(len(&collection(map, "tag1")), 3);
    assert_eq!(len(&collection(map, "pagingtag")), 2);
    let main = pages(map, "./test/stubs-virtual-nowrite/paged-main.njk");
    assert_eq!(template_content(&main[0]), "<ol><li>/test1/</li><li>/test2/</li></ol>");
    assert_eq!(template_content(&main[1]), "<ol><li>/test3/</li></ol>");
    let downstream = pages(map, "./test/stubs-virtual-nowrite/paged-downstream.njk");
    assert_eq!(template_content(&downstream[0]), "<ol><li>/paged-main/</li></ol>");
    assert_eq!(template_content(&downstream[1]), "<ol><li>/paged-main/1/</li></ol>");
}

/// "Pagination with a Collection (apply all pages to collections)"
#[test]
fn pagination_with_a_collection_apply_all_pages_to_collections() {
    let mut tw = virtual_writer("test/stubs/_site", &["njk"], with_paged(&[("main.njk", PAGED_MAIN_ALL)]), |_| Ok(()));
    let ctx = tw_ctx(&tw);
    let map = template_map(&mut tw);
    assert_eq!(len(&collection(map, "tag1")), 3);
    assert_eq!(len(&collection(map, "pagingtag")), 2);
    let pages = pages(map, "./test/stubs-virtual-nowrite/main.njk");
    let main = Template::new("./test/stubs-virtual-nowrite/main.njk", ctx, false);
    assert_eq!(output_path_of(&main), "./test/stubs/_site/main/index.html");
    assert_eq!(pages.len(), 2);
    assert_eq!(pages[0].output_path(), Value::str("./test/stubs/_site/main/index.html"));
    assert_eq!(pages[1].output_path(), Value::str("./test/stubs/_site/main/1/index.html"));
    assert_eq!(template_content(&pages[0]), "<ol><li>/test1/</li><li>/test2/</li></ol>");
    assert_eq!(template_content(&pages[1]), "<ol><li>/test3/</li></ol>");
}

fn tw_ctx(tw: &TemplateWriter) -> Arc<BuildContext> {
    tw.ctx().clone()
}

/// Renders a page with its layout (`getRenderedTemplates(...)[0].templateContent`)
fn rendered(map: &TemplateMap, input_path: &str) -> String {
    let page = &pages(map, input_path)[0];
    page.template.render_page_entry(page).unwrap().trim().replace("\r\n", "\n")
}

/// "Use a collection inside of a template"
#[test]
fn use_a_collection_inside_of_a_template() {
    let mut tw = virtual_writer(
        "test/stubs/collection-template/_site",
        &["liquid"],
        &[
            ("_includes/layout.liquid", "Layout\n\n{{ content }}"),
            ("dog1.liquid", "---\ntags:\n  - dog\n---"),
            (
                "template.liquid",
                "---\nlayout: layout.liquid\n---\nTemplate\n\nAll {{ collections.all | size }} templates\nTemplate {{ collections.dog | size }} dog",
            ),
        ],
        |_| Ok(()),
    );
    let ctx = tw_ctx(&tw);
    let map = template_map(&mut tw);
    assert_eq!(len(&collection(map, "dog")), 1);
    let main = Template::new("./test/stubs-virtual-nowrite/template.liquid", ctx, false);
    assert_eq!(output_path_of(&main), "./test/stubs/collection-template/_site/template/index.html");
    assert_eq!(
        rendered(map, "./test/stubs-virtual-nowrite/template.liquid"),
        "Layout\n\nTemplate\n\nAll 2 templates\nTemplate 1 dog"
    );
}

/// "Use a collection inside of a layout"
#[test]
fn use_a_collection_inside_of_a_layout() {
    let mut tw = virtual_writer(
        "test/stubs/collection-layout/_site",
        &["liquid"],
        &[
            (
                "_includes/layout.liquid",
                "Layout\n\n{{ content }}\n\nAll {{ collections.all | size }} templates\nLayout {{ collections.dog | size }} dog",
            ),
            ("dog1.liquid", "---\ntags:\n  - dog\n---"),
            ("template.liquid", "---\nlayout: layout.liquid\n---\nTemplate"),
        ],
        |_| Ok(()),
    );
    let ctx = tw_ctx(&tw);
    let map = template_map(&mut tw);
    assert_eq!(len(&collection(map, "dog")), 1);
    let main = Template::new("./test/stubs-virtual-nowrite/template.liquid", ctx, false);
    assert_eq!(output_path_of(&main), "./test/stubs/collection-layout/_site/template/index.html");
    assert_eq!(
        rendered(map, "./test/stubs-virtual-nowrite/template.liquid"),
        "Layout\n\nTemplate\n\nAll 2 templates\nLayout 1 dog"
    );
}

/// "Glob Watcher Files with Passthroughs"
#[test]
fn glob_watcher_files_with_passthroughs() {
    let tw = get_template_writer("test/stubs", "test/stubs/_site", &["njk", "png"], |_| Ok(()));
    assert_eq!(tw.get_passthrough_globs(), ["./test/stubs/**/*.png".to_string()]);
}

/// "Pagination and TemplateContent"
#[test]
fn pagination_and_template_content() {
    let site = "./test/stubs/pagination-templatecontent/_site/";
    let _ = std::fs::remove_dir_all(site);
    let mut tw = get_template_writer(
        "test/stubs/pagination-templatecontent",
        "test/stubs/pagination-templatecontent/_site",
        &["njk", "md"],
        |_| Ok(()),
    );
    tw.write().unwrap();
    let content = std::fs::read_to_string("./test/stubs/pagination-templatecontent/_site/index.html").unwrap();
    let _ = std::fs::remove_dir_all(site);
    assert_eq!(content.trim(), "<h1>Post 1</h1>\n<h1>Post 2</h1>");
}

/// "Custom collection returns array"
#[test]
fn custom_collection_returns_array() {
    let mut tw = virtual_writer("test/stubs/_site", &["md"], COLLECTION2_TEMPLATES, |cfg| {
        cfg.add_collection("returnAllInputPaths", |c| {
            Ok(Value::array(c.get_all_sorted().to_vec().iter().map(|item| item.get("inputPath")).collect()))
        })
    });
    let map = template_map(&mut tw);
    let items = collection(map, "returnAllInputPaths").to_vec();
    assert_eq!(items.len(), 2);
    assert_eq!(base(&items[0]), "test1.md");
    assert_eq!(base(&items[1]), "test2.md");
}

/// "Custom collection returns a string"
#[test]
fn custom_collection_returns_a_string() {
    let mut tw = virtual_writer("test/stubs/_site", &["md"], COLLECTION2_TEMPLATES, |cfg| {
        cfg.add_collection("returnATestString", |_| Ok(Value::str("test")))
    });
    let map = template_map(&mut tw);
    assert_eq!(collection(map, "returnATestString"), Value::str("test"));
}

/// "Custom collection returns an object"
#[test]
fn custom_collection_returns_an_object() {
    let mut tw = virtual_writer("test/stubs/_site", &["md"], COLLECTION2_TEMPLATES, |cfg| {
        cfg.add_collection("returnATestObject", |_| Ok(twelvety::testing::json(r#"{"test": "value"}"#)))
    });
    let map = template_map(&mut tw);
    assert_eq!(collection(map, "returnATestObject"), twelvety::testing::json(r#"{"test": "value"}"#));
}

/// "fileSlug should exist in a collection"
#[test]
fn file_slug_should_exist_in_a_collection() {
    let mut tw = virtual_writer(
        "test/stubs/collection-slug/_site",
        &["njk"],
        &[
            ("dog1.njk", "---\ntags:\n  - dog\n---"),
            ("template.njk", "fileSlug:{% for post in collections.dog %}{{ post.url }}:{{ post.fileSlug }}{% endfor %}"),
        ],
        |_| Ok(()),
    );
    let map = template_map(&mut tw);
    assert_eq!(len(&collection(map, "dog")), 1);
    assert_eq!(rendered(map, "./test/stubs-virtual-nowrite/template.njk"), "fileSlug:/dog1/:dog1");
}

// "Write Test 11ty.js": JavaScript templates (skipped). "Markdown with alias" and
// "JavaScript with alias" are skipped upstream.

/// "Passthrough file output"
#[test]
fn passthrough_file_output() {
    let site = "./test/stubs/template-passthrough/_site/";
    let _ = std::fs::remove_dir_all(site);
    let mut tw = get_template_writer(
        "test/stubs/template-passthrough",
        "test/stubs/template-passthrough/_site",
        &["njk", "md"],
        |cfg| {
            cfg.add_passthrough_copy("./test/stubs/template-passthrough/static");
            cfg.add_passthrough_copy_mapping("./test/stubs/template-passthrough/static/", "./");
            cfg.add_passthrough_copy_mapping("./test/stubs/template-passthrough/static/**/*", "./all/");
            cfg.add_passthrough_copy_mapping("./test/stubs/template-passthrough/static/**/*.js", "./js/");
            Ok(())
        },
    );
    let result = tw.write();
    let missing: Vec<&str> = [
        "./test/stubs/template-passthrough/_site/static/nested/test-nested.css",
        "./test/stubs/template-passthrough/_site/all/test.js",
        "./test/stubs/template-passthrough/_site/all/test.css",
        "./test/stubs/template-passthrough/_site/all/test-nested.css",
        "./test/stubs/template-passthrough/_site/js/",
        "./test/stubs/template-passthrough/_site/js/test.js",
        "./test/stubs/template-passthrough/_site/nested/",
        "./test/stubs/template-passthrough/_site/nested/test-nested.css",
        "./test/stubs/template-passthrough/_site/test.css",
        "./test/stubs/template-passthrough/_site/test.js",
    ]
    .into_iter()
    .filter(|p| !Path::new(p).exists())
    .collect();
    let _ = std::fs::remove_dir_all(site);
    result.unwrap();
    assert!(missing.is_empty(), "{missing:?}");
}
