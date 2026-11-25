//! Port of `Util/ExistsCache.js`: cached file existence checks (case-aware).

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::{node_path, template_path};

#[derive(Default)]
pub struct ExistsCache {
    exists: Mutex<HashMap<String, bool>>,
    listings: Mutex<HashMap<String, Option<Vec<String>>>>,
    pub allow_case_collisions: bool,
    /// Number of file system reads (directory listings and directory checks)
    pub lookup_count: AtomicUsize,
}

fn normalize_dir(dir: &str) -> String {
    let mut dir = dir.to_string();
    if node_path::is_absolute(&dir) {
        dir = node_path::relative(&node_path::cwd(), &dir);
        if dir.is_empty() {
            dir = ".".into();
        }
    }
    let s = template_path::standardize_file_path(&dir);
    let s = s.trim_end_matches('/').to_string();
    if s.is_empty() { ".".into() } else { s }
}

impl ExistsCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&self) {
        self.exists.lock().unwrap().clear();
        self.listings.lock().unwrap().clear();
    }

    pub fn exists(&self, path: &str) -> bool {
        let key = template_path::add_leading_dot_slash(path);
        if let Some(v) = self.exists.lock().unwrap().get(&key) {
            return *v;
        }
        let exists = Path::new(path).exists();
        self.exists.lock().unwrap().insert(key, exists);
        exists
    }

    pub fn set(&self, path: &str, exists: bool) {
        self.exists
            .lock()
            .unwrap()
            .insert(template_path::add_leading_dot_slash(path), exists);
        self.listings.lock().unwrap().clear();
    }

    pub fn delete(&self, path: &str) {
        let mut exists = self.exists.lock().unwrap();
        exists.remove(path);
        exists.remove(&template_path::add_leading_dot_slash(path));
        self.listings.lock().unwrap().clear();
    }

    pub fn is_directory(&self, path: &str) -> bool {
        Path::new(path).is_dir()
    }

    fn listing(&self, dir: &str) -> Option<Vec<String>> {
        let key = normalize_dir(dir);
        if let Some(l) = self.listings.lock().unwrap().get(&key) {
            return l.clone();
        }
        self.lookup_count.fetch_add(1, Ordering::Relaxed);
        let entries = std::fs::read_dir(&key).ok().map(|rd| {
            rd.flatten()
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect::<Vec<_>>()
        });
        self.listings.lock().unwrap().insert(key, entries.clone());
        entries
    }

    /// Seed a directory listing (shared with glob searches)
    pub fn add_listing(&self, dir: &str, names: Vec<String>) {
        self.listings
            .lock()
            .unwrap()
            .insert(normalize_dir(dir), Some(names));
    }

    pub fn get_lookup_count(&self) -> usize {
        self.lookup_count.load(Ordering::Relaxed)
    }

    /// Resolve a path to its on-disk casing (case-insensitive lookup).
    /// Returns `None` if the path does not exist.
    pub fn resolve(&self, file_path: &str) -> Option<String> {
        let is_abs = file_path.starts_with('/');
        let mut dir = if is_abs {
            "/".to_string()
        } else {
            ".".to_string()
        };
        let mut out: Vec<String> = vec![];
        let had_dot = file_path.starts_with("./");
        for name in file_path.split('/') {
            if name.is_empty() || name == "." {
                continue;
            }
            if name == ".." {
                out.push("..".into());
                dir = format!("{dir}/..");
                continue;
            }
            let listing = self.listing(&dir)?;
            let matched = if listing.iter().any(|e| e == name) {
                name.to_string()
            } else {
                let lower = name.to_lowercase();
                let matches: Vec<&String> = listing
                    .iter()
                    .filter(|e| e.to_lowercase() == lower)
                    .collect();
                matches.first()?.to_string()
            };
            dir = if dir == "/" {
                format!("/{matched}")
            } else {
                format!("{dir}/{matched}")
            };
            out.push(matched);
        }
        let joined = out.join("/");
        let result = if is_abs {
            format!("/{joined}")
        } else if had_dot {
            format!("./{joined}")
        } else {
            joined
        };
        if file_path.ends_with('/') && !result.ends_with('/') {
            Some(format!("{result}/"))
        } else {
            Some(result)
        }
    }
}
