//! Port of upstream `test/HtmlRelativeCopyTest.js`.

mod common;

use std::path::Path;
use std::sync::{Arc, Mutex};

use indexmap::IndexMap;
use twelvety::plugins::{html_base, input_path_to_url};
use twelvety::template::PageResult;
use twelvety::template_passthrough::CopyResult;
use twelvety::testing::json;
use twelvety::user_config::UserConfig;
use twelvety::{Eleventy, Options, Value};

/// Removes an output directory when the test ends
struct Cleanup(&'static str);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.0);
    }
}

type Captured = Arc<Mutex<Option<Value>>>;

/// Builds `./test/stubs-autocopy/` into `output` (ignoring sibling output directories) and captures
/// the `buildawesome.passthrough` event
fn build(
    output: &'static str,
    path_prefix: Option<&str>,
    config: impl Fn(&mut UserConfig) -> anyhow::Result<()> + Send + Sync + 'static,
) -> (anyhow::Result<(Vec<CopyResult>, Vec<PageResult>)>, Captured) {
    let captured: Captured = Arc::new(Mutex::new(None));
    let c = captured.clone();
    let mut options: Options = common::options(move |cfg: &mut UserConfig| {
        cfg.ignores.insert("./test/stubs-autocopy/_site*/**".into());
        let c = c.clone();
        cfg.on("buildawesome.passthrough", move |arg: &Value| {
            *c.lock().unwrap() = Some(arg.clone());
            Ok(())
        });
        config(cfg)
    });
    options.path_prefix = path_prefix.map(String::from);
    let mut elev = Eleventy::new(Some("./test/stubs-autocopy/"), Some(output), options);
    elev.disable_logger();
    (elev.write(), captured)
}

fn html_relative(glob: &str) -> (String, Value) {
    (glob.to_string(), json(r#"{"mode": "html-relative"}"#))
}

fn add_html_relative(cfg: &mut UserConfig, glob: &str) {
    let (glob, options) = html_relative(glob);
    cfg.add_passthrough_copy_with_options(&glob, options);
}

fn event(map: &[(&str, &str)]) -> Value {
    let map: serde_json::Map<String, serde_json::Value> =
        map.iter().map(|(k, v)| (k.to_string(), serde_json::Value::String(v.to_string()))).collect();
    json(&serde_json::json!({ "entries": [], "incremental": false, "map": map }).to_string())
}

fn copy_map(pairs: &[(&str, &str)]) -> IndexMap<String, String> {
    pairs.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
}

fn page(input_path: &str, output_path: &str, url: &str, content: &str, raw_input: &str) -> (String, Value, Value, String, String) {
    (
        input_path.to_string(),
        Value::str(output_path),
        Value::str(url),
        content.to_string(),
        raw_input.to_string(),
    )
}

fn describe(p: &PageResult) -> (String, Value, Value, String, String) {
    (p.input_path.clone(), p.output_path.clone(), p.url.clone(), p.content.clone(), p.raw_input.clone())
}

fn exists(path: &str) -> bool {
    Path::new(path).exists()
}

/// "Basic usage"
#[test]
fn basic_usage() {
    let _cleanup = Cleanup("./test/stubs-autocopy/_site-basica");
    let (result, captured) = build("./test/stubs-autocopy/_site-basica", None, |cfg| {
        add_html_relative(cfg, "**/*.png");
        cfg.add_template("test.njk", r#"<img src="possum.png">"#, Value::object())
    });
    let (copy, templates) = result.unwrap();
    assert_eq!(
        captured.lock().unwrap().clone().unwrap(),
        event(&[("/test/possum.png", "test/stubs-autocopy/possum.png")])
    );
    assert_eq!(copy.len(), 1);
    assert_eq!(templates.len(), 1);
    assert_eq!(
        describe(&templates[0]),
        page(
            "./test/stubs-autocopy/test.njk",
            "./test/stubs-autocopy/_site-basica/test/index.html",
            "/test/",
            r#"<img src="possum.png">"#,
            r#"<img src="possum.png">"#
        )
    );
    assert_eq!(copy[0].count, 1);
    assert_eq!(
        copy[0].map,
        copy_map(&[("test/stubs-autocopy/possum.png", "test/stubs-autocopy/_site-basica/test/possum.png")])
    );
    assert!(exists("test/stubs-autocopy/_site-basica/test/possum.png"));
    assert!(exists("test/stubs-autocopy/_site-basica/test/index.html"));
}

// "Accepts `file:` URL strings and `URL` instances": JavaScript `import.meta.resolve`/`URL` inputs (skipped).

/// "More complex image path (parent dir)"
#[test]
fn more_complex_image_path_parent_dir() {
    let _cleanup = Cleanup("./test/stubs-autocopy/_site-basicb");
    let (result, captured) = build("./test/stubs-autocopy/_site-basicb", None, |cfg| {
        add_html_relative(cfg, "**/*.png");
        cfg.add_template("test.njk", r#"<img src="../stubs-img-transform/possum.png">"#, Value::object())
    });
    let (copy, templates) = result.unwrap();
    assert_eq!(
        captured.lock().unwrap().clone().unwrap(),
        event(&[("/stubs-img-transform/possum.png", "test/stubs-img-transform/possum.png")])
    );
    assert_eq!(copy.len(), 1);
    assert_eq!(templates.len(), 1);
    assert_eq!(
        describe(&templates[0]),
        page(
            "./test/stubs-autocopy/test.njk",
            "./test/stubs-autocopy/_site-basicb/test/index.html",
            "/test/",
            r#"<img src="../stubs-img-transform/possum.png">"#,
            r#"<img src="../stubs-img-transform/possum.png">"#
        )
    );
    assert_eq!(copy[0].count, 1);
    assert_eq!(
        copy[0].map,
        copy_map(&[(
            "test/stubs-img-transform/possum.png",
            "test/stubs-autocopy/_site-basicb/stubs-img-transform/possum.png"
        )])
    );
    assert!(exists("test/stubs-autocopy/_site-basicb/stubs-img-transform/possum.png"));
    assert!(exists("test/stubs-autocopy/_site-basicb/test/index.html"));
}

/// "No matches"
#[test]
fn no_matches() {
    let _cleanup = Cleanup("./test/stubs-autocopy/_site2");
    let (result, captured) = build("./test/stubs-autocopy/_site2", None, |cfg| {
        add_html_relative(cfg, "**/*.jpeg");
        cfg.add_template("test.njk", r#"<img src="lol.lol">"#, Value::object())
    });
    let (copy, templates) = result.unwrap();
    assert_eq!(captured.lock().unwrap().clone().unwrap(), event(&[]));
    assert_eq!(copy.len(), 0);
    assert_eq!(templates.len(), 1);
    assert!(!exists("test/stubs-autocopy/_site2/test/lol.lol"));
    assert!(exists("test/stubs-autocopy/_site2/test/index.html"));
}

/// "Match but does not exist (throws error)"
#[test]
fn match_but_does_not_exist_throws_error() {
    let _cleanup = Cleanup("./test/stubs-autocopy/_site3");
    let (result, _) = build("./test/stubs-autocopy/_site3", None, |cfg| {
        add_html_relative(cfg, "**/*.png");
        cfg.add_template("test.njk", r#"<img src="missing.png">"#, Value::object())
    });
    assert_eq!(
        result.unwrap_err().to_string(),
        r#"Having trouble writing to "./test/stubs-autocopy/_site3/test/index.html" from "./test/stubs-autocopy/test.njk""#
    );
    assert!(!exists("test/stubs-autocopy/_site3/test/index.html"));
}

/// "Match but does not exist (no error, using `failOnError: false`)"
#[test]
fn match_but_does_not_exist_fail_on_error_false() {
    let _cleanup = Cleanup("./test/stubs-autocopy/_site4");
    let (result, captured) = build("./test/stubs-autocopy/_site4", None, |cfg| {
        cfg.add_passthrough_copy_with_options("**/*.png", json(r#"{"mode": "html-relative", "failOnError": false}"#));
        cfg.add_template("test.njk", r#"<img src="missing.png">"#, Value::object())
    });
    let (copy, templates) = result.unwrap();
    assert_eq!(captured.lock().unwrap().clone().unwrap(), event(&[]));
    assert_eq!(copy.len(), 0);
    assert_eq!(templates.len(), 1);
    assert!(!exists("test/stubs-autocopy/_site4/test/missing.png"));
    assert!(exists("test/stubs-autocopy/_site4/test/index.html"));
}

/// "Copying dotfiles are not allowed"
#[test]
fn copying_dotfiles_are_not_allowed() {
    let _cleanup = Cleanup("./test/stubs-autocopy/_site5");
    let (result, captured) = build("./test/stubs-autocopy/_site5", None, |cfg| {
        // WARNING: don’t do this
        cfg.add_passthrough_copy_with_options("**/*", json(r#"{"mode": "html-relative", "copyOptions": {}}"#));
        cfg.add_template("test.njk", r#"<img src=".gitkeep">"#, Value::object())
    });
    let (copy, templates) = result.unwrap();
    assert_eq!(captured.lock().unwrap().clone().unwrap(), event(&[]));
    assert_eq!(copy.len(), 1);
    assert_eq!(copy[0].count, 0);
    assert_eq!(templates.len(), 1);
    assert!(!exists("test/stubs-autocopy/_site5/.gitkeep"));
    assert!(!exists("test/stubs-autocopy/_site5/test/.gitkeep"));
    assert!(exists("test/stubs-autocopy/_site5/test/index.html"));
}

fn input_path_to_url_build(output: &'static str, path_prefix: Option<&str>, plugin_first: bool, base: bool, expected: &str) {
    let _cleanup = Cleanup(output);
    let (result, captured) = build(output, path_prefix, move |cfg| {
        // order of addPlugin shouldn’t matter here
        if plugin_first {
            cfg.add_plugin("InputPathToUrlTransformPlugin", input_path_to_url::transform_plugin, Value::object())?;
        }
        add_html_relative(cfg, "**/*.{html,njk}");
        if !plugin_first {
            cfg.add_plugin("InputPathToUrlTransformPlugin", input_path_to_url::transform_plugin, Value::object())?;
        }
        if base {
            cfg.add_plugin("HtmlBasePlugin", html_base::plugin, Value::object())?;
        }
        cfg.add_template("test1.njk", "Test 1", Value::object())?;
        cfg.add_template("test2.njk", r#"<a href="test1.njk">Test 2</a>"#, Value::object())
    });
    let (copy, templates) = result.unwrap();
    assert_eq!(captured.lock().unwrap().clone().unwrap(), event(&[]));
    assert_eq!(copy.len(), 0);
    assert_eq!(templates.len(), 2);
    let test2 = templates.iter().find(|t| t.url.to_output_string().ends_with("/test2/")).unwrap();
    assert_eq!(test2.content, expected);
    assert!(!exists(&format!("{output}/test2/test1.njk")));
    assert!(exists(&format!("{output}/test2/index.html")));
}

/// "Using with InputPathToUrl plugin"
#[test]
fn using_with_input_path_to_url_plugin() {
    input_path_to_url_build("./test/stubs-autocopy/_site6", None, false, false, r#"<a href="/test1/">Test 2</a>"#);
}

/// "Using with InputPathToUrl plugin (reverse addPlugin order)"
#[test]
fn using_with_input_path_to_url_plugin_reverse_add_plugin_order() {
    input_path_to_url_build("./test/stubs-autocopy/_site7", None, true, false, r#"<a href="/test1/">Test 2</a>"#);
}

/// "Use with HtmlBasePlugin usage"
#[test]
fn use_with_html_base_plugin_usage() {
    let _cleanup = Cleanup("./test/stubs-autocopy/_site8a");
    let (result, captured) = build("./test/stubs-autocopy/_site8a", Some("yolo"), |cfg| {
        cfg.add_plugin("HtmlBasePlugin", html_base::plugin, Value::object())?;
        add_html_relative(cfg, "**/*.png");
        cfg.add_template("test.njk", r#"<img src="possum.png"><img src="/test/possum.png">"#, Value::object())
    });
    let (copy, templates) = result.unwrap();
    assert_eq!(
        captured.lock().unwrap().clone().unwrap(),
        event(&[("/test/possum.png", "test/stubs-autocopy/possum.png")])
    );
    assert_eq!(copy.len(), 1);
    assert_eq!(templates.len(), 1);
    assert_eq!(
        describe(&templates[0]),
        page(
            "./test/stubs-autocopy/test.njk",
            "./test/stubs-autocopy/_site8a/test/index.html",
            "/test/",
            r#"<img src="possum.png"><img src="/yolo/test/possum.png">"#,
            r#"<img src="possum.png"><img src="/test/possum.png">"#
        )
    );
    assert_eq!(copy[0].count, 1);
    assert_eq!(
        copy[0].map,
        copy_map(&[("test/stubs-autocopy/possum.png", "test/stubs-autocopy/_site8a/test/possum.png")])
    );
    assert!(exists("test/stubs-autocopy/_site8a/test/possum.png"));
    assert!(exists("test/stubs-autocopy/_site8a/test/index.html"));
}

/// "Using with InputPathToUrl plugin and HtmlBasePlugin"
#[test]
fn using_with_input_path_to_url_plugin_and_html_base_plugin() {
    input_path_to_url_build("./test/stubs-autocopy/_site8b", Some("yolo"), false, true, r#"<a href="/yolo/test1/">Test 2</a>"#);
}

fn two_globs(output: &'static str) {
    let _cleanup = Cleanup(output);
    let (result, captured) = build(output, None, |cfg| {
        add_html_relative(cfg, "**/*.jpg");
        add_html_relative(cfg, "**/*.png");
        cfg.add_template("test.njk", r#"<img src="possum.png"><img src="possum.jpg">"#, Value::object())
    });
    let (copy, templates) = result.unwrap();
    assert_eq!(
        captured.lock().unwrap().clone().unwrap(),
        event(&[
            ("/test/possum.jpg", "test/stubs-autocopy/possum.jpg"),
            ("/test/possum.png", "test/stubs-autocopy/possum.png"),
        ])
    );
    assert_eq!(copy.len(), 2);
    assert_eq!(templates.len(), 1);
    let site = output.trim_start_matches("./");
    assert_eq!(
        describe(&templates[0]),
        page(
            "./test/stubs-autocopy/test.njk",
            &format!("{output}/test/index.html"),
            "/test/",
            r#"<img src="possum.png"><img src="possum.jpg">"#,
            r#"<img src="possum.png"><img src="possum.jpg">"#
        )
    );
    assert_eq!(copy[0].map, copy_map(&[("test/stubs-autocopy/possum.png", &format!("{site}/test/possum.png"))]));
    assert_eq!(copy[1].map, copy_map(&[("test/stubs-autocopy/possum.jpg", &format!("{site}/test/possum.jpg"))]));
    assert!(exists(&format!("{site}/test/possum.jpg")));
    assert!(exists(&format!("{site}/test/possum.png")));
    assert!(exists(&format!("{site}/test/index.html")));
}

/// "Multiple addPlugin calls (use both globs)"
#[test]
fn multiple_add_plugin_calls_use_both_globs() {
    two_globs("./test/stubs-autocopy/_site9");
}

/// "Array of globs" (the port adds each glob of the array separately)
#[test]
fn array_of_globs() {
    two_globs("./test/stubs-autocopy/_site10");
}

/// "overwrite: false"
#[test]
fn overwrite_false() {
    let _cleanup = Cleanup("./test/stubs-autocopy/_site11");
    std::fs::create_dir_all("./test/stubs-autocopy/_site11/test/").unwrap();
    std::fs::copy("./test/stubs-autocopy/possum.png", "./test/stubs-autocopy/_site11/test/possum.png").unwrap();
    let (result, captured) = build("./test/stubs-autocopy/_site11", None, |cfg| {
        cfg.add_passthrough_copy_with_options(
            "**/*.png",
            json(r#"{"mode": "html-relative", "copyOptions": {"overwrite": false}}"#),
        );
        cfg.add_template("test.njk", r#"<img src="possum.png">"#, Value::object())
    });
    let (copy, templates) = result.unwrap();
    assert_eq!(captured.lock().unwrap().clone().unwrap(), event(&[]));
    assert_eq!(copy.len(), 1);
    assert_eq!(templates.len(), 1);
    assert_eq!(copy[0].count, 0);
    assert!(copy[0].map.is_empty());
    assert!(exists("test/stubs-autocopy/_site11/test/possum.png"));
    assert!(exists("test/stubs-autocopy/_site11/test/index.html"));
}

/// "Input -> output remapping not yet supported (throws error)"
#[test]
fn input_output_remapping_not_yet_supported_throws_error() {
    let _cleanup = Cleanup("./test/stubs-autocopy/_site12");
    let (result, _) = build("./test/stubs-autocopy/_site12", None, |cfg| {
        cfg.add_passthrough_copy_mapping_with_options("**/*.png", "yo", json(r#"{"mode": "html-relative"}"#))?;
        cfg.add_template("test.njk", r#"<img src="missing.png">"#, Value::object())
    });
    assert_eq!(
        result.unwrap_err().root_cause().to_string(),
        "mode: 'html-relative' does not yet support passthrough copy objects (input -> output mapping). Use a string glob or an Array of string globs."
    );
    assert!(!exists("test/stubs-autocopy/_site12/test/index.html"));
}

/// "Invalid copy mode throws error"
#[test]
fn invalid_copy_mode_throws_error() {
    let _cleanup = Cleanup("./test/stubs-autocopy/_site13");
    let (result, _) = build("./test/stubs-autocopy/_site13", None, |cfg| {
        cfg.add_passthrough_copy_mapping_with_options("**/*.png", "yo", json(r#"{"mode": "throw-an-error"}"#))?;
        Ok(())
    });
    assert_eq!(
        result.unwrap_err().root_cause().to_string(),
        "Invalid `mode` option for `addPassthroughCopy`. Received: 'throw-an-error'"
    );
    assert!(!exists("test/stubs-autocopy/_site13/test/index.html"));
}
