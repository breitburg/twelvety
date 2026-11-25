//! Declarative configuration files (`buildawesome.config.json` / `.toml` / `.yaml`).
//!
//! Upstream configuration files are JavaScript modules calling the
//! `twelvetyConfig` API. This applies the serializable subset of that API from
//! a data file:
//!
//! ```toml
//! pathPrefix = "/docs/"
//! templateFormats = ["md", "njk", "html"]
//! markdownTemplateEngine = "njk"
//! passthroughCopy = ["img", { "src/fonts" = "fonts" }]
//! dataExtensions = ["yaml", "toml"]
//!
//! [dir]
//! input = "src"
//! output = "dist"
//!
//! [[plugins]]
//! name = "id-attribute"
//! ```

use std::sync::Arc;

use anyhow::{Result, anyhow, bail};

use crate::plugins;
use crate::user_config::UserConfig;
use crate::value::Value;

fn string_list(v: &Value) -> Vec<String> {
    match v {
        Value::String(s) => s.split(',').map(|x| x.trim().to_string()).collect(),
        Value::Array(a) => a.iter().map(|x| x.to_output_string()).collect(),
        _ => vec![],
    }
}

/// Built-in data file parsers usable via `dataExtensions`
pub fn builtin_data_parser(ext: &str) -> Option<crate::user_config::DataParserFn> {
    let f: crate::user_config::DataParserFn = match ext {
        "yaml" | "yml" => Arc::new(|content, _| {
            if content.trim().is_empty() {
                return Ok(Value::object());
            }
            Ok(Value::from_yaml(serde_yaml::from_str(content)?))
        }),
        "toml" => Arc::new(|content, _| Ok(Value::from_toml(toml::from_str(content)?))),
        "json5" | "jsonc" => Arc::new(|content, _| {
            Ok(Value::from_json(serde_json::from_str(&strip_json_comments(content))?))
        }),
        "txt" => Arc::new(|content, _| Ok(Value::str(content))),
        _ => return None,
    };
    Some(f)
}

fn strip_json_comments(s: &str) -> String {
    let re = regex::Regex::new(r#"(?m)("(?:\\.|[^"\\])*")|//[^\n]*|/\*(?s:.*?)\*/"#).unwrap();
    re.replace_all(s, |c: &regex::Captures| {
        c.get(1).map(|m| m.as_str().to_string()).unwrap_or_default()
    })
    .to_string()
}

pub fn apply(config: &mut UserConfig, file: &Value) -> Result<()> {
    if !file.is_object() {
        bail!("Configuration file must contain an object");
    }
    let formats = file.get("templateFormats");
    if !formats.is_undefined() {
        config.template_formats = Some(string_list(&formats));
    }
    for f in string_list(&file.get("addTemplateFormats")) {
        config.template_formats_added.push(vec![f]);
    }
    if let Some(p) = file.get("pathPrefix").as_str() {
        config.set_path_prefix(p);
    }
    for (key, target) in [
        ("markdownTemplateEngine", "md"),
        ("htmlTemplateEngine", "html"),
    ] {
        match file.get(key) {
            Value::String(s) => {
                if target == "md" {
                    config.set_markdown_template_engine(s.to_string());
                } else {
                    config.set_html_template_engine(s.to_string());
                }
            }
            Value::Bool(false) => {
                if target == "md" {
                    config.set_markdown_template_engine("html");
                } else {
                    config.set_html_template_engine("html");
                }
            }
            _ => {}
        }
    }
    match file.get("passthroughCopy") {
        Value::Undefined => {}
        Value::String(s) => {
            config.add_passthrough_copy(&s);
        }
        Value::Array(entries) => {
            for e in entries.iter() {
                match e {
                    Value::String(s) => {
                        config.add_passthrough_copy(s);
                    }
                    Value::Object(m) => {
                        for (k, v) in m.iter() {
                            config.add_passthrough_copy_mapping(k, &v.to_output_string());
                        }
                    }
                    _ => {}
                }
            }
        }
        Value::Object(m) => {
            for (k, v) in m.iter() {
                if v.as_bool() == Some(true) {
                    config.add_passthrough_copy(k);
                } else {
                    config.add_passthrough_copy_mapping(k, &v.to_output_string());
                }
            }
        }
        _ => {}
    }
    if let Value::Object(m) = file.get("globalData") {
        for (k, v) in m.iter() {
            config.add_global_data(k, v.clone());
        }
    }
    if let Value::Object(m) = file.get("layoutAliases") {
        for (k, v) in m.iter() {
            config.add_layout_alias(k, &v.to_output_string());
        }
    }
    if let Some(b) = file.get("layoutResolution").as_bool() {
        config.set_layout_resolution(b);
    }
    let suffixes = file.get("dataFileSuffixes");
    if suffixes.is_array() {
        let list = string_list(&suffixes);
        config.data_file_suffixes_override = Some(list);
    }
    if let Some(s) = file.get("dataFileDirBaseNameOverride").as_str() {
        config.set_data_file_base_name(s);
    }
    if let Value::Object(keys) = file.get("keys") {
        for (k, v) in keys.iter() {
            let value = v.as_str().map(String::from);
            match k.as_str() {
                "package" => config.keys.package = value,
                "layout" => config.keys.layout = value.unwrap_or_default(),
                "tags" => config.keys.tags = value,
                "permalink" => config.keys.permalink = value.unwrap_or_default(),
                "permalinkRoot" => config.keys.permalink_root = value.unwrap_or_default(),
                "engineOverride" => config.keys.engine_override = value.unwrap_or_default(),
                "computed" => config.keys.computed = value.unwrap_or_default(),
                "dataSchema" => config.keys.data_schema = value.unwrap_or_default(),
                "import" => config.keys.import = value.unwrap_or_default(),
                _ => {}
            }
        }
    }
    for i in string_list(&file.get("ignores")) {
        config.add_ignore(&i);
    }
    for i in string_list(&file.get("watchIgnores")) {
        config.add_watch_ignore(&i);
    }
    for t in string_list(&file.get("watchTargets")) {
        config.add_watch_target(&t);
    }
    if let Some(b) = file.get("dynamicPermalinks").as_bool() {
        config.set_dynamic_permalinks(b);
    }
    if let Some(b) = file.get("useGitIgnore").as_bool() {
        config.set_use_git_ignore(b);
    }
    if let Some(b) = file.get("quietMode").as_bool() {
        config.set_quiet_mode(b);
    }
    if let Some(b) = file.get("permalinkCollapseMatchingFolder").as_bool() {
        config.set_permalink_collapse_matching_folder(b);
    }
    if let Some(b) = file.get("freezeReservedData").as_bool() {
        config.set_freeze_reserved_data(b);
    }
    if let Some(z) = file.get("dateTimeZone").as_str() {
        config.set_date_time_zone(z)?;
    }
    if let Value::Object(m) = file.get("errorReporting") {
        for (k, v) in m.iter() {
            let value = match v {
                Value::Bool(true) => "allow".to_string(),
                Value::Bool(false) => "error".to_string(),
                other => other.to_output_string(),
            };
            config.configure_error_reporting(k, &value);
        }
    }
    let server = file.get("serverOptions");
    if server.is_object() {
        config.set_server_options(server);
    }
    let md = file.get("markdownOptions");
    if md.is_object() {
        config.set_markdown_options(md);
    }
    let liquid = file.get("liquidOptions");
    if liquid.is_object() {
        config.set_liquid_options(liquid);
    }
    let njk = file.get("nunjucksEnvironmentOptions");
    if njk.is_object() {
        config.set_nunjucks_environment_options(njk);
    }
    let fm = file.get("frontMatterParsingOptions");
    if fm.is_object() {
        config.set_front_matter_parsing_options(fm);
    }
    for ext in string_list(&file.get("dataExtensions")) {
        let parser = builtin_data_parser(&ext)
            .ok_or_else(|| anyhow!("Unknown data extension in configuration file: {ext}"))?;
        config.add_data_extension_with_options(&ext, parser, true);
    }
    for selector in string_list(&file.get("dataFilterSelectors")) {
        config.data_filter_selectors.insert(selector);
    }
    if let Value::Object(m) = file.get("virtualTemplates") {
        for (path, def) in m.iter() {
            match def {
                Value::String(s) => config.add_template(path, s, Value::object())?,
                Value::Object(_) => config.add_template(
                    path,
                    &def.get("content").to_output_string(),
                    def.get("data"),
                )?,
                _ => {}
            }
        }
    }
    if let Value::Array(list) = file.get("plugins") {
        for p in list.iter() {
            let (name, options) = match p {
                Value::String(s) => (s.to_string(), Value::object()),
                Value::Object(_) => (p.get("name").to_output_string(), {
                    let o = p.get("options");
                    if o.is_undefined() { Value::object() } else { o }
                }),
                _ => continue,
            };
            let plugin = plugins::resolve_builtin(&name)
                .ok_or_else(|| anyhow!("Unknown plugin in configuration file: {name}"))?;
            config.add_plugin_fn(&name, plugin, options)?;
        }
    }
    Ok(())
}
