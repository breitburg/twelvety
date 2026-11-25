//! Port of `Util/AsyncEventEmitter.js`: an event emitter with legacy event name aliases.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::value::Value;

pub type EventListener = Arc<dyn Fn(&Value) -> anyhow::Result<()> + Send + Sync>;

const ALIASES: &[(&str, &[&str])] = &[
    (
        "buildawesome.before",
        &["beforeBuild", "twelvety.before", "buildawesome.before"],
    ),
    (
        "buildawesome.beforewatch",
        &[
            "beforeWatch",
            "twelvety.beforeWatch",
            "buildawesome.beforewatch",
        ],
    ),
    (
        "buildawesome.beforeconfig",
        &["twelvety.beforeConfig", "buildawesome.beforeconfig"],
    ),
    (
        "buildawesome.after",
        &["afterBuild", "twelvety.after", "buildawesome.after"],
    ),
    (
        "buildawesome.aftererror",
        &["twelvety.afterError", "buildawesome.aftererror"],
    ),
    (
        "buildawesome.engine.njk",
        &["twelvety.engine.njk", "buildawesome.engine.njk"],
    ),
    (
        "buildawesome.globaldatafiles",
        &["twelvety.globalDataFiles", "buildawesome.globaldatafiles"],
    ),
    (
        "buildawesome.contentmap",
        &["twelvety.contentMap", "buildawesome.contentmap"],
    ),
    (
        "buildawesome.layouts",
        &["twelvety.layouts", "buildawesome.layouts"],
    ),
    (
        "buildawesome.datafiles",
        &["twelvety.dataFiles", "buildawesome.datafiles"],
    ),
    (
        "buildawesome.passthrough",
        &["twelvety.passthrough", "buildawesome.passthrough"],
    ),
    (
        "buildawesome.config",
        &["twelvety.config", "buildawesome.config"],
    ),
    ("buildawesome.env", &["twelvety.env", "buildawesome.env"]),
    (
        "buildawesome.extensionmap",
        &["twelvety.extensionmap", "buildawesome.extensionmap"],
    ),
    (
        "buildawesome.directories",
        &["twelvety.directories", "buildawesome.directories"],
    ),
    (
        "buildawesome.ignores",
        &["twelvety.ignores", "buildawesome.ignores"],
    ),
    (
        "buildawesome.reset",
        &["twelvety.reset", "buildawesome.reset"],
    ),
    (
        "buildawesome.afterwatch",
        &["twelvety.afterwatch", "buildawesome.afterwatch"],
    ),
    (
        "buildawesome.resourcemodified",
        &["twelvety.resourceModified", "buildawesome.resourcemodified"],
    ),
];

/// Normalize an event name to its canonical form
pub fn normalize_event_name(name: &str) -> String {
    for (canonical, aliases) in ALIASES {
        if aliases.contains(&name) || *canonical == name {
            return canonical.to_string();
        }
    }
    if let Some(rest) = name.strip_prefix("twelvety.") {
        return format!("buildawesome.{}", rest.to_lowercase());
    }
    name.to_string()
}

#[derive(Clone, Default)]
pub struct EventEmitter {
    listeners: Arc<Mutex<HashMap<String, Vec<(EventListener, bool)>>>>,
}

impl EventEmitter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn on(&self, name: &str, listener: EventListener) {
        self.listeners
            .lock()
            .unwrap()
            .entry(normalize_event_name(name))
            .or_default()
            .push((listener, false));
    }

    pub fn once(&self, name: &str, listener: EventListener) {
        self.listeners
            .lock()
            .unwrap()
            .entry(normalize_event_name(name))
            .or_default()
            .push((listener, true));
    }

    pub fn has_listeners(&self, name: &str) -> bool {
        self.listeners
            .lock()
            .unwrap()
            .get(&normalize_event_name(name))
            .map(|l| !l.is_empty())
            .unwrap_or(false)
    }

    pub fn emit(&self, name: &str, arg: &Value) -> anyhow::Result<()> {
        let name = normalize_event_name(name);
        let listeners: Vec<EventListener> = {
            let mut map = self.listeners.lock().unwrap();
            let Some(list) = map.get_mut(&name) else {
                return Ok(());
            };
            let current: Vec<EventListener> = list.iter().map(|(l, _)| l.clone()).collect();
            list.retain(|(_, once)| !once);
            current
        };
        for listener in listeners {
            listener(arg)?;
        }
        Ok(())
    }

    /// Only computes the argument if there are listeners
    pub fn emit_lazy(&self, name: &str, arg: impl FnOnce() -> Value) -> anyhow::Result<()> {
        if !self.has_listeners(name) {
            return Ok(());
        }
        self.emit(name, &arg())
    }

    pub fn reset(&self) {
        self.listeners.lock().unwrap().clear();
    }
}
