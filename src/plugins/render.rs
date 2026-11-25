//! Port of `Plugins/RenderPlugin.js`: `renderTemplate`, `renderFile` and `renderContent`.

use std::sync::{Arc, RwLock, Weak};

use anyhow::{Context, Result, anyhow, bail};

use crate::build_context::{BuildContext, BuildContextRef};
use crate::data::front_matter::{self, FrontMatterOptions};
use crate::engines::RenderFn;
use crate::plugins::pagination::proxy_wrap;
use crate::template_pipeline;
use crate::template_render::TemplateRender;
use crate::user_config::{CallContext, UserConfig};
use crate::value::{Map, Value};

type ContextHandle = Arc<RwLock<Weak<BuildContext>>>;

fn current_context(handle: &ContextHandle) -> Result<Arc<BuildContext>> {
    handle
        .read()
        .unwrap()
        .upgrade()
        .ok_or_else(|| anyhow!("The Render plugin can only render during a build."))
}

/// `compile(content, templateLang)`
pub fn compile(ctx: &Arc<BuildContext>, content: &str, template_lang: &str) -> Result<RenderFn> {
    let steps = template_pipeline::parse(template_lang)?;
    let engine = steps
        .last()
        .cloned()
        .unwrap_or_else(|| template_pipeline::PLAINTEXT_FORMAT.to_string());
    if engine.starts_with("12ty.") {
        bail!("12ty.js is not yet supported as a template engine for `renderTemplate`. Use `renderFile` instead!");
    }
    let tr = TemplateRender::new(ctx.clone(), &engine)?;
    tr.get_compiled_template(content, false, Some(template_lang))
}

/// `compileFileWithFrontMatter(inputPath, options, templateLang)`
pub fn compile_file(ctx: &Arc<BuildContext>, input_path: &str, template_lang: Option<&str>) -> Result<(RenderFn, Option<Value>)> {
    if input_path.is_empty() {
        bail!("Missing file path argument passed to the `renderFile` shortcode.");
    }
    if !ctx.exists_cache.exists(input_path) {
        bail!("Could not find render plugin file for the `renderFile` shortcode, looking for: {input_path}");
    }
    let tr = TemplateRender::new(ctx.clone(), input_path)?;
    let resolved = tr.resolve(template_lang)?;
    if !resolved.engine.needs_to_read_file_contents() {
        return Ok((tr.get_compiled_template("", false, template_lang)?, None));
    }
    let raw = std::fs::read_to_string(input_path)
        .with_context(|| format!("Could not read `renderFile` target {input_path}"))?;
    let mut options = FrontMatterOptions::from_value(&ctx.config.front_matter_parsing_options);
    options.excerpt = false;
    let fm = front_matter::parse(&raw, &options, input_path).with_context(|| {
        format!("Having trouble reading front matter from `renderFile` target {input_path}")
    })?;
    let data = if fm.data.is_plain_object() && fm.data.len() > 0 {
        Some(fm.data.clone())
    } else {
        None
    };
    Ok((tr.get_compiled_template(&fm.content, false, template_lang)?, data))
}

/// `renderShortcodeFn`: builds the data for the render
fn render_with(fn_: &RenderFn, data: Value, call: &CallContext, access_global_data: bool) -> Result<String> {
    let data = if data.is_plain_object() {
        data
    } else {
        let mut m = Map::new();
        m.insert("_".into(), data);
        Value::from_map(m)
    };
    let data = if access_global_data {
        proxy_wrap(&data, &call.get("__all__"))
    } else {
        let mut m = Map::new();
        for key in ["page", "twelvety", "buildawesome"] {
            m.insert(key.into(), call.get(key));
        }
        if let Value::Object(o) = &data {
            for (k, v) in o.iter() {
                m.insert(k.clone(), v.clone());
            }
        }
        Value::from_map(m)
    };
    fn_(&data)
}

fn add_dependency(ctx: &BuildContext, page: &Value, nodes: &[String]) {
    let input_path = page.get("inputPath");
    if let Some(p) = input_path.as_str() {
        ctx.uses.lock().unwrap().add_dependency(p, nodes);
    }
}

pub fn plugin(config: &mut UserConfig, options: &Value) -> Result<()> {
    let tag_name = options.get("tagName").as_str().unwrap_or("renderTemplate").to_string();
    if !tag_name.chars().all(|c| c.is_alphanumeric() || c == '_') || tag_name.is_empty() {
        bail!(
            "Invalid RenderPlugin `tagName` option: {tag_name:?}. Use only letters, numbers and underscores."
        );
    }
    let tag_name_file = options.get("tagNameFile").as_str().unwrap_or("renderFile").to_string();
    let filter_name = options.get("filterName").as_str().unwrap_or("renderContent").to_string();
    let access_global_data = options.get("accessGlobalData").is_truthy();

    let handle: ContextHandle = Arc::new(RwLock::new(Weak::new()));
    let h = handle.clone();
    config.on("buildawesome#buildcontext", move |arg| {
        if let Some(ctx) = BuildContextRef::from_value(arg) {
            *h.write().unwrap() = Arc::downgrade(&ctx);
        }
        Ok(())
    });

    // `_renderStringShortcodeFn(content, templateLang, data)`
    let render_string = {
        let handle = handle.clone();
        Arc::new(move |call: &CallContext, content: &str, args: &[Value]| -> Result<String> {
            let ctx = current_context(&handle)?;
            let (template_lang, data) = match args.first() {
                Some(Value::String(s)) => (Some(s.to_string()), args.get(1).cloned().unwrap_or_default()),
                Some(other) => (None, other.clone()),
                None => (None, Value::object()),
            };
            let data = if data.is_undefined() { Value::object() } else { data };
            let page = call.page();
            let lang = template_lang
                .clone()
                .or_else(|| page.get("templateSyntax").as_str().map(String::from))
                .unwrap_or_else(|| "html".into());
            let mut deps = vec![lang.clone()];
            deps.extend(lang.split(',').map(String::from));
            add_dependency(&ctx, &page, &deps);
            let f = compile(&ctx, content, &lang)?;
            render_with(&f, data, call, access_global_data)
        })
    };

    if !tag_name.is_empty() {
        let rs = render_string.clone();
        config.add_paired_shortcode(&tag_name, move |call, content, args| rs(call, content, args));
        config.raw_paired_shortcodes.insert(tag_name.clone());
    }
    if !filter_name.is_empty() {
        let rs = render_string.clone();
        config.add_filter(&filter_name, move |call, content, args| {
            Ok(Value::str(rs(call, &content.to_output_string(), args)?))
        });
    }
    if !tag_name_file.is_empty() {
        let handle = handle.clone();
        config.add_shortcode(&tag_name_file, move |call, args| {
            let ctx = current_context(&handle)?;
            let input_path = args.first().map(|a| a.to_output_string()).unwrap_or_default();
            let data = args.get(1).cloned().unwrap_or_else(Value::object);
            let template_lang = args.get(2).and_then(|v| v.as_str().map(String::from));
            add_dependency(&ctx, &call.page(), std::slice::from_ref(&input_path));
            let (f, fm_data) = compile_file(&ctx, &input_path, template_lang.as_deref())?;
            let data = match fm_data {
                Some(fm) => {
                    let mut merged = fm.as_object().cloned().unwrap_or_default();
                    let data = if data.is_plain_object() {
                        data
                    } else {
                        let mut m = Map::new();
                        m.insert("_".into(), data);
                        Value::from_map(m)
                    };
                    if let Value::Object(o) = &data {
                        for (k, v) in o.iter() {
                            merged.insert(k.clone(), v.clone());
                        }
                    }
                    Value::from_map(merged)
                }
                None => data,
            };
            render_with(&f, data, call, access_global_data)
        });
    }
    Ok(())
}
