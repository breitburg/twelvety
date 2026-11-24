//! Port of `FileSystemSearch.js`: cached glob searches of the file system.

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};

use anyhow::{Result, bail};
use indexmap::IndexSet;
use walkdir::WalkDir;

use crate::util::glob_matcher::{
    IgnoreMatcher, build_glob_set, glob_base, has_negated_pattern, is_glob_match, strip_negation,
};
use crate::util::exists_cache::ExistsCache;
use crate::util::template_path;

/// A cached search: its globs, options, negated-ignore matcher and (insertion ordered) results
struct SearchEntry {
    input: Vec<String>,
    ignore: Vec<String>,
    ignore_matcher: Option<IgnoreMatcher>,
    results: IndexSet<String>,
}

#[derive(Clone, Default, Debug)]
pub struct SearchOptions {
    pub ignore: Vec<String>,
    pub git_ignore: Vec<String>,
}

#[derive(Default)]
pub struct FileSystemSearch {
    outputs: Mutex<HashMap<String, SearchEntry>>,
    /// Directory listings read during searches are shared with this cache
    exists_cache: Option<Arc<ExistsCache>>,
    pub allow_case_collisions: bool,
    pub count: Mutex<usize>,
}

fn check_case_collisions(paths: &IndexSet<String>) -> Result<()> {
    let mut seen: HashMap<String, &String> = HashMap::new();
    for path in paths {
        let key = path.to_lowercase();
        if let Some(prev) = seen.get(&key) {
            bail!(
                "File names that differ only by case can’t be used, because case-insensitive file systems (macOS, Windows) keep only one: \"{prev}\" and \"{path}\". Use `$config.configureErrorReporting({{ allowCaseCollisions: true }})` to allow this on case-sensitive file systems (Linux)."
            );
        }
        seen.insert(key, path);
    }
    Ok(())
}

impl FileSystemSearch {
    pub fn new() -> Self {
        Self::default()
    }

    /// Share the directory listings read by searches with an `ExistsCache`
    pub fn with_exists_cache(exists_cache: Arc<ExistsCache>) -> Self {
        Self {
            exists_cache: Some(exists_cache),
            ..Self::default()
        }
    }

    fn cache_key(key: &str, globs: &[String], options: &SearchOptions) -> String {
        let mut sorted = globs.to_vec();
        sorted.sort();
        format!("{key}{sorted:?}{:?}{:?}", options.ignore, options.git_ignore)
    }

    /// Search the file system for files matching `globs`
    pub fn search(&self, key: &str, globs: &[String], options: SearchOptions) -> Result<Vec<String>> {
        let globs: Vec<String> = globs
            .iter()
            .map(|g| template_path::strip_leading_dot_slash(g))
            .collect();
        let cache_key = Self::cache_key(key, &globs, &options);
        if let Some(entry) = self.outputs.lock().unwrap().get(&cache_key) {
            return Ok(entry.results.iter().cloned().collect());
        }
        *self.count.lock().unwrap() += 1;

        let mut ignores: Vec<String> = options
            .ignore
            .iter()
            .map(|e| template_path::strip_leading_dot_slash(&relative_to_project_root(e)))
            .collect();
        let mut ignore_matcher = None;
        if has_negated_pattern(&ignores) {
            let m = IgnoreMatcher::new(&ignores, &options.git_ignore);
            ignores = m.get_prunable_ignores();
            ignore_matcher = Some(m);
        }
        let ignore_set = build_glob_set(&ignores);

        let mut results = BTreeSet::new();
        let mut listings: HashMap<String, Vec<String>> = HashMap::new();
        // Group globs by base directory so we walk each tree once
        let mut positives: Vec<String> = vec![];
        let mut negatives: Vec<String> = vec![];
        for g in &globs {
            if let Some(n) = strip_negation(g) {
                negatives.push(n.to_string());
            } else {
                positives.push(g.clone());
            }
        }
        let positive_set = build_glob_set(&positives);
        let negative_set = build_glob_set(&negatives);

        let mut bases: BTreeSet<String> = BTreeSet::new();
        for g in &positives {
            let mut base = glob_base(g);
            if !crate::util::glob_matcher::is_dynamic_pattern(g) {
                // A literal path
                if std::path::Path::new(g).is_file() {
                    if !ignore_set.is_match(g) {
                        results.insert(template_path::standardize_file_path(g));
                    }
                    continue;
                }
                base = g.clone();
            }
            bases.insert(base);
        }
        // Remove bases nested in other bases
        let base_list: Vec<String> = bases.iter().cloned().collect();
        let bases: Vec<String> = base_list
            .iter()
            .filter(|b| {
                !base_list.iter().any(|other| {
                    other != *b && (other.is_empty() || b.starts_with(&format!("{other}/")))
                })
            })
            .cloned()
            .collect();

        for base in bases {
            let root = if base.is_empty() { ".".to_string() } else { base.clone() };
            if !std::path::Path::new(&root).is_dir() {
                continue;
            }
            let walker = WalkDir::new(&root).follow_links(true).into_iter();
            let walker = walker.filter_entry(|entry| {
                if entry.depth() == 0 {
                    return true;
                }
                let rel = relative_entry_path(&root, entry.path());
                if entry.file_type().is_dir() {
                    // prune ignored directories
                    let probe = format!("{rel}/__twelvety_probe__");
                    if ignore_set.is_match(&probe) || ignore_set.is_match(&rel) {
                        return false;
                    }
                }
                true
            });
            for entry in walker.flatten() {
                if self.exists_cache.is_some() {
                    if entry.file_type().is_dir() {
                        listings
                            .entry(entry.path().to_string_lossy().to_string())
                            .or_default();
                    }
                    if let Some(parent) = entry.path().parent().filter(|_| entry.depth() > 0) {
                        listings
                            .entry(parent.to_string_lossy().to_string())
                            .or_default()
                            .push(entry.file_name().to_string_lossy().to_string());
                    }
                }
                if !entry.file_type().is_file() {
                    continue;
                }
                let rel = relative_entry_path(&root, entry.path());
                if !positive_set.is_match(&rel) || negative_set.is_match(&rel) {
                    continue;
                }
                if ignore_set.is_match(&rel) {
                    continue;
                }
                if let Some(m) = &ignore_matcher {
                    if m.is_ignored(&rel, false) {
                        continue;
                    }
                }
                results.insert(template_path::standardize_file_path(&rel));
            }
        }

        if let Some(cache) = &self.exists_cache {
            for (dir, names) in listings {
                cache.add_listing(&dir, names);
            }
        }

        let results: IndexSet<String> = results.into_iter().collect();
        let allow_case_collisions = self.allow_case_collisions
            || self.exists_cache.as_ref().is_some_and(|c| c.allow_case_collisions);
        if !allow_case_collisions {
            check_case_collisions(&results)?;
        }

        let out: Vec<String> = results.iter().cloned().collect();
        self.outputs.lock().unwrap().insert(
            cache_key,
            SearchEntry {
                input: globs,
                ignore: ignores,
                ignore_matcher,
                results,
            },
        );
        Ok(out)
    }

    fn modify(&self, path: &str, add: bool) {
        let normalized = template_path::standardize_file_path(path);
        let mut outputs = self.outputs.lock().unwrap();
        for entry in outputs.values_mut() {
            if !is_glob_match(&normalized, &entry.input)
                || is_glob_match(&normalized, &entry.ignore)
                || entry
                    .ignore_matcher
                    .as_ref()
                    .is_some_and(|m| m.is_ignored(&normalized, false))
            {
                continue;
            }
            if add {
                entry.results.insert(normalized.clone());
            } else {
                entry.results.shift_remove(&normalized);
            }
        }
    }

    pub fn add(&self, path: &str) {
        self.modify(path, true);
    }

    pub fn delete(&self, path: &str) {
        self.modify(path, false);
    }

    pub fn clear(&self) {
        self.outputs.lock().unwrap().clear();
    }
}

/// Absolute ignore globs inside the project are made project-root relative (#403)
fn relative_to_project_root(glob: &str) -> String {
    let (negation, path) = match glob.strip_prefix('!') {
        Some(rest) => ("!", rest),
        None => ("", glob),
    };
    let cwd = format!("{}/", template_path::get_working_dir().trim_end_matches('/'));
    match path.strip_prefix(&cwd) {
        Some(rest) if path.starts_with('/') => format!("{negation}{rest}"),
        _ => glob.to_string(),
    }
}

/// Path of a walked entry relative to the project root (without `./`)
fn relative_entry_path(root: &str, path: &std::path::Path) -> String {
    let p = path.to_string_lossy().replace('\\', "/");
    let p = p.strip_prefix("./").unwrap_or(&p).to_string();
    if root == "." {
        return p;
    }
    p
}
