//! A rendered page of a template (the objects in `map._pages` and collections upstream).
//!
//! Collections hold these entries by reference, so `templateContent` becomes
//! available on collection items once the page renders, just like the getter
//! upstream. Reading it too early records a "premature use" which fails the
//! render (and schedules a second pass).

use std::cell::RefCell;
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use crate::data::computed_data::ComputedData;
use crate::template::Template;
use crate::value::{DynObject, Map, Value};

thread_local! {
    static PREMATURE_USE: RefCell<Option<String>> = const { RefCell::new(None) };
}

pub fn clear_premature_use() {
    PREMATURE_USE.with(|p| *p.borrow_mut() = None);
}

pub fn take_premature_use() -> Option<String> {
    PREMATURE_USE.with(|p| p.borrow_mut().take())
}

fn record_premature_use(message: String) {
    PREMATURE_USE.with(|p| {
        let mut p = p.borrow_mut();
        if p.is_none() {
            *p = Some(message);
        }
    });
}

pub struct PageEntry {
    pub template: Arc<Template>,
    pub extra_output_subdir: String,
    pub input_path: String,
    pub file_slug: String,
    pub file_path_stem: String,
    pub page_number: Option<usize>,
    pub group_number: Value,
    pub raw_input: String,
    pub data: RwLock<Value>,
    pub computed: Mutex<Option<ComputedData>>,
    template_content: OnceLock<String>,
    /// Inputs to each pipeline step (e.g. Liquid output before Markdown)
    pub step_inputs: Mutex<Vec<String>>,
    /// Final rendered content (layouts + transforms), cached
    pub rendered: Mutex<Option<String>>,
}

impl std::fmt::Debug for PageEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PageEntry({})", self.input_path)
    }
}

impl PageEntry {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        template: Arc<Template>,
        extra_output_subdir: String,
        page_number: Option<usize>,
        group_number: Value,
        raw_input: String,
        data: Value,
        computed: Option<ComputedData>,
    ) -> Arc<Self> {
        Arc::new(PageEntry {
            input_path: template.input_path.clone(),
            file_slug: template.file_slug_str.clone(),
            file_path_stem: template.file_path_stem.clone(),
            template,
            extra_output_subdir,
            page_number,
            group_number,
            raw_input,
            data: RwLock::new(data),
            computed: Mutex::new(computed),
            template_content: OnceLock::new(),
            step_inputs: Mutex::new(vec![]),
            rendered: Mutex::new(None),
        })
    }

    pub fn data(&self) -> Value {
        self.data.read().unwrap().clone()
    }

    pub fn page(&self) -> Value {
        self.data.read().unwrap().get("page")
    }

    pub fn url(&self) -> Value {
        self.page().get("url")
    }

    pub fn output_path(&self) -> Value {
        self.page().get("outputPath")
    }

    pub fn date(&self) -> Value {
        self.page().get("date")
    }

    pub fn set_template_content(&self, content: String) {
        let _ = self.template_content.set(content);
    }

    pub fn template_content(&self) -> Option<&String> {
        self.template_content.get()
    }

    fn suffix(&self) -> String {
        match self.page_number {
            Some(n) if n > 0 => format!("{} (page {n})", self.input_path),
            _ => self.input_path.clone(),
        }
    }

    fn read_template_content(&self, prop: &str) -> Value {
        match self.template_content.get() {
            Some(c) => Value::str(c),
            None => {
                if self.template.is_renderable() {
                    record_premature_use(format!(
                        "Tried to use {prop} too early on {}",
                        self.suffix()
                    ));
                    Value::Undefined
                } else {
                    // TemplateContentUnrenderedTemplateError
                    record_premature_use(format!(
                        "Tried to use {prop} on unrendered template: {}",
                        self.suffix()
                    ));
                    Value::Undefined
                }
            }
        }
    }

    pub fn as_value(self: &Arc<Self>) -> Value {
        Value::Dynamic(self.clone())
    }

    fn content_pipeline(&self) -> Value {
        let Ok(names) = self.template.content.template_render.as_ref().map(|t| t.pipeline_step_names()) else {
            return Value::object();
        };
        let inputs = self.step_inputs.lock().unwrap();
        let mut m = Map::new();
        for (j, name) in names.iter().enumerate() {
            let v = if j == 0 {
                Value::str(&self.raw_input)
            } else {
                inputs.get(j - 1).map(Value::str).unwrap_or_default()
            };
            m.insert(name.clone(), v);
        }
        Value::from_map(m)
    }
}

const KEYS: &[&str] = &[
    "rawInput",
    "groupNumber",
    "data",
    "page",
    "inputPath",
    "fileSlug",
    "filePathStem",
    "date",
    "url",
    "outputPath",
    "content",
];

impl DynObject for PageEntry {
    fn get(&self, key: &str) -> Option<Value> {
        Some(match key {
            "rawInput" => Value::str(&self.raw_input),
            "groupNumber" => self.group_number.clone(),
            "pageNumber" => self.page_number.map(Value::from).unwrap_or_default(),
            "data" => self.data(),
            "page" => self.page(),
            "inputPath" => Value::str(&self.input_path),
            "fileSlug" => Value::str(&self.file_slug),
            "filePathStem" => Value::str(&self.file_path_stem),
            "date" => self.date(),
            "url" => self.url(),
            "outputPath" => self.output_path(),
            "templateContent" | "content" => self.read_template_content(key),
            "contentPipeline" => {
                if self.template_content.get().is_none() {
                    return Some(self.read_template_content(key));
                }
                self.content_pipeline()
            }
            _ => return None,
        })
    }

    fn keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = KEYS.iter().map(|s| s.to_string()).collect();
        if self.page_number.is_some() {
            keys.push("pageNumber".into());
        }
        keys
    }

    fn type_name(&self) -> &'static str {
        "page"
    }

    /// `getSerializableEntry`
    fn to_plain(&self) -> Value {
        let mut map = Map::new();
        for key in self.keys() {
            let value = match key.as_str() {
                "content" => match self.template_content.get() {
                    Some(c) => Value::str(c),
                    None => Value::str("[Internals: content queued to be rendered later]"),
                },
                "data" => {
                    let mut d = self.data();
                    if d.has("collections") {
                        d.set("collections", Value::str("[Internals: collections omitted]"));
                    }
                    d
                }
                other => self.get(other).unwrap_or_default(),
            };
            map.insert(key, value);
        }
        Value::from_map(map)
    }

    fn identity(&self) -> usize {
        self as *const Self as usize
    }
}

/// The shared `collections` object (filled in as the dependency graph is processed)
#[derive(Default)]
pub struct SharedCollections {
    map: RwLock<indexmap::IndexMap<String, Value>>,
}

impl SharedCollections {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn set(&self, name: &str, value: Value) {
        self.map.write().unwrap().insert(name.to_string(), value);
    }

    pub fn has(&self, name: &str) -> bool {
        self.map.read().unwrap().contains_key(name)
    }

    pub fn names(&self) -> Vec<String> {
        self.map.read().unwrap().keys().cloned().collect()
    }

    pub fn snapshot(&self) -> indexmap::IndexMap<String, Value> {
        self.map.read().unwrap().clone()
    }

    /// Break reference cycles (pages -> data -> collections -> pages)
    pub fn clear(&self) {
        self.map.write().unwrap().clear();
    }
}

impl DynObject for SharedCollections {
    fn get(&self, key: &str) -> Option<Value> {
        self.map.read().unwrap().get(key).cloned()
    }

    fn keys(&self) -> Vec<String> {
        self.names()
    }

    fn type_name(&self) -> &'static str {
        "collections"
    }

    fn to_plain(&self) -> Value {
        Value::from_map(
            self.map
                .read()
                .unwrap()
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        )
    }
}

/// `collections` when `configureFeatures({ collections: false })`
pub struct DisabledCollections;

impl DynObject for DisabledCollections {
    fn get(&self, key: &str) -> Option<Value> {
        record_premature_use(format!(
            "Collections are disabled via `configureFeatures({{ collections: false }})` (tried to read `collections.{key}`)."
        ));
        None
    }
    fn keys(&self) -> Vec<String> {
        vec![]
    }
}
