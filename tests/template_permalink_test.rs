//! Port of upstream `test/TemplatePermalinkTest.js`.

use std::sync::{Arc, Mutex};

use twelvety::Value;
use twelvety::template_permalink::TemplatePermalink;
use twelvety::testing::json;
use twelvety::user_config::UrlTransformFn;

fn new(link: &str) -> TemplatePermalink {
    TemplatePermalink::new(&Value::str(link), "").unwrap()
}

fn new_sub(link: &str, subdir: &str) -> TemplatePermalink {
    TemplatePermalink::new(&Value::str(link), subdir).unwrap()
}

/// `generate(dir, filenameNoExt, extraSubdir, fileExtension = "html")`
fn generate(dir: &str, name: &str, subdir: Option<&str>, ext: Option<&str>) -> TemplatePermalink {
    TemplatePermalink::generate(dir, name, subdir.unwrap_or(""), ext.unwrap_or("html"), false).unwrap()
}

fn out(p: &TemplatePermalink) -> String {
    p.to_output_path().unwrap()
}

fn href(p: &TemplatePermalink) -> String {
    p.to_href().unwrap()
}

fn transform<F>(f: F) -> UrlTransformFn
where
    F: Fn(&str, &str) -> Option<String> + Send + Sync + 'static,
{
    Arc::new(f)
}

#[test]
fn simple_straight_permalink() {
    assert_eq!(out(&new("permalinksubfolder/test.html")), "permalinksubfolder/test.html");
    assert_eq!(out(&new("./permalinksubfolder/test.html")), "permalinksubfolder/test.html");
    assert_eq!(href(&new("permalinksubfolder/test.html")), "/permalinksubfolder/test.html");
    assert_eq!(href(&new("./permalinksubfolder/test.html")), "/permalinksubfolder/test.html");
    assert_eq!(href(&new("./testindex.html")), "/testindex.html");
    assert_eq!(href(&new("./permalinksubfolder/testindex.html")), "/permalinksubfolder/testindex.html");
}

#[test]
fn permalink_without_filename() {
    assert_eq!(out(&new("permalinksubfolder/")), "permalinksubfolder/index.html");
    assert_eq!(out(&new("./permalinksubfolder/")), "permalinksubfolder/index.html");
    assert_eq!(out(&new("/permalinksubfolder/")), "/permalinksubfolder/index.html");
    assert_eq!(href(&new("permalinksubfolder/")), "/permalinksubfolder/");
    assert_eq!(href(&new("./permalinksubfolder/")), "/permalinksubfolder/");
    assert_eq!(href(&new("/permalinksubfolder/")), "/permalinksubfolder/");
}

#[test]
fn permalink_with_pagination_subdir() {
    assert_eq!(out(&new_sub("permalinksubfolder/test.html", "0/")), "permalinksubfolder/0/test.html");
    assert_eq!(out(&new_sub("permalinksubfolder/test.html", "1/")), "permalinksubfolder/1/test.html");
    assert_eq!(href(&new_sub("permalinksubfolder/test.html", "0/")), "/permalinksubfolder/0/test.html");
    assert_eq!(href(&new_sub("permalinksubfolder/test.html", "1/")), "/permalinksubfolder/1/test.html");
}

#[test]
fn permalink_generate() {
    assert_eq!(out(&generate("./", "index", None, None)), "index.html");
    assert_eq!(href(&generate("./", "index", None, None)), "/");
    assert_eq!(out(&generate(".", "index", None, None)), "index.html");
    assert_eq!(href(&generate(".", "index", None, None)), "/");
    assert_eq!(out(&generate(".", "test", None, None)), "test/index.html");
    assert_eq!(href(&generate(".", "test", None, None)), "/test/");
    assert_eq!(out(&generate(".", "test", Some("0/"), None)), "test/0/index.html");
    assert_eq!(href(&generate(".", "test", Some("0/"), None)), "/test/0/");
    assert_eq!(out(&generate(".", "test", Some("1/"), None)), "test/1/index.html");
    assert_eq!(href(&generate(".", "test", Some("1/"), None)), "/test/1/");
}

#[test]
fn permalink_generate_with_suffix() {
    assert_eq!(out(&generate(".", "test", None, None)), "test/index.html");
    assert_eq!(href(&generate(".", "test", None, None)), "/test/");
    assert_eq!(out(&generate(".", "test", Some("1/"), None)), "test/1/index.html");
    assert_eq!(href(&generate(".", "test", Some("1/"), None)), "/test/1/");
}

#[test]
fn permalink_generate_with_new_extension() {
    assert_eq!(out(&generate(".", "test", None, Some("css"))), "test.css");
    assert_eq!(href(&generate(".", "test", None, Some("css"))), "/test.css");
    assert_eq!(out(&generate(".", "test", Some("1/"), Some("css"))), "1/test.css");
    assert_eq!(href(&generate(".", "test", Some("1/"), Some("css"))), "/1/test.css");
}

#[test]
fn permalink_generate_with_subfolders() {
    assert_eq!(out(&generate("permalinksubfolder/", "index", None, None)), "permalinksubfolder/index.html");
    assert_eq!(out(&generate("permalinksubfolder/", "test", None, None)), "permalinksubfolder/test/index.html");
    assert_eq!(
        out(&generate("permalinksubfolder/", "test", Some("1/"), None)),
        "permalinksubfolder/test/1/index.html"
    );
    assert_eq!(href(&generate("permalinksubfolder/", "index", None, None)), "/permalinksubfolder/");
    assert_eq!(href(&generate("permalinksubfolder/", "test", None, None)), "/permalinksubfolder/test/");
    assert_eq!(href(&generate("permalinksubfolder/", "test", Some("1/"), None)), "/permalinksubfolder/test/1/");
}

#[test]
fn permalink_matching_folder_and_filename() {
    let has_dupe = TemplatePermalink::has_duplicate_folder;
    assert!(!has_dupe("subfolder", "component"));
    assert!(!has_dupe("subfolder/", "component"));
    assert!(!has_dupe(".", "component"));
    assert!(has_dupe("component", "component"));
    assert!(has_dupe("component/", "component"));

    assert_eq!(out(&generate("component/", "component", None, None)), "component/component/index.html");
    assert_eq!(href(&generate("component/", "component", None, None)), "/component/component/");

    let legacy = TemplatePermalink::generate("component/", "component", "", "html", true).unwrap();
    assert_eq!(out(&legacy), "component/index.html");
    assert_eq!(href(&legacy), "/component/");
}

#[test]
fn permalink_object_build_alias() {
    let p = TemplatePermalink::new(&json(r#"{"build": "permalinksubfolder/test.html"}"#), "").unwrap();
    assert_eq!(out(&p), "permalinksubfolder/test.html");
    let p = TemplatePermalink::new(&json(r#"{"build": false}"#), "").unwrap();
    assert_eq!(p.to_output_path(), None);
    assert!(TemplatePermalink::new(&json(r#"{"build": true}"#), "").is_err());
}

#[test]
fn permalink_object_without_build_throws() {
    assert!(TemplatePermalink::new(&json("{}"), "").is_err());
    assert!(TemplatePermalink::new(&json(r#"{"someotherkey": "/"}"#), "").is_err());
}

#[test]
fn permalink_generate_apache_content_negotiation_761() {
    let mut tp = new("index.es.html");
    tp.set_url_transforms(&[transform(|_, _| Some("/".into()))]);
    assert_eq!(href(&tp), "/");
    assert_eq!(out(&tp), "index.es.html");

    // `generate` does some preprocessing to the raw permalink value (compared to `new`)
    let mut tp1 = generate("", "index.es", None, None);
    tp1.set_url_transforms(&[transform(|_, _| Some("/".into()))]);
    assert_eq!(href(&tp1), "/");
    assert_eq!(out(&tp1), "index.es/index.html");
}

#[test]
fn permalink_generate_apache_content_negotiation_with_subdirectory_761() {
    let mut tp = new("test/index.es.html");
    tp.set_url_transforms(&[transform(|_, _| Some("/test/".into()))]);
    assert_eq!(href(&tp), "/test/");
    assert_eq!(out(&tp), "test/index.es.html");

    let mut tp1 = generate("test", "index.es", None, None);
    tp1.set_url_transforms(&[transform(|_, _| Some("/test/".into()))]);
    assert_eq!(href(&tp1), "/test/");
    assert_eq!(out(&tp1), "test/index.es/index.html");
}

#[test]
fn permalink_generate_apache_content_negotiation_non_index_file_name_761() {
    let tp = generate("permalinksubfolder", "about.es", None, None);
    assert_eq!(href(&tp), "/permalinksubfolder/about.es/");
    assert_eq!(out(&tp), "permalinksubfolder/about.es/index.html");
}

#[test]
fn permalink_generate_with_url_transforms_761() {
    let mut tp = generate("permalinksubfolder", "index.es", None, None);
    tp.set_url_transforms(&[transform(|_, _| Some("/permalinksubfolder/".into()))]);
    assert_eq!(href(&tp), "/permalinksubfolder/");
    assert_eq!(out(&tp), "permalinksubfolder/index.es/index.html");
}

#[test]
fn permalink_generate_with_url_transforms_skip_via_undefined_761() {
    let mut tp = generate("permalinksubfolder", "index.es", None, None);
    tp.set_url_transforms(&[transform(|_, _| None)]);
    assert_eq!(href(&tp), "/permalinksubfolder/index.es/");
    assert_eq!(out(&tp), "permalinksubfolder/index.es/index.html");
}

#[test]
fn permalink_generate_with_2_url_transforms_761() {
    let mut tp = generate("permalinksubfolder", "index.es", None, None);
    tp.set_url_transforms(&[
        transform(|_, _| Some("/abc/".into())),
        transform(|_, _| Some("/def/".into())),
    ]);
    assert_eq!(href(&tp), "/def/");
    assert_eq!(out(&tp), "permalinksubfolder/index.es/index.html");
}

#[test]
fn permalink_generate_with_url_transforms_returns_index_html_761() {
    let mut tp = generate("permalinksubfolder", "index.es", None, None);
    tp.set_url_transforms(&[transform(|_, _| Some("/abc/index.html".into()))]);
    assert_eq!(href(&tp), "/abc/");
    assert_eq!(out(&tp), "permalinksubfolder/index.es/index.html");
}

fn language_suffix() -> regex::Regex {
    regex::Regex::new(r"(?i)\.[a-z]{2}\.html$").unwrap()
}

/// Strips a `.xx.html` language suffix, with or without a trailing slash, checking the
/// `url`/`urlStem` arguments the transform receives
fn strip_language(expected: Option<(&'static str, &'static str)>, trailing_slash: bool) -> (UrlTransformFn, Arc<Mutex<usize>>) {
    let calls = Arc::new(Mutex::new(0));
    let c = calls.clone();
    let re = language_suffix();
    let f = transform(move |url, url_stem| {
        if let Some((expected_url, expected_stem)) = expected {
            assert_eq!(url, expected_url);
            assert_eq!(url_stem, expected_stem);
        }
        *c.lock().unwrap() += 1;
        if re.is_match(url) {
            let stripped = &url[..url.len() - ".en.html".len()];
            return Some(if trailing_slash { format!("{stripped}/") } else { stripped.to_string() });
        }
        None
    });
    (f, calls)
}

fn transformed_href(link: &str, expected: Option<(&'static str, &'static str)>, trailing_slash: bool) -> String {
    let mut tp = new(link);
    let (f, calls) = strip_language(expected, trailing_slash);
    tp.set_url_transforms(&[f]);
    let h = href(&tp);
    assert_eq!(*calls.lock().unwrap(), 1);
    h
}

#[test]
fn permalink_generate_with_url_transforms_code_index_file_761() {
    let expected = Some(("/index.es.html", "/index.es"));
    assert_eq!(transformed_href("index.es.html", expected, true), "/");
    assert_eq!(transformed_href("index.es.html", expected, false), "/");
}

#[test]
fn permalink_generate_with_url_transforms_code_not_index_file_761() {
    assert_eq!(transformed_href("about.es.html", Some(("/about.es.html", "/about.es")), true), "/about/");
    assert_eq!(transformed_href("about.es.html", None, false), "/about");
}

#[test]
fn permalink_generate_with_url_transforms_code_index_file_with_subdir_761() {
    let expected = Some(("/subdir/index.es.html", "/subdir/index.es"));
    assert_eq!(transformed_href("subdir/index.es.html", expected, true), "/subdir/");
    assert_eq!(transformed_href("subdir/index.es.html", expected, false), "/subdir/");
}

#[test]
fn permalink_generate_with_url_transforms_code_not_index_file_with_subdir_761() {
    let expected = Some(("/subdir/about.es.html", "/subdir/about.es"));
    assert_eq!(transformed_href("subdir/about.es.html", expected, true), "/subdir/about/");
    assert_eq!(transformed_href("subdir/about.es.html", expected, false), "/subdir/about");
}
