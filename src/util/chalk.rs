//! Minimal terminal color helpers (stands in for the `chalk` package).

use std::io::IsTerminal;

pub fn supports_color() -> bool {
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    if std::env::var_os("FORCE_COLOR").is_some() {
        return true;
    }
    std::io::stdout().is_terminal()
}

pub fn color(name: &str, s: &str) -> String {
    let code = match name {
        "red" => "31",
        "green" => "32",
        "yellow" => "33",
        "blue" => "34",
        "magenta" => "35",
        "cyan" => "36",
        "gray" | "grey" => "90",
        "bold" => "1",
        _ => return s.to_string(),
    };
    let close = if name == "bold" { "22" } else { "39" };
    format!("\x1b[{code}m{s}\x1b[{close}m")
}

pub fn gray(s: &str) -> String {
    if supports_color() {
        color("gray", s)
    } else {
        s.to_string()
    }
}

pub fn bold(s: &str) -> String {
    if supports_color() {
        color("bold", s)
    } else {
        s.to_string()
    }
}
