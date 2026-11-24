//! Port of upstream `test/cmdTest.js` (runs the `twelvety` binary).

use std::process::{Command, Output};

use twelvety::Value;

fn run(args: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_twelvety"))
        .args(args.split_whitespace())
        .env_remove("DEBUG")
        .output()
        .unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

fn read_events(path: &str) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| Value::from_json(serde_json::from_str(l).unwrap()))
        .collect()
}

fn types(events: &[Value]) -> Vec<String> {
    events.iter().map(|e| e.get("type").to_output_string()).collect()
}

fn find<'a>(events: &'a [Value], kind: &str) -> &'a Value {
    events.iter().find(|e| e.get("type").as_str() == Some(kind)).unwrap_or_else(|| panic!("no {kind} event"))
}

/// Removes a directory when the test ends
struct Cleanup(&'static str);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.0);
        // The shared `_site` parent is removed once empty
        if let Some(parent) = std::path::Path::new(self.0).parent() {
            let _ = std::fs::remove_dir(parent);
        }
    }
}

fn strip_ansi(s: &str) -> String {
    regex::Regex::new("\x1b\\[[0-9;]*m").unwrap().replace_all(s, "").to_string()
}

/// "Test command line exit code success"
#[test]
fn exit_code_success() {
    assert!(run("--input=test/stubs/exitCode_success --dryrun").status.success());
}

/// "Test command line exit code for template error"
#[test]
fn exit_code_for_template_error() {
    assert_eq!(run("--input=test/stubs/exitCode --dryrun").status.code(), Some(1));
}

// "Test command line exit code for global data error": the fixture is a JavaScript data file,
// which the port skips (skipped).

/// "Test data should not process in a --help"
#[test]
fn data_should_not_process_in_help() {
    let o = run("--input=test/stubs/cmd-help-processing --help");
    assert!(o.status.success());
    assert!(!stdout(&o).contains("THIS SHOULD NOT LOG TO CONSOLE"));
}

/// "Test data should not process in a --version"
#[test]
fn data_should_not_process_in_version() {
    let o = run("--input=test/stubs/cmd-help-processing --version");
    assert!(o.status.success());
    assert!(!stdout(&o).contains("THIS SHOULD NOT LOG TO CONSOLE"));
}

/// "--version prints the package name before the version"
#[test]
fn version_prints_the_package_name_before_the_version() {
    let o = run("--version");
    assert!(o.status.success());
    assert!(regex::Regex::new(r"^eleventy \d+\.\d+\.\d+\S*\n$").unwrap().is_match(&stdout(&o)), "{}", stdout(&o));
}

/// "Test command line --events-file writes build.end on success"
#[test]
fn events_file_writes_build_end_on_success() {
    let _cleanup = Cleanup("test/stubs/events-file-ok/_site/success");
    // Parent dir doesn’t exist yet
    let events_file = "test/stubs/events-file-ok/_site/success/logs/.events.ndjson";
    let o = run(&format!(
        "--input=test/stubs/events-file-ok --output=test/stubs/events-file-ok/_site/success --formats=md --events-file={events_file}"
    ));
    assert!(o.status.success(), "{}", stderr(&o));
    let events = read_events(events_file);
    let start = find(&events, "build.start");
    let end = find(&events, "build.end");
    assert_eq!(start.get("v"), Value::Number(1.0));
    let uuid = regex::Regex::new(r"^[\da-f]{8}(-[\da-f]{4}){3}-[\da-f]{12}$").unwrap();
    assert!(uuid.is_match(&start.get("id").to_output_string()), "{:?}", start.get("id"));
    assert_eq!(end.get("id"), start.get("id"));
    assert_eq!(end.get("ok"), Value::Bool(true));
    let duration = end.get("durationMs").as_f64().unwrap();
    assert_eq!(duration.fract(), 0.0);
    let pages = end.get("pages").to_vec();
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0].get("url"), Value::str("/"));
    assert!(pages[0].get("inputPath").as_str().is_some());
    assert!(pages[0].get("content").is_undefined());
    assert!(strip_ansi(&stdout(&o)).contains("Wrote 1 file"), "{}", stdout(&o));
    assert!(!stdout(&o).contains(r#"{"v":1"#));
}

// "Test command line --events-file writes a passthrough event" and "… writes build.error instead
// of build.end when an after listener throws": need JavaScript configuration files (skipped).

/// "Test command line --events-file writes an empty passthrough map without passthrough copy"
#[test]
fn events_file_writes_an_empty_passthrough_map_without_passthrough_copy() {
    let _cleanup = Cleanup("test/stubs/events-file-ok/_site/no-passthrough");
    let events_file = "test/stubs/events-file-ok/_site/no-passthrough/.events.ndjson";
    let o = run(&format!("--input=test/stubs/events-file-ok --formats=md --events-file={events_file} --dryrun"));
    assert!(o.status.success(), "{}", stderr(&o));
    let events = read_events(events_file);
    let passthrough: Vec<&Value> = events.iter().filter(|e| e.get("type").as_str() == Some("passthrough")).collect();
    assert_eq!(passthrough.len(), 1);
    assert_eq!(passthrough[0].get("map"), Value::object());
}

/// "Test command line --events-file works with --to=json"
#[test]
fn events_file_works_with_to_json() {
    let _cleanup = Cleanup("test/stubs/events-file-ok/_site/json");
    let events_file = "test/stubs/events-file-ok/_site/json/.events.ndjson";
    let o = run(&format!("--input=test/stubs/events-file-ok --formats=md --to=json --events-file={events_file}"));
    assert!(o.status.success(), "{}", stderr(&o));
    let json: serde_json::Value = serde_json::from_str(&stdout(&o)).unwrap();
    assert_eq!(json[0]["url"], "/");
    let events = read_events(events_file);
    assert_eq!(find(&events, "build.end").get("pages").to_vec().len(), 1);
}

/// "Test command line --events-file requires a path"
#[test]
fn events_file_requires_a_path() {
    let o = run("--input=test/stubs/events-file-ok --formats=md --events-file --dryrun");
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).contains("--events-file requires a single file path"));
}

/// "Test command line --events-file rejects repeated flags"
#[test]
fn events_file_rejects_repeated_flags() {
    let _cleanup = Cleanup("test/stubs/events-file-ok/_site/repeated");
    let o = run(
        "--input=test/stubs/events-file-ok --formats=md --events-file=test/stubs/events-file-ok/_site/repeated/a/.events.ndjson --events-file=test/stubs/events-file-ok/_site/repeated/b/.events.ndjson --dryrun",
    );
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).contains("--events-file requires a single file path"));
}

/// "Test command line --events-file writes build.error for a template error"
#[test]
fn events_file_writes_build_error_for_a_template_error() {
    let _cleanup = Cleanup("test/stubs/events-file-broken/_site");
    let events_file = "test/stubs/events-file-broken/_site/template-error/.events.ndjson";
    let o = run(&format!("--input=test/stubs/events-file-broken --formats=njk --events-file={events_file} --dryrun"));
    assert_eq!(o.status.code(), Some(1));
    let events = read_events(events_file);
    let build_error = find(&events, "build.error");
    assert_eq!(build_error.get("ok"), Value::Bool(false));
    assert!(build_error.get("error").get("message").as_str().is_some());
    assert_eq!(
        build_error.get("error").get("filePath"),
        Value::str("./test/stubs/events-file-broken/index.njk")
    );
    assert_eq!(build_error.get("id"), events[0].get("id"));
}

/// "Test command line --events-file writes build.error for a data file error"
#[test]
fn events_file_writes_build_error_for_a_data_file_error() {
    let _cleanup = Cleanup("test/stubs/events-file-broken-data/_site");
    let events_file = "test/stubs/events-file-broken-data/_site/data-file-error/.events.ndjson";
    let o = run(&format!("--input=test/stubs/events-file-broken-data --formats=md --events-file={events_file} --dryrun"));
    assert_eq!(o.status.code(), Some(1));
    let events = read_events(events_file);
    let build_error = find(&events, "build.error");
    assert_eq!(
        build_error.get("error").get("filePath"),
        Value::str("./test/stubs/events-file-broken-data/_data/bad.json")
    );
    assert_eq!(build_error.get("id"), events[0].get("id"));
}

/// "Test command line --events-file writes build.error with a null id for a configuration error"
/// (the port can’t load `broken-config.cjs`, which is a configuration error too)
#[test]
fn events_file_writes_build_error_with_a_null_id_for_a_configuration_error() {
    let _cleanup = Cleanup("test/stubs/events-file-ok/_site/config-error");
    let events_file = "test/stubs/events-file-ok/_site/config-error/.events.ndjson";
    let o = run(&format!(
        "--input=test/stubs/events-file-ok --formats=md --config=test/stubs/broken-config.cjs --events-file={events_file} --dryrun"
    ));
    assert_eq!(o.status.code(), Some(1));
    let events = read_events(events_file);
    assert_eq!(types(&events), ["build.error"]);
    assert_eq!(events[0].get("id"), Value::Null);
    assert_eq!(events[0].get("durationMs"), Value::Null);
}
