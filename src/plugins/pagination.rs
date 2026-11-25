//! Port of `Plugins/Pagination.js`.

use std::sync::Arc;

use anyhow::{Result, bail};

use crate::data::resolve_configuration_data as rcd;
use crate::data::template_data::get_included_tag_names;
use crate::template::Template;
use crate::template_config::Config;
use crate::util::merge::deep_copy;
use crate::value::{Map, Value};

pub struct PaginationPage {
    pub page_number: usize,
    pub group_number: Value,
    pub extra_subdir: String,
    pub data: Value,
}

pub struct Pagination {
    template: Arc<Template>,
    data: Value,
    /// `size === 1`: page items are normalized to the single item
    single_item_pages: bool,
    alias: Option<String>,
    /// The resolved items (after `before`, `reverse` and `exclude`)
    target: Vec<Value>,
    chunked_items: Vec<Vec<Value>>,
    permalink_key: String,
    computed_key: String,
}

/// `ProxyWrap(target, fallback)`: `target` values take precedence, objects are layered
pub fn proxy_wrap(target: &Value, fallback: &Value) -> Value {
    match (target, fallback) {
        (Value::Object(t), Value::Object(f)) => {
            let mut out: Map = f.as_ref().clone();
            for (k, v) in t.iter() {
                let merged = match out.get(k) {
                    Some(fv) if v.is_plain_object() && fv.is_plain_object() => proxy_wrap(v, fv),
                    _ => v.clone(),
                };
                out.insert(k.clone(), merged);
            }
            Value::from_map(out)
        }
        (t, _) => t.clone(),
    }
}

/// `ProxyWrap(target, fallback)` with upstream’s argument validation: both must be plain objects
pub fn try_proxy_wrap(target: &Value, fallback: &Value) -> Result<Value> {
    if !target.is_plain_object() || !fallback.is_plain_object() {
        bail!("ProxyWrap expects objects for both the target and fallback");
    }
    Ok(proxy_wrap(target, fallback))
}

impl Pagination {
    pub fn has_pagination(data: &Value) -> bool {
        data.get("pagination").is_truthy()
    }

    pub fn new(template: Arc<Template>, data: &Value, config: &Config) -> Result<Self> {
        let input_path = template.input_path.clone();
        let mut paging = Pagination {
            template,
            data: data.clone(),
            single_item_pages: false,
            alias: None,
            target: vec![],
            chunked_items: vec![],
            permalink_key: config.keys.permalink.clone(),
            computed_key: config.keys.computed.clone(),
        };
        if !Self::has_pagination(data) {
            return Ok(paging);
        }
        let pagination = data.get("pagination");
        if !pagination.has("size") {
            bail!("Missing pagination size in front matter data ({input_path})");
        }
        // circular reference check
        if let Some(key) = pagination.get("data").as_str() {
            for tag in get_included_tag_names(data, config.keys.tags.as_deref()) {
                if format!("collections.{tag}") == key {
                    bail!(
                        "Pagination circular reference ({input_path}), data:`{key}` iterates over both the `{tag}` collection and also supplies pages to that collection."
                    );
                }
            }
        }
        let size = pagination.get("size");
        let alias = pagination.get("alias").as_str().map(String::from);
        let excluded = {
            let e = pagination.get("exclude");
            let e = if e.is_nullish() {
                pagination.get("filter")
            } else {
                e
            };
            match e {
                Value::Undefined | Value::Null => vec![],
                Value::Array(a) => a.as_ref().clone(),
                other => vec![other],
            }
        };

        // _getDataSet
        let key = pagination.get("data");
        let full_data_set = match &key {
            Value::Array(_) | Value::Object(_) => key.clone(),
            other => {
                let k = other.to_output_string();
                if !data.has_path(&k) {
                    bail!("Could not find pagination data ({input_path}), went looking for: {k}");
                }
                data.get_path(&k)
            }
        };

        // _resolveItems
        let mut items: Vec<Value> = match &full_data_set {
            Value::Array(a) => a.as_ref().clone(),
            Value::Object(o) => {
                if pagination.get("resolve").as_str() == Some("values") {
                    o.values().cloned().collect()
                } else {
                    o.keys().map(Value::from).collect()
                }
            }
            Value::Dynamic(d) => match d.to_plain() {
                Value::Object(o) => {
                    if pagination.get("resolve").as_str() == Some("values") {
                        o.values().cloned().collect()
                    } else {
                        o.keys().map(Value::from).collect()
                    }
                }
                Value::Array(a) => a.as_ref().clone(),
                _ => vec![],
            },
            _ => bail!(
                "Unexpected data found in pagination target ({input_path}): expected an Array or an Object."
            ),
        };
        if let Value::Function(before) = pagination.get("before") {
            let result = before(&[Value::array(items.clone()), data.clone()])?;
            items = result.to_vec();
        }
        if pagination.get("reverse").as_bool() == Some(true) {
            items.reverse();
        }
        if !excluded.is_empty() {
            items.retain(|v| !excluded.iter().any(|e| e.strict_equals(v)));
        }

        // lodash `chunk`: the size is truncated to an integer and a size below 1 yields no chunks
        let chunk_size = size.to_number();
        let chunk_size = if chunk_size.is_nan() {
            0.0
        } else {
            chunk_size.trunc().max(0.0)
        };
        let mut chunked: Vec<Vec<Value>> = if chunk_size < 1.0 {
            vec![]
        } else {
            items
                .chunks(chunk_size.min(usize::MAX as f64) as usize)
                .map(|c| c.to_vec())
                .collect()
        };
        if chunked.is_empty() && pagination.get("generatePageOnEmptyData").is_truthy() {
            chunked = vec![vec![]];
        }

        paging.single_item_pages = size.strict_equals(&Value::Number(1.0));
        paging.alias = alias;
        paging.target = items;
        paging.chunked_items = chunked;
        Ok(paging)
    }

    pub fn get_page_count(&self) -> usize {
        self.chunked_items.len()
    }

    /// The resolved items to paginate over (`target`)
    pub fn target(&self) -> &[Value] {
        &self.target
    }

    /// The items for each page (`pagedItems`)
    pub fn paged_items(&self) -> &[Vec<Value>] {
        &self.chunked_items
    }

    fn normalized(&self, items: &[Value]) -> Value {
        if self.single_item_pages {
            items.first().cloned().unwrap_or_default()
        } else {
            Value::array(items.to_vec())
        }
    }

    pub fn get_page_templates(&self) -> Result<Vec<PaginationPage>> {
        if !Self::has_pagination(&self.data) {
            return Ok(vec![]);
        }
        let items = &self.chunked_items;
        let pages: Vec<Value> = items.iter().map(|chunk| self.normalized(chunk)).collect();
        let computed = rcd::get_value(&self.data, &self.computed_key);
        let has_permalink_field = self.data.get(&self.permalink_key).is_truthy()
            || computed.get(&self.permalink_key).is_truthy();

        let collections = self.data.get("collections");
        let mut data_without_collections = self.data.clone();
        data_without_collections.remove("collections");
        let mut pagination_base = Map::new();
        pagination_base.insert("data".into(), self.data.get("pagination").get("data"));
        pagination_base.insert("size".into(), self.data.get("pagination").get("size"));
        pagination_base.insert(
            "alias".into(),
            self.alias.clone().map(Value::from).unwrap_or_default(),
        );
        pagination_base.insert("pages".into(), Value::array(pages));
        let mut base = Map::new();
        base.insert("pagination".into(), Value::from_map(pagination_base));
        let mut parent_data = deep_copy(Value::from_map(base), &[&data_without_collections]);
        if !collections.is_undefined() {
            parent_data.set("collections", collections);
        }

        let mut links: Vec<Value> = vec![];
        let mut hrefs: Vec<Value> = vec![];
        let mut entries: Vec<PaginationPage> = vec![];
        for page_number in 0..items.len() {
            let extra_subdir = if page_number > 0 && !has_permalink_field {
                page_number.to_string()
            } else {
                String::new()
            };
            let mut page = Map::new();
            page.insert(
                "previous".into(),
                if page_number > 0 {
                    self.normalized(&items[page_number - 1])
                } else {
                    Value::Null
                },
            );
            page.insert(
                "next".into(),
                if page_number + 1 < items.len() {
                    self.normalized(&items[page_number + 1])
                } else {
                    Value::Null
                },
            );
            page.insert(
                "first".into(),
                items
                    .first()
                    .map(|i| self.normalized(i))
                    .unwrap_or(Value::Null),
            );
            page.insert(
                "last".into(),
                items
                    .last()
                    .map(|i| self.normalized(i))
                    .unwrap_or(Value::Null),
            );
            let mut pagination = Map::new();
            pagination.insert("items".into(), Value::array(items[page_number].clone()));
            pagination.insert("page".into(), Value::from_map(page));
            pagination.insert("pageNumber".into(), Value::from(page_number));
            let mut pagination_data = Map::new();
            pagination_data.insert("pagination".into(), Value::from_map(pagination));
            pagination_data.insert("page".into(), Value::object());
            let mut pagination_data = Value::from_map(pagination_data);
            if let Some(alias) = &self.alias {
                pagination_data.set_path(alias, self.normalized(&items[page_number]));
            }
            let mut cloned = proxy_wrap(&pagination_data, &parent_data);
            let loc = self.template.get_output_locations(&cloned, &extra_subdir)?;
            if let Some(raw) = &loc.raw_path {
                links.push(Value::str(crate::util::node_path::join(&["/", raw])));
            }
            hrefs.push(loc.href.clone());
            let mut page = cloned.get("page");
            page.set("url", loc.href);
            page.set("outputPath", loc.path);
            cloned.set("page", page);
            let first = items[page_number].first().cloned().unwrap_or_default();
            let group = first.get("buildawesomeTranslationKey");
            let group = if group.is_nullish() {
                first.get("twelvetyPaginationGroupNumber")
            } else {
                group
            };
            entries.push(PaginationPage {
                page_number,
                group_number: group,
                extra_subdir,
                data: cloned,
            });
        }

        let count = items.len();
        for (index, entry) in entries.iter_mut().enumerate() {
            let mut pagination = entry.data.get("pagination");
            let get = |list: &Vec<Value>, i: Option<usize>| -> Value {
                i.and_then(|i| list.get(i).cloned()).unwrap_or(Value::Null)
            };
            let prev = if index > 0 { Some(index - 1) } else { None };
            let next = if index + 1 < count {
                Some(index + 1)
            } else {
                None
            };
            pagination.set("previousPageLink", get(&links, prev));
            pagination.set("previous", get(&links, prev));
            pagination.set("nextPageLink", get(&links, next));
            pagination.set("next", get(&links, next));
            pagination.set(
                "firstPageLink",
                links.first().cloned().unwrap_or(Value::Null),
            );
            pagination.set("lastPageLink", links.last().cloned().unwrap_or(Value::Null));
            pagination.set("links", Value::array(links.clone()));
            pagination.set("pageLinks", Value::array(links.clone()));
            pagination.set("previousPageHref", get(&hrefs, prev));
            pagination.set("nextPageHref", get(&hrefs, next));
            pagination.set(
                "firstPageHref",
                hrefs.first().cloned().unwrap_or(Value::Null),
            );
            pagination.set("lastPageHref", hrefs.last().cloned().unwrap_or(Value::Null));
            pagination.set("hrefs", Value::array(hrefs.clone()));
            let mut href = Map::new();
            href.insert("previous".into(), get(&hrefs, prev));
            href.insert("next".into(), get(&hrefs, next));
            href.insert(
                "first".into(),
                hrefs.first().cloned().unwrap_or(Value::Null),
            );
            href.insert("last".into(), hrefs.last().cloned().unwrap_or(Value::Null));
            pagination.set("href", Value::from_map(href));
            entry.data.set("pagination", pagination);
        }
        Ok(entries)
    }
}
