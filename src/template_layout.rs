//! Port of `TemplateLayout.js`, `TemplateLayoutPathResolver.js` and `LayoutCache.js`.

use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};

use crate::build_context::BuildContext;
use crate::engines::RenderFn;
use crate::page_entry::PageEntry;
use crate::template_content::{RenderOptions, TemplateContent};
use crate::util::merge::merge;
use crate::util::template_path;
use crate::value::Value;

const CDATA_PREFIX: &str = "<![CDATA[STARTRAW";
const CDATA_POSTFIX: &str = "ENDRAW]]>";

/// Prevents Markdown-in-Markdown layouts from re-processing content
struct CdataWrapper {
    is_eligible: bool,
}

impl CdataWrapper {
    fn new(page_syntax: &str, layout_syntax: &str) -> Self {
        CdataWrapper {
            is_eligible: page_syntax.split(',').any(|s| s == "md")
                && layout_syntax.split(',').any(|s| s == "md"),
        }
    }

    fn wrap(&self, content: &str) -> String {
        if self.is_eligible {
            format!("{CDATA_PREFIX}{content}{CDATA_POSTFIX}")
        } else {
            content.to_string()
        }
    }

    fn unwrap(&self, content: String) -> String {
        if self.is_eligible {
            content.replace(CDATA_PREFIX, "").replace(CDATA_POSTFIX, "")
        } else {
            content
        }
    }
}

/// `TemplateLayoutPathResolver`
pub struct TemplateLayoutPathResolver {
    original_path: String,
    filename: Option<String>,
    full_path: Option<String>,
    layouts_dir: String,
}

impl TemplateLayoutPathResolver {
    pub fn new(path: &str, ctx: &BuildContext) -> Self {
        let layouts_dir = ctx.dirs.layouts().unwrap_or_else(|| ctx.dirs.includes());
        let mut p = path.to_string();
        if let Some(alias) = ctx.config.layout_aliases.get(path) {
            p = alias.clone();
        }
        let mut r = TemplateLayoutPathResolver {
            original_path: path.to_string(),
            filename: None,
            full_path: None,
            layouts_dir,
        };
        if let Some(resolved) = Self::resolve(&p, ctx) {
            r.full_path = Some(ctx.dirs.get_layout_path(&resolved));
            r.filename = Some(resolved);
        } else if ctx.config.layout_resolution {
            for file in ctx.extension_map.get_file_list(&p, None) {
                if let Some(resolved) = Self::resolve(&file, ctx) {
                    r.full_path = Some(ctx.dirs.get_layout_path(&resolved));
                    r.filename = Some(resolved);
                    break;
                }
            }
        }
        r
    }

    fn resolve(layout_path: &str, ctx: &BuildContext) -> Option<String> {
        let rel = ctx.dirs.get_layout_path_relative_to_input_directory(layout_path);
        if ctx.config.virtual_templates.contains_key(&rel) {
            return Some(layout_path.to_string());
        }
        let full_path = ctx.dirs.get_layout_path(layout_path);
        let resolved = ctx.exists_cache.resolve(&full_path)?;
        if ctx.exists_cache.is_directory(&resolved) {
            return None;
        }
        // keep the user’s path but with the on-disk casing of the matched suffix
        let lp: Vec<char> = layout_path.chars().collect();
        let fp: Vec<char> = full_path.chars().collect();
        let mut length = 0;
        while length < lp.len()
            && length < fp.len()
            && lp[lp.len() - 1 - length].to_lowercase().eq(fp[fp.len() - 1 - length].to_lowercase())
        {
            length += 1;
        }
        let rp: Vec<char> = resolved.chars().collect();
        Some(
            lp[..lp.len() - length].iter().collect::<String>()
                + &rp[rp.len().saturating_sub(length)..].iter().collect::<String>(),
        )
    }

    fn does_not_exist_error(&self) -> anyhow::Error {
        anyhow::anyhow!(
            "You’re trying to use a layout that does not exist: {} (via `layout: {}`)",
            template_path::join(&[&self.layouts_dir, &self.original_path]),
            self.original_path
        )
    }

    /// `getFileName`: the layout path relative to the layouts directory
    pub fn get_file_name(&self) -> Result<String> {
        self.filename.clone().ok_or_else(|| self.does_not_exist_error())
    }

    pub fn get_full_path(&self) -> Result<String> {
        match &self.full_path {
            Some(p) if self.filename.is_some() => Ok(p.clone()),
            _ => Err(self.does_not_exist_error()),
        }
    }

    pub fn get_normalized_layout_key(&self) -> String {
        template_path::strip_leading_sub_path(
            self.full_path.as_deref().unwrap_or(""),
            &self.layouts_dir,
        )
    }
}

pub struct TemplateLayout {
    pub content: TemplateContent,
    pub key: String,
    pub data_key_layout_path: String,
    pub input_path: String,
    compiled: Mutex<Option<RenderFn>>,
    data_cache: Mutex<Option<Value>>,
}

struct LayoutMapEntry {
    key: String,
    input_path: String,
    front_matter_data: Value,
}

impl TemplateLayout {
    fn new(key: &str, ctx: Arc<BuildContext>) -> Result<Self> {
        let resolver = TemplateLayoutPathResolver::new(key, &ctx);
        let resolved_path = resolver.get_full_path()?;
        Ok(TemplateLayout {
            content: TemplateContent::new(ctx, &resolved_path),
            key: resolver.get_normalized_layout_key(),
            data_key_layout_path: key.to_string(),
            input_path: resolved_path,
            compiled: Mutex::new(None),
            data_cache: Mutex::new(None),
        })
    }

    pub fn resolve_full_key(key: &str, input_dir: &str) -> String {
        template_path::join(&[input_dir, key])
    }

    /// `TemplateLayout.getTemplate` (via the layout cache)
    pub fn get_template(key: &str, ctx: &Arc<BuildContext>) -> Result<Arc<TemplateLayout>> {
        let full_key = Self::resolve_full_key(key, &ctx.dirs.input());
        if ctx.config.use_template_cache {
            if let Some(l) = ctx.layout_cache.lock().unwrap().get(&full_key) {
                return Ok(l.clone());
            }
        }
        let layout = Arc::new(Self::new(key, ctx.clone())?);
        if ctx.config.use_template_cache {
            let mut cache = ctx.layout_cache.lock().unwrap();
            cache.insert(full_key, layout.clone());
            cache.insert(layout.key.clone(), layout.clone());
        }
        Ok(layout)
    }

    fn ctx(&self) -> &Arc<BuildContext> {
        &self.content.ctx
    }

    fn get_template_layout_map_entry(&self) -> Result<LayoutMapEntry> {
        Ok(LayoutMapEntry {
            key: self.data_key_layout_path.clone(),
            input_path: self.input_path.clone(),
            front_matter_data: self.content.get_front_matter_data()?,
        })
    }

    fn get_template_layout_map(&self) -> Result<Vec<LayoutMapEntry>> {
        let cfg_key = self.ctx().config.keys.layout.clone();
        let mut chain = vec![self.input_path.clone()];
        let mut map = vec![];
        let mut entry = self.get_template_layout_map_entry()?;
        loop {
            let parent = entry.front_matter_data.get(&cfg_key);
            let has_parent = entry.front_matter_data.has(&cfg_key) && parent.is_truthy();
            map.push(entry);
            if !has_parent {
                break;
            }
            let layout = TemplateLayout::get_template(&parent.to_output_string(), self.ctx())?;
            if chain.contains(&layout.input_path) {
                bail!(
                    "Your layouts have a circular reference, starting at {}! The layout at {} was specified twice in this layout chain.",
                    map[0].key,
                    layout.input_path
                );
            }
            chain.push(layout.input_path.clone());
            entry = layout.get_template_layout_map_entry()?;
        }
        Ok(map)
    }

    pub fn get_layout_chain(&self) -> Result<Vec<String>> {
        Ok(self
            .get_template_layout_map()?
            .into_iter()
            .map(|e| e.input_path)
            .collect())
    }

    pub fn get_data(&self) -> Result<Value> {
        if let Some(d) = self.data_cache.lock().unwrap().as_ref() {
            return Ok(d.clone());
        }
        let map = self.get_template_layout_map()?;
        let sources: Vec<Value> = map.iter().rev().map(|e| e.front_matter_data.clone()).collect();
        let refs: Vec<&Value> = sources.iter().collect();
        let mut data = merge(Value::object(), &refs);
        let keys = &self.ctx().config.keys;
        data.remove(&keys.layout);
        data.remove(&keys.engine_override);
        data.remove(&format!("override:{}", keys.engine_override));
        *self.data_cache.lock().unwrap() = Some(data.clone());
        Ok(data)
    }

    fn get_compiled_layout_function(&self) -> Result<RenderFn> {
        if let Some(f) = self.compiled.lock().unwrap().as_ref() {
            return Ok(f.clone());
        }
        let raw = self.content.get_pre_render()?;
        let data = self.content.get_front_matter_data()?;
        let f = self
            .content
            .compile(&raw, false, self.content.get_engine_override(&data).as_deref())?;
        *self.compiled.lock().unwrap() = Some(f.clone());
        Ok(f)
    }

    /// This layout followed by its parent layouts
    fn get_compiled_layout_functions(self: &Arc<Self>) -> Result<Vec<(Arc<TemplateLayout>, RenderFn)>> {
        let map = self.get_template_layout_map()?;
        let mut fns = vec![(self.clone(), self.get_compiled_layout_function()?)];
        if map.len() > 1 {
            let parent = TemplateLayout::get_template(&map[1].key, self.ctx())?;
            fns.extend(parent.get_compiled_layout_functions()?);
        }
        Ok(fns)
    }

    pub fn render_layout_page_entry(self: &Arc<Self>, page: &PageEntry, template_content: String) -> Result<String> {
        let page_data = page.data();
        let page_syntax = page
            .template
            .content
            .get_engine_names(page.template.content.get_engine_override(&page_data).as_deref());
        let mut content = template_content;
        for (layout, render) in self.get_compiled_layout_functions()? {
            let layout_fm = layout.content.get_front_matter_data()?;
            let layout_syntax = layout
                .content
                .get_engine_names(layout.content.get_engine_override(&layout_fm).as_deref());
            let cdata = CdataWrapper::new(&page_syntax, &layout_syntax);
            let mut data = page_data.clone();
            data.set("content", Value::str(cdata.wrap(&content)));
            crate::page_entry::clear_premature_use();
            let rendered = render(&data);
            if let Some(m) = crate::page_entry::take_premature_use() {
                return Err(anyhow::Error::new(crate::errors::TemplateContentPrematureUseError(m)));
            }
            content = cdata.unwrap(rendered.with_context(|| {
                format!(
                    "Having trouble rendering {} layout {}",
                    layout.content.render().map(|r| r.get_readable_engines_list()).unwrap_or_default(),
                    layout.input_path
                )
            })?);
        }
        let _ = RenderOptions {
            template_only: false,
            kind: "Layout",
            step_inputs: None,
        };
        Ok(content)
    }
}
