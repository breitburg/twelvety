//! A dynamic value type modelled after JavaScript values.
//!
//! The upstream project passes plain JavaScript objects through the data
//! cascade. In Rust we use a single dynamic `Value` enum. Objects and arrays are
//! reference counted (and copy-on-write) so cloning data is cheap, much like
//! passing object references around in JavaScript.

use std::cmp::Ordering;
use std::fmt;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use serde::ser::{SerializeMap, SerializeSeq};
use serde::{Serialize, Serializer};

pub type Map = IndexMap<String, Value>;

/// A host object with dynamic property access (e.g. a collection item whose
/// `templateContent` is only available after rendering).
pub trait DynObject: Send + Sync {
    fn get(&self, key: &str) -> Option<Value>;
    fn keys(&self) -> Vec<String>;
    fn type_name(&self) -> &'static str {
        "object"
    }
    /// Convert to a plain value (used for serialization, iteration)
    fn to_plain(&self) -> Value {
        let mut map = Map::new();
        for key in self.keys() {
            if let Some(v) = self.get(&key) {
                map.insert(key, v);
            }
        }
        Value::Object(Arc::new(map))
    }
    /// Identity used for equality checks
    fn identity(&self) -> usize {
        self as *const Self as *const () as usize
    }
    /// Downcasting support for host objects
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        None
    }
}

pub type NativeFn = dyn Fn(&[Value]) -> anyhow::Result<Value> + Send + Sync;

#[derive(Clone, Default)]
pub enum Value {
    #[default]
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(Arc<str>),
    Array(Arc<Vec<Value>>),
    Object(Arc<Map>),
    Date(DateTime<Utc>),
    Dynamic(Arc<dyn DynObject>),
    Function(Arc<NativeFn>),
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Undefined => write!(f, "undefined"),
            Value::Null => write!(f, "null"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Number(n) => write!(f, "{}", format_number(*n)),
            Value::String(s) => write!(f, "{s:?}"),
            Value::Array(a) => f.debug_list().entries(a.iter()).finish(),
            Value::Object(o) => f.debug_map().entries(o.iter()).finish(),
            Value::Date(d) => write!(f, "{}", to_iso_string(d)),
            Value::Dynamic(d) => write!(f, "[{}]", d.type_name()),
            Value::Function(_) => write!(f, "[Function]"),
        }
    }
}

/// JavaScript `Number.prototype.toString()` approximation
pub fn format_number(n: f64) -> String {
    if n.is_nan() {
        return "NaN".into();
    }
    if n.is_infinite() {
        return if n > 0.0 {
            "Infinity".into()
        } else {
            "-Infinity".into()
        };
    }
    if n == 0.0 {
        return "0".into();
    }
    if n.fract() == 0.0 && n.abs() < 1e21 {
        return format!("{n:.0}");
    }
    let s = format!("{n}");
    s
}

/// `Date.prototype.toISOString()`
pub fn to_iso_string(d: &DateTime<Utc>) -> String {
    d.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

/// `Date.prototype.toString()` (in UTC)
pub fn to_js_date_string(d: &DateTime<Utc>) -> String {
    d.format("%a %b %d %Y %H:%M:%S GMT+0000 (Coordinated Universal Time)")
        .to_string()
}

impl Value {
    pub fn str(s: impl AsRef<str>) -> Value {
        Value::String(Arc::from(s.as_ref()))
    }

    pub fn object() -> Value {
        Value::Object(Arc::new(Map::new()))
    }

    pub fn from_map(map: Map) -> Value {
        Value::Object(Arc::new(map))
    }

    pub fn array(items: Vec<Value>) -> Value {
        Value::Array(Arc::new(items))
    }

    pub fn function<F>(f: F) -> Value
    where
        F: Fn(&[Value]) -> anyhow::Result<Value> + Send + Sync + 'static,
    {
        Value::Function(Arc::new(f))
    }

    pub fn is_undefined(&self) -> bool {
        matches!(self, Value::Undefined)
    }

    pub fn is_nullish(&self) -> bool {
        matches!(self, Value::Undefined | Value::Null)
    }

    pub fn is_object(&self) -> bool {
        matches!(self, Value::Object(_))
    }

    /// lodash `isPlainObject`
    pub fn is_plain_object(&self) -> bool {
        matches!(self, Value::Object(_))
    }

    pub fn is_array(&self) -> bool {
        matches!(self, Value::Array(_))
    }

    pub fn is_string(&self) -> bool {
        matches!(self, Value::String(_))
    }

    pub fn is_function(&self) -> bool {
        matches!(self, Value::Function(_))
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&Vec<Value>> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&Map> {
        match self {
            Value::Object(o) => Some(o),
            _ => None,
        }
    }

    /// Mutable access to an object (copy-on-write). Converts non-objects to objects.
    pub fn as_object_mut(&mut self) -> &mut Map {
        if !matches!(self, Value::Object(_)) {
            *self = Value::object();
        }
        match self {
            Value::Object(o) => Arc::make_mut(o),
            _ => unreachable!(),
        }
    }

    pub fn as_array_mut(&mut self) -> Option<&mut Vec<Value>> {
        match self {
            Value::Array(a) => Some(Arc::make_mut(a)),
            _ => None,
        }
    }

    /// JavaScript truthiness
    pub fn is_truthy(&self) -> bool {
        match self {
            Value::Undefined | Value::Null => false,
            Value::Bool(b) => *b,
            Value::Number(n) => *n != 0.0 && !n.is_nan(),
            Value::String(s) => !s.is_empty(),
            _ => true,
        }
    }

    /// JavaScript `typeof`
    pub fn type_of(&self) -> &'static str {
        match self {
            Value::Undefined => "undefined",
            Value::Null => "object",
            Value::Bool(_) => "boolean",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Function(_) => "function",
            _ => "object",
        }
    }

    /// Property access (`value[key]`)
    pub fn get(&self, key: &str) -> Value {
        match self {
            Value::Object(o) => o.get(key).cloned().unwrap_or_default(),
            Value::Array(a) => {
                if key == "length" {
                    return Value::Number(a.len() as f64);
                }
                match key.parse::<usize>() {
                    Ok(i) => a.get(i).cloned().unwrap_or_default(),
                    Err(_) => Value::Undefined,
                }
            }
            Value::String(s) => {
                if key == "length" {
                    return Value::Number(s.chars().count() as f64);
                }
                match key.parse::<usize>() {
                    Ok(i) => s
                        .chars()
                        .nth(i)
                        .map(|c| Value::str(c.to_string()))
                        .unwrap_or_default(),
                    Err(_) => Value::Undefined,
                }
            }
            Value::Dynamic(d) => d.get(key).unwrap_or_default(),
            _ => Value::Undefined,
        }
    }

    pub fn get_index(&self, index: usize) -> Value {
        match self {
            Value::Array(a) => a.get(index).cloned().unwrap_or_default(),
            _ => self.get(&index.to_string()),
        }
    }

    /// `key in value` / hasOwnProperty
    pub fn has(&self, key: &str) -> bool {
        match self {
            Value::Object(o) => o.contains_key(key),
            Value::Dynamic(d) => d.keys().iter().any(|k| k == key),
            Value::Array(a) => {
                key == "length" || key.parse::<usize>().map(|i| i < a.len()).unwrap_or(false)
            }
            _ => false,
        }
    }

    pub fn keys(&self) -> Vec<String> {
        match self {
            Value::Object(o) => o.keys().cloned().collect(),
            Value::Array(a) => (0..a.len()).map(|i| i.to_string()).collect(),
            Value::Dynamic(d) => d.keys(),
            _ => vec![],
        }
    }

    /// Set a key on an object (no-op on non-objects, mirroring JS for primitives)
    pub fn set(&mut self, key: impl Into<String>, value: Value) {
        match self {
            Value::Object(o) => {
                Arc::make_mut(o).insert(key.into(), value);
            }
            Value::Array(a) => {
                let key = key.into();
                if let Ok(i) = key.parse::<usize>() {
                    let a = Arc::make_mut(a);
                    if i >= a.len() {
                        a.resize(i + 1, Value::Undefined);
                    }
                    a[i] = value;
                }
            }
            _ => {}
        }
    }

    pub fn remove(&mut self, key: &str) -> Option<Value> {
        match self {
            Value::Object(o) => Arc::make_mut(o).shift_remove(key),
            _ => None,
        }
    }

    /// lodash `_.get(obj, path)`
    pub fn get_path(&self, path: &str) -> Value {
        let parts = parse_path(path);
        let mut current = self.clone();
        for part in parts {
            if current.is_nullish() {
                return Value::Undefined;
            }
            current = current.get(&part);
        }
        current
    }

    /// Returns `None` when the path does not resolve (lodash default value semantics)
    pub fn has_path(&self, path: &str) -> bool {
        let parts = parse_path(path);
        let mut current = self.clone();
        for part in parts {
            if !current.has(&part) {
                return false;
            }
            current = current.get(&part);
        }
        true
    }

    /// lodash `_.set(obj, path, value)`
    pub fn set_path(&mut self, path: &str, value: Value) {
        let parts = parse_path(path);
        set_path_parts(self, &parts, value);
    }

    /// Length for arrays, strings, objects
    pub fn len(&self) -> usize {
        match self {
            Value::Array(a) => a.len(),
            Value::String(s) => s.chars().count(),
            Value::Object(o) => o.len(),
            Value::Dynamic(d) => d.keys().len(),
            _ => 0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Array-like iteration (arrays, or dynamic iterables converted to plain)
    pub fn to_vec(&self) -> Vec<Value> {
        match self {
            Value::Array(a) => a.as_ref().clone(),
            Value::Dynamic(d) => match d.to_plain() {
                Value::Array(a) => a.as_ref().clone(),
                _ => vec![],
            },
            _ => vec![],
        }
    }

    /// Resolve dynamic objects to plain values (shallow)
    pub fn resolve(&self) -> Value {
        match self {
            Value::Dynamic(d) => d.to_plain(),
            other => other.clone(),
        }
    }

    /// JavaScript `String(value)`
    pub fn to_js_string(&self) -> String {
        match self {
            Value::Undefined => "undefined".into(),
            Value::Null => "null".into(),
            Value::Bool(b) => b.to_string(),
            Value::Number(n) => format_number(*n),
            Value::String(s) => s.to_string(),
            Value::Array(a) => a
                .iter()
                .map(|v| {
                    if v.is_nullish() {
                        String::new()
                    } else {
                        v.to_js_string()
                    }
                })
                .collect::<Vec<_>>()
                .join(","),
            Value::Object(_) | Value::Dynamic(_) => "[object Object]".into(),
            Value::Date(d) => to_js_date_string(d),
            Value::Function(_) => "function () { [native code] }".into(),
        }
    }

    /// Output form for templates: undefined/null render as empty string
    pub fn to_output_string(&self) -> String {
        match self {
            Value::Undefined | Value::Null => String::new(),
            other => other.to_js_string(),
        }
    }

    /// JavaScript `Number(value)`
    pub fn to_number(&self) -> f64 {
        match self {
            Value::Undefined => f64::NAN,
            Value::Null => 0.0,
            Value::Bool(b) => {
                if *b {
                    1.0
                } else {
                    0.0
                }
            }
            Value::Number(n) => *n,
            Value::String(s) => {
                let t = s.trim();
                if t.is_empty() {
                    0.0
                } else if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
                    i64::from_str_radix(hex, 16)
                        .map(|n| n as f64)
                        .unwrap_or(f64::NAN)
                } else if t == "Infinity" || t == "+Infinity" {
                    f64::INFINITY
                } else if t == "-Infinity" {
                    f64::NEG_INFINITY
                } else {
                    t.parse::<f64>().unwrap_or(f64::NAN)
                }
            }
            Value::Date(d) => d.timestamp_millis() as f64,
            Value::Array(a) => match a.len() {
                0 => 0.0,
                1 => a[0].to_number(),
                _ => f64::NAN,
            },
            _ => f64::NAN,
        }
    }

    /// Strict equality (`===`) with structural comparison for plain values
    pub fn strict_equals(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Undefined, Value::Undefined) => true,
            (Value::Null, Value::Null) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Number(a), Value::Number(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Date(a), Value::Date(b)) => a == b,
            (Value::Array(a), Value::Array(b)) => Arc::ptr_eq(a, b) || a == b,
            (Value::Object(a), Value::Object(b)) => Arc::ptr_eq(a, b) || a == b,
            (Value::Dynamic(a), Value::Dynamic(b)) => a.identity() == b.identity(),
            (Value::Function(a), Value::Function(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }

    /// Loose equality (`==`) approximation
    pub fn loose_equals(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Undefined | Value::Null, Value::Undefined | Value::Null) => true,
            (Value::Undefined | Value::Null, _) | (_, Value::Undefined | Value::Null) => false,
            (Value::Number(_), Value::String(_))
            | (Value::String(_), Value::Number(_))
            | (Value::Bool(_), _)
            | (_, Value::Bool(_)) => self.to_number() == other.to_number(),
            _ => self.strict_equals(other),
        }
    }

    /// JavaScript relational comparison (`<`)
    pub fn js_compare(&self, other: &Value) -> Option<Ordering> {
        match (self, other) {
            (Value::String(a), Value::String(b)) => Some(a.cmp(b)),
            (Value::Date(a), Value::Date(b)) => Some(a.cmp(b)),
            _ => {
                let a = self.to_number();
                let b = other.to_number();
                a.partial_cmp(&b)
            }
        }
    }

    /// Deep clone that also resolves dynamic objects (similar to `DeepCopy`)
    pub fn deep_copy(&self) -> Value {
        match self {
            Value::Array(a) => Value::array(a.iter().map(|v| v.deep_copy()).collect()),
            Value::Object(o) => {
                Value::from_map(o.iter().map(|(k, v)| (k.clone(), v.deep_copy())).collect())
            }
            other => other.clone(),
        }
    }

    /// Call a function value
    pub fn call(&self, args: &[Value]) -> anyhow::Result<Value> {
        match self {
            Value::Function(f) => f(args),
            _ => anyhow::bail!("Value is not a function: {}", self.to_js_string()),
        }
    }

    /// Convert to serde_json (for JSON output and serialization)
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
    }

    pub fn from_json(v: serde_json::Value) -> Value {
        match v {
            serde_json::Value::Null => Value::Null,
            serde_json::Value::Bool(b) => Value::Bool(b),
            serde_json::Value::Number(n) => Value::Number(n.as_f64().unwrap_or(0.0)),
            serde_json::Value::String(s) => Value::str(s),
            serde_json::Value::Array(a) => {
                Value::array(a.into_iter().map(Value::from_json).collect())
            }
            serde_json::Value::Object(o) => Value::from_map(
                o.into_iter()
                    .map(|(k, v)| (k, Value::from_json(v)))
                    .collect(),
            ),
        }
    }

    pub fn from_yaml(v: serde_yaml::Value) -> Value {
        match v {
            serde_yaml::Value::Null => Value::Null,
            serde_yaml::Value::Bool(b) => Value::Bool(b),
            serde_yaml::Value::Number(n) => Value::Number(n.as_f64().unwrap_or(0.0)),
            serde_yaml::Value::String(s) => {
                // js-yaml parses YAML timestamps into Date instances
                if let Some(date) = crate::util::date_parse::parse_yaml_timestamp(&s) {
                    Value::Date(date)
                } else {
                    Value::str(s)
                }
            }
            serde_yaml::Value::Sequence(a) => {
                Value::array(a.into_iter().map(Value::from_yaml).collect())
            }
            serde_yaml::Value::Mapping(m) => Value::from_map(
                m.into_iter()
                    .map(|(k, v)| {
                        let key = match k {
                            serde_yaml::Value::String(s) => s,
                            other => Value::from_yaml(other).to_js_string(),
                        };
                        (key, Value::from_yaml(v))
                    })
                    .collect(),
            ),
            serde_yaml::Value::Tagged(t) => Value::from_yaml(t.value),
        }
    }

    pub fn from_toml(v: toml::Value) -> Value {
        match v {
            toml::Value::String(s) => Value::str(s),
            toml::Value::Integer(i) => Value::Number(i as f64),
            toml::Value::Float(f) => Value::Number(f),
            toml::Value::Boolean(b) => Value::Bool(b),
            toml::Value::Datetime(d) => crate::util::date_parse::parse_iso(&d.to_string())
                .map(Value::Date)
                .unwrap_or_else(|| Value::str(d.to_string())),
            toml::Value::Array(a) => Value::array(a.into_iter().map(Value::from_toml).collect()),
            toml::Value::Table(t) => Value::from_map(
                t.into_iter()
                    .map(|(k, v)| (k, Value::from_toml(v)))
                    .collect(),
            ),
        }
    }

    /// Node’s `util.inspect(value)` (used in error messages)
    pub fn inspect(&self) -> String {
        fn key(k: &str) -> String {
            let ident = !k.is_empty()
                && !k.chars().next().unwrap().is_ascii_digit()
                && k.chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '$');
            if ident {
                k.to_string()
            } else {
                format!("'{k}'")
            }
        }
        match self {
            Value::String(s) => format!("'{}'", s.replace('\'', "\\'")),
            Value::Array(a) if a.is_empty() => "[]".into(),
            Value::Array(a) => format!(
                "[ {} ]",
                a.iter().map(|v| v.inspect()).collect::<Vec<_>>().join(", ")
            ),
            Value::Object(o) if o.is_empty() => "{}".into(),
            Value::Object(o) => format!(
                "{{ {} }}",
                o.iter()
                    .map(|(k, v)| format!("{}: {}", key(k), v.inspect()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Value::Date(d) => to_iso_string(d),
            Value::Dynamic(d) => d.to_plain().inspect(),
            Value::Function(_) => "[Function (anonymous)]".into(),
            other => other.to_js_string(),
        }
    }

    /// `JSON.stringify(value)`
    pub fn to_json_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn to_json_string_pretty(&self, indent: usize) -> String {
        let mut buf = Vec::new();
        let indent_str = " ".repeat(indent);
        let formatter = serde_json::ser::PrettyFormatter::with_indent(indent_str.as_bytes());
        let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
        if self.serialize(&mut ser).is_err() {
            return String::new();
        }
        String::from_utf8(buf).unwrap_or_default()
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        self.strict_equals(other)
    }
}

impl Serialize for Value {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Value::Undefined | Value::Null | Value::Function(_) => serializer.serialize_unit(),
            Value::Bool(b) => serializer.serialize_bool(*b),
            Value::Number(n) => {
                if n.is_finite() && n.fract() == 0.0 && n.abs() < 9.007_199_254_740_992e15 {
                    serializer.serialize_i64(*n as i64)
                } else if n.is_finite() {
                    serializer.serialize_f64(*n)
                } else {
                    serializer.serialize_unit()
                }
            }
            Value::String(s) => serializer.serialize_str(s),
            Value::Date(d) => serializer.serialize_str(&to_iso_string(d)),
            Value::Array(a) => {
                let mut seq = serializer.serialize_seq(Some(a.len()))?;
                for v in a.iter() {
                    if v.is_undefined() || v.is_function() {
                        seq.serialize_element(&Value::Null)?;
                    } else {
                        seq.serialize_element(v)?;
                    }
                }
                seq.end()
            }
            Value::Object(o) => {
                let mut map = serializer.serialize_map(None)?;
                for (k, v) in o.iter() {
                    if v.is_undefined() || v.is_function() {
                        continue;
                    }
                    map.serialize_entry(k, v)?;
                }
                map.end()
            }
            Value::Dynamic(d) => d.to_plain().serialize(serializer),
        }
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::str(s)
    }
}
impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::String(Arc::from(s))
    }
}
impl From<&String> for Value {
    fn from(s: &String) -> Self {
        Value::str(s)
    }
}
impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}
impl From<f64> for Value {
    fn from(n: f64) -> Self {
        Value::Number(n)
    }
}
impl From<i64> for Value {
    fn from(n: i64) -> Self {
        Value::Number(n as f64)
    }
}
impl From<i32> for Value {
    fn from(n: i32) -> Self {
        Value::Number(n as f64)
    }
}
impl From<usize> for Value {
    fn from(n: usize) -> Self {
        Value::Number(n as f64)
    }
}
impl From<DateTime<Utc>> for Value {
    fn from(d: DateTime<Utc>) -> Self {
        Value::Date(d)
    }
}
impl From<Vec<Value>> for Value {
    fn from(v: Vec<Value>) -> Self {
        Value::array(v)
    }
}
impl From<Map> for Value {
    fn from(m: Map) -> Self {
        Value::from_map(m)
    }
}
impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(o: Option<T>) -> Self {
        match o {
            Some(v) => v.into(),
            None => Value::Null,
        }
    }
}
impl From<serde_json::Value> for Value {
    fn from(v: serde_json::Value) -> Self {
        Value::from_json(v)
    }
}

/// Parse a lodash-style property path: `a.b[0].c`, `a["b"]`
pub fn parse_path(path: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = path.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '.' => {
                if !current.is_empty() {
                    parts.push(std::mem::take(&mut current));
                }
            }
            '[' => {
                if !current.is_empty() {
                    parts.push(std::mem::take(&mut current));
                }
                i += 1;
                let mut quote = None;
                if i < chars.len() && (chars[i] == '"' || chars[i] == '\'') {
                    quote = Some(chars[i]);
                    i += 1;
                }
                let mut seg = String::new();
                while i < chars.len() {
                    if let Some(q) = quote {
                        if chars[i] == q && i + 1 < chars.len() && chars[i + 1] == ']' {
                            i += 1;
                            break;
                        }
                    } else if chars[i] == ']' {
                        break;
                    }
                    seg.push(chars[i]);
                    i += 1;
                }
                parts.push(seg);
            }
            _ => current.push(c),
        }
        i += 1;
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

fn set_path_parts(target: &mut Value, parts: &[String], value: Value) {
    if parts.is_empty() {
        *target = value;
        return;
    }
    let key = &parts[0];
    if parts.len() == 1 {
        if !(target.is_object() || target.is_array()) {
            *target = Value::object();
        }
        target.set(key.clone(), value);
        return;
    }
    if !(target.is_object() || target.is_array()) {
        *target = Value::object();
    }
    let mut child = target.get(key);
    if !(child.is_object() || child.is_array()) {
        // lodash creates arrays for numeric next keys
        child = if parts[1].parse::<usize>().is_ok() {
            Value::array(vec![])
        } else {
            Value::object()
        };
    }
    // Avoid holding two references to the same Arc during mutation
    target.set(key.clone(), Value::Undefined);
    set_path_parts(&mut child, &parts[1..], value);
    target.set(key.clone(), child);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_parsing() {
        assert_eq!(parse_path("a.b[0].c"), vec!["a", "b", "0", "c"]);
        assert_eq!(
            parse_path("collections['my-tag']"),
            vec!["collections", "my-tag"]
        );
    }

    #[test]
    fn set_and_get_path() {
        let mut v = Value::object();
        v.set_path("a.b.c", Value::from(1));
        assert_eq!(v.get_path("a.b.c"), Value::from(1));
        assert!(v.has_path("a.b"));
        assert!(!v.has_path("a.x"));
    }

    #[test]
    fn number_formatting() {
        assert_eq!(format_number(1.0), "1");
        assert_eq!(format_number(1.5), "1.5");
        assert_eq!(format_number(-0.0), "0");
    }
}
