//! Port of `defaultConfig.js` and `defaultConfigExtended.js`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{Result, bail};

use crate::filters;
use crate::plugins;
use crate::template_config::ConfigHandle;
use crate::user_config::{CallContext, UserConfig};
use crate::util::slugify::{SlugifyOptions, slugify};
use crate::util::transforms_util;
use crate::value::Value;

pub const TEMPLATE_FORMATS: &[&str] = &["liquid", "md", "njk", "html", "12ty.js"];

fn get_locale_collection_item(
    ctx: &CallContext,
    config: &ConfigHandle,
    collection: &Value,
    page_override: &Value,
    lang_code: &Value,
    modifier: i64,
) -> Value {
    let page = if page_override.is_truthy() {
        page_override.clone()
    } else {
        ctx.page()
    };
    if !lang_code.is_truthy() && !ctx.page().get("lang").is_truthy() {
        return filters::get_collection_item(collection, &page, modifier);
    }
    crate::plugins::i18n::get_locale_collection_item(ctx, config, collection, &page, lang_code, modifier)
}

pub fn apply(config: &mut UserConfig, handle: ConfigHandle) -> Result<()> {
    // defaultConfigExtended.js
    plugins::html_transformer::register(config, handle.clone())?;
    config.add_plugin(
        "@12ty/twelvety/html-relative-copy-plugin",
        |c, _| plugins::html_relative_copy::register(c),
        Value::object(),
    )?;
    plugins::input_path_to_url::register_filter(config, handle.clone())?;

    config.add_filter("slug", |_, _, _| {
        bail!("The `slug` filter (deprecated since v1) has been removed in Twelvety v4. You can add it manually to your configuration file for backwards compatibility, read more at GitHub Issue #3893: https://github.com/11ty/eleventy/issues/3893 Alternatively (more risky), you can swap to use the `slugify` filter instead (outputs may be different and production URLs may break!)")
    });

    let memo: Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(HashMap::new()));
    config.add_filter("slugify", move |_, input, args| {
        let s = input.to_js_string();
        let options = args.first().cloned().unwrap_or_default();
        let mut opts = SlugifyOptions {
            decamelize: false,
            ..Default::default()
        };
        if options.is_object() {
            if let Some(b) = options.get("decamelize").as_bool() {
                opts.decamelize = b;
            }
            if let Some(b) = options.get("lowercase").as_bool() {
                opts.lowercase = b;
            }
            if let Some(sep) = options.get("separator").as_str() {
                opts.separator = sep.to_string();
            }
            if let Some(reps) = options.get("customReplacements").as_array() {
                for r in reps {
                    if let Some(pair) = r.as_array() {
                        if pair.len() == 2 {
                            opts.custom_replacements
                                .push((pair[0].to_js_string(), pair[1].to_js_string()));
                        }
                    }
                }
            }
            if let Some(b) = options.get("preserveLeadingUnderscore").as_bool() {
                opts.preserve_leading_underscore = b;
            }
            if let Some(b) = options.get("preserveTrailingDash").as_bool() {
                opts.preserve_trailing_dash = b;
            }
            return Ok(Value::str(slugify(&s, &opts)));
        }
        if let Some(cached) = memo.lock().unwrap().get(&s) {
            return Ok(Value::str(cached));
        }
        let result = slugify(&s, &opts);
        memo.lock().unwrap().insert(s, result.clone());
        Ok(Value::str(result))
    });

    let h = handle.clone();
    config.add_filter("getCollectionItemIndex", move |ctx, collection, args| {
        let page = match args.first() {
            Some(p) if p.is_truthy() => p.clone(),
            _ => ctx.page(),
        };
        if page.get("lang").is_truthy() {
            return Ok(crate::plugins::i18n::get_collection_item_index(ctx, &h, &collection, &page));
        }
        Ok(filters::find_collection_item_index(&collection, &page)
            .map(Value::from)
            .unwrap_or_default())
    });
    for (name, modifier) in [
        ("getCollectionItem", 0),
        ("getPreviousCollectionItem", -1),
        ("getNextCollectionItem", 1),
    ] {
        let h = handle.clone();
        config.add_filter(name, move |ctx, collection, args| {
            let page_override = args.first().cloned().unwrap_or_default();
            let lang_code = args.get(1).cloned().unwrap_or_default();
            Ok(get_locale_collection_item(
                ctx,
                &h,
                &collection,
                &page_override,
                &lang_code,
                modifier,
            ))
        });
    }

    let h = handle.clone();
    config.add_filter("url", move |_, url, args| {
        let path_prefix = match args.first() {
            Some(Value::String(s)) if !s.is_empty() => s.to_string(),
            _ => h.path_prefix(),
        };
        let url = if url.is_truthy() { url.to_js_string() } else { String::new() };
        Ok(Value::str(filters::url_filter(&url, &path_prefix)))
    });

    config.add_filter("getParentDirectory", |_, input, _| {
        Ok(filters::get_parent_directory(&input))
    });

    // defaultConfig.js
    config.add_filter("log", |_, input, args| {
        let mut entries = vec![input.to_js_string()];
        entries.extend(args.iter().map(|a| a.to_js_string()));
        println!("{}", entries.join(" "));
        Ok(input)
    });

    config.add_filter("parseDateString", |_, input, _| {
        crate::util::date_parse::iso_date_parse(&input.to_js_string())
            .map(Value::Date)
            .map_err(|e| anyhow::anyhow!(e))
    });

    let h = handle.clone();
    config.add_filter("renderTransforms", move |ctx, content, args| {
        let Some(cfg) = h.get() else {
            return Ok(content);
        };
        let page = match args.first() {
            Some(p) if p.is_truthy() => p.clone(),
            _ => ctx.page(),
        };
        let base_href = args.get(1).and_then(|v| v.as_str().map(String::from));
        let result = transforms_util::run_all(
            content.to_output_string(),
            &page,
            &cfg.transforms,
            base_href,
            None,
        )?;
        Ok(Value::str(result))
    });

    // These values are merged into the config upstream (returned object)
    if config.markdown_template_engine.is_none() {
        // defaults are applied during merge; nothing to do here
    }
    Ok(())
}
