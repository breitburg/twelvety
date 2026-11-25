//! Watch mode (port of the watch parts of `Core.js`, `Watch.js` and `WatchQueue.js`).
//!
//! Uses `notify` to watch the project; changes are debounced and trigger a full
//! rebuild.

use std::path::Path;
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::{Duration, Instant};

use anyhow::Result;
use notify::{RecursiveMode, Watcher};

use crate::core::Twelvety;
use crate::serve::DevServer;
use crate::util::glob_matcher::{build_glob_set, is_glob_match};
use crate::util::template_path;

fn relative(path: &Path) -> String {
    let cwd = std::env::current_dir().unwrap_or_default();
    let rel = path.strip_prefix(&cwd).unwrap_or(path);
    template_path::add_leading_dot_slash(&rel.to_string_lossy().replace('\\', "/"))
}

/// Watch for changes and rebuild. Blocks forever.
pub fn watch(twelvety: &mut Twelvety, server: Option<&DevServer>) -> Result<()> {
    let (tx, rx) = channel();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(event) = res {
            if matches!(
                event.kind,
                notify::EventKind::Create(_) | notify::EventKind::Modify(_) | notify::EventKind::Remove(_)
            ) {
                for p in event.paths {
                    let _ = tx.send(p);
                }
            }
        }
    })?;
    watcher.watch(Path::new("."), RecursiveMode::Recursive)?;
    let logger = twelvety.logger();
    logger.force_log("Watching…");

    let throttle = Duration::from_millis(100);
    loop {
        let first = match rx.recv() {
            Ok(p) => p,
            Err(_) => return Ok(()),
        };
        let mut changed = vec![first];
        let deadline = Instant::now() + throttle;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(remaining) {
                Ok(p) => changed.push(p),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return Ok(()),
            }
        }
        let output_dir = twelvety.output_dir.clone();
        let output_glob = format!("{}**", template_path::add_leading_dot_slash(&output_dir));
        let ignores: Vec<String> = twelvety
            .watch_ignores
            .iter()
            .cloned()
            .chain([output_glob, "./.git/**".to_string(), "**/node_modules/**".to_string()])
            .collect();
        let ignore_set = build_glob_set(&ignores);
        let targets = twelvety.watch_targets.clone();
        let relevant: Vec<String> = changed
            .iter()
            .map(|p| relative(p))
            .filter(|p| !ignore_set.is_match(template_path::strip_leading_dot_slash(p)))
            .filter(|p| targets.is_empty() || is_glob_match(p, &targets) || targets.contains(p))
            .collect();
        if relevant.is_empty() {
            continue;
        }
        let mut unique = relevant.clone();
        unique.sort();
        unique.dedup();
        logger.force_log(&format!("File changed: {}", unique.join(", ")));
        match twelvety.write() {
            Ok(_) => {
                if let Some(s) = server {
                    s.reload();
                }
            }
            Err(e) => logger.error(&crate::errors::format_error_chain(&e)),
        }
        logger.force_log("Watching…");
    }
}
