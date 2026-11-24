//! Port of `TemplateContent.js`: reading, front matter, compiling and rendering.

use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, anyhow};

use crate::build_context::BuildContext;
use crate::data::front_matter::{self, FrontMatter, FrontMatterOptions};
use crate::data::resolve_configuration_data as rcd;
use crate::data::template_data::cleanup_data_with;
use crate::engines::{PermalinkCompilation, RenderFn, TemplateEngine};
use crate::errors::TemplateContentPrematureUseError;
use crate::page_entry::{clear_premature_use, take_premature_use};
use crate::template_render::TemplateRender;
use crate::user_config::VirtualTemplate;
use crate::value::Value;

pub struct TemplateContent {
    pub ctx: Arc<BuildContext>,
    pub input_path: String,
    pub template_render: Result<TemplateRender, String>,
    read_cache: Mutex<Option<Arc<FrontMatter>>>,
    front_matter_data_cache: Mutex<Option<Value>>,
}

/// Options for `_render`
pub struct RenderOptions<'a> {
    pub template_only: bool,
    pub kind: &'a str,
    pub step_inputs: Option<&'a Mutex<Vec<String>>>,
}

impl TemplateContent {
    pub fn new(ctx: Arc<BuildContext>, input_path: &str) -> Self {
        let template_render =
            TemplateRender::new(ctx.clone(), input_path).map_err(|e| format!("{e:#}"));
        TemplateContent {
            ctx,
            input_path: input_path.to_string(),
            template_render,
            read_cache: Mutex::new(None),
            front_matter_data_cache: Mutex::new(None),
        }
    }

    pub fn render(&self) -> Result<&TemplateRender> {
        self.template_render
            .as_ref()
            .map_err(|e| anyhow!("{e}"))
    }

    pub fn engine(&self) -> Result<Arc<dyn TemplateEngine>> {
        Ok(self.render()?.engine.clone())
    }

    pub fn reset_caches(&self) {
        *self.read_cache.lock().unwrap() = None;
        *self.front_matter_data_cache.lock().unwrap() = None;
    }

    pub fn input_dir(&self) -> String {
        self.ctx.dirs.input()
    }

    pub fn output_dir(&self) -> String {
        self.ctx.dirs.output()
    }

    pub fn get_virtual_template_definition(&self) -> Option<&VirtualTemplate> {
        let rel = self
            .ctx
            .dirs
            .get_input_path_relative_to_input_directory(&self.input_path);
        self.ctx.config.virtual_templates.get(&rel)
    }

    pub fn is_virtual_template(&self) -> bool {
        self.get_virtual_template_definition().is_some()
    }

    fn front_matter_options(&self) -> FrontMatterOptions {
        FrontMatterOptions::from_value(&self.ctx.config.front_matter_parsing_options)
    }

    pub fn get_input_content(&self) -> Result<String> {
        if let Some(def) = self.get_virtual_template_definition() {
            return Ok(def.content.clone());
        }
        if !self.engine()?.needs_to_read_file_contents() {
            return Ok(String::new());
        }
        if self.ctx.config.use_template_cache {
            if let Some(c) = self.ctx.input_cache.lock().unwrap().get(&self.input_path) {
                return Ok(c.clone());
            }
        }
        let content = std::fs::read_to_string(&self.input_path)
            .with_context(|| format!("Having trouble reading template {}", self.input_path))?;
        if self.ctx.config.use_template_cache {
            self.ctx
                .input_cache
                .lock()
                .unwrap()
                .insert(self.input_path.clone(), content.clone());
        }
        Ok(content)
    }

    pub fn read(&self) -> Result<Arc<FrontMatter>> {
        if let Some(fm) = self.read_cache.lock().unwrap().as_ref() {
            return Ok(fm.clone());
        }
        let content = self.get_input_content()?;
        let options = self.front_matter_options();
        let mut fm = front_matter::parse(&content, &options, &self.input_path).with_context(|| {
            format!(
                "Having trouble reading front matter from template {}",
                self.input_path
            )
        })?;
        front_matter::apply_eleventy_excerpt(&mut fm, &options);
        let fm = Arc::new(fm);
        *self.read_cache.lock().unwrap() = Some(fm.clone());
        Ok(fm)
    }

    pub fn get_pre_render(&self) -> Result<String> {
        Ok(self.read()?.content.clone())
    }

    pub fn get_front_matter_data(&self) -> Result<Value> {
        if let Some(d) = self.front_matter_data_cache.lock().unwrap().as_ref() {
            return Ok(d.clone());
        }
        let fm = self.read()?;
        let tags_key = self.ctx.config.keys.tags.clone();
        let virtual_def = self.get_virtual_template_definition();
        let is_virtual = virtual_def.is_some();
        let virtual_data = match virtual_def {
            Some(def) => Some(cleanup_data_with(def.data.clone(), tags_key.as_deref(), &self.input_path, true)?),
            None => None,
        };
        let mut data = cleanup_data_with(fm.data.clone(), tags_key.as_deref(), &self.input_path, is_virtual)?;
        if !data.is_object() {
            data = Value::object();
        }
        let extra = cleanup_data_with(
            self.engine()?.get_extra_data_from_file(&self.input_path)?,
            tags_key.as_deref(),
            &self.input_path,
            is_virtual,
        )?;
        for source in [Some(extra), virtual_data].into_iter().flatten() {
            if let Value::Object(m) = source {
                for (k, v) in m.iter() {
                    data.set(k.clone(), v.clone());
                }
            }
        }
        let data = rcd::normalize(data);
        *self.front_matter_data_cache.lock().unwrap() = Some(data.clone());
        Ok(data)
    }

    pub fn get_engine_override(&self, data: &Value) -> Option<String> {
        match data.get(&self.ctx.config.keys.engine_override) {
            Value::String(s) => Some(s.to_string()),
            Value::Bool(false) | Value::Null => Some(String::new()),
            _ => None,
        }
    }

    pub fn get_engine_names(&self, engine_override: Option<&str>) -> String {
        self.render()
            .and_then(|r| r.get_engines_list(engine_override))
            .unwrap_or_default()
    }

    /// `compile(str, { templateOnly, engineOverride })` with the compile cache
    pub fn compile(&self, src: &str, template_only: bool, engine_override: Option<&str>) -> Result<RenderFn> {
        let tr = self.render()?;
        let resolved = tr.resolve(engine_override)?;
        if template_only && !resolved.engine.needs_compilation(src) {
            let s = src.to_string();
            return Ok(Arc::new(move |_| Ok(s.clone())));
        }
        let cache_key = if self.ctx.config.use_template_cache && resolved.engine.cacheable() {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            src.hash(&mut hasher);
            Some(format!(
                "{}|{}|{:?}|{:x}",
                self.input_path,
                template_only,
                engine_override,
                hasher.finish()
            ))
        } else {
            None
        };
        if let Some(key) = &cache_key {
            if let Some(f) = self.ctx.compile_cache.lock().unwrap().get(key) {
                return Ok(f.clone());
            }
        }
        let f = tr
            .get_compiled_template(src, template_only, engine_override)
            .with_context(|| format!("Having trouble compiling template {}", self.input_path))?;
        if let Some(key) = cache_key {
            self.ctx.compile_cache.lock().unwrap().insert(key, f.clone());
        }
        Ok(f)
    }

    pub fn get_computed_data_engine(&self, data: &Value) -> Option<Arc<dyn TemplateEngine>> {
        let over = self.get_engine_override(data);
        self.render()
            .ok()?
            .get_template_only_engine(over.as_deref())
            .ok()
            .flatten()
    }

    pub fn parse_for_symbols(&self, value: &Value, engine: Option<&Arc<dyn TemplateEngine>>) -> Vec<String> {
        let parse = |s: &str| -> Vec<String> {
            let mut symbols = engine
                .and_then(|e| e.parse_for_symbols(s))
                .unwrap_or_default();
            // Liquid’s parser only reports output symbols: add tag references too
            for sym in crate::engines::find_symbols(s) {
                if !symbols.contains(&sym) {
                    symbols.push(sym);
                }
            }
            symbols
        };
        match value {
            Value::String(s) => parse(s),
            Value::Array(a) => a
                .iter()
                .filter_map(|v| v.as_str())
                .flat_map(parse)
                .collect(),
            _ => vec![],
        }
    }

    pub fn render_computed_data(&self, s: &str, data: &Value) -> Result<String> {
        self.render_string(
            s,
            data,
            RenderOptions {
                template_only: true,
                kind: "Computed Data",
                step_inputs: None,
            },
        )
    }

    pub fn render_permalink(&self, permalink: &Value, data: &Value) -> Result<Value> {
        if let Value::Function(f) = permalink {
            return f(&[data.clone()]);
        }
        let s = permalink.to_output_string();
        let engine = self.engine()?;
        match engine.permalink_needs_compilation(&s) {
            PermalinkCompilation::Raw => return Ok(permalink.clone()),
            PermalinkCompilation::Disabled => return Ok(Value::Bool(false)),
            PermalinkCompilation::Compile => {}
        }
        Ok(Value::str(self.render_string(
            &s,
            data,
            RenderOptions {
                template_only: true,
                kind: "Permalink",
                step_inputs: None,
            },
        )?))
    }

    /// `_render`
    pub fn render_string(&self, s: &str, data: &Value, options: RenderOptions) -> Result<String> {
        let engine_override = self.get_engine_override(data);
        let f = self.compile(s, options.template_only, engine_override.as_deref())?;
        clear_premature_use();
        let result = f(data);
        if let Some(message) = take_premature_use() {
            return Err(anyhow::Error::new(TemplateContentPrematureUseError(message)));
        }
        let _ = options.step_inputs;
        result.with_context(|| {
            let engine = self
                .render()
                .map(|r| r.get_readable_engines_list())
                .unwrap_or_default();
            format!(
                "Having trouble rendering {engine} template {}",
                self.input_path
            )
        })
    }
}
