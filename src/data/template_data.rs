//! Port of `Data/TemplateData.js`: global data, directory data and template data files.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{Context, Result, bail};
use indexmap::IndexSet;

use super::resolve_configuration_data::{self as rcd, EXCLUDE_FROM_COLLECTIONS_KEY};
use crate::extension_map::ExtensionMap;
use crate::file_system_search::{FileSystemSearch, SearchOptions};
use crate::global_dependency_map::GlobalDependencyMap;
use crate::template_config::Config;
use crate::util::console_logger::ConsoleLogger;
use crate::util::exists_cache::ExistsCache;
use crate::util::merge::merge;
use crate::util::project_directories::ProjectDirectories;
use crate::util::{node_path, reserved_data, template_path};
use crate::value::{Map, Value};

pub const DEFAULT_TAGS_KEY: &str = "tags";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// The upstream version this port tracks
pub const UPSTREAM_VERSION: &str = "4.0.0";

const ELIGIBLE_JAVASCRIPT_EXTENSIONS: &[&str] = &["js", "cjs", "mjs"];
const GLOBAL_DATA_ORDERED_EXTENSIONS: &[&str] = &["json", "mjs", "cjs", "js"];

#[derive(Clone)]
struct DirectoryEntry {
    sources: Vec<Value>,
    data: Value,
}

pub struct TemplateData {
    config: Arc<Config>,
    dirs: ProjectDirectories,
    extension_map: Arc<ExtensionMap>,
    file_system_search: Arc<FileSystemSearch>,
    exists_cache: Arc<ExistsCache>,
    uses: Arc<Mutex<GlobalDependencyMap>>,
    logger: Arc<ConsoleLogger>,
    pub environment_variables: Value,
    global_data: OnceLock<Value>,
    initial_global_data: OnceLock<Value>,
    template_directory_data: Mutex<HashMap<String, DirectoryEntry>>,
    data_file_contents: Mutex<HashMap<String, String>>,
    pub global_data_files_by_key: Mutex<HashMap<String, Vec<String>>>,
    warned_js: Mutex<bool>,
    raw_imports: OnceLock<Option<Value>>,
}

impl TemplateData {
    pub fn new(
        config: Arc<Config>,
        dirs: ProjectDirectories,
        extension_map: Arc<ExtensionMap>,
        file_system_search: Arc<FileSystemSearch>,
        exists_cache: Arc<ExistsCache>,
        uses: Arc<Mutex<GlobalDependencyMap>>,
        logger: Arc<ConsoleLogger>,
    ) -> Self {
        TemplateData {
            config,
            dirs,
            extension_map,
            file_system_search,
            exists_cache,
            uses,
            logger,
            environment_variables: Value::object(),
            global_data: OnceLock::new(),
            initial_global_data: OnceLock::new(),
            template_directory_data: Mutex::new(HashMap::new()),
            data_file_contents: Mutex::new(HashMap::new()),
            global_data_files_by_key: Mutex::new(HashMap::new()),
            warned_js: Mutex::new(false),
            raw_imports: OnceLock::new(),
        }
    }

    pub fn input_dir(&self) -> String {
        self.dirs.input()
    }

    pub fn data_dir(&self) -> String {
        self.dirs.data()
    }

    fn get_raw_imports(&self) -> Option<Value> {
        self.raw_imports
            .get_or_init(|| {
                let key = self.config.keys.package.clone()?;
                let pkg = std::fs::read_to_string("package.json")
                    .ok()
                    .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                    .map(Value::from_json)
                    .unwrap_or_else(Value::object);
                let mut m = Map::new();
                m.insert(key, pkg);
                Some(Value::from_map(m))
            })
            .clone()
    }

    pub fn get_configuration_data_file_suffixes(&self) -> Vec<String> {
        self.config.data_file_suffixes.clone()
    }

    fn user_data_extensions(&self) -> Vec<String> {
        // later additions have priority (reverse order)
        let mut exts: Vec<String> = self.config.data_extensions.keys().cloned().collect();
        exts.reverse();
        exts
    }

    fn legacy_data_file_names(&self) -> (Vec<String>, Vec<String>) {
        let base_name = self.config.data_file_dir_base_name_override.clone();
        let mut template = vec![];
        let mut directory = vec![];
        for suffix in self.get_configuration_data_file_suffixes() {
            let patterns: Vec<String> = if suffix.is_empty() {
                vec!["{name}.{ext}".into()]
            } else {
                vec![format!("{{name}}{suffix}.{{js}}"), format!("{{name}}{suffix}.{{ext}}")]
            };
            template.extend(patterns.clone());
            match &base_name {
                None => directory.extend(patterns),
                Some(b) if !suffix.is_empty() => {
                    directory.extend(patterns.iter().map(|p| p.replace("{name}", b)))
                }
                _ => {}
            }
        }
        (template, directory)
    }

    fn data_file_names(&self) -> (Vec<String>, Vec<String>, bool, bool) {
        let (mut template, mut directory) = self.legacy_data_file_names();
        let mut user_t = false;
        let mut user_d = false;
        if let Some(names) = &self.config.data_file_names {
            if let Some(t) = &names.template {
                template = t.clone();
                user_t = true;
            }
            if let Some(d) = &names.directory {
                directory = d.clone();
                user_d = true;
            }
        }
        (template, directory, user_t, user_d)
    }

    fn pattern_extensions(&self, token: &str, is_user_pattern: bool) -> Vec<String> {
        if token == "js" {
            return ELIGIBLE_JAVASCRIPT_EXTENSIONS.iter().map(|s| s.to_string()).collect();
        }
        let mut exts = vec!["json".to_string()];
        exts.extend(self.user_data_extensions());
        if !is_user_pattern {
            return exts;
        }
        exts.into_iter()
            .filter(|e| !ELIGIBLE_JAVASCRIPT_EXTENSIONS.contains(&e.as_str()))
            .collect()
    }

    fn expand_data_file_name(&self, pattern: &str, name: &str, is_user_pattern: bool) -> Vec<String> {
        let file_name = pattern.replace("{name}", name);
        let token = if file_name.contains("{ext}") {
            Some("ext")
        } else if file_name.contains("{js}") {
            Some("js")
        } else {
            None
        };
        let Some(token) = token else {
            return vec![file_name];
        };
        let placeholder = format!("{{{token}}}");
        self.pattern_extensions(token, is_user_pattern)
            .into_iter()
            .flat_map(|ext| {
                self.expand_data_file_name(&file_name.replacen(&placeholder, &ext, 1), name, is_user_pattern)
            })
            .collect()
    }

    fn push_data_file_paths(&self, paths: &mut Vec<String>, dir: &str, is_template: bool, name: &str) {
        let (template, directory, user_t, user_d) = self.data_file_names();
        let (patterns, is_user) = if is_template {
            (template, user_t)
        } else {
            (directory, user_d)
        };
        for pattern in patterns {
            if pattern.contains("{name}") && (name.is_empty() || name == ".") {
                continue;
            }
            for file_name in self.expand_data_file_name(&pattern, name, is_user) {
                paths.push(format!("{dir}/{file_name}"));
            }
        }
    }

    fn global_data_extension_priorities(&self) -> Vec<String> {
        let mut out = self.user_data_extensions();
        out.extend(GLOBAL_DATA_ORDERED_EXTENSIONS.iter().map(|s| s.to_string()));
        out
    }

    fn calculate_extension_priority(path: &str, priorities: &[String]) -> usize {
        let normalized = path.to_lowercase();
        priorities
            .iter()
            .position(|ext| normalized.ends_with(ext.as_str()))
            .unwrap_or(priorities.len())
    }

    pub fn get_global_data_glob(&self) -> Vec<String> {
        let ext_glob = self.global_data_extension_priorities().join(",");
        vec![template_path::add_leading_dot_slash(&template_path::join(&[
            &self.data_dir(),
            &format!("/**/*.{{{ext_glob}}}"),
        ]))]
    }

    /// `getAllDataFileSuffixes`
    fn get_all_data_file_suffixes(&self) -> Vec<String> {
        let mut suffixes: IndexSet<String> = IndexSet::new();
        suffixes.insert("json".into()); // covers .data.json too
        for suffix in self.get_configuration_data_file_suffixes() {
            let suffix = suffix.strip_prefix('.').unwrap_or(&suffix);
            if suffix.is_empty() {
                continue;
            }
            for ext in ["mjs", "cjs", "js"] {
                suffixes.insert(format!("{suffix}.{ext}"));
            }
        }
        for ext in self.user_data_extensions() {
            suffixes.insert(ext.strip_prefix('.').unwrap_or(&ext).to_string());
        }
        suffixes.into_iter().collect()
    }

    /// Used for `--watch` and `--serve` targets
    pub fn get_template_data_file_glob(&self) -> Vec<String> {
        let input_dir = self.input_dir();
        if self.config.data_file_names.is_none() {
            // Legacy glob format, kept unchanged when `setDataFileNames` is not used
            let suffixes = self.get_all_data_file_suffixes();
            return vec![template_path::add_leading_dot_slash(&format!(
                "{input_dir}**/*.{{{}}}",
                suffixes.join(",")
            ))];
        }
        let (template, directory, user_t, user_d) = self.data_file_names();
        let mut names: IndexSet<String> = IndexSet::new();
        for p in &template {
            names.extend(self.expand_data_file_name(p, "*", user_t));
        }
        for p in &directory {
            names.extend(self.expand_data_file_name(p, "*", user_d));
        }
        let names: Vec<String> = names.into_iter().collect();
        if names.is_empty() {
            return vec![];
        }
        let glob = if names.len() == 1 {
            names[0].clone()
        } else {
            format!("{{{}}}", names.join(","))
        };
        vec![template_path::add_leading_dot_slash(&format!("{input_dir}**/{glob}"))]
    }

    pub fn get_global_data_files(&self) -> Result<Vec<String>> {
        let priorities = self.global_data_extension_priorities();
        let mut paths = self
            .file_system_search
            .search("global-data", &self.get_global_data_glob(), SearchOptions::default())?;
        paths.sort_by_key(|p| Self::calculate_extension_priority(p, &priorities));
        Ok(paths)
    }

    pub fn get_object_path_for_data_file(&self, data_file_path: &str) -> Vec<String> {
        let absolute = template_path::absolute_path(&[data_file_path]);
        let reduced = template_path::strip_leading_sub_path(
            &absolute,
            &template_path::absolute_path(&[&self.data_dir()]),
        );
        let parsed = node_path::parse(&reduced);
        let mut folders: Vec<String> = if parsed.dir.is_empty() {
            vec![]
        } else {
            parsed.dir.split('/').map(String::from).collect()
        };
        folders.push(parsed.name);
        folders
    }

    fn get_all_global_data(&self) -> Result<Value> {
        let mut global_data = Value::object();
        let files = template_path::add_leading_dot_slash_array(&self.get_global_data_files()?);
        self.config.events.emit(
            "buildawesome.globaldatafiles",
            &Value::array(files.iter().map(Value::from).collect()),
        )?;
        let mut conflicts: HashMap<String, String> = HashMap::new();
        for file in &files {
            let mut data = self.get_data_value(file)?;
            let object_path = self.get_object_path_for_data_file(file);
            let key = object_path.join("/");
            let path_str = object_path
                .iter()
                .map(|p| format!("[{}]", serde_json::to_string(p).unwrap()))
                .collect::<String>();
            if conflicts.contains_key(&key) {
                let old = global_data.get_path(&path_str);
                data = merge(old, &[&data]);
            }
            conflicts.insert(key, file.clone());
            global_data.set_path(&path_str, data);
            self.global_data_files_by_key
                .lock()
                .unwrap()
                .entry(object_path[0].clone())
                .or_default()
                .push(file.clone());
            if self.config.freeze_reserved_data {
                reserved_data::check(&global_data, Some(file))?;
            }
        }
        Ok(global_data)
    }

    pub fn get_core_global(&self) -> Value {
        let mut global = Map::new();
        global.insert("version".into(), Value::str(UPSTREAM_VERSION));
        global.insert(
            "generator".into(),
            Value::str(format!("Twelvety (Build Awesome) v{UPSTREAM_VERSION}")),
        );
        global.insert("env".into(), self.environment_variables.clone());
        global.insert("directories".into(), self.dirs.get_userspace_instance());
        Value::from_map(global)
    }

    /// `ConfigurationGlobalData` (addGlobalData) plus the `twelvety` core global
    pub fn get_initial_global_data(&self) -> Result<Value> {
        if let Some(v) = self.initial_global_data.get() {
            return Ok(v.clone());
        }
        let mut global = Value::object();
        for (key, value) in &self.config.global_data {
            let resolved = match value {
                Value::Function(f) => f(&[])?,
                other => other.clone(),
            };
            global.set_path(key, resolved);
        }
        if self.config.freeze_reserved_data {
            // Upstream reports the active configuration file as the source; the
            // resolved `Config` doesn’t carry it.
            reserved_data::check(&global, self.config.config_path.as_deref())?;
        }
        let core = self.get_core_global();
        global.set("twelvety", core.clone());
        global.set("buildawesome", core);
        let _ = self.initial_global_data.set(global.clone());
        Ok(global)
    }

    pub fn get_global_data(&self) -> Result<Value> {
        if let Some(v) = self.global_data.get() {
            return Ok(v.clone());
        }
        let raw_imports = self.get_raw_imports();
        let config_api = self.get_initial_global_data()?;
        let global_json = self.get_all_global_data()?;
        let mut merged = merge(rcd::normalize(global_json), &[&rcd::normalize(config_api)]);
        if let Some(Value::Object(imports)) = raw_imports {
            for (k, v) in imports.iter() {
                merged.set(k.clone(), v.clone());
            }
        }
        let _ = self.global_data.set(merged.clone());
        Ok(merged)
    }

    pub fn get_local_data_sources(&self, paths: &[String]) -> Result<Vec<Value>> {
        let paths: Vec<String> = paths
            .iter()
            .filter(|p| self.exists_cache.exists(p))
            .cloned()
            .collect();
        self.config.events.emit(
            "buildawesome.datafiles",
            &Value::array(paths.iter().map(Value::from).collect()),
        )?;
        let mut sources = vec![];
        for path in &paths {
            let data = self.get_data_value(path)?;
            if !data.is_plain_object() {
                continue;
            }
            let cleaned = cleanup_data(data, self.config.keys.tags.as_deref(), path)?;
            sources.push(rcd::normalize(cleaned));
        }
        Ok(sources)
    }

    fn get_template_directory_entry(&self, template_path_str: &str) -> Result<DirectoryEntry> {
        if let Some(e) = self.template_directory_data.lock().unwrap().get(template_path_str) {
            return Ok(e.clone());
        }
        let paths: Vec<String> = self
            .get_local_data_paths(template_path_str)
            .into_iter()
            .filter_map(|p| self.exists_cache.resolve(&p))
            .filter(|p| self.exists_cache.exists(p))
            .collect();
        let sources = self.get_local_data_sources(&paths)?;
        self.uses
            .lock()
            .unwrap()
            .add_data_files(template_path_str, &paths);
        let refs: Vec<&Value> = sources.iter().collect();
        let entry = DirectoryEntry {
            data: merge(Value::object(), &refs),
            sources,
        };
        self.template_directory_data
            .lock()
            .unwrap()
            .insert(template_path_str.to_string(), entry.clone());
        Ok(entry)
    }

    pub fn get_template_directory_data(&self, template_path: &str) -> Result<Value> {
        Ok(self.get_template_directory_entry(template_path)?.data)
    }

    pub fn get_template_directory_data_sources(&self, template_path: &str) -> Result<Vec<Value>> {
        Ok(self.get_template_directory_entry(template_path)?.sources)
    }

    fn read_data_file(&self, path: &str) -> Result<String> {
        if let Some(c) = self.data_file_contents.lock().unwrap().get(path) {
            return Ok(c.clone());
        }
        // `LoadContent`: a missing file is empty, contents are trimmed
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c.trim().to_string(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => {
                return Err(e).with_context(|| format!("Having trouble reading data file {path}"));
            }
        };
        self.data_file_contents
            .lock()
            .unwrap()
            .insert(path.to_string(), content.clone());
        Ok(content)
    }

    pub fn get_data_value(&self, path: &str) -> Result<Value> {
        let extension = template_path::get_extension(path).to_lowercase();
        if ELIGIBLE_JAVASCRIPT_EXTENSIONS.contains(&extension.as_str()) {
            let mut warned = self.warned_js.lock().unwrap();
            if !*warned {
                *warned = true;
                self.logger.warn(&format!(
                    "JavaScript data files are not supported in the Rust port, skipping: {path}"
                ));
            }
            return Ok(Value::object());
        }
        if let Some(ext) = self.config.data_extensions.get(&extension) {
            let result = if ext.read {
                let content = self.read_data_file(path)?;
                if content.is_empty() {
                    return Ok(Value::object());
                }
                (ext.parser)(&content, path)
            } else {
                (ext.parser)(path, path)
            };
            return result.with_context(|| parse_error(path));
        }
        if extension == "json" {
            let content = self.read_data_file(path)?;
            if content.is_empty() {
                return Ok(Value::object());
            }
            let parsed: serde_json::Value = serde_json::from_str(&content)
                .with_context(|| parse_error(path))?;
            return Ok(Value::from_json(parsed));
        }
        bail!(
            "Could not find an appropriate data parser for {path}. Do you need to add a plugin to your config file?"
        )
    }

    pub fn get_local_data_paths(&self, template_path_str: &str) -> Vec<String> {
        let mut paths = vec![];
        let parsed = node_path::parse(template_path_str);
        if !parsed.dir.is_empty() {
            let file_name_no_ext = self.extension_map.remove_template_extension(&parsed.base);
            self.push_data_file_paths(&mut paths, &parsed.dir, true, &file_name_no_ext);
            paths.extend(self.get_directory_data_paths(&parsed.dir));
        }
        let mut unique: IndexSet<String> = IndexSet::new();
        for p in paths {
            unique.insert(p);
        }
        let mut out: Vec<String> = unique.into_iter().collect();
        out.reverse();
        out
    }

    fn get_directory_data_paths(&self, parsed_dir: &str) -> Vec<String> {
        let mut paths = vec![];
        let input_dir = self.input_dir();
        for dir in template_path::get_all_dirs(parsed_dir) {
            if input_dir.is_empty() || (dir.starts_with(&input_dir) && dir != input_dir) {
                let segment = template_path::get_last_path_segment(&dir);
                self.push_data_file_paths(&mut paths, &dir, false, &segment);
            }
        }
        if !input_dir.is_empty() {
            let segment = template_path::get_last_path_segment(&input_dir);
            let root_dir = template_path::get_dir_from_file_path(&template_path::add_leading_dot_slash(
                &template_path::join(&[&input_dir, &segment]),
            ));
            self.push_data_file_paths(&mut paths, &root_dir, false, &segment);
        }
        paths
    }

    pub fn get_global_data_files_by_key(&self) -> HashMap<String, Vec<String>> {
        self.global_data_files_by_key.lock().unwrap().clone()
    }
}

/// `TemplateDataParseError` (with the data file as its `filePath`)
fn parse_error(path: &str) -> crate::template_writer::TemplateFileError {
    crate::template_writer::TemplateFileError {
        message: format!("Having trouble parsing data file {path}"),
        file_path: path.to_string(),
    }
}

/// `getCleanedTagsImmutable`
pub fn get_cleaned_tags(data: &Value, tags_key: Option<&str>, file: &str) -> Result<Vec<String>> {
    get_cleaned_tags_with(data, tags_key, file, false)
}

fn get_cleaned_tags_with(data: &Value, tags_key: Option<&str>, file: &str, is_virtual: bool) -> Result<Vec<String>> {
    let Some(key) = tags_key else {
        return Ok(vec![]);
    };
    if !data.is_plain_object() {
        return Ok(vec![]);
    }
    let value = data.get(key);
    if !value.is_truthy() {
        return Ok(vec![]);
    }
    let tags: Vec<String> = match &value {
        Value::String(s) => s.split(',').map(String::from).collect(),
        Value::Array(a) => a.iter().map(|v| v.to_js_string()).collect(),
        other => bail!(
            "String or Array expected for `{key}`{}. Received: {}",
            if file.is_empty() {
                String::new()
            } else {
                format!(" in {}template: {file}", if is_virtual { "virtual " } else { "" })
            },
            other.inspect()
        ),
    };
    let mut out: IndexSet<String> = IndexSet::new();
    for t in tags {
        out.insert(t.trim().to_string());
    }
    Ok(out.into_iter().collect())
}

/// `TemplateData.cleanupData`
pub fn cleanup_data(data: Value, tags_key: Option<&str>, file: &str) -> Result<Value> {
    cleanup_data_with(data, tags_key, file, false)
}

/// `cleanupData` with the `isVirtualTemplate` option (used in error messages)
pub fn cleanup_data_with(mut data: Value, tags_key: Option<&str>, file: &str, is_virtual: bool) -> Result<Value> {
    if let Some(key) = tags_key {
        if data.is_plain_object() && data.has(key) {
            let tags = get_cleaned_tags_with(&data, Some(key), file, is_virtual)?;
            data.set(key, Value::array(tags.into_iter().map(Value::from).collect()));
        }
    }
    Ok(data)
}

/// `getNormalizedExcludedCollections`
pub fn get_normalized_excluded_collections(data: &Value) -> (Vec<String>, bool) {
    let value = rcd::get_value(data, EXCLUDE_FROM_COLLECTIONS_KEY);
    match &value {
        Value::Bool(true) => (vec![], true),
        Value::Array(a) => (a.iter().map(|v| v.to_js_string()).collect(), false),
        Value::String(s) => (s.split(',').map(String::from).collect(), false),
        _ => (vec![], false),
    }
}

pub fn get_included_collection_names(data: &Value, tags_key: Option<&str>) -> Vec<String> {
    let tags = get_cleaned_tags(data, tags_key, "").unwrap_or_default();
    let (excludes, exclude_all) = get_normalized_excluded_collections(data);
    if exclude_all {
        return vec![];
    }
    let mut out = vec!["all".to_string()];
    out.extend(tags);
    out.into_iter().filter(|t| !excludes.contains(t)).collect()
}

pub fn get_included_tag_names(data: &Value, tags_key: Option<&str>) -> Vec<String> {
    get_included_collection_names(data, tags_key)
        .into_iter()
        .filter(|t| t != "all")
        .collect()
}
