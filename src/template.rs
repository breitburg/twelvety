//! Port of `Template.js`.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, TimeZone, Utc};

use crate::build_context::BuildContext;
use crate::data::computed_data::{ComputedData, ComputedDataBuilder, ComputedEvaluator, ComputedValue};
use crate::data::resolve_configuration_data::{self as rcd, EXCLUDE_FROM_COLLECTIONS_KEY};
use crate::page_entry::PageEntry;
use crate::plugins::pagination::Pagination;
use crate::template_content::TemplateContent;
use crate::template_file_slug::TemplateFileSlug;
use crate::template_layout::TemplateLayout;
use crate::template_permalink::TemplatePermalink;
use crate::user_config::{PreprocessorContext, PreprocessorResult, TransformContext};
use crate::util::date_parse::from_iso_to_date_utc;
use crate::util::merge::merge;
use crate::util::node_path::{self, ParsedPath};
use crate::util::{chalk, reserved_data, template_path, transforms_util, url_util};
use crate::value::{Map, Value};

/// `TemplateBehavior`
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RenderableOverride {
    Unset,
    Disabled,
    Optional,
}

pub struct OutputLocations {
    pub raw_path: Option<String>,
    pub href: Value,
    pub path: Value,
    pub dir: Value,
}

/// The result of writing (or rendering to JSON) a page
#[derive(Clone, Debug)]
pub struct PageResult {
    pub input_path: String,
    pub output_path: Value,
    pub url: Value,
    pub content: String,
    pub raw_input: String,
    pub data: Option<Value>,
}

impl PageResult {
    pub fn to_value(&self) -> Value {
        let mut m = Map::new();
        m.insert("url".into(), self.url.clone());
        m.insert("inputPath".into(), Value::str(&self.input_path));
        m.insert("outputPath".into(), self.output_path.clone());
        m.insert("rawInput".into(), Value::str(&self.raw_input));
        m.insert("content".into(), Value::str(&self.content));
        if let Some(d) = &self.data {
            m.insert("data".into(), d.clone());
        }
        Value::from_map(m)
    }
}

pub struct Template {
    pub content: TemplateContent,
    pub input_path: String,
    pub parsed: ParsedPath,
    pub file_slug_str: String,
    pub file_path_stem: String,
    renderable: Mutex<RenderableOverride>,
    data_cache: Mutex<Option<Value>>,
    computed: Mutex<Option<ComputedData>>,
    preprocessor_cache: Mutex<Option<(Option<String>, String)>>,
    pub write_count: AtomicUsize,
    pub render_count: AtomicUsize,
    pub is_dry_run: bool,
}

/// Evaluates computed data for a page of a template
struct Evaluator<'a> {
    template: &'a Template,
    extra_subdir: &'a str,
}

impl ComputedEvaluator for Evaluator<'_> {
    fn evaluate(&self, _key: &str, value: &ComputedValue, data: &Value) -> Result<Value> {
        match value {
            ComputedValue::Template(v) => self.template.render_computed_unit(v, data),
            ComputedValue::Function(f) => f(&[data.clone()]),
            ComputedValue::Static(v) => Ok(v.clone()),
            ComputedValue::PageUrl => Ok(self.template.get_output_locations(data, self.extra_subdir)?.href),
            ComputedValue::PageOutputPath => {
                Ok(self.template.get_output_locations(data, self.extra_subdir)?.path)
            }
            ComputedValue::PageDate => {
                let page = data.get("page");
                Ok(self
                    .template
                    .get_mapped_date(&data.get("date"), &page)?
                    .map(Value::Date)
                    .unwrap_or_default())
            }
        }
    }

    fn symbols(&self, value: &ComputedValue, data: &Value) -> Vec<String> {
        match value {
            ComputedValue::Template(v) => {
                let engine = self.template.content.get_computed_data_engine(data);
                self.template.content.parse_for_symbols(v, engine.as_ref())
            }
            _ => vec![],
        }
    }
}

impl Template {
    pub fn new(input_path: &str, ctx: Arc<BuildContext>, is_dry_run: bool) -> Self {
        let slug = TemplateFileSlug::new(input_path, &ctx.extension_map, &ctx.dirs.input());
        Template {
            parsed: node_path::parse(input_path),
            input_path: input_path.to_string(),
            file_slug_str: slug.get_slug(),
            file_path_stem: slug.get_full_path_without_extension(),
            content: TemplateContent::new(ctx, input_path),
            renderable: Mutex::new(RenderableOverride::Unset),
            data_cache: Mutex::new(None),
            computed: Mutex::new(None),
            preprocessor_cache: Mutex::new(None),
            write_count: AtomicUsize::new(0),
            render_count: AtomicUsize::new(0),
            is_dry_run,
        }
    }

    pub fn ctx(&self) -> &Arc<BuildContext> {
        &self.content.ctx
    }

    pub fn is_renderable(&self) -> bool {
        *self.renderable.lock().unwrap() != RenderableOverride::Disabled
    }

    pub fn set_renderable_override(&self, value: RenderableOverride) {
        *self.renderable.lock().unwrap() = value;
    }

    pub fn is_virtual_template(&self) -> bool {
        self.content.is_virtual_template()
    }

    fn keys(&self) -> &crate::user_config::Keys {
        &self.ctx().config.keys
    }

    pub fn get_template_subfolder(&self) -> String {
        let dir = template_path::absolute_path(&[&self.parsed.dir]);
        let input_dir = template_path::absolute_path(&[&self.ctx().dirs.input()]);
        if input_dir == "/" && dir.starts_with('/') {
            return dir;
        }
        template_path::strip_leading_sub_path(&dir, &input_dir)
    }

    pub fn templates_uses_layouts(&self, data: &Value) -> bool {
        let key = &self.keys().layout;
        data.get(key).is_truthy() && self.content.engine().map(|e| e.use_layouts()).unwrap_or(false)
    }

    pub fn get_layout(&self, layout_key: &str) -> Result<Arc<TemplateLayout>> {
        TemplateLayout::get_template(layout_key, self.ctx()).with_context(|| {
            format!(
                "Problem creating an Twelvety Layout for the \"{}\" template file.",
                self.input_path
            )
        })
    }

    pub fn base_file(&self) -> String {
        self.ctx().extension_map.remove_template_extension(&self.parsed.base)
    }

    fn use_permalink_root(&self, data: &Value) -> bool {
        data.get(&self.keys().permalink_root).is_truthy()
    }

    /// `_getLink`
    fn get_link(&self, data: &Value, extra_subdir: &str) -> Result<TemplatePermalink> {
        let keys = self.keys();
        let computed = rcd::get_value(data, &keys.computed);
        let mut permalink = rcd::get_value(data, &keys.permalink);
        if permalink.is_nullish() {
            permalink = computed.get(&keys.permalink);
        }
        let permalink = TemplatePermalink::resolve_build_alias(&permalink);
        let config = &self.ctx().config;
        let dynamic = config.dynamic_permalinks && data.get("dynamicPermalink").as_bool() != Some(false);
        let mut value: Option<Value> = None;
        match &permalink {
            Value::Bool(_) => value = Some(permalink.clone()),
            Value::Object(o) => {
                if !o.is_empty() {
                    bail!(
                        "Permalink objects are not supported (other than `permalink.build`). Use a string or `false` instead in {}.",
                        self.input_path
                    );
                }
            }
            p if p.is_truthy() && !dynamic => value = Some(p.clone()),
            p if p.is_truthy() => value = Some(self.content.render_permalink(p, data)?),
            _ => {}
        }
        if let Some(v) = value {
            let mut p = TemplatePermalink::new(&v, extra_subdir)?;
            p.set_url_transforms(&config.url_transforms);
            return Ok(p);
        }
        let subfolder = self.get_template_subfolder();
        let ext = self.content.engine()?.default_template_file_extension();
        let collapse = config.permalink_collapse_matching_folder;
        let generate = |c: bool| -> Result<TemplatePermalink> {
            let mut p = TemplatePermalink::generate(&subfolder, &self.base_file(), extra_subdir, &ext, c)?;
            p.set_url_transforms(&config.url_transforms);
            Ok(p)
        };
        let p = generate(collapse)?;
        if !collapse && !config.check_error_reporting("collapsedMatchingFolder", "allow") {
            let href = p.to_href().unwrap_or_default();
            let v3_href = generate(true)?.to_href().unwrap_or_default();
            if href != v3_href {
                let index_file = format!("index{}", &self.parsed.base[self.base_file().len()..]);
                self.ctx().warn_once(
                    &format!("collapse-matching-folder:{}", self.input_path),
                    &format!(
                        "{} now outputs to `{href}` (v3 output it to `{v3_href}`: file names that match their parent folder are no longer collapsed). To keep `{v3_href}`, rename it to `{index_file}` or set a `permalink`. Or use `$config.setPermalinkCollapseMatchingFolder(true)` to restore the v3 behavior, or `$config.configureErrorReporting({{ collapsedMatchingFolder: \"allow\" }})` to silence this warning.",
                        self.input_path
                    ),
                    true,
                );
            }
        }
        Ok(p)
    }

    pub fn get_output_locations(&self, data: &Value, extra_subdir: &str) -> Result<OutputLocations> {
        let link = self.get_link(data, extra_subdir)?;
        let path = if self.use_permalink_root(data) {
            link.to_path_from_root()
        } else {
            link.to_path(&self.content.output_dir())
        };
        let href = link.to_href();
        Ok(OutputLocations {
            raw_path: link.to_output_path(),
            dir: match &href {
                Some(h) => Value::str(url_util::get_directory_from_url(h)),
                None => Value::Bool(false),
            },
            href: href.map(Value::from).unwrap_or(Value::Bool(false)),
            path: path.map(Value::from).unwrap_or(Value::Bool(false)),
        })
    }

    fn get_cascade_sources(&self) -> Result<(Value, Vec<Value>, Value, Value, Value)> {
        let td = &self.ctx().template_data;
        let local = td.get_template_directory_data(&self.input_path)?;
        let local_sources = td.get_template_directory_data_sources(&self.input_path)?;
        let global = td.get_global_data()?;
        let fm = self.content.get_front_matter_data()?;
        let key = &self.keys().layout;
        let layout_key = [&fm, &local, &global]
            .iter()
            .map(|d| d.get(key))
            .find(|v| v.is_truthy())
            .unwrap_or_default();
        Ok((local, local_sources, global, fm, layout_key))
    }

    fn get_layout_data(&self, layout_key: &Value) -> Result<Value> {
        if !layout_key.is_truthy() {
            return Ok(Value::object());
        }
        self.get_layout(&layout_key.to_output_string())?.get_data()
    }

    fn compute_data(&self) -> Result<Value> {
        let (local, local_sources, global, fm, layout_key) = self.get_cascade_sources()?;
        let mut layout_data = Value::object();
        if self.content.engine()?.use_layouts() {
            layout_data = self.get_layout_data(&layout_key)?;
        }
        let config = &self.ctx().config;
        if config.freeze_reserved_data {
            let mut reserved = vec!["twelvety", "buildawesome"];
            if let Some(p) = &config.keys.package {
                reserved.push(p.as_str());
            }
            for d in [&layout_data, &local, &fm] {
                reserved_data::check_global_overrides(d, &reserved, Some(&self.input_path))?;
            }
        }
        let mut sources: Vec<&Value> = vec![&global, &layout_data];
        sources.extend(local_sources.iter());
        sources.push(&fm);
        let mut merged = merge(Value::object(), &sources);
        if config.freeze_reserved_data {
            reserved_data::check_subset(&merged, Some(&self.input_path))?;
        }
        let page = self.get_page_data(&merged)?;
        let mut page_wrapper = Map::new();
        page_wrapper.insert("page".into(), page);
        merged = merge(merged, &[&Value::from_map(page_wrapper)]);
        Ok(merged)
    }

    pub fn get_data(&self) -> Result<Value> {
        if let Some(d) = self.data_cache.lock().unwrap().as_ref() {
            return Ok(d.clone());
        }
        let d = self.compute_data()?;
        *self.data_cache.lock().unwrap() = Some(d.clone());
        Ok(d)
    }

    pub fn get_page_data(&self, data: &Value) -> Result<Value> {
        let ctx = self.ctx();
        let mut page = Map::new();
        page.insert("inputPath".into(), Value::str(&self.input_path));
        page.insert("fileSlug".into(), Value::str(&self.file_slug_str));
        page.insert("filePathStem".into(), Value::str(&self.file_path_stem));
        page.insert(
            "relativeInputPath".into(),
            Value::str(ctx.dirs.get_input_path_relative_to_input_directory(&self.input_path)),
        );
        page.insert(
            "outputFileExtension".into(),
            Value::str(self.content.engine()?.default_template_file_extension()),
        );
        page.insert(
            "templateSyntax".into(),
            Value::str(self.content.get_engine_names(self.content.get_engine_override(data).as_deref())),
        );
        let computed = rcd::get_value(data, &self.keys().computed);
        if !computed.is_plain_object() || !computed.has("date") {
            let page_value = Value::from_map(page.clone());
            if let Some(d) = self.get_mapped_date(&data.get("date"), &page_value)? {
                page.insert("date".into(), Value::Date(d));
            }
        }
        Ok(Value::from_map(page))
    }

    fn render_computed_unit(&self, entry: &Value, data: &Value) -> Result<Value> {
        Ok(match entry {
            Value::String(s) => Value::str(self.content.render_computed_data(s, data)?),
            Value::Function(f) => f(&[data.clone()])?,
            Value::Array(a) => Value::array(
                a.iter()
                    .map(|v| self.render_computed_unit(v, data))
                    .collect::<Result<Vec<_>>>()?,
            ),
            Value::Object(o) => {
                let mut m = Map::new();
                for (k, v) in o.iter() {
                    m.insert(k.clone(), self.render_computed_unit(v, data)?);
                }
                Value::from_map(m)
            }
            other => other.clone(),
        })
    }

    fn create_computed_data(&self, data: &Value) -> Result<Option<ComputedData>> {
        let keys = self.keys();
        let computed = rcd::get_value(data, &keys.computed);
        if !computed.is_plain_object() {
            return Ok(None);
        }
        let mut b = ComputedDataBuilder::new();
        b.add(
            "page.url",
            ComputedValue::PageUrl,
            vec!["permalink".into(), "dynamicPermalink".into(), keys.permalink_root.clone()],
        );
        b.add("page.outputPath", ComputedValue::PageOutputPath, vec!["page.url".into()]);
        if computed.has("date") {
            b.add("page.date", ComputedValue::PageDate, vec!["date".into()]);
        }
        if self.ctx().config.freeze_reserved_data {
            reserved_data::check_subset(&computed, Some(&self.input_path))?;
        }
        b.add_computed_entry(&computed, "", &[]);
        Ok(Some(b.build()))
    }

    fn unavailable_before_pagination(data: &Value) -> Vec<String> {
        let mut out = vec!["collections".to_string()];
        if Pagination::has_pagination(data) {
            out.extend(["pagination".into(), "page.url".into(), "page.outputPath".into()]);
            if let Some(alias) = data.get("pagination").get("alias").as_str() {
                out.push(alias.to_string());
            }
        }
        out
    }

    fn early_computed_keys(&self, cd: &ComputedData, data: &Value) -> Vec<String> {
        let keys = self.keys();
        let mut early: Vec<String> = vec!["page.date".into()];
        if let Some(t) = &keys.tags {
            early.push(t.clone());
        }
        early.extend(rcd::get_eligible_locations(&keys.import));
        early.extend(rcd::get_eligible_locations(EXCLUDE_FROM_COLLECTIONS_KEY));
        if let Some(d) = data.get("pagination").get("data").as_str() {
            early.push(d.to_string());
        }
        cd.computed_keys()
            .into_iter()
            .filter(|key| {
                early.iter().any(|name| {
                    key == name
                        || key.starts_with(&format!("{name}."))
                        || name.starts_with(&format!("{key}."))
                })
            })
            .collect()
    }

    /// First round of computed data (before preprocessors and pagination)
    fn setup_computed_data(&self, data: &mut Value) -> Result<()> {
        let Some(mut cd) = self.create_computed_data(data)? else {
            return Ok(());
        };
        let eval = Evaluator {
            template: self,
            extra_subdir: "",
        };
        // pagination keys
        let pagination_keys: Vec<String> = cd
            .computed_keys()
            .into_iter()
            .filter(|k| k == "pagination" || k.starts_with("pagination."))
            .collect();
        if !pagination_keys.is_empty() {
            let unavailable: Vec<String> = ["page.rawInput", "collections", "page.url", "page.outputPath"]
                .iter()
                .map(|s| s.to_string())
                .collect();
            cd.setup_data(data, &unavailable, Some(&pagination_keys), &eval)?;
            for key in &pagination_keys {
                if let Some((_, reason)) = cd.get_deferred(key) {
                    bail!(
                        "Computed `{key}` ({}) reads `{reason}`, which isn’t available until after pagination.",
                        self.input_path
                    );
                }
            }
        }
        let mut unavailable = vec!["page.rawInput".to_string()];
        unavailable.extend(Self::unavailable_before_pagination(data));
        // computed layout
        if cd.has_key(&self.keys().layout) && self.content.engine()?.use_layouts() {
            let layout_key = self.keys().layout.clone();
            cd.setup_data(data, &unavailable, Some(&[layout_key.clone()]), &eval)?;
            self.apply_computed_layout_data(data, &cd)?;
        }
        let early = self.early_computed_keys(&cd, data);
        cd.setup_data(data, &unavailable, Some(&early), &eval)?;
        *self.computed.lock().unwrap() = Some(cd);
        Ok(())
    }

    /// `#setupComputedLayout`: re-merge layout front matter when `layout` is computed
    fn apply_computed_layout_data(&self, data: &mut Value, cd: &ComputedData) -> Result<()> {
        let key = self.keys().layout.clone();
        if cd.get_deferred(&key).is_some() {
            return Ok(());
        }
        let (_, local_sources, global, fm, merged_layout_key) = self.get_cascade_sources()?;
        let layout_key = data.get(&key);
        if layout_key.strict_equals(&merged_layout_key) {
            return Ok(());
        }
        let previous = self.get_layout_data(&merged_layout_key)?;
        let layout_data = self.get_layout_data(&layout_key)?;
        let mut sources: Vec<&Value> = vec![&global, &layout_data];
        sources.extend(local_sources.iter());
        sources.push(&fm);
        let merged = merge(Value::object(), &sources);
        let computed_keys = cd.computed_keys();
        let mut names = previous.keys();
        names.extend(layout_data.keys());
        for name in names {
            if !computed_keys.contains(&name) {
                data.set(name.clone(), merged.get(&name));
            }
        }
        Ok(())
    }

    /// `runPreprocessors` (memoized): returns (skippedVia, content)
    pub fn run_preprocessors(&self, data: &mut Value) -> Result<(Option<String>, String)> {
        if let Some(r) = self.preprocessor_cache.lock().unwrap().as_ref() {
            return Ok(r.clone());
        }
        self.setup_computed_data(data)?;
        let mut content = self.content.get_pre_render()?;
        let mut skipped_via = None;
        for (name, pre) in &self.ctx().config.preprocessors {
            let filters: Vec<String> = pre
                .filter
                .iter()
                .map(|e| {
                    if e.starts_with('.') || e == "*" {
                        e.clone()
                    } else {
                        format!(".{e}")
                    }
                })
                .collect();
            if !filters.iter().any(|e| e == "*" || self.input_path.ends_with(e.as_str())) {
                continue;
            }
            let ctx = PreprocessorContext {
                input_path: self.input_path.clone(),
            };
            match (pre.callback)(&ctx, data, &content).with_context(|| {
                format!(
                    "Preprocessor `{name}` encountered an error when transforming {}.",
                    self.input_path
                )
            })? {
                PreprocessorResult::Skip => skipped_via = Some(name.clone()),
                PreprocessorResult::Content(c) => content = c,
                PreprocessorResult::Unchanged => {}
            }
        }
        let result = (skipped_via, content);
        *self.preprocessor_cache.lock().unwrap() = Some(result.clone());
        Ok(result)
    }

    /// `addComputedData` (second round): resolves `page.url`/`page.outputPath`
    fn add_computed_data(&self, data: &mut Value, parent: Option<&ComputedData>, extra_subdir: &str) -> Result<Option<ComputedData>> {
        let cd = match parent {
            Some(p) => Some(p.inherit()),
            None => self.computed.lock().unwrap().clone(),
        };
        if let Some(mut cd) = cd {
            let eval = Evaluator {
                template: self,
                extra_subdir,
            };
            if cd.has_key(&self.keys().layout) && self.content.engine()?.use_layouts() {
                let layout_key = self.keys().layout.clone();
                cd.setup_data(data, &["collections".into()], Some(&[layout_key]), &eval)?;
                self.apply_computed_layout_data(data, &cd)?;
            }
            cd.setup_data(data, &["collections".into()], None, &eval)?;
            return Ok(Some(cd));
        }
        if !data.has("page") {
            data.set("page", Value::object());
        }
        let page = data.get("page");
        if page.get("url").is_truthy() && page.get("outputPath").is_truthy() {
            return Ok(None);
        }
        let loc = self.get_output_locations(data, extra_subdir)?;
        let mut page = data.get("page");
        page.set("url", loc.href);
        page.set("outputPath", loc.path);
        data.set("page", page);
        Ok(None)
    }

    /// `getTemplates`: the page entries for this template (multiple with pagination)
    pub fn get_templates(self: &Arc<Self>, data: &mut Value) -> Result<Vec<Arc<PageEntry>>> {
        let (skipped, raw_input) = self.run_preprocessors(data)?;
        if skipped.is_some() {
            return Ok(vec![]);
        }
        let mut page = data.get("page");
        page.set("rawInput", Value::str(&raw_input));
        data.set("page", page);

        if !Pagination::has_pagination(data) {
            let cd = self.add_computed_data(data, None, "")?;
            return Ok(vec![PageEntry::new(
                self.clone(),
                String::new(),
                None,
                Value::Number(0.0),
                raw_input,
                data.clone(),
                cd,
            )]);
        }

        // pagination
        if let Some(key) = data.get("pagination").get("data").as_str() {
            if let Some(cd) = self.computed.lock().unwrap().as_ref() {
                if let Some((dkey, reason)) = cd.get_deferred(key) {
                    bail!(
                        "Pagination data `{key}` can’t use computed data `{dkey}` ({}) because it reads `{reason}`, which isn’t available until after pagination.",
                        self.input_path
                    );
                }
            }
        }
        {
            let mut guard = self.computed.lock().unwrap();
            if let Some(cd) = guard.as_mut() {
                let eval = Evaluator {
                    template: self,
                    extra_subdir: "",
                };
                cd.setup_data(data, &Self::unavailable_before_pagination(data), None, &eval)?;
            }
        }
        let paging = Pagination::new(self.clone(), data, &self.ctx().config)?;
        let parent_cd = self.computed.lock().unwrap().clone();
        let mut entries = vec![];
        for page in paging.get_page_templates()? {
            let mut page_data = page.data;
            let cd = self.add_computed_data(&mut page_data, parent_cd.as_ref(), &page.extra_subdir)?;
            entries.push(PageEntry::new(
                self.clone(),
                page.extra_subdir,
                Some(page.page_number),
                page.group_number,
                raw_input.clone(),
                page_data,
                cd,
            ));
        }
        Ok(entries)
    }

    /// Last round of computed data (collections are now available)
    pub fn resolve_remaining_computed_data(&self, entry: &PageEntry) -> Result<()> {
        let mut guard = entry.computed.lock().unwrap();
        let Some(cd) = guard.as_mut() else {
            return Ok(());
        };
        let mut data = entry.data();
        let eval = Evaluator {
            template: self,
            extra_subdir: &entry.extra_output_subdir,
        };
        cd.process_remaining_data(&mut data, &eval)?;
        *entry.data.write().unwrap() = data;
        Ok(())
    }

    /// `renderPageEntryWithoutLayout`
    pub fn render_page_entry_without_layout(&self, entry: &PageEntry) -> Result<String> {
        if let Some(c) = entry.template_content() {
            return Ok(c.clone());
        }
        let data = entry.data();
        let content = self.content.render_string(
            &entry.raw_input,
            &data,
            crate::template_content::RenderOptions {
                template_only: false,
                kind: "Content",
                step_inputs: Some(&entry.step_inputs),
            },
        )?;
        self.render_count.fetch_add(1, Ordering::Relaxed);
        Ok(content)
    }

    fn run_linters(&self, content: &str, entry: &PageEntry) -> Result<()> {
        let page = entry.page();
        let ctx = TransformContext {
            input_path: self.input_path.clone(),
            output_path: page.get("outputPath"),
            url: page.get("url"),
            page,
            base_href: None,
        };
        for linter in self.ctx().config.linters.values() {
            linter(&ctx, content)?;
        }
        Ok(())
    }

    /// `renderPageEntry` (layouts, linters and transforms)
    pub fn render_page_entry(&self, entry: &PageEntry) -> Result<String> {
        if let Some(c) = entry.rendered.lock().unwrap().as_ref() {
            return Ok(c.clone());
        }
        let data = entry.data();
        let template_content = entry
            .template_content()
            .cloned()
            .ok_or_else(|| anyhow!("Tried to use templateContent too early on {}", self.input_path))?;
        let layout_key = data.get(&self.keys().layout);
        let mut content = if self.content.engine()?.use_layouts() && layout_key.is_truthy() {
            let layout = self.get_layout(&layout_key.to_output_string())?;
            layout.render_layout_page_entry(entry, template_content)?
        } else {
            template_content
        };
        self.run_linters(&content, entry)?;
        content = transforms_util::run_all(
            content,
            &entry.page(),
            &self.ctx().config.transforms,
            None,
            Some(&self.ctx().logger),
        )?;
        *entry.rendered.lock().unwrap() = Some(content.clone());
        Ok(content)
    }

    pub fn retrieve_data_for_json_output(&self, data: &Value) -> Value {
        let selectors = &self.ctx().config.data_filter_selectors;
        if selectors.contains("*") {
            return data.clone();
        }
        let mut filtered = Value::object();
        for selector in selectors {
            filtered.set_path(selector, data.get_path(selector));
        }
        filtered
    }

    /// `_write`
    pub fn write(&self, entry: &PageEntry, content: String) -> Result<Option<PageResult>> {
        let ctx = self.ctx();
        let data = entry.data();
        let output_path = entry.output_path().to_output_string();
        if !self.use_permalink_root(&data) && !ctx.dirs.is_file_in_output_folder(&output_path) {
            bail!(
                "Output path `{output_path}` for {} is outside of the output directory. Check your `permalink` value.",
                self.input_path
            );
        }
        if self.is_dry_run {
            return Ok(None);
        }
        if ctx.logger.is_logging_enabled() {
            let engines = self
                .content
                .render()
                .ok()
                .and_then(|r| r.get_readable_engines_list_differing_from_file_extension());
            let suffix = format!(
                "{}{}",
                if self.is_virtual_template() { " (virtual)" } else { "" },
                engines.map(|e| format!(" ({e})")).unwrap_or_default()
            );
            ctx.logger.log(&format!(
                "Writing {output_path} {}",
                chalk::gray(&format!("from {}{suffix}", self.input_path))
            ));
        }
        if let Some(parent) = std::path::Path::new(&output_path).parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Could not create directory for {output_path}"))?;
        }
        let unchanged = ctx.config.skip_unchanged_writes
            && ctx.is_watch_or_serve()
            && std::fs::read(&output_path)
                .map(|existing| existing == content.as_bytes())
                .unwrap_or(false);
        if !unchanged {
            std::fs::write(&output_path, &content)
                .with_context(|| format!("Could not write {output_path}"))?;
        }
        self.write_count.fetch_add(1, Ordering::Relaxed);
        let data_out = if !ctx.config.data_filter_selectors.is_empty() {
            Some(self.retrieve_data_for_json_output(&data))
        } else {
            None
        };
        Ok(Some(PageResult {
            input_path: self.input_path.clone(),
            output_path: entry.output_path(),
            url: entry.url(),
            content,
            raw_input: entry.raw_input.clone(),
            data: data_out,
        }))
    }

    fn file_date(&self, created: bool) -> DateTime<Utc> {
        let Ok(meta) = std::fs::metadata(&self.input_path) else {
            return Utc::now();
        };
        let time = if created {
            meta.created().or_else(|_| meta.modified())
        } else {
            meta.modified()
        };
        match time {
            Ok(t) => DateTime::<Utc>::from(t),
            Err(_) => Utc::now(),
        }
    }

    /// `getMappedDate`
    pub fn get_mapped_date(&self, date_value: &Value, page: &Value) -> Result<Option<DateTime<Utc>>> {
        let config = &self.ctx().config;
        let mut date_value = date_value.clone();
        for f in &config.custom_date_parsing {
            if let Some(v) = f(&date_value, page) {
                if v.is_truthy() {
                    date_value = v;
                }
            }
        }
        if date_value.is_truthy() {
            return match &date_value {
                Value::Date(d) => Ok(Some(*d)),
                Value::Number(n) => Ok(Utc.timestamp_millis_opt(*n as i64).single()),
                Value::String(s) => {
                    if !self.is_virtual_template() {
                        let lower = s.to_lowercase();
                        match lower.as_str() {
                            "git last modified" => {
                                return Ok(Some(
                                    crate::util::git::get_updated_timestamp(&self.input_path)
                                        .and_then(|t| Utc.timestamp_millis_opt(t).single())
                                        .unwrap_or_else(Utc::now),
                                ));
                            }
                            "last modified" => return Ok(Some(self.file_date(false))),
                            "git created" => {
                                return Ok(Some(
                                    crate::util::git::get_created_timestamp(&self.input_path)
                                        .and_then(|t| Utc.timestamp_millis_opt(t).single())
                                        .unwrap_or_else(Utc::now),
                                ));
                            }
                            "created" => return Ok(Some(self.file_date(true))),
                            _ => {}
                        }
                    }
                    Ok(Some(from_iso_to_date_utc(
                        s,
                        &self.input_path,
                        config.date_time_zone.as_ref(),
                    )?))
                }
                other => bail!(
                    "Data cascade value for `date` ({}) is invalid for {}. Expected a JavaScript Date instance, luxon DateTime instance, or String value.",
                    other.to_js_string(),
                    self.input_path
                ),
            };
        }
        static FILE_DATE: std::sync::LazyLock<regex::Regex> =
            std::sync::LazyLock::new(|| regex::Regex::new(r"(\d{4}-\d{2}-\d{2})").unwrap());
        if let Some(c) = FILE_DATE.captures(&self.input_path) {
            return Ok(Some(from_iso_to_date_utc(
                &c[1],
                &self.input_path,
                config.date_time_zone.as_ref(),
            )?));
        }
        if self.is_virtual_template() {
            return Ok(Some(Utc::now()));
        }
        Ok(Some(self.file_date(true)))
    }
}
