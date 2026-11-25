//! Port of upstream `test/GetParentDirectoryTest.js`.

use twelvety::Value;
use twelvety::filters::get_parent_directory;

fn parent(input: Value) -> Value {
    get_parent_directory(&input)
}

fn p(s: &str) -> Value {
    parent(Value::str(s))
}

fn some(s: &str) -> Value {
    Value::str(s)
}

/// "Directory paths - returns parent directory"
#[test]
fn directory_paths_returns_parent_directory() {
    assert_eq!(p("/mydir/"), some("/"));
    assert_eq!(p("/a/b/"), some("/a/"));
    assert_eq!(p("/a/b/c/"), some("/a/b/"));
    assert_eq!(p("/a/b/c/d/"), some("/a/b/c/"));
}

/// "File paths - returns containing directory"
#[test]
fn file_paths_returns_containing_directory() {
    assert_eq!(p("/mydir/slug.html"), some("/mydir/"));
    assert_eq!(p("/mydir/index.md"), some("/mydir/"));
    assert_eq!(p("/a/b/file.html"), some("/a/b/"));
    assert_eq!(p("/a/b/c/page.md"), some("/a/b/c/"));
}

/// "Root level paths"
#[test]
fn root_level_paths() {
    assert_eq!(p("/"), some("/"));
    assert_eq!(p("/file.html"), some("/"));
    assert_eq!(p("/index.md"), some("/"));
}

/// "Edge cases"
#[test]
fn edge_cases() {
    assert_eq!(p(""), Value::Undefined);
    assert_eq!(parent(Value::Null), Value::Undefined);
    assert_eq!(parent(Value::Undefined), Value::Undefined);
    assert_eq!(parent(Value::Number(123.0)), Value::Undefined);
    assert_eq!(parent(Value::object()), Value::Undefined);
    assert_eq!(p("file.html"), Value::Undefined);
    assert_eq!(p("dir/file.html"), some("dir/"));
    assert_eq!(p("a/b/c/"), some("a/b/"));
}

/// "Single segment paths"
#[test]
fn single_segment_paths() {
    assert_eq!(p("/foo/"), some("/"));
    assert_eq!(p("/foo"), some("/"));
}

/// "Relative paths without a parent"
#[test]
fn relative_paths_without_a_parent() {
    assert_eq!(p("dir/"), Value::Undefined);
    assert_eq!(p("./"), Value::Undefined);
    assert_eq!(p("../"), Value::Undefined);
}

/// "Full URLs keep their origin"
#[test]
fn full_urls_keep_their_origin() {
    assert_eq!(p("https://example.com"), some("https://example.com/"));
    assert_eq!(p("https://example.com/"), some("https://example.com/"));
    assert_eq!(p("https://example.com/a/"), some("https://example.com/"));
    assert_eq!(
        p("https://example.com/a/b.html"),
        some("https://example.com/a/")
    );
    assert_eq!(
        p("https://example.com/a/b/?x=1#y"),
        some("https://example.com/a/")
    );
}

/// "page.inputPath values"
#[test]
fn page_input_path_values() {
    assert_eq!(p("./src/blog/post.md"), some("./src/blog/"));
    assert_eq!(p("./index.md"), some("./"));
    assert_eq!(p("../content/a.md"), some("../content/"));
}

/// "Paths are returned as an exact, unencoded prefix"
#[test]
fn paths_are_returned_as_an_exact_unencoded_prefix() {
    assert_eq!(p("/my dir/é/page.html"), some("/my dir/é/"));
    assert_eq!(p("/100%-off/a.html"), some("/100%-off/"));
    assert_eq!(p("/a%20b/c.html"), some("/a%20b/"));
    assert_eq!(p("/what?/"), some("/"));
    assert_eq!(p("./src/c#/intro.md"), some("./src/c#/"));
}

// "String objects": JS `new String()` wrappers have no equivalent in `Value` (skipped).
