//! Bridges the crate's [`Value`] to `minijinja::Value` without deep copies.

use std::fmt;
use std::sync::Arc;

use chrono::{DateTime, Datelike, Timelike, Utc};
use minijinja::value::{Enumerator, Object, ObjectRepr, ValueKind};
use minijinja::{Error, ErrorKind, State, Value as MjValue};

use crate::value::{DynObject, Map, NativeFn, Value, to_iso_string, to_js_date_string};

#[derive(Debug)]
pub struct MjObject(pub Arc<Map>);

#[derive(Debug)]
pub struct MjArray(pub Arc<Vec<Value>>);

pub struct MjDynamic(pub Arc<dyn DynObject>);

pub struct MjFunction(pub Arc<NativeFn>);

#[derive(Debug)]
pub struct MjDate(pub DateTime<Utc>);

impl fmt::Debug for MjDynamic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}]", self.0.type_name())
    }
}

impl fmt::Debug for MjFunction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[Function]")
    }
}

pub fn to_mj(v: &Value) -> MjValue {
    match v {
        Value::Undefined => MjValue::UNDEFINED,
        Value::Null => MjValue::from(()),
        Value::Bool(b) => MjValue::from(*b),
        Value::Number(n) => {
            if n.fract() == 0.0 && n.abs() < 9.007_199_254_740_992e15 {
                MjValue::from(*n as i64)
            } else {
                MjValue::from(*n)
            }
        }
        Value::String(s) => MjValue::from(s.as_ref()),
        Value::Array(a) => MjValue::from_object(MjArray(a.clone())),
        Value::Object(o) => MjValue::from_object(MjObject(o.clone())),
        Value::Date(d) => MjValue::from_object(MjDate(*d)),
        Value::Dynamic(d) => MjValue::from_object(MjDynamic(d.clone())),
        Value::Function(f) => MjValue::from_object(MjFunction(f.clone())),
    }
}

pub fn from_mj(v: &MjValue) -> Value {
    if let Some(o) = v.downcast_object_ref::<MjObject>() {
        return Value::Object(o.0.clone());
    }
    if let Some(a) = v.downcast_object_ref::<MjArray>() {
        return Value::Array(a.0.clone());
    }
    if let Some(d) = v.downcast_object_ref::<MjDynamic>() {
        return Value::Dynamic(d.0.clone());
    }
    if let Some(d) = v.downcast_object_ref::<MjDate>() {
        return Value::Date(d.0);
    }
    if let Some(f) = v.downcast_object_ref::<MjFunction>() {
        return Value::Function(f.0.clone());
    }
    match v.kind() {
        ValueKind::Undefined => Value::Undefined,
        ValueKind::None => Value::Null,
        ValueKind::Bool => Value::Bool(v.is_true()),
        ValueKind::Number => {
            if let Ok(i) = i64::try_from(v.clone()) {
                Value::Number(i as f64)
            } else {
                Value::Number(f64::try_from(v.clone()).unwrap_or(f64::NAN))
            }
        }
        ValueKind::String => Value::str(v.as_str().unwrap_or("")),
        ValueKind::Seq | ValueKind::Iterable => match v.try_iter() {
            Ok(iter) => Value::array(iter.map(|x| from_mj(&x)).collect()),
            Err(_) => Value::Undefined,
        },
        ValueKind::Map => {
            let mut map = Map::new();
            if let Ok(iter) = v.try_iter() {
                for key in iter {
                    let item = v.get_item(&key).unwrap_or_default();
                    let k = key.as_str().map(String::from).unwrap_or_else(|| key.to_string());
                    map.insert(k, from_mj(&item));
                }
            }
            if v.is_kwargs() {
                map.insert("__keywords".into(), Value::Bool(true));
            }
            Value::from_map(map)
        }
        _ => Value::str(v.to_string()),
    }
}

fn key_str(key: &MjValue) -> Option<String> {
    if let Some(s) = key.as_str() {
        return Some(s.to_string());
    }
    if let Ok(i) = i64::try_from(key.clone()) {
        return Some(i.to_string());
    }
    None
}

impl Object for MjObject {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Map
    }

    fn get_value(self: &Arc<Self>, key: &MjValue) -> Option<MjValue> {
        let k = key_str(key)?;
        self.0.get(&k).map(to_mj)
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        let keys: Vec<MjValue> = self.0.keys().map(|k| MjValue::from(k.as_str())).collect();
        Enumerator::Values(keys)
    }

    fn enumerator_len(self: &Arc<Self>) -> Option<usize> {
        Some(self.0.len())
    }

    fn call_method(
        self: &Arc<Self>,
        state: &State<'_, '_>,
        method: &str,
        args: &[MjValue],
    ) -> Result<MjValue, Error> {
        if let Some(Value::Function(f)) = self.0.get(method) {
            let args: Vec<Value> = args.iter().map(from_mj).collect();
            return f(&args)
                .map(|v| to_mj(&v))
                .map_err(|e| Error::new(ErrorKind::InvalidOperation, e.to_string()));
        }
        let _ = state;
        Err(Error::from(ErrorKind::UnknownMethod))
    }
}

impl Object for MjArray {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Seq
    }

    fn get_value(self: &Arc<Self>, key: &MjValue) -> Option<MjValue> {
        if let Some(s) = key.as_str() {
            if s == "length" {
                return Some(MjValue::from(self.0.len()));
            }
            return None;
        }
        let idx = i64::try_from(key.clone()).ok()?;
        let idx = if idx < 0 { self.0.len() as i64 + idx } else { idx };
        self.0.get(idx as usize).map(to_mj)
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        Enumerator::Seq(self.0.len())
    }

    fn render(self: &Arc<Self>, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // JavaScript Array.prototype.toString
        write!(f, "{}", Value::Array(self.0.clone()).to_js_string())
    }
}

impl Object for MjDynamic {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Map
    }

    fn get_value(self: &Arc<Self>, key: &MjValue) -> Option<MjValue> {
        let k = key_str(key)?;
        self.0.get(&k).map(|v| to_mj(&v))
    }

    fn enumerate(self: &Arc<Self>) -> Enumerator {
        Enumerator::Values(self.0.keys().into_iter().map(MjValue::from).collect())
    }
}

impl Object for MjFunction {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Plain
    }

    fn call(self: &Arc<Self>, _state: &State<'_, '_>, args: &[MjValue]) -> Result<MjValue, Error> {
        let args: Vec<Value> = args.iter().map(from_mj).collect();
        (self.0)(&args)
            .map(|v| to_mj(&v))
            .map_err(|e| Error::new(ErrorKind::InvalidOperation, e.to_string()))
    }
}

impl Object for MjDate {
    fn repr(self: &Arc<Self>) -> ObjectRepr {
        ObjectRepr::Plain
    }

    fn render(self: &Arc<Self>, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", to_js_date_string(&self.0))
    }

    /// A few `Date.prototype` methods (UTC)
    fn call_method(
        self: &Arc<Self>,
        _state: &State<'_, '_>,
        method: &str,
        _args: &[MjValue],
    ) -> Result<MjValue, Error> {
        let d = &self.0;
        Ok(match method {
            "toISOString" | "toJSON" => MjValue::from(to_iso_string(d)),
            "toString" => MjValue::from(to_js_date_string(d)),
            "toUTCString" | "toGMTString" => {
                MjValue::from(d.format("%a, %d %b %Y %H:%M:%S GMT").to_string())
            }
            "toDateString" => MjValue::from(d.format("%a %b %d %Y").to_string()),
            "getTime" | "valueOf" => MjValue::from(d.timestamp_millis()),
            "getFullYear" | "getUTCFullYear" => MjValue::from(d.year()),
            "getMonth" | "getUTCMonth" => MjValue::from(d.month0()),
            "getDate" | "getUTCDate" => MjValue::from(d.day()),
            "getDay" | "getUTCDay" => MjValue::from(d.weekday().num_days_from_sunday()),
            "getHours" | "getUTCHours" => MjValue::from(d.hour()),
            "getMinutes" | "getUTCMinutes" => MjValue::from(d.minute()),
            "getSeconds" | "getUTCSeconds" => MjValue::from(d.second()),
            "toLocaleDateString" => MjValue::from(d.format("%-m/%-d/%Y").to_string()),
            _ => return Err(Error::from(ErrorKind::UnknownMethod)),
        })
    }
}
