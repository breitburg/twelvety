//! Port of `Util/ConsoleLogger.js`.

use std::io::Write;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use super::chalk;

#[derive(Debug)]
pub struct ConsoleLogger {
    is_verbose: AtomicBool,
    is_chalk_enabled: AtomicBool,
    /// When `false`, all output is suppressed (`disableLogger`)
    enabled: AtomicBool,
    /// Captured output (for tests and programmatic use)
    capture: Mutex<Option<Vec<String>>>,
}

impl Default for ConsoleLogger {
    fn default() -> Self {
        Self::new()
    }
}

impl ConsoleLogger {
    pub fn new() -> Self {
        Self {
            is_verbose: AtomicBool::new(true),
            is_chalk_enabled: AtomicBool::new(chalk::supports_color()),
            enabled: AtomicBool::new(true),
            capture: Mutex::new(None),
        }
    }

    pub fn is_verbose(&self) -> bool {
        self.is_verbose.load(Ordering::Relaxed)
    }

    pub fn set_verbose(&self, verbose: bool) {
        self.is_verbose.store(verbose, Ordering::Relaxed);
    }

    pub fn set_chalk_enabled(&self, enabled: bool) {
        self.is_chalk_enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn is_logging_enabled(&self) -> bool {
        self.is_verbose() && self.enabled.load(Ordering::Relaxed)
    }

    pub fn override_logger(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    /// Start capturing output instead of printing it
    pub fn capture(&self) {
        *self.capture.lock().unwrap() = Some(vec![]);
    }

    pub fn take_captured(&self) -> Vec<String> {
        self.capture
            .lock()
            .unwrap()
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default()
    }

    fn color(&self, message: &str, color: Option<&str>) -> String {
        if !self.is_chalk_enabled.load(Ordering::Relaxed) {
            return message.to_string();
        }
        match color {
            Some(c) => chalk::color(c, message),
            None => message.to_string(),
        }
    }

    fn output(&self, message: &str, to_stderr: bool) {
        if !self.enabled.load(Ordering::Relaxed) {
            return;
        }
        let mut capture = self.capture.lock().unwrap();
        if let Some(buf) = capture.as_mut() {
            buf.push(message.to_string());
            return;
        }
        let prefix = "[12ty] ";
        let line = message
            .lines()
            .map(|l| format!("{prefix}{l}"))
            .collect::<Vec<_>>()
            .join("\n");
        if to_stderr {
            let _ = writeln!(std::io::stderr(), "{line}");
        } else {
            let _ = writeln!(std::io::stdout(), "{line}");
        }
    }

    /// Verbose-only log
    pub fn log(&self, message: &str) {
        if self.is_verbose() {
            self.output(message, false);
        }
    }

    pub fn info(&self, message: &str) {
        if self.is_verbose() {
            let msg = self.color(message, Some("blue"));
            self.output(&msg, false);
        }
    }

    pub fn warn(&self, message: &str) {
        if self.is_verbose() {
            let msg = self.color(message, Some("yellow"));
            self.output(&msg, true);
        }
    }

    pub fn error(&self, message: &str) {
        let msg = self.color(message, Some("red"));
        self.output(&msg, true);
    }

    /// Always logs (ignores quiet mode)
    pub fn force_log(&self, message: &str) {
        self.output(message, false);
    }

    pub fn force_warn(&self, message: &str) {
        let msg = self.color(message, Some("yellow"));
        self.output(&msg, true);
    }

    pub fn log_with_options(&self, message: &str, color: Option<&str>, force: bool) {
        if force || self.is_verbose() {
            let msg = self.color(message, color);
            self.output(&msg, false);
        }
    }
}
