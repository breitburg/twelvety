//! Shared helpers for the ported integration tests.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use twelvety::template::PageResult;
use twelvety::user_config::UserConfig;
use twelvety::{Twelvety, Options};

/// Programmatic options without a configuration file (`configPath: false`, quiet)
pub fn options<F>(config: F) -> Options
where
    F: Fn(&mut UserConfig) -> anyhow::Result<()> + Send + Sync + 'static,
{
    Options {
        config_path: Some(String::new()),
        quiet_mode: Some(true),
        ..Default::default()
    }
    .with_config(config)
}

pub fn twelvety(input: &str, output: Option<&str>, options: Options) -> Twelvety {
    let mut elev = Twelvety::new(Some(input), output, options);
    elev.set_is_verbose(false);
    elev.disable_logger();
    elev
}

/// `new Twelvety(input, output, options).toJSON()`
pub fn to_json(input: &str, output: Option<&str>, options: Options) -> anyhow::Result<Vec<PageResult>> {
    twelvety(input, output, options).to_json()
}

/// `getContentFor(results, filename)`: trimmed content of the page whose output path ends with `filename`
pub fn content_for(results: &[PageResult], filename: &str) -> String {
    results
        .iter()
        .find(|r| r.output_path.to_output_string().ends_with(filename))
        .unwrap_or_else(|| panic!("no result for {filename}"))
        .content
        .trim()
        .replace("\r\n", "\n")
}

/// A unique temporary directory (removed on drop)
pub struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new(name: &str) -> Self {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "twelvety-{name}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn join(&self, p: &str) -> String {
        self.0.join(p).to_string_lossy().to_string()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
