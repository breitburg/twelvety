//! Port of `Engines/Custom.js`: engines added via `addExtension`.

use std::sync::{Arc, Mutex};

use anyhow::{Result, bail};

use super::{PermalinkCompilation, RenderFn, TemplateEngine};
use crate::user_config::ExtensionOptions;
use crate::value::Value;

pub struct CustomEngine {
    name: String,
    entry: ExtensionOptions,
    default_engine: Option<Arc<dyn TemplateEngine>>,
    /// Whether `init` still needs to run
    needs_init: Mutex<bool>,
}

impl CustomEngine {
    pub fn new(name: &str, entry: ExtensionOptions, default_engine: Option<Arc<dyn TemplateEngine>>) -> Self {
        CustomEngine {
            name: name.to_string(),
            needs_init: Mutex::new(entry.init.is_some()),
            entry,
            default_engine,
        }
    }

    /// Runs the `init` callback once (`_runningInit`)
    fn ensure_init(&self) -> Result<()> {
        let mut needs_init = self.needs_init.lock().unwrap();
        if *needs_init {
            if let Some(init) = &self.entry.init {
                init()?;
            }
            *needs_init = false;
        }
        Ok(())
    }

    /// `defaultRenderer`: compile and render with the overridden/aliased engine
    fn default_renderer(&self, src: &str, input_path: Option<&str>) -> Option<RenderFn> {
        let engine = self.default_engine.clone()?;
        let src = src.to_string();
        let input_path = input_path.map(String::from);
        Some(Arc::new(move |data| engine.compile(&src, input_path.as_deref())?(data)))
    }
}

impl TemplateEngine for CustomEngine {
    fn name(&self) -> &str {
        &self.name
    }

    fn cacheable(&self) -> bool {
        if let Some(c) = self.entry.cache {
            return c;
        }
        if self.needs_to_read_file_contents() {
            return true;
        }
        self.default_engine.as_ref().map(|e| e.cacheable()).unwrap_or(false)
    }

    fn needs_to_read_file_contents(&self) -> bool {
        if let Some(r) = self.entry.read {
            return r;
        }
        self.default_engine
            .as_ref()
            .map(|e| e.needs_to_read_file_contents())
            .unwrap_or(true)
    }

    fn get_extra_data_from_file(&self, input_path: &str) -> Result<Value> {
        if let Some(f) = &self.entry.get_data {
            self.ensure_init()?;
            return f(input_path);
        }
        if let Some(e) = &self.default_engine {
            return e.get_extra_data_from_file(input_path);
        }
        Ok(Value::object())
    }

    fn ready(&self) -> Result<()> {
        self.ensure_init()
    }

    fn compile(&self, src: &str, input_path: Option<&str>) -> Result<RenderFn> {
        self.ensure_init()?;
        if let Some(compile) = &self.entry.compile_with_default_renderer {
            let default_renderer = self.default_renderer(src, input_path);
            let f = compile(src, input_path.unwrap_or(""), default_renderer)?;
            return Ok(Arc::new(move |data| f(data)));
        }
        match &self.entry.compile {
            Some(compile) => {
                let f = compile(src, input_path.unwrap_or(""))?;
                Ok(Arc::new(move |data| f(data)))
            }
            None => match &self.default_engine {
                Some(e) => e.compile(src, input_path),
                None => bail!(
                    "Missing `compile` property for custom template syntax definition twelvetyConfig.addExtension(\"{}\"). This is not necessary when aliasing to an existing template syntax.",
                    self.name
                ),
            },
        }
    }

    fn needs_compilation(&self, src: &str) -> bool {
        match &self.default_engine {
            Some(e) => e.needs_compilation(src),
            None => true,
        }
    }

    fn permalink_needs_compilation(&self, _src: &str) -> PermalinkCompilation {
        match self.entry.permalink_needs_compilation {
            Some(false) => PermalinkCompilation::Raw,
            Some(true) => PermalinkCompilation::Compile,
            None => PermalinkCompilation::Raw,
        }
    }

    fn default_template_file_extension(&self) -> String {
        self.entry
            .output_file_extension
            .clone()
            .unwrap_or_else(|| "html".into())
    }

    fn use_layouts(&self) -> bool {
        self.entry.use_layouts.unwrap_or(true)
    }

    fn custom_name(&self) -> Option<String> {
        self.entry.name.clone()
    }

    fn is_custom(&self) -> bool {
        true
    }
}
