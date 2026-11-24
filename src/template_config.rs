//! Port of `TemplateConfig.js`: loads and merges project configuration.
//!
//! Upstream executes a JavaScript configuration file. In Rust the configuration
//! API is exposed as closures (`Options::config`) and a declarative
//! configuration file (`buildawesome.config.json`/`.toml`/`.yaml`, also
//! `eleventy.config.*`) covering the serializable subset of the API.

use std::ops::Deref;
use std::sync::{Arc, Mutex, RwLock};

use anyhow::{Context, Result, bail};

use crate::default_config;
use crate::global_dependency_map::GlobalDependencyMap;
use crate::user_config::{DataFileNames, UserConfig};
use crate::util::console_logger::ConsoleLogger;
use crate::util::exists_cache::ExistsCache;
use crate::util::project_directories::{ConfigDirs, ProjectDirectories};
use crate::util::project_template_formats::ProjectTemplateFormats;
use crate::util::template_path;
use crate::value::Value;

/// A programmatic configuration callback (the equivalent of a config file's default export)
pub type ConfigCallback = Arc<dyn Fn(&mut UserConfig) -> Result<()> + Send + Sync>;

/// The merged, read-only project configuration (`this.config` upstream)
pub struct Config {
    pub user: UserConfig,
    pub template_formats: Vec<String>,
    pub path_prefix: String,
    pub markdown_template_engine: Option<String>,
    pub html_template_engine: Option<String>,
    pub data_file_suffixes: Vec<String>,
    pub data_file_dir_base_name_override: Option<String>,
    pub data_file_names: Option<DataFileNames>,
    /// The active configuration file (used as a source in error messages)
    pub config_path: Option<String>,
}

impl Deref for Config {
    type Target = UserConfig;
    fn deref(&self) -> &UserConfig {
        &self.user
    }
}

impl Config {
    pub fn features_collections(&self) -> bool {
        self.user.features_collections
    }
}

/// A late-bound reference to the merged configuration
#[derive(Clone, Default)]
pub struct ConfigHandle(Arc<RwLock<Option<Arc<Config>>>>);

impl ConfigHandle {
    pub fn set(&self, config: Arc<Config>) {
        *self.0.write().unwrap() = Some(config);
    }

    pub fn get(&self) -> Option<Arc<Config>> {
        self.0.read().unwrap().clone()
    }

    pub fn path_prefix(&self) -> String {
        self.get()
            .map(|c| c.path_prefix.clone())
            .unwrap_or_else(|| "/".into())
    }
}

pub const DEFAULT_CONFIG_PATHS: &[&str] = &[
    "buildawesome.config.json",
    "buildawesome.config.toml",
    "buildawesome.config.yaml",
    "buildawesome.config.yml",
    "eleventy.config.json",
    "eleventy.config.toml",
    "eleventy.config.yaml",
    "eleventy.config.yml",
    ".eleventy.json",
];

/// JavaScript configuration files we can detect (but not execute)
const JS_CONFIG_PATHS: &[&str] = &[
    "buildawesome.config.js",
    "buildawesome.config.mjs",
    "buildawesome.config.cjs",
    ".eleventy.js",
    "eleventy.config.js",
    "eleventy.config.mjs",
    "eleventy.config.cjs",
];

pub struct TemplateConfig {
    pub user_config: UserConfig,
    config: Option<Arc<Config>>,
    pub directories: ProjectDirectories,
    pub template_formats: ProjectTemplateFormats,
    cli_template_formats: Option<Vec<String>>,
    project_config_paths: Vec<String>,
    config_manually_defined: bool,
    override_path_prefix: Option<String>,
    pub uses: Arc<Mutex<GlobalDependencyMap>>,
    pub exists_cache: Arc<ExistsCache>,
    pub logger: Arc<ConsoleLogger>,
    pub run_mode: String,
    /// Shared with filters that need the merged configuration at render time (e.g. `url`)
    pub config_handle: ConfigHandle,
    pub programmatic_config: Option<ConfigCallback>,
    pub verbose: bool,
}

impl TemplateConfig {
    pub fn new(project_config_path: Option<&str>) -> Self {
        let (paths, manual) = match project_config_path {
            Some("") => (vec![], true),
            Some(p) => (vec![p.to_string()], true),
            None => (DEFAULT_CONFIG_PATHS.iter().map(|s| s.to_string()).collect(), false),
        };
        TemplateConfig {
            user_config: UserConfig::new(),
            config: None,
            directories: ProjectDirectories::new(),
            template_formats: ProjectTemplateFormats::new(),
            cli_template_formats: None,
            project_config_paths: paths,
            config_manually_defined: manual,
            override_path_prefix: None,
            uses: Arc::new(Mutex::new(GlobalDependencyMap::new())),
            exists_cache: Arc::new(ExistsCache::new()),
            logger: Arc::new(ConsoleLogger::new()),
            run_mode: "build".into(),
            config_handle: ConfigHandle::default(),
            programmatic_config: None,
            verbose: true,
        }
    }

    pub fn set_logger(&mut self, logger: Arc<ConsoleLogger>) {
        self.logger = logger;
    }

    pub fn set_directories(&mut self, directories: ProjectDirectories) {
        self.user_config.directories = directories.get_userspace_instance();
        self.directories = directories;
    }

    pub fn set_template_formats(&mut self, formats: ProjectTemplateFormats) {
        self.cli_template_formats = formats.get_command_line_formats();
        if let Some(f) = &self.cli_template_formats {
            self.user_config.template_formats = Some(f.clone());
        }
        self.template_formats = formats;
    }

    pub fn set_run_mode(&mut self, run_mode: &str) {
        self.run_mode = run_mode.to_string();
    }

    pub fn is_watch_or_serve(&self) -> bool {
        self.run_mode == "watch" || self.run_mode == "serve"
    }

    pub fn set_path_prefix(&mut self, path_prefix: &str) {
        if !path_prefix.is_empty() && path_prefix != "/" {
            self.override_path_prefix = Some(path_prefix.to_string());
            self.user_config.path_prefix = Some(path_prefix.to_string());
        }
    }

    pub fn get_path_prefix(&self) -> String {
        if let Some(p) = &self.override_path_prefix {
            return p.clone();
        }
        self.config
            .as_ref()
            .map(|c| c.path_prefix.clone())
            .unwrap_or_else(|| "/".into())
    }

    pub fn get_local_project_config_file(&self) -> Option<String> {
        self.project_config_paths
            .iter()
            .filter(|p| !p.is_empty())
            .map(|p| template_path::add_leading_dot_slash(p))
            .find(|p| self.exists_cache.exists(p))
    }

    pub fn get_config(&self) -> Arc<Config> {
        self.config
            .clone()
            .expect("Invalid call to .getConfig(). Needs an .init() first.")
    }

    pub fn has_initialized(&self) -> bool {
        self.config.is_some()
    }

    pub fn init(&mut self) -> Result<()> {
        // Root (default) configuration runs first
        default_config::apply(&mut self.user_config, self.config_handle.clone())?;
        let config = Arc::new(self.merge_config()?);
        self.config_handle.set(config.clone());
        self.config = Some(config);
        self.exists_cache.reset();
        Ok(())
    }

    fn read_config_file(&mut self) -> Result<Option<Value>> {
        let path = self.get_local_project_config_file();
        if !self.project_config_paths.is_empty() && self.config_manually_defined && path.is_none() {
            bail!(
                "A configuration file was specified but not found: {}",
                self.project_config_paths.join(", ")
            );
        }
        let Some(path) = path else {
            if !self.config_manually_defined {
                for js in JS_CONFIG_PATHS {
                    if std::path::Path::new(js).exists() {
                        self.logger.warn(&format!(
                            "Found JavaScript configuration file {js}, which is not supported. Use a buildawesome.config.json or .toml file (or the Rust API) instead."
                        ));
                    }
                }
            }
            return Ok(None);
        };
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Error in your Eleventy config file '{path}'."))?;
        let ext = template_path::get_extension(&path);
        let value = match ext.as_str() {
            "json" => Value::from_json(
                serde_json::from_str(&content)
                    .with_context(|| format!("Error in your Eleventy config file '{path}'."))?,
            ),
            "toml" => Value::from_toml(
                toml::from_str(&content)
                    .with_context(|| format!("Error in your Eleventy config file '{path}'."))?,
            ),
            "yaml" | "yml" => Value::from_yaml(
                serde_yaml::from_str(&content)
                    .with_context(|| format!("Error in your Eleventy config file '{path}'."))?,
            ),
            _ => bail!("Unsupported configuration file format: {path}"),
        };
        Ok(Some(value))
    }

    pub fn get_active_config_path(&self) -> Option<String> {
        self.get_local_project_config_file()
    }

    fn merge_config(&mut self) -> Result<Config> {
        // Programmatic config (via options)
        if let Some(cb) = self.programmatic_config.clone() {
            cb(&mut self.user_config)?;
        }

        // Configuration file (declarative)
        let local_config = self.read_config_file()?;
        let mut local_dirs: Option<ConfigDirs> = None;
        if let Some(cfg) = &local_config {
            crate::config_file::apply(&mut self.user_config, cfg)?;
            let dir = cfg.get("dir");
            let dir = if dir.is_undefined() { cfg.get("directories") } else { dir };
            if dir.is_object() {
                local_dirs = Some(ConfigDirs {
                    input: dir.get("input").as_str().map(String::from),
                    output: dir.get("output").as_str().map(String::from),
                    data: dir.get("data").as_str().map(String::from),
                    includes: dir.get("includes").as_str().map(String::from),
                    layouts: dir.get("layouts").as_str().map(String::from),
                });
            }
        }

        if self.user_config.directories_relative_to.as_deref() == Some(".") {
            self.directories.set_relative_to_project_root(true);
        }
        let assignments = self.user_config.directory_assignments.clone();
        if !assignments.is_empty() {
            self.directories.set_via_config_object(&ConfigDirs {
                input: assignments.input,
                output: assignments.output,
                data: assignments.data,
                includes: assignments.includes,
                layouts: assignments.layouts,
            })?;
        }
        if let Some(dirs) = &local_dirs {
            self.directories.set_via_config_object(dirs)?;
        }
        self.user_config.directories = self.directories.get_userspace_instance();

        // Template formats
        let default_formats: Vec<String> = default_config::TEMPLATE_FORMATS
            .iter()
            .map(|s| s.to_string())
            .collect();
        match (&self.user_config.template_formats, &self.cli_template_formats) {
            (Some(f), Some(cli)) if f == cli => {
                self.template_formats.set_via_config(&default_formats);
            }
            (Some(f), _) => {
                let f = f.clone();
                self.template_formats.set_via_config(&f);
            }
            (None, _) => self.template_formats.set_via_config(&default_formats),
        }
        for added in self.user_config.template_formats_added.clone() {
            self.template_formats.add_via_config(&added)?;
        }

        self.user_config
            .emit("buildawesome.beforeconfig", &Value::Undefined)?;

        // Plugins
        self.process_plugins()?;
        for added in self.user_config.template_formats_added.clone() {
            self.template_formats.add_via_config(&added)?;
        }

        if let Some(tags) = &self.user_config.keys.tags {
            if tags.contains('.') {
                bail!("Nested properties are not supported in `keys.tags`, received: {tags:?}");
            }
        }
        if !self.user_config.features_collections && !self.user_config.collections.is_empty() {
            let names: Vec<String> = self.user_config.collections.keys().cloned().collect();
            bail!(
                "Collections are disabled via `configureFeatures({{ collections: false }})`: `$config.addCollection()` cannot be used (received: {}).",
                names.join(", ")
            );
        }

        let path_prefix = self
            .override_path_prefix
            .clone()
            .or_else(|| self.user_config.path_prefix.clone())
            .filter(|p| !p.is_empty())
            .unwrap_or_else(|| "/".into());

        let data_file_suffixes = self
            .user_config
            .data_file_suffixes_override
            .clone()
            .unwrap_or_else(|| vec![".data".into(), ".11tydata".into(), "".into()]);

        let user = self.user_config.clone();
        Ok(Config {
            template_formats: self.template_formats.get_template_formats(),
            path_prefix,
            markdown_template_engine: Some(
                user.markdown_template_engine
                    .clone()
                    .unwrap_or_else(|| "liquid".into()),
            ),
            html_template_engine: Some(
                user.html_template_engine
                    .clone()
                    .unwrap_or_else(|| "liquid".into()),
            ),
            data_file_suffixes,
            data_file_dir_base_name_override: user.data_file_dir_base_name_override.clone(),
            data_file_names: user.data_file_names.clone(),
            config_path: self.get_local_project_config_file(),
            user,
        })
    }

    fn process_plugins(&mut self) -> Result<()> {
        self.user_config.enable_plugin_execution();
        let stored = self.user_config.active_namespace.clone();
        let plugins = std::mem::take(&mut self.user_config.plugins);
        for entry in plugins {
            self.user_config.active_namespace = entry.namespace.clone();
            self.user_config
                .execute_plugin(&entry.name, &entry.plugin, &entry.options)
                .with_context(|| {
                    format!(
                        "Error processing {} plugin",
                        if entry.name.is_empty() {
                            "a".to_string()
                        } else {
                            format!("the `{}`", entry.name)
                        }
                    )
                })?;
        }
        self.user_config.active_namespace = stored;
        self.user_config.disable_plugin_execution();
        Ok(())
    }
}
