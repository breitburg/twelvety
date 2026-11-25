//! Port of `CoreMinimal.js` and `Core.js`: the `Twelvety` class.

use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};

use crate::build_context::BuildContext;
use crate::data::template_data::UPSTREAM_VERSION;
use crate::files::Files;
use crate::template::PageResult;
use crate::template_config::{ConfigCallback, TemplateConfig};
use crate::template_passthrough::{CopyResult, TemplatePassthroughManager};
use crate::template_writer::{TemplateWriter, WriterMetadata};
use crate::user_config::UserConfig;
use crate::util::console_logger::ConsoleLogger;
use crate::util::project_directories::ProjectDirectories;
use crate::util::project_template_formats::ProjectTemplateFormats;
use crate::util::{chalk, template_path};
use crate::value::{Map, Value};

#[derive(Clone, Default)]
pub struct Options {
    /// `cli` or `script`
    pub source: Option<String>,
    /// `build`, `serve` or `watch`
    pub run_mode: Option<String>,
    pub dry_run: bool,
    /// Override the configuration file path (`Some("")` disables config files)
    pub config_path: Option<String>,
    pub path_prefix: Option<String>,
    pub quiet_mode: Option<bool>,
    /// Programmatic configuration (runs before the configuration file)
    pub config: Option<ConfigCallback>,
    pub input_dir: Option<String>,
    /// Comma separated template formats (`--formats`)
    pub formats: Option<String>,
}

impl Options {
    pub fn with_config<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut UserConfig) -> Result<()> + Send + Sync + 'static,
    {
        self.config = Some(Arc::new(f));
        self
    }
}

pub struct Twelvety {
    pub raw_input: Option<String>,
    pub raw_output: Option<String>,
    pub options: Options,
    pub source: String,
    pub run_mode: String,
    pub is_dry_run: bool,
    pub is_run_initial_build: bool,
    pub build_count: usize,
    start: Instant,
    logger: Arc<ConsoleLogger>,
    verbose_override: Option<bool>,
    last_metadata: Option<WriterMetadata>,
    active_config_path: Option<String>,
    pub directories: Option<ProjectDirectories>,
    /// Paths of files used in the last build (for watch mode)
    pub watch_targets: Vec<String>,
    pub watch_ignores: Vec<String>,
    pub output_dir: String,
    pub server_options: Value,
    /// Unique id of the most recent build (shared with `--events-file`)
    pub build_id: Option<String>,
}

pub enum BuildOutput {
    Fs,
    FsTemplates,
    Json,
}

impl Twelvety {
    pub fn new(input: Option<&str>, output: Option<&str>, options: Options) -> Self {
        let source = options.source.clone().unwrap_or_else(|| "script".into());
        let run_mode = options.run_mode.clone().unwrap_or_else(|| "build".into());
        let is_dry_run = options.dry_run;
        let verbose_override = if std::env::var_os("DEBUG").is_some() {
            Some(false)
        } else {
            options.quiet_mode.map(|q| !q)
        };
        Twelvety {
            raw_input: input.filter(|s| !s.is_empty()).map(String::from),
            raw_output: output.filter(|s| !s.is_empty()).map(String::from),
            options,
            source,
            run_mode,
            is_dry_run,
            is_run_initial_build: true,
            build_count: 0,
            start: Instant::now(),
            logger: Arc::new(ConsoleLogger::new()),
            verbose_override,
            last_metadata: None,
            active_config_path: None,
            directories: None,
            watch_targets: vec![],
            watch_ignores: vec![],
            output_dir: String::new(),
            server_options: Value::object(),
            build_id: None,
        }
    }

    pub fn get_version() -> &'static str {
        UPSTREAM_VERSION
    }

    pub fn logger(&self) -> Arc<ConsoleLogger> {
        self.logger.clone()
    }

    pub fn set_logger(&mut self, logger: Arc<ConsoleLogger>) {
        self.logger = logger;
    }

    pub fn set_dry_run(&mut self, dry_run: bool) {
        self.is_dry_run = dry_run;
    }

    pub fn set_ignore_initial(&mut self, ignore: bool) {
        self.is_run_initial_build = !ignore;
    }

    pub fn set_formats(&mut self, formats: &str) {
        self.options.formats = Some(formats.to_string());
    }

    pub fn set_run_mode(&mut self, run_mode: &str) {
        self.run_mode = run_mode.to_string();
    }

    pub fn set_is_verbose(&mut self, verbose: bool) {
        if self.verbose_override.is_none() {
            self.logger.set_verbose(verbose);
        }
    }

    pub fn disable_logger(&self) {
        self.logger.override_logger(false);
    }

    /// Project directories from the constructor arguments (before configuration is applied)
    pub fn create_directories(&self) -> Result<ProjectDirectories> {
        let mut dirs = ProjectDirectories::new();
        dirs.set_input(self.raw_input.as_deref(), self.options.input_dir.as_deref())?;
        dirs.set_output(self.raw_output.as_deref());
        if self.source == "cli" && (self.raw_input.is_some() || self.raw_output.is_some()) {
            dirs.freeze();
        }
        Ok(dirs)
    }

    fn environment_variables(&self) -> Value {
        let mut env = Map::new();
        env.insert("source".into(), Value::str(&self.source));
        env.insert("runMode".into(), Value::str(&self.run_mode));
        if let Some(c) = &self.active_config_path {
            env.insert(
                "config".into(),
                Value::str(template_path::absolute_path(&[c])),
            );
        }
        env.insert("root".into(), Value::str(template_path::get_working_dir()));
        Value::from_map(env)
    }

    fn set_env_vars(&self, env: &Value) {
        // SAFETY: called before any threads that read the environment are spawned
        for (suffix, value) in [
            ("VERSION", Value::str(UPSTREAM_VERSION)),
            ("ROOT", env.get("root")),
            ("SOURCE", env.get("source")),
            ("RUN_MODE", env.get("runMode")),
        ] {
            let v = value.to_output_string();
            unsafe {
                std::env::set_var(format!("TWELVETY_{suffix}"), &v);
                std::env::set_var(format!("BUILDAWESOME_{suffix}"), &v);
            }
        }
    }

    /// Initialize configuration and create a fresh writer for a build
    fn prepare(&mut self) -> Result<TemplateWriter> {
        let dirs = self.create_directories()?;
        let mut tc = TemplateConfig::new(self.options.config_path.as_deref());
        tc.set_logger(self.logger.clone());
        tc.set_run_mode(&self.run_mode);
        tc.set_directories(dirs);
        let mut formats = ProjectTemplateFormats::new();
        formats.set_via_command_line(self.options.formats.as_deref());
        tc.set_template_formats(formats);
        if let Some(p) = &self.options.path_prefix {
            tc.set_path_prefix(p);
        }
        if let Some(v) = self.verbose_override {
            tc.user_config.set_quiet_mode_override(!v);
        }
        tc.programmatic_config = self.options.config.clone();
        self.active_config_path = self
            .options
            .config_path
            .clone()
            .filter(|p| !p.is_empty())
            .or_else(|| tc.get_local_project_config_file());

        let env = self.environment_variables();
        self.set_env_vars(&env);
        tc.init()?;
        tc.directories.check_case(&tc.exists_cache)?;
        tc.directories.check_input_relative(&tc.exists_cache)?;
        let config = tc.get_config();
        self.logger
            .set_verbose(self.verbose_override.unwrap_or(!config.quiet_mode));
        config
            .events
            .emit("buildawesome.config", &Value::Undefined)?;
        config.events.emit("buildawesome.env", &env)?;

        let dirs = tc.directories.clone();
        self.directories = Some(dirs.clone());
        self.output_dir = dirs.output();
        self.server_options = config.server_options.clone();

        let (ctx, search) =
            BuildContext::create(&tc, self.logger.clone(), &self.run_mode, env.clone())?;
        let extension_map = ctx.extension_map.clone();

        let files = Files::new(
            config.clone(),
            dirs.clone(),
            extension_map.clone(),
            search.clone(),
        );
        self.watch_targets = files.get_glob_watcher_files();
        self.watch_targets
            .extend(config.additional_watch_targets.iter().cloned());
        if let Some(c) = &self.active_config_path {
            self.watch_targets.push(c.clone());
        }
        self.watch_ignores = files.get_glob_watcher_ignores();
        let mut passthrough = TemplatePassthroughManager::new(
            config.clone(),
            dirs.clone(),
            search,
            tc.exists_cache.clone(),
            self.logger.clone(),
        );
        passthrough.is_dry_run = self.is_dry_run;
        self.watch_targets
            .extend(passthrough.get_config_path_globs());
        let mut writer = TemplateWriter::new(ctx, files, Arc::new(passthrough));
        writer.is_dry_run = self.is_dry_run;
        writer.is_run_initial_build = self.is_run_initial_build;
        Ok(writer)
    }

    /// `initializeConfig`: applies configuration (sets `directories`) without building
    pub fn initialize_config(&mut self) -> Result<()> {
        self.prepare().map(|writer| writer.dispose())
    }

    pub fn is_verbose(&self) -> bool {
        self.logger.is_verbose()
    }

    fn events_arg(&self, output_mode: &str) -> Value {
        let mut arg = Map::new();
        if let Some(d) = &self.directories {
            arg.insert("directories".into(), d.get_userspace_instance());
            arg.insert("inputDir".into(), Value::str(d.input()));
        }
        arg.insert("runMode".into(), Value::str(&self.run_mode));
        arg.insert("outputMode".into(), Value::str(output_mode));
        arg.insert("dryRun".into(), Value::Bool(self.is_dry_run));
        arg.insert("incremental".into(), Value::Bool(false));
        if let Some(id) = &self.build_id {
            arg.insert("buildId".into(), Value::str(id));
        }
        Value::from_map(arg)
    }

    /// `executeBuild`
    pub fn execute_build(&mut self, to: BuildOutput) -> Result<(Vec<CopyResult>, Vec<PageResult>)> {
        self.start = Instant::now();
        let mut writer = self.prepare()?;
        let config = writer_config(&writer);
        let output_mode = match to {
            BuildOutput::Json => "json",
            _ => "fs",
        };
        self.build_id = Some(random_uuid());
        let mut events_arg = self.events_arg(output_mode);
        let result = config
            .events
            .emit("buildawesome.before", &events_arg)
            .and_then(|_| match to {
                BuildOutput::Fs => writer.write().map(|r| (r.passthrough_copy, r.templates)),
                BuildOutput::FsTemplates => writer.write_templates().map(|t| (vec![], t)),
                BuildOutput::Json => writer.get_json().map(|t| (vec![], t)),
            })
            .and_then(|(copies, templates)| {
                events_arg.set(
                    "results",
                    Value::array(templates.iter().map(|t| t.to_value()).collect()),
                );
                // A throwing `after` listener fails the build
                config.events.emit("buildawesome.after", &events_arg)?;
                Ok((copies, templates))
            });
        self.last_metadata = Some(writer.get_metadata());
        let outcome = match result {
            Ok(r) => {
                self.build_count += 1;
                Ok(r)
            }
            Err(e) => {
                let mut arg = events_arg.clone();
                arg.set("error", error_to_value(&e));
                if let Err(listener_error) = config.events.emit("buildawesome.aftererror", &arg) {
                    self.logger.warn(&format!(
                        "Problem in `buildawesome.aftererror` event listener: {listener_error:#}"
                    ));
                }
                // Logged as “Problem writing Twelvety templates” by the CLI
                Err(e)
            }
        };
        writer.dispose();
        self.is_run_initial_build = true;
        if output_mode == "fs" {
            let message = self.log_finished();
            self.logger.log_with_options(
                &message,
                Some(if outcome.is_err() { "red" } else { "green" }),
                true,
            );
        }
        outcome
    }

    /// Writes templates (and passthrough copy) to the file system
    pub fn write(&mut self) -> Result<(Vec<CopyResult>, Vec<PageResult>)> {
        self.execute_build(BuildOutput::Fs)
    }

    /// Renders templates to JSON-like results (no files written)
    pub fn to_json(&mut self) -> Result<Vec<PageResult>> {
        self.execute_build(BuildOutput::Json).map(|(_, t)| t)
    }

    pub fn to_json_value(&mut self) -> Result<Value> {
        Ok(Value::array(
            self.to_json()?.iter().map(|r| r.to_value()).collect(),
        ))
    }

    pub fn log_finished(&self) -> String {
        let meta = self.last_metadata.clone().unwrap_or_default();
        let mut slash = vec![];
        if meta.copy_count > 0 {
            slash.push(format!(
                "Copied {}",
                chalk::bold(&meta.copy_count.to_string())
            ));
        }
        slash.push(format!(
            "Wrote {} {}{}",
            chalk::bold(&meta.write_count.to_string()),
            if meta.write_count == 1 {
                "file"
            } else {
                "files"
            },
            if meta.skip_count > 0 {
                format!(" (skipped {})", meta.skip_count)
            } else {
                String::new()
            }
        ));
        let mut ret = vec![slash.join(" ")];
        let time = self.start.elapsed().as_secs_f64();
        let time_str = format!("{time:.2}");
        ret.push(format!(
            "in {} {}",
            chalk::bold(&time_str),
            if time_str == "1.00" {
                "second"
            } else {
                "seconds"
            }
        ));
        let cfg = match &self.active_config_path {
            Some(p) => format!(", {}", template_path::strip_leading_dot_slash(p)),
            None => " no config file".to_string(),
        };
        if time >= 1.0 && meta.write_count > 1 {
            ret.push(chalk::gray(&format!(
                "({:.1}ms each, v{UPSTREAM_VERSION}{cfg})",
                time * 1000.0 / meta.write_count as f64
            )));
        } else {
            ret.push(chalk::gray(&format!("(v{UPSTREAM_VERSION}{cfg})")));
        }
        ret.join(" ")
    }
}

/// A random (version 4) UUID
fn random_uuid() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let random_u64 = || {
        let mut h = RandomState::new().build_hasher();
        h.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
        );
        h.finish()
    };
    let mut bytes = [random_u64().to_be_bytes(), random_u64().to_be_bytes()].concat();
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}

/// The `error` passed to `buildawesome.aftererror` listeners (like a JavaScript `Error`)
pub fn error_to_value(e: &anyhow::Error) -> Value {
    let mut error = Map::new();
    error.insert("message".into(), Value::str(e.to_string()));
    error.insert("name".into(), Value::str("Error"));
    error.insert("stack".into(), Value::str(format!("{e:#}")));
    if let Some(file_path) = crate::template_writer::error_file_path(e) {
        error.insert("filePath".into(), Value::str(file_path));
    }
    Value::from_map(error)
}

fn writer_config(writer: &TemplateWriter) -> Arc<crate::template_config::Config> {
    writer.config()
}

impl TemplateWriter {
    pub fn config(&self) -> Arc<crate::template_config::Config> {
        self.ctx_config()
    }
}

/// Convenience: build a site programmatically
pub fn build(input: &str, output: &str, options: Options) -> Result<Vec<PageResult>> {
    let mut e = Twelvety::new(Some(input), Some(output), options);
    let (_, results) = e.write().context("Build failed")?;
    Ok(results)
}
