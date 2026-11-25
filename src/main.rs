//! The `twelvety` command line (port of `cmd.cjs` / `cli.cjs`).

use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use twelvety::core::{Twelvety, Options};
use twelvety::errors::format_error_chain;
use twelvety::user_config::UserConfig;
use twelvety::util::template_path;
use twelvety::value::{Map, Value};

const HELP: &str = "Usage: twelvety
       twelvety --input=. --output=./_site
       twelvety --serve

Arguments:

     --version

     --input=.
       Input template files (default: `.`)

     --output=_site
       Write HTML output to this folder (default: `_site`)

     --serve
       Run web server on --port (default 8080) and watch them too

     --port
       Run the --serve web server on this port (default 8080)

     --watch
       Wait for files to change and automatically rewrite (no web server)

     --ignore-initial
       Start without a build; build when files change. Works best with watch/serve.

     --formats=liquid,md
       Allow only certain template types (default: `*`)

     --quiet
       Don’t print all written files (off by default)

     --config=filename.json
       Override the configuration file (default: `buildawesome.config.json`)

     --pathprefix='/'
       Change all url template filters to use this subdirectory.

     --dryrun
       Don’t write any files. Useful in DEBUG mode, for example: `DEBUG=Twelvety* twelvety --dryrun`

     --to=json
       Change the output to JSON (default: `fs`)

     --events-file=.events.ndjson
       Write build events as newline-delimited JSON to this file

     --help";

const STRING_ARGS: &[&str] = &[
    "input", "output", "formats", "config", "pathprefix", "port", "to", "incremental", "loader",
    "events-file",
];
const BOOLEAN_ARGS: &[&str] = &[
    "quiet", "version", "watch", "dryrun", "help", "serve", "ignore-initial",
];

struct Args {
    strings: HashMap<String, String>,
    booleans: HashMap<String, bool>,
    /// String arguments passed more than once
    repeated: Vec<String>,
}

fn parse_args(argv: &[String]) -> Result<Args, String> {
    let mut strings: HashMap<String, String> = HashMap::new();
    let mut booleans = HashMap::new();
    let mut repeated = vec![];
    let mut i = 0;
    while i < argv.len() {
        let arg = &argv[i];
        let Some(rest) = arg.strip_prefix("--") else {
            return Err(format!(
                "We don’t know what '{arg}' is. Use --help to see the list of supported commands."
            ));
        };
        let (name, value) = match rest.split_once('=') {
            Some((n, v)) => (n.to_string(), Some(v.to_string())),
            None => (rest.to_string(), None),
        };
        if let Some(negated) = name.strip_prefix("no-") {
            if BOOLEAN_ARGS.contains(&negated) {
                booleans.insert(negated.to_string(), false);
                i += 1;
                continue;
            }
        }
        if STRING_ARGS.contains(&name.as_str()) {
            let value = match value {
                Some(v) => v,
                None => {
                    if i + 1 < argv.len() && !argv[i + 1].starts_with("--") {
                        i += 1;
                        argv[i].clone()
                    } else {
                        String::new()
                    }
                }
            };
            if strings.insert(name.clone(), value).is_some() {
                repeated.push(name);
            }
        } else if BOOLEAN_ARGS.contains(&name.as_str()) {
            let b = match value.as_deref() {
                None | Some("true") | Some("") => true,
                Some("false") => false,
                Some(_) => true,
            };
            booleans.insert(name, b);
        } else {
            return Err(format!(
                "We don’t know what '{arg}' is. Use --help to see the list of supported commands."
            ));
        }
        i += 1;
    }
    Ok(Args { strings, booleans, repeated })
}

/// Bump on a breaking change
const EVENTS_SCHEMA_VERSION: u32 = 1;

#[derive(Default)]
struct ReporterState {
    started_at: Option<Instant>,
    build_id: Option<String>,
    /// The current build’s error was already written
    error_reported: bool,
}

/// Build events as newline-delimited JSON (`--events-file`, port of `NdjsonReporter.js`)
#[derive(Clone)]
struct NdjsonReporter {
    file: Arc<Mutex<File>>,
    file_path: String,
    state: Arc<Mutex<ReporterState>>,
}

fn now_ms() -> Value {
    Value::Number(SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as f64)
}

fn event(kind: &str, id: Option<String>) -> Map {
    let mut m = Map::new();
    m.insert("v".into(), Value::Number(EVENTS_SCHEMA_VERSION as f64));
    m.insert("type".into(), Value::str(kind));
    m.insert("id".into(), id.map(Value::from).unwrap_or(Value::Null));
    m.insert("ts".into(), now_ms());
    m
}

impl NdjsonReporter {
    fn new(file_path: &str) -> std::io::Result<Self> {
        if let Some(parent) = std::path::Path::new(file_path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        Ok(NdjsonReporter {
            file: Arc::new(Mutex::new(File::create(file_path)?)),
            file_path: file_path.to_string(),
            state: Arc::new(Mutex::new(ReporterState::default())),
        })
    }

    fn emit(&self, event: Map) {
        let line = format!("{}\n", Value::from_map(event).to_json_string());
        // Event file failures must not fail the build
        if let Err(e) = self.file.lock().unwrap().write_all(line.as_bytes()) {
            eprintln!("[12ty] Could not write to --events-file: {e}");
        }
    }

    fn duration(state: &ReporterState) -> Value {
        state
            .started_at
            .map(|s| Value::Number(s.elapsed().as_millis() as f64))
            .unwrap_or(Value::Null)
    }

    /// `build.error`: `id` and `durationMs` are null for errors outside of a build
    fn error(&self, error: &Value) {
        let mut state = self.state.lock().unwrap();
        if state.error_reported {
            return;
        }
        state.error_reported = true;
        let mut e = event("build.error", state.build_id.take());
        e.insert("ok".into(), Value::Bool(false));
        e.insert("durationMs".into(), Self::duration(&state));
        let mut details = Map::new();
        for key in ["message", "name", "filePath"] {
            details.insert(key.into(), error.get(key));
        }
        e.insert("error".into(), Value::from_map(details));
        state.started_at = None;
        drop(state);
        self.emit(e);
    }

    fn config(&self, config: &mut UserConfig) {
        // Writing the file must not trigger a rebuild
        let relative = template_path::add_leading_dot_slash(&template_path::relative_path(
            &template_path::absolute_path(&[&self.file_path]),
        ));
        let escaped: String = relative
            .chars()
            .flat_map(|c| {
                let escape = matches!(c, '\\' | '*' | '?' | '[' | ']' | '{' | '}' | '(' | ')' | '!' | '+' | '@' | '|');
                escape.then_some('\\').into_iter().chain(std::iter::once(c))
            })
            .collect();
        config.watch_ignores.insert(escaped);

        let reporter = self.clone();
        let events = config.events.clone();
        let after_registered = Arc::new(Mutex::new(false));
        config.on("buildawesome.before", move |arg: &Value| {
            let id = arg.get("buildId").as_str().map(String::from);
            {
                let mut state = reporter.state.lock().unwrap();
                state.started_at = Some(Instant::now());
                state.build_id = id.clone();
                state.error_reported = false;
            }
            let mut e = event("build.start", id);
            e.insert("runMode".into(), arg.get("runMode"));
            e.insert("incremental".into(), Value::Bool(arg.get("incremental").is_truthy()));
            reporter.emit(e);

            // Registered last so a throwing user `after` listener skips `build.end`
            let mut registered = after_registered.lock().unwrap();
            if !*registered {
                *registered = true;
                let reporter = reporter.clone();
                events.on(
                    "buildawesome.after",
                    Arc::new(move |arg: &Value| {
                        let mut state = reporter.state.lock().unwrap();
                        let mut e = event("build.end", state.build_id.take());
                        e.insert("ok".into(), Value::Bool(true));
                        e.insert("durationMs".into(), Self::duration(&state));
                        state.started_at = None;
                        drop(state);
                        let pages = arg
                            .get("results")
                            .to_vec()
                            .iter()
                            .map(|r| {
                                let mut page = Map::new();
                                for key in ["inputPath", "outputPath", "url"] {
                                    page.insert(key.into(), r.get(key));
                                }
                                Value::from_map(page)
                            })
                            .collect();
                        e.insert("pages".into(), Value::array(pages));
                        reporter.emit(e);
                        Ok(())
                    }),
                );
            }
            Ok(())
        });

        let reporter = self.clone();
        config.on("buildawesome.passthrough", move |arg: &Value| {
            let id = reporter.state.lock().unwrap().build_id.clone();
            let mut e = event("passthrough", id);
            // The dev server doesn’t emulate passthrough copy in the port
            e.insert("emulated".into(), Value::Bool(false));
            let map = arg.get("map");
            let copied = map.keys().into_iter().map(Value::from).collect();
            e.insert("map".into(), map);
            e.insert("copied".into(), Value::array(copied));
            reporter.emit(e);
            Ok(())
        });

        let reporter = self.clone();
        config.on("buildawesome.aftererror", move |arg: &Value| {
            reporter.error(&arg.get("error"));
            Ok(())
        });
    }
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse_args(&argv) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("[12ty] {e}");
            return ExitCode::from(1);
        }
    };
    let flag = |name: &str| args.booleans.get(name).copied().unwrap_or(false);
    if flag("version") {
        println!("twelvety {}", Twelvety::get_version());
        return ExitCode::SUCCESS;
    }
    if flag("help") {
        println!("{HELP}");
        return ExitCode::SUCCESS;
    }
    let to = args.strings.get("to").cloned().unwrap_or_else(|| "fs".into());
    let is_serve = flag("serve");
    let is_watch = flag("watch");
    if to == "json" && (is_serve || is_watch) {
        eprintln!("[12ty] --to=json is not compatible with --serve or --watch.");
        return ExitCode::from(1);
    }
    if to != "fs" && to != "json" {
        eprintln!("[12ty] Invalid --to value: {to}. Supported values: `fs` (default), `json`.");
        return ExitCode::from(1);
    }
    let run_mode = if is_serve {
        "serve"
    } else if is_watch {
        "watch"
    } else {
        "build"
    };
    if args.repeated.iter().any(|a| a == "events-file")
        || args.strings.get("events-file").is_some_and(|f| f.is_empty())
    {
        eprintln!("[12ty] --events-file requires a single file path, e.g. --events-file=.events.ndjson.");
        return ExitCode::from(1);
    }
    let reporter = match args.strings.get("events-file").map(|f| NdjsonReporter::new(f)) {
        Some(Ok(r)) => Some(r),
        Some(Err(e)) => {
            eprintln!("[12ty] Could not open --events-file: {e}");
            return ExitCode::from(1);
        }
        None => None,
    };
    let mut options = Options {
        source: Some("cli".into()),
        run_mode: Some(run_mode.into()),
        dry_run: flag("dryrun"),
        config_path: args.strings.get("config").cloned(),
        path_prefix: args.strings.get("pathprefix").cloned(),
        quiet_mode: args.booleans.get("quiet").copied(),
        formats: args.strings.get("formats").cloned(),
        ..Default::default()
    };
    if let Some(reporter) = reporter.clone() {
        options = options.with_config(move |config| {
            reporter.config(config);
            Ok(())
        });
    }
    let mut twelvety = Twelvety::new(
        args.strings.get("input").map(|s| s.as_str()),
        args.strings.get("output").map(|s| s.as_str()),
        options,
    );
    if to == "json" {
        twelvety.disable_logger();
    }
    let ignore_initial = flag("ignore-initial");
    if ignore_initial {
        twelvety.set_ignore_initial(true);
    }

    let result = if to == "json" {
        twelvety
            .to_json_value()
            .map(|v| println!("{}", v.to_json_string_pretty(2)))
    } else {
        twelvety.write().map(|_| ())
    };
    if let Err(e) = &result {
        if let Some(reporter) = &reporter {
            reporter.error(&twelvety::core::error_to_value(e));
        }
        eprintln!("[12ty] Problem writing Twelvety templates:\n[12ty] {}", format_error_chain(e));
        if !is_serve && !is_watch {
            return ExitCode::from(1);
        }
    }

    if is_serve || is_watch {
        let server = if is_serve {
            let port: u16 = args
                .strings
                .get("port")
                .and_then(|p| p.parse().ok())
                .or_else(|| {
                    twelvety
                        .server_options
                        .get("port")
                        .as_f64()
                        .map(|p| p as u16)
                })
                .unwrap_or(8080);
            let prefix = args.strings.get("pathprefix").cloned().unwrap_or_else(|| "/".into());
            let server = twelvety::serve::DevServer::new(port, &twelvety.output_dir, &prefix);
            if let Err(e) = server.start() {
                eprintln!("[12ty] {e}");
                return ExitCode::from(1);
            }
            twelvety
                .logger()
                .force_log(&format!("Server at http://localhost:{port}{prefix}"));
            Some(server)
        } else {
            None
        };
        if let Err(e) = twelvety::watch::watch(&mut twelvety, server.as_ref()) {
            eprintln!("[12ty] {}", format_error_chain(&e));
            return ExitCode::from(1);
        }
    }
    ExitCode::SUCCESS
}
