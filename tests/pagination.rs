//! Ported from upstream `test/PaginationTest.js`.
//!
//! The `---js` front matter stubs (`before` callbacks, `pagination.data` as an Array/Object)
//! are ported by passing the same data (with Rust closures for functions) to `addTemplate`.
//!
//! Skipped (JS-only features):
//! - "Pagination mutable global data": uses a `testdata.cjs` global data file.
//! - "Pagination template/dir data files run once, Issue 919": uses a `test.data.cjs` data file.

use std::sync::Arc;

use twelvety::Value;
use twelvety::core::{Options, Twelvety};
use twelvety::page_entry::PageEntry;
use twelvety::plugins::pagination::Pagination;
use twelvety::template::Template;
use twelvety::testing::{TestOptions, get_new_template, json, render_template};

const ITEMS_LIST: &str =
    "<ol>{% for item in pagination.items %}<li>{{ item }}</li>{% endfor %}</ol>";

fn stub(relative_path: &str) -> String {
    let fm = match relative_path {
        "paged/notpaged.njk" => {
            "testdata:\n  sub:\n   - item1\n   - item2\n   - item3\n   - item4\n   - item5\n   - item6\n   - item7\n   - item8\n"
        }
        "paged/paged-empty-pageonemptydata.njk" => {
            "pagination:\n  data: items\n  size: 1\n  generatePageOnEmptyData: true\nitems: []\n"
        }
        "paged/paged-empty.njk" => "pagination:\n  data: items\n  size: 1\nitems: []\n",
        "paged/pagedalias.njk" => {
            return "---\npagination:\n  data: items\n  size: 1\n  alias: font.test\nitems:\n  - item1\n  - item2\npermalink: pagedalias/{{ font.test }}/index.html\n---\n{{ font.test }}\n".into();
        }
        "paged/pagedaliassize2.njk" => {
            return "---\npagination:\n  data: items\n  size: 2\n  alias: font.test\nitems:\n  - item1\n  - item2\n  - item3\n  - item4\npermalink: pagedalias/{{ font.test[0] }}/index.html\n---\n{{ font.test[0] }}\n".into();
        }
        "paged/pagedinlinedata-reverse.njk" => {
            return format!(
                "---\npagination:\n  data: testdata\n  size: 4\n  reverse: true\ntestdata:\n - item1\n - item2\n - item3\n - item4\n - item5\n - item6\n - item7\n - item8\n---\n{ITEMS_LIST}"
            );
        }
        "paged/pagedinlinedata.njk" => {
            "pagination:\n  data: testdata\n  size: 4\ntestdata:\n - item1\n - item2\n - item3\n - item4\n - item5\n - item6\n - item7\n - item8\n"
        }
        "paged/pagedobject.njk" => return object_stub(""),
        "paged/pagedobjectexcludearray.njk" => return object_stub("  exclude:\n    - item4\n"),
        "paged/pagedobjectfilterarray.njk" => return object_stub("  filter:\n    - item4\n"),
        "paged/pagedobjectfilterstring.njk" => return object_stub("  filter: item4\n"),
        "paged/pagedobjectvalues.njk" => return object_stub("  resolve: values\n"),
        "paged/pagedpermalink.njk" => {
            return permalink_stub("paged/{{ pagination.items[0] | slugify }}/index.html");
        }
        "paged/pagedpermalinkif.liquid" | "paged/pagedpermalinkif.njk" => {
            "pagination:\n  data: items\n  size: 2\nitems:\n  - item1\n  - item2\n  - item3\n  - item4\npermalink: paged/{% if pagination.pageNumber > 0 %}page-{{ pagination.pageNumber }}/{% endif %}index.html\n"
        }
        "paged/pagedpermalinkleadingslash.njk" => {
            return permalink_stub("/paged/page-{{ pagination.pageNumber }}/index.html");
        }
        "paged/pagedpermalinknumeric.njk" => {
            return permalink_stub("paged/page-{{ pagination.pageNumber }}/index.html");
        }
        "paged/pagedpermalinknumericoneindexed.njk" => {
            return permalink_stub("paged/page-{{ pagination.pageNumber + 1 }}/index.html");
        }
        "paged/pagedresolve.njk" => {
            "pagination:\n  data: testdata.sub\n  size: 4\ntestdata:\n  sub:\n   - item1\n   - item2\n   - item3\n   - item4\n   - item5\n   - item6\n   - item7\n   - item8\n"
        }
        "pagination-twelvetycomputed-permalink.liquid" => {
            return "---\nvenues:\n  - first\n  - second\n  - third\npagination:\n  data: venues\n  size: 1\n  alias: venue\n  addAllPagesToCollections: true\ntwelvetyComputed:\n  permalink: \"venues/{{ venue }}/\"\n---\n".into();
        }
        other => panic!("unknown stub {other}"),
    };
    format!("---\n{fm}---\n{ITEMS_LIST}\n")
}

fn object_stub(extra: &str) -> String {
    format!(
        "---\npagination:\n  data: testdata\n  size: 4\n{extra}testdata:\n  item1: itemvalue1\n  item2: itemvalue2\n  item3: itemvalue3\n  item4: itemvalue4\n  item5: itemvalue5\n  item6: itemvalue6\n  item7: itemvalue7\n  item8: itemvalue8\n  item9: itemvalue9\n---\n{ITEMS_LIST}\n"
    )
}

fn permalink_stub(permalink: &str) -> String {
    format!(
        "---\npagination:\n  data: items\n  size: 5\nitems:\n  - Slug CANDIDATE\n  - item2\n  - item3\n  - item4\n  - item5\n  - another-slug CandiDate\n  - item7\n  - item8\npermalink: {permalink}\n---\n{ITEMS_LIST}\n"
    )
}

fn virtual_template_with_data(relative_path: &str, content: String, data: Value) -> Arc<Template> {
    let path = relative_path.to_string();
    get_new_template(
        &format!("./test/stubs/{relative_path}"),
        TestOptions::new("./test/stubs/", "./dist").with_config(move |cfg| {
            // upstream creates these templates without a `TemplateData` instance: no
            // template/directory data files (e.g. `paged/paged.json` would apply to `paged/*`)
            cfg.set_data_file_suffixes(&[]);
            cfg.add_template(&path, &content, data.clone())
        }),
    )
    .unwrap()
}

fn get_new_virtual_template(relative_path: &str) -> Arc<Template> {
    virtual_template_with_data(relative_path, stub(relative_path), Value::object())
}

fn get_templates(tmpl: &Arc<Template>) -> Vec<Arc<PageEntry>> {
    let mut data = tmpl.get_data().unwrap();
    tmpl.get_templates(&mut data).unwrap()
}

/// `renderTemplate(page.template, page.data)`
fn render(page: &Arc<PageEntry>) -> String {
    page.template
        .render_page_entry_without_layout(page)
        .unwrap()
        .trim()
        .to_string()
}

fn output_path(page: &Arc<PageEntry>) -> String {
    page.output_path().to_output_string()
}

fn pagination(page: &Arc<PageEntry>, key: &str) -> Value {
    page.data().get("pagination").get_path(key)
}

fn paging(tmpl: &Arc<Template>, data: &Value) -> anyhow::Result<Pagination> {
    Pagination::new(tmpl.clone(), data, &tmpl.ctx().config)
}

/// The `test/stubs` project with its global data
fn stubs_template(path: &str) -> Arc<Template> {
    get_new_template(path, TestOptions::new("./test/stubs/", "./dist")).unwrap()
}

#[test]
fn no_data_passed_to_pagination() {
    let tmpl = get_new_virtual_template("paged/notpaged.njk");
    let paging = paging(&tmpl, &Value::object()).unwrap();
    assert_eq!(paging.paged_items().len(), 0);
    assert_eq!(paging.get_page_templates().unwrap().len(), 0);
}

#[test]
fn no_pagination() {
    let tmpl = get_new_virtual_template("paged/notpaged.njk");
    let data = tmpl.get_data().unwrap();
    let paging = paging(&tmpl, &data).unwrap();
    assert!(!data.get("pagination").is_truthy());
    assert_eq!(paging.get_page_count(), 0);
    assert_eq!(paging.paged_items().len(), 0);
    assert_eq!(paging.get_page_templates().unwrap().len(), 0);
}

#[test]
fn empty_paged_data() {
    let tmpl = get_new_virtual_template("paged/paged-empty.njk");
    let data = tmpl.get_data().unwrap();
    let paging = paging(&tmpl, &data).unwrap();
    assert_eq!(paging.get_page_count(), 0);
    assert_eq!(paging.paged_items().len(), 0);
    assert_eq!(paging.get_page_templates().unwrap().len(), 0);
}

#[test]
fn empty_paged_data_with_generate_page_on_empty_data() {
    let tmpl = get_new_virtual_template("paged/paged-empty-pageonemptydata.njk");
    let data = tmpl.get_data().unwrap();
    let paging = paging(&tmpl, &data).unwrap();
    assert_eq!(paging.get_page_count(), 1);
    assert_eq!(paging.paged_items().len(), 1);
    assert_eq!(paging.get_page_templates().unwrap().len(), 1);
}

#[test]
fn pagination_enabled_in_frontmatter() {
    let tmpl = get_new_virtual_template("paged/pagedresolve.njk");
    let data = tmpl.get_data().unwrap();
    let paging = paging(&tmpl, &data).unwrap();
    assert!(data.get("testdata").is_truthy());
    assert!(data.get_path("testdata.sub").is_truthy());
    assert!(data.get("pagination").is_truthy());
    assert_eq!(data.get_path("pagination.data"), Value::str("testdata.sub"));
    assert_eq!(paging.get_page_count(), 2);
    assert_eq!(data.get_path("pagination.size"), Value::from(4));
}

#[test]
fn resolve_paged_data_in_frontmatter() {
    let tmpl = get_new_virtual_template("paged/pagedresolve.njk");
    let data = tmpl.get_data().unwrap();
    let paging = paging(&tmpl, &data).unwrap();
    assert_eq!(paging.target().len(), 8);
    assert_eq!(paging.get_page_count(), 2);
    assert_eq!(paging.paged_items().len(), 2);
}

#[test]
fn paginate_data_in_frontmatter() {
    let pages = get_templates(&get_new_virtual_template("paged/pagedinlinedata.njk"));
    assert_eq!(pages.len(), 2);
    assert_eq!(
        output_path(&pages[0]),
        "./dist/paged/pagedinlinedata/index.html"
    );
    assert_eq!(
        render(&pages[0]),
        "<ol><li>item1</li><li>item2</li><li>item3</li><li>item4</li></ol>"
    );
    assert_eq!(
        output_path(&pages[1]),
        "./dist/paged/pagedinlinedata/1/index.html"
    );
    assert_eq!(
        render(&pages[1]),
        "<ol><li>item5</li><li>item6</li><li>item7</li><li>item8</li></ol>"
    );
}

#[test]
fn paginate_external_data_file() {
    let tmpl = stubs_template("./test/stubs/paged/paged.njk");
    let data = tmpl.get_data().unwrap();
    // local data
    assert!(!data.get_path("items.sub").is_empty());

    let pages = get_templates(&tmpl);
    assert_eq!(pages.len(), 2);
    assert_eq!(output_path(&pages[0]), "./dist/paged/paged/index.html");
    assert_eq!(
        render(&pages[0]),
        "<ol><li>item1</li><li>item2</li><li>item3</li><li>item4</li><li>item5</li></ol>"
    );
    assert_eq!(output_path(&pages[1]), "./dist/paged/paged/1/index.html");
    assert_eq!(
        render(&pages[1]),
        "<ol><li>item6</li><li>item7</li><li>item8</li></ol>"
    );
}

#[test]
fn permalink_with_pagination_variables() {
    let pages = get_templates(&get_new_virtual_template("paged/pagedpermalink.njk"));
    assert_eq!(
        output_path(&pages[0]),
        "./dist/paged/slug-candidate/index.html"
    );
    assert_eq!(
        output_path(&pages[1]),
        "./dist/paged/another-slug-candidate/index.html"
    );
}

#[test]
fn permalink_with_pagination_variables_numeric() {
    let pages = get_templates(&get_new_virtual_template("paged/pagedpermalinknumeric.njk"));

    assert!(pagination(&pages[0], "firstPageLink").is_truthy());
    assert!(pagination(&pages[0], "firstPageHref").is_truthy());
    assert!(pagination(&pages[0], "lastPageLink").is_truthy());
    assert!(pagination(&pages[0], "lastPageHref").is_truthy());
    assert_eq!(output_path(&pages[0]), "./dist/paged/page-0/index.html");
    assert!(!pagination(&pages[0], "previousPageLink").is_truthy());
    assert_eq!(
        pagination(&pages[0], "nextPageLink"),
        Value::str("/paged/page-1/index.html")
    );
    assert_eq!(
        pagination(&pages[0], "nextPageHref"),
        Value::str("/paged/page-1/")
    );
    assert_eq!(pagination(&pages[0], "pageLinks").len(), 2);
    assert_eq!(pagination(&pages[0], "links").len(), 2);
    assert_eq!(pagination(&pages[0], "hrefs").len(), 2);

    assert_eq!(output_path(&pages[1]), "./dist/paged/page-1/index.html");
    assert_eq!(
        pagination(&pages[1], "previousPageLink"),
        Value::str("/paged/page-0/index.html")
    );
    assert_eq!(
        pagination(&pages[1], "previousPageHref"),
        Value::str("/paged/page-0/")
    );
    assert!(!pagination(&pages[1], "nextPageLink").is_truthy());
    assert_eq!(pagination(&pages[1], "pageLinks").len(), 2);
    assert_eq!(pagination(&pages[1], "links").len(), 2);
    assert_eq!(pagination(&pages[1], "hrefs").len(), 2);
}

#[test]
fn pagination_links_with_a_leading_slash_permalink_issue_1350() {
    let pages = get_templates(&get_new_virtual_template(
        "paged/pagedpermalinkleadingslash.njk",
    ));
    assert_eq!(
        pagination(&pages[0], "links"),
        json(r#"["/paged/page-0/index.html", "/paged/page-1/index.html"]"#)
    );
    assert_eq!(
        pagination(&pages[0], "nextPageLink"),
        Value::str("/paged/page-1/index.html")
    );
    assert_eq!(
        pagination(&pages[1], "previousPageLink"),
        Value::str("/paged/page-0/index.html")
    );
    assert_eq!(
        pagination(&pages[1], "firstPageLink"),
        Value::str("/paged/page-0/index.html")
    );
    assert_eq!(
        pagination(&pages[0], "hrefs"),
        json(r#"["/paged/page-0/", "/paged/page-1/"]"#)
    );
}

#[test]
fn permalink_with_pagination_variables_numeric_one_indexed() {
    let pages = get_templates(&get_new_virtual_template(
        "paged/pagedpermalinknumericoneindexed.njk",
    ));

    assert_eq!(output_path(&pages[0]), "./dist/paged/page-1/index.html");
    assert!(!pagination(&pages[0], "previousPageLink").is_truthy());
    assert_eq!(
        pagination(&pages[0], "nextPageLink"),
        Value::str("/paged/page-2/index.html")
    );
    assert_eq!(
        pagination(&pages[0], "nextPageHref"),
        Value::str("/paged/page-2/")
    );
    assert_eq!(pagination(&pages[0], "pageLinks").len(), 2);
    assert_eq!(pagination(&pages[0], "links").len(), 2);
    assert_eq!(pagination(&pages[0], "hrefs").len(), 2);

    assert_eq!(output_path(&pages[1]), "./dist/paged/page-2/index.html");
    assert_eq!(
        pagination(&pages[1], "previousPageLink"),
        Value::str("/paged/page-1/index.html")
    );
    assert_eq!(
        pagination(&pages[1], "previousPageHref"),
        Value::str("/paged/page-1/")
    );
    assert!(!pagination(&pages[1], "nextPageLink").is_truthy());
    assert_eq!(pagination(&pages[1], "pageLinks").len(), 2);
    assert_eq!(pagination(&pages[1], "links").len(), 2);
    assert_eq!(pagination(&pages[1], "hrefs").len(), 2);
}

#[test]
fn permalink_first_and_last_page_link_numeric() {
    let pages = get_templates(&get_new_virtual_template("paged/pagedpermalinknumeric.njk"));
    for page in &pages {
        assert_eq!(
            pagination(page, "firstPageLink"),
            Value::str("/paged/page-0/index.html")
        );
        assert_eq!(
            pagination(page, "lastPageLink"),
            Value::str("/paged/page-1/index.html")
        );
    }
}

#[test]
fn permalink_first_and_last_page_link_numeric_one_indexed() {
    let pages = get_templates(&get_new_virtual_template(
        "paged/pagedpermalinknumericoneindexed.njk",
    ));
    for page in &pages {
        assert_eq!(
            pagination(page, "firstPageLink"),
            Value::str("/paged/page-1/index.html")
        );
        assert_eq!(
            pagination(page, "lastPageLink"),
            Value::str("/paged/page-2/index.html")
        );
    }
}

#[test]
fn alias_to_page_data() {
    let pages = get_templates(&get_new_virtual_template("paged/pagedalias.njk"));
    assert_eq!(output_path(&pages[0]), "./dist/pagedalias/item1/index.html");
    assert_eq!(output_path(&pages[1]), "./dist/pagedalias/item2/index.html");
    assert_eq!(render(&pages[0]), "item1");
    assert_eq!(render(&pages[1]), "item2");
}

#[test]
fn alias_to_page_data_size_2() {
    let pages = get_templates(&get_new_virtual_template("paged/pagedaliassize2.njk"));
    assert_eq!(output_path(&pages[0]), "./dist/pagedalias/item1/index.html");
    assert_eq!(output_path(&pages[1]), "./dist/pagedalias/item3/index.html");
    assert_eq!(render(&pages[0]), "item1");
    assert_eq!(render(&pages[1]), "item3");
}

#[test]
fn permalink_with_pagination_variables_and_an_if_statement_nunjucks() {
    let pages = get_templates(&get_new_virtual_template("paged/pagedpermalinkif.njk"));
    assert_eq!(output_path(&pages[0]), "./dist/paged/index.html");
    assert_eq!(output_path(&pages[1]), "./dist/paged/page-1/index.html");
}

#[test]
fn permalink_with_pagination_variables_and_an_if_statement_liquid() {
    let pages = get_templates(&get_new_virtual_template("paged/pagedpermalinkif.liquid"));
    assert_eq!(output_path(&pages[0]), "./dist/paged/index.html");
    assert_eq!(output_path(&pages[1]), "./dist/paged/page-1/index.html");
}

#[test]
fn template_with_pagination() {
    let tmpl = get_new_virtual_template("paged/pagedpermalinkif.njk");
    let data = tmpl.get_data().unwrap();
    let loc = tmpl.get_output_locations(&data, "").unwrap();
    assert_eq!(loc.path, Value::str("./dist/paged/index.html"));
    assert_eq!(render_template(&tmpl).unwrap().len(), 2);
}

#[test]
fn issue_135() {
    let tmpl = stubs_template("./test/stubs/issue-135/template.njk");
    let data = tmpl.get_data().unwrap();
    let templates = render_template(&tmpl).unwrap();
    assert_eq!(data.get("articles").len(), 1);
    assert_eq!(
        data.get_path("articles.0.title"),
        Value::str("Do you even paginate bro?")
    );
    assert_eq!(
        templates[0].output_path,
        Value::str("./dist/blog/do-you-even-paginate-bro/index.html")
    );

    let pages = get_templates(&tmpl);
    assert_eq!(pages.len(), 1);
    assert_eq!(
        output_path(&pages[0]),
        "./dist/blog/do-you-even-paginate-bro/index.html"
    );
}

#[test]
fn get_templates_has_page_variables_set() {
    let pages = get_templates(&get_new_virtual_template("paged/pagedpermalinkif.njk"));
    assert_eq!(pages[0].data().get_path("page.url"), Value::str("/paged/"));
    assert_eq!(
        pages[0].data().get_path("page.outputPath"),
        Value::str("./dist/paged/index.html")
    );
    assert_eq!(
        pages[1].data().get_path("page.url"),
        Value::str("/paged/page-1/")
    );
    assert_eq!(
        pages[1].data().get_path("page.outputPath"),
        Value::str("./dist/paged/page-1/index.html")
    );
}

#[test]
fn rendered_templates_have_page_variables_set() {
    let pages = render_template(&get_new_virtual_template("paged/pagedpermalinkif.njk")).unwrap();
    assert_eq!(pages[0].data.get_path("page.url"), Value::str("/paged/"));
    assert_eq!(
        pages[0].data.get_path("page.outputPath"),
        Value::str("./dist/paged/index.html")
    );
    assert_eq!(
        pages[1].data.get_path("page.url"),
        Value::str("/paged/page-1/")
    );
    assert_eq!(
        pages[1].data.get_path("page.outputPath"),
        Value::str("./dist/paged/page-1/index.html")
    );
}

#[test]
fn page_over_an_object_use_keys() {
    let pages = get_templates(&get_new_virtual_template("paged/pagedobject.njk"));
    assert_eq!(pages.len(), 3);
    assert_eq!(
        output_path(&pages[0]),
        "./dist/paged/pagedobject/index.html"
    );
    assert_eq!(
        render(&pages[0]),
        "<ol><li>item1</li><li>item2</li><li>item3</li><li>item4</li></ol>"
    );
    assert_eq!(
        output_path(&pages[1]),
        "./dist/paged/pagedobject/1/index.html"
    );
    assert_eq!(
        render(&pages[1]),
        "<ol><li>item5</li><li>item6</li><li>item7</li><li>item8</li></ol>"
    );
}

#[test]
fn page_over_an_object_use_values() {
    let pages = get_templates(&get_new_virtual_template("paged/pagedobjectvalues.njk"));
    assert_eq!(pages.len(), 3);
    assert_eq!(
        output_path(&pages[0]),
        "./dist/paged/pagedobjectvalues/index.html"
    );
    assert_eq!(
        render(&pages[0]),
        "<ol><li>itemvalue1</li><li>itemvalue2</li><li>itemvalue3</li><li>itemvalue4</li></ol>"
    );
    assert_eq!(
        output_path(&pages[1]),
        "./dist/paged/pagedobjectvalues/1/index.html"
    );
    assert_eq!(
        render(&pages[1]),
        "<ol><li>itemvalue5</li><li>itemvalue6</li><li>itemvalue7</li><li>itemvalue8</li></ol>"
    );
}

#[test]
fn page_over_an_object_filtered_array() {
    let pages = get_templates(&get_new_virtual_template(
        "paged/pagedobjectfilterarray.njk",
    ));
    assert_eq!(
        render(&pages[0]),
        "<ol><li>item1</li><li>item2</li><li>item3</li><li>item5</li></ol>"
    );
    assert_eq!(
        render(&pages[1]),
        "<ol><li>item6</li><li>item7</li><li>item8</li><li>item9</li></ol>"
    );
}

#[test]
fn page_over_an_object_exclude_array() {
    let pages = get_templates(&get_new_virtual_template(
        "paged/pagedobjectexcludearray.njk",
    ));
    assert_eq!(
        render(&pages[0]),
        "<ol><li>item1</li><li>item2</li><li>item3</li><li>item5</li></ol>"
    );
}

#[test]
fn pagination_exclude_with_a_falsy_value() {
    let tmpl = get_new_virtual_template("paged/notpaged.njk");
    let paging = paging(
        &tmpl,
        &json(r#"{"pagination": {"data": "items", "size": 1, "exclude": 0}, "items": [0, 1, 2]}"#),
    )
    .unwrap();
    assert_eq!(paging.target(), &[Value::from(1), Value::from(2)]);
}

#[test]
fn page_over_an_object_filtered_string() {
    let pages = get_templates(&get_new_virtual_template(
        "paged/pagedobjectfilterstring.njk",
    ));
    assert_eq!(pages.len(), 2);
    assert_eq!(
        render(&pages[0]),
        "<ol><li>item1</li><li>item2</li><li>item3</li><li>item5</li></ol>"
    );
    assert_eq!(
        render(&pages[1]),
        "<ol><li>item6</li><li>item7</li><li>item8</li><li>item9</li></ol>"
    );
}

#[test]
fn pagination_with_deep_data_merge_147() {
    // upstream also sets `tmpl.config.keys.layout = "layout"` (the default)
    let pages = get_templates(&get_new_virtual_template("paged/pagedinlinedata.njk"));
    assert_eq!(pages.len(), 2);
    assert_eq!(
        output_path(&pages[0]),
        "./dist/paged/pagedinlinedata/index.html"
    );
    assert_eq!(
        render(&pages[0]),
        "<ol><li>item1</li><li>item2</li><li>item3</li><li>item4</li></ol>"
    );
    assert_eq!(
        output_path(&pages[1]),
        "./dist/paged/pagedinlinedata/1/index.html"
    );
    assert_eq!(
        render(&pages[1]),
        "<ol><li>item5</li><li>item6</li><li>item7</li><li>item8</li></ol>"
    );
}

#[test]
fn pagination_with_deep_data_merge_with_alias_147() {
    // upstream also sets `tmpl.config.dynamicPermalinks = true` (the default)
    let pages = get_templates(&get_new_virtual_template("paged/pagedalias.njk"));
    assert_eq!(output_path(&pages[0]), "./dist/pagedalias/item1/index.html");
    assert_eq!(output_path(&pages[1]), "./dist/pagedalias/item2/index.html");
    assert_eq!(render(&pages[0]), "item1");
    assert_eq!(render(&pages[1]), "item2");
}

#[test]
fn paginate_data_in_frontmatter_reversed() {
    let pages = get_templates(&get_new_virtual_template(
        "paged/pagedinlinedata-reverse.njk",
    ));
    assert_eq!(pages.len(), 2);
    assert_eq!(
        output_path(&pages[0]),
        "./dist/paged/pagedinlinedata-reverse/index.html"
    );
    assert_eq!(
        render(&pages[0]),
        "<ol><li>item8</li><li>item7</li><li>item6</li><li>item5</li></ol>"
    );
    assert_eq!(
        output_path(&pages[1]),
        "./dist/paged/pagedinlinedata-reverse/1/index.html"
    );
    assert_eq!(
        render(&pages[1]),
        "<ol><li>item4</li><li>item3</li><li>item2</li><li>item1</li></ol>"
    );
}

#[test]
fn no_circular_dependency_does_not_throw() {
    let tmpl = get_new_virtual_template("paged/notpaged.njk");
    paging(
        &tmpl,
        &json(r#"{"collections": {"tag1": []}, "pagination": {"data": "collections.tag1", "size": 1}, "tags": ["tag2"]}"#),
    )
    .unwrap();
}

#[test]
fn circular_dependency_pagination_iterates_over_tag1_but_also_supplies_pages_to_tag1() {
    let tmpl = get_new_virtual_template("paged/notpaged.njk");
    assert!(
        paging(
            &tmpl,
            &json(r#"{"collections": {"tag1": [], "tag2": []}, "pagination": {"data": "collections.tag1", "size": 1}, "tags": ["tag1"]}"#),
        )
        .is_err()
    );
}

#[test]
fn circular_dependency_but_uses_twelvety_exclude_from_collections() {
    let tmpl = get_new_virtual_template("paged/notpaged.njk");
    paging(
        &tmpl,
        &json(
            r#"{"twelvetyExcludeFromCollections": true, "collections": {"tag1": [], "tag2": []}, "pagination": {"data": "collections.tag1", "size": 1}, "tags": ["tag1"]}"#,
        ),
    )
    .unwrap();
}

const SIX_ITEMS: &str = r#"["item1", "item2", "item3", "item4", "item5", "item6"]"#;

/// A `---js` front matter stub: `pagination` (with a `before` callback) and `items`
fn before_template(
    relative_path: &str,
    pagination: &str,
    extra: &str,
    before: fn(Vec<Value>, &Value) -> Vec<Value>,
) -> Arc<Template> {
    let mut data = json(&format!(
        r#"{{"pagination": {pagination}, "items": {SIX_ITEMS}{extra}}}"#
    ));
    let mut p = data.get("pagination");
    p.set(
        "before",
        Value::function(move |args| Ok(Value::array(before(args[0].to_vec(), &args[1])))),
    );
    data.set("pagination", p);
    virtual_template_with_data(relative_path, ITEMS_LIST.to_string(), data)
}

#[test]
fn pagination_before_callback() {
    let tmpl = before_template(
        "paged/paged-before.njk",
        r#"{"data": "items", "size": 1, "alias": "myalias"}"#,
        "",
        |mut items, _| {
            items.reverse();
            items
        },
    );
    let pages = get_templates(&tmpl);
    assert_eq!(pagination(&pages[0], "items"), json(r#"["item6"]"#));
    assert_eq!(pages[0].data().get("myalias"), Value::str("item6"));
}

#[test]
fn pagination_before_callback_with_metadata() {
    let tmpl = before_template(
        "paged/paged-before-metadata.njk",
        r#"{"data": "items", "size": 1}"#,
        r#", "keyword": "item3""#,
        |items, metadata| {
            items
                .into_iter()
                .filter(|el| *el == metadata.get("keyword"))
                .collect()
        },
    );
    let pages = get_templates(&tmpl);
    assert_eq!(pagination(&pages[0], "items"), json(r#"["item3"]"#));
}

#[test]
fn pagination_before_callback_with_a_filter() {
    let tmpl = before_template(
        "paged/paged-before-filter.njk",
        r#"{"data": "items", "size": 1, "alias": "myalias"}"#,
        "",
        |items, _| items.into_iter().take(2).rev().collect(),
    );
    let pages = get_templates(&tmpl);
    assert_eq!(pagination(&pages[0], "items"), json(r#"["item2"]"#));
    assert_eq!(pages[0].data().get("myalias"), Value::str("item2"));
}

#[test]
fn pagination_data_as_an_array_issue_4420() {
    let tmpl = virtual_template_with_data(
        "paged/paged-data-direct.njk",
        format!("{ITEMS_LIST}\n"),
        json(r#"{"pagination": {"data": ["item1", "item2", "item3"], "size": 2}}"#),
    );
    let pages = get_templates(&tmpl);
    assert_eq!(pages.len(), 2);
    assert_eq!(
        pagination(&pages[0], "items"),
        json(r#"["item1", "item2"]"#)
    );
    assert_eq!(pagination(&pages[1], "items"), json(r#"["item3"]"#));
}

#[test]
fn pagination_data_as_an_object_issue_4420() {
    let tmpl = virtual_template_with_data(
        "paged/paged-data-direct-object.njk",
        "{{ pagination.items[0] }}\n".into(),
        json(
            r#"{"pagination": {"data": {"a": "item1", "b": "item2"}, "size": 1, "resolve": "values"}}"#,
        ),
    );
    let pages = get_templates(&tmpl);
    assert_eq!(pages.len(), 2);
    assert_eq!(pagination(&pages[0], "items"), json(r#"["item1"]"#));
    assert_eq!(pagination(&pages[1], "items"), json(r#"["item2"]"#));
}

#[test]
fn pagination_before_callback_with_reverse_order_of_operations() {
    let tmpl = before_template(
        "paged/paged-before-and-reverse.njk",
        r#"{"data": "items", "size": 1, "reverse": true}"#,
        "",
        |items, _| items.into_iter().take(2).collect(),
    );
    let pages = get_templates(&tmpl);
    assert_eq!(pagination(&pages[0], "items"), json(r#"["item2"]"#));
}

#[test]
fn pagination_new_v0_10_0_href_hrefs() {
    let pages = get_templates(&stubs_template("./test/stubs/paged/paged.njk"));
    assert_eq!(pagination(&pages[0], "hrefs").len(), 2);
    assert!(pagination(&pages[0], "href.first").is_truthy());
    assert!(pagination(&pages[0], "href.last").is_truthy());
    assert!(!pagination(&pages[0], "href.previous").is_truthy());
    assert!(pagination(&pages[0], "href.next").is_truthy());

    assert_eq!(pagination(&pages[1], "hrefs").len(), 2);
    assert!(pagination(&pages[1], "href.first").is_truthy());
    assert!(pagination(&pages[1], "href.last").is_truthy());
    assert!(pagination(&pages[1], "href.previous").is_truthy());
    assert!(!pagination(&pages[1], "href.next").is_truthy());
}

#[test]
fn pagination_new_v0_10_0_page_pages() {
    let pages = get_templates(&stubs_template("./test/stubs/paged/paged.njk"));
    for (i, page) in pages.iter().enumerate() {
        assert_eq!(pagination(page, "pages").len(), 2);
        assert_eq!(pagination(page, "pages.0").len(), 5);
        assert_eq!(pagination(page, "pages.1").len(), 3);
        assert!(pagination(page, "page.first").is_truthy());
        assert!(pagination(page, "page.last").is_truthy());
        assert_eq!(pagination(page, "page.previous").is_truthy(), i == 1);
        assert_eq!(pagination(page, "page.next").is_truthy(), i == 0);
    }
}

#[test]
fn pagination_new_v0_10_0_alias() {
    let pages = get_templates(&get_new_virtual_template("paged/pagedalias.njk"));
    assert_eq!(pagination(&pages[0], "alias"), Value::str("font.test"));
    assert_eq!(pagination(&pages[1], "alias"), Value::str("font.test"));
}

#[test]
fn pagination_page_number_is_numeric_issue_760() {
    let pages = get_templates(&get_new_virtual_template("paged/pagedinlinedata.njk"));
    assert_eq!(pagination(&pages[0], "pageNumber"), Value::Number(0.0));
    assert_ne!(pagination(&pages[0], "pageNumber"), Value::str("0"));
}

#[test]
#[ignore = "JS-only: the stubs-919 template data file is `test.data.cjs`"]
fn pagination_template_dir_data_files_run_once_issue_919() {}

#[test]
#[ignore = "JS-only: the paged-global-data-mutable global data file is `testdata.cjs`"]
fn pagination_mutable_global_data() {}

#[test]
fn pagination_and_twelvety_computed_permalink_issue_1555_and_1865() {
    let pages = get_templates(&get_new_virtual_template(
        "pagination-twelvetycomputed-permalink.liquid",
    ));
    assert_eq!(
        pages[0].data().get_path("page.url"),
        Value::str("/venues/first/")
    );
    assert_eq!(
        pages[1].data().get_path("page.url"),
        Value::str("/venues/second/")
    );
    assert_eq!(
        pages[2].data().get_path("page.url"),
        Value::str("/venues/third/")
    );
}

fn no_config_file() -> Options {
    Options {
        config_path: Some(String::new()),
        quiet_mode: Some(true),
        ..Default::default()
    }
}

#[test]
fn pagination_and_twelvety_computed_data_issues_2512_2837_3013() {
    for lang in ["liquid", "html", "md", "njk"] {
        let apostrophe = if lang == "njk" { "&amp;#39;" } else { "'" };
        let le = if lang == "md" { "\n" } else { "" };
        let mut elev = Twelvety::new(
            Some(&format!("./test/stubs-3013/{lang}/")),
            Some(&format!("./test/stubs-3013/{lang}/_site")),
            Options {
                source: Some("cli".into()),
                run_mode: Some("build".into()),
                ..no_config_file()
            },
        );
        let written = elev.to_json().unwrap();
        assert_eq!(written[0].url, Value::str("/paul-mescal/"), "lang: {lang}");
        assert_eq!(
            written[0].content,
            format!("<title>The Effervescent adventures of Paul Mescal</title>{le}"),
            "lang: {lang}"
        );
        assert_eq!(
            written[1].url,
            Value::str("/populace-and-power/"),
            "lang: {lang}"
        );
        assert_eq!(
            written[1].content,
            format!("<title>Populace and Power: A user{apostrophe}s guide</title>{le}"),
            "lang: {lang}"
        );
    }
}

#[test]
fn pagination_data_keys_and_values_are_stable() {
    // upstream renders the data from an `12ty.js` template, this reads the same page data
    let tmpl = virtual_template_with_data(
        "paged.njk",
        String::new(),
        json(r#"{"pagination": {"data": "letters", "size": 1}, "letters": ["a", "b", "c"]}"#),
    );
    let pages = get_templates(&tmpl);
    let second = pages
        .iter()
        .find(|p| p.url() == Value::str("/paged/1/"))
        .unwrap();
    let p = second.data().get("pagination");
    assert_eq!(
        p.keys(),
        [
            "data",
            "size",
            "alias",
            "pages",
            "items",
            "page",
            "pageNumber",
            "previousPageLink",
            "previous",
            "nextPageLink",
            "next",
            "firstPageLink",
            "lastPageLink",
            "links",
            "pageLinks",
            "previousPageHref",
            "nextPageHref",
            "firstPageHref",
            "lastPageHref",
            "hrefs",
            "href",
        ]
    );
    assert_eq!(p.get("pageNumber"), Value::from(1));
    assert_eq!(p.get("items"), json(r#"["b"]"#));
    assert_eq!(
        p.get("page"),
        json(r#"{"previous": "a", "next": "c", "first": "a", "last": "c"}"#)
    );
    assert_eq!(p.get("previous"), Value::str("/paged/index.html"));
    assert_eq!(p.get("next"), Value::str("/paged/2/index.html"));
    assert_eq!(
        p.get("hrefs"),
        json(r#"["/paged/", "/paged/1/", "/paged/2/"]"#)
    );
    assert_eq!(
        p.get("href"),
        json(
            r#"{"previous": "/paged/", "next": "/paged/2/", "first": "/paged/", "last": "/paged/2/"}"#
        )
    );
}

#[test]
fn paginated_nunjucks_templates_share_a_compiled_template_but_render_each_page() {
    let mut elev = Twelvety::new(
        Some("./test/stubs-virtual/"),
        None,
        no_config_file().with_config(|cfg| {
            cfg.add_template(
                "paged.njk",
                r#"{% set tag = "b" %}<{{ tag }}>{{ item }}</{{ tag }}>"#,
                json(r#"{"pagination": {"data": "letters", "size": 1, "alias": "item"}, "letters": ["a", "b", "c"], "permalink": "/{{ item }}/"}"#),
            )
        }),
    );
    let mut results: Vec<(String, String)> = elev
        .to_json()
        .unwrap()
        .into_iter()
        .map(|r| (r.url.to_output_string(), r.content))
        .collect();
    results.sort();
    assert_eq!(
        results,
        [
            ("/a/", "<b>a</b>"),
            ("/b/", "<b>b</b>"),
            ("/c/", "<b>c</b>")
        ]
        .map(|(a, b)| (a.to_string(), b.to_string()))
    );
}
