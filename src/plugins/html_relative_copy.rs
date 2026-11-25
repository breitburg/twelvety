//! Port of `Plugins/HtmlRelativeCopyPlugin.js` and `Util/HtmlRelativeCopy.js`.

use std::sync::Arc;

use anyhow::{Result, bail};

use super::html_transformer::AddOptions;
use crate::user_config::UserConfig;
use crate::util::glob_matcher::is_glob_match;
use crate::util::{node_path, template_path, url_util::is_valid_url};
use crate::value::{Map, Value};

struct HtmlRelativeCopy {
    matching_globs: Vec<String>,
    paths: Vec<String>,
    fail_on_error: bool,
    copy_options: Value,
}

impl HtmlRelativeCopy {
    fn is_skippable_href(&self, raw: &str) -> bool {
        self.matching_globs.is_empty()
            || raw.is_empty()
            || node_path::is_absolute(raw)
            || is_valid_url(raw)
    }

    fn is_copyable_target(&self, target: &str) -> bool {
        is_glob_match(target, &self.matching_globs)
    }

    fn get_aliased_path(&self, r: &str) -> Option<String> {
        for dir in &self.paths {
            let found = template_path::join(&[dir, r]);
            if self.is_copyable_target(&found) && std::path::Path::new(&found).exists() {
                return Some(found);
            }
        }
        None
    }

    fn relative_to(&self, r: &str, context_file_path: &str) -> String {
        let dir = template_path::get_dir_from_file_path(context_file_path);
        template_path::join(&[&dir, r])
    }

    fn copy(
        &self,
        config_events: &crate::util::event_emitter::EventEmitter,
        file_ref: &str,
        input_path: &str,
        output_path: &str,
    ) -> Result<()> {
        // strip query/hash
        let file_ref = file_ref.split(['?', '#']).next().unwrap_or("");
        if self.is_skippable_href(file_ref) {
            return Ok(());
        }
        let mut source = self.relative_to(file_ref, input_path);
        if !self.is_copyable_target(&source) {
            return Ok(());
        }
        if !std::path::Path::new(&source).exists() {
            match self.get_aliased_path(file_ref) {
                Some(alias) => source = alias,
                None => {
                    if self.fail_on_error {
                        bail!(
                            "Missing input file for `html-relative` Passthrough Copy file: {}",
                            template_path::absolute_path(&[&source])
                        );
                    }
                    return Ok(());
                }
            }
        }
        let target = self.relative_to(file_ref, output_path);
        let mut arg = Map::new();
        arg.insert("source".into(), Value::str(source));
        arg.insert("target".into(), Value::str(target));
        arg.insert("options".into(), self.copy_options.clone());
        config_events.emit("buildawesome#copy", &Value::from_map(arg))
    }
}

pub fn register(config: &mut UserConfig) -> Result<()> {
    let entries = config.passthrough_copies_html_relative.clone();
    for entry in entries {
        let options = &entry.options;
        let extensions = options
            .get("extensions")
            .as_str()
            .unwrap_or("html")
            .to_string();
        let paths: Vec<String> = options
            .get("paths")
            .to_vec()
            .iter()
            .map(|p| template_path::get_dir(&p.to_output_string()))
            .collect();
        let mut copy_options = Map::new();
        copy_options.insert("dot".into(), Value::Bool(false));
        if let Value::Object(o) = options.get("copyOptions") {
            for (k, v) in o.iter() {
                copy_options.insert(k.clone(), v.clone());
            }
        }
        let rel = Arc::new(HtmlRelativeCopy {
            matching_globs: entry.matches.clone(),
            paths,
            fail_on_error: options.get("failOnError").as_bool().unwrap_or(true),
            copy_options: Value::from_map(copy_options),
        });
        let events = config.events.clone();
        let enabled = !rel.matching_globs.is_empty();
        config.html_transformer.add_url_transform(
            &extensions,
            Arc::new(move |ctx, target, _| {
                rel.copy(
                    &events,
                    target,
                    &ctx.page.get("inputPath").to_output_string(),
                    &ctx.page.get("outputPath").to_output_string(),
                )?;
                Ok(target.to_string())
            }),
            AddOptions {
                name: None,
                priority: -1,
                enabled: Some(Arc::new(move |_| enabled)),
            },
        );
    }
    Ok(())
}
