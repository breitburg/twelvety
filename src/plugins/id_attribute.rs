//! Port of `Plugins/IdAttributePlugin.js`.
//!
//! Two passes with `lol_html`: the first collects heading text and existing ids,
//! the second assigns the computed ids (a streaming rewriter can't look ahead).

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;

use anyhow::{Result, anyhow, bail};
use lol_html::{HtmlRewriter, Settings, element, text};

use super::html_transformer::AddOptions;
use crate::user_config::{CallContext, UserConfig};
use crate::value::Value;

const IGNORE_ATTR: &str = "buildawesome:id-ignore";
const IGNORE_ATTR_ALIAS: &str = "eleventy:id-ignore";

#[derive(Default, Debug, Clone)]
struct Match {
    existing_id: Option<String>,
    text: String,
}

fn escape_selector_attr(name: &str) -> String {
    name.replace(':', "\\:")
}

pub fn plugin(config: &mut UserConfig, options: &Value) -> Result<()> {
    let selector = options
        .get("selector")
        .as_str()
        .unwrap_or("[id],h1,h2,h3,h4,h5,h6")
        .to_string();
    let decode_entities = options.get("decodeEntities").as_bool().unwrap_or(true);
    let check_duplicates = match options.get("checkDuplicates") {
        Value::Bool(false) => false,
        Value::String(s) => &*s == "error",
        _ => true,
    };
    let slugify = config
        .get_filter("slugify")
        .ok_or_else(|| anyhow!("Missing `slugify` filter for the IdAttribute plugin"))?;
    let custom_slugify = match options.get("slugify") {
        f @ Value::Function(_) => Some(f),
        _ => None,
    };
    let filter = match options.get("filter") {
        f @ Value::Function(_) => Some(f),
        _ => None,
    };

    config.html_transformer.add_plugin(
        "html",
        Arc::new(move |ctx, content| {
            if let Some(filter) = &filter {
                let mut opts = crate::value::Map::new();
                opts.insert("page".into(), ctx.page.clone());
                opts.insert("url".into(), ctx.url.clone());
                opts.insert("outputPath".into(), ctx.output_path.clone());
                if !filter.call(&[Value::from_map(opts)])?.is_truthy() {
                    return Ok(content);
                }
            }
            let slug = |text: &str| -> Result<String> {
                if let Some(f) = &custom_slugify {
                    return Ok(f.call(&[Value::str(text)])?.to_output_string());
                }
                let lookup = |_: &str| Value::Undefined;
                let call = CallContext::new("html", &lookup);
                Ok(slugify(&call, Value::str(text), &[])?.to_output_string())
            };

            // Pass 1: collect matches
            let matches: Rc<RefCell<Vec<Match>>> = Rc::new(RefCell::new(vec![]));
            let open: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(vec![]));
            let ignore_depth: Rc<RefCell<usize>> = Rc::new(RefCell::new(0));
            {
                let (m1, o1) = (matches.clone(), open.clone());
                let (o2, m2, i2) = (open.clone(), matches.clone(), ignore_depth.clone());
                let i3 = ignore_depth.clone();
                let ignore_selector = format!(
                    "[{}], [{}]",
                    escape_selector_attr(IGNORE_ATTR),
                    escape_selector_attr(IGNORE_ATTR_ALIAS)
                );
                let mut rewriter = HtmlRewriter::new(
                    Settings {
                        element_content_handlers: vec![
                            element!(selector.as_str(), move |el| {
                                let idx = m1.borrow().len();
                                m1.borrow_mut().push(Match {
                                    existing_id: el.get_attribute("id").filter(|s| !s.is_empty()),
                                    text: String::new(),
                                });
                                if el.can_have_content() {
                                    o1.borrow_mut().push(idx);
                                    let o = o1.clone();
                                    if let Some(handlers) = el.end_tag_handlers() {
                                        let handler: lol_html::EndTagHandler<'static> =
                                            Box::new(move |_| {
                                                o.borrow_mut().retain(|i| *i != idx);
                                                Ok(())
                                            });
                                        handlers.push(handler);
                                    }
                                }
                                Ok(())
                            }),
                            element!(ignore_selector.as_str(), move |el| {
                                if el.can_have_content() {
                                    *i3.borrow_mut() += 1;
                                    let i = i3.clone();
                                    if let Some(handlers) = el.end_tag_handlers() {
                                        let handler: lol_html::EndTagHandler<'static> =
                                            Box::new(move |_| {
                                                *i.borrow_mut() -= 1;
                                                Ok(())
                                            });
                                        handlers.push(handler);
                                    }
                                }
                                Ok(())
                            }),
                            text!("*", move |t| {
                                if *i2.borrow() > 0 {
                                    return Ok(());
                                }
                                let open = o2.borrow();
                                let mut m = m2.borrow_mut();
                                for idx in open.iter() {
                                    m[*idx].text.push_str(t.as_str());
                                }
                                Ok(())
                            }),
                        ],
                        ..Settings::new()
                    },
                    |_: &[u8]| {},
                );
                rewriter.write(content.as_bytes()).map_err(|e| anyhow!("{e}"))?;
                rewriter.end().map_err(|e| anyhow!("{e}"))?;
            }

            // Compute ids
            let matches = matches.borrow().clone();
            let mut used_ids: HashSet<String> = HashSet::new();
            let mut headings: Vec<(usize, String)> = vec![];
            for (i, m) in matches.iter().enumerate() {
                if let Some(id) = &m.existing_id {
                    if used_ids.contains(id) && check_duplicates {
                        bail!(
                            "You have more than one HTML `id` attribute using the same value (id=\"{id}\") in your template ({}). You can disable this error in the IdAttribute plugin with the `checkDuplicates: false` option.",
                            ctx.page.get("inputPath").to_output_string()
                        );
                    }
                    used_ids.insert(id.clone());
                } else {
                    let mut text = m.text.clone();
                    if decode_entities {
                        text = html_escape::decode_html_entities(&text).to_string();
                    }
                    if text.is_empty() {
                        continue;
                    }
                    headings.push((i, slug(&text)?));
                }
            }
            let mut reserved: HashSet<String> = used_ids.clone();
            reserved.extend(headings.iter().map(|(_, s)| s.clone()));
            let mut assigned: Vec<Option<String>> = vec![None; matches.len()];
            for (i, slug) in headings {
                let mut id = slug.clone();
                if used_ids.contains(&id) {
                    let mut suffix = 2;
                    while reserved.contains(&format!("{slug}-{suffix}")) {
                        suffix += 1;
                    }
                    id = format!("{slug}-{suffix}");
                    reserved.insert(id.clone());
                }
                used_ids.insert(id.clone());
                assigned[i] = Some(id);
            }

            // Pass 2: assign ids (and drop ignore attributes)
            let counter = Rc::new(RefCell::new(0usize));
            let mut output = Vec::with_capacity(content.len());
            {
                let c = counter.clone();
                let ignore_selector = format!(
                    "[{}], [{}]",
                    escape_selector_attr(IGNORE_ATTR),
                    escape_selector_attr(IGNORE_ATTR_ALIAS)
                );
                let mut rewriter = HtmlRewriter::new(
                    Settings {
                        element_content_handlers: vec![
                            element!(selector.as_str(), move |el| {
                                let idx = *c.borrow();
                                *c.borrow_mut() += 1;
                                if let Some(Some(id)) = assigned.get(idx) {
                                    el.set_attribute("id", id)?;
                                }
                                Ok(())
                            }),
                            element!(ignore_selector.as_str(), |el| {
                                el.remove_attribute(IGNORE_ATTR);
                                el.remove_attribute(IGNORE_ATTR_ALIAS);
                                Ok(())
                            }),
                        ],
                        ..Settings::new()
                    },
                    |chunk: &[u8]| output.extend_from_slice(chunk),
                );
                rewriter.write(content.as_bytes()).map_err(|e| anyhow!("{e}"))?;
                rewriter.end().map_err(|e| anyhow!("{e}"))?;
            }
            Ok(String::from_utf8_lossy(&output).to_string())
        }),
        AddOptions {
            name: Some("awesome.me/buildawesome/id-attribute".into()),
            ..Default::default()
        },
    );
    Ok(())
}
