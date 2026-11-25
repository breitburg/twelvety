//! Port of `Engines/Liquid.js`: wires the Liquid library into Twelvety.

use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};

use super::{RenderFn, TemplateEngine};
use crate::global_dependency_map::GlobalDependencyMap;
use crate::liquid::lexer::{Token, tokenize};
use crate::liquid::{Liquid, LiquidOptions};
use crate::template_config::Config;
use crate::util::project_directories::ProjectDirectories;
use crate::util::{node_path, template_path};
use crate::value::{Map, Value};

pub struct LiquidEngine {
    name: String,
    lib: Arc<Liquid>,
    globals: Value,
}

impl LiquidEngine {
    pub fn new(
        name: &str,
        config: &Config,
        dirs: &ProjectDirectories,
        uses: Arc<Mutex<GlobalDependencyMap>>,
    ) -> Self {
        let user_options = &config.liquid_options;
        let mut options = LiquidOptions::default();
        if let Some(zone) = &config.date_time_zone {
            options.timezone = Some(zone.zone.clone());
        }
        match user_options.get("timezoneOffset") {
            Value::Number(n) => options.timezone_offset = -(n as i32),
            Value::String(s) => options.timezone = Some(s.to_string()),
            _ => {}
        }
        if let Some(b) = user_options.get("strictFilters").as_bool() {
            options.strict_filters = b;
        }
        if let Some(b) = user_options.get("jsTruthy").as_bool() {
            options.js_truthy = b;
        }
        if let Some(b) = user_options.get("dynamicPartials").as_bool() {
            options.dynamic_partials = b;
        }
        let mut lib = Liquid::new(options);

        // root: [includes, input]; layouts: [layouts, ...root]
        let mut roots: Vec<String> = vec![];
        if let Some(l) = dirs.layouts() {
            roots.push(l);
        }
        roots.push(dirs.includes());
        roots.push(dirs.input());
        if let Value::Array(extra) = user_options.get("root") {
            for r in extra.iter() {
                roots.push(r.to_output_string());
            }
        }
        let extname = user_options
            .get("extname")
            .as_str()
            .unwrap_or(".liquid")
            .to_string();
        lib.partial_loader = Some(Arc::new(move |name: &str, from: Option<&str>| {
            let file = if node_path::extname(name).is_empty() {
                format!("{name}{extname}")
            } else {
                name.to_string()
            };
            let mut candidates = vec![];
            if let Some(from) = from
                && (file.starts_with("./") || file.starts_with("../"))
                && !from.is_empty()
            {
                let dir = template_path::get_dir_from_file_path(from);
                candidates.push(template_path::join(&[&dir, &file]));
            }
            for root in &roots {
                candidates.push(template_path::join(&[root, &file]));
            }
            for candidate in &candidates {
                let path = std::path::Path::new(candidate);
                if path.is_file() {
                    let contents = std::fs::read_to_string(path)
                        .with_context(|| format!("Failed to read {candidate}"))?;
                    return Ok((template_path::add_leading_dot_slash(candidate), contents));
                }
            }
            bail!("Failed to lookup \"{name}\" in \"{}\"", roots.join(","))
        }));
        let engine_name = name.to_string();
        lib.on_partial = Some(Arc::new(move |from, to| {
            let from = from.filter(|f| !f.is_empty()).unwrap_or(&engine_name);
            uses.lock().unwrap().add_dependency(from, &[to.to_string()]);
        }));

        for (name, f) in &config.liquid.filters {
            lib.filters.insert(name.clone(), f.clone());
        }
        for (name, f) in &config.liquid.shortcodes {
            lib.shortcodes.insert(name.clone(), f.clone());
        }
        lib.raw_paired_shortcodes = config.raw_paired_shortcodes.iter().cloned().collect();
        for (name, f) in &config.liquid.paired_shortcodes {
            lib.paired_shortcodes.insert(name.clone(), f.clone());
        }

        LiquidEngine {
            name: name.into(),
            lib: Arc::new(lib),
            globals: user_options.get("globals"),
        }
    }

    pub fn lib(&self) -> Arc<Liquid> {
        self.lib.clone()
    }
}

impl TemplateEngine for LiquidEngine {
    fn name(&self) -> &str {
        &self.name
    }

    fn cacheable(&self) -> bool {
        true
    }

    fn needs_compilation(&self, src: &str) -> bool {
        src.contains("{%") || src.contains("{{")
    }

    fn parse_for_symbols(&self, src: &str) -> Option<Vec<String>> {
        let tokens = tokenize(src).ok()?;
        Some(
            tokens
                .into_iter()
                .filter_map(|t| match t {
                    Token::Output { content, .. } => {
                        Some(content.split('|').next().unwrap_or("").trim().to_string())
                    }
                    _ => None,
                })
                .collect(),
        )
    }

    fn compile(&self, src: &str, input_path: Option<&str>) -> Result<RenderFn> {
        let file = match input_path {
            None | Some("liquid") | Some("md") => None,
            Some(p) => Some(p),
        };
        let tmpl = self.lib.parse(src, file)?;
        let lib = self.lib.clone();
        let user_globals = self.globals.clone();
        Ok(Arc::new(move |data: &Value| {
            let mut globals = Map::new();
            for key in ["page", "twelvety", "buildawesome", "collections"] {
                globals.insert(key.to_string(), data.get(key));
            }
            if let Value::Object(g) = &user_globals {
                for (k, v) in g.iter() {
                    globals.insert(k.clone(), v.clone());
                }
            }
            lib.render(&tmpl, data, &Value::from_map(globals))
        }))
    }
}
