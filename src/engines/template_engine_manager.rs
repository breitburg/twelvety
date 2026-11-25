//! Port of `Engines/TemplateEngineManager.js`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{Result, bail};

use super::custom::CustomEngine;
use super::html::Html;
use super::liquid::LiquidEngine;
use super::markdown::MarkdownEngine;
use super::nunjucks::NunjucksEngine;
use super::TemplateEngine;
use crate::extension_map::ExtensionMap;
use crate::global_dependency_map::GlobalDependencyMap;
use crate::template_config::Config;
use crate::util::project_directories::ProjectDirectories;

pub struct TemplateEngineManager {
    config: Arc<Config>,
    dirs: ProjectDirectories,
    uses: Arc<Mutex<GlobalDependencyMap>>,
    cache: Mutex<HashMap<String, Arc<dyn TemplateEngine>>>,
}

fn builtin_class(key: &str) -> Option<&'static str> {
    match key {
        "md" => Some("Markdown"),
        "html" => Some("Html"),
        "njk" => Some("Nunjucks"),
        "liquid" => Some("Liquid"),
        "12ty.js" => Some("JavaScript"),
        _ => None,
    }
}

impl TemplateEngineManager {
    pub fn new(config: Arc<Config>, dirs: ProjectDirectories, uses: Arc<Mutex<GlobalDependencyMap>>) -> Self {
        TemplateEngineManager {
            config,
            dirs,
            uses,
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// `keyToClassNameMap`
    fn class_name_for(&self, key: &str) -> Option<String> {
        for entry in &self.config.extension_map {
            let alias = entry.alias_key.as_deref();
            if alias == Some(key) {
                if entry.is_simple_alias {
                    return builtin_class(&entry.key)
                        .map(String::from)
                        .or_else(|| self.class_name_for(&entry.key));
                }
                return Some("Custom".into());
            }
            if alias.is_none() && entry.key == key {
                return Some("Custom".into());
            }
        }
        builtin_class(key).map(String::from)
    }

    pub fn has_engine(&self, name: &str) -> bool {
        self.class_name_for(name).is_some()
    }

    pub fn reset(&self) {
        self.cache.lock().unwrap().clear();
    }

    pub fn get_engine(&self, name: &str, extension_map: &ExtensionMap) -> Result<Arc<dyn TemplateEngine>> {
        if ["ejs", "hbs", "mustache", "haml", "pug"].contains(&name) && !self.has_engine(name) {
            bail!(
                "Per the 2023 community survey, the \"{name}\" template language was moved from core to an officially supported plugin in v3.0. These plugins live here: https://github.com/11ty/eleventy-plugin-template-languages and are documented on their respective template language docs at https://v3.11ty.dev/docs/languages/ You are also empowered to implement *any* template language yourself using https://v3.11ty.dev/docs/languages/custom/"
            );
        }
        if let Some(e) = self.cache.lock().unwrap().get(name) {
            return Ok(e.clone());
        }
        let engine = self.create_engine(name, extension_map)?;
        self.cache
            .lock()
            .unwrap()
            .insert(name.to_string(), engine.clone());
        Ok(engine)
    }

    fn create_builtin(&self, class: &str, name: &str) -> Result<Arc<dyn TemplateEngine>> {
        Ok(match class {
            "Markdown" => Arc::new(MarkdownEngine::new(name, &self.config)),
            "Html" => Arc::new(Html::new(name)),
            "Nunjucks" => Arc::new(NunjucksEngine::new(name, &self.config, &self.dirs, self.uses.clone())),
            "Liquid" => Arc::new(LiquidEngine::new(name, &self.config, &self.dirs, self.uses.clone())),
            "JavaScript" => bail!(
                "JavaScript templates (`*.12ty.js`) are not supported in the Rust port. Use `addExtension` with a Rust `compile` function instead."
            ),
            other => bail!("Template Engine {other} does not exist in getEngine()"),
        })
    }

    fn create_engine(&self, name: &str, extension_map: &ExtensionMap) -> Result<Arc<dyn TemplateEngine>> {
        let Some(class) = self.class_name_for(name) else {
            bail!("Template Engine {name} does not exist in getEngine()");
        };
        if class != "Custom" {
            return self.create_builtin(&class, name);
        }
        // Custom engine
        let entry = self
            .config
            .extension_map
            .iter()
            .find(|e| e.alias_key.as_deref().unwrap_or(&e.key) == name)
            .cloned();
        let Some(entry) = entry else {
            bail!("Could not find a custom extension for {name}. Did you add it to your config file?");
        };
        let mut default_engine = None;
        if entry.alias_key.is_some() && entry.key != name {
            // alias of an existing engine (e.g. `addExtension("12ty.jsx", { key: "12ty.js" })`)
            default_engine = Some(self.get_engine(&entry.key, extension_map)?);
        } else if let Some(class) = builtin_class(name) {
            // override of a built-in syntax, e.g. addExtension("md", { compile })
            if class != "JavaScript" {
                default_engine = Some(self.create_builtin(class, name)?);
            }
        }
        Ok(Arc::new(CustomEngine::new(name, entry, default_engine)))
    }
}
