//! Port of `TemplateRender.js`: resolves the engine (and preprocessing steps)
//! used to render a template, and compiles template strings.

use std::sync::Arc;

use anyhow::{Result, anyhow, bail};

use crate::build_context::BuildContext;
use crate::engines::{RenderFn, TemplateEngine};
use crate::template_pipeline as pipeline;

/// A step that runs before the primary engine: an engine name or an engine instance
#[derive(Clone)]
pub enum PipelineStep {
    Name(String),
    Engine(Arc<dyn TemplateEngine>),
}

impl PipelineStep {
    pub fn name(&self) -> String {
        match self {
            PipelineStep::Name(n) => n.clone(),
            PipelineStep::Engine(e) => e.name().to_string(),
        }
    }
}

impl From<&str> for PipelineStep {
    fn from(name: &str) -> Self {
        PipelineStep::Name(name.to_string())
    }
}

impl From<String> for PipelineStep {
    fn from(name: String) -> Self {
        PipelineStep::Name(name)
    }
}

impl From<Arc<dyn TemplateEngine>> for PipelineStep {
    fn from(engine: Arc<dyn TemplateEngine>) -> Self {
        PipelineStep::Engine(engine)
    }
}

fn step_names(steps: &[PipelineStep]) -> Vec<String> {
    steps.iter().map(PipelineStep::name).collect()
}

#[derive(Clone)]
pub struct TemplateRender {
    ctx: Arc<BuildContext>,
    pub engine_name_or_path: String,
    pub engine_name: String,
    pub engine: Arc<dyn TemplateEngine>,
    preceding_steps: Vec<PipelineStep>,
}

/// Resolved steps for a render (honoring `templateEngineOverride`)
#[derive(Clone)]
pub struct ResolvedSteps {
    pub engine_name: String,
    pub engine: Arc<dyn TemplateEngine>,
    pub preceding: Vec<PipelineStep>,
}

impl TemplateRender {
    pub fn new(ctx: Arc<BuildContext>, tmpl_path: &str) -> Result<Self> {
        let (engine_name, engine) = Self::resolve_engine(&ctx, tmpl_path)?;
        let mut tr = TemplateRender {
            ctx,
            engine_name_or_path: tmpl_path.to_string(),
            engine_name,
            engine,
            preceding_steps: vec![],
        };
        let steps = tr.default_preceding_steps()?;
        tr.set_preceding_steps(steps)?;
        Ok(tr)
    }

    /// `#setPrecedingSteps`: validates the full pipeline before applying it
    fn set_preceding_steps(&mut self, steps: Vec<PipelineStep>) -> Result<()> {
        let mut names = step_names(&steps);
        names.push(self.engine_name.clone());
        pipeline::validate(&names)?;
        self.preceding_steps = steps;
        Ok(())
    }

    /// Re-run engine resolution (`init(engineNameOrPath)`)
    fn init(&mut self, name: &str) -> Result<()> {
        let (engine_name, engine) = Self::resolve_engine(&self.ctx, name)?;
        self.engine_name = engine_name;
        self.engine = engine;
        let steps = self.default_preceding_steps()?;
        self.set_preceding_steps(steps)
    }

    /// `setEngineOverride`: e.g. `njk` or `njk,md`
    pub fn set_engine_override(&mut self, engine_name: &str) -> Result<()> {
        let steps = pipeline::parse(engine_name)?;
        let Some((last, preceding)) = steps.split_last() else {
            self.init(pipeline::PLAINTEXT_FORMAT)?;
            return self.set_preceding_steps(vec![]);
        };
        self.init(last)?;
        self.set_preceding_steps(
            preceding
                .iter()
                .map(|s| PipelineStep::from(s.as_str()))
                .collect(),
        )
    }

    /// Replaces the template engine step: an engine name, an engine instance, or `None` to skip it
    pub fn set_preprocessing_engine(&mut self, engine: Option<PipelineStep>) -> Result<()> {
        let mut steps: Vec<PipelineStep> = engine.into_iter().collect();
        steps.extend(
            self.preceding_steps
                .iter()
                .filter(|s| matches!(s, PipelineStep::Name(n) if pipeline::is_format(n)))
                .cloned(),
        );
        self.set_preceding_steps(steps)
    }

    pub fn get_engine_name(&self) -> &str {
        &self.engine_name
    }

    pub fn is_engine(&self, name: &str) -> bool {
        self.engine_name == name
    }

    pub fn input_dir(&self) -> String {
        self.ctx.dirs.input()
    }

    pub fn includes_dir(&self) -> String {
        self.ctx.dirs.includes()
    }

    /// Backwards compat
    pub fn get_includes_dir(&self) -> String {
        self.includes_dir()
    }

    fn resolve_engine(ctx: &BuildContext, name: &str) -> Result<(String, Arc<dyn TemplateEngine>)> {
        let entry = ctx.extension_map.get_extension_entry(name);
        let Some(entry) = entry else {
            bail!(
                "Unknown engine for {name} (supported extensions: {})",
                ctx.extension_map.get_readable_file_extensions()
            );
        };
        let mut engine_name = entry.alias_key.clone().unwrap_or_else(|| entry.key.clone());
        if entry.is_simple_alias {
            engine_name = entry.key.clone();
        }
        let engine = ctx
            .engine_manager
            .get_engine(&engine_name, &ctx.extension_map)?;
        Ok((engine_name, engine))
    }

    fn default_preceding_steps(&self) -> Result<Vec<PipelineStep>> {
        let Some(chain) = pipeline::get_chain(&self.engine_name) else {
            return Ok(vec![]);
        };
        let mut steps = vec![];
        if let Some(pre) = pipeline::get_preprocessor(chain, &self.ctx.config)? {
            steps.push(PipelineStep::Name(pre));
        }
        steps.extend(
            chain[..chain.len() - 1]
                .iter()
                .map(|s| PipelineStep::from(*s)),
        );
        Ok(steps)
    }

    pub fn pipeline_step_names(&self) -> Vec<String> {
        let mut names = step_names(&self.preceding_steps);
        names.push(self.engine_name.clone());
        names
    }

    pub fn get_engine_by_name(&self, name: &str) -> Result<Arc<dyn TemplateEngine>> {
        self.ctx
            .engine_manager
            .get_engine(name, &self.ctx.extension_map)
    }

    /// Steps after applying an engine override (`templateEngineOverride`)
    pub fn resolve(&self, engine_override: Option<&str>) -> Result<ResolvedSteps> {
        let Some(over) = engine_override else {
            return Ok(ResolvedSteps {
                engine_name: self.engine_name.clone(),
                engine: self.engine.clone(),
                preceding: self.preceding_steps.clone(),
            });
        };
        let steps = pipeline::parse(over)?;
        if steps.is_empty() {
            let (name, engine) = Self::resolve_engine(&self.ctx, pipeline::PLAINTEXT_FORMAT)?;
            return Ok(ResolvedSteps {
                engine_name: name,
                engine,
                preceding: vec![],
            });
        }
        let last = steps.last().unwrap();
        let (name, engine) = Self::resolve_engine(&self.ctx, last)?;
        let mut all = steps[..steps.len() - 1].to_vec();
        all.push(name.clone());
        pipeline::validate(&all)?;
        let preceding = steps[..steps.len() - 1]
            .iter()
            .map(|s| PipelineStep::from(s.as_str()))
            .collect();
        Ok(ResolvedSteps {
            engine_name: name,
            engine,
            preceding,
        })
    }

    pub fn get_preprocessor_engine_name(&self) -> Option<String> {
        if let Some(first) = self.preceding_steps.first() {
            return Some(first.name());
        }
        self.ctx.extension_map.get_key(&self.engine_name_or_path)
    }

    /// `getEnginesList` (e.g. `liquid,md`)
    pub fn get_engines_list(&self, engine_override: Option<&str>) -> Result<String> {
        if let Some(over) = engine_override {
            return Ok(pipeline::parse(over)?.join(","));
        }
        if !self.preceding_steps.is_empty() {
            let mut names = step_names(&self.preceding_steps);
            if self.engine_name != pipeline::PLAINTEXT_FORMAT {
                names.push(self.engine_name.clone());
            }
            return Ok(names.join(","));
        }
        Ok(self
            .ctx
            .extension_map
            .get_key(&self.engine_name_or_path)
            .unwrap_or_default())
    }

    pub fn get_readable_engines_list_differing_from_file_extension(&self) -> Option<String> {
        let key = self.ctx.extension_map.get_key(&self.engine_name_or_path);
        if self.engine.is_custom() {
            return match self.engine.custom_name() {
                Some(n) if Some(&n) != key.as_ref() => Some(n),
                _ => None,
            };
        }
        if !self.preceding_steps.is_empty() {
            return Some(step_names(&self.preceding_steps).join(","));
        }
        if key.as_deref() != Some(self.engine_name.as_str()) {
            return Some(self.engine_name.clone());
        }
        None
    }

    pub fn get_readable_engines_list(&self) -> String {
        self.get_readable_engines_list_differing_from_file_extension()
            .unwrap_or_else(|| self.engine_name.clone())
    }

    /// All steps in execution order (the primary engine last), with their names
    fn all_steps(resolved: &ResolvedSteps) -> Vec<(String, PipelineStep)> {
        let mut steps: Vec<(String, PipelineStep)> = resolved
            .preceding
            .iter()
            .map(|s| (s.name(), s.clone()))
            .collect();
        steps.push((
            resolved.engine_name.clone(),
            PipelineStep::Engine(resolved.engine.clone()),
        ));
        steps
    }

    /// Steps that are template engines (not output formats), for permalinks/computed data
    fn template_only_steps(&self, resolved: &ResolvedSteps) -> Result<Vec<PipelineStep>> {
        let template_steps: Vec<PipelineStep> = Self::all_steps(resolved)
            .into_iter()
            .filter(|(name, _)| !pipeline::is_format(name))
            .map(|(_, step)| step)
            .collect();
        if template_steps.is_empty() {
            // e.g. permalinks with `templateEngineOverride: md` use the format’s default preprocessor (#446)
            if let Some(chain) = pipeline::get_chain(&resolved.engine_name) {
                return Ok(pipeline::get_preprocessor(chain, &self.ctx.config)?
                    .into_iter()
                    .map(PipelineStep::Name)
                    .collect());
            }
        }
        Ok(template_steps)
    }

    fn get_step_engine(&self, step: &PipelineStep) -> Result<Arc<dyn TemplateEngine>> {
        match step {
            PipelineStep::Name(name) => self.get_engine_by_name(name),
            PipelineStep::Engine(engine) => Ok(engine.clone()),
        }
    }

    pub fn get_template_only_engine(
        &self,
        engine_override: Option<&str>,
    ) -> Result<Option<Arc<dyn TemplateEngine>>> {
        let resolved = self.resolve(engine_override)?;
        match self.template_only_steps(&resolved)?.first() {
            Some(step) => Ok(Some(self.get_step_engine(step)?)),
            None => Ok(None),
        }
    }

    /// `getCompiledTemplate`; `template_only` skips output format steps (e.g. Markdown)
    pub fn get_compiled_template(
        &self,
        src: &str,
        template_only: bool,
        engine_override: Option<&str>,
    ) -> Result<RenderFn> {
        let resolved = self.resolve(engine_override)?;
        let steps: Vec<PipelineStep> = if template_only {
            self.template_only_steps(&resolved)?
        } else {
            Self::all_steps(&resolved)
                .into_iter()
                .map(|(_, step)| step)
                .collect()
        };
        if steps.is_empty() {
            let s = src.to_string();
            return Ok(Arc::new(move |_| Ok(s.clone())));
        }
        let engines: Vec<Arc<dyn TemplateEngine>> = steps
            .iter()
            .map(|step| self.get_step_engine(step))
            .collect::<Result<_>>()?;
        let input_path = self.engine_name_or_path.clone();
        let first = engines[0].compile(src, Some(&input_path))?;
        if engines.len() == 1 {
            return Ok(first);
        }
        let rest: Vec<Arc<dyn TemplateEngine>> = engines[1..].to_vec();
        for engine in &rest {
            engine.ready()?;
        }
        // Later steps compile the previous step’s output on every render
        Ok(Arc::new(move |data| {
            let mut content = first(data)?;
            for engine in &rest {
                let f = engine
                    .compile(&content, Some(&input_path))
                    .map_err(|e| anyhow!(e))?;
                content = f(data)?;
            }
            Ok(content)
        }))
    }
}
