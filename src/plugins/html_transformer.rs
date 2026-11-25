//! Port of `Util/HtmlTransformer.js`.
//!
//! Upstream uses posthtml (plus `posthtml-urls`). Here URL rewriting is done with
//! `lol_html`, which streams and only touches modified attributes. posthtml
//! plugins become content-level callbacks (`HtmlPlugin`).

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{Result, anyhow};
use lol_html::{HtmlRewriter, Settings, element};

use crate::template_config::ConfigHandle;
use crate::user_config::{TransformContext, UserConfig};
use crate::util::template_path;

/// Where a URL was found in the markup
#[derive(Clone, Debug)]
pub struct UrlContext {
    pub attribute: String,
    pub tag: String,
}

pub type UrlCallback =
    Arc<dyn Fn(&TransformContext, &str, &UrlContext) -> Result<String> + Send + Sync>;
pub type HtmlPlugin = Arc<dyn Fn(&TransformContext, String) -> Result<String> + Send + Sync>;
pub type EnabledFn = Arc<dyn Fn(&TransformContext) -> bool + Send + Sync>;

#[derive(Clone)]
struct Entry<T> {
    name: Option<String>,
    f: T,
    priority: i32,
    enabled: Option<EnabledFn>,
}

#[derive(Default)]
struct Inner {
    callbacks: HashMap<String, Vec<Entry<UrlCallback>>>,
    plugins: HashMap<String, Vec<Entry<HtmlPlugin>>>,
}

#[derive(Clone, Default)]
pub struct HtmlTransformer(Arc<Mutex<Inner>>);

#[derive(Clone, Default)]
pub struct AddOptions {
    pub name: Option<String>,
    pub priority: i32,
    pub enabled: Option<EnabledFn>,
}

/// Tag/attribute pairs that contain URLs (from `posthtml-urls`)
const URL_ATTRIBUTES: &[(&str, &[&str])] = &[
    ("a", &["href", "ping"]),
    ("applet", &["archive", "code", "codebase", "object", "src"]),
    ("area", &["href", "ping"]),
    ("audio", &["src"]),
    ("base", &["href"]),
    ("blockquote", &["cite"]),
    ("body", &["background"]),
    ("button", &["formaction"]),
    ("del", &["cite"]),
    ("embed", &["src"]),
    ("form", &["action"]),
    ("frame", &["longdesc", "src"]),
    ("head", &["profile"]),
    ("html", &["manifest"]),
    ("iframe", &["longdesc", "src"]),
    ("img", &["longdesc", "src", "srcset"]),
    ("input", &["formaction", "src"]),
    ("ins", &["cite"]),
    ("link", &["href"]),
    ("menuitem", &["icon"]),
    ("meta", &["content"]),
    ("object", &["codebase", "data"]),
    ("q", &["cite"]),
    ("script", &["src"]),
    ("source", &["src", "srcset"]),
    ("table", &["background"]),
    ("tbody", &["background"]),
    ("td", &["background"]),
    ("tfoot", &["background"]),
    ("th", &["background"]),
    ("thead", &["background"]),
    ("tr", &["background"]),
    ("track", &["src"]),
    ("video", &["poster", "src"]),
];

fn sort_entries<T>(list: &mut [Entry<T>]) {
    // descending priority, stable
    list.sort_by_key(|a| std::cmp::Reverse(a.priority));
}

impl HtmlTransformer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_url_transform(&self, extensions: &str, callback: UrlCallback, options: AddOptions) {
        let mut inner = self.0.lock().unwrap();
        for ext in extensions.split(',') {
            let list = inner.callbacks.entry(ext.trim().to_string()).or_default();
            list.push(Entry {
                name: options.name.clone(),
                f: callback.clone(),
                priority: options.priority,
                enabled: options.enabled.clone(),
            });
            sort_entries(list);
        }
    }

    /// Content-level HTML plugin (the equivalent of a posthtml plugin)
    pub fn add_plugin(&self, extensions: &str, plugin: HtmlPlugin, options: AddOptions) {
        let mut inner = self.0.lock().unwrap();
        for ext in extensions.split(',') {
            let list = inner.plugins.entry(ext.trim().to_string()).or_default();
            list.push(Entry {
                name: options.name.clone(),
                f: plugin.clone(),
                priority: options.priority,
                enabled: options.enabled.clone(),
            });
            sort_entries(list);
        }
    }

    pub fn remove(&self, extensions: &str, name: &str) {
        let mut inner = self.0.lock().unwrap();
        for ext in extensions.split(',') {
            if let Some(list) = inner.callbacks.get_mut(ext) {
                list.retain(|e| e.name.as_deref() != Some(name));
            }
            if let Some(list) = inner.plugins.get_mut(ext) {
                list.retain(|e| e.name.as_deref() != Some(name));
            }
        }
    }

    fn get_callbacks(&self, extension: &str, ctx: &TransformContext) -> Vec<UrlCallback> {
        let inner = self.0.lock().unwrap();
        inner
            .callbacks
            .get(extension)
            .map(|list| {
                list.iter()
                    .filter(|e| e.enabled.as_ref().map(|f| f(ctx)).unwrap_or(true))
                    .map(|e| e.f.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn get_plugins(&self, extension: &str, ctx: &TransformContext) -> Vec<HtmlPlugin> {
        let inner = self.0.lock().unwrap();
        inner
            .plugins
            .get(extension)
            .map(|list| {
                list.iter()
                    .filter(|e| e.enabled.as_ref().map(|f| f(ctx)).unwrap_or(true))
                    .map(|e| e.f.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn is_transformable(&self, extension: &str, ctx: &TransformContext) -> bool {
        !self.get_callbacks(extension, ctx).is_empty()
            || !self.get_plugins(extension, ctx).is_empty()
    }

    pub fn transform_content(
        &self,
        output_path: &str,
        content: String,
        ctx: &TransformContext,
    ) -> Result<String> {
        let extension = template_path::get_extension(output_path);
        let callbacks = self.get_callbacks(&extension, ctx);
        let plugins = self.get_plugins(&extension, ctx);
        if callbacks.is_empty() && plugins.is_empty() {
            return Ok(content);
        }
        let mut content = content;
        for plugin in plugins {
            content = plugin(ctx, content)?;
        }
        if callbacks.is_empty() {
            return Ok(content);
        }
        rewrite_urls(&content, &|url, url_ctx| {
            let mut url = url.to_string();
            for cb in &callbacks {
                url = cb(ctx, &url, url_ctx)?;
            }
            Ok(url)
        })
    }

    /// `HtmlTransformer.transformStandalone`
    pub fn transform_standalone(
        content: &str,
        callback: &dyn Fn(&str, &UrlContext) -> Result<String>,
    ) -> Result<String> {
        rewrite_urls(content, callback)
    }
}

fn transform_srcset(value: &str, f: &dyn Fn(&str) -> Result<String>) -> Result<String> {
    let mut out = vec![];
    for candidate in value.split(',') {
        let trimmed = candidate.trim();
        if trimmed.is_empty() {
            continue;
        }
        let mut parts = trimmed.splitn(2, char::is_whitespace);
        let url = parts.next().unwrap_or("");
        let descriptor = parts.next().map(|d| d.trim()).unwrap_or("");
        let new_url = f(url)?;
        if descriptor.is_empty() {
            out.push(new_url);
        } else {
            out.push(format!("{new_url} {descriptor}"));
        }
    }
    Ok(out.join(", "))
}

/// Rewrite all URLs in HTML attributes with `callback`
pub fn rewrite_urls(
    content: &str,
    callback: &dyn Fn(&str, &UrlContext) -> Result<String>,
) -> Result<String> {
    let error: RefCell<Option<anyhow::Error>> = RefCell::new(None);
    let mut output = Vec::with_capacity(content.len());
    {
        let mut handlers = vec![];
        for (tag, attrs) in URL_ATTRIBUTES {
            let tag = *tag;
            let attrs = *attrs;
            let error = &error;
            handlers.push(element!(tag, move |el| {
                for attr in attrs {
                    // Attribute names match case-sensitively (as in posthtml); attributes
                    // without a value are skipped
                    let Some(value) = el
                        .attributes()
                        .iter()
                        .find(|a| a.name_preserve_case() == *attr)
                        .map(|a| a.value())
                        .filter(|v| !v.is_empty())
                    else {
                        continue;
                    };
                    if error.borrow().is_some() {
                        return Ok(());
                    }
                    let url_ctx = UrlContext {
                        attribute: attr.to_string(),
                        tag: tag.to_string(),
                    };
                    let result = if *attr == "srcset" {
                        transform_srcset(&value, &|u| callback(u, &url_ctx))
                    } else if tag == "meta" && *attr == "content" {
                        // only `<meta http-equiv="refresh" content="0; url=…">`
                        let is_refresh = el
                            .get_attribute("http-equiv")
                            .map(|v| v.eq_ignore_ascii_case("refresh"))
                            .unwrap_or(false);
                        if !is_refresh {
                            continue;
                        }
                        match value.to_lowercase().find("url=") {
                            Some(i) => callback(&value[i + 4..], &url_ctx)
                                .map(|u| format!("{}{}", &value[..i + 4], u)),
                            None => continue,
                        }
                    } else if (*attr == "ping") || (*attr == "archive") {
                        Ok(value.clone())
                    } else {
                        callback(&value, &url_ctx)
                    };
                    match result {
                        Ok(new_value) => {
                            if new_value != value {
                                el.set_attribute(attr, &new_value)?;
                            }
                        }
                        Err(e) => {
                            *error.borrow_mut() = Some(e);
                            return Ok(());
                        }
                    }
                }
                Ok(())
            }));
        }
        let mut rewriter = HtmlRewriter::new(
            Settings {
                element_content_handlers: handlers,
                ..Settings::new()
            },
            |c: &[u8]| output.extend_from_slice(c),
        );
        rewriter
            .write(content.as_bytes())
            .map_err(|e| anyhow!("HTML rewriting error: {e}"))?;
        rewriter
            .end()
            .map_err(|e| anyhow!("HTML rewriting error: {e}"))?;
    }
    if let Some(e) = error.into_inner() {
        return Err(e);
    }
    Ok(String::from_utf8_lossy(&output).to_string())
}

/// Registers the `@12ty/twelvety/html-transformer` transform (defaultConfigExtended.js)
pub fn register(config: &mut UserConfig, _handle: ConfigHandle) -> Result<()> {
    let transformer = config.html_transformer.clone();
    config.add_transform("@12ty/twelvety/html-transformer", move |ctx, content| {
        let output_path = ctx.output_path.to_output_string();
        transformer.transform_content(&output_path, content, ctx)
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites() {
        let html =
            r#"<a href="/test/">x</a><img src="a.png" srcset="a.png 1x, b.png 2x"><p>/test/</p>"#;
        let out = rewrite_urls(html, &|u, _| Ok(format!("/prefix{u}"))).unwrap();
        assert_eq!(
            out,
            r#"<a href="/prefix/test/">x</a><img src="/prefixa.png" srcset="/prefixa.png 1x, /prefixb.png 2x"><p>/test/</p>"#
        );
    }
}
