//! Port of upstream `test/UrlTest.js`.

use twelvety::Value;
use twelvety::filters::url_filter as url;
use twelvety::template_config::TemplateConfig;
use twelvety::user_config::CallContext;

fn init_config() -> TemplateConfig {
    let mut tc = TemplateConfig::new(Some(""));
    tc.init().unwrap();
    tc
}

/// "Test url filter passing in pathPrefix from config"
#[test]
fn test_url_filter_passing_in_pathprefix_from_config() {
    let tc = init_config();
    let pp = tc.get_config().path_prefix.clone();
    assert_eq!(pp, "/");
    assert_eq!(url("test", &pp), "test");
    assert_eq!(url("/test", &pp), "/test");
}

/// "Test url filter without passing in pathPrefix"
#[test]
fn test_url_filter_without_passing_in_pathprefix() {
    let tc = init_config();
    let filter = tc.user_config.get_filter("url").expect("url filter");
    let lookup = |_: &str| Value::Undefined;
    let ctx = CallContext::new("liquid", &lookup);
    assert_eq!(
        filter(&ctx, Value::str("test"), &[]).unwrap(),
        Value::str("test")
    );
    assert_eq!(
        filter(&ctx, Value::str("/test"), &[]).unwrap(),
        Value::str("/test")
    );
}

/// "Test url filter with passthrough urls"
#[test]
fn test_url_filter_with_passthrough_urls() {
    assert_eq!(
        url("http://foo.com/blah_blah", ""),
        "http://foo.com/blah_blah"
    );
    assert_eq!(
        url("http://foo.com/blah_blah/", ""),
        "http://foo.com/blah_blah/"
    );
    assert_eq!(
        url("http://foo.com/blah_blah_(wikipedia)", ""),
        "http://foo.com/blah_blah_(wikipedia)"
    );
    assert_eq!(
        url("http://foo.com/blah_blah_(wikipedia)_(again)", ""),
        "http://foo.com/blah_blah_(wikipedia)_(again)"
    );
    assert_eq!(
        url("http://www.example.com/wpstyle/?p=364", ""),
        "http://www.example.com/wpstyle/?p=364"
    );
    assert_eq!(
        url("https://www.example.com/foo/?bar=baz&inga=42&quux", ""),
        "https://www.example.com/foo/?bar=baz&inga=42&quux"
    );
    assert_eq!(
        url("http://userid:password@example.com:8080", ""),
        "http://userid:password@example.com:8080"
    );
    assert_eq!(
        url("http://userid:password@example.com:8080/", ""),
        "http://userid:password@example.com:8080/"
    );
    assert_eq!(
        url("http://userid@example.com", ""),
        "http://userid@example.com"
    );
    assert_eq!(
        url("http://userid@example.com/", ""),
        "http://userid@example.com/"
    );
    assert_eq!(
        url("http://userid@example.com:8080", ""),
        "http://userid@example.com:8080"
    );
    assert_eq!(
        url("http://userid@example.com:8080/", ""),
        "http://userid@example.com:8080/"
    );
    assert_eq!(
        url("http://userid:password@example.com", ""),
        "http://userid:password@example.com"
    );
    assert_eq!(
        url("http://userid:password@example.com/", ""),
        "http://userid:password@example.com/"
    );
    assert_eq!(url("http://142.42.1.1/", ""), "http://142.42.1.1/");
    assert_eq!(
        url("http://142.42.1.1:8080/", ""),
        "http://142.42.1.1:8080/"
    );
    assert_eq!(
        url("http://foo.com/blah_(wikipedia)#cite-1", ""),
        "http://foo.com/blah_(wikipedia)#cite-1"
    );
    assert_eq!(
        url("http://foo.com/blah_(wikipedia)_blah#cite-1", ""),
        "http://foo.com/blah_(wikipedia)_blah#cite-1"
    );
    assert_eq!(
        url("http://foo.com/(something)?after=parens", ""),
        "http://foo.com/(something)?after=parens"
    );
    assert_eq!(
        url("http://code.google.com/events/#&product=browser", ""),
        "http://code.google.com/events/#&product=browser"
    );
    assert_eq!(url("http://j.mp", ""), "http://j.mp");
    assert_eq!(url("ftp://foo.bar/baz", ""), "ftp://foo.bar/baz");
    assert_eq!(
        url("http://foo.bar/?q=Test%20URL-encoded%20stuff", ""),
        "http://foo.bar/?q=Test%20URL-encoded%20stuff"
    );
    assert_eq!(
        url("http://-.~_!$&'()*+,;=:%40:80%2f::::::@example.com", ""),
        "http://-.~_!$&'()*+,;=:%40:80%2f::::::@example.com"
    );
    assert_eq!(url("http://1337.net", ""), "http://1337.net");
    assert_eq!(url("http://a.b-c.de", ""), "http://a.b-c.de");
    assert_eq!(url("http://223.255.255.254", ""), "http://223.255.255.254");
    assert_eq!(url("http://✪df.ws/123", ""), "http://✪df.ws/123");
    assert_eq!(url("http://➡.ws/䨹", ""), "http://➡.ws/䨹");
    assert_eq!(url("http://⌘.ws", ""), "http://⌘.ws");
    assert_eq!(url("http://⌘.ws/", ""), "http://⌘.ws/");
    assert_eq!(
        url("http://foo.com/unicode_(✪)_in_parens", ""),
        "http://foo.com/unicode_(✪)_in_parens"
    );
    assert_eq!(url("http://☺.damowmow.com/", ""), "http://☺.damowmow.com/");
    assert_eq!(url("http://مثال.إختبار", ""), "http://مثال.إختبار");
    assert_eq!(url("http://例子.测试", ""), "http://例子.测试");
    assert_eq!(url("http://उदाहरण.परीक्षा", ""), "http://उदाहरण.परीक्षा");
}

/// "Test url filter"
#[test]
fn test_url_filter() {
    assert_eq!(url("/", "/"), "/");
    assert_eq!(url("//", "/"), "/");
    assert_eq!(url("", "/"), ".");
    assert_eq!(url("", "/"), ".");
    assert_eq!(url(".", "/"), ".");
    assert_eq!(url("./", "/"), "./");
    assert_eq!(url("..", "/"), "..");
    assert_eq!(url("../", "/"), "../");
    assert_eq!(url("test", "/"), "test");
    assert_eq!(url("/test", "/"), "/test");
    assert_eq!(url("//test", "/"), "//test");
    assert_eq!(url("./test", "/"), "test");
    assert_eq!(url("../test", "/"), "../test");
    assert_eq!(url("test/", "/"), "test/");
    assert_eq!(url("/test/", "/"), "/test/");
    assert_eq!(url("//test/", "/"), "//test/");
    assert_eq!(url("./test/", "/"), "test/");
    assert_eq!(url("../test/", "/"), "../test/");
}

/// "Test url filter with custom pathPrefix (empty, gets overwritten by root config `/`)"
#[test]
fn test_url_filter_with_custom_pathprefix_empty_gets_overwritten_by_root_config() {
    assert_eq!(url("/", ""), "/");
    assert_eq!(url("//", ""), "/");
    assert_eq!(url("", ""), ".");
    assert_eq!(url("", ""), ".");
    assert_eq!(url(".", ""), ".");
    assert_eq!(url("./", ""), "./");
    assert_eq!(url("..", ""), "..");
    assert_eq!(url("../", ""), "../");
    assert_eq!(url("test", ""), "test");
    assert_eq!(url("/test", ""), "/test");
    assert_eq!(url("//test", ""), "//test");
    assert_eq!(url("./test", ""), "test");
    assert_eq!(url("../test", ""), "../test");
    assert_eq!(url("test/", ""), "test/");
    assert_eq!(url("/test/", ""), "/test/");
    assert_eq!(url("//test/", ""), "//test/");
    assert_eq!(url("./test/", ""), "test/");
    assert_eq!(url("../test/", ""), "../test/");
}

/// "Test url filter with custom pathPrefix (leading slash)"
#[test]
fn test_url_filter_with_custom_pathprefix_leading_slash() {
    assert_eq!(url("/", "/testdir"), "/testdir/");
    assert_eq!(url("//", "/testdir"), "/testdir/");
    assert_eq!(url("", "/testdir"), ".");
    assert_eq!(url("", "/testdir"), ".");
    assert_eq!(url(".", "/testdir"), ".");
    assert_eq!(url("./", "/testdir"), "./");
    assert_eq!(url("..", "/testdir"), "..");
    assert_eq!(url("../", "/testdir"), "../");
    assert_eq!(url("test", "/testdir"), "test");
    assert_eq!(url("/test", "/testdir"), "/testdir/test");
    assert_eq!(url("//test", "/testdir"), "//test");
    assert_eq!(url("./test", "/testdir"), "test");
    assert_eq!(url("../test", "/testdir"), "../test");
    assert_eq!(url("test/", "/testdir"), "test/");
    assert_eq!(url("/test/", "/testdir"), "/testdir/test/");
    assert_eq!(url("//test/", "/testdir"), "//test/");
    assert_eq!(url("./test/", "/testdir"), "test/");
    assert_eq!(url("../test/", "/testdir"), "../test/");
}

/// "Test url filter with custom pathPrefix (double slash)"
#[test]
fn test_url_filter_with_custom_pathprefix_double_slash() {
    assert_eq!(url("/", "/testdir/"), "/testdir/");
    assert_eq!(url("//", "/testdir/"), "/testdir/");
    assert_eq!(url("", "/testdir/"), ".");
    assert_eq!(url("", "/testdir/"), ".");
    assert_eq!(url(".", "/testdir/"), ".");
    assert_eq!(url("./", "/testdir/"), "./");
    assert_eq!(url("..", "/testdir/"), "..");
    assert_eq!(url("../", "/testdir/"), "../");
    assert_eq!(url("test", "/testdir/"), "test");
    assert_eq!(url("/test", "/testdir/"), "/testdir/test");
    assert_eq!(url("//test", "/testdir/"), "//test");
    assert_eq!(url("./test", "/testdir/"), "test");
    assert_eq!(url("../test", "/testdir/"), "../test");
    assert_eq!(url("test/", "/testdir/"), "test/");
    assert_eq!(url("/test/", "/testdir/"), "/testdir/test/");
    assert_eq!(url("//test/", "/testdir/"), "//test/");
    assert_eq!(url("./test/", "/testdir/"), "test/");
    assert_eq!(url("../test/", "/testdir/"), "../test/");
}

/// "Test url filter with custom pathPrefix (trailing slash)"
#[test]
fn test_url_filter_with_custom_pathprefix_trailing_slash() {
    assert_eq!(url("/", "testdir/"), "/testdir/");
    assert_eq!(url("//", "testdir/"), "/testdir/");
    assert_eq!(url("", "testdir/"), ".");
    assert_eq!(url("", "testdir/"), ".");
    assert_eq!(url(".", "testdir/"), ".");
    assert_eq!(url("./", "testdir/"), "./");
    assert_eq!(url("..", "testdir/"), "..");
    assert_eq!(url("../", "testdir/"), "../");
    assert_eq!(url("test", "testdir/"), "test");
    assert_eq!(url("/test", "testdir/"), "/testdir/test");
    assert_eq!(url("//test", "testdir/"), "//test");
    assert_eq!(url("./test", "testdir/"), "test");
    assert_eq!(url("../test", "testdir/"), "../test");
    assert_eq!(url("test/", "testdir/"), "test/");
    assert_eq!(url("/test/", "testdir/"), "/testdir/test/");
    assert_eq!(url("//test/", "testdir/"), "//test/");
    assert_eq!(url("./test/", "testdir/"), "test/");
    assert_eq!(url("../test/", "testdir/"), "../test/");
}

/// "Test url filter with custom pathPrefix (no slash)"
#[test]
fn test_url_filter_with_custom_pathprefix_no_slash() {
    assert_eq!(url("/", "testdir"), "/testdir/");
    assert_eq!(url("//", "testdir"), "/testdir/");
    assert_eq!(url("", "testdir"), ".");
    assert_eq!(url("", "testdir"), ".");
    assert_eq!(url(".", "testdir"), ".");
    assert_eq!(url("./", "testdir"), "./");
    assert_eq!(url("..", "testdir"), "..");
    assert_eq!(url("../", "testdir"), "../");
    assert_eq!(url("test", "testdir"), "test");
    assert_eq!(url("/test", "testdir"), "/testdir/test");
    assert_eq!(url("//test", "testdir"), "//test");
    assert_eq!(url("//foo.com", "testdir"), "//foo.com");
    assert_eq!(url("./test", "testdir"), "test");
    assert_eq!(url("../test", "testdir"), "../test");
    assert_eq!(url("test/", "testdir"), "test/");
    assert_eq!(url("/test/", "testdir"), "/testdir/test/");
    assert_eq!(url("//test/", "testdir"), "//test/");
    assert_eq!(url("./test/", "testdir"), "test/");
    assert_eq!(url("../test/", "testdir"), "../test/");
}
