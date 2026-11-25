//! Port of `ExtensionMap.js`: maps file extensions to template engine keys.

use indexmap::{IndexMap, IndexSet};

use crate::user_config::ExtensionOptions;
use crate::util::template_path;

#[derive(Clone, Debug)]
pub struct ExtensionEntry {
    pub key: String,
    pub extension: String,
    pub alias_key: Option<String>,
    /// Index into `config.extension_map` for custom (addExtension) entries
    pub custom_index: Option<usize>,
    pub is_simple_alias: bool,
}

#[derive(Clone, Debug, Default)]
pub struct ExtensionMap {
    pub format_keys: Vec<String>,
    pub unfiltered_format_keys: Vec<String>,
    pub valid_template_language_keys: Vec<String>,
    pub passthrough_copy_keys: Vec<String>,
    extension_to_key_map: IndexMap<String, ExtensionEntry>,
}

impl ExtensionMap {
    pub fn new(custom: &[ExtensionOptions]) -> Self {
        let mut map: IndexMap<String, ExtensionEntry> = IndexMap::new();
        let builtin = [
            ("md", "md"),
            ("html", "html"),
            ("njk", "njk"),
            ("liquid", "liquid"),
            ("server.js", "12ty.js"),
            ("server.cjs", "12ty.js"),
            ("server.mjs", "12ty.js"),
            ("12ty.js", "12ty.js"),
            ("12ty.cjs", "12ty.js"),
            ("12ty.mjs", "12ty.js"),
        ];
        for (ext, key) in builtin {
            map.insert(
                ext.to_string(),
                ExtensionEntry {
                    key: key.to_string(),
                    extension: ext.to_string(),
                    alias_key: None,
                    custom_index: None,
                    is_simple_alias: false,
                },
            );
        }
        for (i, entry) in custom.iter().enumerate() {
            map.insert(
                entry.extension.clone(),
                ExtensionEntry {
                    key: entry.key.clone(),
                    extension: entry.extension.clone(),
                    alias_key: entry.alias_key.clone(),
                    custom_index: Some(i),
                    is_simple_alias: entry.is_simple_alias,
                },
            );
        }
        ExtensionMap {
            extension_to_key_map: map,
            ..Default::default()
        }
    }

    pub fn set_formats(&mut self, format_keys: &[String]) {
        self.format_keys = format_keys.to_vec();
        self.unfiltered_format_keys = format_keys.iter().map(|k| k.trim().to_lowercase()).collect();
        self.valid_template_language_keys = self
            .unfiltered_format_keys
            .iter()
            .filter(|k| self.has_extension(k))
            .cloned()
            .collect();
        self.passthrough_copy_keys = self
            .unfiltered_format_keys
            .iter()
            .filter(|k| !self.has_extension(k))
            .cloned()
            .collect();
    }

    pub fn get_file_list(&self, path: &str, dir: Option<&str>) -> Vec<String> {
        if path.is_empty() {
            return vec![];
        }
        let mut files = vec![];
        for key in &self.valid_template_language_keys {
            for extension in self.get_extensions_from_key(key) {
                files.push(format!(
                    "{}{}.{}",
                    dir.map(|d| format!("{d}/")).unwrap_or_default(),
                    path,
                    extension
                ));
            }
        }
        files
    }

    fn valid_template_extensions(&self) -> IndexSet<String> {
        self.valid_template_language_keys
            .iter()
            .flat_map(|k| self.get_extensions_from_key(k))
            .collect()
    }

    pub fn is_full_template_file_path(&self, path: &str) -> bool {
        let normalized = path.to_lowercase();
        self.valid_template_extensions()
            .iter()
            .any(|ext| normalized.ends_with(&format!(".{ext}")))
    }

    /// Valid template extensions matching a path, longest first (e.g. `12ty.js` then `js`)
    pub fn get_valid_extensions_for_path(&self, path: &str) -> Vec<String> {
        let normalized = path.to_lowercase();
        let valid = self.valid_template_extensions();
        let mut extensions: Vec<String> = self
            .extension_to_key_map
            .keys()
            .filter(|ext| normalized.ends_with(&format!(".{ext}")) && valid.contains(*ext))
            .cloned()
            .collect();
        extensions.sort_by(|a, b| b.len().cmp(&a.len()));
        extensions
    }

    pub fn get_passthrough_copy_globs(&self, input_dir: &str) -> Vec<String> {
        self.get_globs_for(&self.passthrough_copy_keys, input_dir)
    }

    pub fn get_valid_globs(&self, input_dir: &str) -> Vec<String> {
        self.get_globs_for(&self.valid_template_language_keys, input_dir)
    }

    pub fn get_globs(&self, input_dir: &str) -> Vec<String> {
        self.get_globs_for(&self.unfiltered_format_keys, input_dir)
    }

    fn get_globs_for(&self, format_keys: &[String], input_dir: &str) -> Vec<String> {
        let mut extensions: IndexSet<String> = IndexSet::new();
        for key in format_keys {
            if self.has_extension(key) {
                extensions.extend(self.get_extensions_from_key(key));
            } else {
                extensions.insert(key.clone());
            }
        }
        let dir = template_path::convert_to_recursive_glob_sync(input_dir);
        match extensions.len() {
            0 => vec![],
            1 => vec![format!("{dir}/*.{}", extensions[0])],
            _ => vec![format!(
                "{dir}/*.{{{}}}",
                extensions.iter().cloned().collect::<Vec<_>>().join(",")
            )],
        }
    }

    pub fn has_extension(&self, key: &str) -> bool {
        self.extension_to_key_map
            .values()
            .any(|e| e.key == key || e.alias_key.as_deref() == Some(key))
    }

    pub fn get_extensions_from_key(&self, key: &str) -> Vec<String> {
        let mut out = IndexSet::new();
        for (ext, entry) in &self.extension_to_key_map {
            if let Some(alias) = &entry.alias_key {
                if alias == key {
                    out.insert(ext.clone());
                }
            } else if entry.key == key {
                out.insert(ext.clone());
            }
        }
        out.into_iter().collect()
    }

    pub fn has_engine(&self, path_or_key: &str) -> bool {
        self.get_key(path_or_key).is_some()
    }

    fn matching_extension(&self, path_or_key: &str) -> Option<(&String, &ExtensionEntry)> {
        let lower = path_or_key.to_lowercase();
        // Prefer the longest matching extension (e.g. `12ty.js` over `js`)
        self.extension_to_key_map
            .iter()
            .filter(|(ext, _)| lower == **ext || lower.ends_with(&format!(".{ext}")))
            .max_by_key(|(ext, _)| ext.len())
    }

    pub fn get_key(&self, path_or_key: &str) -> Option<String> {
        let lower = path_or_key.to_lowercase();
        let mut candidates: Vec<(&String, &ExtensionEntry)> = self
            .extension_to_key_map
            .iter()
            .filter(|(ext, _)| lower == **ext || lower.ends_with(&format!(".{ext}")))
            .collect();
        candidates.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
        for (_, entry) in candidates {
            let key = entry.alias_key.clone().unwrap_or_else(|| entry.key.clone());
            if self.valid_template_language_keys.contains(&key) {
                return Some(key);
            }
        }
        None
    }

    pub fn get_extension_entry(&self, path_or_key: &str) -> Option<ExtensionEntry> {
        self.matching_extension(path_or_key).map(|(_, e)| e.clone())
    }

    pub fn remove_template_extension(&self, path: &str) -> String {
        match self.matching_extension(path) {
            Some((ext, _)) => {
                let cut = path.len().saturating_sub(1 + ext.len());
                path[..cut].to_string()
            }
            None => path.to_string(),
        }
    }

    pub fn get_readable_file_extensions(&self) -> String {
        self.extension_to_key_map
            .keys()
            .cloned()
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> ExtensionMap {
        let mut m = ExtensionMap::new(&[]);
        m.set_formats(&["liquid".into(), "md".into(), "njk".into(), "html".into(), "png".into()]);
        m
    }

    #[test]
    fn keys() {
        let m = map();
        assert_eq!(m.get_key("./test.md").as_deref(), Some("md"));
        assert_eq!(m.get_key("./test.12ty.js"), None);
        assert_eq!(m.remove_template_extension("test.njk"), "test");
        assert_eq!(m.remove_template_extension("test.12ty.js"), "test");
        assert_eq!(m.passthrough_copy_keys, vec!["png".to_string()]);
    }

    #[test]
    fn globs() {
        let m = map();
        assert_eq!(m.get_globs("."), vec!["./**/*.{liquid,md,njk,html,png}".to_string()]);
    }
}
