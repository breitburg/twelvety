//! Port of upstream `test/TemplateFileSlugTest.js`.

use twelvety::template_file_slug::TemplateFileSlug;
use twelvety::testing::{TestOptions, get_context};

/// `getNewSlugInstance(path, inputDir)`: returns `(getSlug(), getFullPathWithoutExtension())`
fn slug(path: &str, input_dir: Option<&str>) -> (String, String) {
    let options = match input_dir {
        Some(dir) => TestOptions::input(dir),
        None => TestOptions::default(),
    };
    let ctx = get_context(options).unwrap();
    let fs = TemplateFileSlug::new(path, &ctx.extension_map, &ctx.dirs.input());
    (fs.get_slug(), fs.get_full_path_without_extension())
}

#[test]
fn easy_slug() {
    let (slug, stem) = slug("./file.html", None);
    assert_eq!(slug, "file");
    assert_eq!(stem, "/file");
}

#[test]
fn easy_slug_with_dot() {
    let (slug, stem) = slug("./file.test.html", None);
    assert_eq!(slug, "file.test");
    assert_eq!(stem, "/file.test");
}

#[test]
fn easy_slug_with_dot_12ty_js() {
    let (slug, stem) = slug("./file.test.12ty.js", None);
    assert_eq!(slug, "file.test");
    assert_eq!(stem, "/file.test");
}

#[test]
fn easy_slug_with_date() {
    let (slug, stem) = slug("./2018-01-01-file.html", None);
    assert_eq!(slug, "file");
    assert_eq!(stem, "/file");
}

#[test]
fn easy_slug_with_date_and_dot_in_slug() {
    let (slug, stem) = slug("./2018-01-01-file.test.html", None);
    assert_eq!(slug, "file.test");
    assert_eq!(stem, "/file.test");
}

#[test]
fn easy_slug_index() {
    let (slug, stem) = slug("./index.html", None);
    assert_eq!(slug, "");
    assert_eq!(stem, "/index");
}

#[test]
fn easy_slug_with_date_index() {
    let (slug, stem) = slug("./2018-01-01-index.html", None);
    assert_eq!(slug, "");
    assert_eq!(stem, "/index");
}

#[test]
fn easy_slug_with_only_a_date_and_no_suffix() {
    let (slug, stem) = slug("./2018-01-01.html", None);
    assert_eq!(slug, "2018-01-01");
    assert_eq!(stem, "/2018-01-01");
}

#[test]
fn easy_slug_with_dir() {
    let (slug, stem) = slug("./test/file.html", None);
    assert_eq!(slug, "file");
    assert_eq!(stem, "/test/file");
}

#[test]
fn easy_slug_with_dot_with_dir() {
    let (slug, stem) = slug("./test/file.test.html", None);
    assert_eq!(slug, "file.test");
    assert_eq!(stem, "/test/file.test");
}

#[test]
fn easy_slug_with_date_with_dir() {
    let (slug, stem) = slug("./test/2018-01-01-file.html", None);
    assert_eq!(slug, "file");
    assert_eq!(stem, "/test/file");
}

#[test]
fn easy_slug_with_date_and_dot_in_slug_with_dir() {
    let (slug, stem) = slug("./test/2018-01-01-file.test.html", None);
    assert_eq!(slug, "file.test");
    assert_eq!(stem, "/test/file.test");
}

#[test]
fn easy_slug_index_with_dir() {
    let (slug, stem) = slug("./test/index.html", None);
    assert_eq!(slug, "test");
    assert_eq!(stem, "/test/index");
}

#[test]
fn easy_slug_with_date_index_with_dir() {
    let (slug, stem) = slug("./test/2018-01-01-index.html", None);
    assert_eq!(slug, "test");
    assert_eq!(stem, "/test/index");
}

#[test]
fn strips_date_from_dir_name() {
    let (slug, stem) = slug("./2021-11-20-my-awesome-post/index.md", None);
    assert_eq!(slug, "my-awesome-post");
    assert_eq!(stem, "/2021-11-20-my-awesome-post/index");
}

#[test]
fn easy_slug_input_dir() {
    let (slug, stem) = slug("./file.html", Some("."));
    assert_eq!(slug, "file");
    assert_eq!(stem, "/file");
}

#[test]
fn easy_slug_with_dot_input_dir() {
    let (slug, stem) = slug("./file.test.html", Some("."));
    assert_eq!(slug, "file.test");
    assert_eq!(stem, "/file.test");
}

#[test]
fn easy_slug_with_date_input_dir() {
    let (slug, stem) = slug("./2018-01-01-file.html", Some("."));
    assert_eq!(slug, "file");
    assert_eq!(stem, "/file");
}

#[test]
fn easy_slug_with_date_and_dot_in_slug_input_dir() {
    let (slug, stem) = slug("./2018-01-01-file.test.html", Some("."));
    assert_eq!(slug, "file.test");
    assert_eq!(stem, "/file.test");
}

#[test]
fn easy_slug_index_input_dir() {
    let (slug, stem) = slug("./index.html", Some("."));
    assert_eq!(slug, "");
    assert_eq!(stem, "/index");
}

#[test]
fn easy_slug_with_date_index_input_dir() {
    let (slug, stem) = slug("./2018-01-01-index.html", Some("."));
    assert_eq!(slug, "");
    assert_eq!(stem, "/index");
}

#[test]
fn easy_slug_with_dir_and_input_dir() {
    let (slug, stem) = slug("./test/file.html", Some("./test"));
    assert_eq!(slug, "file");
    assert_eq!(stem, "/file");
}

#[test]
fn easy_slug_with_dot_with_dir_and_input_dir() {
    let (slug, stem) = slug("./test/file.test.html", Some("./test"));
    assert_eq!(slug, "file.test");
    assert_eq!(stem, "/file.test");
}

#[test]
fn easy_slug_with_date_with_dir_and_input_dir() {
    let (slug, stem) = slug("./test/2018-01-01-file.html", Some("./test"));
    assert_eq!(slug, "file");
    assert_eq!(stem, "/file");
}

#[test]
fn easy_slug_with_date_and_dot_in_slug_with_dir_and_input_dir() {
    let (slug, stem) = slug("./test/2018-01-01-file.test.html", Some("./test"));
    assert_eq!(slug, "file.test");
    assert_eq!(stem, "/file.test");
}

#[test]
fn easy_slug_index_with_dir_and_input_dir() {
    let (slug, stem) = slug("./test/index.html", Some("./test"));
    assert_eq!(slug, "");
    assert_eq!(stem, "/index");
}

#[test]
fn easy_slug_with_date_index_with_dir_and_input_dir() {
    let (slug, stem) = slug("./test/2018-01-01-index.html", Some("./test"));
    assert_eq!(slug, "");
    assert_eq!(stem, "/index");
}

#[test]
fn easy_slug_with_multiple_dirs() {
    let (slug, stem) = slug("./dir1/dir2/dir3/file.html", Some("."));
    assert_eq!(slug, "file");
    assert_eq!(stem, "/dir1/dir2/dir3/file");
}

#[test]
fn easy_slug_with_multiple_dirs_and_an_index_file() {
    let (slug, stem) = slug("./dir1/dir2/dir3/index.html", Some("."));
    assert_eq!(slug, "dir3");
    assert_eq!(stem, "/dir1/dir2/dir3/index");
}
