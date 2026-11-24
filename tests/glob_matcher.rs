//! Port of upstream `test/GlobMatcherTest.js`.

use twelvety::util::glob_matcher::{IgnoreMatcher, is_glob_match};

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn create_ignore_matcher(patterns: &[&str]) -> IgnoreMatcher {
    IgnoreMatcher::new(&strings(patterns), &[])
}

/// "[!...] bracket negation"
#[test]
fn bracket_negation() {
    assert!(is_glob_match("src/post.md", &strings(&["src/[!_]*.md"])));
    assert!(is_glob_match("./src/post.md", &strings(&["./src/[!_]*.md"])));
    assert!(!is_glob_match("src/_draft.md", &strings(&["src/[!_]*.md"])));
    assert!(!is_glob_match("src/_.md", &strings(&["src/[!_].md"])));
}

/// "createIgnoreMatcher: last match wins"
#[test]
fn create_ignore_matcher_last_match_wins() {
    let matcher = create_ignore_matcher(&["./site/**", "!./site/events/2018/**"]);
    assert!(matcher.is_ignored("./site/a.md", false));
    assert!(!matcher.is_ignored("site/events/2018/b.md", false));
    assert!(matcher.is_ignored("site/events/2019/c.md", false));
    assert!(!matcher.is_ignored("index.md", false));

    let reignored = create_ignore_matcher(&["site/**", "!site/events/**", "site/events/2019/**"]);
    assert!(!reignored.is_ignored("site/events/2018/b.md", false));
    assert!(reignored.is_ignored("site/events/2019/c.md", false));
}

/// "createIgnoreMatcher: directories holding re-included files are traversed"
#[test]
fn create_ignore_matcher_directories_holding_reincluded_files_are_traversed() {
    let matcher = create_ignore_matcher(&["site/**", "!site/events/2018/**"]);
    assert!(!matcher.is_ignored("site", true));
    assert!(!matcher.is_ignored("site/events", true));
    assert!(matcher.is_ignored("site/other", true));
    assert!(matcher.is_ignored("site/other", false));
}

/// "createIgnoreMatcher: getPrunableIgnores"
#[test]
fn create_ignore_matcher_get_prunable_ignores() {
    let matcher = create_ignore_matcher(&[
        "./node_modules/**",
        "site/**",
        "!site/events/2018/**",
        "**/node_modules/**",
    ]);
    assert_eq!(matcher.get_prunable_ignores(), strings(&["node_modules/**", "**/node_modules/**"]));
}

/// "createIgnoreMatcher: git patterns can’t re-include files in an excluded directory"
#[test]
fn create_ignore_matcher_git_patterns_cant_reinclude_files_in_an_excluded_directory() {
    let matcher = IgnoreMatcher::new(&strings(&["site/**", "!site/events/2018/**"]), &strings(&["site/**"]));
    assert!(matcher.is_ignored("site/events", true));
    assert!(matcher.is_ignored("site/events/2018/b.md", false));
    assert!(!matcher.is_ignored("index.md", false));
}

/// "createIgnoreMatcher: git patterns re-include directory by directory"
#[test]
fn create_ignore_matcher_git_patterns_reinclude_directory_by_directory() {
    let matcher = IgnoreMatcher::new(
        &strings(&["site/*", "!site/events", "site/events/*", "!site/events/2018"]),
        &strings(&["site/*", "site/events/*"]),
    );
    assert!(matcher.is_ignored("site/a.md", false));
    assert!(!matcher.is_ignored("site/events", true));
    assert!(!matcher.is_ignored("site/events/2018/b.md", false));
    assert!(matcher.is_ignored("site/events/2019/c.md", false));
    assert!(matcher.is_ignored("site/other/d.md", false));
}

/// Extglobs (from upstream `TemplateCollectionTest.js` "micromatch assumptions, issue #127")
#[test]
fn micromatch_assumptions_issue_127() {
    let glob = "**/+(bookmarks|posts|screencasts)/**/!(index)*.md";
    assert!(is_glob_match("src/bookmarks/test.md", &strings(&[glob])));
    assert!(is_glob_match(
        "./src/bookmarks/test.md",
        &strings(&["./**/+(bookmarks|posts|screencasts)/**/!(index)*.md"])
    ));
    let globs = strings(&["./**/+(bookmarks|posts|screencasts)/**/!(index)*.md"]);
    assert!(is_glob_match("./src/bookmarks/test.md", &globs));
    assert!(!is_glob_match("./src/bookmarks/index.md", &globs));
    assert!(!is_glob_match("./src/bookmarks/index2.md", &globs));
    assert!(is_glob_match("./src/_content/bookmarks/2018-03-27-git-message.md", &globs));
}

/// Other extglob forms (picomatch semantics)
#[test]
fn extglob_forms() {
    let m = |path: &str, glob: &str| is_glob_match(path, &strings(&[glob]));
    assert!(m("a/foo.md", "a/@(foo|bar).md"));
    assert!(!m("a/foofoo.md", "a/@(foo|bar).md"));
    assert!(m("a/.md", "a/?(foo|bar).md"));
    assert!(m("a/foo.md", "a/?(foo|bar).md"));
    assert!(!m("a/foobar.md", "a/?(foo|bar).md"));
    assert!(m("a/foobar.md", "a/+(foo|bar).md"));
    assert!(!m("a/.md", "a/+(foo|bar).md"));
    assert!(m("a/.md", "a/*(foo|bar).md"));
    assert!(m("a/barfoobar.md", "a/*(foo|bar).md"));
    assert!(m("glob-pages/about.md", "glob-pages/!(contact.md)"));
    assert!(!m("glob-pages/contact.md", "glob-pages/!(contact.md)"));
    assert!(!m("glob-pages/sub/about.md", "glob-pages/!(contact.md)"));
    assert!(m("a/b/c/x.MD", "a/**/!(y)*.md"));
}
