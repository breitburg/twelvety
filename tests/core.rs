//! Port of upstream `test/CoreTest.js` (tests that don’t need JavaScript configuration files,
//! JavaScript templates or JavaScript data files).

mod common;

use std::sync::{Arc, Mutex};

use twelvety::template::PageResult;
use twelvety::testing::json;
use twelvety::user_config::UserConfig;
use twelvety::{Options, Twelvety, Value};

use common::options;

fn new(input: Option<&str>, output: Option<&str>, options: Options) -> Twelvety {
    let elev = Twelvety::new(input, output, options);
    elev.disable_logger();
    elev
}

/// Programmatic options without configuration files
fn opts(config: impl Fn(&mut UserConfig) -> anyhow::Result<()> + Send + Sync + 'static) -> Options {
    options(config)
}

fn no_config() -> Options {
    options(|_| Ok(()))
}

fn page(url: &str, input_path: &str, output_path: &str, raw_input: &str, content: &str) -> Value {
    let mut m = twelvety::value::Map::new();
    m.insert("url".into(), Value::str(url));
    m.insert("inputPath".into(), Value::str(input_path));
    m.insert("outputPath".into(), Value::str(output_path));
    m.insert("rawInput".into(), Value::str(raw_input));
    m.insert("content".into(), Value::str(content));
    Value::from_map(m)
}

fn by_url<'a>(results: &'a [PageResult], url: &str) -> Vec<&'a PageResult> {
    results
        .iter()
        .filter(|r| r.url.as_str() == Some(url))
        .collect()
}

#[allow(dead_code)]
fn content_by_url(results: &[PageResult], url: &str) -> String {
    by_url(results, url)
        .first()
        .unwrap_or_else(|| panic!("no page for {url}"))
        .content
        .clone()
}

/// Removes a directory when the test ends
struct Cleanup(&'static str);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.0);
    }
}

/// "Twelvety, defaults inherit from config"
#[test]
fn defaults_inherit_from_config() {
    let mut elev = new(None, None, no_config());
    elev.initialize_config().unwrap();
    let dirs = elev.directories.clone().unwrap();
    assert_eq!(dirs.input(), "./");
    assert_eq!(dirs.output(), "./_site/");
}

/// "Twelvety, null output directory should default to _site"
#[test]
fn null_output_directory_should_default_to_site() {
    let mut elev = new(Some("."), None, no_config());
    elev.initialize_config().unwrap();
    let dirs = elev.directories.clone().unwrap();
    assert_eq!(dirs.input(), "./");
    assert_eq!(dirs.output(), "./_site/");
}

/// "Twelvety, get version"
#[test]
fn get_version() {
    assert!(!Twelvety::get_version().is_empty());
}

// "Twelvety, get help": the help text lives in the CLI binary (see tests/cmd.rs).

/// "Twelvety, set is verbose (before config init)"
#[test]
fn set_is_verbose_before_config_init() {
    let mut elev = Twelvety::new(
        None,
        None,
        Options {
            config_path: Some(String::new()),
            ..Default::default()
        },
    );
    elev.set_is_verbose(true);
    elev.initialize_config().unwrap();
    assert!(elev.is_verbose());
}

/// "Twelvety, set is verbose (after config init)"
#[test]
fn set_is_verbose_after_config_init() {
    let mut elev = Twelvety::new(
        None,
        None,
        Options {
            config_path: Some(String::new()),
            ..Default::default()
        },
    );
    elev.initialize_config().unwrap();
    elev.set_is_verbose(true);
    assert!(elev.is_verbose());
}

/// "Twelvety set input/output"
#[test]
fn set_input_output() {
    let mut elev = new(
        Some("./test/stubs"),
        Some("./test/stubs/_site"),
        no_config(),
    );
    let dirs = elev.create_directories().unwrap();
    assert_eq!(dirs.input(), "./test/stubs/");
    assert_eq!(dirs.output(), "./test/stubs/_site/");
    elev.initialize_config().unwrap();
}

/// "process.env.TWELVETY_*" and "process.env.BUILDAWESOME_*"
#[test]
fn process_env_twelvety_and_buildawesome() {
    let mut elev = new(
        Some("./test/stubs-virtual-nowrite"),
        Some("./test/stubs-virtual-nowrite/_site"),
        opts(|cfg| {
            cfg.add_template(
                "index.njk",
                "{{ twelvety.env.root }}|{{ buildawesome.env.root }}",
                Value::object(),
            )
        }),
    );
    let results = elev.to_json().unwrap();
    assert!(std::env::var("TWELVETY_ROOT").is_ok_and(|v| !v.is_empty()));
    assert!(std::env::var("BUILDAWESOME_ROOT").is_ok_and(|v| !v.is_empty()));
    let root = twelvety::util::template_path::get_working_dir();
    assert_eq!(results[0].content, format!("{root}|{root}"));
}

// "Twelvety file watching" (+ variants): watch targets include JavaScript dependency spidering
// and `package.json` (skipped).

/// "Twelvety set input/output, one file input"
#[test]
fn set_input_output_one_file_input() {
    let elev = new(
        Some("./test/stubs/index.html"),
        Some("./test/stubs/_site"),
        no_config(),
    );
    let dirs = elev.create_directories().unwrap();
    assert_eq!(dirs.input_file.as_deref(), Some("./test/stubs/index.html"));
    assert_eq!(dirs.input(), "./test/stubs/");
    assert_eq!(dirs.output(), "./test/stubs/_site/");
}

/// "Twelvety set input/output, one file input, deeper subdirectory"
#[test]
fn set_input_output_one_file_input_deeper_subdirectory() {
    let elev = new(
        Some("./test/stubs/subdir/index.html"),
        Some("./test/stubs/_site"),
        Options {
            input_dir: Some("./test/stubs".into()),
            ..no_config()
        },
    );
    let dirs = elev.create_directories().unwrap();
    assert_eq!(
        dirs.input_file.as_deref(),
        Some("./test/stubs/subdir/index.html")
    );
    assert_eq!(dirs.input(), "./test/stubs/");
    assert_eq!(dirs.output(), "./test/stubs/_site/");
}

/// "Twelvety set input/output, one file input root dir" (with `Cargo.toml`, upstream uses `README.md`)
#[test]
fn set_input_output_one_file_input_root_dir() {
    for input in ["./Cargo.toml", "Cargo.toml"] {
        let elev = new(Some(input), Some("./test/stubs/_site"), no_config());
        let dirs = elev.create_directories().unwrap();
        assert_eq!(dirs.input_file.as_deref(), Some("./Cargo.toml"));
        assert_eq!(dirs.input(), "./");
        assert_eq!(dirs.output(), "./test/stubs/_site/");
    }
}

/// "Twelvety set input/output, one file input exitCode (script)" and "… (cli)"
#[test]
fn set_input_output_one_file_input_exit_code() {
    for source in ["script", "cli"] {
        let mut elev = new(
            Some("./test/stubs/exitCode/failure.njk"),
            Some("./test/stubs/exitCode/_site"),
            Options {
                source: Some(source.into()),
                ..no_config()
            },
        );
        let err = elev.write().unwrap_err();
        assert_eq!(
            err.to_string(),
            "Having trouble rendering njk template ./test/stubs/exitCode/failure.njk"
        );
    }
}

// "setReporter config re-runs for each new user config": reporters are a CLI concern in the port
// (`--events-file`, see tests/cmd.rs).

/// "Throwing buildawesome.aftererror listener does not mask the original error"
#[test]
fn throwing_aftererror_listener_does_not_mask_the_original_error() {
    let mut elev = new(
        Some("./test/stubs/exitCode/failure.njk"),
        Some("./test/stubs/exitCode/_site"),
        opts(|cfg| {
            cfg.on("buildawesome.aftererror", |_| {
                anyhow::bail!("Listener failure")
            });
            Ok(())
        }),
    );
    let err = elev.to_json().unwrap_err();
    assert_eq!(
        err.to_string(),
        "Having trouble rendering njk template ./test/stubs/exitCode/failure.njk"
    );
}

fn add_to_json_templates(cfg: &mut UserConfig) -> anyhow::Result<()> {
    cfg.add_template("test.md", "# hi\n", Value::object())?;
    cfg.add_template(
        "test2.liquid",
        "---\nhi: hello\n---\n{{ hi }}",
        Value::object(),
    )
}

fn to_json_results() -> Vec<PageResult> {
    let mut elev = new(
        Some("./test/stubs-virtual-nowrite/"),
        None,
        opts(add_to_json_templates),
    );
    elev.set_is_verbose(false);
    elev.to_json().unwrap()
}

fn check_to_json(result: &[PageResult]) {
    let values = |url| {
        by_url(result, url)
            .iter()
            .map(|r| r.to_value())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        values("/test/"),
        vec![page(
            "/test/",
            "./test/stubs-virtual-nowrite/test.md",
            "./_site/test/index.html",
            "# hi\n",
            "<h1>hi</h1>\n"
        )]
    );
    assert_eq!(
        values("/test2/"),
        vec![page(
            "/test2/",
            "./test/stubs-virtual-nowrite/test2.liquid",
            "./_site/test2/index.html",
            "{{ hi }}",
            "hello"
        )]
    );
}

/// "Twelvety to json"
#[test]
fn twelvety_to_json() {
    check_to_json(&to_json_results());
}

// "Two Eleventies, two configs!!!" and "Config propagates to other instances correctly": compare
// JavaScript object identities of internals (skipped).

/// "Twelvety programmatic API without init"
#[test]
fn programmatic_api_without_init() {
    check_to_json(&to_json_results());
}

/// "Can Twelvety run two executeBuilds in parallel?" (sequentially: builds take `&mut self`)
#[test]
fn can_twelvety_run_two_execute_builds() {
    let mut elev = new(
        Some("./test/stubs-virtual-nowrite/"),
        None,
        opts(add_to_json_templates),
    );
    elev.set_is_verbose(false);
    let result1 = elev.to_json().unwrap();
    let result2 = elev.to_json().unwrap();
    check_to_json(&result1);
    check_to_json(&result2);
}

fn nowrite(
    config: impl Fn(&mut UserConfig) -> anyhow::Result<()> + Send + Sync + 'static,
) -> Twelvety {
    new(
        Some("./test/stubs-virtual-nowrite/"),
        Some("./test/stubs-virtual-nowrite/_site"),
        opts(config),
    )
}

fn nowrite_json(
    config: impl Fn(&mut UserConfig) -> anyhow::Result<()> + Send + Sync + 'static,
) -> Vec<PageResult> {
    nowrite(config).to_json().unwrap()
}

fn sorted_contents(results: &[PageResult]) -> Vec<String> {
    let mut content: Vec<String> = results
        .iter()
        .map(|r| r.content.replace("\r\n", "\n"))
        .collect();
    content.sort();
    content
}

/// "Unicode in front matter `tags`, issue #670"
#[test]
fn unicode_in_front_matter_tags_670() {
    let mut results = nowrite_json(|cfg| {
        cfg.add_template(
            "content.njk",
            "---\ntags:\n  - Cañon City\n---",
            Value::object(),
        )?;
        cfg.add_template(
            "index.njk",
            "{{ collections | length }},{% for key,item in collections %}{{ key }},{% endfor %}",
            Value::object(),
        )
    });
    results.sort_by(|a, b| b.input_path.cmp(&a.input_path));
    assert_eq!(results[0].content.trim(), "2,Cañon City,all,");
}

// "#142: date 'git Last Modified' populates page.date" and "#2224: date 'git created' populates
// page.date": the port’s checkout has no git history for the fixtures (skipped).

/// "git getUpdatedTimestamp returns undefined on nonexistent path"
#[test]
fn git_get_updated_timestamp_returns_undefined_on_nonexistent_path() {
    assert_eq!(
        twelvety::util::git::get_updated_timestamp("./test/invalid.invalid"),
        None
    );
}

/// "git getCreatedTimestamp returns undefined on nonexistent path"
#[test]
fn git_get_created_timestamp_returns_undefined_on_nonexistent_path() {
    assert_eq!(
        twelvety::util::git::get_created_timestamp("./test/invalid.invalid"),
        None
    );
}

/// "#2167: Pagination with permalink: false"
#[test]
fn pagination_with_permalink_false_2167() {
    let mut elev = nowrite(|cfg| {
        cfg.add_template(
            "paginated.njk",
            "---\ndropdown:\n  - a\n  - b\n  - c\n  - d\n  - e\npagination:\n  data: dropdown\n  size: 1\npermalink: false\n---\n",
            Value::object(),
        )
    });
    elev.set_dry_run(true);
    let (_, pages) = elev.write().unwrap();
    assert_eq!(pages.len(), 0);
    assert_eq!(elev.to_json().unwrap().len(), 5);
}

fn pagination_over_collection_using_twelvety_computed(ext: &'static str) {
    let calls = Arc::new(Mutex::new(vec![]));
    let c = calls.clone();
    let results = nowrite_json(move |cfg| {
        cfg.add_template(
            &format!("post.{ext}"),
            "---\ntags: posts\n---\nNo",
            Value::object(),
        )?;
        cfg.add_template(
            &format!("test.{ext}"),
            "---\npagination:\n  data: \"collections.posts\"\n  size: 1\nquotes:\n  - The person that shared this is awesome\ntwelvetyComputed:\n  quote: \"{{ quotes | selectRandomFromArray }}\"\n---\n{{ quote }}",
            Value::object(),
        )?;
        let c = c.clone();
        cfg.add_filter("selectRandomFromArray", move |_, arr, _| {
            c.lock().unwrap().push(arr.clone());
            Ok(arr.to_vec().first().cloned().unwrap_or_default())
        });
        Ok(())
    });
    for arr in calls.lock().unwrap().iter() {
        assert_eq!(arr, &json(r#"["The person that shared this is awesome"]"#));
    }
    assert!(!calls.lock().unwrap().is_empty());
    assert_eq!(results.len(), 2);
    assert_eq!(
        sorted_contents(&results),
        ["No", "The person that shared this is awesome"]
    );
}

/// "Pagination over collection using twelvetyComputed (liquid)"
#[test]
fn pagination_over_collection_using_twelvety_computed_liquid() {
    pagination_over_collection_using_twelvety_computed("liquid");
}

/// "Pagination over collection using twelvetyComputed (njk)"
#[test]
fn pagination_over_collection_using_twelvety_computed_njk() {
    pagination_over_collection_using_twelvety_computed("njk");
}

// "Paginated template uses proxy and global data": JavaScript data file and template (skipped).

/// "Liquid shortcode with multiple arguments(issue #2348)"
#[test]
fn liquid_shortcode_with_multiple_arguments_2348() {
    let results = nowrite_json(|cfg| {
        cfg.add_template(
            "templateWithLiquidShortcodeMultipleArguments.liquid",
            "---\nlayout: layout.liquid\n---",
            Value::object(),
        )?;
        cfg.add_template(
            "templateWithLiquidShortcodeMultipleArguments-template2.liquid",
            "---\nlayout: layout.liquid\n---",
            Value::object(),
        )?;
        cfg.add_template(
            "_includes/layout.liquid",
            "---\ntext: layout\nurl: \"/mylayout\"\n---\n{% simplelink text url text url text url %}\n{% simplelink text, url, text, url, text, url %}",
            Value::object(),
        )?;
        cfg.add_shortcode("simplelink", |_, args| {
            Ok(Value::array(args.to_vec()).to_json_string())
        });
        Ok(())
    });
    let arr = r#"["layout","/mylayout","layout","/mylayout","layout","/mylayout"]"#;
    let expected = format!("{arr}\n{arr}");
    assert_eq!(results.len(), 2);
    assert_eq!(sorted_contents(&results), [expected.clone(), expected]);
}

/// "Does pathPrefix affect page URLs" (a virtual `README.md`, upstream uses the project’s)
#[test]
fn does_path_prefix_affect_page_urls() {
    let mut options = opts(|cfg| cfg.add_template("README.md", "# Hi", Value::object()));
    options.path_prefix = Some("/testdirectory/".into());
    let results = new(
        Some("./test/stubs-virtual-nowrite/"),
        Some("./_site"),
        options,
    )
    .to_json()
    .unwrap();
    assert_eq!(results[0].url, Value::str("/README/"));
}

/// "Lodash get (for pagination data target) object key with spaces, issue #2851" (the
/// `TemplateMap.getTagTarget` part; lodash itself isn’t ported)
#[test]
fn get_tag_target_object_key_with_spaces_2851() {
    use twelvety::template_map::TemplateMap;
    assert_eq!(
        TemplateMap::get_tag_target("collections.tag with spaces").as_deref(),
        Some("tag with spaces")
    );
    assert_eq!(
        TemplateMap::get_tag_target("collections['tag with spaces']").as_deref(),
        Some("tag with spaces")
    );
    assert_eq!(
        TemplateMap::get_tag_target("collections[\"tag with spaces\"]").as_deref(),
        Some("tag with spaces")
    );
}

/// "Twelvety tag collection with spaces in the tag name, issue #2851"
#[test]
fn tag_collection_with_spaces_in_the_tag_name_2851() {
    let mut elev = nowrite(|cfg| {
        cfg.add_template(
            "paginated.njk",
            "---\npagination:\n  data: \"collections['tag with spaces']\"\n  size: 1\n---\n",
            Value::object(),
        )?;
        cfg.add_template(
            "content.njk",
            "---\ntags: ['tag with spaces']\n---",
            Value::object(),
        )?;
        cfg.data_filter_selectors.insert("collections".into());
        Ok(())
    });
    elev.set_is_verbose(false);
    let result = elev.to_json().unwrap();
    assert_eq!(result.len(), 2);
    let data = result[0].data.clone().unwrap();
    assert_eq!(
        data.get("collections").get("all").to_vec().len(),
        result.len()
    );
    assert_eq!(
        data.get("collections")
            .get("tag with spaces")
            .to_vec()
            .len(),
        1
    );
}

/// "this.page in Liquid filters with strictVariables"
#[test]
fn this_page_in_liquid_filters_with_strict_variables() {
    let results = common::to_json(
        "./test/stubs-virtual/",
        None,
        opts(|cfg| {
            cfg.set_liquid_options(json(r#"{"strictVariables": true}"#));
            cfg.add_filter("pageUrl", |ctx, _, _| {
                Ok(Value::str(format!(
                    "{}|{}",
                    ctx.page().get("url").to_output_string(),
                    ctx.get("title").to_output_string()
                )))
            });
            cfg.add_template(
                "index.liquid",
                "{{ '' | pageUrl }}",
                json(r#"{"title": "Hi"}"#),
            )
        }),
    )
    .unwrap();
    assert_eq!(results[0].content, "/|Hi");
}

// "Global data JS files should only execute once, issue #2753": JavaScript data files (skipped).

fn check_raw_input(results: &mut [PageResult]) {
    results.sort_by(|a, b| a.content.cmp(&b.content));
    assert_eq!(results.len(), 2);
    assert_eq!(
        results[0].content,
        "This is the first template.This is the first template.{{ page.rawInput }}"
    );
    assert_eq!(
        results[0].raw_input,
        "This is the first template.{{ page.rawInput }}"
    );
    assert_eq!(
        results[1].content,
        "This is the second template.This is the first template.{{ page.rawInput }}"
    );
    assert_eq!(
        results[1].raw_input,
        "This is the second template.{{ collections.tag1[0].rawInput }}"
    );
}

/// "Access to raw input of file (toJSON), issue #1206"
#[test]
fn access_to_raw_input_of_file_to_json_1206() {
    let mut results = nowrite_json(|cfg| {
        cfg.add_template(
            "page1.njk",
            "---\ntags: tag1\n---\nThis is the first template.{{ page.rawInput }}",
            Value::object(),
        )?;
        cfg.add_template(
            "page2.njk",
            "This is the second template.{{ collections.tag1[0].rawInput }}",
            Value::object(),
        )
    });
    check_raw_input(&mut results);
}

/// "Access to raw input of file (dryRun), issue #1206"
#[test]
fn access_to_raw_input_of_file_write_1206() {
    let _cleanup = Cleanup("./test/stubs-1206/_site/");
    let (_, mut results) = new(
        Some("./test/stubs-1206"),
        Some("./test/stubs-1206/_site"),
        no_config(),
    )
    .write()
    .unwrap();
    check_raw_input(&mut results);
}

/// "buildawesome.before and buildawesome.after share a new buildId per build"
#[test]
fn before_and_after_share_a_new_build_id_per_build() {
    let ids = Arc::new(Mutex::new(Vec::<String>::new()));
    let after_ids = Arc::new(Mutex::new(Vec::<String>::new()));
    let (i, a) = (ids.clone(), after_ids.clone());
    let mut elev = new(
        Some("./test/noop/"),
        Some("./test/noop/_site"),
        opts(move |cfg| {
            let (i, a) = (i.clone(), a.clone());
            cfg.on("buildawesome.before", move |arg| {
                i.lock()
                    .unwrap()
                    .push(arg.get("buildId").to_output_string());
                Ok(())
            });
            cfg.on("buildawesome.after", move |arg| {
                a.lock()
                    .unwrap()
                    .push(arg.get("buildId").to_output_string());
                Ok(())
            });
            Ok(())
        }),
    );
    elev.to_json().unwrap();
    elev.to_json().unwrap();
    let ids = ids.lock().unwrap().clone();
    assert_eq!(ids, *after_ids.lock().unwrap());
    assert_eq!(ids.len(), 2);
    assert!(
        regex::Regex::new(r"^[\da-f]{8}(-[\da-f]{4}){3}-[\da-f]{12}$")
            .unwrap()
            .is_match(&ids[0])
    );
    assert_ne!(ids[0], ids[1]);
    assert_eq!(elev.build_id.as_deref(), Some(ids[1].as_str()));
}

/// Collects the `before`/`after` event arguments (both event name prefixes)
fn event_args(dry_run: bool) -> Vec<Value> {
    let args = Arc::new(Mutex::new(vec![]));
    let a = args.clone();
    let mut options = opts(move |cfg| {
        for name in [
            "twelvety.before",
            "buildawesome.before",
            "twelvety.after",
            "buildawesome.after",
        ] {
            let a = a.clone();
            cfg.on(name, move |arg| {
                a.lock().unwrap().push(arg.clone());
                Ok(())
            });
        }
        Ok(())
    });
    options.dry_run = dry_run;
    new(Some("./test/noop/"), Some("./test/noop/_site"), options)
        .to_json()
        .unwrap();
    let args = args.lock().unwrap().clone();
    assert_eq!(args.len(), 4);
    args
}

/// "{twelvety,buildawesome}.before and {twelvety,buildawesome}.after Event Arguments, directories"
#[test]
fn before_and_after_event_arguments_directories() {
    for arg in event_args(false) {
        assert_eq!(arg.get("inputDir"), Value::str("./test/noop/"));
        assert_eq!(
            arg.get("directories").get("input"),
            Value::str("./test/noop/")
        );
        assert_eq!(
            arg.get("directories").get("includes"),
            Value::str("./test/noop/_includes/")
        );
    }
}

/// "{twelvety,buildawesome}.before and {twelvety,buildawesome}.after Event Arguments, dryRun default"
#[test]
fn before_and_after_event_arguments_dry_run_default() {
    for arg in event_args(false) {
        assert_eq!(arg.get("dryRun"), Value::Bool(false));
    }
}

/// "{twelvety,buildawesome}.before and {twelvety,buildawesome}.after Event Arguments, dryRun enabled"
#[test]
fn before_and_after_event_arguments_dry_run_enabled() {
    for arg in event_args(true) {
        assert_eq!(arg.get("dryRun"), Value::Bool(true));
    }
}

// "buildawesome.after fires sequentially setting eventEmitterMode 'sequential'": listeners are
// always sequential (synchronous) in the port (skipped).

/// The directories passed to `buildawesome.before` (plus the build result)
fn before_directories(
    input: &str,
    output: &str,
    config: impl Fn(&mut UserConfig) -> anyhow::Result<()> + Send + Sync + 'static,
) -> (Value, anyhow::Result<Vec<PageResult>>) {
    let dirs = Arc::new(Mutex::new(Value::Undefined));
    let d = dirs.clone();
    let mut elev = new(
        Some(input),
        Some(output),
        opts(move |cfg| {
            config(cfg)?;
            let d = d.clone();
            cfg.on("buildawesome.before", move |arg| {
                *d.lock().unwrap() = arg.get("directories");
                Ok(())
            });
            Ok(())
        }),
    );
    let result = elev.to_json();
    let dirs = dirs.lock().unwrap().clone();
    (dirs, result)
}

fn assert_dirs(
    dirs: &Value,
    input: &str,
    includes: &str,
    data: &str,
    layouts: Option<&str>,
    output: Option<&str>,
) {
    assert_eq!(dirs.get("input"), Value::str(input));
    assert_eq!(dirs.get("includes"), Value::str(includes));
    assert_eq!(dirs.get("data"), Value::str(data));
    assert_eq!(
        dirs.get("layouts"),
        layouts.map(Value::str).unwrap_or_default()
    );
    if let Some(output) = output {
        assert_eq!(dirs.get("output"), Value::str(output));
    }
}

/// "setInputDirectory config method #1503"
#[test]
fn set_input_directory_config_method_1503() {
    let (dirs, result) = before_directories("./test/noop/", "./test/noop/_site", |cfg| {
        cfg.set_input_directory("./test/noop2/")
    });
    result.unwrap();
    assert_dirs(
        &dirs,
        "./test/noop2/",
        "./test/noop2/_includes/",
        "./test/noop2/_data/",
        None,
        Some("./test/noop/_site/"),
    );
}

/// "setDirectoriesRelativeTo config method"
#[test]
fn set_directories_relative_to_config_method() {
    let (dirs, result) = before_directories("./test/noop/", "./test/noop/_site", |cfg| {
        cfg.set_directories_relative_to(".")?;
        cfg.set_includes_directory("test/stubs/_includes")?;
        cfg.set_data_directory("test/stubs/_data")?;
        cfg.add_template(
            "index.liquid",
            r#"{% include "included.liquid" %} {{ globalData.datakey1 }}"#,
            Value::object(),
        )
    });
    assert_dirs(
        &dirs,
        "./test/noop/",
        "./test/stubs/_includes/",
        "./test/stubs/_data/",
        None,
        None,
    );
    assert_eq!(
        result.unwrap()[0].content.trim(),
        "This is an include. datavalue1"
    );
}

/// "setDirectoriesRelativeTo only supports \".\""
#[test]
fn set_directories_relative_to_only_supports_dot() {
    let (_, result) = before_directories("./test/noop/", "./test/noop/_site", |cfg| {
        cfg.set_directories_relative_to("src")
    });
    let err = result.unwrap_err();
    assert!(
        err.chain().any(|e| e
            .to_string()
            .contains(r#"Only "." (the project root) is supported."#)),
        "{err:#}"
    );
}

/// "setIncludesDirectory config method #1503"
#[test]
fn set_includes_directory_config_method_1503() {
    let (dirs, result) = before_directories("./test/noop/", "./test/noop/_site", |cfg| {
        cfg.set_includes_directory("myincludes")
    });
    result.unwrap();
    assert_dirs(
        &dirs,
        "./test/noop/",
        "./test/noop/myincludes/",
        "./test/noop/_data/",
        None,
        Some("./test/noop/_site/"),
    );
}

/// "setDataDirectory config method #1503"
#[test]
fn set_data_directory_config_method_1503() {
    let (dirs, result) = before_directories("./test/noop/", "./test/noop/_site", |cfg| {
        cfg.set_data_directory("data")
    });
    result.unwrap();
    assert_dirs(
        &dirs,
        "./test/noop/",
        "./test/noop/_includes/",
        "./test/noop/data/",
        None,
        Some("./test/noop/_site/"),
    );
}

/// "setLayoutsDirectory config method #1503"
#[test]
fn set_layouts_directory_config_method_1503() {
    let (dirs, result) = before_directories("./test/noop/", "./test/noop/_site", |cfg| {
        cfg.set_layouts_directory("layouts")
    });
    result.unwrap();
    assert_dirs(
        &dirs,
        "./test/noop/",
        "./test/noop/_includes/",
        "./test/noop/_data/",
        Some("./test/noop/layouts/"),
        Some("./test/noop/_site/"),
    );
}

/// "setInputDirectory config method #1503 in a plugin throws error"
#[test]
fn set_input_directory_config_method_in_a_plugin_throws_error() {
    let (_, result) = before_directories("./test/noop/", "./test/noop/_site", |cfg| {
        cfg.add_plugin(
            "",
            |cfg, _| cfg.set_input_directory("./test/noop2/"),
            Value::object(),
        )
    });
    let err = result.unwrap_err();
    // The `set*Directory` configuration API methods are not yet allowed in plugins.
    assert!(
        err.chain()
            .any(|e| e.to_string() == "Error processing a plugin"),
        "{err:#}"
    );
}

fn absolute(p: &str) -> String {
    std::path::Path::new(p)
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .to_string()
}

/// "Accepts absolute paths for input and output"
#[test]
fn accepts_absolute_paths_for_input_and_output() {
    let mut elev = new(
        Some(&absolute("./test/noop/")),
        Some(&format!("{}/_site", absolute("./test/noop/"))),
        no_config(),
    );
    elev.to_json().unwrap();
    let dirs = elev.directories.clone().unwrap();
    assert_eq!(dirs.input(), "./test/noop/");
    assert_eq!(dirs.includes(), "./test/noop/_includes/");
    assert_eq!(dirs.data(), "./test/noop/_data/");
    assert_eq!(dirs.layouts(), None);
    assert_eq!(dirs.output(), "./test/noop/_site/");
}

/// "Accepts absolute paths urls for input and output, results output #3805"
#[test]
fn accepts_absolute_paths_for_input_and_output_results_output_3805() {
    let input = absolute("./test/stubs-absolute/test.md");
    let output = format!("{}/_site", absolute("./test/stubs-absolute"));
    assert_eq!(
        new(Some(&input), Some(&output), no_config())
            .to_json()
            .unwrap()
            .len(),
        1
    );
}

/// "Accepts absolute paths urls for input and output and a virtual template, results output #3805"
#[test]
fn accepts_absolute_paths_for_input_and_output_and_a_virtual_template_3805() {
    let input = absolute("./test/noop/");
    let output = format!("{input}/_site");
    let mut elev = new(
        Some(&input),
        Some(&output),
        opts(|cfg| cfg.add_template("index.md", "# Title", Value::object())),
    );
    assert_eq!(elev.to_json().unwrap().len(), 1);
}

/// "Twelvety pagination works okay with reserved data throws (twelvety) Issue #3262"
#[test]
fn pagination_works_okay_with_reserved_data_throws_3262() {
    let results = common::to_json(
        "./test/stubs-virtual/",
        None,
        opts(|cfg| {
            cfg.add_template(
                "index.html",
                "---\npagination:\n  data: \"test\"\n  size: 1\ntest:\n  - a\n  - b\n  - c\n---\n{{ twelvety.generator }}",
                Value::object(),
            )
        }),
    )
    .unwrap();
    assert_eq!(results.len(), 3);
}

fn virtual_json(
    config: impl Fn(&mut UserConfig) -> anyhow::Result<()> + Send + Sync + 'static,
) -> anyhow::Result<Vec<PageResult>> {
    common::to_json("./test/stubs-virtual/", None, opts(config))
}

/// "Twelvety setting pkg data is okay when pkg is remapped to parkour"
#[test]
fn setting_pkg_data_is_okay_when_pkg_is_remapped_to_parkour() {
    let results = virtual_json(|cfg| {
        cfg.keys.package = Some("parkour".into());
        cfg.add_template(
            "index.html",
            "---\npkg:\n  myOwn: OVERRIDE\n---",
            Value::object(),
        )
    })
    .unwrap();
    assert_eq!(
        results[0].to_value(),
        page(
            "/",
            "./test/stubs-virtual/index.html",
            "./_site/index.html",
            "",
            ""
        )
    );
}

/// "Twelvety setting pkg data is okay when keys.package is false"
#[test]
fn setting_pkg_data_is_okay_when_keys_package_is_false() {
    let results = virtual_json(|cfg| {
        cfg.keys.package = None;
        cfg.add_template(
            "index.html",
            "---\npkg:\n  myOwn: OVERRIDE\n---\n{{ pkg.myOwn }}",
            Value::object(),
        )
    })
    .unwrap();
    assert_eq!(
        results[0].to_value(),
        page(
            "/",
            "./test/stubs-virtual/index.html",
            "./_site/index.html",
            "{{ pkg.myOwn }}",
            "OVERRIDE"
        )
    );
}

fn index_content(results: &[PageResult]) -> String {
    results
        .iter()
        .find(|r| r.input_path.ends_with("index.njk"))
        .unwrap()
        .content
        .clone()
}

/// "keys.tags false skips tag collections, #1254"
#[test]
fn keys_tags_false_skips_tag_collections_1254() {
    let results = virtual_json(|cfg| {
        cfg.keys.tags = None;
        cfg.add_collection("custom", |api| {
            Ok(Value::array(
                api.get_all()
                    .to_vec()
                    .into_iter()
                    .filter(|item| {
                        item.get("data")
                            .get("tags")
                            .to_vec()
                            .contains(&Value::str("post"))
                    })
                    .collect(),
            ))
        })?;
        cfg.add_collection("byTag", |api| Ok(api.get_filtered_by_tag("post")))?;
        cfg.add_template("post.md", "", json(r#"{"tags": ["post"]}"#))?;
        cfg.add_template(
            "index.njk",
            "{% for name, items in collections %}{{ name }}:{{ items.length }},{% endfor %}",
            Value::object(),
        )
    })
    .unwrap();
    assert_eq!(index_content(&results), "custom:1,byTag:0,all:2,");
}

/// "keys.tags renamed creates collections from another key, #1254"
#[test]
fn keys_tags_renamed_creates_collections_from_another_key_1254() {
    let results = virtual_json(|cfg| {
        cfg.keys.tags = Some("category".into());
        cfg.add_collection("byTag", |api| Ok(api.get_filtered_by_tag("news")))?;
        cfg.add_template(
            "post.md",
            "",
            json(r#"{"category": "news", "tags": {"custom": true}}"#),
        )?;
        cfg.add_template(
            "index.njk",
            "{% for name, items in collections %}{{ name }}:{{ items.length }},{% endfor %}",
            Value::object(),
        )
    })
    .unwrap();
    assert_eq!(index_content(&results), "news:1,byTag:1,all:2,");
}

/// "keys.tags with a nested property throws, #1254"
#[test]
fn keys_tags_with_a_nested_property_throws_1254() {
    let mut elev = new(
        Some("./test/stubs-virtual/"),
        None,
        opts(|cfg| {
            cfg.keys.tags = Some("meta.tags".into());
            Ok(())
        }),
    );
    let err = elev.initialize_config().unwrap_err();
    assert!(
        format!("{err:#}").contains("Nested properties are not supported in `keys.tags`"),
        "{err:#}"
    );
}

fn draft_schema() -> Value {
    Value::function(|args| {
        if args[0].get("draft").as_bool().is_none() {
            anyhow::bail!("Invalid data type for draft.");
        }
        Ok(Value::Undefined)
    })
}

fn with_schema(mut data: Value, schema: Value) -> Value {
    data.set("twelvetyDataSchema", schema);
    data
}

/// "Twelvety data schema (success) #879"
#[test]
fn data_schema_success_879() {
    let results = virtual_json(|cfg| {
        for name in ["index1.html", "index2.html"] {
            cfg.add_template(
                name,
                "",
                with_schema(json(r#"{"draft": true}"#), draft_schema()),
            )?;
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(results.len(), 2);
}

/// "Twelvety data schema (fails) #879"
#[test]
fn data_schema_fails_879() {
    let err = virtual_json(|cfg| {
        cfg.add_template(
            "index1.html",
            "",
            with_schema(json(r#"{"draft": 1}"#), draft_schema()),
        )
    })
    .unwrap_err();
    assert_eq!(
        err.to_string(),
        "Error in the data schema for: ./test/stubs-virtual/index1.html (via `buildawesomeDataSchema` or `twelvetyDataSchema`)"
    );
    assert_eq!(
        err.source().unwrap().to_string(),
        "Invalid data type for draft."
    );
}

// "Twelvety data schema (fails, using zod) #879": zod is a JavaScript library (skipped).

/// "Twelvety data schema has access to custom collections created via API #879"
#[test]
fn data_schema_has_access_to_custom_collections_879() {
    let lengths = Arc::new(Mutex::new(vec![]));
    let l = lengths.clone();
    let results = virtual_json(move |cfg| {
        cfg.add_collection("userCollection", |c| Ok(c.get_all()))?;
        let l = l.clone();
        let schema = Value::function(move |args| {
            l.lock().unwrap().push(
                args[0]
                    .get("collections")
                    .get("userCollection")
                    .to_vec()
                    .len(),
            );
            Ok(Value::Undefined)
        });
        cfg.add_template("index1.html", "", with_schema(Value::object(), schema))
    })
    .unwrap();
    assert_eq!(*lengths.lock().unwrap(), [1]);
    assert_eq!(results.len(), 1);
}

/// "Twelvety transforms filter (using collections and page override data)"
#[test]
fn transforms_filter_using_collections_and_page_override_data() {
    let mut options = opts(|cfg| {
        cfg.add_plugin(
            "HtmlBasePlugin",
            twelvety::plugins::html_base::plugin,
            Value::object(),
        )?;
        cfg.add_template(
            "index.html",
            r#"<img src="/test.png" alt="abc">"#,
            json(r#"{"tags": "posts"}"#),
        )?;
        cfg.add_template(
            "feed.njk",
            "{% for post in collections.posts %}{{ post.content | renderTransforms(post.page) | safe }}{% endfor %}",
            json(r#"{"permalink": "feed.xml"}"#),
        )
    });
    options.path_prefix = Some("hi".into());
    let results = common::to_json("./test/stubs-virtual/", None, options).unwrap();
    assert_eq!(results.len(), 2);
    let content = |suffix: &str| {
        results
            .iter()
            .find(|r| r.input_path.ends_with(suffix))
            .unwrap()
            .content
            .clone()
    };
    assert_eq!(
        content("index.html"),
        r#"<img src="/hi/test.png" alt="abc">"#
    );
    assert_eq!(content("feed.njk"), r#"<img src="/hi/test.png" alt="abc">"#);
}

fn compile_extension(
    f: impl Fn(&str) -> String + Send + Sync + 'static,
) -> twelvety::user_config::ExtensionOptions {
    let f = Arc::new(f);
    twelvety::user_config::ExtensionOptions {
        compile: Some(Arc::new(move |src: &str, _input_path: &str| {
            let (f, src) = (f.clone(), src.to_string());
            Ok(Arc::new(move |_: &Value| Ok(f(&src))) as twelvety::user_config::CompiledFn)
        })),
        ..Default::default()
    }
}

/// "Custom Markdown Render with permalink, Issue #2780" (a minimal heading renderer stands in for `marked`)
#[test]
fn custom_markdown_render_with_permalink_2780() {
    let results = virtual_json(|cfg| {
        cfg.add_extension(
            "md",
            compile_extension(|s| format!("<h1>{}</h1>", s.trim_start_matches("# "))),
        )?;
        cfg.add_template(
            "template.md",
            "# Markdown?",
            json(r#"{"permalink": "/permalink.html"}"#),
        )
    })
    .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].url, Value::str("/permalink.html"));
    assert_eq!(results[0].content.trim(), "<h1>Markdown?</h1>");
}

/// "Custom Markdown Render with permalink, Issue #2780 #3339"
#[test]
fn custom_markdown_render_with_permalink_2780_3339() {
    let results = virtual_json(|cfg| {
        cfg.add_template_formats(&["markdown"]);
        cfg.add_extension(
            "markdown",
            twelvety::user_config::ExtensionOptions {
                key: "md".into(),
                ..Default::default()
            },
        )?;
        cfg.add_template(
            "filename-hi.markdown",
            "# Markdown?",
            json(r#"{"permalink": "/{{ page.fileSlug }}.html"}"#),
        )
    })
    .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].url, Value::str("/filename-hi.html"));
    assert_eq!(results[0].content.trim(), "<h1>Markdown?</h1>");
}

/// "Alias a custom template syntax registered after the alias, Issue #2827"
#[test]
fn alias_a_custom_template_syntax_registered_after_the_alias_2827() {
    let results = virtual_json(|cfg| {
        cfg.add_template_formats(&["foo", "bar"]);
        cfg.add_extension(
            "bar",
            twelvety::user_config::ExtensionOptions {
                key: "foo".into(),
                ..Default::default()
            },
        )?;
        cfg.add_extension("foo", compile_extension(|s| format!("FOO({s})")))?;
        cfg.add_template("index.bar", "hi", Value::object())
    })
    .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].content, "FOO(hi)");
}

/// "Alias a custom template syntax added via plugin, Issue #2827"
#[test]
fn alias_a_custom_template_syntax_added_via_plugin_2827() {
    let results = virtual_json(|cfg| {
        cfg.add_plugin(
            "",
            |api, _| {
                api.add_template_formats(&["foo"]);
                api.add_extension("foo", compile_extension(|s| format!("FOO({s})")))
            },
            Value::object(),
        )?;
        cfg.add_template_formats(&["bar"]);
        cfg.add_extension(
            "bar",
            twelvety::user_config::ExtensionOptions {
                key: "foo".into(),
                ..Default::default()
            },
        )?;
        cfg.add_template("index.bar", "hi", Value::object())
    })
    .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].content, "FOO(hi)");
}

fn is_duplicate_permalink_error(err: &anyhow::Error) -> bool {
    err.chain().any(|e| {
        e.downcast_ref::<twelvety::errors::DuplicatePermalinkOutputError>()
            .is_some()
    }) || err
        .downcast_ref::<twelvety::errors::DuplicatePermalinkOutputError>()
        .is_some()
}

/// "Test input/output conflicts (input overwrites output), Issue #3327"
#[test]
fn input_output_conflicts_input_overwrites_output_3327() {
    let err = common::to_json(
        "./test/stubs-virtual/",
        Some("./test/stubs-virtual/"),
        opts(|cfg| {
            cfg.add_template(
                "test.html",
                "# Markdown",
                json(r#"{"permalink": "test.html"}"#),
            )
        }),
    )
    .unwrap_err();
    assert!(is_duplicate_permalink_error(&err), "{err:#}");
}

/// "Test input/output conflicts (output overwrites another input), Issue #3327"
#[test]
fn input_output_conflicts_output_overwrites_another_input_3327() {
    let err = common::to_json(
        "./test/stubs-virtual/",
        Some("./test/stubs-virtual/"),
        opts(|cfg| {
            cfg.add_template("test.html", "# Markdown", Value::object())?;
            cfg.add_template(
                "index.html",
                "# Markdown",
                json(r#"{"permalink": "test.html"}"#),
            )
        }),
    )
    .unwrap_err();
    assert!(is_duplicate_permalink_error(&err), "{err:#}");
}

// "Twelvety data schema has access to custom collections created via API #613 #3345": modifies
// `item.template.frontMatter` from JavaScript (skipped).

// "Custom Nunjucks syntax has shortcode with access to `this`, Issue #3310": uses a JavaScript
// Nunjucks environment and `augmentFunctionContext` (skipped).

/// "Related to issue 3206: Does Nunjucks throwOnUndefined variables require normalizeContext to be a lazy get"
#[test]
fn nunjucks_throw_on_undefined_with_shortcode_3206() {
    let results = virtual_json(|cfg| {
        cfg.add_shortcode("customized", |ctx, _| {
            Ok(format!(
                "{}:Custom Shortcode",
                ctx.page().get("url").to_output_string()
            ))
        });
        cfg.set_nunjucks_environment_options(json(r#"{"throwOnUndefined": true}"#));
        cfg.add_template(
            "index.html",
            "HELLO{% customized %}:{{ page.url }}",
            Value::object(),
        )
    })
    .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].content.trim(), "HELLO/:Custom Shortcode:/");
}

/// "#727: Error messaging when trying to use a missing layout"
#[test]
fn error_messaging_when_trying_to_use_a_missing_layout_727() {
    let err = virtual_json(|cfg| {
        cfg.add_template(
            "index.html",
            "HELLO {{ page.url }}",
            json(r#"{"layout": "does-not-exist.html"}"#),
        )
    })
    .unwrap_err();
    assert_eq!(
        err.to_string(),
        r#"Problem creating an Twelvety Layout for the "./test/stubs-virtual/index.html" template file."#
    );
}

/// "#1419: Shortcode in a permalink"
#[test]
fn shortcode_in_a_permalink_1419() {
    let results = virtual_json(|cfg| {
        cfg.add_shortcode("shortcode", |_, _| Ok("url-slug".into()));
        cfg.add_template(
            "index.njk",
            "",
            json(r#"{"permalink": "/{% shortcode %}/"}"#),
        )
    })
    .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].url, Value::str("/url-slug/"));
}

/// "#3373: Throw an error when explicit config path is not found."
#[test]
fn throw_an_error_when_explicit_config_path_is_not_found_3373() {
    let options = Options {
        config_path: Some("this-file-is-not-found.js".into()),
        ..Default::default()
    };
    let err = new(Some("./test/stubs-virtual/"), None, options)
        .to_json()
        .unwrap_err();
    assert_eq!(
        err.to_string(),
        "A configuration file was specified but not found: this-file-is-not-found.js"
    );
}

// "Twelvety loader can force ESM mode" / "… CommonJS mode": JavaScript module loading (skipped).
// "Truthy outputPath without a file extension now throws an error, issue #3399": JavaScript template (skipped).

/// "Truthy outputPath without a file extension can be ignored, issue #3399"
#[test]
fn truthy_output_path_without_a_file_extension_can_be_ignored_3399() {
    let results = virtual_json(|cfg| {
        cfg.add_template(
            "index.html",
            "",
            json(r#"{"permalink": "foo", "twelvetyAllowMissingExtension": true}"#),
        )
    })
    .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].url, Value::str("/foo"));
}

/// "Allow list for some file types without a file extension, issue #3399"
#[test]
fn allow_list_for_some_file_types_without_a_file_extension_3399() {
    let results = virtual_json(|cfg| {
        cfg.add_template(
            "index.html",
            "",
            json(r#"{"permalink": "/test/_redirects"}"#),
        )
    })
    .unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].url, Value::str("/test/_redirects"));
}

/// "Truthy outputPath without a file extension error message is disabled, issue #3399"
#[test]
fn truthy_output_path_without_a_file_extension_error_message_is_disabled_3399() {
    let results = virtual_json(|cfg| {
        cfg.configure_error_reporting("allowMissingExtensions", "true");
        cfg.add_template("index.html", "", json(r#"{"permalink": "foo"}"#))
    })
    .unwrap();
    assert_eq!(results.len(), 1);
}

/// "Truthy outputPath without a file extension error message is disabled via missingExtensions: \"allow\""
#[test]
fn truthy_output_path_without_a_file_extension_error_message_is_disabled_via_missing_extensions_allow()
 {
    let results = virtual_json(|cfg| {
        cfg.configure_error_reporting("missingExtensions", "allow");
        cfg.add_template("index.html", "", json(r#"{"permalink": "foo"}"#))
    })
    .unwrap();
    assert_eq!(results.len(), 1);
}

/// "permalink: false outputPath new error message won’t throw an error, issue #3399"
#[test]
fn permalink_false_output_path_wont_throw_an_error_3399() {
    let results =
        virtual_json(|cfg| cfg.add_template("index.html", "", json(r#"{"permalink": false}"#)))
            .unwrap();
    assert_eq!(results.len(), 1);
}

// "permalink on custom template lang, issue #3619" and "sass docs on 12ty.dev, issue #408":
// JavaScript `compileOptions.permalink` functions and sass (skipped).

/// "Template data throws error when tags is not an Array or String #1791"
#[test]
fn template_data_throws_error_when_tags_is_not_an_array_or_string_1791() {
    let err = new(
        Some("./test/noop/"),
        Some("./test/noop/_site"),
        opts(|cfg| cfg.add_template("index.html", "", json(r#"{"tags": {"one": 1, "two": 2}}"#))),
    )
    .to_json()
    .unwrap_err();
    assert_eq!(
        err.to_string(),
        "String or Array expected for `tags` in virtual template: ./test/noop/index.html. Received: { one: 1, two: 2 }"
    );
}

/// "Use a date object for `date`, issue #3022" (JavaScript front matter variants skipped)
#[test]
fn use_a_date_object_for_date_3022() {
    let results = virtual_json(|cfg| {
        cfg.data_filter_selectors.insert("page.date".into());
        let mut data = Value::object();
        data.set("date", Value::Date(chrono::Utc::now()));
        cfg.add_template("index.html", "", data)
    })
    .unwrap();
    assert_eq!(results.len(), 1);
    assert!(matches!(
        results[0].data.as_ref().unwrap().get("page").get("date"),
        Value::Date(_)
    ));
}

/// "Cleaner constructor args #3880"
#[test]
fn cleaner_constructor_args_3880() {
    let results =
        virtual_json(|cfg| cfg.add_template("index.md", "# Title", Value::object())).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].content.trim(), "<h1>Title</h1>");
}

/// "--pathprefix is available as $config.pathPrefix in the config callback #3439"
#[test]
fn pathprefix_is_available_in_the_config_callback_3439() {
    let seen = Arc::new(Mutex::new(None));
    let s = seen.clone();
    let mut options = opts(move |cfg| {
        *s.lock().unwrap() = cfg.path_prefix.clone();
        Ok(())
    });
    options.path_prefix = Some("/hi/".into());
    new(Some("./test/stubs-virtual/"), None, options)
        .initialize_config()
        .unwrap();
    assert_eq!(seen.lock().unwrap().as_deref(), Some("/hi/"));
}

/// "--formats is available as $config.templateFormats in the config callback #3440"
#[test]
fn formats_is_available_in_the_config_callback_3440() {
    let seen = Arc::new(Mutex::new(None));
    let s = seen.clone();
    let mut elev = new(
        Some("./test/stubs-virtual/"),
        None,
        opts(move |cfg| {
            *s.lock().unwrap() = cfg.template_formats.clone();
            Ok(())
        }),
    );
    elev.set_formats("md,njk");
    elev.initialize_config().unwrap();
    assert_eq!(
        seen.lock().unwrap().clone(),
        Some(vec!["md".to_string(), "njk".to_string()])
    );
}

// "--formats=* still includes default formats #3440": JavaScript templates; "… in a config file"
// variants need JavaScript configuration files (skipped).

/// "Global data objects are shared across templates and copied on write" (object identity and
/// freezing checks are JavaScript-specific)
#[test]
fn global_data_objects_are_shared_across_templates_and_copied_on_write() {
    let mut results = virtual_json(|cfg| {
        cfg.add_global_data(
            "site",
            json(r#"{"meta": {"title": "Global"}, "nav": {"home": "/"}}"#),
        );
        cfg.add_template(
            "a.njk",
            "{{ site.meta.title }} {{ site.meta.lang }}",
            json(r#"{"site": {"meta": {"lang": "en"}}}"#),
        )?;
        cfg.add_template(
            "b.njk",
            "{{ site.meta.title }} {{ site.meta.lang }}",
            Value::object(),
        )
    })
    .unwrap();
    results.sort_by(|a, b| a.input_path.cmp(&b.input_path));
    assert_eq!(results[0].content, "Global en");
    assert_eq!(results[1].content, "Global ");
}

/// "Global data passed in by the user isn’t frozen in place" (content check only)
#[test]
fn global_data_passed_in_by_the_user_isnt_frozen_in_place() {
    let results = virtual_json(|cfg| {
        cfg.add_global_data("site", json(r#"{"meta": {"title": "Global"}}"#));
        cfg.add_template("index.njk", "{{ site.meta.title }}", Value::object())
    })
    .unwrap();
    assert_eq!(results[0].content, "Global");
}

// "setFreezeGlobalData(false) gives each template its own mutable copy": mutates JavaScript
// objects held by the template map (skipped).
