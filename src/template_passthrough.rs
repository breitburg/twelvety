//! Port of `TemplatePassthrough.js` and `TemplatePassthroughManager.js`.

use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};
use indexmap::IndexMap;

use crate::extension_map::ExtensionMap;
use crate::file_system_search::{FileSystemSearch, SearchOptions};
use crate::template_config::Config;
use crate::util::console_logger::ConsoleLogger;
use crate::util::exists_cache::ExistsCache;
use crate::util::glob_matcher::is_dynamic_pattern;
use crate::util::project_directories::ProjectDirectories;
use crate::util::{chalk, node_path, template_path};
use crate::value::{Map, Value};

#[derive(Clone, Debug, Default)]
pub struct CopyResult {
    pub count: usize,
    pub size: u64,
    /// source -> destination
    pub map: IndexMap<String, String>,
}

#[derive(Clone)]
pub struct TemplatePassthrough {
    pub input_path: String,
    /// `true` (same relative path) or an explicit output path
    pub output_path: Value,
    pub copy_options: Value,
    is_already_normalized: bool,
    is_input_path_glob: bool,
}

fn copy_option(options: &Value, key: &str, default: bool) -> bool {
    options.get(key).as_bool().unwrap_or(default)
}

impl TemplatePassthrough {
    pub fn new(input_path: &str, output_path: Value, copy_options: Value) -> Self {
        let input_path = Self::normalize_if_directory(input_path);
        TemplatePassthrough {
            is_input_path_glob: is_dynamic_pattern(&input_path),
            input_path,
            output_path,
            copy_options,
            is_already_normalized: false,
        }
    }

    pub fn normalize_if_directory(input: &str) -> String {
        if input.ends_with('/') {
            return input.to_string();
        }
        if Path::new(input).is_dir() {
            return format!("{input}/");
        }
        input.to_string()
    }

    pub fn get_output_path(
        &self,
        dirs: &ProjectDirectories,
        input_file_from_glob: Option<&str>,
    ) -> String {
        let input_dir = dirs.input();
        let output_dir = dirs.output();
        if self.output_path.as_bool() == Some(true) {
            let source = input_file_from_glob.unwrap_or(&self.input_path);
            let rel = template_path::strip_leading_sub_path(source, &input_dir);
            if Path::new(&self.input_path).is_dir() {
                return ProjectDirectories::normalize_directory(&template_path::join(&[
                    &output_dir,
                    &rel,
                ]));
            }
            return template_path::normalize(&template_path::join(&[&output_dir, &rel]));
        }
        let output_path = self.output_path.to_output_string();
        if let Some(glob_file) = input_file_from_glob {
            return self.get_output_path_for_glob_file(dirs, glob_file);
        }
        let mut full = template_path::normalize(&template_path::join(&[&output_dir, &output_path]));
        if output_path.is_empty() || Path::new(&self.input_path).is_dir() {
            full = ProjectDirectories::normalize_directory(&full);
        }
        if !self.is_input_path_glob
            && Path::new(&self.input_path).exists()
            && !Path::new(&self.input_path).is_dir()
            && Path::new(&full).is_dir()
        {
            let filename = node_path::parse(&self.input_path).base;
            return template_path::normalize(&template_path::join(&[&full, &filename]));
        }
        full
    }

    pub fn is_dynamic_pattern(&self) -> bool {
        self.is_input_path_glob
    }

    pub fn get_output_path_for_glob_file(
        &self,
        dirs: &ProjectDirectories,
        input_file_from_glob: &str,
    ) -> String {
        let base = self.get_output_path(dirs, None);
        template_path::join(&[
            &base,
            &template_path::get_last_path_segment(input_file_from_glob),
        ])
    }

    /// Search the file system for a glob (*only* ignores the output directory)
    pub fn get_files(
        &self,
        dirs: &ProjectDirectories,
        search: &FileSystemSearch,
        glob: &str,
    ) -> Result<Vec<String>> {
        let files = search.search(
            "passthrough",
            &[glob.to_string()],
            SearchOptions {
                ignore: vec![dirs.output()],
                git_ignore: vec![],
            },
        )?;
        Ok(template_path::add_leading_dot_slash_array(&files))
    }

    pub fn get_file_map(
        &self,
        dirs: &ProjectDirectories,
        search: &FileSystemSearch,
        exists_cache: &ExistsCache,
        config: &Config,
        logger: &ConsoleLogger,
    ) -> Result<Vec<(String, String)>> {
        if self.is_already_normalized {
            return Ok(vec![(
                self.input_path.clone(),
                self.output_path.to_output_string(),
            )]);
        }
        if !self.is_input_path_glob {
            if !Path::new(&self.input_path).exists() {
                // case-only mismatch check
                if !config.check_error_reporting("passthroughCaseMismatch", "allow")
                    && let Some(resolved) = exists_cache.resolve(&self.input_path)
                {
                    let message = format!(
                        "Passthrough copy source {:?} does not exist, but {resolved:?} does (differs only by case). This resolves on case-insensitive file systems (macOS, Windows) but is missing on Linux—fix the casing.",
                        self.input_path
                    );
                    if !config.check_error_reporting("passthroughCaseMismatch", "warn") {
                        bail!(
                            "{message}\nTo downgrade this to a warning, use `$config.configureErrorReporting({{ passthroughCaseMismatch: \"warn\" }})` (or `\"allow\"` to skip this check)."
                        );
                    }
                    logger.warn(&format!("Warning: {message}"));
                }
                return Ok(vec![]);
            }
            return Ok(vec![(
                self.input_path.clone(),
                self.get_output_path(dirs, None),
            )]);
        }
        let files = self.get_files(dirs, search, &self.input_path)?;
        Ok(files
            .into_iter()
            .map(|f| {
                let out = self.get_output_path(dirs, Some(&f));
                (f, out)
            })
            .collect())
    }
}

/// OS junk files skipped unless `junk: true` (the `junk` package used by `recursive-copy`)
fn is_junk(name: &str) -> bool {
    matches!(
        name,
        ".DS_Store"
            | ".AppleDouble"
            | ".LSOverride"
            | ".Spotlight-V100"
            | ".Trashes"
            | "Thumbs.db"
            | "ehthumbs.db"
            | "Desktop.ini"
            | "desktop.ini"
            | "npm-debug.log"
            | ".localized"
    ) || name.starts_with("._")
        || name.starts_with(".~lock.")
        || name.ends_with('~') && name.len() > 1
        || name.starts_with(".nfs")
}

/// Calls a predicate copy option (`filter`, `rawPathFilter`), `true` when not set
fn copy_predicate(options: &Value, key: &str, arg: &str) -> Result<bool> {
    let f = options.get(key);
    if !f.is_function() {
        return Ok(true);
    }
    Ok(f.call(&[Value::str(arg)])?.is_truthy())
}

/// Recursive copy (`recursive-copy` with Twelvety’s defaults)
fn copy_recursive(
    src: &str,
    dest: &str,
    options: &Value,
    result: &mut CopyResult,
    dirs: &ProjectDirectories,
) -> Result<()> {
    let dot = copy_option(options, "dot", true);
    let junk = copy_option(options, "junk", false);
    let overwrite = copy_option(options, "overwrite", true);
    if !dirs.is_file_in_output_folder(dest) {
        bail!("Destination is not in the site output directory. Check your passthrough paths.");
    }
    let rename = |relative: &str| -> Result<String> {
        let f = options.get("rename");
        if !f.is_function() {
            return Ok(relative.to_string());
        }
        let renamed = f.call(&[Value::str(relative)])?.to_output_string();
        if !dirs.is_file_in_output_folder(&node_path::join(&[dest, &renamed])) {
            bail!(
                "Passthrough copy `rename` result `{renamed}` is outside of the site output directory."
            );
        }
        Ok(renamed)
    };
    let src_path = Path::new(src.trim_end_matches('/'));
    if src_path.is_dir() {
        for entry in walkdir::WalkDir::new(src_path).follow_links(true) {
            let entry = entry?;
            let rel = entry.path().strip_prefix(src_path).unwrap_or(entry.path());
            let rel_str = rel.to_string_lossy().replace('\\', "/");
            if rel_str.is_empty() {
                continue;
            }
            if !dot && rel_str.split('/').any(|seg| seg.starts_with('.')) {
                continue;
            }
            if !junk && rel_str.split('/').any(is_junk) {
                continue;
            }
            if !copy_predicate(options, "filter", &rel_str)?
                || !copy_predicate(options, "rawPathFilter", &entry.path().to_string_lossy())?
            {
                continue;
            }
            let target = format!("{}/{}", dest.trim_end_matches('/'), rename(&rel_str)?);
            if entry.file_type().is_dir() {
                std::fs::create_dir_all(&target)?;
                continue;
            }
            copy_file(
                &entry.path().to_string_lossy(),
                &target,
                overwrite,
                result,
                dirs,
            )?;
        }
        return Ok(());
    }
    let name = node_path::basename(src);
    if (!dot && name.starts_with('.'))
        || (!junk && is_junk(&name))
        || !copy_predicate(options, "rawPathFilter", src)?
    {
        return Ok(());
    }
    copy_file(src, dest, overwrite, result, dirs)
}

fn copy_file(
    src: &str,
    dest: &str,
    overwrite: bool,
    result: &mut CopyResult,
    dirs: &ProjectDirectories,
) -> Result<()> {
    if !dirs.is_file_in_output_folder(dest) {
        bail!("Destination is not in the site output directory. Check your passthrough paths.");
    }
    if !overwrite && Path::new(dest).exists() {
        return Ok(());
    }
    if let Some(parent) = Path::new(dest).parent() {
        std::fs::create_dir_all(parent)?;
    }
    let size = std::fs::copy(src, dest)
        .with_context(|| format!("Having trouble copying {src} to {dest}"))?;
    result.count += 1;
    result.size += size;
    result.map.insert(src.replace('\\', "/"), dest.to_string());
    Ok(())
}

pub struct TemplatePassthroughManager {
    config: Arc<Config>,
    dirs: ProjectDirectories,
    search: Arc<FileSystemSearch>,
    exists_cache: Arc<ExistsCache>,
    logger: Arc<ConsoleLogger>,
    pub is_dry_run: bool,
    pub count: Mutex<usize>,
    pub size: Mutex<u64>,
    pub html_relative_copies: Arc<Mutex<Vec<(String, String, Value)>>>,
}

impl TemplatePassthroughManager {
    pub fn new(
        config: Arc<Config>,
        dirs: ProjectDirectories,
        search: Arc<FileSystemSearch>,
        exists_cache: Arc<ExistsCache>,
        logger: Arc<ConsoleLogger>,
    ) -> Self {
        let html_relative_copies: Arc<Mutex<Vec<(String, String, Value)>>> =
            Arc::new(Mutex::new(vec![]));
        let c = html_relative_copies.clone();
        config.events.on(
            "buildawesome#copy",
            Arc::new(move |arg: &Value| {
                c.lock().unwrap().push((
                    arg.get("source").to_output_string(),
                    arg.get("target").to_output_string(),
                    arg.get("options"),
                ));
                Ok(())
            }),
        );
        TemplatePassthroughManager {
            config,
            dirs,
            search,
            exists_cache,
            logger,
            is_dry_run: false,
            count: Mutex::new(0),
            size: Mutex::new(0),
            html_relative_copies,
        }
    }

    /// Globs configured via `addPassthroughCopy` (inputs)
    pub fn get_config_path_globs(&self) -> Vec<String> {
        self.config
            .passthrough_copies
            .keys()
            .map(|p| {
                let normalized = TemplatePassthrough::normalize_if_directory(p);
                if normalized.ends_with('/') {
                    format!("{}**", template_path::add_leading_dot_slash(&normalized))
                } else {
                    template_path::add_leading_dot_slash(&normalized)
                }
            })
            .collect()
    }

    fn normalize_path(
        &self,
        path: &str,
        output_path: &Value,
        copy_options: &Value,
    ) -> TemplatePassthrough {
        let output_path = match output_path {
            Value::String(p) if !p.is_empty() => {
                Value::str(template_path::strip_leading_dot_slash(p))
            }
            v if v.is_truthy() => v.clone(),
            _ => Value::Bool(true),
        };
        let copy_options = crate::util::merge::merge(
            self.config.passthrough_copy_options.clone(),
            &[copy_options],
        );
        TemplatePassthrough::new(
            &template_path::add_leading_dot_slash(path),
            output_path,
            copy_options,
        )
    }

    /// Paths configured via `addPassthroughCopy`
    pub fn get_config_paths(&self) -> Vec<TemplatePassthrough> {
        self.config
            .passthrough_copies
            .iter()
            .map(|(input, entry)| {
                self.normalize_path(input, &entry.output_path, &entry.copy_options)
            })
            .collect()
    }

    /// Template-glob matches without a template engine are copied (by file extension)
    pub fn get_non_template_paths(
        &self,
        paths: &[String],
        extension_map: &ExtensionMap,
    ) -> Vec<String> {
        paths
            .iter()
            .filter(|path| !extension_map.has_engine(path))
            .cloned()
            .collect()
    }

    pub fn get_all_normalized_paths(
        &self,
        paths: &[String],
        extension_map: &ExtensionMap,
    ) -> Vec<TemplatePassthrough> {
        let mut normalized = self.get_config_paths();
        for path in self.get_non_template_paths(paths, extension_map) {
            normalized.push(self.normalize_path(&path, &Value::Bool(true), &Value::object()));
        }
        normalized
    }

    fn get_url(&self, output_path: &str) -> String {
        let relative = template_path::strip_leading_sub_path(output_path, &self.dirs.output());
        format!("/{}", crate::util::url_util::encode_uri(&relative))
    }

    /// Output URLs mapped to their input paths
    pub fn get_aliases_from_passthrough_results(
        &self,
        results: &[CopyResult],
    ) -> IndexMap<String, String> {
        results
            .iter()
            .flat_map(|r| r.map.iter())
            .map(|(src, dest)| (self.get_url(dest), src.clone()))
            .collect()
    }

    fn copy_passthrough_entry(
        &self,
        pass: &TemplatePassthrough,
        map: &mut IndexMap<String, String>,
    ) -> Result<CopyResult> {
        let mut result = CopyResult::default();
        let file_map = pass.get_file_map(
            &self.dirs,
            &self.search,
            &self.exists_cache,
            &self.config,
            &self.logger,
        )?;
        for (input, output) in file_map {
            if self.is_dry_run {
                continue;
            }
            // Outputs that differ only by case overwrite each other on macOS and Windows
            let dest_key = if self.config.check_error_reporting("caseCollisions", "allow") {
                output.clone()
            } else {
                output.to_lowercase()
            };
            if let Some(existing) = map.get(&dest_key)
                && existing != &input
            {
                let mut paths = [input.clone(), existing.clone()];
                paths.sort();
                bail!(
                    "Multiple passthrough copy files are trying to write to the same output file ({}). {}",
                    template_path::standardize_file_path(&output),
                    paths
                        .map(|p| template_path::standardize_file_path(&p))
                        .join(" and ")
                );
            }
            map.insert(dest_key, input.clone());
            copy_recursive(&input, &output, &pass.copy_options, &mut result, &self.dirs)
                .with_context(|| format!("Having trouble copying '{}'", pass.input_path))?;
        }
        Ok(result)
    }

    /// Copies a single passthrough (its files are checked against `map`, output -> input)
    pub fn copy_passthrough(
        &self,
        pass: &TemplatePassthrough,
        map: &mut IndexMap<String, String>,
    ) -> Result<CopyResult> {
        self.copy_passthrough_entry(pass, map)
    }

    /// Copies all passthrough files. `template_paths` are all files found by the template glob.
    /// Html-relative copies happen later (`copy_html_relative`), after templates render; the
    /// `buildawesome.passthrough` event is sent once both are done (`emit_passthrough_event`).
    pub fn copy_all(
        &self,
        template_paths: &[String],
        extension_map: &ExtensionMap,
    ) -> Result<Vec<CopyResult>> {
        let passthroughs = self.get_all_normalized_paths(template_paths, extension_map);
        let mut results = vec![];
        let mut map = IndexMap::new();
        for p in &passthroughs {
            let r = self.copy_passthrough_entry(p, &mut map)?;
            *self.count.lock().unwrap() += r.count;
            *self.size.lock().unwrap() += r.size;
            results.push(r);
        }
        Ok(results)
    }

    pub fn emit_passthrough_event(&self, results: &[CopyResult]) -> Result<()> {
        let mut arg = Map::new();
        arg.insert("entries".into(), Value::array(vec![]));
        arg.insert(
            "map".into(),
            Value::from_map(
                self.get_aliases_from_passthrough_results(results)
                    .into_iter()
                    .map(|(url, src)| (url, Value::str(src)))
                    .collect(),
            ),
        );
        arg.insert("incremental".into(), Value::Bool(false));
        self.config
            .events
            .emit("buildawesome.passthrough", &Value::from_map(arg))
    }

    /// Copies requested by the html-relative passthrough plugin (after templates render), one
    /// result per unique source and target
    pub fn copy_html_relative(&self) -> Result<Vec<CopyResult>> {
        let copies = std::mem::take(&mut *self.html_relative_copies.lock().unwrap());
        let mut seen = std::collections::HashSet::new();
        let mut results = vec![];
        for (source, target, options) in copies {
            if self.is_dry_run || !seen.insert(format!("{source}=>{target}")) {
                continue;
            }
            let mut result = CopyResult::default();
            copy_recursive(&source, &target, &options, &mut result, &self.dirs)
                .with_context(|| format!("Having trouble copying '{source}'"))?;
            *self.count.lock().unwrap() += result.count;
            *self.size.lock().unwrap() += result.size;
            results.push(result);
        }
        Ok(results)
    }

    pub fn get_metadata(&self) -> (usize, u64) {
        (*self.count.lock().unwrap(), *self.size.lock().unwrap())
    }

    pub fn log_large(&self) {
        let _ = chalk::gray("");
    }
}
