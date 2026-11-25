//! Port of `Util/ProjectDirectories.js`.

use std::path::Path;

use anyhow::{Result, bail};

use super::glob_matcher::is_dynamic_pattern;
use super::exists_cache::ExistsCache;
use super::{dir_contains, node_path, template_path};
use crate::value::{Map, Value};

pub const DEFAULT_INPUT: &str = "./";
pub const DEFAULT_DATA: &str = "./_data/";
pub const DEFAULT_INCLUDES: &str = "./_includes/";
pub const DEFAULT_LAYOUTS: &str = "./_layouts/";
pub const DEFAULT_OUTPUT: &str = "./_site/";

#[derive(Clone, Debug, Default)]
struct Raw {
    input: Option<String>,
    data: Option<String>,
    includes: Option<String>,
    layouts: Option<String>,
    output: Option<String>,
}

#[derive(Clone, Debug, Default)]
struct Dirs {
    input: Option<String>,
    data: Option<String>,
    includes: Option<String>,
    layouts: Option<String>,
    output: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ProjectDirectories {
    frozen: bool,
    relative_to_project_root: bool,
    raw: Raw,
    dirs: Dirs,
    pub input_file: Option<String>,
    pub input_glob: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ConfigDirs {
    pub input: Option<String>,
    pub output: Option<String>,
    pub data: Option<String>,
    pub includes: Option<String>,
    pub layouts: Option<String>,
}

impl ProjectDirectories {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn relative_to_project_root(file_or_dir: &str) -> String {
        Self::normalize_path(&template_path::relative_path(file_or_dir))
    }

    pub fn normalize_path(file_or_dir: &str) -> String {
        template_path::standardize_file_path(file_or_dir)
    }

    pub fn normalize_directory(dir: &str) -> String {
        Self::add_trailing_slash(&Self::normalize_path(dir))
    }

    fn normalize_directory_path_relative_to_input_directory(&self, file_path: &str) -> String {
        let base = if self.relative_to_project_root {
            "".to_string()
        } else {
            self.input()
        };
        Self::normalize_directory(&node_path::join(&[&base, file_path]))
    }

    pub fn add_trailing_slash(path: &str) -> String {
        if path.ends_with('/') {
            path.to_string()
        } else {
            format!("{path}/")
        }
    }

    pub fn freeze(&mut self) {
        self.frozen = true;
    }

    pub fn set_via_config_object(&mut self, config_dirs: &ConfigDirs) -> Result<()> {
        let mut input_changed = false;
        if let Some(input) = &config_dirs.input {
            if !input.is_empty() && Self::normalize_directory(input) != self.input() {
                self.set_input_raw(Some(input), None)?;
                input_changed = true;
            }
        }
        if let Some(output) = &config_dirs.output {
            if Self::normalize_directory(output) != self.output() {
                self.set_output(Some(output));
            }
        }
        if let Some(data) = &config_dirs.data {
            if input_changed
                || self.normalize_directory_path_relative_to_input_directory(data) != self.data()
            {
                self.set_data(Some(data));
            }
        }
        if let Some(includes) = &config_dirs.includes {
            if input_changed
                || self.normalize_directory_path_relative_to_input_directory(includes)
                    != self.includes()
            {
                self.set_includes(Some(includes));
            }
        }
        if let Some(layouts) = &config_dirs.layouts {
            if input_changed
                || Some(self.normalize_directory_path_relative_to_input_directory(layouts))
                    != self.layouts()
            {
                self.set_layouts(Some(layouts));
            }
        }
        if input_changed {
            self.update_input_dependencies();
        }
        Ok(())
    }

    pub fn set_relative_to_project_root(&mut self, value: bool) {
        self.relative_to_project_root = value;
        self.update_input_dependencies();
    }

    fn is_relative_to_project_root(&self, name: &str) -> bool {
        self.relative_to_project_root && self.raw_value(name).is_some()
    }

    fn raw_value(&self, name: &str) -> Option<&String> {
        match name {
            "input" => self.raw.input.as_ref(),
            "data" => self.raw.data.as_ref(),
            "includes" => self.raw.includes.as_ref(),
            "layouts" => self.raw.layouts.as_ref(),
            "output" => self.raw.output.as_ref(),
            _ => None,
        }
    }

    pub fn update_input_dependencies(&mut self) {
        self.resolve("data");
        self.resolve("includes");
        if self.raw.layouts.is_some() {
            self.resolve("layouts");
        }
    }

    fn resolve(&mut self, name: &str) {
        let default = match name {
            "data" => DEFAULT_DATA,
            "includes" => DEFAULT_INCLUDES,
            "layouts" => DEFAULT_LAYOUTS,
            _ => "",
        };
        let dir = self
            .raw_value(name)
            .cloned()
            .unwrap_or_else(|| default.to_string());
        let base = if self.is_relative_to_project_root(name) {
            String::new()
        } else {
            self.input()
        };
        let resolved = Self::normalize_directory(&template_path::join(&[&base, &dir]));
        match name {
            "data" => self.dirs.data = Some(resolved),
            "includes" => self.dirs.includes = Some(resolved),
            "layouts" => self.dirs.layouts = Some(resolved),
            _ => {}
        }
    }

    fn set_input_raw(&mut self, dir_or_file: Option<&str>, input_dir: Option<&str>) -> Result<()> {
        if self.frozen && self.raw.input.is_some() {
            return Ok(());
        }
        self.raw.input = dir_or_file.map(|s| s.to_string());
        let Some(dir_or_file) = dir_or_file else {
            return Ok(());
        };
        if dir_or_file.is_empty() {
            return Ok(());
        }
        let mut dir_or_file = dir_or_file.to_string();
        if node_path::is_absolute(&dir_or_file) {
            dir_or_file = node_path::relative(".", &dir_or_file);
        }
        let path = Path::new(&dir_or_file);
        let exists = path.exists();
        if exists && path.is_dir() {
            self.dirs.input = Some(Self::normalize_directory(&dir_or_file));
        } else {
            if exists {
                self.input_file = Some(Self::normalize_path(&dir_or_file));
            } else {
                if !is_dynamic_pattern(&dir_or_file) {
                    bail!(
                        "The \"{dir_or_file}\" `input` parameter (directory or file path) must exist on the file system (unless detected as a glob by the `tinyglobby` package)"
                    );
                }
                self.input_glob = Some(dir_or_file.clone());
            }
            if let Some(input_dir) = input_dir {
                if !Path::new(input_dir).exists() {
                    bail!("Directory must exist (via inputDir option to Twelvety constructor).");
                }
                self.dirs.input = Some(Self::normalize_directory(input_dir));
            } else {
                self.dirs.input = Some(Self::normalize_directory(
                    &template_path::get_dir_from_file_path(&glob_parent(&dir_or_file)),
                ));
            }
        }
        Ok(())
    }

    pub fn set_input(&mut self, dir_or_file: Option<&str>, input_dir: Option<&str>) -> Result<()> {
        self.set_input_raw(dir_or_file, input_dir)?;
        self.update_input_dependencies();
        Ok(())
    }

    pub fn set_includes(&mut self, dir: Option<&str>) {
        if let Some(dir) = dir {
            self.raw.includes = Some(dir.to_string());
            self.resolve("includes");
        }
    }

    pub fn set_layouts(&mut self, dir: Option<&str>) {
        if let Some(dir) = dir {
            self.raw.layouts = Some(dir.to_string());
            self.resolve("layouts");
        }
    }

    pub fn set_data(&mut self, dir: Option<&str>) {
        if let Some(dir) = dir {
            self.raw.data = Some(dir.to_string());
            self.resolve("data");
        }
    }

    pub fn set_output(&mut self, dir: Option<&str>) {
        if self.frozen && self.raw.output.is_some() {
            return;
        }
        if let Some(dir) = dir {
            self.raw.output = Some(dir.to_string());
            let mut dir = dir.to_string();
            if node_path::is_absolute(&dir) {
                dir = node_path::relative(".", &dir);
            }
            self.dirs.output = Some(Self::normalize_directory(&dir));
        }
    }

    /// Throws when a configured directory doesn’t match the casing on disk (#4224)
    pub fn check_case(&self, exists_cache: &ExistsCache) -> Result<()> {
        let paths = [
            ("input", Some(self.input_file.clone().unwrap_or_else(|| self.input()))),
            ("data", Some(self.data())),
            ("includes", Some(self.includes())),
            ("layouts", self.layouts()),
            ("output", Some(self.output())),
        ];
        for (name, dir) in paths {
            let Some(dir) = dir.filter(|d| !d.is_empty()) else {
                continue;
            };
            if let Some(resolved) = exists_cache.resolve(&dir) {
                if resolved != dir {
                    bail!(
                        "The `{name}` path \"{dir}\" doesn’t match the casing on disk: \"{resolved}\""
                    );
                }
            }
        }
        Ok(())
    }

    /// Catch input-relative directories that were configured as if they were project-root relative
    pub fn check_input_relative(&self, exists_cache: &ExistsCache) -> Result<()> {
        for name in ["data", "includes", "layouts"] {
            let dir = match name {
                "data" => Some(self.data()),
                "includes" => Some(self.includes()),
                _ => self.layouts(),
            };
            let raw = self.raw_value(name).cloned().or_else(|| match name {
                "data" => Some(DEFAULT_DATA.to_string()),
                "includes" => Some(DEFAULT_INCLUDES.to_string()),
                _ => None,
            });
            let (Some(dir), Some(raw)) = (dir, raw) else {
                continue;
            };
            if dir.is_empty()
                || raw.is_empty()
                || node_path::is_absolute(&raw)
                || self.is_relative_to_project_root(name)
            {
                continue;
            }
            let root_relative = Self::normalize_directory(&raw);
            if root_relative != dir
                && !exists_cache.exists(&dir)
                && exists_cache.is_directory(&root_relative)
            {
                let suggestion = Self::add_trailing_slash(&template_path::standardize_file_path(
                    &node_path::relative(&self.input(), &root_relative),
                ));
                bail!(
                    "The `{name}` directory is relative to the input directory: \"{raw}\" resolves to \"{dir}\", which doesn’t exist. Did you mean \"{suggestion}\" (resolves to \"{root_relative}\")?"
                );
            }
        }
        Ok(())
    }

    pub fn input(&self) -> String {
        self.dirs
            .input
            .clone()
            .unwrap_or_else(|| DEFAULT_INPUT.to_string())
    }

    pub fn data(&self) -> String {
        self.dirs
            .data
            .clone()
            .unwrap_or_else(|| DEFAULT_DATA.to_string())
    }

    pub fn includes(&self) -> String {
        self.dirs
            .includes
            .clone()
            .unwrap_or_else(|| DEFAULT_INCLUDES.to_string())
    }

    pub fn layouts(&self) -> Option<String> {
        self.dirs.layouts.clone()
    }

    pub fn output(&self) -> String {
        self.dirs
            .output
            .clone()
            .unwrap_or_else(|| DEFAULT_OUTPUT.to_string())
    }

    pub fn is_template_file(&self, file_path: &str) -> bool {
        let project_path = Self::relative_to_project_root(file_path);
        let input = self.input();
        for dir in [self.layouts(), Some(self.includes()), Some(self.data())]
            .into_iter()
            .flatten()
        {
            if dir != input && project_path.starts_with(&dir) {
                return false;
            }
        }
        project_path.starts_with(&input)
    }

    pub fn get_input_path(&self, file_path_relative_to_input_dir: &str) -> String {
        template_path::add_leading_dot_slash(&template_path::join(&[
            &self.input(),
            &template_path::standardize_file_path(file_path_relative_to_input_dir),
        ]))
    }

    pub fn get_input_path_relative_to_input_directory(&self, file_path: &str) -> String {
        if !dir_contains::dir_contains(&self.input(), file_path) {
            return template_path::standardize_file_path(&node_path::relative(
                &self.input(),
                file_path,
            ));
        }
        let input_dir = template_path::add_leading_dot_slash(&template_path::join(&[&self.input()]));
        template_path::strip_leading_sub_path(file_path, &input_dir)
    }

    pub fn get_output_path_relative_to_output_directory(&self, file_path: &str) -> String {
        let file_path = template_path::add_leading_dot_slash(file_path);
        let output_dir =
            template_path::add_leading_dot_slash(&template_path::join(&[&self.output()]));
        format!(
            "/{}",
            template_path::strip_leading_sub_path(&file_path, &output_dir)
        )
    }

    pub fn get_layout_path(&self, file_path_relative_to_layout_dir: &str) -> String {
        let base = self.layouts().unwrap_or_else(|| self.includes());
        template_path::add_leading_dot_slash(&template_path::join(&[
            &base,
            &template_path::standardize_file_path(file_path_relative_to_layout_dir),
        ]))
    }

    pub fn get_layout_path_relative_to_input_directory(&self, p: &str) -> String {
        self.get_input_path_relative_to_input_directory(&self.get_layout_path(p))
    }

    pub fn get_project_path(&self, file_path: &str) -> String {
        template_path::add_leading_dot_slash(&template_path::join(&[
            ".",
            &template_path::standardize_file_path(file_path),
        ]))
    }

    pub fn is_file_in_project_folder(&self, file_path: &str) -> bool {
        dir_contains::dir_contains(&template_path::get_working_dir(), file_path)
    }

    pub fn is_file_in_output_folder(&self, file_path: &str) -> bool {
        dir_contains::dir_contains(&self.output(), file_path)
    }

    /// The object exposed as `twelvety.directories` and `twelvetyConfig.directories`
    pub fn get_userspace_instance(&self) -> Value {
        let mut map = Map::new();
        map.insert("input".into(), Value::str(self.input()));
        map.insert(
            "inputFile".into(),
            self.input_file.clone().map(Value::from).unwrap_or_default(),
        );
        map.insert(
            "inputGlob".into(),
            self.input_glob.clone().map(Value::from).unwrap_or_default(),
        );
        map.insert("data".into(), Value::str(self.data()));
        map.insert("includes".into(), Value::str(self.includes()));
        map.insert(
            "layouts".into(),
            self.layouts().map(Value::from).unwrap_or_default(),
        );
        map.insert("output".into(), Value::str(self.output()));
        Value::from_map(map)
    }
}

/// The non-glob parent directory of a glob (tinyglobby / glob-parent behavior)
fn glob_parent(pattern: &str) -> String {
    if !is_dynamic_pattern(pattern) {
        return pattern.to_string();
    }
    let mut parts: Vec<&str> = vec![];
    for seg in pattern.split('/') {
        if is_dynamic_pattern(seg) {
            break;
        }
        parts.push(seg);
    }
    let mut dir = parts.join("/");
    if dir.is_empty() {
        dir = ".".into();
    }
    format!("{dir}/x")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults() {
        let mut d = ProjectDirectories::new();
        d.update_input_dependencies();
        assert_eq!(d.input(), "./");
        assert_eq!(d.includes(), "./_includes/");
        assert_eq!(d.data(), "./_data/");
        assert_eq!(d.output(), "./_site/");
    }

    #[test]
    fn layout_path() {
        let mut d = ProjectDirectories::new();
        d.update_input_dependencies();
        assert_eq!(d.get_layout_path("base.njk"), "./_includes/base.njk");
    }
}
