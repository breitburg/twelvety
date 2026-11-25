//! Port of `TemplateWriter.js`.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use anyhow::{Context, Result, bail};

use crate::build_context::BuildContext;
use crate::files::Files;
use crate::template::{PageResult, RenderableOverride, Template};
use crate::template_map::TemplateMap;
use crate::template_passthrough::{CopyResult, TemplatePassthroughManager};
use crate::value::Value;

#[derive(Default, Debug, Clone)]
pub struct WriterMetadata {
    pub copy_count: usize,
    pub copy_size: u64,
    pub skip_count: usize,
    pub write_count: usize,
    pub render_count: usize,
}

/// Context attached to errors from a template, carrying its input path (`TemplateError#filePath`)
#[derive(Debug, Clone)]
pub struct TemplateFileError {
    pub message: String,
    pub file_path: String,
}

impl std::fmt::Display for TemplateFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// The input path of the template an error came from, if any
pub fn error_file_path(e: &anyhow::Error) -> Option<String> {
    e.downcast_ref::<TemplateFileError>()
        .map(|t| t.file_path.clone())
}

/// Record the file an error came from (unless a more specific one is already known).
/// The marker repeats the error’s message, so the displayed error is unchanged.
pub fn attach_file_path(e: anyhow::Error, file_path: &str) -> anyhow::Error {
    if error_file_path(&e).is_some() {
        return e;
    }
    let message = e.to_string();
    e.context(TemplateFileError {
        message,
        file_path: file_path.to_string(),
    })
}

fn writing_error(output_path: &Value, input_path: &str) -> TemplateFileError {
    TemplateFileError {
        message: format!(
            "Having trouble writing to \"{}\" from \"{input_path}\"",
            output_path.to_output_string()
        ),
        file_path: input_path.to_string(),
    }
}

pub struct WriteResults {
    pub passthrough_copy: Vec<CopyResult>,
    pub templates: Vec<PageResult>,
}

pub struct TemplateWriter {
    ctx: Arc<BuildContext>,
    files: Files,
    passthrough_manager: Arc<TemplatePassthroughManager>,
    pub is_dry_run: bool,
    pub is_run_initial_build: bool,
    pub write_count: usize,
    pub render_count: usize,
    pub skipped_count: usize,
    pub template_map: Option<TemplateMap>,
}

impl TemplateWriter {
    pub fn new(
        ctx: Arc<BuildContext>,
        files: Files,
        passthrough_manager: Arc<TemplatePassthroughManager>,
    ) -> Self {
        TemplateWriter {
            ctx,
            files,
            passthrough_manager,
            is_dry_run: false,
            is_run_initial_build: true,
            write_count: 0,
            render_count: 0,
            skipped_count: 0,
            template_map: None,
        }
    }

    pub fn ctx_config(&self) -> Arc<crate::template_config::Config> {
        self.ctx.config.clone()
    }

    pub fn ctx(&self) -> &Arc<BuildContext> {
        &self.ctx
    }

    pub fn files(&self) -> &Files {
        &self.files
    }

    fn get_paths_with_virtual_templates(&self, mut paths: Vec<String>) -> Result<Vec<String>> {
        let dirs = &self.ctx.dirs;
        for path in self.ctx.config.virtual_templates.keys() {
            let full = dirs.get_input_path(path);
            if !dirs.is_template_file(&full) {
                continue;
            }
            if self.ctx.extension_map.get_key(&full).is_none() {
                bail!(
                    "The virtual template at {full} is using a template format that’s not valid for your project. Your project is using: \"{}\". Read more about formats: https://v3.11ty.dev/docs/config/#template-formats",
                    self.ctx.config.template_formats.join(",")
                );
            }
            if paths.contains(&full) {
                bail!(
                    "A virtual template had the same path as a file on the file system: \"{full}\""
                );
            }
            paths.push(full);
        }
        Ok(paths)
    }

    /// Globs for files copied by file extension (formats without a template engine)
    pub fn get_passthrough_globs(&self) -> Vec<String> {
        self.ctx
            .extension_map
            .get_passthrough_copy_globs(&self.ctx.dirs.input())
    }

    pub fn get_all_paths(&self) -> Result<Vec<String>> {
        let paths = self.files.get_files()?;
        self.get_paths_with_virtual_templates(paths)
    }

    /// Creates (and caches) the template map for `paths`
    pub fn create_template_map(&mut self, paths: &[String]) -> Result<()> {
        let mut map = TemplateMap::new(self.ctx.clone());
        for path in paths {
            if !self.ctx.extension_map.has_engine(path) {
                continue;
            }
            let tmpl = Arc::new(Template::new(path, self.ctx.clone(), self.is_dry_run));
            if !self.is_run_initial_build {
                tmpl.set_renderable_override(RenderableOverride::Disabled);
            }
            map.add(tmpl)?;
        }
        map.cache()?;
        self.template_map = Some(map);
        Ok(())
    }

    fn generate_templates(&mut self, paths: &[String], to_json: bool) -> Result<Vec<PageResult>> {
        self.create_template_map(paths)?;
        let map = self.template_map.as_ref().unwrap();
        let mut results = vec![];
        for entry in &map.map {
            for page in &entry.pages {
                let tmpl = &page.template;
                if !to_json {
                    if !tmpl.is_renderable() {
                        continue;
                    }
                    if page.output_path().as_bool() == Some(false) {
                        continue;
                    }
                }
                let content =
                    if tmpl.is_renderable() {
                        Some(tmpl.render_page_entry(page).with_context(|| {
                            writing_error(&page.output_path(), &entry.input_path)
                        })?)
                    } else {
                        None
                    };
                if to_json {
                    let mut data = page.data();
                    // Collections are resolved lazily at render time; `dataFilterSelectors` can select them
                    let collections = map.collections.snapshot();
                    if !collections.is_empty() {
                        data.set("collections", Value::from_map(collections));
                    }
                    results.push(PageResult {
                        input_path: page.input_path.clone(),
                        output_path: page.output_path(),
                        url: page.url(),
                        content: content.unwrap_or_default(),
                        raw_input: page.raw_input.clone(),
                        data: if self.ctx.config.data_filter_selectors.is_empty() {
                            None
                        } else {
                            Some(tmpl.retrieve_data_for_json_output(&data))
                        },
                    });
                    continue;
                }
                if let Some(content) = content
                    && let Some(r) = tmpl
                        .write(page, content)
                        .with_context(|| writing_error(&page.output_path(), &entry.input_path))?
                {
                    results.push(r);
                }
            }
        }
        for entry in &map.map {
            self.write_count += entry.template.write_count.load(Ordering::Relaxed);
            self.render_count += entry.template.render_count.load(Ordering::Relaxed);
        }
        Ok(results)
    }

    fn write_passthrough_copy(&self, paths: &[String]) -> Result<Vec<CopyResult>> {
        self.passthrough_manager
            .copy_all(paths, &self.ctx.extension_map)
            .context("Having trouble copying")
    }

    pub fn write(&mut self) -> Result<WriteResults> {
        let paths = self.get_all_paths()?;
        self.ctx
            .config
            .events
            .emit("buildawesome#beforerender", &Value::Undefined)?;
        let mut passthrough = self.write_passthrough_copy(&paths)?;
        let templates = self.generate_templates(&paths, false)?;
        passthrough.extend(self.passthrough_manager.copy_html_relative()?);
        self.passthrough_manager
            .emit_passthrough_event(&passthrough)?;
        self.ctx
            .config
            .events
            .emit("buildawesome#render", &Value::Undefined)?;
        Ok(WriteResults {
            passthrough_copy: passthrough,
            templates,
        })
    }

    pub fn write_templates(&mut self) -> Result<Vec<PageResult>> {
        let paths = self.get_all_paths()?;
        self.generate_templates(&paths, false)
    }

    pub fn get_json(&mut self) -> Result<Vec<PageResult>> {
        let paths = self.get_all_paths()?;
        self.generate_templates(&paths, true)
    }

    pub fn get_metadata(&self) -> WriterMetadata {
        let (copy_count, copy_size) = self.passthrough_manager.get_metadata();
        WriterMetadata {
            copy_count,
            copy_size,
            skip_count: self.skipped_count,
            write_count: self.write_count,
            render_count: self.render_count,
        }
    }

    pub fn dispose(&self) {
        if let Some(map) = &self.template_map {
            map.dispose();
        }
    }
}
