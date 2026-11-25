//! Port of upstream `test/TemplateDataTest.js`.
//!
//! JavaScript (and TypeScript) data files are not supported by the port: expected data file path
//! lists only contain the non-JS entries, and the JS-only fixtures are skipped:
//! - "Get local data async JS", "getAllGlobalData() with js object data file", "getAllGlobalData()
//!   with js function data file", "getAllGlobalData() with common js function data file",
//!   "ESM data file": JS data files.
//! - "Add local data": the JS-file override assertions are dropped (JSON values are checked).
//! - "Create", "getRawImports": read the project’s `package.json`, which the crate root doesn’t have
//!   (the `pkg` key is still checked to exist).
//! - "TemplateData.merge is now Merge()": covered by the `Merge` port (`util::merge`) here too.

mod template_support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use template_support::fixture_copy;
use twelvety::data::template_data::{TemplateData, cleanup_data, get_included_collection_names, get_included_tag_names};
use twelvety::testing::{TestOptions, get_context, json};
use twelvety::util::merge::merge;
use twelvety::{UserConfig, Value};

fn with_data<F, R>(options: TestOptions, f: F) -> R
where
    F: FnOnce(&TemplateData) -> R,
{
    let ctx = get_context(options).unwrap();
    f(&ctx.template_data)
}

fn input(dir: &str) -> TestOptions {
    TestOptions::input(dir)
}

fn input_with<F>(dir: &str, config: F) -> TestOptions
where
    F: Fn(&mut UserConfig) -> anyhow::Result<()> + Send + Sync + 'static,
{
    TestOptions::input(dir).with_config(config)
}

/// `testGetLocalData`: global data shallow-merged with the template’s local data files
fn local_data(td: &TemplateData, template_path: &str) -> Value {
    let paths = td.get_local_data_paths(template_path);
    let sources = td.get_local_data_sources(&paths).unwrap();
    let refs: Vec<&Value> = sources.iter().collect();
    let imported = merge(Value::object(), &refs);
    let mut local = td.get_global_data().unwrap();
    if let Value::Object(m) = &imported {
        for (k, v) in m.iter() {
            local.set(k.clone(), v.clone());
        }
    }
    local
}

/// Local data paths without the JavaScript/TypeScript entries
fn local_data_paths(td: &TemplateData, template_path: &str) -> Vec<String> {
    td.get_local_data_paths(template_path)
        .into_iter()
        .filter(|p| {
            !["js", "cjs", "mjs", "ts", "cts", "mts"]
                .iter()
                .any(|ext| p.ends_with(&format!(".{ext}")))
        })
        .collect()
}

fn default_paths(dirs: &[(&str, &str)]) -> Vec<String> {
    let mut out = vec![];
    for (dir, name) in dirs {
        out.push(format!("{dir}/{name}.json"));
        out.push(format!("{dir}/{name}.12tydata.json"));
        out.push(format!("{dir}/{name}.data.json"));
    }
    out
}

const STUBS_COMPONENT: &[(&str, &str)] = &[("./test/stubs", "stubs"), ("./test/stubs/component", "component")];

#[test]
fn create() {
    with_data(input("test/stubs"), |td| {
        assert!(td.get_global_data().unwrap().has("pkg"));
    });
}

#[test]
fn get_global_data() {
    with_data(input("test/stubs"), |td| {
        let data = td.get_global_data().unwrap();
        assert_eq!(data.get_path("globalData.datakey1").as_str(), Some("datavalue1"));
        // variables are not resolved (default processing)
        assert_eq!(data.get_path("globalData.datakey2").as_str(), Some("{{pkg.name}}"));
        assert!(data.has("pkg"));
    });
}

#[test]
fn data_dir_does_not_exist() {
    let err = get_context(input("test/thisdirectorydoesnotexist"))
        .and_then(|ctx| ctx.template_data.get_global_data())
        .err()
        .unwrap();
    assert_eq!(
        err.to_string(),
        "The \"test/thisdirectorydoesnotexist\" `input` parameter (directory or file path) must exist on the file system (unless detected as a glob by the `tinyglobby` package)"
    );
}

#[test]
fn add_local_data() {
    with_data(input("test/stubs"), |td| {
        let data = td.get_global_data().unwrap();
        assert_eq!(data.get_path("globalData.datakey1").as_str(), Some("datavalue1"));
        assert_eq!(data.get_path("globalData.datakey2").as_str(), Some("{{pkg.name}}"));

        let local = local_data(td, "./test/stubs/component/component.njk");
        assert_eq!(local.get_path("globalData.datakey1").as_str(), Some("datavalue1"));
        assert_eq!(local.get_path("globalData.datakey2").as_str(), Some("{{pkg.name}}"));
        assert_eq!(local.get("localdatakey1").as_str(), Some("localdatavalue1"));
        assert_eq!(local.get("localdatakeyfromjs2").as_str(), Some("howdy2"));
    });
}

#[test]
fn add_local_data_does_not_exist_but_does_not_fail() {
    for template in [
        "./test/stubs/datafiledoesnotexist/template.njk",
        "./test/stubs/datafiledoesnotexist/templatedoesnotexist.njk",
    ] {
        with_data(input("test/stubs"), |td| {
            let before = td.get_global_data().unwrap().keys();
            let local = local_data(td, template);
            assert_eq!(local.get_path("globalData.datakey1").as_str(), Some("datavalue1"));
            assert_eq!(local.get_path("globalData.datakey2").as_str(), Some("{{pkg.name}}"));
            assert_eq!(local.keys(), before);
        });
    }
}

#[test]
fn global_dir_directory() {
    with_data(input("."), |td| {
        assert_eq!(td.get_global_data_glob(), ["./_data/**/*.{json,mjs,cjs,js}"]);
    });
}

#[test]
fn global_dir_directory_with_constructor_path_arg() {
    with_data(input("test/stubs"), |td| {
        assert_eq!(td.get_global_data_glob(), ["./test/stubs/_data/**/*.{json,mjs,cjs,js}"]);
    });
}

#[test]
fn get_all_global_data_with_other_data_files() {
    with_data(input("test/stubs"), |td| {
        let data = td.get_global_data().unwrap();
        let files = td.get_global_data_files().unwrap();
        assert!(!files.is_empty());
        assert!(
            files
                .iter()
                .any(|p| twelvety::util::template_path::add_leading_dot_slash(p)
                    .starts_with("./test/stubs/_data/globalData.json"))
        );
        assert_eq!(data.get_path("globalData.datakey1").as_str(), Some("datavalue1"));
        assert_eq!(data.get("testData"), json(r#"{"testdatakey1": "testdatavalue1"}"#));
        assert_eq!(data.get_path("subdir.testDataSubdir"), json(r#"{"subdirkey": "subdirvalue"}"#));
    });
}

#[test]
fn get_all_global_data_with_config_global_data() {
    let options = input_with("test/stubs", |cfg| {
        cfg.add_global_data("example", Value::function(|_| Ok(Value::str("one"))));
        cfg.add_global_data("example2", Value::function(|_| Ok(Value::str("two"))));
        cfg.add_global_data("example3", Value::str("static"));
        Ok(())
    });
    with_data(options, |td| {
        let data = td.get_global_data().unwrap();
        assert_eq!(data.get("example").as_str(), Some("one"));
        assert_eq!(data.get("example2").as_str(), Some("two"));
        assert_eq!(data.get("example3").as_str(), Some("static"));
    });
}

#[test]
fn get_data_value_without_template_engine_preprocessing() {
    with_data(input("test/stubs"), |td| {
        assert_eq!(
            td.get_data_value("./test/stubs/_data/testDataLiquid.json").unwrap(),
            json(r#"{"datakey1": "datavalue1", "datakey2": "{{ pkg.name }}"}"#)
        );
    });
}

#[test]
fn get_local_data_paths() {
    with_data(input("test/stubs"), |td| {
        assert_eq!(
            local_data_paths(td, "./test/stubs/component/component.liquid"),
            default_paths(STUBS_COMPONENT)
        );
    });
}

fn paths_with<F>(config: F) -> Vec<String>
where
    F: Fn(&mut UserConfig) -> anyhow::Result<()> + Send + Sync + 'static,
{
    with_data(input_with("test/stubs", config), |td| {
        local_data_paths(td, "./test/stubs/component/component.liquid")
    })
}

#[test]
fn get_local_data_paths_with_set_data_file_base_name_1699() {
    let paths = paths_with(|cfg| {
        cfg.set_data_file_base_name("index");
        Ok(())
    });
    assert_eq!(
        paths,
        [
            "./test/stubs/index.12tydata.json",
            "./test/stubs/index.data.json",
            "./test/stubs/component/index.12tydata.json",
            "./test/stubs/component/index.data.json",
            "./test/stubs/component/component.json",
            "./test/stubs/component/component.12tydata.json",
            "./test/stubs/component/component.data.json",
        ]
    );
}

#[test]
fn get_local_data_paths_with_empty_set_data_file_suffixes_1699() {
    let paths = paths_with(|cfg| {
        cfg.set_data_file_suffixes(&[]);
        Ok(())
    });
    assert!(paths.is_empty());
}

#[test]
fn get_local_data_paths_with_set_data_file_suffixes_override_1699() {
    let paths = paths_with(|cfg| {
        cfg.set_data_file_suffixes(&[".howdy"]);
        Ok(())
    });
    assert_eq!(
        paths,
        ["./test/stubs/stubs.howdy.json", "./test/stubs/component/component.howdy.json"]
    );
}

#[test]
fn get_local_data_paths_with_set_data_file_suffixes_empty_string_override_1699() {
    let paths = paths_with(|cfg| {
        cfg.set_data_file_suffixes(&[""]);
        Ok(())
    });
    assert_eq!(paths, ["./test/stubs/stubs.json", "./test/stubs/component/component.json"]);
}

#[test]
fn get_local_data_paths_with_set_data_file_suffixes_override_with_two_entries_1699() {
    let paths = paths_with(|cfg| {
        cfg.set_data_file_suffixes(&[".howdy", ""]);
        Ok(())
    });
    assert_eq!(
        paths,
        [
            "./test/stubs/stubs.json",
            "./test/stubs/stubs.howdy.json",
            "./test/stubs/component/component.json",
            "./test/stubs/component/component.howdy.json",
        ]
    );
}

#[test]
fn get_local_data_paths_with_set_data_file_suffixes_and_set_data_file_base_name_1699() {
    let paths = paths_with(|cfg| {
        cfg.set_data_file_base_name("index");
        cfg.set_data_file_suffixes(&[".howdy", ""]);
        Ok(())
    });
    assert_eq!(
        paths,
        [
            "./test/stubs/index.howdy.json",
            "./test/stubs/component/index.howdy.json",
            "./test/stubs/component/component.json",
            "./test/stubs/component/component.howdy.json",
        ]
    );
}

#[test]
fn deeper_get_local_data_paths() {
    with_data(TestOptions::default(), |td| {
        assert_eq!(
            local_data_paths(td, "./test/stubs/component/component.liquid"),
            default_paths(&[
                ("./test", "test"),
                ("./test/stubs", "stubs"),
                ("./test/stubs/component", "component"),
            ])
        );
    });
}

#[test]
fn get_local_data_paths_with_an_12ty_js_template() {
    with_data(input("test/stubs"), |td| {
        assert_eq!(
            local_data_paths(td, "./test/stubs/component/component.12ty.js"),
            default_paths(STUBS_COMPONENT)
        );
    });
}

#[test]
fn get_local_data_paths_with_input_dir_variants() {
    for dir in ["test/stubs", "test/stubs/", "./test/stubs"] {
        with_data(input(dir), |td| {
            assert_eq!(
                local_data_paths(td, "./test/stubs/component/component.liquid"),
                default_paths(STUBS_COMPONENT),
                "{dir}"
            );
        });
    }
}

#[test]
fn get_template_data_file_glob() {
    with_data(input("test/stubs"), |td| {
        assert_eq!(
            td.get_template_data_file_glob(),
            ["./test/stubs/**/*.{json,data.mjs,data.cjs,data.js,12tydata.mjs,12tydata.cjs,12tydata.js}"]
        );
    });
}

// https://github.com/11ty/eleventy/issues/3937
#[test]
fn template_data_merge_is_now_merge() {
    assert_eq!(
        merge(json(r#"{"tags": [1, 2, 3]}"#), &[&json(r#"{"tags": [4, 5, 6]}"#)]),
        json(r#"{"tags": [1, 2, 3, 4, 5, 6]}"#)
    );
}

#[test]
fn template_data_cleanup_data() {
    let cleanup = |s: &str| cleanup_data(json(s), Some("tags"), "").unwrap();
    assert_eq!(cleanup("{}"), json("{}"));
    assert_eq!(cleanup(r#"{"tags": null}"#), json(r#"{"tags": []}"#));
    assert_eq!(cleanup(r#"{"tags": ""}"#), json(r#"{"tags": []}"#));
    assert_eq!(cleanup(r#"{"tags": []}"#), json(r#"{"tags": []}"#));
    assert_eq!(cleanup(r#"{"tags": "test"}"#), json(r#"{"tags": ["test"]}"#));
    assert_eq!(cleanup(r#"{"tags": ["test1", "test2"]}"#), json(r#"{"tags": ["test1", "test2"]}"#));
}

#[test]
fn parent_directory_for_data_issue_337() {
    let options = input_with("./test/stubs-337/src/", |cfg| cfg.set_data_directory("../data/"));
    with_data(options, |td| {
        assert_eq!(td.get_global_data().unwrap().get("xyz"), json(r#"{"hi": "bye"}"#));
    });
}

#[test]
fn dots_in_datafile_path_issue_1242() {
    with_data(input("./test/stubs-1242/"), |td| {
        assert_eq!(
            td.get_global_data().unwrap().get("xyz.dottest"),
            json(r#"{"hi": "bye", "test": {"abc": 42}}"#)
        );
    });
}

#[test]
fn add_global_data_values() {
    let options = input_with("./test/stubs-global-data-config-api/", |cfg| {
        cfg.add_global_data("myFunction", Value::function(|_| Ok(Value::str("fn-value"))));
        cfg.add_global_data("myPromise", Value::function(|_| Ok(Value::str("promise-value"))));
        Ok(())
    });
    with_data(options, |td| {
        let data = td.get_global_data().unwrap();
        assert_eq!(data.get("myFunction").as_str(), Some("fn-value"));
        assert_eq!(data.get("myPromise").as_str(), Some("promise-value"));
    });
}

#[test]
fn add_global_data_should_execute_once() {
    let count = Arc::new(AtomicUsize::new(0));
    let c = count.clone();
    let options = input_with("./test/stubs-global-data-config-api/", move |cfg| {
        let c = c.clone();
        cfg.add_global_data(
            "count",
            Value::function(move |_| Ok(Value::from(c.fetch_add(1, Ordering::SeqCst) + 1))),
        );
        Ok(())
    });
    with_data(options, |td| {
        let data = td.get_global_data().unwrap();
        assert_eq!(data.get("count").as_f64(), Some(1.0));
        td.get_global_data().unwrap();
    });
    assert_eq!(count.load(Ordering::SeqCst), 1);
}

#[test]
fn add_global_data_complex_key() {
    // `_data/deep.cjs` as JSON
    let dir = fixture_copy(
        "./test/stubs-global-data-config-api-nested",
        "template-data-config-api-nested",
        &[("_data/deep.json", r#"{"existing": true}"#)],
    );
    let options = input_with(&dir, |cfg| {
        cfg.add_global_data("deep.nested.one", Value::function(|_| Ok(Value::str("first"))));
        cfg.add_global_data("deep.nested.two", Value::function(|_| Ok(Value::str("second"))));
        Ok(())
    });
    with_data(options, |td| {
        let data = td.get_global_data().unwrap();
        assert_eq!(data.get_path("deep.existing"), Value::Bool(true));
        assert_eq!(data.get_path("deep.nested.one").as_str(), Some("first"));
        assert_eq!(data.get_path("deep.nested.two").as_str(), Some("second"));
    });
}

#[test]
fn twelvety_version_and_generator_returned_from_data() {
    let options = input_with("./test/stubs-empty/", |cfg| {
        cfg.add_global_data("deep.nested.one", Value::function(|_| Ok(Value::str("first"))));
        cfg.add_global_data("deep.nested.two", Value::function(|_| Ok(Value::str("second"))));
        Ok(())
    });
    with_data(options, |td| {
        let data = td.get_global_data().unwrap();
        let version = twelvety::data::template_data::UPSTREAM_VERSION;
        assert_eq!(data.get_path("twelvety.version").as_str(), Some(version));
        assert_eq!(
            data.get_path("twelvety.generator").to_js_string(),
            format!("Twelvety (Build Awesome) v{version}")
        );
        assert_eq!(data.get_path("deep.nested.one").as_str(), Some("first"));
        assert_eq!(data.get_path("deep.nested.two").as_str(), Some("second"));
    });
}

#[test]
fn get_global_data_empty_json_file() {
    with_data(input("./test/stubs-empty-json-data/"), |td| {
        assert_eq!(td.get_global_data().unwrap().get("empty"), json("{}"));
    });
}

fn collection_names(data: &str) -> Vec<String> {
    get_included_collection_names(&json(data), Some("tags"))
}

fn tag_names(data: &str) -> Vec<String> {
    get_included_tag_names(&json(data), Some("tags"))
}

#[test]
fn collection_names_from_data_empty_assigned() {
    assert_eq!(collection_names(r#"{"tags": []}"#), ["all"]);
    assert!(collection_names(r#"{"tags": [], "twelvetyExcludeFromCollections": true}"#).is_empty());
    assert_eq!(collection_names(r#"{"tags": [], "twelvetyExcludeFromCollections": ["one"]}"#), ["all"]);
}

#[test]
fn collection_names_from_data_tags_assigned() {
    assert_eq!(collection_names(r#"{"tags": ["one", "two"]}"#), ["all", "one", "two"]);
    assert!(collection_names(r#"{"tags": ["one", "two"], "twelvetyExcludeFromCollections": true}"#).is_empty());
    assert_eq!(
        collection_names(r#"{"tags": ["one", "two"], "twelvetyExcludeFromCollections": ["one"]}"#),
        ["all", "two"]
    );
}

#[test]
fn tag_names_from_data_empty_assigned() {
    assert!(tag_names(r#"{"tags": []}"#).is_empty());
    assert!(tag_names(r#"{"tags": [], "twelvetyExcludeFromCollections": true}"#).is_empty());
    assert!(tag_names(r#"{"tags": [], "twelvetyExcludeFromCollections": ["one"]}"#).is_empty());
}

#[test]
fn tag_names_from_data_tags_assigned() {
    assert_eq!(tag_names(r#"{"tags": ["one", "two"]}"#), ["one", "two"]);
    assert!(tag_names(r#"{"tags": ["one", "two"], "twelvetyExcludeFromCollections": true}"#).is_empty());
    assert_eq!(
        tag_names(r#"{"tags": ["one", "two"], "twelvetyExcludeFromCollections": ["one"]}"#),
        ["two"]
    );
}

#[test]
fn tags_are_trimmed_and_deduplicated_after_trimming_2462() {
    assert_eq!(tag_names(r#"{"tags": "post, javascript ,post"}"#), ["post", "javascript"]);
    assert_eq!(tag_names(r#"{"tags": [" one", "one ", 1, "1"]}"#), ["one", "1"]);
}
