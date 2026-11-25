//! Port of upstream `test/FilesTest.js`.

use std::sync::Arc;

use twelvety::extension_map::ExtensionMap;
use twelvety::file_system_search::{FileSystemSearch, SearchOptions};
use twelvety::files::{Files, NEGATED_IGNORES_FLAG};
use twelvety::template_config::TemplateConfig;
use twelvety::template_passthrough::TemplatePassthroughManager;
use twelvety::testing::quiet_logger;
use twelvety::user_config::UserConfig;
use twelvety::util::project_directories::{ConfigDirs, ProjectDirectories};

fn s(v: &str) -> Option<String> {
    Some(v.to_string())
}

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn input_dirs(input: &str) -> ConfigDirs {
    ConfigDirs { input: s(input), ..Default::default() }
}

/// `getTemplateConfigInstance` (+ an optional configuration callback)
fn get_template_config(dirs: ConfigDirs, callback: impl Fn(&mut UserConfig)) -> TemplateConfig {
    let mut tc = TemplateConfig::new(Some(""));
    tc.set_logger(quiet_logger());
    callback(&mut tc.user_config);
    let mut d = ProjectDirectories::new();
    d.set_via_config_object(&dirs).unwrap();
    tc.set_directories(d);
    tc.init().unwrap();
    tc
}

/// `getTwelvetyFilesInstance`
fn get_files(formats: &[&str], tc: &TemplateConfig) -> Files {
    let config = tc.get_config();
    let mut map = ExtensionMap::new(&config.extension_map);
    map.set_formats(&strings(formats));
    Files::new(config, tc.directories.clone(), Arc::new(map), Arc::new(FileSystemSearch::new()))
}

fn files_for(input: &str, formats: &[&str]) -> Files {
    get_files(formats, &get_template_config(input_dirs(input), |_| {}))
}

/// `glob(evf.getFileGlobs(), { ignore })`
fn glob(globs: &[String], ignore: &[String]) -> Vec<String> {
    FileSystemSearch::new()
        .search("test", globs, SearchOptions { ignore: ignore.to_vec(), git_ignore: vec![] })
        .unwrap()
}

/// "Dirs paths"
#[test]
fn dirs_paths() {
    let tc = get_template_config(
        ConfigDirs {
            input: s("src"),
            includes: s("includes"),
            data: s("data"),
            output: s("dist"),
            ..Default::default()
        },
        |_| {},
    );
    let evf = get_files(&[], &tc);
    assert_eq!(evf.dirs().input(), "./src/");
    assert_eq!(evf.dirs().includes(), "./src/includes/");
    assert_eq!(evf.dirs().data(), "./src/data/");
    assert_eq!(evf.dirs().output(), "./dist/");
}

/// "Dirs paths (relative)"
#[test]
fn dirs_paths_relative() {
    let tc = get_template_config(
        ConfigDirs {
            input: s("src"),
            includes: s("../includes"),
            data: s("../data"),
            output: s("dist"),
            ..Default::default()
        },
        |_| {},
    );
    let evf = get_files(&[], &tc);
    assert_eq!(evf.dirs().input(), "./src/");
    assert_eq!(evf.dirs().includes(), "./includes/");
    assert_eq!(evf.dirs().data(), "./data/");
    assert_eq!(evf.dirs().output(), "./dist/");
}

/// "getFiles"
#[test]
fn get_files_test() {
    let evf = files_for("./test/stubs/writeTest", &["liquid", "md"]);
    assert_eq!(evf.get_files().unwrap(), strings(&["./test/stubs/writeTest/test.md"]));
}

/// "getFiles (without 12ty.js)"
#[test]
fn get_files_without_12ty_js() {
    let evf = files_for("./test/stubs/writeTestJS", &["liquid", "md"]);
    assert_eq!(evf.get_files().unwrap(), Vec::<String>::new());
}

// "getFiles (with 12ty.js)", "getFiles (with js, treated as passthrough copy)" and
// "getFiles (with case insensitivity)": need the `12ty.js` template language (skipped, JS-only).

/// "Mutually exclusive Input and Output dirs"
#[test]
fn mutually_exclusive_input_and_output_dirs() {
    let evf = files_for("./test/stubs/writeTest", &["liquid", "md"]);
    let files = glob(evf.get_file_globs(), &[]);
    assert_eq!(evf.get_file_globs(), strings(&["./test/stubs/writeTest/**/*.{liquid,md}"]));
    assert!(!files.is_empty());
    assert_eq!(files[0], "./test/stubs/writeTest/test.md");
}

/// "Single File Input (deep path)"
#[test]
fn single_file_input_deep_path() {
    let evf = files_for("./test/stubs/index.html", &["liquid", "md"]);
    let files = glob(evf.get_file_globs(), &[]);
    assert_eq!(evf.get_file_globs().len(), 1);
    assert_eq!(files, strings(&["./test/stubs/index.html"]));
}

// "Single File Input (shallow path)": relies on the upstream project’s README.md (skipped).

/// "Glob Input"
#[test]
fn glob_input() {
    let evf = files_for("./test/stubs/glob-pages/!(contact.md)", &["md"]);
    let files = glob(evf.get_file_globs(), &[]);
    assert_eq!(files, strings(&["./test/stubs/glob-pages/about.md", "./test/stubs/glob-pages/home.md"]));
}

/// ".twelvetyignore parsing"
#[test]
fn twelvetyignore_parsing() {
    let evf = files_for("./test/stubs/", &["md"]);
    let ignores = evf.get_file_ignores("./test/stubs/.twelvetyignore");
    assert_eq!(ignores, strings(&["./test/stubs/ignoredFolder/**", "./test/stubs/ignoredFolder/ignored.md"]));
}

/// "Parse multiple .twelvetyignores"
#[test]
fn parse_multiple_twelvetyignores() {
    let evf = files_for("./test/stubs/multiple-ignores/", &["md"]);
    let ignores: Vec<String> = [
        "./test/stubs/multiple-ignores/.twelvetyignore",
        "./test/stubs/multiple-ignores/subfolder/.twelvetyignore",
    ]
    .iter()
    .flat_map(|p| evf.get_file_ignores(p))
    .collect();
    assert_eq!(
        ignores,
        strings(&[
            "./test/stubs/multiple-ignores/ignoredFolder/**",
            "./test/stubs/multiple-ignores/ignoredFolder/ignored.md",
            "./test/stubs/multiple-ignores/subfolder/ignoredFolder2/**",
            "./test/stubs/multiple-ignores/subfolder/ignoredFolder2/ignored2.md",
        ])
    );
}

/// "Passed file name does not exist"
#[test]
fn passed_file_name_does_not_exist() {
    let evf = files_for("./", &["md"]);
    assert_eq!(evf.get_file_ignores(".thisfiledoesnotexist"), Vec::<String>::new());
}

/// ".twelvetyignore files"
#[test]
fn twelvetyignore_files() {
    let evf = files_for("test/stubs", &["liquid", "md"]);
    let ignored = glob(&strings(&["test/stubs/ignoredFolder/*.md"]), &[]);
    assert_eq!(ignored.len(), 1);
    let files = glob(evf.get_file_globs(), &evf.unique_ignores);
    assert!(!files.is_empty());
    assert!(!files.iter().any(|f| f.contains("./test/stubs/ignoredFolder")));
}

// "getTemplateData caching": template data is not owned by `Files` in the port (skipped).

/// "getDataDir"
#[test]
fn get_data_dir() {
    assert_eq!(files_for(".", &[]).dirs().data(), "./_data/");
}

/// "getDataDir subdir"
#[test]
fn get_data_dir_subdir() {
    assert_eq!(files_for("test/stubs", &[]).dirs().data(), "./test/stubs/_data/");
}

/// "Include and Data Dirs"
#[test]
fn include_and_data_dirs() {
    let evf = files_for("test/stubs", &[]);
    assert_eq!(
        evf.get_includes_and_data_dirs(),
        strings(&["./test/stubs/_includes/**", "./test/stubs/_data/**"])
    );
}

/// "Input to 'src' and empty includes dir (issue #403)"
#[test]
fn input_to_src_and_empty_includes_dir_issue_403() {
    let mut evf = files_for("src", &["md", "liquid", "html"]);
    evf.twelvety_ignore_content = s("!./src/_includes/**");
    evf.init(); // duplicate init
    assert_eq!(evf.get_file_globs(), strings(&["./src/**/*.{md,liquid,html}"]));
}

/// "Glob Watcher Files"
#[test]
fn glob_watcher_files() {
    let evf = files_for("test/stubs", &["njk"]);
    assert_eq!(
        evf.get_glob_watcher_files(),
        strings(&["./test/stubs/**/*.njk", "./test/stubs/_includes/**", "./test/stubs/_data/**"])
    );
}

/// "Glob Watcher Files with File Extension Passthroughs"
#[test]
fn glob_watcher_files_with_file_extension_passthroughs() {
    let evf = files_for("test/stubs", &["njk", "png"]);
    assert_eq!(
        evf.get_glob_watcher_files(),
        strings(&[
            "./test/stubs/**/*.njk",
            "./test/stubs/**/*.png",
            "./test/stubs/_includes/**",
            "./test/stubs/_data/**",
        ])
    );
}

// The three "… with Dev Server (for free passthrough copy #2456)" / "… can’t be emulated" tests:
// the port’s dev server does not emulate passthrough copy (`serverPassthroughCopyBehavior`), so
// there are no separate passthrough watcher globs (skipped).

/// "Glob Watcher Files with Config Passthroughs (one template format)"
#[test]
fn glob_watcher_files_with_config_passthroughs_one_template_format() {
    let tc = get_template_config(
        ConfigDirs { input: s("test/stubs"), output: s("test/stubs/_site"), ..Default::default() },
        |cfg| {
            cfg.add_passthrough_copy("test/stubs/img/");
        },
    );
    let evf = get_files(&["njk"], &tc);
    assert_eq!(
        evf.get_glob_watcher_files(),
        strings(&["./test/stubs/**/*.njk", "./test/stubs/_includes/**", "./test/stubs/_data/**"])
    );
    let mgr = TemplatePassthroughManager::new(
        tc.get_config(),
        tc.directories.clone(),
        Arc::new(FileSystemSearch::new()),
        tc.exists_cache.clone(),
        quiet_logger(),
    );
    assert_eq!(mgr.get_config_path_globs(), strings(&["./test/stubs/img/**"]));
}

// "Glob Watcher Files with Config Passthroughs (no template formats)": the template data file glob
// lives in `TemplateData` (with JS data file extensions upstream); skipped.

/// "Negations are skipped in ignore content without the experimental flag"
#[test]
fn negations_are_skipped_in_ignore_content_without_the_experimental_flag() {
    let evf = files_for("test/stubs", &[]);
    assert_eq!(evf.normalize_ignore_content("./", "hello\n!testing"), strings(&["./hello"]));
}

/// "Negations are kept in ignore content with the experimental flag"
#[test]
fn negations_are_kept_in_ignore_content_with_the_experimental_flag() {
    let tc = get_template_config(input_dirs("test/stubs"), |cfg| {
        cfg.add_experimental_behavior(NEGATED_IGNORES_FLAG);
    });
    let evf = get_files(&[], &tc);
    assert_eq!(evf.normalize_ignore_content("./", "hello\n!testing"), strings(&["./hello", "!./testing"]));
}
