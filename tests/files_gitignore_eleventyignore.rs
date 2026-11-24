//! Port of upstream `test/FilesGitIgnoreEleventyIgnoreTest.js`.

use std::sync::Arc;

use twelvety::extension_map::ExtensionMap;
use twelvety::file_system_search::FileSystemSearch;
use twelvety::files::{Files, NEGATED_IGNORES_FLAG};
use twelvety::template_config::TemplateConfig;
use twelvety::testing::quiet_logger;
use twelvety::user_config::UserConfig;
use twelvety::util::glob_matcher::IgnoreMatcher;
use twelvety::util::project_directories::{ConfigDirs, ProjectDirectories};
use twelvety::util::template_path;

fn s(v: &str) -> Option<String> {
    Some(v.to_string())
}

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn dirs(input: &str, output: &str) -> ConfigDirs {
    ConfigDirs { input: s(input), output: s(output), ..Default::default() }
}

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

fn get_files(formats: &[&str], tc: &TemplateConfig) -> Files {
    let config = tc.get_config();
    let mut map = ExtensionMap::new(&config.extension_map);
    map.set_formats(&strings(formats));
    Files::new(config, tc.directories.clone(), Arc::new(map), Arc::new(FileSystemSearch::new()))
}

/// `evf._setLocalPathRoot(root)` then `getIgnores()`/`getIgnoreGlobs()`
fn with_local_root(dirs: ConfigDirs, use_git_ignore: bool, root: &str) -> Files {
    let tc = get_template_config(dirs, |cfg| {
        if !use_git_ignore {
            cfg.set_use_git_ignore(false);
        }
    });
    let mut evf = get_files(&[], &tc);
    evf.local_path_root = s(root);
    evf.file_ignores = evf.get_ignores();
    evf
}

fn last_two(v: Vec<String>) -> Vec<String> {
    v[v.len().saturating_sub(2)..].to_vec()
}

fn check(dirs: ConfigDirs, use_git_ignore: bool, root: &str, ignores: &[&str]) -> Files {
    let mut evf = with_local_root(dirs, use_git_ignore, root);
    assert_eq!(evf.get_ignores(), strings(ignores));
    assert_eq!(
        last_two(evf.get_ignore_globs()),
        strings(&["**/node_modules/**", &format!("{root}/.git/**")])
    );
    evf
}

/// "Get ignores (no .eleventyignore no .gitignore)"
#[test]
fn get_ignores_no_eleventyignore_no_gitignore() {
    check(
        dirs("test/stubs/ignore1", "test/stubs/ignore1/_site"),
        true,
        "./test/stubs/ignorelocalroot",
        &["./test/stubs/ignorelocalroot/test.md", "./test/stubs/ignore1/_site/**"],
    );
}

/// "Get ignores (no .eleventyignore)"
#[test]
fn get_ignores_no_eleventyignore() {
    let evf = check(
        dirs("test/stubs/ignore2", "test/stubs/ignore2/_site"),
        true,
        "./test/stubs/ignorelocalrootgitignore",
        &[
            "./test/stubs/ignorelocalrootgitignore/thisshouldnotexist12345",
            "./test/stubs/ignorelocalrootgitignore/test.md",
            "./test/stubs/ignore2/_site/**",
        ],
    );
    assert_eq!(evf.git_ignores, strings(&["./test/stubs/ignorelocalrootgitignore/thisshouldnotexist12345"]));
}

/// "Get ignores (no .eleventyignore, using setUseGitIgnore(false))"
#[test]
fn get_ignores_no_eleventyignore_using_set_use_git_ignore_false() {
    check(
        dirs("test/stubs/ignore2", "test/stubs/ignore2/_site"),
        false,
        "./test/stubs/ignorelocalroot",
        &["./test/stubs/ignorelocalroot/test.md", "./test/stubs/ignore2/_site/**"],
    );
}

/// "Get ignores (no .gitignore)"
#[test]
fn get_ignores_no_gitignore() {
    check(
        dirs("test/stubs/ignore3", "test/stubs/ignore3/_site"),
        true,
        "./test/stubs/ignorelocalroot",
        &[
            "./test/stubs/ignorelocalroot/test.md",
            "./test/stubs/ignore3/ignoredFolder/**",
            "./test/stubs/ignore3/ignoredFolder/ignored.md",
            "./test/stubs/ignore3/_site/**",
        ],
    );
}

/// "Get ignores (project .eleventyignore and root .gitignore)"
#[test]
fn get_ignores_project_eleventyignore_and_root_gitignore() {
    check(
        dirs("test/stubs/ignore4", "test/stubs/ignore4/_site"),
        true,
        "./test/stubs/ignorelocalrootgitignore",
        &[
            "./test/stubs/ignorelocalrootgitignore/thisshouldnotexist12345",
            "./test/stubs/ignorelocalrootgitignore/test.md",
            "./test/stubs/ignore4/ignoredFolder/**",
            "./test/stubs/ignore4/ignoredFolder/ignored.md",
            "./test/stubs/ignore4/_site/**",
        ],
    );
}

/// "Get ignores (project .eleventyignore and root .gitignore, using setUseGitIgnore(false))"
#[test]
fn get_ignores_project_eleventyignore_and_root_gitignore_using_set_use_git_ignore_false() {
    check(
        dirs("test/stubs/ignore4", "test/stubs/ignore4/_site"),
        false,
        "./test/stubs/ignorelocalrootgitignore",
        &[
            "./test/stubs/ignorelocalrootgitignore/test.md",
            "./test/stubs/ignore4/ignoredFolder/**",
            "./test/stubs/ignore4/ignoredFolder/ignored.md",
            "./test/stubs/ignore4/_site/**",
        ],
    );
}

/// "Get ignores (no .eleventyignore  .gitignore exists but empty)"
#[test]
fn get_ignores_no_eleventyignore_gitignore_exists_but_empty() {
    check(
        dirs("test/stubs/ignore5", "test/stubs/ignore5/_site"),
        true,
        "./test/stubs/ignorelocalroot",
        &["./test/stubs/ignorelocalroot/test.md", "./test/stubs/ignore5/_site/**"],
    );
}

/// "Get ignores (both .eleventyignore and .gitignore exists, but .gitignore is empty)"
#[test]
fn get_ignores_both_eleventyignore_and_gitignore_exists_but_gitignore_is_empty() {
    check(
        dirs("test/stubs/ignore6", "test/stubs/ignore6/_site"),
        true,
        "./test/stubs/ignorelocalroot",
        &[
            "./test/stubs/ignorelocalroot/test.md",
            "./test/stubs/ignore6/ignoredFolder/**",
            "./test/stubs/ignore6/ignoredFolder/ignored.md",
            "./test/stubs/ignore6/_site/**",
        ],
    );
}

fn stubs_403_files() -> Files {
    let tc = get_template_config(
        ConfigDirs {
            input: s("test/stubs-403"),
            output: s("_site"),
            includes: s(""),
            data: s(""),
            ..Default::default()
        },
        |cfg| cfg.set_use_git_ignore(false),
    );
    get_files(&["liquid"], &tc)
}

/// "Bad expected output, this indicates a bug upstream in a dependency (update, was fixed in
/// fast-glob@3.3.3). Input to 'src' and empty includes dir (issue #403, full paths in eleventyignore)"
#[test]
fn input_to_src_and_empty_includes_dir_issue_403_full_paths_in_eleventyignore() {
    let mut evf = stubs_403_files();
    evf.eleventy_ignore_content = Some(format!("{}/**", template_path::absolute_path(&["test/stubs-403/_includes"])));
    evf.init();
    assert_eq!(evf.get_files().unwrap(), strings(&["./test/stubs-403/template.liquid"]));
}

/// "Workaround for Bad expected output, this indicates a bug upstream in a dependency. Input to
/// 'src' and empty includes dir (issue #403, full paths in eleventyignore)"
#[test]
fn workaround_input_to_src_and_empty_includes_dir_issue_403() {
    let mut evf = stubs_403_files();
    evf.eleventy_ignore_content = s("./test/stubs-403/_includes/**");
    evf.init();
    assert_eq!(evf.get_files().unwrap(), strings(&["./test/stubs-403/template.liquid"]));
}

/// "Issue #403: all .eleventyignores should be relative paths not absolute paths"
#[test]
fn issue_403_all_eleventyignores_should_be_relative_paths() {
    let evf = stubs_403_files();
    let abs = template_path::absolute_path(&[]);
    assert!(!evf.get_file_globs().iter().any(|g| g.contains(&abs)));
}

/// "Same input and output directories, issues #186 and #1129"
#[test]
fn same_input_and_output_directories_issues_186_and_1129() {
    let tc = get_template_config(dirs("test/stubs", ""), |cfg| cfg.set_use_git_ignore(false));
    let mut evf = get_files(&[], &tc);
    assert!(!evf.get_ignores().iter().any(|e| e.contains("_site")));
}

/// "Single input file is in the output directory, issues #186"
#[test]
fn single_input_file_is_in_the_output_directory_issues_186() {
    let tc = get_template_config(
        ConfigDirs { input: s("test/stubs"), output: s(""), includes: s(""), ..Default::default() },
        |cfg| cfg.set_use_git_ignore(false),
    );
    let mut evf = get_files(&["njk"], &tc);
    assert!(!evf.get_ignores().iter().any(|e| e.contains("_site")));
}

/// "De-duplicated ignores"
#[test]
fn deduplicated_ignores() {
    let evf = check(
        dirs("test/stubs/ignore-dedupe", "test/stubs/ignore-dedupe/_site"),
        true,
        "./test/stubs/ignore-dedupe",
        &["./test/stubs/ignore-dedupe/ignoredFolder", "./test/stubs/ignore-dedupe/_site/**"],
    );
    assert_eq!(
        evf.get_glob_watcher_files(),
        strings(&["./test/stubs/ignore-dedupe/_includes/**", "./test/stubs/ignore-dedupe/_data/**"])
    );
}

/// "Negated .eleventyignore patterns are skipped without the experimental flag"
#[test]
fn negated_eleventyignore_patterns_are_skipped_without_the_experimental_flag() {
    let tc = get_template_config(dirs("test/stubs-4439", "test/stubs-4439/_site"), |_| {});
    let mut evf = get_files(&["md"], &tc);
    evf.init();
    let mut files = evf.get_files().unwrap();
    files.sort();
    assert_eq!(files, strings(&["./test/stubs-4439/index.md"]));
}

/// "Negated .eleventyignore patterns re-include files"
#[test]
fn negated_eleventyignore_patterns_reinclude_files() {
    let tc = get_template_config(dirs("test/stubs-4439", "test/stubs-4439/_site"), |cfg| {
        cfg.add_experimental_behavior(NEGATED_IGNORES_FLAG);
    });
    let mut evf = get_files(&["md"], &tc);
    evf.init();
    let mut files = evf.get_files().unwrap();
    files.sort();
    assert_eq!(files, strings(&["./test/stubs-4439/index.md", "./test/stubs-4439/site/events/2018/b.md"]));

    // `toWatcherIgnores`: negated project ignores become one ordered matcher
    let matcher = IgnoreMatcher::new(&evf.get_glob_watcher_project_ignores(), &evf.git_ignores);
    assert!(!matcher.is_ignored("test/stubs-4439/site/events", true));
    assert!(matcher.is_ignored("test/stubs-4439/site/other", true));
    assert!(!matcher.is_ignored("test/stubs-4439/site/events/2018/b.md", false));
    assert!(matcher.is_ignored("test/stubs-4439/site/events/2019/c.md", false));
}
