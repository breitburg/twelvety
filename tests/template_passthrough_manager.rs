//! Port of upstream `test/TemplatePassthroughManagerTest.js`.

use std::path::Path;
use std::sync::Arc;

use indexmap::IndexMap;
use twelvety::extension_map::ExtensionMap;
use twelvety::file_system_search::FileSystemSearch;
use twelvety::template_config::TemplateConfig;
use twelvety::template_passthrough::{CopyResult, TemplatePassthrough, TemplatePassthroughManager};
use twelvety::testing::{json, quiet_logger};
use twelvety::user_config::UserConfig;
use twelvety::util::project_directories::{ConfigDirs, ProjectDirectories};
use twelvety::{Eleventy, Options, Value};

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn config_with(dirs: Option<ProjectDirectories>, callback: impl Fn(&mut UserConfig)) -> TemplateConfig {
    let mut tc = TemplateConfig::new(Some(""));
    tc.set_logger(quiet_logger());
    if let Some(dirs) = dirs {
        tc.set_directories(dirs);
    }
    callback(&mut tc.user_config);
    tc.init().unwrap();
    tc
}

fn manager(tc: &TemplateConfig) -> TemplatePassthroughManager {
    TemplatePassthroughManager::new(
        tc.get_config(),
        tc.directories.clone(),
        Arc::new(FileSystemSearch::new()),
        tc.exists_cache.clone(),
        quiet_logger(),
    )
}

fn extension_map(tc: &TemplateConfig, formats: &[&str]) -> ExtensionMap {
    let mut map = ExtensionMap::new(&tc.get_config().extension_map);
    map.set_formats(&strings(formats));
    map
}

/// `{ inputPath, outputPath, isDynamicPattern, copyOptions }`
fn describe(p: &TemplatePassthrough) -> (String, Value, bool, Value) {
    (p.input_path.clone(), p.output_path.clone(), p.is_dynamic_pattern(), p.copy_options.clone())
}

fn entry(input: &str, output: Value, dynamic: bool) -> (String, Value, bool, Value) {
    (input.to_string(), output, dynamic, Value::object())
}

fn passthrough_mapping(cfg: &mut UserConfig, input: &str, output: &str) {
    cfg.add_passthrough_copy_mapping(input, output);
}

/// "Get paths from Config"
#[test]
fn get_paths_from_config() {
    let tc = config_with(None, |cfg| {
        cfg.add_passthrough_copy("img");
    });
    let mgr = manager(&tc);
    assert_eq!(
        mgr.get_config_paths().iter().map(describe).collect::<Vec<_>>(),
        vec![entry("./img", Value::Bool(true), false)]
    );
}

// "filterToPassthroughCopyFilesOnly": incremental (watch) passthrough copy isn’t ported (skipped).

/// "Get glob paths from config"
#[test]
fn get_glob_paths_from_config() {
    let tc = config_with(None, |cfg| {
        cfg.add_passthrough_copy("test/stubs/img");
        passthrough_mapping(cfg, "test/stubs/img/**", "./");
        passthrough_mapping(cfg, "test/stubs/img/*.js", "./");
    });
    assert_eq!(
        manager(&tc).get_config_path_globs(),
        strings(&["./test/stubs/img/**", "./test/stubs/img/**", "./test/stubs/img/*.js"])
    );
}

/// "Get file paths"
#[test]
fn get_file_paths() {
    let tc = config_with(None, |_| {});
    let map = extension_map(&tc, &[]);
    assert_eq!(manager(&tc).get_non_template_paths(&strings(&["test.png"]), &map), strings(&["test.png"]));
}

/// "Get file paths (filter out real templates)"
#[test]
fn get_file_paths_filter_out_real_templates() {
    let tc = config_with(None, |_| {});
    let map = extension_map(&tc, &["njk"]);
    assert_eq!(manager(&tc).get_non_template_paths(&strings(&["test.njk"]), &map), Vec::<String>::new());
}

/// "Get file paths (filter out real templates), multiple"
#[test]
fn get_file_paths_filter_out_real_templates_multiple() {
    let tc = config_with(None, |_| {});
    let map = extension_map(&tc, &["njk"]);
    assert_eq!(
        manager(&tc).get_non_template_paths(&strings(&["test.njk", "test.png"]), &map),
        strings(&["test.png"])
    );
}

/// "Get file paths with a js file (filter out real templates), multiple"
#[test]
fn get_file_paths_with_a_js_file_filter_out_real_templates_multiple() {
    let tc = config_with(None, |_| {});
    let map = extension_map(&tc, &["njk"]);
    assert_eq!(
        manager(&tc).get_non_template_paths(&strings(&["test.njk", "test.js"]), &map),
        strings(&["test.js"])
    );
}

/// "Get file paths (one image path)"
#[test]
fn get_file_paths_one_image_path() {
    let tc = config_with(None, |_| {});
    let map = extension_map(&tc, &[]);
    assert_eq!(manager(&tc).get_non_template_paths(&strings(&["test.png"]), &map), strings(&["test.png"]));
}

/// "Naughty paths outside of project dir"
///
/// The `"../*": "./"` entry is left out: it globs (and walks) the parent of the project directory.
#[test]
fn naughty_paths_outside_of_project_dir() {
    let mut dirs = ProjectDirectories::new();
    dirs.set_input(Some("./test/stubs/template-passthrough2/"), None).unwrap();
    dirs.set_output(Some("./test/stubs/template-passthrough2/_site/"));
    let tc = config_with(Some(dirs), |cfg| {
        cfg.add_passthrough_copy("../static");
        passthrough_mapping(cfg, "./test/stubs/template-passthrough2/static/*.css", "./");
        passthrough_mapping(cfg, "./test/stubs/template-passthrough2/static/*.js", "../../");
        passthrough_mapping(cfg, "./test/stubs/template-passthrough2/img.jpg", "../../");
    });
    let mgr = manager(&tc);
    let mut map = IndexMap::new();
    let result: anyhow::Result<()> = mgr
        .get_config_paths()
        .iter()
        .try_for_each(|pass| mgr.copy_passthrough(pass, &mut map).map(|_| ()));
    let _ = std::fs::remove_dir_all("./test/stubs/template-passthrough2/_site");
    assert_eq!(
        result.unwrap_err().to_string(),
        "Having trouble copying './test/stubs/template-passthrough2/static/*.js'"
    );
    for path in [
        "./test/stubs/template-passthrough2/_site/static",
        "./test/stubs/template-passthrough2/_site/nope.txt",
        "./test/stubs/template-passthrough2/_site/nope/",
        "./test/stubs/test.js",
        "./test/stubs/img.jpg",
    ] {
        assert!(!Path::new(path).exists(), "{path}");
    }
}

/// "getAllNormalizedPaths"
#[test]
fn get_all_normalized_paths() {
    let tc = config_with(None, |cfg| {
        cfg.add_passthrough_copy("img");
    });
    let map = extension_map(&tc, &[]);
    assert_eq!(
        manager(&tc).get_all_normalized_paths(&[], &map).iter().map(describe).collect::<Vec<_>>(),
        vec![entry("./img", Value::Bool(true), false)]
    );
}

/// "getAllNormalizedPaths with globs"
#[test]
fn get_all_normalized_paths_with_globs() {
    let tc = config_with(None, |cfg| {
        cfg.add_passthrough_copy("img");
        passthrough_mapping(cfg, "img/**", "./");
        passthrough_mapping(cfg, "img/*.js", "./");
    });
    let map = extension_map(&tc, &[]);
    assert_eq!(
        manager(&tc).get_all_normalized_paths(&[], &map).iter().map(describe).collect::<Vec<_>>(),
        vec![
            entry("./img", Value::Bool(true), false),
            entry("./img/**", Value::str(""), true),
            entry("./img/*.js", Value::str(""), true),
        ]
    );
}

/// "getAliasesFromPassthroughResults with Unicode filenames"
#[test]
fn get_aliases_from_passthrough_results_with_unicode_filenames() {
    let tc = config_with(None, |_| {});
    let result = |src: &str, dest: &str| CopyResult {
        map: [(src.to_string(), dest.to_string())].into_iter().collect(),
        ..Default::default()
    };
    let results = vec![
        result("./path/file", "_site/path/file"),
        result("./测试.用例/⌘", "_site/测试.用例/⌘"),
        result("./path/测试.用例", "_site/path/测试.用例"),
        result("./测试.用例/file", "_site/测试.用例/file"),
    ];
    let expected: IndexMap<String, String> = [
        ("/path/file", "./path/file"),
        ("/%E6%B5%8B%E8%AF%95.%E7%94%A8%E4%BE%8B/%E2%8C%98", "./测试.用例/⌘"),
        ("/path/%E6%B5%8B%E8%AF%95.%E7%94%A8%E4%BE%8B", "./path/测试.用例"),
        ("/%E6%B5%8B%E8%AF%95.%E7%94%A8%E4%BE%8B/file", "./测试.用例/file"),
    ]
    .into_iter()
    .map(|(a, b)| (a.to_string(), b.to_string()))
    .collect();
    assert_eq!(manager(&tc).get_aliases_from_passthrough_results(&results), expected);
}

/// "Look for uniqueness on template passthrough paths #1677"
#[test]
fn look_for_uniqueness_on_template_passthrough_paths_1677() {
    let mut dirs = ProjectDirectories::new();
    dirs.set_via_config_object(&ConfigDirs {
        input: Some("test/stubs/template-passthrough-duplicates/".into()),
        output: Some("test/stubs/template-passthrough-duplicates/_site".into()),
        ..Default::default()
    })
    .unwrap();
    let tc = config_with(Some(dirs), |cfg| {
        passthrough_mapping(cfg, "./test/stubs/template-passthrough-duplicates/input/**/*.png", "./");
    });
    let map = extension_map(&tc, &[]);
    let err = manager(&tc).copy_all(&[], &map).unwrap_err();
    let _ = std::fs::remove_dir_all("test/stubs/template-passthrough-duplicates/_site/");
    assert_eq!(
        err.to_string(),
        "Multiple passthrough copy files are trying to write to the same output file (./test/stubs/template-passthrough-duplicates/_site/avatar.png). ./test/stubs/template-passthrough-duplicates/input/avatar.png and ./test/stubs/template-passthrough-duplicates/input/src/views/avatar.png"
    );
}

// "Incremental passthrough, issue #3285", "Emulated passthrough copy sends entries …",
// "Emulated incremental passthrough only includes what changed",
// "setPassthroughCopyOptions with `rename` disables dev server emulation" and
// "`rawPathFilter` disables dev server emulation": incremental builds and dev server passthrough
// emulation aren’t ported (skipped).

/// "setPassthroughCopyOptions sets defaults that per-call options override"
#[test]
fn set_passthrough_copy_options_sets_defaults_that_per_call_options_override() {
    let tc = config_with(None, |cfg| {
        cfg.set_passthrough_copy_options(json(r#"{"junk": true, "dot": false}"#)).unwrap();
        cfg.add_passthrough_copy("img");
        cfg.add_passthrough_copy_with_options("static", json(r#"{"dot": true}"#));
    });
    assert_eq!(
        manager(&tc).get_config_paths().iter().map(|p| p.copy_options.clone()).collect::<Vec<_>>(),
        vec![json(r#"{"junk": true, "dot": false}"#), json(r#"{"junk": true, "dot": true}"#)]
    );
}

/// A unique output directory (removed on drop)
struct TempOutput(std::path::PathBuf);

impl TempOutput {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        TempOutput(dir)
    }

    fn output(&self) -> String {
        self.0.join("_site").to_string_lossy().to_string()
    }
}

impl Drop for TempOutput {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn write_site(output: &str, config: impl Fn(&mut UserConfig) -> anyhow::Result<()> + Send + Sync + 'static) -> anyhow::Result<()> {
    let options = Options {
        quiet_mode: Some(true),
        config_path: Some(String::new()),
        ..Default::default()
    }
    .with_config(config);
    let mut core = Eleventy::new(Some("test/stubs/template-passthrough"), Some(output), options);
    core.disable_logger();
    core.write().map(|_| ())
}

/// "setPassthroughCopyOptions applies to copied files"
#[test]
fn set_passthrough_copy_options_applies_to_copied_files() {
    let tmp = TempOutput::new("passthrough-defaults");
    let output = tmp.output();
    write_site(&output, |cfg| {
        cfg.set_passthrough_copy_options(json(r#"{"dot": false}"#))?;
        cfg.add_passthrough_copy_mapping("test/stubs/template-passthrough", "copy");
        Ok(())
    })
    .unwrap();
    assert!(Path::new(&output).join("copy/img.jpg").exists());
    assert!(!Path::new(&output).join("copy/.htaccess").exists());
}

/// "`rawPathFilter` receives full paths when copying"
#[test]
fn raw_path_filter_receives_full_paths_when_copying() {
    let tmp = TempOutput::new("passthrough-rawpathfilter");
    let output = tmp.output();
    write_site(&output, |cfg| {
        let mut options = Value::object();
        options.set("rawPathFilter", Value::function(|args| {
            Ok(Value::Bool(!args[0].to_output_string().ends_with("test.js")))
        }));
        cfg.add_passthrough_copy_mapping_with_options("test/stubs/template-passthrough/static", "static", options)?;
        Ok(())
    })
    .unwrap();
    assert!(Path::new(&output).join("static/test.css").exists());
    assert!(!Path::new(&output).join("static/test.js").exists());
}

/// "Passthrough copy `rename` outside of the output directory throws"
#[test]
fn passthrough_copy_rename_outside_of_the_output_directory_throws() {
    let tmp = TempOutput::new("passthrough-rename");
    let output = tmp.output();
    let err = write_site(&output, |cfg| {
        let mut options = Value::object();
        options.set("rename", Value::function(|args| Ok(Value::str(format!("../../{}", args[0].to_output_string())))));
        cfg.add_passthrough_copy_mapping_with_options("test/stubs/template-passthrough/src/views", "views", options)?;
        Ok(())
    })
    .unwrap_err();
    assert!(format!("{err:#}").contains("outside of the site output directory"), "{err:#}");
    let others: Vec<_> = std::fs::read_dir(&tmp.0)
        .unwrap()
        .flatten()
        .filter(|e| e.file_name() != "_site")
        .collect();
    assert!(others.is_empty());
}
