//! Port of `Plugins/HtmlBasePlugin.js`.

use std::sync::Arc;

use anyhow::{Result, bail};

use super::html_transformer::{AddOptions, HtmlTransformer};
use crate::filters::url_filter;
use crate::util::url_util::is_valid_url;
use crate::user_config::UserConfig;
use crate::value::Value;

fn join_url_parts(a: &str, b: &str) -> String {
    crate::util::node_path::join(&[a, b])
}

pub fn add_path_prefix_to_url(url: &str, path_prefix: Option<&str>, base: Option<&str>) -> Result<String> {
    let mut u = match base {
        Some(base) => url::Url::parse(base)?.join(url)?,
        None => url::Url::parse(url)?,
    };
    if let Some(prefix) = path_prefix {
        if !prefix.is_empty() {
            let new_path = join_url_parts(prefix, u.path());
            u.set_path(&new_path);
        }
    }
    Ok(u.to_string())
}

/// `transformUrl` (exported as `applyBaseToUrl`)
pub fn apply_base_to_url(url: &str, base: &str, path_prefix: Option<&str>, page_url: Option<&str>) -> Result<String> {
    if is_valid_url(url) {
        return Ok(url.to_string());
    }
    if is_valid_url(base) {
        let mut url = url.to_string();
        if let Some(page_url) = page_url {
            if !url.starts_with('/') {
                let u = url::Url::parse(&format!("http://example.com{page_url}"))?.join(&url)?;
                url = format!(
                    "{}{}",
                    u.path(),
                    u.fragment().map(|f| format!("#{f}")).unwrap_or_default()
                );
            }
        }
        return add_path_prefix_to_url(&url, path_prefix, Some(base));
    }
    Ok(url_filter(url, base))
}

pub fn plugin(config: &mut UserConfig, options: &Value) -> Result<()> {
    let path_prefix = config.path_prefix.clone().unwrap_or_else(|| "/".into());
    let base_href = match options.get("baseHref") {
        Value::String(s) => s.to_string(),
        Value::Undefined => path_prefix.clone(),
        _ => bail!("The `baseHref` option is required in the HTML Base plugin."),
    };
    if !options.get("filters").is_undefined() {
        bail!("The `filters` option in the HTML Base plugin was removed to prevent future cross-plugin compatibility issues.");
    }
    // A falsy `extensions` list opts out of the transform
    let extensions = match options.get("extensions") {
        Value::Undefined => Some("html".to_string()),
        Value::String(s) if !s.is_empty() => Some(s.to_string()),
        _ => None,
    };

    let pp = path_prefix.clone();
    config.add_filter("addPathPrefixToFullUrl", move |_, url, _| {
        Ok(Value::str(add_path_prefix_to_url(&url.to_output_string(), Some(&pp), None)?))
    });

    let (pp, bh) = (path_prefix.clone(), base_href.clone());
    config.add_filter("htmlBaseUrl", move |ctx, url, args| {
        let base = match args.first() {
            Some(Value::String(s)) if !s.is_empty() => s.to_string(),
            _ => bh.clone(),
        };
        if base == "/" {
            return Ok(url);
        }
        let page_url = match args.get(1) {
            Some(Value::String(s)) if !s.is_empty() => s.to_string(),
            _ => ctx.page().get("url").to_output_string(),
        };
        Ok(Value::str(apply_base_to_url(
            &url.to_output_string(),
            &base,
            Some(&pp),
            Some(&page_url).filter(|s| !s.is_empty()).map(|s| s.as_str()),
        )?))
    });

    let (pp, bh) = (path_prefix.clone(), base_href.clone());
    config.add_filter("transformWithHtmlBase", move |ctx, content, args| {
        let base = match args.first() {
            Some(Value::String(s)) if !s.is_empty() => s.to_string(),
            _ => bh.clone(),
        };
        if base == "/" {
            return Ok(content);
        }
        let page_url = match args.get(1) {
            Some(Value::String(s)) if !s.is_empty() => s.to_string(),
            _ => ctx.page().get("url").to_output_string(),
        };
        let result = HtmlTransformer::transform_standalone(&content.to_output_string(), &|url, _| {
            apply_base_to_url(url.trim(), &base, Some(&pp), Some(&page_url).filter(|s| !s.is_empty()).map(|s| s.as_str()))
        })?;
        Ok(Value::str(result))
    });

    let Some(extensions) = extensions else {
        return Ok(());
    };
    let (pp, bh, bh2) = (path_prefix.clone(), base_href.clone(), base_href.clone());
    config.html_transformer.add_url_transform(
        &extensions,
        Arc::new(move |ctx, url, _| {
            let base = ctx.base_href.clone().filter(|b| !b.is_empty()).unwrap_or_else(|| bh.clone());
            let page_url = ctx.url.to_output_string();
            apply_base_to_url(url.trim(), &base, Some(&pp), Some(page_url.as_str()).filter(|s| !s.is_empty()))
        }),
        AddOptions {
            name: Some("@11ty/eleventy/html-base-plugin".into()),
            priority: -2,
            enabled: Some(Arc::new(move |ctx| {
                ctx.base_href.as_ref().map(|b| !b.is_empty()).unwrap_or(false) || bh2 != "/"
            })),
        },
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applies_base() {
        assert_eq!(apply_base_to_url("/test/", "/", None, None).unwrap(), "/test/");
        assert_eq!(apply_base_to_url("/test/", "/prefix/", None, None).unwrap(), "/prefix/test/");
        assert_eq!(
            apply_base_to_url("/test/", "https://example.com/", None, None).unwrap(),
            "https://example.com/test/"
        );
        assert_eq!(
            apply_base_to_url("/test/", "https://example.com/", Some("/prefix/"), None).unwrap(),
            "https://example.com/prefix/test/"
        );
        assert_eq!(
            apply_base_to_url("relative.html", "https://example.com/", None, Some("/page/")).unwrap(),
            "https://example.com/page/relative.html"
        );
    }
}
