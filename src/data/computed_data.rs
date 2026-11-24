//! Port of `Data/ComputedData.js` and `Data/Signals.js`.
//!
//! Entries are pulled in dependency order. Dependencies of template strings are
//! found by parsing the template for symbols; functions read the data through a
//! tracking proxy (a `DynObject`) and re-run after each unresolved computed key
//! they hit, like upstream’s Proxy-based `#runFunction`.

use std::sync::{Arc, Mutex};

use anyhow::{Result, bail};
use indexmap::{IndexMap, IndexSet};

use crate::value::{DynObject, Map, NativeFn, Value};

#[derive(Clone)]
pub enum ComputedValue {
    /// A template string (or array/object of template strings) rendered with the computed data engine
    Template(Value),
    /// A function of the data
    Function(Arc<NativeFn>),
    /// A literal value
    Static(Value),
    /// Internal: `page.url`
    PageUrl,
    /// Internal: `page.outputPath`
    PageOutputPath,
    /// Internal: `page.date` (when `date` is computed)
    PageDate,
}

#[derive(Clone)]
pub struct ComputedEntry {
    pub value: ComputedValue,
    pub declared_dependencies: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum State {
    Idle,
    Done,
    Deferred,
}

#[derive(Clone, Debug)]
struct Signal {
    state: State,
    deps: IndexSet<String>,
    reason: Option<String>,
}

impl Signal {
    fn new() -> Self {
        Signal {
            state: State::Idle,
            deps: IndexSet::new(),
            reason: None,
        }
    }
    fn defer(&mut self, reason: &str) {
        self.state = State::Deferred;
        self.reason = Some(reason.to_string());
    }
    fn reset(&mut self) {
        self.state = State::Idle;
        self.deps.clear();
        self.reason = None;
    }
}

/// Evaluates computed entries (supplied by `Template`)
pub trait ComputedEvaluator {
    fn evaluate(&self, key: &str, value: &ComputedValue, data: &Value) -> Result<Value>;
    fn symbols(&self, value: &ComputedValue, data: &Value) -> Vec<String>;
}

#[derive(Clone)]
pub struct ComputedData {
    entries: Arc<IndexMap<String, ComputedEntry>>,
    signals: IndexMap<String, Signal>,
    unavailable: Vec<String>,
}

pub struct ComputedDataBuilder {
    entries: IndexMap<String, ComputedEntry>,
}

impl Default for ComputedDataBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ComputedDataBuilder {
    pub fn new() -> Self {
        Self {
            entries: IndexMap::new(),
        }
    }

    pub fn add(&mut self, key: &str, value: ComputedValue, deps: Vec<String>) {
        self.entries.insert(
            key.to_string(),
            ComputedEntry {
                value,
                declared_dependencies: deps,
            },
        );
    }

    /// `Template#_addComputedEntry`: flattens nested objects into dotted keys
    pub fn add_computed_entry(&mut self, obj: &Value, parent_key: &str, deps: &[String]) {
        match obj {
            Value::Function(f) => self.add(parent_key, ComputedValue::Function(f.clone()), deps.to_vec()),
            Value::Array(_) | Value::String(_) => {
                self.add(parent_key, ComputedValue::Template(obj.clone()), deps.to_vec())
            }
            Value::Object(map) => {
                for (k, v) in map.iter() {
                    let child = if parent_key.is_empty() {
                        k.clone()
                    } else {
                        format!("{parent_key}.{k}")
                    };
                    self.add_computed_entry(v, &child, deps);
                }
            }
            other => self.add(parent_key, ComputedValue::Static(other.clone()), deps.to_vec()),
        }
    }

    pub fn build(self) -> ComputedData {
        let signals = self.entries.keys().map(|k| (k.clone(), Signal::new())).collect();
        ComputedData {
            entries: Arc::new(self.entries),
            signals,
            unavailable: vec![],
        }
    }
}

fn overlaps(a: &str, b: &str) -> bool {
    a == b || a.starts_with(&format!("{b}.")) || b.starts_with(&format!("{a}."))
}

impl ComputedData {
    pub fn computed_keys(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }

    pub fn has_key(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    fn is_unavailable(&self, path: &str) -> bool {
        self.unavailable
            .iter()
            .any(|e| path == e || path.starts_with(&format!("{e}.")))
    }

    fn has_unavailable_child(&self, path: &str) -> bool {
        self.unavailable.iter().any(|e| e.starts_with(&format!("{path}.")))
    }

    /// A fresh copy for a paginated page that inherits finished entries
    pub fn inherit(&self) -> ComputedData {
        let mut c = self.clone();
        for s in c.signals.values_mut() {
            if s.state != State::Done {
                s.reset();
            }
        }
        c
    }

    pub fn setup_data(
        &mut self,
        data: &mut Value,
        unavailable: &[String],
        keys: Option<&[String]>,
        evaluator: &dyn ComputedEvaluator,
    ) -> Result<()> {
        for s in self.signals.values_mut() {
            if s.state == State::Deferred {
                s.reset();
            }
        }
        self.unavailable = unavailable.to_vec();
        let keys: Vec<String> = match keys {
            Some(k) => k.to_vec(),
            None => self.entries.keys().cloned().collect(),
        };
        for key in keys {
            if self.entries.contains_key(&key) {
                self.pull(&key, data, &mut vec![], evaluator)?;
            }
        }
        Ok(())
    }

    pub fn process_remaining_data(&mut self, data: &mut Value, evaluator: &dyn ComputedEvaluator) -> Result<()> {
        self.setup_data(data, &[], None, evaluator)
    }

    /// `getDependencies(key)`: keys a computed entry was observed reading
    pub fn get_dependencies(&self, key: &str) -> Vec<String> {
        self.signals
            .get(key)
            .map(|s| s.deps.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// `getDeferred(path)`: a deferred entry overlapping `path`
    pub fn get_deferred(&self, path: &str) -> Option<(String, String)> {
        for (key, signal) in &self.signals {
            if signal.state == State::Deferred && overlaps(key, path) {
                return Some((key.clone(), signal.reason.clone().unwrap_or_default()));
            }
        }
        None
    }

    fn pull(
        &mut self,
        key: &str,
        data: &mut Value,
        stack: &mut Vec<String>,
        evaluator: &dyn ComputedEvaluator,
    ) -> Result<()> {
        if self.signals[key].state != State::Idle {
            return Ok(());
        }
        if stack.iter().any(|s| s == key) {
            let mut path = stack.clone();
            path.push(key.to_string());
            bail!("Circular computed data dependency: {}", path.join(" -> "));
        }
        if self.is_unavailable(key) {
            self.signals.get_mut(key).unwrap().defer(key);
            return Ok(());
        }
        stack.push(key.to_string());
        let entry = self.entries[key].clone();
        if let ComputedValue::Function(f) = &entry.value {
            let result = self.run_function(key, f, &entry.declared_dependencies, data, stack, evaluator)?;
            stack.pop();
            if let Some(result) = result {
                if self.signals[key].state != State::Deferred {
                    data.set_path(key, result);
                    self.signals.get_mut(key).unwrap().state = State::Done;
                }
            }
            return Ok(());
        }
        let mut vars: Vec<String> = entry.declared_dependencies.clone();
        if matches!(entry.value, ComputedValue::Template(_)) {
            vars.extend(evaluator.symbols(&entry.value, data));
        }
        let ok = self.pull_vars(key, &vars, data, stack, evaluator)?;
        let result = if ok {
            match &entry.value {
                ComputedValue::Static(v) => Some(v.clone()),
                other => Some(evaluator.evaluate(key, other, data)?),
            }
        } else {
            None
        };
        stack.pop();
        if self.signals[key].state == State::Deferred {
            return Ok(());
        }
        if let Some(result) = result {
            data.set_path(key, result);
            self.signals.get_mut(key).unwrap().state = State::Done;
        }
        Ok(())
    }

    /// `#pullDependency`: resolves `dep` for `key`, returns false (and defers `key`) if it can’t resolve yet
    fn pull_dependency(
        &mut self,
        key: &str,
        dep: &str,
        data: &mut Value,
        stack: &mut Vec<String>,
        evaluator: &dyn ComputedEvaluator,
    ) -> Result<bool> {
        self.signals.get_mut(key).unwrap().deps.insert(dep.to_string());
        self.pull(dep, data, stack, evaluator)?;
        if self.signals[dep].state == State::Deferred {
            let reason = self.signals[dep].reason.clone().unwrap_or_default();
            self.signals.get_mut(key).unwrap().defer(&reason);
            return Ok(false);
        }
        Ok(true)
    }

    /// `#runFunction`: functions read through a tracking proxy and re-run after each
    /// unresolved computed key they hit. Returns `None` when the key was deferred.
    fn run_function(
        &mut self,
        key: &str,
        f: &Arc<NativeFn>,
        declared: &[String],
        data: &mut Value,
        stack: &mut Vec<String>,
        evaluator: &dyn ComputedEvaluator,
    ) -> Result<Option<Value>> {
        if !self.pull_vars(key, declared, data, stack, evaluator)? {
            return Ok(None);
        }
        loop {
            let context = Arc::new(TrackingContext {
                key: key.to_string(),
                pending_keys: self
                    .signals
                    .iter()
                    .filter(|(_, s)| s.state != State::Done)
                    .map(|(k, _)| k.clone())
                    .collect(),
                computed_keys: self.entries.keys().cloned().collect(),
                unavailable: self.unavailable.clone(),
                reads: Mutex::new(TrackedReads::default()),
            });
            let proxy = TrackingProxy::wrap(data.clone(), String::new(), context.clone());
            let result = f(&[proxy]);
            let reads = std::mem::take(&mut *context.reads.lock().unwrap());
            let signal = self.signals.get_mut(key).unwrap();
            signal.deps.extend(reads.deps);
            // Checked before the result so a swallowed interrupted read still defers or re-runs
            if let Some(path) = reads.unavailable {
                signal.defer(&path);
                return Ok(None);
            }
            if let Some(dep) = reads.pending {
                if !self.pull_dependency(key, &dep, data, stack, evaluator)? {
                    return Ok(None);
                }
                continue;
            }
            // Returned objects (e.g. `data => data.page`) must not leak the tracking proxy
            return Ok(Some(TrackingProxy::unwrap(result?)));
        }
    }

    fn pull_vars(
        &mut self,
        key: &str,
        vars: &[String],
        data: &mut Value,
        stack: &mut Vec<String>,
        evaluator: &dyn ComputedEvaluator,
    ) -> Result<bool> {
        for var in vars {
            if self.is_unavailable(var) || self.has_unavailable_child(var) {
                self.signals.get_mut(key).unwrap().defer(var);
                return Ok(false);
            }
            let deps: Vec<String> = self
                .entries
                .keys()
                .filter(|dep| *dep != key && overlaps(dep, var))
                .cloned()
                .collect();
            for dep in deps {
                if !self.pull_dependency(key, &dep, data, stack, evaluator)? {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}

#[derive(Default)]
struct TrackedReads {
    deps: IndexSet<String>,
    /// The first unresolved computed key read
    pending: Option<String>,
    /// The first unavailable path read
    unavailable: Option<String>,
}

/// Per-run state shared by a tracking proxy and its nested proxies
struct TrackingContext {
    key: String,
    /// Computed keys not yet resolved
    pending_keys: Vec<String>,
    computed_keys: Vec<String>,
    unavailable: Vec<String>,
    reads: Mutex<TrackedReads>,
}

impl TrackingContext {
    fn is_unavailable(&self, path: &str) -> bool {
        self.unavailable
            .iter()
            .any(|e| path == e || path.starts_with(&format!("{e}.")))
    }

    /// `Signals#hasPendingChild`
    fn has_pending_child(&self, path: &str) -> bool {
        let prefix = format!("{path}.");
        self.pending_keys
            .iter()
            .any(|k| *k != self.key && k.starts_with(&prefix))
            || self.unavailable.iter().any(|e| e.starts_with(&prefix))
    }

    /// `Signals#getChildNames`
    fn child_names(&self, path: &str) -> Vec<String> {
        let mut names: IndexSet<String> = IndexSet::new();
        for key in self.computed_keys.iter().chain(self.unavailable.iter()) {
            if *key == self.key {
                continue;
            }
            let rest = if path.is_empty() {
                key.as_str()
            } else {
                match key.strip_prefix(&format!("{path}.")) {
                    Some(rest) => rest,
                    None => continue,
                }
            };
            if let Some(name) = rest.split('.').next().filter(|n| !n.is_empty()) {
                names.insert(name.to_string());
            }
        }
        names.into_iter().collect()
    }
}

/// `#getTrackingProxy`: records the data paths a computed function reads
struct TrackingProxy {
    target: Value,
    path: String,
    context: Arc<TrackingContext>,
}

impl TrackingProxy {
    fn wrap(target: Value, path: String, context: Arc<TrackingContext>) -> Value {
        Value::Dynamic(Arc::new(TrackingProxy { target, path, context }))
    }

    fn unwrap(value: Value) -> Value {
        if let Value::Dynamic(d) = &value {
            if let Some(proxy) = d.as_any().and_then(|a| a.downcast_ref::<TrackingProxy>()) {
                return proxy.target.clone();
            }
        }
        value
    }
}

impl DynObject for TrackingProxy {
    fn get(&self, prop: &str) -> Option<Value> {
        let ctx = &self.context;
        let path = if self.path.is_empty() {
            prop.to_string()
        } else {
            format!("{}.{prop}", self.path)
        };
        if ctx.is_unavailable(&path) {
            ctx.reads.lock().unwrap().unavailable.get_or_insert(path);
            return Some(Value::Undefined);
        }
        if path != ctx.key && ctx.computed_keys.contains(&path) {
            let mut reads = ctx.reads.lock().unwrap();
            reads.deps.insert(path.clone());
            if ctx.pending_keys.contains(&path) {
                reads.pending.get_or_insert(path);
                return Some(Value::Undefined);
            }
        }
        let value = self.target.get(prop);
        // `value && typeof value === "object"`
        let is_object = matches!(value, Value::Object(_) | Value::Array(_) | Value::Dynamic(_));
        if ctx.has_pending_child(&path) {
            // e.g. reading `page` before `page.url` is set
            let target = if is_object { value } else { Value::object() };
            return Some(Self::wrap(target, path, ctx.clone()));
        }
        // Parents of a dotted key stay proxied so it reads its own previous value
        if is_object && ctx.key.starts_with(&format!("{path}.")) {
            return Some(Self::wrap(value, path, ctx.clone()));
        }
        Some(value)
    }

    fn keys(&self) -> Vec<String> {
        let mut keys: IndexSet<String> = self.target.keys().into_iter().collect();
        keys.extend(self.context.child_names(&self.path));
        keys.into_iter().collect()
    }

    fn to_plain(&self) -> Value {
        if self.target.is_array() {
            return self.target.clone();
        }
        let mut map = Map::new();
        for key in self.keys() {
            map.insert(key.clone(), self.get(&key).unwrap_or_default());
        }
        Value::from_map(map)
    }

    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Eval;
    impl ComputedEvaluator for Eval {
        fn evaluate(&self, _key: &str, value: &ComputedValue, data: &Value) -> Result<Value> {
            match value {
                ComputedValue::Template(Value::String(s)) => {
                    // "{{ a }}-x" style: replace {{ name }} with data
                    let re = regex::Regex::new(r"\{\{\s*([\w.]+)\s*\}\}").unwrap();
                    Ok(Value::str(
                        re.replace_all(s, |c: &regex::Captures| data.get_path(&c[1]).to_output_string())
                            .to_string(),
                    ))
                }
                ComputedValue::Function(f) => f(&[data.clone()]),
                _ => Ok(Value::Undefined),
            }
        }
        fn symbols(&self, value: &ComputedValue, _data: &Value) -> Vec<String> {
            match value {
                ComputedValue::Template(Value::String(s)) => crate::engines::find_symbols(s),
                _ => vec![],
            }
        }
    }

    #[test]
    fn resolves_in_order() {
        let mut b = ComputedDataBuilder::new();
        b.add_computed_entry(
            &Value::from_json(serde_json::json!({"b": "{{ a }}-b", "a": "{{ base }}-a"})),
            "",
            &[],
        );
        let mut cd = b.build();
        let mut data = Value::from_json(serde_json::json!({"base": "x"}));
        cd.setup_data(&mut data, &[], None, &Eval).unwrap();
        assert_eq!(data.get("a"), Value::str("x-a"));
        assert_eq!(data.get("b"), Value::str("x-a-b"));
    }

    #[test]
    fn defers_unavailable() {
        let mut b = ComputedDataBuilder::new();
        b.add_computed_entry(
            &Value::from_json(serde_json::json!({"n": "{{ collections.all }}"})),
            "",
            &[],
        );
        let mut cd = b.build();
        let mut data = Value::object();
        cd.setup_data(&mut data, &["collections".into()], None, &Eval).unwrap();
        assert!(data.get("n").is_undefined());
        assert!(cd.get_deferred("n").is_some());
        cd.process_remaining_data(&mut data, &Eval).unwrap();
        assert!(!data.get("n").is_undefined());
    }
}
