//! Port of upstream `test/ReservedDataTest.js`.
//!
//! - The configuration API tests point `configPath` at a JavaScript config file (unsupported); they
//!   run without a configuration file, so the error has no `(source: …)` suffix.
//! - The global data file tests use JavaScript data files; they run against scratch copies of the
//!   fixtures with equivalent JSON data files.
//! - Skipped: "Twelvety freeze data written in a global data function throws error #4341"
//!   (JavaScript data function mutating the frozen `twelvety` global).

mod template_support;

use template_support::fixture_copy;
use twelvety::testing::json;
use twelvety::util::reserved_data::{FULL_PROPERTIES, get_reserved_keys};
use twelvety::{Twelvety, Options, UserConfig, Value};

fn reserved(data: &str) -> Vec<String> {
    let mut keys = get_reserved_keys(&json(data), FULL_PROPERTIES);
    keys.sort();
    keys
}

#[test]
fn no_reserved_keys() {
    assert!(reserved(r#"{"key": {}}"#).is_empty());
}

#[test]
fn standard_keys_are_reserved() {
    assert_eq!(reserved(r#"{"content": ""}"#), ["content"]);
    assert_eq!(reserved(r#"{"collections": {}}"#), ["collections"]);
    assert_eq!(reserved(r#"{"content": "", "collections": {}}"#), ["collections", "content"]);
}

#[test]
fn page_subkeys() {
    assert!(reserved(r#"{"page": {}}"#).is_empty());
    assert_eq!(reserved(r#"{"page": ""}"#), ["page"]);
    for key in [
        "date",
        "inputPath",
        "fileSlug",
        "filePathStem",
        "outputFileExtension",
        "templateSyntax",
        "url",
        "outputPath",
    ] {
        assert_eq!(
            reserved(&format!(r#"{{"page": {{"{key}": "", "otherkey": ""}}}}"#)),
            [format!("page.{key}")]
        );
    }
    assert_eq!(
        reserved(r#"{"page": {"date": "", "outputPath": "", "otherkey": ""}}"#),
        ["page.date", "page.outputPath"]
    );
}

const MESSAGE_SUFFIX: &str = ". You can opt-out of this behavior with `$config.setFreezeReservedData(false)` or rename/remove the property in your data cascade that conflicts with reserved property names (e.g. `buildawesome`, `twelvety`, `pkg`, and others). Learn more: https://v3.11ty.dev/docs/data-eleventy-supplied/";

fn to_json_error<F>(input: &str, config: F) -> String
where
    F: Fn(&mut UserConfig) -> anyhow::Result<()> + Send + Sync + 'static,
{
    let options = Options {
        config_path: Some(String::new()),
        ..Default::default()
    }
    .with_config(config);
    let mut elev = Twelvety::new(Some(input), None, options);
    elev.disable_logger();
    let err = elev.to_json().err().expect("expected a reserved data error");
    format!("{err:#}")
}

fn assert_reserved_error(err: &str, names: &str, source: Option<&str>) {
    let source = source.map(|s| format!(" (source: {s})")).unwrap_or_default();
    let expected = format!("You attempted to set one of Build Awesome’s reserved data property names: {names}{source}{MESSAGE_SUFFIX}");
    assert!(err.contains(&expected), "{err}");
}

#[test]
fn freeze_data_set_via_config_api_throws_error_page() {
    let err = to_json_error("./test/stubs-virtual/", |cfg| {
        cfg.add_global_data("page", Value::str("lol no"));
        cfg.add_template("index.html", "", Value::object())
    });
    assert_reserved_error(&err, "page", None);
}

#[test]
fn freeze_data_set_via_config_api_throws_error_twelvety() {
    let err = to_json_error("./test/stubs-virtual/", |cfg| {
        cfg.add_global_data("twelvety", Value::str("lol no"));
        cfg.add_template("index.html", "", Value::object())
    });
    assert_reserved_error(&err, "twelvety", None);
}

#[test]
fn freeze_data_set_global_data_file_throws_error_page() {
    let dir = fixture_copy(
        "./test/stubs-freeze/page",
        "reserved-data-page",
        &[("_data/page.json", r#"{"key": "value", "url": "lksjdklfjlskdjf"}"#)],
    );
    let err = to_json_error(&format!("{dir}/"), |cfg| cfg.add_template("index.html", "", Value::object()));
    assert_reserved_error(&err, "page.url", Some(&format!("{dir}/_data/page.json")));
}

#[test]
fn freeze_data_set_global_data_file_throws_error_twelvety() {
    let dir = fixture_copy(
        "./test/stubs-freeze/twelvety",
        "reserved-data-twelvety",
        &[("_data/twelvety.json", r#"{"key": "value"}"#)],
    );
    let err = to_json_error(&format!("{dir}/"), |cfg| cfg.add_template("index.html", "", Value::object()));
    assert_reserved_error(&err, "twelvety", Some(&format!("{dir}/_data/twelvety.json")));
}

#[test]
fn freeze_data_override_prefix_on_a_reserved_global_key_throws_error() {
    let err = to_json_error("./test/stubs-virtual/", |cfg| {
        cfg.add_template("index.html", "", json(r#"{"override:pkg": {"name": "nope"}}"#))
    });
    assert!(err.contains("reserved data property names: pkg"), "{err}");
}
