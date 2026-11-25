//! Helpers for tests (the equivalent of upstream’s `_getNewTemplateForTests.js`
//! and `_getRenderedTemplates.js`).

use std::sync::Arc;

use anyhow::Result;

use crate::build_context::BuildContext;
use crate::template::Template;
use crate::template_config::{ConfigCallback, TemplateConfig};
use crate::template_map::TemplateMap;
use crate::template_render::TemplateRender;
use crate::user_config::UserConfig;
use crate::util::console_logger::ConsoleLogger;
use crate::util::project_directories::ProjectDirectories;
use crate::value::{Map, Value};

/// Configuration for a test build context
#[derive(Clone, Default)]
pub struct TestOptions {
    pub input: Option<String>,
    pub output: Option<String>,
    pub config: Option<ConfigCallback>,
    pub path_prefix: Option<String>,
    pub formats: Option<String>,
}

impl TestOptions {
    pub fn new(input: &str, output: &str) -> Self {
        TestOptions {
            input: Some(input.to_string()),
            output: Some(output.to_string()),
            ..Default::default()
        }
    }

    pub fn input(input: &str) -> Self {
        TestOptions {
            input: Some(input.to_string()),
            ..Default::default()
        }
    }

    pub fn with_config<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut UserConfig) -> Result<()> + Send + Sync + 'static,
    {
        self.config = Some(Arc::new(f));
        self
    }
}

/// A quiet logger for tests
pub fn quiet_logger() -> Arc<ConsoleLogger> {
    let logger = ConsoleLogger::new();
    logger.set_verbose(false);
    Arc::new(logger)
}

/// Create the shared build state (no configuration file is loaded)
pub fn get_context(options: TestOptions) -> Result<Arc<BuildContext>> {
    let mut dirs = ProjectDirectories::new();
    dirs.set_input(options.input.as_deref(), None)?;
    dirs.set_output(options.output.as_deref());
    let mut tc = TemplateConfig::new(Some(""));
    let logger = quiet_logger();
    tc.set_logger(logger.clone());
    tc.set_directories(dirs);
    if let Some(p) = &options.path_prefix {
        tc.set_path_prefix(p);
    }
    if options.formats.is_some() {
        let mut f = crate::util::project_template_formats::ProjectTemplateFormats::new();
        f.set_via_command_line(options.formats.as_deref());
        tc.set_template_formats(f);
    }
    tc.programmatic_config = options.config.clone();
    tc.init()?;
    let mut env = Map::new();
    env.insert("source".into(), Value::str("script"));
    env.insert("runMode".into(), Value::str("build"));
    env.insert("root".into(), Value::str(crate::util::template_path::get_working_dir()));
    let (ctx, _) = BuildContext::create(&tc, logger, "build", Value::from_map(env))?;
    Ok(ctx)
}

/// `getNewTemplate(path, inputDir, outputDir, twelvetyConfig)`
pub fn get_new_template(path: &str, options: TestOptions) -> Result<Arc<Template>> {
    let ctx = get_context(options)?;
    Ok(Arc::new(Template::new(path, ctx, false)))
}

pub fn template_with_context(path: &str, ctx: &Arc<BuildContext>) -> Arc<Template> {
    Arc::new(Template::new(path, ctx.clone(), false))
}

/// A rendered page (`getRenderedTemplates` result shape)
#[derive(Clone, Debug)]
pub struct RenderedPage {
    pub input_path: String,
    pub url: Value,
    pub output_path: Value,
    pub content: String,
    pub template_content: String,
    pub data: Value,
}

/// Build a `TemplateMap` from templates and process it (`templateMap.cache()`)
pub fn get_template_map(templates: &[Arc<Template>], ctx: &Arc<BuildContext>) -> Result<TemplateMap> {
    let mut map = TemplateMap::new(ctx.clone());
    for t in templates {
        map.add(t.clone())?;
    }
    map.cache()?;
    Ok(map)
}

/// `getRenderedTemplates(template, data)`: render all pages with layouts and transforms
pub fn render_templates(templates: &[Arc<Template>], ctx: &Arc<BuildContext>) -> Result<Vec<RenderedPage>> {
    let map = get_template_map(templates, ctx)?;
    let mut out = vec![];
    for entry in &map.map {
        for page in &entry.pages {
            if !page.template.is_renderable() {
                continue;
            }
            let content = page.template.render_page_entry(page)?;
            out.push(RenderedPage {
                input_path: page.input_path.clone(),
                url: page.url(),
                output_path: page.output_path(),
                content,
                template_content: page.template_content().cloned().unwrap_or_default(),
                data: page.data(),
            });
        }
    }
    map.dispose();
    Ok(out)
}

pub fn render_template(template: &Arc<Template>) -> Result<Vec<RenderedPage>> {
    let ctx = template.ctx().clone();
    render_templates(std::slice::from_ref(template), &ctx)
}

/// Render a single template file and return its (first page) content
pub fn render_file(path: &str, options: TestOptions) -> Result<String> {
    let tmpl = get_new_template(path, options)?;
    Ok(render_template(&tmpl)?
        .into_iter()
        .next()
        .map(|p| p.content)
        .unwrap_or_default())
}

/// `getNewTemplateRender(name, inputDir)` + `getCompiledTemplate(str)` + render
pub fn render_string(engine_or_path: &str, src: &str, data: Value, options: TestOptions) -> Result<String> {
    let ctx = get_context(options)?;
    let tr = TemplateRender::new(ctx, engine_or_path)?;
    let f = tr.get_compiled_template(src, false, None)?;
    f(&data)
}

/// Data for a template (`template.getData()`)
pub fn get_data(path: &str, options: TestOptions) -> Result<Value> {
    get_new_template(path, options)?.get_data()
}

/// Parse a JSON literal into a `Value` (test convenience)
pub fn json(s: &str) -> Value {
    Value::from_json(serde_json::from_str(s).expect("invalid JSON in test"))
}
