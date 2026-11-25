//! Ported from upstream `test/TemplateCollectionTest.js`. Collection items are compared by
//! `inputPath` (the port’s items don’t expose the `Template` instance to JS-style lookups).

use std::sync::Arc;

use twelvety::Value;
use twelvety::build_context::BuildContext;
use twelvety::template::Template;
use twelvety::template_collection::{TemplateCollection, sort_by_date, sort_by_date_input_path};
use twelvety::testing::{TestOptions, get_context, template_with_context};
use twelvety::util::glob_matcher::is_glob_match;

// A shared date keeps the input path as the sort tiebreaker.
const COLLECTION_TEMPLATES: &[(&str, &str)] = &[
    (
        "test/stubs/collection/test1.md",
        "---\ntitle: Test Title\ndate: 2020-01-01\ntags:\n  - post\n  - dog\n---\n\n# Test 1\n",
    ),
    (
        "test/stubs/collection/test2.md",
        "---\ndate: 2020-01-01\ntags: cat\n---\n\n# Test 2\n",
    ),
    (
        "test/stubs/collection/test3.md",
        "---\ndate: 2020-01-01\ntags:\n  - post\n  - cat\n---\n\n# Test 3\n",
    ),
    (
        "test/stubs/collection/test4.md",
        "---\ndate: 1983-01-01\n---\n\n# Test 3\n",
    ),
    (
        "test/stubs/collection/test5.md",
        "---\ndate: 2038-01-01\n---\n\n# Test 3\n",
    ),
    ("test/stubs/collection/test6.html", "# Test 6\n"),
    ("test/stubs/collection/test7.njk", "# Test 7"),
    (
        "test/stubs/collection/test8.md",
        "---\ntitle: Test Title\ndate: 2020-01-01\ntags:\n  - post\n  - office\ntwelvetyExcludeFromCollections: true\n---\n\n# Test 1\n",
    ),
    (
        "test/stubs/collection/test9.md",
        "---\ntitle: Test Title\ndate: 2020-01-01\ntags:\n  - post\n  - office\ntwelvetyExcludeFromCollections: post\n---\n\n# Test 1\n",
    ),
    (
        "test/stubs/collection/test10.md",
        "---\ntitle: Test Title\ndate: 2020-01-01\ntags:\n  - post\n  - office\ntwelvetyExcludeFromCollections:\n  - post\n---\n\n# Test 1\n",
    ),
    (
        "test/stubs/issue-95/cat.md",
        "---\ntags: \n  - cat\n---\n\n# Test 8\n",
    ),
    (
        "test/stubs/issue-95/notacat.md",
        "---\ntags: notacat\n---\n\n# Test 8\n",
    ),
];

const EXTENSIONS: [&str; 10] = [
    "md", "md", "md", "md", "md", "html", "njk", "md", "md", "md",
];

fn get_config() -> Arc<BuildContext> {
    get_context(
        TestOptions::new("./test/stubs/", "./test/stubs/_site").with_config(|cfg| {
            for (path, content) in COLLECTION_TEMPLATES {
                // virtual template paths are relative to the input directory
                cfg.add_template(
                    path.trim_start_matches("test/stubs/"),
                    content,
                    Value::object(),
                )?;
            }
            Ok(())
        }),
    )
    .unwrap()
}

fn path_for(num: usize) -> String {
    format!("./test/stubs/collection/test{num}.{}", EXTENSIONS[num - 1])
}

fn by_number(num: usize, ctx: &Arc<BuildContext>) -> Arc<Template> {
    template_with_context(&path_for(num), ctx)
}

fn add_template(c: &mut TemplateCollection, template: &Arc<Template>) {
    let mut data = template.get_data().unwrap();
    for page in template.get_templates(&mut data).unwrap() {
        c.add(page);
    }
}

/// A collection built from the numbered templates (in this order)
fn collection(nums: &[usize]) -> TemplateCollection {
    let ctx = get_config();
    let mut c = TemplateCollection::new();
    for num in nums {
        add_template(&mut c, &by_number(*num, &ctx));
    }
    c
}

fn input_paths(items: &Value) -> Vec<String> {
    items
        .to_vec()
        .iter()
        .map(|i| i.get("inputPath").to_output_string())
        .collect()
}

fn paths(nums: &[usize]) -> Vec<String> {
    nums.iter().map(|n| path_for(*n)).collect()
}

#[test]
fn basic_setup() {
    assert_eq!(collection(&[1, 2, 3]).len(), 3);
}

#[test]
fn sort_function_date() {
    let c = collection(&[1, 4, 5]);
    let posts: Vec<String> = c
        .sort_entries(sort_by_date)
        .iter()
        .map(|p| p.input_path.clone())
        .collect();
    assert_eq!(posts, paths(&[4, 1, 5]));
}

#[test]
fn sort_function_date_input_path() {
    let c = collection(&[1, 4, 5]);
    let posts: Vec<String> = c
        .sort_entries(sort_by_date_input_path)
        .iter()
        .map(|p| p.input_path.clone())
        .collect();
    assert_eq!(posts, paths(&[4, 1, 5]));
}

#[test]
fn get_filtered_by_tag() {
    let c = collection(&[1, 2, 3]);
    assert_eq!(input_paths(&c.get_filtered_by_tag("post")), paths(&[1, 3]));
    assert_eq!(input_paths(&c.get_filtered_by_tag("cat")), paths(&[2, 3]));
    assert_eq!(input_paths(&c.get_filtered_by_tag("dog")), paths(&[1]));
}

#[test]
fn get_filtered_by_tag_added_out_of_order_sorted() {
    let c = collection(&[3, 2, 1]);
    assert_eq!(input_paths(&c.get_filtered_by_tag("post")), paths(&[1, 3]));
    assert_eq!(input_paths(&c.get_filtered_by_tag("cat")), paths(&[2, 3]));
    assert_eq!(input_paths(&c.get_filtered_by_tag("dog")), paths(&[1]));
}

#[test]
fn get_filtered_by_tag_results_are_cached_cache_is_invalidated_by_add() {
    let ctx = get_config();
    let mut c = TemplateCollection::new();
    add_template(&mut c, &by_number(1, &ctx));

    let posts = c.get_filtered_by_tag("post");
    assert_eq!(posts.len(), 1);

    add_template(&mut c, &by_number(2, &ctx));
    add_template(&mut c, &by_number(3, &ctx));

    assert_eq!(c.get_filtered_by_tag("post").len(), 2);
    assert_eq!(c.get_filtered_by_tag("cat").len(), 2);
    assert_eq!(c.get_filtered_by_tags(&["post", "cat"]).len(), 1);
    assert_eq!(c.get_all_sorted().len(), 3);

    // The result returned before the additions is unaffected
    assert_eq!(posts.len(), 1);
}

#[test]
fn get_filtered_by_tag_results_are_mutable_copies() {
    let c = collection(&[1, 3]);
    let mut posts = c.get_filtered_by_tag("post");
    let items = posts.as_array_mut().unwrap();
    items.reverse();
    items.pop();

    assert_eq!(input_paths(&c.get_filtered_by_tag("post")), paths(&[1, 3]));

    let mut posts_plural = c.get_filtered_by_tags(&["post"]);
    posts_plural.as_array_mut().unwrap().pop();
    assert_eq!(c.get_filtered_by_tags(&["post"]).len(), 2);
}

#[test]
fn get_filtered_by_tag_no_tag_name_returns_everything_sorted() {
    let c = collection(&[1, 4, 5]);
    assert_eq!(input_paths(&c.get_filtered_by_tag("")), paths(&[4, 1, 5]));

    // `all` is not a tag name, it is filtered out of the included tag names
    assert_eq!(c.get_filtered_by_tag("all").len(), 0);
    assert_eq!(c.get_filtered_by_tag("unknown-tag").len(), 0);
    assert_eq!(c.get_filtered_by_tags(&[]).len(), 3);
}

#[test]
fn get_filtered_by_tags() {
    let c = collection(&[1, 2, 3]);
    assert_eq!(
        input_paths(&c.get_filtered_by_tags(&["post", "cat"])),
        paths(&[3])
    );
    assert_eq!(
        input_paths(&c.get_filtered_by_tags(&["cat"])),
        paths(&[2, 3])
    );
    assert_eq!(input_paths(&c.get_filtered_by_tags(&["dog"])), paths(&[1]));
}

#[test]
fn get_filtered_by_tags_added_out_of_order_sorted() {
    let c = collection(&[3, 2, 1]);
    assert_eq!(
        input_paths(&c.get_filtered_by_tags(&["post", "cat"])),
        paths(&[3])
    );
    assert_eq!(
        input_paths(&c.get_filtered_by_tags(&["cat"])),
        paths(&[2, 3])
    );
    assert_eq!(input_paths(&c.get_filtered_by_tags(&["dog"])), paths(&[1]));
}

#[test]
fn get_filtered_by_glob() {
    let c = collection(&[1, 6, 7]);
    assert_eq!(
        input_paths(&c.get_filtered_by_glob("./**/*.md")),
        paths(&[1])
    );
}

#[test]
fn get_filtered_by_glob_no_dash_dot() {
    let c = collection(&[1, 6, 7]);
    assert_eq!(input_paths(&c.get_filtered_by_glob("**/*.md")), paths(&[1]));
    assert_eq!(
        input_paths(&c.get_filtered_by_glob("**/*.{html,njk}")),
        paths(&[6, 7])
    );
}

#[test]
fn get_filtered_by_glob_cache_is_invalidated_by_add() {
    let ctx = get_config();
    let mut c = TemplateCollection::new();
    add_template(&mut c, &by_number(1, &ctx));
    add_template(&mut c, &by_number(6, &ctx));

    let markdowns = c.get_filtered_by_glob("./**/*.md");
    assert_eq!(markdowns.len(), 1);

    add_template(&mut c, &by_number(8, &ctx));
    assert_eq!(c.get_filtered_by_glob("./**/*.md").len(), 2);

    // The result returned before the addition is unaffected
    assert_eq!(markdowns.len(), 1);
}

#[test]
fn partial_match_on_tag_string_issue_95() {
    let ctx = get_config();
    let mut c = TemplateCollection::new();
    add_template(
        &mut c,
        &template_with_context("./test/stubs/issue-95/cat.md", &ctx),
    );
    add_template(
        &mut c,
        &template_with_context("./test/stubs/issue-95/notacat.md", &ctx),
    );
    assert_eq!(c.get_filtered_by_tag("cat").len(), 1);
}

#[test]
fn micromatch_assumptions_issue_127() {
    let is_match = |path: &str, globs: &[&str]| {
        is_glob_match(
            path,
            &globs.iter().map(|g| g.to_string()).collect::<Vec<_>>(),
        )
    };
    assert!(is_match(
        "src/bookmarks/test.md",
        &["**/+(bookmarks|posts|screencasts)/**/!(index)*.md"]
    ));
    assert!(is_match(
        "./src/bookmarks/test.md",
        &["./**/+(bookmarks|posts|screencasts)/**/!(index)*.md"]
    ));

    let globs =
        TemplateCollection::get_globs(&["**/+(bookmarks|posts|screencasts)/**/!(index)*.md"]);
    assert_eq!(
        globs,
        vec!["./**/+(bookmarks|posts|screencasts)/**/!(index)*.md".to_string()]
    );

    assert!(is_glob_match("./src/bookmarks/test.md", &globs));
    assert!(!is_glob_match("./src/bookmarks/index.md", &globs));
    assert!(!is_glob_match("./src/bookmarks/index2.md", &globs));
    assert!(is_glob_match(
        "./src/_content/bookmarks/2018-03-27-git-message.md",
        &globs
    ));
}

#[test]
fn sort_in_place_issue_352() {
    let c = collection(&[1, 4, 5]);
    assert_eq!(input_paths(&c.get_all_sorted()), paths(&[4, 1, 5]));

    let mut posts2 = c.get_all_sorted();
    posts2.as_array_mut().unwrap().reverse();
    assert_eq!(input_paths(&posts2), paths(&[5, 1, 4]));

    let mut posts3 = c.get_all_sorted();
    posts3.as_array_mut().unwrap().reverse();
    assert_eq!(input_paths(&posts3), paths(&[5, 1, 4]));
}

#[test]
fn get_filtered_by_tag_with_excludes() {
    let c = collection(&[8, 9, 10]);
    assert_eq!(c.get_filtered_by_tag("post").len(), 0);

    // `offices.sort(Sortable.sortFunctionDate)`: the dates are equal, so the order is kept
    let offices = c.get_filtered_by_tag("office");
    assert_eq!(input_paths(&offices), paths(&[10, 9]));
}
