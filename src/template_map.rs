//! Port of `TemplateMap.js`: data, collections and render order for all templates.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow, bail};
use indexmap::{IndexMap, IndexSet};

use crate::build_context::BuildContext;
use crate::data::resolve_configuration_data::{self as rcd, EXCLUDE_FROM_COLLECTIONS_KEY};
use crate::data::template_data::{get_included_collection_names, get_included_tag_names};
use crate::errors::{
    DuplicatePermalinkOutputError, UsingCircularTemplateContentReferenceError,
    is_premature_template_content_error,
};
use crate::global_dependency_map::GlobalDependencyMap;
use crate::page_entry::{DisabledCollections, PageEntry, SharedCollections};
use crate::template::Template;
use crate::template_collection::TemplateCollection;
use crate::template_writer::attach_file_path;
use crate::util::template_path;
use crate::value::{Map, Value};

const EXTENSIONLESS_URL_ALLOWLIST: &[&str] = &["/_redirects", "/.htaccess", "/_headers"];

pub struct MapEntry {
    pub template: Arc<Template>,
    pub input_path: String,
    pub data: Value,
    pub pages: Vec<Arc<PageEntry>>,
}

pub struct TemplateMap {
    ctx: Arc<BuildContext>,
    pub map: Vec<MapEntry>,
    input_path_index: HashMap<String, usize>,
    pub collection: TemplateCollection,
    pub collections: Arc<SharedCollections>,
    pub cached: bool,
    dependency_map_initialized: bool,
}

impl TemplateMap {
    pub fn new(ctx: Arc<BuildContext>) -> Self {
        let mut collection = TemplateCollection::new();
        collection.tags_key = ctx.config.keys.tags.clone();
        TemplateMap {
            ctx,
            map: vec![],
            input_path_index: HashMap::new(),
            collection,
            collections: SharedCollections::new(),
            cached: false,
            dependency_map_initialized: false,
        }
    }

    fn collections_disabled(&self) -> bool {
        !self.ctx.config.features_collections()
    }

    pub fn add(&mut self, template: Arc<Template>) -> Result<()> {
        let with_path = |e| attach_file_path(e, &template.input_path);
        let mut data = template.get_data().map_err(with_path)?;
        let (skipped, _) = template.run_preprocessors(&mut data).map_err(with_path)?;
        if skipped.is_some() {
            return Ok(());
        }
        let input_path = template.input_path.clone();
        self.input_path_index
            .insert(template_path::absolute_path(&[&input_path]), self.map.len());
        self.map.push(MapEntry {
            template,
            input_path,
            data,
            pages: vec![],
        });
        Ok(())
    }

    fn get_index(&self, input_path: &str) -> Option<usize> {
        self.input_path_index
            .get(&template_path::absolute_path(&[input_path]))
            .copied()
    }

    pub fn get_map_entry_for_input_path(&self, input_path: &str) -> Option<&MapEntry> {
        self.get_index(input_path).map(|i| &self.map[i])
    }

    pub fn get_tag_target(s: &str) -> Option<String> {
        if s == "collections" {
            return Some("[keys]".into());
        }
        if let Some(rest) = s.strip_prefix("collections.") {
            return Some(rest.to_string());
        }
        if s.starts_with("collections['") || s.starts_with("collections[\"") {
            return Some(s["collections['".len()..s.len() - 2].to_string());
        }
        None
    }

    fn get_pagination_tag_target(data: &Value) -> Option<String> {
        data.get("pagination")
            .get("data")
            .as_str()
            .and_then(Self::get_tag_target)
    }

    pub fn add_all_to_global_dependency_graph(&mut self) -> Result<()> {
        self.dependency_map_initialized = true;
        let mut uses = self.ctx.uses.lock().unwrap();
        let names: Vec<String> = self.ctx.config.collections.keys().cloned().collect();
        uses.add_collection_api_names(&names);
        let keys = &self.ctx.config.keys;
        for entry in &self.map {
            let mut consumes: Vec<String> = vec![];
            if let Some(t) = Self::get_pagination_tag_target(&entry.data) {
                consumes.push(t);
            }
            let import = rcd::get_value(&entry.data, &keys.import);
            if let Value::Array(tags) = import.get("collections") {
                consumes.extend(tags.iter().map(|t| t.to_output_string()));
            }
            let publishes = if self.collections_disabled() {
                if !consumes.is_empty() {
                    bail!(
                        "Collections are disabled via `configureFeatures({{ collections: false }})`: {} consumes collections ({}) via `pagination.data` or `{}.collections`.",
                        entry.input_path,
                        consumes.join(", "),
                        keys.import
                    );
                }
                vec![]
            } else {
                get_included_collection_names(&entry.data, keys.tags.as_deref())
            };
            uses.add_new_node_relationships(&entry.input_path, &consumes, &publishes);
        }
        Ok(())
    }

    pub fn is_user_config_collection_name(&self, name: &str) -> bool {
        self.ctx.config.collections.contains_key(name)
    }

    pub fn get_user_config_collection_names(&self) -> Vec<String> {
        self.ctx.config.collections.keys().cloned().collect()
    }

    fn is_deferred_user_config_collection_name(
        &self,
        name: &str,
        ignored: &IndexSet<String>,
    ) -> bool {
        self.is_user_config_collection_name(name)
            && self.ctx.config.get_collection_options(name).defer
            && !ignored.contains(name)
    }

    fn set_collection_by_tag_name(&mut self, tag_name: &str) -> Result<()> {
        let value = if self.is_user_config_collection_name(tag_name) {
            let callback = self.ctx.config.collections[tag_name].clone();
            callback(&self.collection).with_context(|| {
                format!("Error in the `{tag_name}` collection (via `addCollection`)")
            })?
        } else {
            self.get_tagged_collection(tag_name)
        };
        let mut value = value;
        if let Some(pre) = self.ctx.config.precompiled_collections.get(tag_name)
            && (tag_name == "all" || !value.is_array() || value.is_empty())
        {
            value = pre.clone();
        }
        self.collections.set(tag_name, value);
        Ok(())
    }

    pub fn get_tagged_collection(&self, tag: &str) -> Value {
        if tag.is_empty() || tag == "all" {
            self.collection.get_all_sorted()
        } else {
            self.collection.get_filtered_by_tag(tag)
        }
    }

    /// Input paths and `__collection:` entries in processing order
    pub fn get_template_order(&self) -> Vec<String> {
        let order = self.ctx.uses.lock().unwrap().get_template_order();
        order
            .into_iter()
            .filter_map(|entry| {
                if GlobalDependencyMap::is_collection(&entry) {
                    return Some(entry);
                }
                let input_path = template_path::add_leading_dot_slash(&entry);
                self.get_index(&input_path).map(|_| input_path)
            })
            .collect()
    }

    fn init_dependency_map(&mut self, order: &[String]) -> Result<()> {
        // `defer: true` is ignored for collections that are paginated over
        let mut ignored_defer: IndexSet<String> = IndexSet::new();
        for entry in &self.map {
            if let Some(name) = Self::get_pagination_tag_target(&entry.data)
                && self.is_deferred_user_config_collection_name(&name, &ignored_defer)
            {
                ignored_defer.insert(name.clone());
                self.ctx.logger.warn(&format!(
                        "Warning: `defer: true` was ignored for `collections.{name}` (via `addCollection`) because {} paginates over it.",
                        entry.input_path
                    ));
            }
        }
        for dep in order {
            if GlobalDependencyMap::is_collection(dep) {
                if self.collections_disabled() {
                    continue;
                }
                let tag = GlobalDependencyMap::get_tag_name(dep).unwrap_or_default();
                if tag == "[keys]" {
                    self.set_collection_by_tag_name("all")?;
                    let names: Vec<String> = self.ctx.config.collections.keys().cloned().collect();
                    for name in names {
                        if self.is_deferred_user_config_collection_name(&name, &ignored_defer) {
                            self.set_collection_by_tag_name(&name)?;
                        }
                    }
                } else if !(tag.starts_with('[') && tag.ends_with(']'))
                    && !self.is_deferred_user_config_collection_name(&tag, &ignored_defer)
                {
                    self.set_collection_by_tag_name(&tag)?;
                }
                continue;
            }
            let Some(idx) = self.get_index(dep) else {
                continue;
            };
            self.init_dependency_map_entry(idx)?;
        }
        if self.collections_disabled() {
            return Ok(());
        }
        for tag in self.collection.get_tag_names() {
            if !self.is_user_config_collection_name(&tag) {
                self.set_collection_by_tag_name(&tag)?;
            }
        }
        self.check_tag_case_collisions()
    }

    fn check_tag_case_collisions(&self) -> Result<()> {
        let config = &self.ctx.config;
        if config.check_error_reporting("tagCaseCollisions", "allow") {
            return Ok(());
        }
        let mut groups: IndexMap<String, Vec<String>> = IndexMap::new();
        for tag in self.collection.get_tag_names() {
            groups.entry(tag.to_lowercase()).or_default().push(tag);
        }
        let mut messages = vec![];
        for tags in groups.values() {
            if tags.len() < 2 {
                continue;
            }
            let details: Vec<String> = tags
                .iter()
                .map(|tag| {
                    let paths: IndexSet<String> = self
                        .collection
                        .get_filtered_by_tag(tag)
                        .to_vec()
                        .iter()
                        .map(|i| i.get("inputPath").to_output_string())
                        .collect();
                    format!(
                        "`{tag}` ({})",
                        paths.into_iter().collect::<Vec<_>>().join(", ")
                    )
                })
                .collect();
            messages.push(format!(
                "tags that differ only by case create separate collections: {}",
                details.join(", ")
            ));
        }
        if messages.is_empty() {
            return Ok(());
        }
        if !config.check_error_reporting("tagCaseCollisions", "warn") {
            bail!(
                "{}\nTo downgrade this to a warning, use `$config.configureErrorReporting({{ tagCaseCollisions: \"warn\" }})` (or `\"allow\"` to skip this check).",
                messages.join("\n")
            );
        }
        for m in messages {
            self.ctx.logger.warn(&format!("Warning: {m}"));
        }
        Ok(())
    }

    fn init_dependency_map_entry(&mut self, idx: usize) -> Result<()> {
        let template = self.map[idx].template.clone();
        let mut data = std::mem::take(&mut self.map[idx].data);
        let result = template.get_templates(&mut data);
        self.map[idx].data = data;
        let pages = result.map_err(|e| {
            attach_file_path(e, &self.map[idx].input_path).context(format!(
                "Error generating template page(s) for {}.",
                self.map[idx].input_path
            ))
        })?;
        let add_all = self.map[idx]
            .data
            .get("pagination")
            .get("addAllPagesToCollections")
            .is_truthy();
        for (counter, page) in pages.iter().enumerate() {
            if counter == 0 || add_all {
                let excluded = rcd::get_value(&page.data(), EXCLUDE_FROM_COLLECTIONS_KEY).as_bool()
                    == Some(true);
                if !self.collections_disabled() && !excluded {
                    self.collection.add(page.clone());
                }
            }
        }
        self.map[idx].pages = pages;
        Ok(())
    }

    /// `cache()`: process the dependency graph, data and content
    pub fn cache(&mut self) -> Result<()> {
        if !self.dependency_map_initialized {
            self.add_all_to_global_dependency_graph()?;
        }
        // Every page is added again below, so repeated calls (e.g. --watch) don’t accumulate
        let tags_key = self.collection.tags_key.clone();
        self.collection = TemplateCollection::new();
        self.collection.tags_key = tags_key;
        for entry in self.map.iter_mut() {
            entry.pages.clear();
        }
        let collections_value = if self.collections_disabled() {
            Value::Dynamic(Arc::new(DisabledCollections))
        } else {
            Value::Dynamic(self.collections.clone())
        };
        for entry in self.map.iter_mut() {
            entry.data.set("collections", collections_value.clone());
        }
        let order = self.get_template_order();
        self.init_dependency_map(&order)?;
        self.resolve_remaining_computed_data()?;
        // tags may have changed via computed data
        self.collection.refresh();

        let ordered: Vec<usize> = order
            .iter()
            .filter(|d| !GlobalDependencyMap::is_collection(d))
            .filter_map(|d| self.get_index(d))
            .collect();

        let events = self.ctx.config.events.clone();
        events.emit_lazy("buildawesome.contentmap", || {
            let mut m = Map::new();
            m.insert(
                "inputPathToUrl".into(),
                self.generate_input_url_content_map(&ordered),
            );
            m.insert("urlToInputPath".into(), self.generate_url_map(&ordered));
            Value::from_map(m)
        })?;

        self.run_data_schemas(&ordered)?;
        self.populate_content_data_in_map(&ordered)?;
        self.cached = true;
        self.check_for_duplicate_permalinks()?;
        self.check_for_missing_file_extensions()?;
        let layouts = self.generate_layouts_map()?;
        events.emit("buildawesome.layouts", &layouts)?;
        Ok(())
    }

    pub fn generate_input_url_content_map(&self, ordered: &[usize]) -> Value {
        let mut m = Map::new();
        for &i in ordered {
            let entry = &self.map[i];
            m.insert(
                entry.input_path.clone(),
                Value::array(entry.pages.iter().map(|p| p.url()).collect()),
            );
        }
        Value::from_map(m)
    }

    pub fn generate_url_map(&self, ordered: &[usize]) -> Value {
        let mut m = Map::new();
        for &i in ordered {
            let entry = &self.map[i];
            for page in &entry.pages {
                let mut e = Map::new();
                e.insert("inputPath".into(), Value::str(&entry.input_path));
                e.insert("groupNumber".into(), page.group_number.clone());
                m.insert(page.url().to_output_string(), Value::from_map(e));
            }
        }
        Value::from_map(m)
    }

    fn resolve_remaining_computed_data(&self) -> Result<()> {
        let key = &self.ctx.config.keys.computed;
        for entry in &self.map {
            for page in &entry.pages {
                if !rcd::get_value(&page.data(), key).is_undefined() {
                    entry.template.resolve_remaining_computed_data(page)?;
                }
            }
        }
        Ok(())
    }

    fn run_data_schemas(&self, ordered: &[usize]) -> Result<()> {
        let key = &self.ctx.config.keys.data_schema;
        for &i in ordered {
            let entry = &self.map[i];
            for page in &entry.pages {
                let data = page.data();
                if let Value::Function(f) = rcd::get_value(&data, key) {
                    f(std::slice::from_ref(&data)).with_context(|| {
                        format!(
                            "Error in the data schema for: {} (via `{}`)",
                            entry.input_path,
                            rcd::get_eligible_locations(key).join("` or `")
                        )
                    })?;
                }
            }
        }
        Ok(())
    }

    fn render_entry_content(&self, idx: usize) -> Result<()> {
        let entry = &self.map[idx];
        for page in &entry.pages {
            if page.template_content().is_some() {
                continue;
            }
            let content = entry
                .template
                .render_page_entry_without_layout(page)
                .map_err(|e| attach_file_path(e, &entry.input_path))?;
            page.set_template_content(content);
        }
        Ok(())
    }

    fn populate_content_data_in_map(&self, ordered: &[usize]) -> Result<()> {
        let mut too_early = vec![];
        for &i in ordered {
            if !self.map[i].template.is_renderable() {
                continue;
            }
            if let Err(e) = self.render_entry_content(i) {
                if is_premature_template_content_error(&e) {
                    too_early.push(i);
                } else {
                    return Err(e);
                }
            }
        }
        for i in too_early {
            if let Err(e) = self.render_entry_content(i) {
                if is_premature_template_content_error(&e) {
                    return Err(anyhow::Error::new(UsingCircularTemplateContentReferenceError(format!(
                        "{} contains a circular reference (using collections) to its own templateContent.",
                        self.map[i].input_path
                    )))
                    .context(e));
                }
                return Err(e);
            }
        }
        Ok(())
    }

    fn generate_layouts_map(&self) -> Result<Value> {
        let mut layouts: IndexMap<String, IndexSet<String>> = IndexMap::new();
        let key = &self.ctx.config.keys.layout;
        for entry in &self.map {
            for page in &entry.pages {
                let data = page.data();
                if !entry.template.templates_uses_layouts(&data) {
                    continue;
                }
                let layout = entry
                    .template
                    .get_layout(&data.get(key).to_output_string())?;
                let chain = layout.get_layout_chain()?;
                let mut priors: Vec<String> = vec![];
                for filepath in chain {
                    let set = layouts.entry(filepath.clone()).or_default();
                    set.insert(page.input_path.clone());
                    for p in &priors {
                        set.insert(p.clone());
                    }
                    priors.push(filepath);
                }
            }
        }
        let mut m = Map::new();
        for (k, v) in layouts {
            m.insert(k, Value::array(v.into_iter().map(Value::from).collect()));
        }
        Ok(Value::from_map(m))
    }

    fn check_for_duplicate_permalinks(&self) -> Result<()> {
        let allow_case = self
            .ctx
            .config
            .check_error_reporting("caseCollisions", "allow");
        let to_key = |p: &str| {
            if allow_case {
                p.to_string()
            } else {
                p.to_lowercase()
            }
        };
        let mut inputs: HashMap<String, bool> = HashMap::new();
        let mut output_paths: IndexMap<String, Vec<(String, String, Option<usize>)>> =
            IndexMap::new();
        let mut warnings: IndexMap<String, String> = IndexMap::new();
        for entry in &self.map {
            for page in &entry.pages {
                let output = page.output_path();
                let url = page.url();
                if output.as_bool() == Some(false) || url.as_bool() == Some(false) {
                    continue;
                }
                let output = output.to_output_string();
                if to_key(&output) == to_key(&page.input_path) {
                    bail!(DuplicatePermalinkOutputError(format!(
                        "The template at \"{}\" attempted to overwrite itself.",
                        page.input_path
                    )));
                }
                let out_key = to_key(&output);
                if inputs.contains_key(&out_key) {
                    bail!(DuplicatePermalinkOutputError(format!(
                        "The template at \"{}\" attempted to overwrite an existing template at \"{output}\".",
                        page.input_path
                    )));
                }
                inputs.insert(to_key(&page.input_path), true);
                let e = (entry.input_path.clone(), output.clone(), page.page_number);
                match output_paths.get_mut(&out_key) {
                    None => {
                        output_paths.insert(out_key, vec![e]);
                    }
                    Some(list) => {
                        let mut entries = vec![e.clone()];
                        entries.extend(list.iter().cloned());
                        let differs = entries.iter().any(|x| x.1 != output);
                        let listing: String = entries
                            .iter()
                            .enumerate()
                            .map(|(i, (ip, op, pn))| {
                                format!(
                                    "  {}. {ip}{}{}\n",
                                    i + 1,
                                    pn.map(|n| format!(" (pagination.pageNumber: {n})"))
                                        .unwrap_or_default(),
                                    if differs {
                                        format!(" → {op}")
                                    } else {
                                        String::new()
                                    }
                                )
                            })
                            .collect();
                        let msg = if entries.iter().all(|x| x.0 == entry.input_path) {
                            format!(
                                "Output conflict: multiple pages from pagination template `{}` are writing to `{output}`. Often this means you used an unknown property which resolved to an empty string \"\" in your permalink.\n{listing}",
                                entry.input_path
                            )
                        } else {
                            format!(
                                "Output conflict: multiple input files are writing to `{output}`. Use distinct `permalink` values to resolve this conflict.\n{listing}"
                            )
                        };
                        warnings.insert(out_key.clone(), msg);
                        list.push(e);
                    }
                }
            }
        }
        if let Some((_, w)) = warnings.into_iter().next() {
            bail!(DuplicatePermalinkOutputError(w));
        }
        Ok(())
    }

    fn check_for_missing_file_extensions(&self) -> Result<()> {
        if self
            .ctx
            .config
            .check_error_reporting("missingExtensions", "allow")
        {
            return Ok(());
        }
        for entry in &self.map {
            for page in &entry.pages {
                let output = page.output_path();
                let url = page.url();
                let data = page.data();
                if output.as_bool() == Some(false)
                    || url.as_bool() == Some(false)
                    || data.get("twelvetyAllowMissingExtension").is_truthy()
                    || EXTENSIONLESS_URL_ALLOWLIST
                        .iter()
                        .any(|u| url.to_output_string().ends_with(u))
                {
                    continue;
                }
                let output = output.to_output_string();
                if template_path::get_extension(&output).is_empty() {
                    let permalink = data.get("permalink");
                    bail!(
                        "The template at '{}' attempted to write to '{output}'{}, which is a target on the file system that does not include a file extension.\nYou *probably* want to add a file extension to your permalink so that hosts will know how to correctly serve this file to web browsers. Without a file extension, this file may not be reliably deployed without additional hosting configuration (it won’t have a mime type) and may also cause local development issues if you later attempt to write to a subdirectory of the same name.\nLearn more: https://v3.11ty.dev/docs/permalinks/#trailing-slashes\nThis is usually but not *always* an error so if you’d like to disable this error message, add `twelvetyAllowMissingExtension: true` somewhere in the data cascade for this template or use `$config.configureErrorReporting({{ missingExtensions: \"allow\" }});` to disable this feature globally.",
                        page.input_path,
                        if permalink.is_truthy() {
                            format!(
                                " (via `permalink` value: '{}')",
                                permalink.to_output_string()
                            )
                        } else {
                            String::new()
                        }
                    );
                }
            }
        }
        Ok(())
    }

    /// `_testGetCollectionsData`: the `collections` object (processing the map first if needed)
    pub fn test_get_collections_data(&mut self) -> Result<Value> {
        if !self.cached {
            self.cache()?;
        }
        Ok(Value::from_map(
            self.collections.snapshot().into_iter().collect(),
        ))
    }

    /// `_testGetAllCollectionsData`: tagged collections (by the map’s `tags` data) and config collections
    pub fn test_get_all_collections_data(&self) -> Result<Value> {
        let mut collections = Map::new();
        collections.insert("all".into(), self.collection.get_all_sorted());
        let tags_key = self.ctx.config.keys.tags.clone().unwrap_or_default();
        for entry in &self.map {
            if let Value::Array(tags) = entry.data.get(&tags_key) {
                for tag in tags.iter() {
                    let tag = tag.to_output_string();
                    collections.insert(tag.clone(), self.collection.get_filtered_by_tag(&tag));
                }
            }
        }
        for (name, callback) in &self.ctx.config.collections {
            collections.insert(name.clone(), callback(&self.collection)?);
        }
        Ok(Value::from_map(collections))
    }

    /// Break reference cycles once the build is complete
    pub fn dispose(&self) {
        self.collections.clear();
    }

    pub fn tag_names_for(&self, data: &Value) -> Vec<String> {
        get_included_tag_names(data, self.ctx.config.keys.tags.as_deref())
    }

    pub fn error_for_missing(&self, input_path: &str) -> anyhow::Error {
        anyhow!("Internal error: _pages not found for {input_path}")
    }
}
