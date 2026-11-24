//! Port of `Util/Git.js`: file timestamps from git history.

use std::process::Command;

fn git_timestamps(path: &str, extra: &[&str]) -> Vec<i64> {
    let mut args = vec!["log", "--format=%at"];
    args.extend_from_slice(extra);
    args.push("--");
    args.push(path);
    let Ok(output) = Command::new("git").args(&args).output() else {
        return vec![];
    };
    if !output.status.success() {
        return vec![];
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|l| l.trim().parse::<i64>().ok())
        .collect()
}

/// Milliseconds timestamp of the most recent commit touching `path`
pub fn get_updated_timestamp(path: &str) -> Option<i64> {
    git_timestamps(path, &["-1"]).first().map(|s| s * 1000)
}

/// Milliseconds timestamp of the commit that added `path`
pub fn get_created_timestamp(path: &str) -> Option<i64> {
    git_timestamps(path, &["--follow", "--diff-filter=A"])
        .last()
        .map(|s| s * 1000)
}
