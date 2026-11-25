//! Port of `Plugins/*.js`.

pub mod html_base;
pub mod html_relative_copy;
pub mod html_transformer;
pub mod i18n;
pub mod id_attribute;
pub mod input_path_to_url;
pub mod pagination;
pub mod render;

use anyhow::{Result, bail};

use crate::user_config::{PluginFn, UserConfig};
use crate::value::Value;
use std::sync::Arc;

/// `PreserveClosingTagsPlugin`: our HTML transformer preserves the source
/// markup (it doesn't re-serialize), so closing tags are always preserved.
pub fn preserve_closing_tags(_config: &mut UserConfig, options: &Value) -> Result<()> {
    if !options.get("tags").is_undefined() && !options.get("tags").is_array() {
        bail!("`tags` passed to the Preserve Closing Tags plugin must be an Array");
    }
    Ok(())
}

/// The canonical name of a built-in plugin marked `unique` upstream (`twelvetyPluginOptions.unique`):
/// only the first `addPlugin` of a unique plugin runs
pub fn unique_plugin_name(name: &str) -> Option<&'static str> {
    Some(match name {
        "html-base" | "HtmlBasePlugin" | "@12ty/twelvety/html-base-plugin" => "@12ty/twelvety/html-base-plugin",
        "i18n" | "I18nPlugin" | "@12ty/twelvety/i18n-plugin" => "@12ty/twelvety/i18n-plugin",
        "input-path-to-url" | "InputPathToUrlTransformPlugin" => "@12ty/twelvety/inputpath-to-url-plugin",
        "render" | "RenderPlugin" | "TwelvetyRenderPlugin" => "@12ty/twelvety/render-plugin",
        _ => return None,
    })
}

/// Resolve a built-in plugin by name (used by declarative configuration files)
pub fn resolve_builtin(name: &str) -> Option<PluginFn> {
    let f: PluginFn = match name {
        "html-base" | "HtmlBasePlugin" | "@12ty/twelvety/html-base-plugin" => Arc::new(html_base::plugin),
        "id-attribute" | "IdAttributePlugin" => Arc::new(id_attribute::plugin),
        "i18n" | "I18nPlugin" | "@12ty/twelvety/i18n-plugin" => Arc::new(i18n::plugin),
        "input-path-to-url" | "InputPathToUrlTransformPlugin" => {
            Arc::new(input_path_to_url::transform_plugin)
        }
        "render" | "RenderPlugin" | "TwelvetyRenderPlugin" => Arc::new(render::plugin),
        "preserve-closing-tags" | "PreserveClosingTagsPlugin" => Arc::new(preserve_closing_tags),
        _ => return None,
    };
    Some(f)
}
