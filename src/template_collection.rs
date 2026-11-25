//! Port of `TemplateCollection.js` and `Util/Objects/Sortable.js`.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{Result, bail};
use indexmap::IndexMap;

use crate::data::template_data::{DEFAULT_TAGS_KEY, get_included_tag_names};
use crate::page_entry::PageEntry;
use crate::util::glob_matcher::is_glob_match;
use crate::util::template_path;
use crate::value::Value;

/// The static sort functions of `Sortable` (comparators return a JS-style number)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortFunction {
    NumericAscending,
    NumericDescending,
    AlphabeticAscending,
    AlphabeticDescending,
}

impl SortFunction {
    /// `sortFunctionStringMap`: `A-Z`, `Z-A`, `0-9`, `9-0`
    pub fn from_name(name: &str) -> Result<Self> {
        Ok(match name {
            "A-Z" => SortFunction::AlphabeticAscending,
            "Z-A" => SortFunction::AlphabeticDescending,
            "0-9" => SortFunction::NumericAscending,
            "9-0" => SortFunction::NumericDescending,
            _ => bail!(
                "Invalid String argument for sort(). Received `{name}`. Valid values: A-Z,Z-A,0-9,9-0"
            ),
        })
    }

    pub fn compare(self, a: &Value, b: &Value) -> f64 {
        match self {
            SortFunction::NumericAscending => a.to_number() - b.to_number(),
            SortFunction::NumericDescending => b.to_number() - a.to_number(),
            SortFunction::AlphabeticAscending => ascending(a, b),
            SortFunction::AlphabeticDescending => ascending(b, a),
        }
    }
}

/// `Sortable.sortFunctionAscending`: `a > b` is 1, `a < b` is -1, otherwise 0
fn ascending(a: &Value, b: &Value) -> f64 {
    match a.js_compare(b) {
        Some(Ordering::Greater) => 1.0,
        Some(Ordering::Less) => -1.0,
        _ => 0.0,
    }
}

/// Port of `Util/Objects/Sortable.js` (for plain values)
#[derive(Clone, Debug)]
pub struct Sortable {
    pub is_sort_ascending: bool,
    pub is_sort_numeric: bool,
    pub items: Vec<Value>,
}

impl Default for Sortable {
    fn default() -> Self {
        Self::new()
    }
}

impl Sortable {
    pub fn new() -> Self {
        Sortable {
            is_sort_ascending: true,
            is_sort_numeric: false,
            items: vec![],
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn add(&mut self, item: impl Into<Value>) {
        self.items.push(item.into());
    }

    /// `sort()` with the configured sort function
    pub fn sort(&self) -> Vec<Value> {
        self.sort_with(self.get_sort_function())
    }

    /// `sort("A-Z")` and friends
    pub fn sort_by_name(&self, name: &str) -> Result<Vec<Value>> {
        Ok(self.sort_with(SortFunction::from_name(name)?))
    }

    pub fn sort_with(&self, f: SortFunction) -> Vec<Value> {
        let mut items = self.items.clone();
        items.sort_by(|a, b| f.compare(a, b).partial_cmp(&0.0).unwrap_or(Ordering::Equal));
        items
    }

    pub fn sort_ascending(&self) -> Vec<Value> {
        self.sort_with(self.get_sort_function_ascending())
    }

    pub fn sort_descending(&self) -> Vec<Value> {
        self.sort_with(self.get_sort_function_descending())
    }

    pub fn set_sort_descending(&mut self, is_descending: bool) {
        self.is_sort_ascending = !is_descending;
    }

    pub fn set_sort_ascending(&mut self, is_ascending: bool) {
        self.is_sort_ascending = is_ascending;
    }

    pub fn set_sort_numeric(&mut self, is_numeric: bool) {
        self.is_sort_numeric = is_numeric;
    }

    pub fn get_sort_function(&self) -> SortFunction {
        if self.is_sort_ascending {
            self.get_sort_function_ascending()
        } else {
            self.get_sort_function_descending()
        }
    }

    pub fn get_sort_function_ascending(&self) -> SortFunction {
        if self.is_sort_numeric {
            SortFunction::NumericAscending
        } else {
            SortFunction::AlphabeticAscending
        }
    }

    pub fn get_sort_function_descending(&self) -> SortFunction {
        if self.is_sort_numeric {
            SortFunction::NumericDescending
        } else {
            SortFunction::AlphabeticDescending
        }
    }
}

/// `Sortable.sortFunctionDateInputPath`
pub fn sort_by_date_input_path(a: &PageEntry, b: &PageEntry) -> Ordering {
    let da = a.date();
    let db = b.date();
    let by_date = match (&da, &db) {
        (Value::Date(x), Value::Date(y)) => x.cmp(y),
        _ => Ordering::Equal,
    };
    if by_date == Ordering::Equal {
        return a.input_path.cmp(&b.input_path);
    }
    by_date
}

pub fn sort_by_date(a: &PageEntry, b: &PageEntry) -> Ordering {
    match (&a.date(), &b.date()) {
        (Value::Date(x), Value::Date(y)) => x.cmp(y),
        _ => Ordering::Equal,
    }
}

/// Sorted items, tag index and glob matches, reset by `add` (the only mutation of `items`)
#[derive(Default)]
struct Cache {
    sorted: Option<Vec<Arc<PageEntry>>>,
    tags: Option<Arc<IndexMap<String, Vec<Arc<PageEntry>>>>>,
    globs: HashMap<String, Vec<Arc<PageEntry>>>,
}

/// The collection API passed to `addCollection` callbacks
pub struct TemplateCollection {
    items: Vec<Arc<PageEntry>>,
    pub tags_key: Option<String>,
    cache: Mutex<Cache>,
}

impl Clone for TemplateCollection {
    fn clone(&self) -> Self {
        TemplateCollection {
            items: self.items.clone(),
            tags_key: self.tags_key.clone(),
            cache: Mutex::new(Cache::default()),
        }
    }
}

impl Default for TemplateCollection {
    fn default() -> Self {
        Self::new()
    }
}

fn to_value(items: &[Arc<PageEntry>]) -> Value {
    Value::array(items.iter().map(|i| i.as_value()).collect())
}

impl TemplateCollection {
    pub fn new() -> Self {
        TemplateCollection {
            items: vec![],
            tags_key: Some(DEFAULT_TAGS_KEY.to_string()),
            cache: Mutex::new(Cache::default()),
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn add(&mut self, item: Arc<PageEntry>) {
        self.items.push(item);
        self.refresh();
    }

    /// Drop the cached sort order and tag index (e.g. after computed data changed tags or dates)
    pub fn refresh(&mut self) {
        *self.cache.get_mut().unwrap() = Cache::default();
    }

    /// Entries in insertion order (for Rust callers)
    pub fn entries(&self) -> &[Arc<PageEntry>] {
        &self.items
    }

    /// `sort(sortFunction)`: a sorted copy of the entries
    pub fn sort_entries(
        &self,
        f: impl Fn(&PageEntry, &PageEntry) -> Ordering,
    ) -> Vec<Arc<PageEntry>> {
        let mut items = self.items.clone();
        items.sort_by(|a, b| f(a, b));
        items
    }

    /// Entries sorted by date, then input path
    pub fn sorted_entries(&self) -> Vec<Arc<PageEntry>> {
        let mut cache = self.cache.lock().unwrap();
        cache
            .sorted
            .get_or_insert_with(|| self.sort_entries(sort_by_date_input_path))
            .clone()
    }

    /// Tag name to entries (in sorted order), so tag lookups don’t walk the full collection
    fn tag_index(&self) -> Arc<IndexMap<String, Vec<Arc<PageEntry>>>> {
        if let Some(tags) = &self.cache.lock().unwrap().tags {
            return tags.clone();
        }
        let mut index: IndexMap<String, Vec<Arc<PageEntry>>> = IndexMap::new();
        for item in self.sorted_entries() {
            for tag in get_included_tag_names(&item.data(), self.tags_key.as_deref()) {
                index.entry(tag).or_default().push(item.clone());
            }
        }
        let index = Arc::new(index);
        self.cache.lock().unwrap().tags = Some(index.clone());
        index
    }

    pub fn get_all(&self) -> Value {
        to_value(&self.items)
    }

    pub fn get_all_sorted(&self) -> Value {
        to_value(&self.sorted_entries())
    }

    pub fn get_sorted_by_date(&self) -> Value {
        to_value(&self.sort_entries(sort_by_date))
    }

    pub fn get_tag_names(&self) -> Vec<String> {
        self.tag_index().keys().cloned().collect()
    }

    pub fn get_globs(globs: &[&str]) -> Vec<String> {
        globs
            .iter()
            .map(|g| template_path::add_leading_dot_slash(g))
            .collect()
    }

    pub fn get_filtered_by_glob(&self, glob: &str) -> Value {
        self.get_filtered_by_globs(&[glob])
    }

    pub fn get_filtered_by_globs(&self, globs: &[&str]) -> Value {
        let globs = Self::get_globs(globs);
        let key = globs.join("::");
        if let Some(filtered) = self.cache.lock().unwrap().globs.get(&key) {
            return to_value(filtered);
        }
        let filtered: Vec<Arc<PageEntry>> = self
            .sorted_entries()
            .into_iter()
            .filter(|i| is_glob_match(&i.input_path, &globs))
            .collect();
        let value = to_value(&filtered);
        self.cache.lock().unwrap().globs.insert(key, filtered);
        value
    }

    pub fn get_filtered_by_tag(&self, tag: &str) -> Value {
        if tag.is_empty() {
            return self.get_all_sorted();
        }
        to_value(
            self.tag_index()
                .get(tag)
                .map(|v| v.as_slice())
                .unwrap_or(&[]),
        )
    }

    pub fn get_filtered_by_tags(&self, tags: &[&str]) -> Value {
        let Some((first, rest)) = tags.split_first() else {
            return self.get_all_sorted();
        };
        let index = self.tag_index();
        let matches = index.get(*first).map(|v| v.as_slice()).unwrap_or(&[]);
        let filtered: Vec<Arc<PageEntry>> = matches
            .iter()
            .filter(|item| {
                rest.iter().all(|t| {
                    index
                        .get(*t)
                        .map(|list| list.iter().any(|x| Arc::ptr_eq(x, item)))
                        .unwrap_or(false)
                })
            })
            .cloned()
            .collect();
        to_value(&filtered)
    }
}
