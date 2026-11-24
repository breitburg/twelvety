//! Port of `Plugins/InputPathToUrl.js`.

use std::sync::{Arc, RwLock};

use anyhow::{Result, bail};

use super::html_transformer::AddOptions;
use crate::template_config::ConfigHandle;
use crate::user_config::UserConfig;
use crate::util::{node_path, template_path, url_util::is_valid_url};
use crate::value::Value;

type ContentMap = Arc<RwLock<Value>>;

fn get_valid_path(content_map: &Value, test_path: &str) -> Option<String> {
    let decoded = percent_encoding::percent_decode_str(test_path)
        .decode_utf8_lossy()
        .to_string();
    let normalized = template_path::add_leading_dot_slash(&decoded);
    if content_map.get(&normalized).is_truthy() {
        Some(normalized)
    } else {
        None
    }
}

fn normalize_input_path(
    target: &str,
    input_dir: &str,
    source_input_path: &str,
    content_map: &Value,
) -> String {
    if template_path::join(&[target]).starts_with(&template_path::join(&[input_dir])) {
        if let Some(p) = get_valid_path(content_map, target) {
            return p;
        }
    }
    if let Some(p) = get_valid_path(content_map, &template_path::join(&[input_dir, target])) {
        return p;
    }
    if !target.is_empty() && !node_path::is_absolute(target) {
        let source_dir = template_path::get_dir_from_file_path(source_input_path);
        if let Some(p) = get_valid_path(content_map, &template_path::join(&[&source_dir, target]))
        {
            return p;
        }
    }
    target.to_string()
}

/// Split `path?search#hash` into (suffix, path)
fn parse_file_path(filepath: &str) -> (String, String) {
    if filepath.starts_with('#') || filepath.starts_with('?') {
        return (filepath.to_string(), String::new());
    }
    let hash_idx = filepath.find('#');
    let search_idx = filepath.find('?').filter(|i| hash_idx.map(|h| *i < h).unwrap_or(true));
    let cut = match (search_idx, hash_idx) {
        (Some(s), _) => Some(s),
        (None, Some(h)) => Some(h),
        _ => None,
    };
    match cut {
        Some(i) => (filepath[i..].to_string(), filepath[..i].to_string()),
        None => (String::new(), filepath.to_string()),
    }
}

fn listen_for_content_map(config: &mut UserConfig) -> ContentMap {
    let map: ContentMap = Arc::new(RwLock::new(Value::Undefined));
    let m = map.clone();
    config.on("buildawesome.contentmap", move |arg| {
        *m.write().unwrap() = arg.get("inputPathToUrl");
        Ok(())
    });
    map
}

fn input_dir(handle: &ConfigHandle) -> String {
    handle
        .get()
        .map(|c| c.directories.get("input").to_output_string())
        .unwrap_or_else(|| "./".into())
}

/// `InputPathToUrlFilterPlugin`
pub fn register_filter(config: &mut UserConfig, handle: ConfigHandle) -> Result<()> {
    let content_map = listen_for_content_map(config);
    config.add_filter("inputPathToUrl", move |ctx, target, _| {
        let map = content_map.read().unwrap().clone();
        if map.is_undefined() {
            bail!("Internal error: contentMap not available for `inputPathToUrl` filter.");
        }
        let target = target.to_output_string();
        if is_valid_url(&target) {
            return Ok(Value::str(target));
        }
        let (suffix, mut target) = parse_file_path(&target);
        if !target.is_empty() {
            target = normalize_input_path(
                &target,
                &input_dir(&handle),
                &ctx.page().get("inputPath").to_output_string(),
                &map,
            );
        }
        let urls = map.get(&target);
        let first = urls.get_index(0);
        if !urls.is_truthy() || urls.len() == 0 {
            bail!("`inputPathToUrl` filter could not find a matching target for {target}");
        }
        Ok(Value::str(format!("{}{}", first.to_output_string(), suffix)))
    });
    Ok(())
}

/// `InputPathToUrlTransformPlugin`
pub fn transform_plugin(config: &mut UserConfig, options: &Value) -> Result<()> {
    let extensions = options
        .get("extensions")
        .as_str()
        .unwrap_or("html")
        .to_string();
    let content_map = listen_for_content_map(config);
    let input_dir = config.directories.get("input").to_output_string();
    let dirs = config.directories.clone();
    config.html_transformer.add_url_transform(
        &extensions,
        Arc::new(move |ctx, target, _| {
            let map = content_map.read().unwrap().clone();
            if map.is_undefined() {
                bail!("Internal error: contentMap not available for the `pathToUrl` Transform.");
            }
            if is_valid_url(target) {
                return Ok(target.to_string());
            }
            let input_dir = if input_dir.is_empty() {
                dirs.get("input").to_output_string()
            } else {
                input_dir.clone()
            };
            let (suffix, mut target) = parse_file_path(target);
            if !target.is_empty() {
                target = normalize_input_path(
                    &target,
                    &input_dir,
                    &ctx.page.get("inputPath").to_output_string(),
                    &map,
                );
            }
            let urls = map.get(&target);
            if target.is_empty() || !urls.is_truthy() || urls.len() == 0 {
                return Ok(format!("{target}{suffix}"));
            }
            Ok(format!("{}{}", urls.get_index(0).to_output_string(), suffix))
        }),
        AddOptions::default(),
    );
    Ok(())
}
