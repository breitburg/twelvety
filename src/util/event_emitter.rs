//! Port of `Util/AsyncEventEmitter.js`: an event emitter with legacy event name aliases.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::value::Value;

pub type EventListener = Arc<dyn Fn(&Value) -> anyhow::Result<()> + Send + Sync>;

const ALIASES: &[(&str, &[&str])] = &[
    ("buildawesome.before", &["beforeBuild", "eleventy.before", "buildawesome.before"]),
    (
        "buildawesome.beforewatch",
        &["beforeWatch", "eleventy.beforeWatch", "buildawesome.beforewatch"],
    ),
    ("buildawesome.beforeconfig", &["eleventy.beforeConfig", "buildawesome.beforeconfig"]),
    ("buildawesome.after", &["afterBuild", "eleventy.after", "buildawesome.after"]),
    ("buildawesome.aftererror", &["eleventy.afterError", "buildawesome.aftererror"]),
    ("buildawesome.engine.njk", &["eleventy.engine.njk", "buildawesome.engine.njk"]),
    (
        "buildawesome.globaldatafiles",
        &["eleventy.globalDataFiles", "buildawesome.globaldatafiles"],
    ),
    ("buildawesome.contentmap", &["eleventy.contentMap", "buildawesome.contentmap"]),
    ("buildawesome.layouts", &["eleventy.layouts", "buildawesome.layouts"]),
    ("buildawesome.datafiles", &["eleventy.dataFiles", "buildawesome.datafiles"]),
    ("buildawesome.passthrough", &["eleventy.passthrough", "buildawesome.passthrough"]),
    ("buildawesome.config", &["eleventy.config", "buildawesome.config"]),
    ("buildawesome.env", &["eleventy.env", "buildawesome.env"]),
    ("buildawesome.extensionmap", &["eleventy.extensionmap", "buildawesome.extensionmap"]),
    ("buildawesome.directories", &["eleventy.directories", "buildawesome.directories"]),
    ("buildawesome.ignores", &["eleventy.ignores", "buildawesome.ignores"]),
    ("buildawesome.reset", &["eleventy.reset", "buildawesome.reset"]),
    ("buildawesome.afterwatch", &["eleventy.afterwatch", "buildawesome.afterwatch"]),
    (
        "buildawesome.resourcemodified",
        &["eleventy.resourceModified", "buildawesome.resourcemodified"],
    ),
];

/// Normalize an event name to its canonical form
pub fn normalize_event_name(name: &str) -> String {
    for (canonical, aliases) in ALIASES {
        if aliases.iter().any(|a| *a == name) || *canonical == name {
            return canonical.to_string();
        }
    }
    if let Some(rest) = name.strip_prefix("eleventy.") {
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
