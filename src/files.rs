//! Port of `Files.js`: finds template files (respecting ignore files).

use std::sync::Arc;

use anyhow::Result;
use indexmap::IndexSet;

use crate::extension_map::ExtensionMap;
use crate::file_system_search::{FileSystemSearch, SearchOptions};
use crate::template_config::Config;
use crate::util::dir_contains::dir_contains;
use crate::util::project_directories::ProjectDirectories;
use crate::util::template_path;
use crate::value::Value;

pub const NEGATED_IGNORES_FLAG: &str = "NEGATED_IGNORES";

fn normalize_glob_path(paths: &[&str]) -> String {
    template_path::add_leading_dot_slash(&template_path::join(paths))
}

/// `TemplateGlob.normalize`
fn template_glob_normalize(path: &str) -> String {
    let path = path.trim();
    if let Some(rest) = path.strip_prefix('!') {
        format!("!{}", normalize_glob_path(&[rest]))
    } else {
        normalize_glob_path(&[path])
    }
}

pub struct Files {
    config: Arc<Config>,
    dirs: ProjectDirectories,
    extension_map: Arc<ExtensionMap>,
    search: Arc<FileSystemSearch>,
    pub template_globs: Vec<String>,
    pub file_ignores: Vec<String>,
    pub git_ignores: Vec<String>,
    pub unique_ignores: Vec<String>,
    pub eleventy_ignore_content: Option<String>,
    pub local_path_root: Option<String>,
}

impl Files {
    pub fn new(
        config: Arc<Config>,
        dirs: ProjectDirectories,
        extension_map: Arc<ExtensionMap>,
        search: Arc<FileSystemSearch>,
    ) -> Self {
        let mut f = Files {
            config,
            dirs,
            extension_map,
            search,
            template_globs: vec![],
            file_ignores: vec![],
            git_ignores: vec![],
            unique_ignores: vec![],
            eleventy_ignore_content: None,
            local_path_root: None,
        };
        f.init();
        f
    }

    pub fn init(&mut self) {
        if let Some(file) = self.dirs.input_file.clone().or_else(|| self.dirs.input_glob.clone()) {
            self.template_globs = vec![template_glob_normalize(&file)];
        } else {
            self.template_globs = self.extension_map.get_globs(&self.dirs.input());
        }
        self.setup_globs();
    }

    fn setup_globs(&mut self) {
        self.file_ignores = self.get_ignores();
        self.unique_ignores = self.get_ignore_globs();
        let _ = self.config.events.emit(
            "buildawesome.ignores",
            &Value::array(self.unique_ignores.iter().map(Value::from).collect()),
        );
    }

    /// File ignores, then includes/data directories, then configured ignores (in that order)
    pub fn get_ignore_globs(&self) -> Vec<String> {
        let mut unique: IndexSet<String> = IndexSet::new();
        unique.extend(self.file_ignores.iter().cloned());
        unique.extend(self.get_includes_and_data_dirs());
        for ignore in &self.config.ignores {
            unique.insert(self.normalize_ignore_entry(ignore));
        }
        unique.into_iter().collect()
    }

    fn normalize_ignore_entry(&self, entry: &str) -> String {
        if let Some(rest) = entry.strip_prefix('!') {
            return format!("!{}", self.normalize_ignore_entry(rest));
        }
        if !entry.starts_with("**/") {
            return normalize_glob_path(&[self.local_path_root.as_deref().unwrap_or("."), entry]);
        }
        entry.to_string()
    }

    pub fn normalize_ignore_content(&self, dir: &str, content: &str) -> Vec<String> {
        let allow_negations = self
            .config
            .experimental_flags
            .iter()
            .any(|f| f == NEGATED_IGNORES_FLAG);
        content
            .lines()
            .map(|l| l.trim())
            .filter(|l| {
                if l.starts_with('!') && !allow_negations {
                    return false;
                }
                !l.is_empty() && !l.starts_with('#')
            })
            .map(|line| {
                let negation = if line.starts_with('!') { "!" } else { "" };
                let path = normalize_glob_path(&[dir, "/", &line[negation.len()..]]);
                let path = template_path::add_leading_dot_slash(&template_path::relative_path(
                    &template_path::absolute_path(&[&path]),
                ));
                if std::path::Path::new(&path).is_dir() {
                    format!("{negation}{path}/**")
                } else {
                    format!("{negation}{path}")
                }
            })
            .collect()
    }

    pub fn get_file_ignores(&self, ignore_path: &str) -> Vec<String> {
        let ignore_path = template_path::normalize(ignore_path);
        match std::fs::read_to_string(&ignore_path) {
            Ok(content) if !content.is_empty() => {
                let dir = template_path::get_dir_from_file_path(&ignore_path);
                self.normalize_ignore_content(&dir, &content)
            }
            _ => vec![],
        }
    }

    pub fn get_ignore_files(&self) -> Vec<String> {
        let mut files: IndexSet<String> = IndexSet::new();
        let root = self.local_path_root.clone().unwrap_or_else(|| ".".into());
        if self.config.use_git_ignore {
            files.insert(template_path::join(&[&root, ".gitignore"]));
        }
        if self.eleventy_ignore_content.is_none() {
            let absolute_input = template_path::absolute_path(&[&self.dirs.input()]);
            files.insert(template_path::join(&[&root, ".eleventyignore"]));
            if root != absolute_input {
                files.insert(template_path::join(&[&self.dirs.input(), ".eleventyignore"]));
            }
        }
        files.into_iter().collect()
    }

    pub fn get_ignores(&mut self) -> Vec<String> {
        let mut files: IndexSet<String> = IndexSet::new();
        let mut git: Vec<String> = vec![];
        for ignore_file in self.get_ignore_files() {
            let is_git = template_path::get_last_path_segment(&ignore_file) == ".gitignore";
            for ignore in self.get_file_ignores(&ignore_file) {
                if is_git {
                    git.push(ignore.clone());
                }
                files.insert(ignore);
            }
        }
        self.git_ignores = git;
        if let Some(c) = &self.eleventy_ignore_content {
            files.insert(c.clone());
        }
        if let Some(o) = self.get_output_dir_ignore() {
            files.insert(o);
        }
        files.into_iter().collect()
    }

    fn get_output_dir_ignore(&self) -> Option<String> {
        if !dir_contains(&self.dirs.output(), &self.dirs.input()) {
            return Some(template_glob_normalize(&format!("{}/**", self.dirs.output())));
        }
        None
    }

    pub fn get_includes_and_data_dirs(&self) -> Vec<String> {
        let mut raw: IndexSet<String> = IndexSet::new();
        raw.insert(self.dirs.includes());
        if let Some(l) = self.dirs.layouts() {
            raw.insert(l);
        }
        raw.insert(self.dirs.data());
        let input = self.dirs.input();
        raw.into_iter()
            .filter(|e| !e.is_empty() && *e != input)
            .map(|e| template_glob_normalize(&format!("{e}**")))
            .collect()
    }

    pub fn dirs(&self) -> &ProjectDirectories {
        &self.dirs
    }

    pub fn extension_map(&self) -> &Arc<ExtensionMap> {
        &self.extension_map
    }

    pub fn get_file_globs(&self) -> &[String] {
        &self.template_globs
    }

    pub fn get_files(&self) -> Result<Vec<String>> {
        let results = self.search.search(
            "templates",
            &self.template_globs,
            SearchOptions {
                ignore: self.unique_ignores.clone(),
                git_ignore: self.git_ignores.clone(),
            },
        )?;
        Ok(template_path::add_leading_dot_slash_array(&results))
    }

    /// Globs to watch for template changes
    pub fn get_glob_watcher_files(&self) -> Vec<String> {
        let mut globs = if self.dirs.input_file.is_some() {
            self.template_globs.clone()
        } else {
            self.extension_map.get_valid_globs(&self.dirs.input())
        };
        globs.extend(self.extension_map.get_passthrough_copy_globs(&self.dirs.input()));
        globs.extend(self.get_includes_and_data_dirs());
        globs
    }

    /// Ignored by the watcher for everything except `addPassthroughCopy` sources
    pub fn get_glob_watcher_project_ignores(&self) -> Vec<String> {
        let mut entries: IndexSet<String> = self
            .file_ignores
            .iter()
            .map(|ignore| template_path::strip_leading_dot_slash(ignore))
            .collect();
        for ignore in &self.config.ignores {
            entries.insert(self.normalize_ignore_entry(ignore));
        }
        entries.into_iter().collect()
    }

    pub fn get_glob_watcher_ignores(&self) -> Vec<String> {
        let mut entries: IndexSet<String> = IndexSet::new();
        if let Some(o) = self.get_output_dir_ignore() {
            entries.insert(template_path::strip_leading_dot_slash(&o));
        }
        for ignore in &self.config.watch_ignores {
            entries.insert(self.normalize_ignore_entry(ignore));
        }
        entries.into_iter().collect()
    }
}
