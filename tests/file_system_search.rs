//! Port of upstream `test/FileSystemSearchTest.js`.

use std::sync::Arc;

use twelvety::file_system_search::{FileSystemSearch, SearchOptions};
use twelvety::util::exists_cache::ExistsCache;

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn search(fs: &FileSystemSearch, key: &str, glob: &str) -> Vec<String> {
    fs.search(key, &strings(&[glob]), SearchOptions::default())
        .unwrap()
}

fn count(fs: &FileSystemSearch) -> usize {
    *fs.count.lock().unwrap()
}

const TXT: &str = "./test/file-system-search/*.txt";

/// "Base"
#[test]
fn base() {
    let fs = FileSystemSearch::new();
    assert_eq!(count(&fs), 0);

    assert_eq!(
        search(&fs, "key", TXT),
        strings(&["./test/file-system-search/file.txt"])
    );
    assert_eq!(count(&fs), 1);

    fs.add("./test/file-system-search/virtual-file.txt");
    assert_eq!(
        search(&fs, "key", TXT),
        strings(&[
            "./test/file-system-search/file.txt",
            "./test/file-system-search/virtual-file.txt"
        ])
    );
    assert_eq!(count(&fs), 1);

    fs.add("./test/file-system-search/another-file.txt");
    assert_eq!(
        search(&fs, "key", TXT),
        strings(&[
            "./test/file-system-search/file.txt",
            "./test/file-system-search/virtual-file.txt",
            "./test/file-system-search/another-file.txt",
        ])
    );
    assert_eq!(count(&fs), 1);

    fs.delete("./test/file-system-search/file.txt");
    assert_eq!(
        search(&fs, "key", TXT),
        strings(&[
            "./test/file-system-search/virtual-file.txt",
            "./test/file-system-search/another-file.txt"
        ])
    );
    assert_eq!(count(&fs), 1);

    fs.delete("./test/file-system-search/another-file.txt");
    assert_eq!(
        search(&fs, "key", TXT),
        strings(&["./test/file-system-search/virtual-file.txt"])
    );
    assert_eq!(count(&fs), 1);

    fs.delete("./test/file-system-search/virtual-file.txt");
    assert_eq!(search(&fs, "key", TXT), Vec::<String>::new());
    assert_eq!(count(&fs), 1);
}

// "New files with Windows paths are added to the cache": Windows only (skipped upstream on POSIX).

/// "Added paths use platform separators (backslashes on Windows)"
#[test]
fn added_paths_use_platform_separators() {
    let fs = FileSystemSearch::new();
    search(&fs, "key", TXT);

    fs.add("test/file-system-search/native-file.txt");
    assert_eq!(
        search(&fs, "key", TXT),
        strings(&[
            "./test/file-system-search/file.txt",
            "./test/file-system-search/native-file.txt"
        ])
    );

    fs.delete("./test/file-system-search/file.txt");
    assert_eq!(
        search(&fs, "key", TXT),
        strings(&["./test/file-system-search/native-file.txt"])
    );
}

// "Modifications during an in-flight search are applied": searches are synchronous in the port,
// so there is no in-flight state to test (skipped).

/// "Glob search seeds ExistsCache directory listings"
#[test]
fn glob_search_seeds_exists_cache_directory_listings() {
    let cache = Arc::new(ExistsCache::new());
    let fs = FileSystemSearch::with_exists_cache(cache.clone());
    fs.search(
        "templates",
        &strings(&["./test/_issues/4224/**/*.vZome"]),
        SearchOptions::default(),
    )
    .unwrap();

    assert!(cache.exists("./test/_issues/4224/sub/sub.JSON"));
    assert_eq!(
        cache
            .resolve("./test/_issues/4224/child.12tydata.json")
            .as_deref(),
        Some("./test/_issues/4224/child.12TYDATA.json")
    );
    // Only the folders above the glob’s starting folder are read
    assert_eq!(cache.get_lookup_count(), 3);
}

/// A temporary directory removed on drop
struct TempDir(std::path::PathBuf);

impl TempDir {
    fn new(prefix: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "{prefix}{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn case_collision_dir() -> Option<TempDir> {
    let dir = TempDir::new("ba-case-");
    std::fs::write(dir.path().join("index.md"), "").unwrap();
    std::fs::write(dir.path().join("INDEX.md"), "").unwrap();
    // Case-insensitive file system
    (std::fs::read_dir(dir.path()).unwrap().count() >= 2).then_some(dir)
}

/// "Glob search throws on file names that differ only by case #4224"
#[test]
fn glob_search_throws_on_file_names_that_differ_only_by_case_4224() {
    let Some(dir) = case_collision_dir() else {
        return;
    };
    let glob = format!("{}/*.md", dir.path().display());
    let err = FileSystemSearch::new()
        .search("x", &[glob], SearchOptions::default())
        .unwrap_err();
    assert!(err.to_string().contains("differ only by case"), "{err}");
}

/// "Glob search allows file names that differ only by case with allowCaseCollisions #4224"
#[test]
fn glob_search_allows_file_names_that_differ_only_by_case_with_allow_case_collisions_4224() {
    let Some(dir) = case_collision_dir() else {
        return;
    };
    let mut cache = ExistsCache::new();
    cache.allow_case_collisions = true;
    let glob = format!("{}/*.md", dir.path().display());
    let results = FileSystemSearch::with_exists_cache(Arc::new(cache))
        .search("x", &[glob], SearchOptions::default())
        .unwrap();
    assert_eq!(results.len(), 2);
}

fn negated_options(git_ignore: &[&str]) -> SearchOptions {
    SearchOptions {
        ignore: strings(&[
            "./test/stubs-4439/site/**",
            "!./test/stubs-4439/site/events/2018/**",
        ]),
        git_ignore: strings(git_ignore),
    }
}

/// "Negated ignores re-include files"
#[test]
fn negated_ignores_reinclude_files() {
    let fs = FileSystemSearch::new();
    let globs = strings(&["./test/stubs-4439/**/*.md"]);
    let mut results = fs.search("key", &globs, negated_options(&[])).unwrap();
    results.sort();
    assert_eq!(
        results,
        strings(&[
            "./test/stubs-4439/index.md",
            "./test/stubs-4439/site/events/2018/b.md"
        ])
    );

    fs.add("./test/stubs-4439/site/events/2018/virtual.md");
    fs.add("./test/stubs-4439/site/other/virtual.md");
    let mut results = fs.search("key", &globs, negated_options(&[])).unwrap();
    results.sort();
    assert_eq!(
        results,
        strings(&[
            "./test/stubs-4439/index.md",
            "./test/stubs-4439/site/events/2018/b.md",
            "./test/stubs-4439/site/events/2018/virtual.md",
        ])
    );
}

/// "Negated ignores can’t re-include files in a directory excluded by .gitignore"
#[test]
fn negated_ignores_cant_reinclude_files_in_a_directory_excluded_by_gitignore() {
    let fs = FileSystemSearch::new();
    let results = fs
        .search(
            "key",
            &strings(&["./test/stubs-4439/**/*.md"]),
            negated_options(&["./test/stubs-4439/site/**"]),
        )
        .unwrap();
    assert_eq!(results, strings(&["./test/stubs-4439/index.md"]));
}
